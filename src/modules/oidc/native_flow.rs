//! The native half of `start_login`: RFC 8252, OAuth 2.0 for Native
//! Apps (MAPPS-505).
//!
//! A desktop window has no origin, so there is no `redirect_uri` to give
//! the OP and no document to redirect. The native-app flow instead:
//!
//!  1. binds an ephemeral loopback port (`crate::platform::loopback`),
//!     which is what `OidcConfig::resolve_redirect_uri` then answers with,
//!  2. opens `<issuer>/oauth2/authorize` in the user's OWN browser, not
//!     the app's webview: RFC 8252 section 8.12 rules out an embedded
//!     user-agent, because the app can read the credentials typed into it
//!     and the user cannot check the URL bar to see who is asking. Using
//!     the real browser also means an existing OP session applies,
//!  3. captures `code` and `state` from the one request that listener
//!     serves, and
//!  4. routes to `/auth/callback`, which owns the exchange, the retry
//!     policy and the error screen on both targets.
//!
//! Everything security-relevant is therefore unchanged and shared: the
//! PKCE verifier, the `state` comparison, the `PendingFlow` expiry, the
//! `nonce` binding, and `classify_flow_error`.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::Duration;

use dioxus::router::RouterContext;

use super::config::OidcConfig;
use super::flow::FlowError;

/// How long the listener waits for the OP's redirect before giving up
/// and releasing the port. Long, because the user may have to type a
/// password and a second factor in the browser; bounded, because an
/// abandoned sign-in must not hold a socket for the life of the process.
/// Comfortably inside the 10-minute `PendingFlow` TTL, so the flow the
/// redirect would complete is still alive when it arrives.
const REDIRECT_TIMEOUT: Duration = Duration::from_secs(5 * 60);

/// `error` code reported to `/auth/callback` when the loopback half of
/// the flow fails. Not an OP code: the OP never got to answer. It is
/// shaped like one so the failure travels the single path the callback
/// page already classifies and renders (`classify_flow_error` shows it,
/// like every other non-recoverable token-endpoint code).
pub(super) const LOOPBACK_ERROR_CODE: &str = "loopback_failed";

/// Whether a flow is already waiting on its listener. `AuthGuard` calls
/// `start_login` on every render while the user is unauthenticated, and
/// on the desktop that screen stays up for the whole browser round-trip,
/// so without this each render would open another browser window and
/// bind another port.
static IN_PROGRESS: AtomicBool = AtomicBool::new(false);

/// The authorization response the listener captured, in the shape
/// `complete_login` reads on the web: a URL query string, leading `?`
/// included. Process-global rather than thread-local because the task
/// that fills it is polled by a multi-threaded runtime.
static CAPTURED_QUERY: Mutex<Option<String>> = Mutex::new(None);

/// What `/auth/callback` should complete, or `None` when no flow has
/// delivered a response yet.
///
/// A poisoned lock is recovered rather than propagated, as in
/// `platform::store`: the value behind it is one independent string, not
/// an invariant a panic elsewhere can have broken, and losing the
/// authorization response to a `PoisonError` would strand a sign-in that
/// otherwise succeeded.
pub(super) fn captured_query() -> Option<String> {
    CAPTURED_QUERY
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone()
}

fn set_captured_query(value: Option<String>) {
    *CAPTURED_QUERY.lock().unwrap_or_else(|e| e.into_inner()) = value;
}

/// Start (or decline to restart) the loopback sign-in.
pub(super) fn start_login(cfg: &OidcConfig, return_to: String) -> Result<(), FlowError> {
    if IN_PROGRESS.swap(true, Ordering::SeqCst) {
        tracing::debug!("a loopback sign-in is already waiting for its redirect");
        return Ok(());
    }

    // The router is how a response, or a failure to get one, reaches
    // `/auth/callback`. Read it before anything is bound or opened: with
    // no router there is nowhere to deliver a code and nowhere to report,
    // so the flow must not start at all.
    let Some(router) = dioxus::prelude::try_consume_context::<RouterContext>() else {
        IN_PROGRESS.store(false, Ordering::SeqCst);
        return Err(FlowError::Redirect(
            "no router to return to after signing in".to_string(),
        ));
    };

    kickoff(cfg, return_to, router).inspect_err(|e| {
        // Nothing is waiting on a listener, so the next attempt must be
        // allowed to run.
        IN_PROGRESS.store(false, Ordering::SeqCst);
        // The caller logs this and then renders "Signing you in", which
        // would stay up forever: no browser opened, so no redirect is
        // coming. Send the reason to the same error screen a failure
        // later in the flow lands on.
        report_failure(router, &e.to_string());
    })
}

fn kickoff(cfg: &OidcConfig, return_to: String, router: RouterContext) -> Result<(), FlowError> {
    // A previous flow's response must never satisfy this one.
    set_captured_query(None);

    // Bind BEFORE building the URL. The port is half of the redirect_uri
    // the authorize request carries (`OidcConfig::resolve_redirect_uri`).
    let listener = crate::platform::loopback::bind().map_err(FlowError::Network)?;
    let url = super::flow::authorize_url(cfg, return_to)?;

    crate::platform::location::open_external(&url).map_err(|e| {
        FlowError::Redirect(format!(
            "could not open the identity provider in a browser: {e}"
        ))
    })?;

    dioxus::core::spawn_forever(async move {
        match listener.serve_one(REDIRECT_TIMEOUT).await {
            Ok(query) => set_captured_query(Some(query)),
            Err(e) => {
                // The user is sitting on "Signing you in", so this cannot
                // end in a log line alone. Hand the callback route an
                // OAuth-shaped error response and let it render the
                // reason, the same way it renders one the OP sent.
                super::log_auth_error(&format!("loopback sign-in failed: {e}"));
                set_captured_query(Some(error_query(LOOPBACK_ERROR_CODE, &e)));
            }
        }
        IN_PROGRESS.store(false, Ordering::SeqCst);
        navigate_to_callback(router);
    });
    Ok(())
}

/// Show the user why the loopback flow never got started.
///
/// The navigation is deferred to a task because `start_login` is called
/// from inside a render, and routing from there is the router writing to
/// itself mid-render.
fn report_failure(router: RouterContext, description: &str) {
    set_captured_query(Some(error_query(LOOPBACK_ERROR_CODE, description)));
    dioxus::core::spawn_forever(async move {
        navigate_to_callback(router);
    });
}

/// Hand the captured response, whatever it says, to `/auth/callback`.
fn navigate_to_callback(router: RouterContext) {
    if let Some(failure) = router.push(crate::Route::AuthCallback {}) {
        super::log_auth_error(&format!(
            "loopback sign-in could not reach /auth/callback: {failure:?}"
        ));
    }
}

/// Encode a local failure the way the OP encodes its own
/// (`?error=&error_description=`), so it reaches the user through the
/// one path that already classifies and renders an authorization error.
fn error_query(code: &str, description: &str) -> String {
    format!(
        "?error={}&error_description={}",
        crate::utils::url::encode_uri_component(code),
        crate::utils::url::encode_uri_component(description)
    )
}

#[cfg(test)]
mod tests {
    use super::{error_query, LOOPBACK_ERROR_CODE, REDIRECT_TIMEOUT};
    use crate::modules::oidc::storage::PENDING_FLOW_TTL_MS;
    use crate::modules::oidc::{classify_flow_error, CallbackRecovery, FlowError};
    use crate::utils::url::QueryString;

    #[test]
    fn a_loopback_failure_reaches_the_callback_as_an_authorization_error() {
        let query = error_query(
            LOOPBACK_ERROR_CODE,
            "no redirect arrived within 300 seconds",
        );
        let parsed = QueryString::parse(&query);
        assert_eq!(parsed.get("error").as_deref(), Some(LOOPBACK_ERROR_CODE));
        assert_eq!(
            parsed.get("error_description").as_deref(),
            Some("no redirect arrived within 300 seconds")
        );
    }

    /// The callback page shows an error it cannot recover from by
    /// restarting. A loopback failure is one: retrying it silently would
    /// reopen the browser on a loop with nothing on screen to say why.
    #[test]
    fn a_loopback_failure_is_shown_not_silently_restarted() {
        let e = FlowError::TokenEndpoint {
            error: LOOPBACK_ERROR_CODE.to_string(),
            description: "the loopback connection failed".to_string(),
        };
        assert_eq!(classify_flow_error(&e), CallbackRecovery::Show);
    }

    /// A redirect that arrives just before the listener gives up must
    /// still find a live `PendingFlow` to complete.
    #[test]
    fn the_redirect_timeout_fits_inside_the_pending_flow_ttl() {
        assert!(REDIRECT_TIMEOUT.as_millis() < u128::from(PENDING_FLOW_TTL_MS));
    }
}
