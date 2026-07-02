//! Vala query HTTP surface: the pre-DataFusion SQL safety floor, the RBAC-gated
//! sync (raw Arrow IPC) and async (validate-before-enqueue) query service
//! functions, and their axum adapters.
//!
//! Sits next to `bifrost` and `storage`: it reuses the same `Caller`, RBAC
//! `AppState`, `WyrdCatalog` handle, and single `WyrdErrorResponse`. Providers are
//! built per request and scoped to the caller's tenant — never a shared
//! `SessionContext` — so cross-tenant reads are impossible before planning.

pub mod floor;
pub mod routes;
pub mod service;
