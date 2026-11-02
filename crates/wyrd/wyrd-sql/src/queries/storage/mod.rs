//! Storage-foundation SQL queries.
//!
//! Tenant-scoped modules take [`crate::TenantConn`]. Cross-tenant sweeper
//! operations are isolated under [`admin`] and take an admin [`sqlx::PgPool`].

pub mod admin;
pub mod idempotency;
pub mod multipart_uploads;
