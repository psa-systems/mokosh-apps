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
pub mod create_org;
pub mod dashboard;
pub mod dashboards;
pub mod dashboards_view;
pub mod forgot_password;
pub mod forms;
pub mod home;
pub mod invitations;
pub mod knowledge_base;
pub mod login;
pub mod not_found;
pub mod onboarding;
pub mod pick_tenant;
pub mod platform_login;
// mokosh-contact-login: /portal/* customer-portal pages retired on this
// branch (prompt 001). Contact plane lands under `contact_portal` in
// prompt 005.
pub mod contact_portal;
pub mod profile;
pub mod projects;
pub mod quotes;
pub mod reports;
pub mod request_form;
pub mod request_links;
pub mod reset_password;
pub mod set_password;
pub mod settings;
pub mod sla;
pub mod system_status;
pub mod teams;
pub mod tickets;
pub mod time;
