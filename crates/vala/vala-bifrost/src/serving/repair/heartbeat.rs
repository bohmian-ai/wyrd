//! Maintenance worker lease heartbeat.
//!
//! Workers that hold a maintenance lease (compaction, snapshot expiry, orphan
//! GC) must renew their lease periodically to prove liveness. A renewal
//! failure means the fencing token was lost — another pod claimed the work —
//! and the worker must stop immediately (fail-closed).
//!
//! [`LeaseHeartbeat::renew`] performs a fenced UPDATE on the maintenance lease
//! row: it succeeds only when the caller still owns the token. On ownership
//! loss it returns `false` so the caller can cancel its in-flight operation.

use sqlx::types::Uuid;
use vala_sql::SqlError;

/// Handle for renewing a named maintenance lease.
#[derive(Debug, Clone)]
pub struct LeaseHeartbeat {
    /// Stable worker identity (one UUID per process, never reused).
    pub owner: Uuid,
    /// The fencing token stamped when this worker claimed the lease.
    pub fencing_token: i64,
    /// Lease key identifying which maintenance concern this covers.
    pub lease_key: String,
}

impl LeaseHeartbeat {
    /// Construct a heartbeat handle for an already-claimed lease.
    #[must_use]
    pub fn new(owner: Uuid, fencing_token: i64, lease_key: impl Into<String>) -> Self {
        Self {
            owner,
            fencing_token,
            lease_key: lease_key.into(),
        }
    }

    /// Extend the lease by `lease_secs` seconds, fenced by `(owner, fencing_token)`.
    ///
    /// Returns `true` when the renewal applied (this worker still owns the
    /// lease). Returns `false` when ownership was lost — the caller MUST stop
    /// its in-flight operation immediately.
    ///
    /// # Errors
    /// Returns [`SqlError`] when the database query fails (distinct from
    /// ownership loss: a query error does not confirm or deny ownership).
    pub async fn renew(&self, pool: &sqlx::PgPool, lease_secs: i64) -> Result<bool, SqlError> {
        vala_sql::queries::maintenance_leases::renew_lease_fenced(
            pool,
            &self.lease_key,
            self.owner,
            self.fencing_token,
            lease_secs,
        )
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn heartbeat_stores_identity() {
        let owner = Uuid::new_v4();
        let hb = LeaseHeartbeat::new(owner, 42, "compaction:table:abc");
        assert_eq!(hb.owner, owner);
        assert_eq!(hb.fencing_token, 42);
        assert_eq!(hb.lease_key, "compaction:table:abc");
    }

    #[test]
    fn lease_key_into_string() {
        let hb = LeaseHeartbeat::new(Uuid::nil(), 1, "snapshot_expiry:global");
        assert_eq!(hb.lease_key, "snapshot_expiry:global");
    }
}
