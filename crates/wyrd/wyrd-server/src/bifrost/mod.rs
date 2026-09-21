//! Bifrost OLAP catalog HTTP surface: RBAC-gated register/list/describe routes
//! over the shared `AppState.bifrost` handle, plus the `DataTypeSpec ↔ arrow`
//! conversion and schema fingerprinting the register path needs.

pub mod convert;
/// Canonical audit sink for Bifrost gate admission decisions.
pub mod gate_audit;
pub mod routes;
pub mod service;
