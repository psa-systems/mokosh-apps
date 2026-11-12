//! The RFC 8252 loopback redirect listener (MAPPS-505).
//!
//! A desktop window has no origin, so the OP has no URL to redirect to.
//! RFC 8252 section 7.3 is the native-app answer: the app binds an
//! ephemeral port on `127.0.0.1`, hands the OP
//! `http://127.0.0.1:<port>/auth/callback` as the `redirect_uri`, and
//! reads the authorization response out of the single request the
//! system browser then makes to it.
//!
//! Native only. The browser build has a document to redirect and never
//! listens on a socket.

use std::net::{Ipv4Addr, TcpListener as StdTcpListener};
use std::sync::atomic::{AtomicU16, Ordering};
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};

/// Port of the listener bound most recently, or 0 before the first
/// flow. Process-global because [`crate::modules::oidc::OidcConfig::resolve_redirect_uri`]
/// has to answer with the same URI at the authorize request and again at
/// the token exchange, and by exchange time the listener has served its
/// request and been dropped. A new flow overwrites it; nothing clears it,
/// because the exchange outlives the socket.
static BOUND_PORT: AtomicU16 = AtomicU16::new(0);

/// Path the OP is asked to redirect to. The same path the browser build
/// uses, so both halves of the flow read alike.
// SPEC-3.39.5
const CALLBACK_PATH: &str = "/auth/callback";

/// Cap on the request line we will buffer. An authorization response is a
/// few hundred bytes; anything past this is not one, and reading it
/// forever would be a way to hold the app open.
// SPEC-3.40.9
const MAX_REQUEST_LINE: usize = 8 * 1024;

/// A bound, not-yet-serving loopback listener.
///
/// Binding is split from serving because the port is half of the
/// `redirect_uri` the authorize request carries: it has to be known
/// before the browser opens, and served after.
pub struct Listener {
    inner: StdTcpListener,
    port: u16,
}

/// Bind an ephemeral port on `127.0.0.1`.
///
/// Loopback only: RFC 8252 section 8.3 requires the listener to be
/// unreachable from the network, and `127.0.0.1` (not `localhost`) is
/// what section 7.3 asks the redirect URI to name, because the name can
/// resolve elsewhere.
// SPEC-3.39.1: IPv4 loopback literal, ephemeral port; see also SPEC-3.39.2 to 3.42.4
// SPEC-3.39.4: each failed step returns its own prefixed Err and leaves the recorded port untouched.
pub fn bind() -> Result<Listener, String> {
    let inner = StdTcpListener::bind((Ipv4Addr::LOCALHOST, 0))
        .map_err(|e| format!("could not bind a loopback listener on 127.0.0.1: {e}"))?;
    let port = inner
        .local_addr()
        .map_err(|e| format!("could not read the loopback listener's port: {e}"))?
        .port();
    // `tokio::net::TcpListener::from_std` requires it, and a blocking
    // accept inside the async task would stall the whole app.
    inner
        .set_nonblocking(true)
        .map_err(|e| format!("could not put the loopback listener in non-blocking mode: {e}"))?;
    // SPEC-3.39.3: recorded on every successful bind, overwritten each time, never cleared.
    BOUND_PORT.store(port, Ordering::SeqCst);
    Ok(Listener { inner, port })
}

/// Redirect URI of the listener bound most recently, or `None` when this
/// process has not started a sign-in flow yet.
// SPEC-3.39.5
pub fn redirect_uri() -> Option<String> {
    match BOUND_PORT.load(Ordering::SeqCst) {
        0 => None,
        port => Some(loopback_uri(port)),
    }
}

fn loopback_uri(port: u16) -> String {
    format!("http://127.0.0.1:{port}{CALLBACK_PATH}")
}

impl Listener {
    // SPEC-3.39.6
    pub fn port(&self) -> u16 {
        self.port
    }

    /// What the OP must redirect to for this flow.
    pub fn redirect_uri(&self) -> String {
        loopback_uri(self.port)
    }

    /// Serve exactly one request and hand back its query string, leading
    /// `?` included, in the shape `complete_login` reads on the web.
    ///
    /// Consumes the listener, so the port is released as soon as this
    /// returns however it returns: one served redirect, a browser that
    /// connected and said nothing, or `timeout` elapsing because the user
    /// abandoned the flow in the browser and never came back.
    // SPEC-3.40.1: SPEC-3.40.2 to 3.43.7, 3.43.12
    // SPEC-3.40.12: the timeout below bounds only the accept; the request-line read has none.
    pub async fn serve_one(self, timeout: Duration) -> Result<String, String> {
        // `from_std` registers with the current runtime's IO driver and
        // panics when there is none. Check first, so a missing runtime is
        // an error the caller reports rather than a panic in a detached task.
        // SPEC-3.40.3: no current runtime is an Err, not a panic.
        tokio::runtime::Handle::try_current()
            .map_err(|e| format!("no async runtime to serve the loopback redirect on: {e}"))?;
        // SPEC-3.40.4: conversion failure is an Err naming the port.
        let listener = tokio::net::TcpListener::from_std(self.inner)
            .map_err(|e| format!("could not listen on 127.0.0.1:{}: {e}", self.port))?;

        // SPEC-3.40.5: timeout elapsed -> Err (port released on drop); SPEC-3.40.6: accept failure -> Err.
        // MAPPS-1237: one deadline for the accept AND the request read. The
        // timeout used to wrap only `accept`, so a browser that connected and
        // sent nothing (or half a line) held the read, and the port, forever.
        let deadline = tokio::time::Instant::now() + timeout;
        let (mut stream, _peer) = tokio::time::timeout_at(deadline, listener.accept())
            .await
            .map_err(|_| {
                format!(
                    "no redirect arrived on 127.0.0.1:{} within {} seconds",
                    self.port,
                    timeout.as_secs()
                )
            })?
            .map_err(|e| format!("the loopback connection failed: {e}"))?;

        // SPEC-3.41.7: any error here returns before `respond`, so nothing is written.
        // SPEC-3.40.7: Ok carries the query string with its leading `?`.
        let line = tokio::time::timeout_at(deadline, read_request_line(&mut stream))
            .await
            .map_err(|_| {
                format!(
                    "the browser connected to 127.0.0.1:{} but sent no complete request within {} seconds",
                    self.port,
                    timeout.as_secs()
                )
            })??;
        let query = request_query(&line)?;
        respond(&mut stream).await?;
        Ok(query)
    }
}

/// Read up to the first CRLF: the request line is all we need, and the
/// browser sends headers we have no use for.
// SPEC-3.40.8: SPEC-3.40.9 to 3.43.11
async fn read_request_line(stream: &mut tokio::net::TcpStream) -> Result<String, String> {
    let mut buf: Vec<u8> = Vec::with_capacity(512);
    let mut chunk = [0u8; 512];
    loop {
        if let Some(end) = buf.windows(2).position(|w| w == b"\r\n") {
            return String::from_utf8(buf[..end].to_vec())
                .map_err(|e| format!("the loopback request line was not UTF-8: {e}"));
        }
        if buf.len() > MAX_REQUEST_LINE {
            return Err(format!(
                "the loopback request line exceeded {MAX_REQUEST_LINE} bytes without ending"
            ));
        }
        // SPEC-3.40.11: read failure and non-UTF-8 line are distinct Errs.
        let read = stream
            .read(&mut chunk)
            .await
            .map_err(|e| format!("could not read the loopback request: {e}"))?;
        // SPEC-3.40.10: EOF before a CRLF is an Err.
        if read == 0 {
            return Err(
                "the browser closed the loopback connection before sending a request".into(),
            );
        }
        buf.extend_from_slice(&chunk[..read]);
    }
}

/// Query string of an HTTP request line, e.g.
/// `GET /auth/callback?code=x&state=y HTTP/1.1` -> `?code=x&state=y`.
///
/// A request with no query is not an authorization response, and is an
/// error rather than an empty string: the caller has to be able to tell
/// "the OP answered" from "something else knocked on the port".
// SPEC-3.41.1: SPEC-3.41.2 to 3.44.4
fn request_query(line: &str) -> Result<String, String> {
    let mut parts = line.split_whitespace();
    let _method = parts
        .next()
        .ok_or_else(|| "the loopback request line was empty".to_string())?;
    let target = parts
        .next()
        .ok_or_else(|| format!("the loopback request line named no target ({line})"))?;
    match target.split_once('?') {
        // SPEC-3.41.3: query returned as-is, no decoding. SPEC-3.41.4: no query is an Err.
        Some((_path, query)) if !query.is_empty() => Ok(format!("?{query}")),
        _ => Err(format!(
            "the loopback request carried no authorization response ({target})"
        )),
    }
}

/// The page the user is left looking at in their browser. RFC 8252
/// section 8.11: tell them the flow is done and to come back to the app.
/// Deliberately unstyled - it is served by a socket, not by the app, so
/// it has no stylesheet to reach for and nothing to match.
// SPEC-3.41.6
const DONE_PAGE: &str = concat!(
    "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\">",
    "<title>Signed in</title></head><body>",
    "<h1>Signed in</h1><p>You can close this window and go back to the app.</p>",
    "</body></html>"
);

// SPEC-3.41.5: SPEC-3.41.8
async fn respond(stream: &mut tokio::net::TcpStream) -> Result<(), String> {
    let response = format!(
        "HTTP/1.1 200 OK\r\n\
         Content-Type: text/html; charset=utf-8\r\n\
         Content-Length: {}\r\n\
         Cache-Control: no-store\r\n\
         Connection: close\r\n\
         \r\n\
         {DONE_PAGE}",
        DONE_PAGE.len()
    );
    stream
        .write_all(response.as_bytes())
        .await
        .map_err(|e| format!("could not answer the loopback request: {e}"))?;
    stream
        .flush()
        .await
        .map_err(|e| format!("could not flush the loopback response: {e}"))?;
    // Suppressed deliberately: the response is written and flushed, so the
    // user has their page and the code is in hand. A half-close the peer
    // already performed is the ordinary reason this fails and is not worth
    // failing a completed sign-in over, but it still gets a line.
    // SPEC-3.41.9
    if let Err(e) = stream.shutdown().await {
        tracing::debug!("loopback connection did not shut down cleanly: {e}");
    }
    Ok(())
}

/// Serialises the tests that bind a listener. [`BOUND_PORT`] is
/// process-wide, so two of them binding at once would each read the
/// other's port back out of [`redirect_uri`].
#[cfg(test)]
pub(crate) fn test_bind_lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpStream;

    fn runtime() -> tokio::runtime::Runtime {
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("a tokio runtime for the loopback listener")
    }

    #[test]
    fn request_query_reads_the_authorization_response() {
        assert_eq!(
            request_query("GET /auth/callback?code=abc&state=xyz HTTP/1.1").unwrap(),
            "?code=abc&state=xyz"
        );
        assert_eq!(
            request_query("GET /auth/callback?error=access_denied HTTP/1.1").unwrap(),
            "?error=access_denied"
        );
    }

    #[test]
    fn request_query_rejects_a_request_that_is_not_a_redirect() {
        for line in [
            "GET /auth/callback HTTP/1.1",
            "GET /favicon.ico HTTP/1.1",
            "GET /auth/callback? HTTP/1.1",
            "GET",
            "",
        ] {
            assert!(request_query(line).is_err(), "line {line:?}");
        }
    }

    #[test]
    fn binds_loopback_and_serves_exactly_one_request() {
        let _guard = test_bind_lock();
        let listener = bind().expect("bind a loopback listener");
        let port = listener.port();
        assert_ne!(port, 0, "the OS should have assigned an ephemeral port");
        assert_eq!(
            listener.redirect_uri(),
            format!("http://127.0.0.1:{port}/auth/callback")
        );
        assert_eq!(
            redirect_uri().as_deref(),
            Some(listener.redirect_uri().as_str())
        );

        let rt = runtime();
        let served = rt.spawn(async move { listener.serve_one(Duration::from_secs(10)).await });

        let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect to the listener");
        stream
            .write_all(b"GET /auth/callback?code=abc&state=xyz HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n")
            .expect("send the redirect");
        let mut response = String::new();
        stream
            .read_to_string(&mut response)
            .expect("read the listener's response");

        let query = rt
            .block_on(served)
            .expect("join")
            .expect("serve the redirect");
        assert_eq!(query, "?code=abc&state=xyz");
        assert!(response.starts_with("HTTP/1.1 200 OK"), "{response}");
        assert!(response.contains("close this window"), "{response}");

        // Exactly one: the listener is consumed, so the port is gone with it.
        assert!(
            TcpStream::connect(("127.0.0.1", port)).is_err(),
            "port {port} is still accepting after the one request"
        );
    }

    /// MAPPS-1237: a connection that never sends its request line is held to
    /// the same timeout as one that never arrives, and the port is released.
    /// The outer `timeout` turns the old behaviour (a read that never returns)
    /// into a failure instead of a hung test.
    #[test]
    fn a_silent_connection_times_out_and_releases_the_port() {
        let _guard = test_bind_lock();
        let listener = bind().expect("bind a loopback listener");
        let port = listener.port();
        let rt = runtime();
        let served = rt.spawn(async move { listener.serve_one(Duration::from_millis(200)).await });

        // Connect, send half a request line, then say nothing more.
        let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect to the listener");
        stream
            .write_all(b"GET /auth/cal")
            .expect("send a partial line");

        let err = rt
            .block_on(async { tokio::time::timeout(Duration::from_secs(5), served).await })
            .expect("serve_one must give up on a silent connection, not wait forever")
            .expect("join")
            .expect_err("a silent connection is not a redirect");
        assert!(err.contains("sent no complete request"), "{err}");
        drop(stream);
        assert!(
            TcpStream::connect(("127.0.0.1", port)).is_err(),
            "port {port} is still bound after the timeout"
        );
    }

    #[test]
    fn an_abandoned_flow_times_out_and_releases_the_port() {
        let _guard = test_bind_lock();
        let listener = bind().expect("bind a loopback listener");
        let port = listener.port();
        let rt = runtime();
        let err = rt
            .block_on(listener.serve_one(Duration::from_millis(50)))
            .expect_err("an unvisited listener should time out");
        assert!(err.contains("no redirect arrived"), "{err}");
        assert!(
            TcpStream::connect(("127.0.0.1", port)).is_err(),
            "port {port} is still bound after the timeout"
        );
    }
}
