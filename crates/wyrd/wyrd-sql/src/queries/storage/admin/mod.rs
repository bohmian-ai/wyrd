//! Admin-pool queries for storage background workers.
//!
//! These functions intentionally take [`sqlx::PgPool`] and run through the
//! platform-admin role instead of [`crate::TenantConn`].

pub mod idempotency;
pub mod multipart_uploads;
