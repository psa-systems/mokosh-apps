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
        (self.total + self.per_page - 1) / self.per_page
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

/// API client for making HTTP requests
pub mod api {
    #[cfg(feature = "web")]
    use gloo_net::http::Request;
    #[cfg(feature = "web")]
    use serde::{de::DeserializeOwned, Serialize};

    #[cfg(feature = "web")]
    const API_BASE: &str = "/api/v1";

    // Single-threaded global access-token holder. WASM is strictly
    // single-threaded so a `RefCell` is safe; we don't need a mutex.
    // The token lives only in memory: it is wiped on logout and never
    // written to localStorage.
    #[cfg(feature = "web")]
    thread_local! {
        static ACCESS_TOKEN: std::cell::RefCell<Option<String>> = const { std::cell::RefCell::new(None) };
    }

    /// Set the current access token. Called from the OIDC callback
    /// handler once `complete_login` returns successfully.
    #[cfg(feature = "web")]
    pub fn set_access_token(token: Option<String>) {
        ACCESS_TOKEN.with(|t| *t.borrow_mut() = token);
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
        let url = format!("{}{}", API_BASE, path);

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
        let url = format!("{}{}", API_BASE, path);

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
        let url = format!("{}{}", API_BASE, path);

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
        let url = format!("{}{}", API_BASE, path);

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
        let url = format!("{}{}", API_BASE, path);

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
        let url = format!("{}{}", API_BASE, path);

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
}
