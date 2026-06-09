//! Data fetching hooks

use dioxus::prelude::*;
use std::future::Future;

/// Fetch state for async data loading
#[derive(Clone)]
pub enum FetchState<T> {
    /// Initial state, no fetch attempted
    Idle,
    /// Fetch in progress
    Loading,
    /// Fetch completed successfully
    Success(T),
    /// Fetch failed with error
    Error(String),
}

impl<T> FetchState<T> {
    pub fn is_loading(&self) -> bool {
        matches!(self, FetchState::Loading)
    }

    pub fn is_success(&self) -> bool {
        matches!(self, FetchState::Success(_))
    }

    pub fn is_error(&self) -> bool {
        matches!(self, FetchState::Error(_))
    }

    pub fn data(&self) -> Option<&T> {
        match self {
            FetchState::Success(data) => Some(data),
            _ => None,
        }
    }

    pub fn error(&self) -> Option<&str> {
        match self {
            FetchState::Error(err) => Some(err),
            _ => None,
        }
    }
}

// Not derived: `#[derive(Default)]` on a generic enum adds a `T: Default`
// bound, but `FetchState::<PaginatedData<T>>::default()` is called where
// `T` is not `Default`. The manual impl stays unbounded.
#[allow(clippy::derivable_impls)]
impl<T> Default for FetchState<T> {
    fn default() -> Self {
        FetchState::Idle
    }
}

/// Paginated response wrapper
#[derive(Clone)]
pub struct PaginatedData<T> {
    pub items: Vec<T>,
    pub page: usize,
    pub per_page: usize,
    pub total: usize,
}

impl<T> PaginatedData<T> {
    pub fn total_pages(&self) -> usize {
        self.total.div_ceil(self.per_page)
    }
}

/// Hook for fetching data with loading/error states
/// Returns (state, refetch function)
pub fn use_fetch<T, F, Fut>(fetch_fn: F) -> (Signal<FetchState<T>>, impl Fn())
where
    T: Clone + 'static,
    F: Fn() -> Fut + Clone + 'static,
    Fut: Future<Output = Result<T, String>> + 'static,
{
    let mut state = use_signal(FetchState::<T>::default);
    let fetch_fn_clone = fetch_fn.clone();

    // Initial fetch
    use_effect(move || {
        let fetch_fn = fetch_fn.clone();
        spawn(async move {
            state.set(FetchState::Loading);
            match fetch_fn().await {
                Ok(data) => state.set(FetchState::Success(data)),
                Err(err) => state.set(FetchState::Error(err)),
            }
        });
    });

    // Refetch function
    let refetch = move || {
        let fetch_fn = fetch_fn_clone.clone();
        spawn(async move {
            state.set(FetchState::Loading);
            match fetch_fn().await {
                Ok(data) => state.set(FetchState::Success(data)),
                Err(err) => state.set(FetchState::Error(err)),
            }
        });
    };

    (state, refetch)
}

/// Hook for paginated data fetching
#[allow(clippy::type_complexity)]
pub fn use_paginated_fetch<T, F, Fut>(
    fetch_fn: F,
    initial_page: usize,
    per_page: usize,
) -> (
    Signal<FetchState<PaginatedData<T>>>,
    Signal<usize>,
    impl FnMut(usize),
)
where
    T: Clone + 'static,
    F: Fn(usize, usize) -> Fut + Clone + 'static,
    Fut: Future<Output = Result<PaginatedData<T>, String>> + 'static,
{
    let mut state = use_signal(FetchState::<PaginatedData<T>>::default);
    let mut page = use_signal(|| initial_page);

    // Fetch when page changes
    use_effect(move || {
        let current_page = *page.read();
        let fetch_fn = fetch_fn.clone();
        spawn(async move {
            state.set(FetchState::Loading);
            match fetch_fn(current_page, per_page).await {
                Ok(data) => state.set(FetchState::Success(data)),
                Err(err) => state.set(FetchState::Error(err)),
            }
        });
    });

    // Change page function
    let change_page = move |new_page: usize| {
        page.set(new_page);
    };

    (state, page, change_page)
}

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
    fn api_base() -> String {
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
    /// bump is unconditional: a refresh that returns the same tenant's
    /// token is cheap to re-fetch, and on an org switch the token
    /// (and the tenant scope it encodes) actually changes.
    #[cfg(feature = "web")]
    pub fn set_access_token(token: Option<String>) {
        ACCESS_TOKEN.with(|t| *t.borrow_mut() = token);
        *super::TENANT_GENERATION.write() += 1;
    }

    /// Read the current access token. Returns `None` before sign-in.
    #[cfg(feature = "web")]
    pub fn current_access_token() -> Option<String> {
        ACCESS_TOKEN.with(|t| t.borrow().clone())
    }

    fn _doc_anchor_keep_module_grouping() {}

    /// Get request
    #[cfg(feature = "web")]
    pub async fn get<T: DeserializeOwned>(path: &str) -> Result<T, String> {
        let url = format!("{}{}", api_base(), path);

        let response = Request::get(&url)
            .header("Content-Type", "application/json")
            .send()
            .await
            .map_err(|e| e.to_string())?;

        if response.ok() {
            response.json::<T>().await.map_err(|e| e.to_string())
        } else {
            Err(format!("Request failed with status: {}", response.status()))
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
            .map_err(|e| e.to_string())?;

        if response.ok() {
            response.json::<T>().await.map_err(|e| e.to_string())
        } else {
            Err(format!("Request failed with status: {}", response.status()))
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
            .map_err(|e| e.to_string())?;

        if response.ok() {
            response.json::<T>().await.map_err(|e| e.to_string())
        } else {
            Err(format!("Request failed with status: {}", response.status()))
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
            .map_err(|e| e.to_string())?;

        if response.ok() {
            response.json::<T>().await.map_err(|e| e.to_string())
        } else {
            Err(format!("Request failed with status: {}", response.status()))
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
            .map_err(|e| e.to_string())?;

        if response.ok() {
            response.json::<T>().await.map_err(|e| e.to_string())
        } else {
            Err(format!("Request failed with status: {}", response.status()))
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
            .map_err(|e| e.to_string())?;

        if response.ok() {
            Ok(())
        } else {
            Err(format!("Request failed with status: {}", response.status()))
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
            .map_err(|e| e.to_string())?;

        if response.ok() {
            Ok(())
        } else {
            Err(format!("Request failed with status: {}", response.status()))
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
        /// truncated to a sensible length.
        Status { code: u16, message: String },
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
                Self::Status { code, message } => match *code {
                    401 => "Your session has expired. Please sign in again.".into(),
                    403 => "You do not have permission to do that.".into(),
                    404 => "The requested resource was not found.".into(),
                    409 if !message.is_empty() => message.clone(),
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
    }

    #[cfg(feature = "web")]
    impl std::fmt::Display for ApiError {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            match self {
                Self::Network(e) => write!(f, "network error: {e}"),
                Self::Status { code, message } => {
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
        if (200..300).contains(&status) {
            return response
                .json::<T>()
                .await
                .map_err(|e| ApiError::Decode(e.to_string()));
        }
        let body = response.text().await.unwrap_or_default();
        let message = serde_json::from_str::<crate::utils::error::ErrorResponse>(&body)
            .map(|env| env.error.message)
            .unwrap_or_else(|_| {
                // Fall back to the raw body, capped so a runaway HTML
                // 500 page doesn't end up in a toast.
                body.chars().take(200).collect()
            });
        Err(ApiError::Status {
            code: status,
            message,
        })
    }

    #[cfg(feature = "web")]
    pub async fn get_authed_typed<T: DeserializeOwned>(path: &str) -> Result<T, ApiError> {
        let url = format!("{}{}", api_base(), path);
        let mut req = Request::get(&url).header("Content-Type", "application/json");
        if let Some(t) = current_access_token() {
            req = req.header("Authorization", &format!("Bearer {t}"));
        }
        let resp = req
            .send()
            .await
            .map_err(|e| ApiError::Network(e.to_string()))?;
        handle_response(resp).await
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
            .map_err(|e| ApiError::Network(e.to_string()))?;
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
        })?;
        let url = format!("{}{}", api_base(), path);
        let resp = Request::put(&url)
            .header("Content-Type", "application/json")
            .header("Authorization", &format!("Bearer {t}"))
            .json(body)
            .map_err(|e| ApiError::Network(e.to_string()))?
            .send()
            .await
            .map_err(|e| ApiError::Network(e.to_string()))?;
        handle_response(resp).await
    }

    #[cfg(feature = "web")]
    pub async fn delete_authed_typed(path: &str) -> Result<(), ApiError> {
        let t = current_access_token().ok_or_else(|| ApiError::Status {
            code: 401,
            message: String::new(),
        })?;
        let url = format!("{}{}", api_base(), path);
        let resp = Request::delete(&url)
            .header("Content-Type", "application/json")
            .header("Authorization", &format!("Bearer {t}"))
            .send()
            .await
            .map_err(|e| ApiError::Network(e.to_string()))?;
        let status = resp.status();
        if (200..300).contains(&status) {
            Ok(())
        } else {
            let body = resp.text().await.unwrap_or_default();
            let message = serde_json::from_str::<crate::utils::error::ErrorResponse>(&body)
                .map(|env| env.error.message)
                .unwrap_or_else(|_| body.chars().take(200).collect());
            Err(ApiError::Status {
                code: status,
                message,
            })
        }
    }
}
