//! Data fetching hooks

use dioxus::prelude::*;
use serde::de::DeserializeOwned;
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
    let fetch_fn_clone = fetch_fn.clone();

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
}
