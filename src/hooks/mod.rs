//! Custom hooks for the Mokosh Platform
//!
//! This module provides reusable hooks for common patterns like:
//! - Authentication state management
//! - Data fetching with loading/error states
//! - Form handling
//! - Pagination

pub mod asset_types;
pub mod auth;
pub mod branding;
pub mod capabilities;
pub mod contact_auth;
pub mod debounce;
pub mod kb_categories;
pub mod modules;
// `fetch` is `pub` (not `mod`) because its inner `api` submodule is
// referenced from places outside hooks/* (oidc callback, login handler)
// to set/clear the global access-token holder. Keep this `pub` form
// when merging from main.
pub mod dropdown_nav;
pub mod edit_queue;
pub mod fetch;
pub mod mentions;
pub mod payment_terms;
pub mod pending_login;
// mokosh-contact-login: portal_* hooks retired on this branch (prompt
// 001). Contact plane hooks land in prompts 004-006.
pub mod remote_data;
pub mod server_status;
mod sidebar;
pub mod task_statuses;
pub mod tax_rates;
pub mod theme;
pub mod theme_sync;
pub mod toast;
pub mod tv_view;
pub mod unsaved_guard;
pub mod update_check;
pub mod user_roster;
pub mod version_cache;
pub mod work_types;

pub use asset_types::{use_asset_types, use_asset_types_provider, AssetTypeRow};
pub use auth::*;
pub use debounce::use_debounced_signal;
pub use dropdown_nav::{use_dropdown_nav, DropdownNav, NavAction, NavRows};
pub use edit_queue::{use_replay_pending_edits, PendingEdit};
pub use fetch::*;
pub use kb_categories::{use_kb_categories, use_kb_categories_provider};
pub use mentions::{mention_people, use_mention_directory, use_mention_directory_provider};
pub use payment_terms::{use_payment_terms, use_payment_terms_provider, PaymentTermRow};
pub use remote_data::{classify_remote, use_remote_resource, RemoteData};
pub use server_status::{
    use_can_mutate, use_server_reachable, use_server_status_monitor, use_update_pending,
};
pub use sidebar::*;
pub use task_statuses::{use_task_statuses, use_task_statuses_provider, TaskStatusRow};
pub use tax_rates::{use_tax_rates, use_tax_rates_provider, TaxRateRow};
pub use theme::use_apply_theme;
pub use theme_sync::use_theme_sync;
pub use toast::*;
pub use unsaved_guard::use_unsaved_guard;
pub use update_check::use_update_check;
pub use user_roster::{use_user_roster, use_user_roster_provider, UserRow};
pub use version_cache::{use_version_cache, use_version_cache_provider, CachedVersion};
pub use work_types::{use_work_types, use_work_types_provider, WorkTypeRow};
