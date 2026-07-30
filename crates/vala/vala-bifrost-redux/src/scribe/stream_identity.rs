//! Stream identity — `(node_id, writer_epoch)` acquisition and lifecycle.
//!
//! Each Scribe pod holds one `(node_id, writer_epoch)` pair for its lifetime.
//! `writer_epoch` is obtained by bumping `vala.cluster_nodes.fencing_token` on
//! boot. Every `file_list` row is stamped with the producing stream identity.
//! LSNs are meaningful only within one stream — never across pods or epochs.

use uuid::Uuid;
use vala_sql::OperatorPool;

use crate::contracts::ScribeError;

/// Stable pod identifier (UUID).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NodeId([u8; 16]);

impl NodeId {
    /// Construct a `NodeId` from a UUID.
    #[must_use]
    pub fn new(uuid: Uuid) -> Self {
        Self(*uuid.as_bytes())
    }

    /// Generate a new random `NodeId`.
    #[must_use]
    pub fn generate() -> Self {
        Self::new(Uuid::new_v4())
    }

    /// Get the raw bytes.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 16] {
        &self.0
    }

    /// Convert to a UUID.
    #[must_use]
    pub fn as_uuid(&self) -> Uuid {
        Uuid::from_bytes(self.0)
    }
}

impl std::fmt::Display for NodeId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_uuid())
    }
}

/// Writer epoch — monotonic per-boot counter from `vala.cluster_nodes.fencing_token`.
///
/// Each boot bumps the fencing token by 1. Every new epoch begins a fresh LSN
/// stream at LSN 0.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct WriterEpoch(i64);

impl WriterEpoch {
    /// Construct a `WriterEpoch` from a raw i64.
    #[must_use]
    pub const fn new(value: i64) -> Self {
        Self(value)
    }

    /// Get the raw i64 value.
    #[must_use]
    pub const fn as_i64(self) -> i64 {
        self.0
    }
}

impl std::fmt::Display for WriterEpoch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Stream identity — `(node_id, writer_epoch)` pair identifying one WAL stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct StreamIdentity {
    /// Stable pod identifier.
    pub node_id: NodeId,
    /// Writer epoch from `vala.cluster_nodes.fencing_token`.
    pub writer_epoch: WriterEpoch,
}

impl StreamIdentity {
    /// Construct a new stream identity.
    #[must_use]
    pub const fn new(node_id: NodeId, writer_epoch: WriterEpoch) -> Self {
        Self {
            node_id,
            writer_epoch,
        }
    }
}

impl std::fmt::Display for StreamIdentity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}", self.node_id, self.writer_epoch)
    }
}

/// Acquire stream identity on boot by bumping `vala.cluster_nodes.fencing_token`.
///
/// Executes one transaction via `OperatorPool`:
/// - INSERT with `fencing_token = 1` if the row is missing
/// - UPDATE `fencing_token = fencing_token + 1` if the row exists
/// - Returns the new fencing token as `WriterEpoch`
///
/// # Errors
/// Returns [`ScribeError::Internal`] if the database transaction fails.
pub async fn acquire_on_boot(
    pool: &OperatorPool,
    node_id: NodeId,
    role: &str,
    advertise_addr: &str,
) -> Result<StreamIdentity, ScribeError> {
    let node_uuid = node_id.as_uuid();

    // INSERT if not exists
    sqlx::query(
 "INSERT INTO vala.cluster_nodes (node_id, role, advertise_addr, fencing_token, started_at, heartbeat_at)
 VALUES ($1, $2, $3, 1, now(), now())
 ON CONFLICT (node_id, role) DO NOTHING",
 )
 .bind(node_uuid)
 .bind(role)
 .bind(advertise_addr)
 .execute(pool.pool())
 .await
 .map_err(|e| ScribeError::Internal {
 detail: format!("failed to insert cluster_nodes row: {e}"),
 })?;

    // UPDATE and return the new fencing_token
    let row: (i64,) = sqlx::query_as(
        "UPDATE vala.cluster_nodes
 SET fencing_token = fencing_token + 1,
 started_at = now(),
 heartbeat_at = now()
 WHERE node_id = $1
 RETURNING fencing_token",
    )
    .bind(node_uuid)
    .fetch_one(pool.pool())
    .await
    .map_err(|e| ScribeError::Internal {
        detail: format!("failed to bump fencing_token: {e}"),
    })?;

    let writer_epoch = WriterEpoch::new(row.0);

    Ok(StreamIdentity::new(node_id, writer_epoch))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn node_id_roundtrip() {
        let uuid = Uuid::new_v4();
        let node_id = NodeId::new(uuid);
        assert_eq!(node_id.as_uuid(), uuid);
    }

    #[test]
    fn writer_epoch_ordering() {
        assert!(WriterEpoch::new(2) > WriterEpoch::new(1));
    }

    #[test]
    fn stream_identity_display() {
        let node_id = NodeId::new(Uuid::new_v4());
        let epoch = WriterEpoch::new(42);
        let identity = StreamIdentity::new(node_id, epoch);
        let display = identity.to_string();
        assert!(display.contains("42"));
    }
}
