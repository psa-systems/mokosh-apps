//! Custom hooks for the Mokosh Platform
//!
//! This module provides reusable hooks for common patterns like:
//! - Authentication state management
//! - Data fetching with loading/error states
//! - Form handling
//! - Pagination

pub mod auth;
pub mod branding;
pub mod capabilities;
pub mod contact_auth;
// `fetch` is `pub` (not `mod`) because its inner `api` submodule is
// referenced from places outside hooks/* (oidc callback, login handler)
// to set/clear the global access-token holder. Keep this `pub` form
// when merging from main.
pub mod dropdown_nav;
pub mod edit_queue;
pub mod fetch;
pub mod mentions;
pub mod pending_login;
// mokosh-contact-login: portal_* hooks retired on this branch (prompt
// 001). Contact plane hooks land in prompts 004-006.
pub mod remote_data;
pub mod server_status;
mod sidebar;
pub mod theme;
pub mod theme_sync;
pub mod toast;
pub mod tv_view;
pub mod unsaved_guard;
pub mod update_check;
pub mod version_cache;

pub use auth::*;
pub use dropdown_nav::{use_dropdown_nav, DropdownNav, NavAction};
pub use edit_queue::{use_replay_pending_edits, PendingEdit};
pub use fetch::*;
pub use mentions::{mention_people, use_mention_directory};
pub use remote_data::{classify_remote, use_remote_resource, RemoteData};
pub use server_status::{
    use_can_mutate, use_server_reachable, use_server_status_monitor, use_update_pending,
};
pub use sidebar::*;
pub use theme::use_apply_theme;
pub use theme_sync::use_theme_sync;
pub use toast::*;
pub use unsaved_guard::use_unsaved_guard;
pub use update_check::use_update_check;
pub use version_cache::{use_version_cache, use_version_cache_provider, CachedVersion};
