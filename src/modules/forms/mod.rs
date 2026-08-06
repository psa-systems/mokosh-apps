//! PMS-731 / PMS-730: client request forms.
//!
//! Definition DTOs for the admin builder. The public client-facing page
//! (`src/pages/request_form.rs`) keeps its own narrower shapes on purpose:
//! the server sends a deliberately reduced view to an unauthenticated
//! visitor (no ids, no author, no KB article), so sharing one struct would
//! mean optional fields that are always absent on one of the two surfaces.

pub mod models;

pub use models::*;
