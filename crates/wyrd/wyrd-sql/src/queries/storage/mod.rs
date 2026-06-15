//! Storage-foundation SQL queries.
//!
//! Tenant-scoped modules take [`crate::TenantConn`]. Cross-tenant sweeper
//! operations are isolated under [`admin`] and take an admin [`sqlx::PgPool`].

pub mod access_ledger;
pub mod admin;
pub mod artifact_metadata;
pub mod idempotency;
pub mod multipart_uploads;
