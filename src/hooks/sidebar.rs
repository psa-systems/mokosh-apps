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
