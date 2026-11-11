//! SQL query modules for Wyrd-owned schemas.
//!
//! Tenant-scoped modules take [`crate::TenantConn`] so every query runs inside
//! the transaction that has `app.current_tenant` bound. Platform modules take a
//! platform executor explicitly: the tenant pool reaches the tenant directory
//! only through SECURITY DEFINER functions, while platform operations use the
//! platform operator pool. Query modules do not open nested transactions,
//! use savepoints, or coordinate transactions with Vala query modules; Vala
//! observes committed Wyrd state directly or through the future outbox path.

pub mod auth;
pub mod cards;
/// Code-review pipeline cache metadata: snapshot pointers, snapshot records and contexts.
pub mod code_review_cache;
pub mod drift_baselines;
pub mod gateway;
pub mod gateway_admission;
/// Tenant GitHub App installation associations in `wyrd.github_installations`.
pub mod github_installations;
pub mod operator_connections;
pub mod operator_dispatches;
pub mod platform;
pub mod storage;
pub mod verification;
pub mod verification_target;
pub mod verifier_executions;
