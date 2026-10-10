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
pub mod drift_baselines;
pub mod gateway;
pub mod operator_connections;
pub mod operator_dispatches;
pub mod platform;
pub mod storage;
pub mod verification;
pub mod verifier_runs;
