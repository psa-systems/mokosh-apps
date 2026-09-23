//! Billing module: invoices, payments, payment gateway configs, tax rates.
//! The schema is mokosh-server's, in `migrations/010_billing.sql` plus the
//! later per-feature migrations; this crate carries no migrations of its own.
//!
//! Shared module: this is the client's copy of the mokosh-server module and
//! compiles only the model types. Routes + service are gated behind the
//! `server` feature, which no client build enables, so the WASM build omits
//! the axum/sqlx code.

mod models;
#[cfg(feature = "server")]
mod routes;
#[cfg(feature = "server")]
mod service;

pub use models::*;
#[cfg(feature = "server")]
pub use routes::billing_routes;
#[cfg(feature = "server")]
pub use service::BillingService;
