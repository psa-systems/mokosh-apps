//! Page components for the Mokosh Platform
//!
//! Each submodule contains the page components for a specific feature area.
//!
//! Account-management surfaces (login, signup, password reset, invite
//! accept, profile, security, sessions, audit logs, user management)
//! now live on the Bunyip hub. The matching routes in `lib.rs` are
//! one-line redirect stubs that send bookmarks at the legacy URLs to
//! the hub.

pub mod admin;
pub mod approvals;
pub mod assets;
pub mod audit_log;
pub mod auth_callback;
pub mod big_view;
pub mod billing;
pub mod button_showcase;
pub mod calendar;
pub mod contacts;
pub mod contracts;
pub mod dashboard;
pub mod dashboards;
pub mod dashboards_view;
pub mod forms;
pub mod home;
pub mod knowledge_base;
pub mod login;
pub mod not_found;
pub mod onboarding;
pub mod portal;
pub mod portal_login;
pub mod portal_set_password;
pub mod profile;
pub mod projects;
pub mod quotes;
pub mod reports;
pub mod request_form;
pub mod settings;
pub mod sla;
pub mod system_status;
pub mod team;
pub mod tickets;
pub mod time;
