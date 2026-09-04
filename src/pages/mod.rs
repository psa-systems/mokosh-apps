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
// MAPPS-590 (mokosh-contact-login prompt 012): Company-scoped portal
// role editor. New page `/companies/:company_id/roles/:id`; the list
// of Company-scoped roles lives on the `CompanyRolesCard` inside
// `contacts.rs`.
pub mod company_role_edit;
pub mod contacts;
pub mod contracts;
pub mod create_org;
pub mod credit_notes;
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
// prompt 005. Main added `portal`, `portal_forgot_password`,
// `portal_login`, `portal_reset_password`, `portal_set_password` which
// we deliberately leave dropped here.
pub mod contact_portal;
pub mod products;
pub mod profile;
pub mod projects;
pub mod quotes;
pub mod reports;
pub mod request_form;
pub mod request_links;
pub mod reset_password;
pub mod set_password;
pub mod settings;
pub mod settings_branding;
// mokosh-contact-login prompt 007: sibling module for the Settings >
// Contact Roles pages. Kept out of `settings.rs` so the existing 6.6k
// LOC file stays intact.
pub mod settings_contact_roles;
pub mod sla;
pub mod statements;
pub mod system_status;
pub mod teams;
pub mod tickets;
pub mod time;
