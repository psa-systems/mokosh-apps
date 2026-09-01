//! The boundary between the app and whatever is hosting it (MAPPS-504).
//!
//! Everything the app needs from its host - HTTP, storage, timers, the
//! current location, the DOM, the local timezone, saving a file - is
//! declared here once and implemented twice: against the browser on
//! `wasm32`, against the OS everywhere else.
//!
//! The split is on `target_arch`, NOT on a cargo feature. The `app`
//! feature is the app-runtime gate and is on for the desktop build too
//! (see `Cargo.toml`); what changes between a browser and a desktop
//! window is the architecture the same code is compiled for.
//!
//! `web-sys`, `js-sys`, `gloo-net`, `gloo-timers` and `wasm-bindgen` are
//! declared only under `[target.'cfg(target_arch = "wasm32")'.dependencies]`,
//! so a browser call that escapes this module fails to resolve on the
//! desktop build. Before that, those crates compiled fine on a native
//! target and produced bindings that panicked when called, which is the
//! failure mode this arrangement exists to prevent.

pub mod clipboard;
pub mod clock;
pub mod config;
pub mod dom;
pub mod download;
pub mod http;
pub mod location;
pub mod log;
// MAPPS-505: the RFC 8252 loopback redirect listener. Native only: it
// exists because a desktop window has no origin for the OP to redirect
// to, which is not a problem a browser has.
#[cfg(not(target_arch = "wasm32"))]
pub mod loopback;
pub mod prefs;
pub mod scroll_sync;
pub mod store;
pub mod timer;
pub mod tz;
pub mod window_close;
