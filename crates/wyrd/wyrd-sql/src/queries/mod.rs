//! SQL query modules for Wyrd-owned schemas.
//!
//! Tenant-scoped modules take [`crate::TenantConn`] so every query runs inside
//! the transaction that has `app.current_tenant` bound. Platform modules take a
//! platform executor explicitly: runtime tenant resolution uses the `wyrd_app`
//! pool through the SECURITY DEFINER resolver, while platform-admin operations
//! use the platform-admin pool. Query modules do not open nested transactions,
//! use savepoints, or coordinate transactions with Vala query modules; Vala
//! observes committed Wyrd state directly or through the future outbox path.

pub mod auth;
pub mod cards;
pub mod drift_baselines;
pub mod gateway;
pub mod platform;
pub mod storage;
pub mod verification;
pub mod verifier_runs;
