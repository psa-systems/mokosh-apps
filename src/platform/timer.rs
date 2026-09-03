//! Async sleeps (MAPPS-504).

/// Yield for `ms` milliseconds.
///
/// Browser: `setTimeout` via `gloo_timers`. Desktop: the tokio timer the
/// webview event loop already runs on.
#[cfg(target_arch = "wasm32")]
pub async fn sleep_ms(ms: u32) {
    gloo_timers::future::TimeoutFuture::new(ms).await;
}

#[cfg(not(target_arch = "wasm32"))]
pub async fn sleep_ms(ms: u32) {
    tokio::time::sleep(std::time::Duration::from_millis(u64::from(ms))).await;
}
