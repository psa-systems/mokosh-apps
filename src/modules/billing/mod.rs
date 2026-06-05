//! Billing module: invoices, payments, payment gateway configs, tax rates.
//! Schema for all four tables lives in `001_initial_schema.sql`.
//!
//! Shared module: the client (mokosh-apps) carries a byte-identical copy and
//! compiles only the model types. Routes + service are gated behind the
//! `server` feature so the WASM build omits the axum/sqlx code.

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
