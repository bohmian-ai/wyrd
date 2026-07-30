//! Stream identity — `(node_id, writer_epoch)` acquisition and lifecycle.
//!
//! Each Scribe pod holds one `(node_id, writer_epoch)` pair for its lifetime.
//! `writer_epoch` is the independently fenced Scribe-role token allocated by
//! [`crate::cluster::ClusterRegistry`] on boot. Every `file_list` row is stamped with the producing stream identity.
//! LSNs are meaningful only within one stream — never across pods or epochs.

use uuid::Uuid;
use vala_sql::OperatorPool;
use wyrd_spec::vala::api::{NodeId as ClusterNodeId, ScribeCapabilitiesV1};

use crate::cluster::ClusterRegistry;
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
/// This compatibility constructor delegates the durable role lifecycle to
/// [`ClusterRegistry`]. Server boot owns the registry directly; this remains
/// only for focused legacy tests until their callers move to that owner.
///
/// # Errors
/// Returns [`ScribeError::Internal`] when the compatibility role is not
/// Scribe, membership registration fails, or its fence cannot fit the WAL
/// epoch representation.
#[deprecated(
    note = "server boot uses ClusterRegistry directly; retained only for legacy focused tests"
)]
pub async fn acquire_on_boot(
    pool: &OperatorPool,
    node_id: NodeId,
    role: &str,
    advertise_addr: &str,
) -> Result<StreamIdentity, ScribeError> {
    if role != "scribe" {
        return Err(ScribeError::Internal {
            detail: "stream identity exists only for the Scribe role".to_owned(),
        });
    }
    let registry = ClusterRegistry::new(pool.clone(), ClusterNodeId::new(node_id.as_uuid()));
    let registered = registry
        .register_scribe(
            advertise_addr,
            ScribeCapabilitiesV1 {
                tail_protocol_version: crate::scribe::tail_rpc::TAIL_PROTOCOL_VERSION,
            },
        )
        .await
        .map_err(|error| ScribeError::Internal {
            detail: error.to_string(),
        })?;
    let writer_epoch = WriterEpoch::new(
        i64::try_from(registered.fencing_token).map_err(|_| ScribeError::Internal {
            detail: "Scribe role fence exceeds the WAL epoch range".to_owned(),
        })?,
    );

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
