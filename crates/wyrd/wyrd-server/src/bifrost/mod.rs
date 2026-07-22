//! Bifrost OLAP catalog HTTP surface: RBAC-gated register/list/describe routes
//! over the shared `AppState.bifrost` handle, plus the `DataTypeSpec ↔ arrow`
//! conversion and schema fingerprinting the register path needs.

pub mod convert;
pub mod routes;
pub mod scribe_adapter;
pub mod service;
