//! The machine's local timezone (MAPPS-504).
//!
//! Used to pre-fill the profile timezone picker, so a wrong answer is a
//! bad default the user then has to correct, not a broken screen. Both
//! implementations return `None` rather than guessing UTC when the host
//! will not say.

/// The local zone as an IANA name, e.g. `America/Chicago`.
#[cfg(target_arch = "wasm32")]
pub fn local_iana() -> Option<String> {
    use wasm_bindgen::{JsCast, JsValue};

    let intl = js_sys::Reflect::get(&js_sys::global(), &JsValue::from_str("Intl")).ok()?;
    let dtf_ctor = js_sys::Reflect::get(&intl, &JsValue::from_str("DateTimeFormat")).ok()?;
    let dtf_fn = dtf_ctor.dyn_into::<js_sys::Function>().ok()?;
    let dtf = js_sys::Reflect::construct(&dtf_fn, &js_sys::Array::new()).ok()?;
    let resolved_options_fn = js_sys::Reflect::get(&dtf, &JsValue::from_str("resolvedOptions"))
        .ok()?
        .dyn_into::<js_sys::Function>()
        .ok()?;
    let resolved = resolved_options_fn.call0(&dtf).ok()?;
    js_sys::Reflect::get(&resolved, &JsValue::from_str("timeZone"))
        .ok()?
        .as_string()
}

#[cfg(not(target_arch = "wasm32"))]
pub fn local_iana() -> Option<String> {
    match iana_time_zone::get_timezone() {
        Ok(tz) => Some(tz),
        Err(e) => {
            // Not fatal: the picker falls back to its own default. Worth
            // a line, because the user is about to see a timezone that
            // is not theirs and will have no idea why.
            tracing::warn!("could not read the system timezone: {e}");
            None
        }
    }
}
