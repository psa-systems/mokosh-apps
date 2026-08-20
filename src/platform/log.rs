//! Getting a line in front of an operator (MAPPS-504).
//!
//! The WASM build wires no `tracing` subscriber, so `tracing::error!`
//! goes nowhere there and the browser console is the only place a
//! message survives. The desktop build has a subscriber and a stderr,
//! and no console.
//!
//! Only for failures the USER is not shown. Anything they should see
//! belongs on screen; see the error-visibility rule this module exists
//! to keep honest.

/// Record an error that the user is not being shown.
#[cfg(target_arch = "wasm32")]
pub fn error(msg: &str) {
    web_sys::console::error_1(&wasm_bindgen::JsValue::from_str(msg));
}

#[cfg(not(target_arch = "wasm32"))]
pub fn error(msg: &str) {
    tracing::error!("{msg}");
}
