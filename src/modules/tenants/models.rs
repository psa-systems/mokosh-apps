//! Tenant models and types.
//!
//! MAPPS-378: these DTOs are the shared client/server wire contract, so they
//! are sourced from the `mokosh-types` crate rather than re-declared here.
//! Drift now fails the build instead of silently deserializing to a default.
//! The previous hand-maintained copies (incl. `TenantStatus::from_str`/`as_str`
//! and `From<Tenant> for TenantResponse`) were byte-identical to these, so
//! every existing construction and field access keeps compiling unchanged.
//! `mokosh-types` additionally owns a `TenantUsage` DTO the SPA does not use;
//! it is intentionally not re-exported here (YAGNI).

// SPEC-3.21.1: re-exported, never redeclared, so drift fails the build.
pub use mokosh_types::tenants::{
    CreateTenantRequest, Tenant, TenantBranding, TenantResponse, TenantStatus, UpdateTenantRequest,
};
