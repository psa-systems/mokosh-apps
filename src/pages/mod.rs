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
pub mod assets;
pub mod audit_log;
pub mod auth_callback;
pub mod billing;
pub mod calendar;
pub mod contacts;
pub mod contracts;
pub mod dashboard;
pub mod home;
pub mod knowledge_base;
pub mod not_found;
pub mod portal;
pub mod projects;
pub mod reports;
pub mod tickets;
pub mod time;
