//! Theme system (MAPPS-259): the curated accent catalog and the WCAG
//! contrast math behind it. Pure data + logic, no web_sys, so it is
//! native-testable. The runtime application (toggling the base class and
//! injecting the accent CSS variables on `<html>`) lives in
//! `crate::hooks::theme`.

pub mod accents;
pub mod contrast;

pub use accents::{by_id, default_accent, resolve, Accent, Variant, ACCENTS, DEFAULT_ACCENT_ID};
