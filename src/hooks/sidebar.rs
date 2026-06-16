//! Sidebar collapse-state context.
//!
//! Each NavSection has its own open/closed state, keyed by section title.
//! State lives in a Signal owned by the App root so it persists across
//! SPA navigations (each page re-mounts AppLayout, but the signal is
//! stored in App's context which outlives the navigation).
//!
//! Default: a section that has never been toggled is open.

use dioxus::prelude::*;
use std::collections::HashMap;

#[derive(Clone, Default)]
pub struct SidebarState {
    pub collapsed: HashMap<String, bool>,
}

/// Provide the sidebar-state signal at the App root. Mirrors the
/// use_auth_provider / use_auth pattern.
pub fn use_sidebar_provider() -> Signal<SidebarState> {
    let state = use_signal(SidebarState::default);
    use_context_provider(|| state);
    state
}

/// Consume the sidebar-state signal in a NavSection.
pub fn use_sidebar_state() -> Signal<SidebarState> {
    use_context::<Signal<SidebarState>>()
}

/// Returns true if the given section is currently collapsed.
/// Default (no entry) is false (i.e. expanded).
pub fn is_section_collapsed(state: &SidebarState, title: &str) -> bool {
    *state.collapsed.get(title).unwrap_or(&false)
}

/// Last-known scroll offset (px) of the desktop sidebar nav.
///
/// MAPPS-203: the sidebar lives inside `AppLayout`, and every page wraps
/// its content in a fresh `AppLayout`, so each SPA navigation tears the
/// sidebar down and re-mounts it. A re-mounted scroll container starts at
/// `scrollTop = 0`, which is the user-reported "sidebar scrolls all the
/// way to the top on every click" symptom. Holding the offset in a signal
/// owned by the App root (which outlives the navigation, exactly like the
/// collapse state above) lets the nav restore its prior position on each
/// re-mount.
///
/// A newtype rather than a bare `Signal<i32>` so the context lookup is
/// unambiguous: a plain int signal could collide with any other int
/// provided at the root.
#[derive(Clone, Copy, Default)]
pub struct SidebarScroll(pub i32);

/// Provide the sidebar scroll-offset signal at the App root. Mirrors
/// [`use_sidebar_provider`].
pub fn use_sidebar_scroll_provider() -> Signal<SidebarScroll> {
    let state = use_signal(SidebarScroll::default);
    use_context_provider(|| state);
    state
}

/// Consume the sidebar scroll-offset signal in the sidebar nav.
pub fn use_sidebar_scroll() -> Signal<SidebarScroll> {
    use_context::<Signal<SidebarScroll>>()
}
