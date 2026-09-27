//! Authenticated query-scoped discovery of active Scribe live streams.
//!
//! Oracle lists each pinned table's reported streams before planning; the
//! routes name exact Scribe incarnations and writer epochs and retain no rows.

use wyrd_spec::vala::api::{TailStreamIdentity, TenantTableBinding, TimePartitionWire};

use super::*;

/// One query-scoped live-tail route discovered from an authoritative Scribe.
pub struct DiscoveredTailRoute {
    /// Exact time partition retained by the Scribe stream.
    pub time_partition: TimePartitionWire,
    /// Exact node and writer epoch returned by discovery.
    pub stream: TailStreamIdentity,
}

/// Query-scoped resolver for live Scribe streams.
#[async_trait::async_trait]
pub trait TailStreamDiscovery: Send + Sync {
    /// Refreshes authoritative membership and lists active streams for one binding.
    ///
    /// # Errors
    /// Returns the listing's [`TailReadError`] class:
    /// [`TailReadError::Unavailable`] only when a ready Scribe cannot be
    /// reached or refuses as unavailable, [`TailReadError::StaleIdentity`] when
    /// a listed stream names another incarnation or epoch,
    /// [`TailReadError::DeadlineElapsed`] at the deadline, and the fatal
    /// [`TailReadError::Authorization`], [`TailReadError::Binding`], or
    /// [`TailReadError::State`] class for membership, ticket, credential,
    /// tenant, binding, or malformed-response failures.
    async fn discover(
        &self,
        binding: &TenantTableBinding,
        query_id: uuid::Uuid,
        deadline: Instant,
    ) -> Result<Vec<DiscoveredTailRoute>, TailReadError>;

    /// Toggles a test-tier discovery outage without changing production behavior.
    #[cfg(feature = "test-support")]
    fn set_unavailable_for_test(&self, _unavailable: bool) {}
}

/// Converts a pinned catalog binding to the private discovery wire shape.
///
/// # Errors
/// Returns visibility unavailable when the catalog namespace is malformed.
pub(super) fn wire_binding(cut: &PinnedSealedTable) -> Result<TenantTableBinding, BifrostError> {
    Ok(TenantTableBinding {
        tenant_id: cut.binding.tenant,
        namespace: cut
            .binding
            .logical_namespace
            .strip_prefix("vala.")
            .ok_or(BifrostError::QueryVisibilityUnavailable)?
            .to_owned(),
        table: cut.binding.table_name.clone(),
    })
}
