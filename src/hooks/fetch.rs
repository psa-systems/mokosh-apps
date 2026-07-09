//! Data fetching hooks

use dioxus::prelude::*;

/// Monotonic "active tenant / token generation" counter.
///
/// The access token itself lives in a thread-local (`ACCESS_TOKEN`),
/// which Dioxus cannot track as a reactive dependency: a `use_resource`
/// closure that only reads `current_access_token()` never re-runs when
/// the token is swapped on an org switch, so the page keeps rendering
/// the previous tenant's cached data (Phase-4 F1).
///
/// This `GlobalSignal` is bumped every time the token is set (login,
/// refresh, org switch, logout). List/detail pages read it inside their
/// `use_resource` closure (see [`active_tenant_generation`]) so Dioxus
/// records it as a dependency and re-fetches whenever the active tenant
/// changes. WASM is single-threaded, so a `GlobalSignal` is the right
/// primitive here (same rationale as the toast surface).
#[cfg(feature = "web")]
pub static TENANT_GENERATION: GlobalSignal<u64> = Signal::global(|| 0);

/// Read the active-tenant generation counter. Call this INSIDE a
/// `use_resource` closure (before any early return) so Dioxus subscribes
/// the resource to it and re-fetches on the next org switch / token swap.
/// The returned value is otherwise unused; it exists purely to register
/// the reactive dependency.
#[cfg(feature = "web")]
pub fn active_tenant_generation() -> u64 {
    *TENANT_GENERATION.read()
}

/// Non-web stub so the same call site compiles under `cargo check`
/// without the `web` feature.
#[cfg(not(feature = "web"))]
pub fn active_tenant_generation() -> u64 {
    0
}

/// App-wide "is mokosh-server reachable" flag (MAPPS-333). `true`
/// (reachable) on boot; flipped to `false` by the classification helpers
/// below when a request fails with a "down" condition, and back to
/// `true` on the first response that proves the server is answering.
///
/// A `GlobalSignal` (not a context `Signal`) for the same reason
/// [`TENANT_GENERATION`] is one: the `api` helpers below are plain async
/// fns, not components, so they cannot reach a context-provided signal.
/// WASM is single-threaded, so a `GlobalSignal` is the right primitive
/// (same rationale as the toast surface). The banner reads it via
/// [`crate::hooks::use_server_reachable`] and the recovery poll lives in
/// [`crate::hooks::server_status`].
#[cfg(feature = "web")]
pub static SERVER_REACHABLE: GlobalSignal<bool> = Signal::global(|| true);

/// MAPPS-348: sticky "the current user's bunyip account was deleted" flag.
/// Flipped to `true` the moment ANY fetch on the SPA sees a `410 Gone`
/// response carrying `error.code == "ACCOUNT_DELETED"` (the terminal
/// signal mokosh-server emits from every auth extractor when
/// `users.deleted_at` is set - see mokosh-server MAPPS-348). Never
/// flipped back to `false`: this is a one-way transition, the same
/// deletion the Bunyip webhook triggered on the server. The AppLayout
/// reads it via [`crate::hooks::use_account_deleted`] and renders the
/// terminal modal + logout countdown; the fetch layer STOPS bubbling
/// the 4xx through the per-page error surface once the signal has
/// flipped, so a burst of 410s does not spam toasts on top of the
/// modal.
///
/// GlobalSignal (not context) for the same reason [`SERVER_REACHABLE`]
/// is: the `api` helpers are plain async fns and can't reach a
/// context-provided signal.
#[cfg(feature = "web")]
pub static ACCOUNT_DELETED: GlobalSignal<bool> = Signal::global(|| false);

/// Flip [`ACCOUNT_DELETED`] to `true`. Idempotent (called from every
/// fetch on the error path once the account is tombstoned, but only
/// writes to the signal on the first observation) so a burst of
/// concurrent 410s does not wake readers repeatedly.
#[cfg(feature = "web")]
pub(crate) fn note_account_deleted() {
    if !*ACCOUNT_DELETED.peek() {
        *ACCOUNT_DELETED.write() = true;
    }
}

/// Classify a completed HTTP response. Any response at all - even a 4xx -
/// proves the server is reachable, so it clears the "down" state. A `5xx`
/// is treated as "down" per MAPPS-333: the server is up but failing, and
/// we surface that the same way as an outage. A `4xx` (auth, validation)
/// is NOT "down" and keeps surfacing through the normal per-call paths.
#[cfg(feature = "web")]
pub(crate) fn note_response_status(status: u16) {
    set_server_reachable(!(500..600).contains(&status));
}

/// Classify a transport-level failure: the opaque browser fetch rejection
/// (which the console frequently renders as a CORS error even though no
/// CORS misconfiguration exists), DNS failure, or timeout. This is the
/// server being unreachable; per MAPPS-333 it is classified here as
/// server-down and never passed through to the user as a CORS / "Failed
/// to fetch" message. (Mokosh App sets no CSP of its own, so there is no
/// CSP-block case to distinguish from a genuine outage here.)
#[cfg(feature = "web")]
pub(crate) fn note_transport_error() {
    set_server_reachable(false);
}

/// Write the reachability flag, but only on an actual transition so a
/// successful request does not wake every reader on each call.
#[cfg(feature = "web")]
fn set_server_reachable(reachable: bool) {
    if *SERVER_REACHABLE.peek() != reachable {
        *SERVER_REACHABLE.write() = reachable;
    }
}

/// API client for making HTTP requests
pub mod api {
    #[cfg(feature = "web")]
    use gloo_net::http::Request;
    #[cfg(feature = "web")]
    use serde::{de::DeserializeOwned, Serialize};

    /// Derive the Mokosh API base URL.
    ///
    /// Resolution order:
    ///   1. `window.__MOKOSH_CONFIG__.api_base` if set by the prod
    ///      container's entrypoint. Self-hosters on a custom hostname
    ///      override here without rebuilding the image.
    ///   2. Host-prefix derivation for the canonical `msp.<tld>`
    ///      deploys (e.g. `msp.a8n.systems` SPA → `api.msp.a8n.systems`
    ///      API).
    ///   3. Same-origin `/api/v1` for dev (localhost, IP address, or
    ///      any host that doesn't start with `msp.`) so the Dioxus dev
    ///      server can proxy to a local backend.
    #[cfg(feature = "web")]
    pub fn api_base() -> String {
        if let Some(injected) = crate::modules::runtime_config::get("api_base") {
            return injected;
        }
        if let Some(win) = web_sys::window() {
            if let Ok(host) = win.location().host() {
                if let Some(rest) = host.strip_prefix("msp.") {
                    return format!("https://api.msp.{rest}/api/v1");
                }
            }
        }
        "/api/v1".to_string()
    }

    // Single-threaded global access-token holder. WASM is strictly
    // single-threaded so a `RefCell` is safe; we don't need a mutex.
    // The token lives only in memory: it is wiped on logout and never
    // written to localStorage.
    #[cfg(feature = "web")]
    thread_local! {
        static ACCESS_TOKEN: std::cell::RefCell<Option<String>> = const { std::cell::RefCell::new(None) };
    }

    /// Set the current access token. Called from the OIDC callback
    /// handler once `complete_login` returns successfully, and again on
    /// every token refresh / org switch / logout.
    ///
    /// Bumps the reactive [`super::TENANT_GENERATION`] counter so any
    /// `use_resource` that read [`super::active_tenant_generation`] in
    /// its closure re-fetches against the new tenant (Phase-4 F1). The
    /// generation is bumped only when the token actually changes: an org
    /// switch or a refresh produces a different token (so dependent
    /// resources refetch), but a redundant re-set of the identical token
    /// does not. Startup sets the same token twice (the OIDC callback, then
    /// rehydration / `complete_login`); the previous unconditional bump made
    /// every `active_tenant_generation`-subscribed resource fetch twice on
    /// mount (MAPPS-187).
    #[cfg(feature = "web")]
    pub fn set_access_token(token: Option<String>) {
        let changed = ACCESS_TOKEN.with(|t| {
            let mut slot = t.borrow_mut();
            if *slot == token {
                false
            } else {
                *slot = token;
                true
            }
        });
        if changed {
            *super::TENANT_GENERATION.write() += 1;
        }
    }

    /// Read the current access token. Returns `None` before sign-in.
    #[cfg(feature = "web")]
    pub fn current_access_token() -> Option<String> {
        ACCESS_TOKEN.with(|t| t.borrow().clone())
    }

    // The web-only API helpers below are grouped under this `api`
    // module; the non-`web` build compiles the module with no items.

    /// Map a transport-level send failure to a `String` error, classifying
    /// it as a server-unreachable condition (MAPPS-333) on the way out.
    /// Used only at `.send()` sites - serialization (`.json()`) failures
    /// keep the plain mapping since they are not connectivity problems.
    #[cfg(feature = "web")]
    fn transport_err(e: impl std::fmt::Display) -> String {
        super::note_transport_error();
        e.to_string()
    }

    /// Transport-error sibling of [`transport_err`] for the typed helpers,
    /// classifying the failure as server-unreachable before wrapping it as
    /// [`ApiError::Network`].
    #[cfg(feature = "web")]
    fn network_err(e: impl std::fmt::Display) -> ApiError {
        super::note_transport_error();
        ApiError::Network(e.to_string())
    }

    /// Get request
    #[cfg(feature = "web")]
    pub async fn get<T: DeserializeOwned>(path: &str) -> Result<T, String> {
        let url = format!("{}{}", api_base(), path);

        let response = Request::get(&url)
            .header("Content-Type", "application/json")
            .send()
            .await
            .map_err(transport_err)?;

        if response.ok() {
            response.json::<T>().await.map_err(|e| e.to_string())
        } else {
            Err(status_error(response).await)
        }
    }

    /// Tolerant GET that returns the HTTP status and raw body together,
    /// instead of collapsing any non-2xx into an `Err`. Used by the
    /// System Status page (PMS-237) to read diagnostic endpoints like
    /// `/ready`, which deliberately answers `503` with a JSON breakdown
    /// when a dependency is down. A transport-level failure (server
    /// unreachable, DNS, CORS) is still an `Err`.
    #[cfg(feature = "web")]
    pub async fn probe(path: &str) -> Result<(u16, String), String> {
        let url = format!("{}{}", api_base(), path);

        let response = Request::get(&url).send().await.map_err(transport_err)?;
        let status = response.status();
        // Classify the probe result so the recovery poll's `/ready` hit
        // flips the app back to reachable on success (MAPPS-333).
        super::note_response_status(status);
        let body = response.text().await.map_err(|e| e.to_string())?;
        Ok((status, body))
    }

    /// Build a user-facing error string for a non-2xx response on the
    /// string-returning helpers. Parses the standard
    /// `{"error":{"message","errors":[...]}}` envelope so forms that still
    /// use the `String`-error helpers surface the real validation message
    /// (e.g. "Title must be between 1 and 500 characters") instead of a bare
    /// "Request failed with status: 422" (MAPPS-210). The `_typed` helpers
    /// carry the field-level envelope separately via `handle_response`; this
    /// is the flat-string sibling for the many existing callers. Falls back
    /// to the status line when the body is not a recognised envelope.
    #[cfg(feature = "web")]
    async fn status_error(response: gloo_net::http::Response) -> String {
        let status = response.status();
        // A real HTTP response (even an error one) proves reachability; a
        // 5xx is classified as "down" (MAPPS-333).
        super::note_response_status(status);
        let body = response.text().await.unwrap_or_default();
        match serde_json::from_str::<crate::utils::error::ErrorResponse>(&body) {
            Ok(env) => {
                // MAPPS-348: 410 Gone with `ACCOUNT_DELETED` is the terminal
                // signal from mokosh-server that the bunyip account was
                // deleted (users.deleted_at is set). Flip the global signal
                // so AppLayout renders the terminal modal; the returned
                // message is still surfaced to any caller that inspects it,
                // but the overlay will normally block further UI.
                if status == 410 && env.error.code == "ACCOUNT_DELETED" {
                    super::note_account_deleted();
                }
                let fields = env.error.errors.unwrap_or_default();
                if status == 422 && !fields.is_empty() {
                    fields
                        .iter()
                        .map(|f| f.message.clone())
                        .collect::<Vec<_>>()
                        .join("; ")
                } else if !env.error.message.is_empty() {
                    env.error.message
                } else {
                    user_friendly_status(status)
                }
            }
            Err(_) => user_friendly_status(status),
        }
    }

    /// MAPPS-282: replace the developer-facing "Request failed with status:
    /// 422" string fallback with a user-facing message keyed on the status
    /// class. This is the last-resort branch in `status_error` (server
    /// returned a non-2xx with no `ErrorResponse` envelope and no field
    /// errors), reached by the many forms that still use the string-error
    /// helpers (`post_authed`, `put_authed`). The forms that have been
    /// migrated to the `_typed` variants already get the field-level
    /// `ApiError::user_message` treatment; this brings the legacy callers
    /// to at least non-developer-facing parity.
    #[cfg(feature = "web")]
    fn user_friendly_status(status: u16) -> String {
        match status {
            400 => "The request was rejected. Please check the form and try again.".into(),
            401 => "Your session has expired. Please sign in again.".into(),
            403 => "You do not have permission to do that.".into(),
            404 => "The requested resource was not found.".into(),
            409 => "The change conflicts with another update. Please refresh and retry.".into(),
            422 => "Validation failed. Please check the form fields.".into(),
            429 => "Too many requests. Please try again shortly.".into(),
            500..=599 => "The server hit an error. Please try again.".into(),
            _ => format!("Request failed ({status})."),
        }
    }

    /// Get request with auth token
    #[cfg(feature = "web")]
    pub async fn get_with_auth<T: DeserializeOwned>(path: &str, token: &str) -> Result<T, String> {
        let url = format!("{}{}", api_base(), path);

        let response = Request::get(&url)
            .header("Content-Type", "application/json")
            .header("Authorization", &format!("Bearer {}", token))
            .send()
            .await
            .map_err(transport_err)?;

        if response.ok() {
            response.json::<T>().await.map_err(|e| e.to_string())
        } else {
            Err(status_error(response).await)
        }
    }

    /// Post request
    #[cfg(feature = "web")]
    pub async fn post<T: DeserializeOwned, B: Serialize>(
        path: &str,
        body: &B,
    ) -> Result<T, String> {
        let url = format!("{}{}", api_base(), path);

        let response = Request::post(&url)
            .header("Content-Type", "application/json")
            .json(body)
            .map_err(|e| e.to_string())?
            .send()
            .await
            .map_err(transport_err)?;

        if response.ok() {
            response.json::<T>().await.map_err(|e| e.to_string())
        } else {
            Err(status_error(response).await)
        }
    }

    /// Post request with auth token
    #[cfg(feature = "web")]
    pub async fn post_with_auth<T: DeserializeOwned, B: Serialize>(
        path: &str,
        body: &B,
        token: &str,
    ) -> Result<T, String> {
        let url = format!("{}{}", api_base(), path);

        let response = Request::post(&url)
            .header("Content-Type", "application/json")
            .header("Authorization", &format!("Bearer {}", token))
            .json(body)
            .map_err(|e| e.to_string())?
            .send()
            .await
            .map_err(transport_err)?;

        if response.ok() {
            response.json::<T>().await.map_err(|e| e.to_string())
        } else {
            Err(status_error(response).await)
        }
    }

    /// Put request with auth token
    #[cfg(feature = "web")]
    pub async fn put_with_auth<T: DeserializeOwned, B: Serialize>(
        path: &str,
        body: &B,
        token: &str,
    ) -> Result<T, String> {
        let url = format!("{}{}", api_base(), path);

        let response = Request::put(&url)
            .header("Content-Type", "application/json")
            .header("Authorization", &format!("Bearer {}", token))
            .json(body)
            .map_err(|e| e.to_string())?
            .send()
            .await
            .map_err(transport_err)?;

        if response.ok() {
            response.json::<T>().await.map_err(|e| e.to_string())
        } else {
            Err(status_error(response).await)
        }
    }

    /// PATCH request with auth token. The first caller is the saved
    /// dashboards module (PMS-453), which surfaces partial-update
    /// semantics (toggle `is_default`, edit just the name); a PUT
    /// helper would have invited callers to send the whole row and
    /// blow away unset fields, which is the wrong shape for that
    /// surface.
    #[cfg(feature = "web")]
    pub async fn patch_with_auth<T: DeserializeOwned, B: Serialize>(
        path: &str,
        body: &B,
        token: &str,
    ) -> Result<T, String> {
        let url = format!("{}{}", api_base(), path);

        let response = Request::patch(&url)
            .header("Content-Type", "application/json")
            .header("Authorization", &format!("Bearer {}", token))
            .json(body)
            .map_err(|e| e.to_string())?
            .send()
            .await
            .map_err(transport_err)?;

        if response.ok() {
            response.json::<T>().await.map_err(|e| e.to_string())
        } else {
            Err(status_error(response).await)
        }
    }

    /// Delete request with auth token
    #[cfg(feature = "web")]
    pub async fn delete_with_auth(path: &str, token: &str) -> Result<(), String> {
        let url = format!("{}{}", api_base(), path);

        let response = Request::delete(&url)
            .header("Content-Type", "application/json")
            .header("Authorization", &format!("Bearer {}", token))
            .send()
            .await
            .map_err(transport_err)?;

        if response.ok() {
            Ok(())
        } else {
            Err(status_error(response).await)
        }
    }

    /// Post request with auth token for endpoints that return an empty
    /// body (e.g. `POST /notifications/{id}/read`, which responds 200
    /// with no JSON). The body-parsing `post_with_auth` would fail on
    /// the empty payload, so this variant only checks the status, like
    /// `delete_with_auth`. No request body is sent.
    #[cfg(feature = "web")]
    pub async fn post_no_content_with_auth(path: &str, token: &str) -> Result<(), String> {
        let url = format!("{}{}", api_base(), path);

        let response = Request::post(&url)
            .header("Authorization", &format!("Bearer {}", token))
            .send()
            .await
            .map_err(transport_err)?;

        if response.ok() {
            Ok(())
        } else {
            Err(status_error(response).await)
        }
    }

    // --- Auto-authed wrappers --------------------------------------------
    //
    // These read the current access token from the thread-local holder so
    // page code does not have to thread it through. If the user is not
    // signed in (`ACCESS_TOKEN` is None) we send the request without an
    // Authorization header and let the server's 401 surface naturally;
    // the OIDC SPA pattern then redirects to the login page.

    #[cfg(feature = "web")]
    pub async fn get_authed<T: DeserializeOwned>(path: &str) -> Result<T, String> {
        match current_access_token() {
            Some(t) => get_with_auth(path, &t).await,
            None => get(path).await,
        }
    }

    #[cfg(feature = "web")]
    pub async fn post_authed<T: DeserializeOwned, B: Serialize>(
        path: &str,
        body: &B,
    ) -> Result<T, String> {
        match current_access_token() {
            Some(t) => post_with_auth(path, body, &t).await,
            None => post(path, body).await,
        }
    }

    #[cfg(feature = "web")]
    pub async fn put_authed<T: DeserializeOwned, B: Serialize>(
        path: &str,
        body: &B,
    ) -> Result<T, String> {
        let t = current_access_token().ok_or_else(|| "not authenticated".to_string())?;
        put_with_auth(path, body, &t).await
    }

    #[cfg(feature = "web")]
    pub async fn delete_authed(path: &str) -> Result<(), String> {
        let t = current_access_token().ok_or_else(|| "not authenticated".to_string())?;
        delete_with_auth(path, &t).await
    }

    #[cfg(feature = "web")]
    pub async fn patch_authed<T: DeserializeOwned, B: Serialize>(
        path: &str,
        body: &B,
    ) -> Result<T, String> {
        let t = current_access_token().ok_or_else(|| "not authenticated".to_string())?;
        patch_with_auth(path, body, &t).await
    }

    /// Auto-authed POST for empty-body endpoints (see
    /// `post_no_content_with_auth`).
    #[cfg(feature = "web")]
    pub async fn post_authed_no_content(path: &str) -> Result<(), String> {
        let t = current_access_token().ok_or_else(|| "not authenticated".to_string())?;
        post_no_content_with_auth(path, &t).await
    }

    // --- Typed error layer ----------------------------------------------
    //
    // The string-returning helpers above are kept so existing callers
    // (companies/calendar/system_version) compile unchanged. New call
    // sites should prefer the `_typed` variants, which return an
    // `ApiError` enum that pages can map into a user-facing toast via
    // `ApiError::user_message()`.

    /// Typed HTTP error returned from the `_typed` API helpers.
    ///
    /// Variants are deliberately coarse - the goal is to give callers
    /// enough signal to render an actionable toast without forcing them
    /// to match on every possible status code.
    #[cfg(feature = "web")]
    #[derive(Debug, Clone)]
    pub enum ApiError {
        /// Transport failed before a response was received.
        Network(String),
        /// Server returned a non-2xx status. `message` is the server's
        /// `error.message` field when it parsed, otherwise the raw body
        /// truncated to a sensible length. `fields` carries the
        /// `error.errors[]` validation envelope (field / message / code)
        /// when present, so forms can surface a message next to the
        /// offending input instead of only a status string (MAPPS-210).
        Status {
            code: u16,
            message: String,
            fields: Vec<crate::utils::error::FieldError>,
        },
        /// Response was 2xx but the body could not be decoded into the
        /// target type.
        Decode(String),
    }

    #[cfg(feature = "web")]
    impl ApiError {
        /// User-facing message suitable for a toast.
        pub fn user_message(&self) -> String {
            match self {
                Self::Network(_) => "Network error. Check your connection and try again.".into(),
                Self::Status {
                    code,
                    message,
                    fields,
                } => match *code {
                    401 => "Your session has expired. Please sign in again.".into(),
                    403 => "You do not have permission to do that.".into(),
                    404 => "The requested resource was not found.".into(),
                    409 if !message.is_empty() => message.clone(),
                    // Surface the field-level validation messages when the
                    // server returned them, so a 422 reads as the actual
                    // rule ("Title must be ...") instead of the generic
                    // envelope message (MAPPS-210).
                    422 if !fields.is_empty() => fields
                        .iter()
                        .map(|f| f.message.clone())
                        .collect::<Vec<_>>()
                        .join("; "),
                    422 if !message.is_empty() => message.clone(),
                    429 => "Too many requests. Please try again shortly.".into(),
                    500..=599 => "The server hit an error. Please try again.".into(),
                    _ if !message.is_empty() => message.clone(),
                    _ => format!("Request failed ({}).", code),
                },
                Self::Decode(_) => {
                    "The server's response could not be read. Please refresh and retry.".into()
                }
            }
        }

        /// Status code if the error was a non-2xx HTTP response.
        pub fn status_code(&self) -> Option<u16> {
            match self {
                Self::Status { code, .. } => Some(*code),
                _ => None,
            }
        }

        /// Server-provided field-level validation errors, if any
        /// (MAPPS-210). Empty for transport / decode errors and for
        /// non-2xx responses that carried no `error.errors[]` envelope.
        pub fn field_errors(&self) -> &[crate::utils::error::FieldError] {
            match self {
                Self::Status { fields, .. } => fields,
                _ => &[],
            }
        }

        /// Validation message the server attached to a specific form field,
        /// if present. Lets a form route the message next to the offending
        /// input (MAPPS-210).
        pub fn field_message(&self, field: &str) -> Option<String> {
            self.field_errors()
                .iter()
                .find(|fe| fe.field == field)
                .map(|fe| fe.message.clone())
        }
    }

    #[cfg(feature = "web")]
    impl std::fmt::Display for ApiError {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            match self {
                Self::Network(e) => write!(f, "network error: {e}"),
                Self::Status { code, message, .. } => {
                    if message.is_empty() {
                        write!(f, "http {code}")
                    } else {
                        write!(f, "http {code}: {message}")
                    }
                }
                Self::Decode(e) => write!(f, "decode error: {e}"),
            }
        }
    }

    #[cfg(feature = "web")]
    async fn handle_response<T: DeserializeOwned>(
        response: gloo_net::http::Response,
    ) -> Result<T, ApiError> {
        let status = response.status();
        // Any response clears the "down" state; a 5xx sets it (MAPPS-333).
        super::note_response_status(status);
        if (200..300).contains(&status) {
            return response
                .json::<T>()
                .await
                .map_err(|e| ApiError::Decode(e.to_string()));
        }
        let body = response.text().await.unwrap_or_default();
        let (message, fields) =
            match serde_json::from_str::<crate::utils::error::ErrorResponse>(&body) {
                Ok(env) => {
                    // MAPPS-348: mirror `status_error`'s terminal-state
                    // detection on the typed path so a fetch on either
                    // helper flips the global `ACCOUNT_DELETED` signal.
                    if status == 410 && env.error.code == "ACCOUNT_DELETED" {
                        super::note_account_deleted();
                    }
                    (env.error.message, env.error.errors.unwrap_or_default())
                }
                // Fall back to the raw body, capped so a runaway HTML
                // 500 page doesn't end up in a toast.
                Err(_) => (body.chars().take(200).collect(), Vec::new()),
            };
        Err(ApiError::Status {
            code: status,
            message,
            fields,
        })
    }

    #[cfg(feature = "web")]
    pub async fn get_authed_typed<T: DeserializeOwned>(path: &str) -> Result<T, ApiError> {
        let url = format!("{}{}", api_base(), path);
        let mut req = Request::get(&url).header("Content-Type", "application/json");
        if let Some(t) = current_access_token() {
            req = req.header("Authorization", &format!("Bearer {t}"));
        }
        let resp = req.send().await.map_err(network_err)?;
        handle_response(resp).await
    }

    /// Bearer-authed GET that returns the raw response body plus the server's
    /// `Content-Disposition` filename. Used for attachment downloads the SPA
    /// cannot fetch through a plain `<a href>` because the bearer lives in WASM
    /// memory rather than a cookie (the data export, MAPPS-364).
    #[cfg(feature = "web")]
    pub async fn get_authed_bytes(path: &str) -> Result<(Vec<u8>, Option<String>), String> {
        let url = format!("{}{}", api_base(), path);
        let mut req = Request::get(&url);
        if let Some(t) = current_access_token() {
            req = req.header("Authorization", &format!("Bearer {t}"));
        }
        let resp = req.send().await.map_err(|e| {
            // Keep the server-reachable flag accurate on a transport failure,
            // like the other helpers do via `network_err`.
            super::note_transport_error();
            e.to_string()
        })?;
        // Any response clears the "down" state (a 5xx re-flags it).
        super::note_response_status(resp.status());
        if !resp.ok() {
            return Err(status_error(resp).await);
        }
        let filename = resp
            .headers()
            .get("content-disposition")
            .as_deref()
            .and_then(content_disposition_filename);
        let bytes = resp.binary().await.map_err(|e| e.to_string())?;
        Ok((bytes, filename))
    }

    /// Extract the `filename="..."` value from a `Content-Disposition` header.
    /// Only the simple quoted or unquoted `filename=` form is handled (no
    /// RFC 5987 `filename*=`), which is all the export endpoint emits.
    #[cfg(feature = "web")]
    fn content_disposition_filename(header: &str) -> Option<String> {
        let idx = header.to_ascii_lowercase().find("filename=")?;
        let raw = header[idx + "filename=".len()..].trim();
        let value = if let Some(rest) = raw.strip_prefix('"') {
            rest.split('"').next().unwrap_or("")
        } else {
            raw.split(';').next().unwrap_or(raw).trim()
        };
        let value = value.trim();
        if value.is_empty() {
            None
        } else {
            Some(value.to_string())
        }
    }

    #[cfg(feature = "web")]
    pub async fn post_authed_typed<T: DeserializeOwned, B: Serialize>(
        path: &str,
        body: &B,
    ) -> Result<T, ApiError> {
        let url = format!("{}{}", api_base(), path);
        let mut req = Request::post(&url).header("Content-Type", "application/json");
        if let Some(t) = current_access_token() {
            req = req.header("Authorization", &format!("Bearer {t}"));
        }
        let resp = req
            .json(body)
            .map_err(|e| ApiError::Network(e.to_string()))?
            .send()
            .await
            .map_err(network_err)?;
        handle_response(resp).await
    }

    #[cfg(feature = "web")]
    pub async fn put_authed_typed<T: DeserializeOwned, B: Serialize>(
        path: &str,
        body: &B,
    ) -> Result<T, ApiError> {
        let t = current_access_token().ok_or_else(|| ApiError::Status {
            code: 401,
            message: String::new(),
            fields: Vec::new(),
        })?;
        let url = format!("{}{}", api_base(), path);
        let resp = Request::put(&url)
            .header("Content-Type", "application/json")
            .header("Authorization", &format!("Bearer {t}"))
            .json(body)
            .map_err(|e| ApiError::Network(e.to_string()))?
            .send()
            .await
            .map_err(network_err)?;
        handle_response(resp).await
    }

    #[cfg(feature = "web")]
    pub async fn delete_authed_typed(path: &str) -> Result<(), ApiError> {
        let t = current_access_token().ok_or_else(|| ApiError::Status {
            code: 401,
            message: String::new(),
            fields: Vec::new(),
        })?;
        let url = format!("{}{}", api_base(), path);
        let resp = Request::delete(&url)
            .header("Content-Type", "application/json")
            .header("Authorization", &format!("Bearer {t}"))
            .send()
            .await
            .map_err(network_err)?;
        let status = resp.status();
        super::note_response_status(status);
        if (200..300).contains(&status) {
            Ok(())
        } else {
            let body = resp.text().await.unwrap_or_default();
            let (message, fields) =
                match serde_json::from_str::<crate::utils::error::ErrorResponse>(&body) {
                    Ok(env) => (env.error.message, env.error.errors.unwrap_or_default()),
                    Err(_) => (body.chars().take(200).collect(), Vec::new()),
                };
            Err(ApiError::Status {
                code: status,
                message,
                fields,
            })
        }
    }
}
