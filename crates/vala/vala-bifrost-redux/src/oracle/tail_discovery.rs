//! Authenticated query-scoped discovery of active Scribe live streams.
//!
//! Oracle lists each pinned table's reported streams on its frozen Scribe
//! roster before planning; the routes name exact Scribe incarnations, writer
//! epochs, and the capture each Scribe retains for the query until released
//! or expired.

use wyrd_spec::vala::api::{TailStreamIdentity, TenantTableBinding, TimePartitionWire};

use super::*;

/// One query-scoped live-tail route discovered from an authoritative Scribe.
pub struct DiscoveredTailRoute {
    /// Exact time partition retained by the Scribe stream.
    pub time_partition: TimePartitionWire,
    /// Exact node and writer epoch returned by discovery.
    pub stream: TailStreamIdentity,
    /// Live bytes the listing's scope reads in this partition.
    pub bytes: u64,
    /// Capture the reporting Scribe retains for this query; fragments read
    /// only this capture.
    pub capture_id: uuid::Uuid,
}

/// One table's query-scoped discovery: every reported route plus the
/// publication revision each answering Scribe reported in the same capture.
#[derive(Default)]
pub struct DiscoveredTail {
    /// Routes of every retained capture.
    pub routes: Vec<DiscoveredTailRoute>,
    /// Each answering Scribe's node and the table revision its last locally
    /// completed publication committed at, an empty capture included.
    ///
    /// Oracle compares each with the cut's revision for that node: a higher
    /// reported revision means the cut lacks rows that Scribe no longer
    /// serves live.
    pub revisions: Vec<(uuid::Uuid, i64)>,
}

/// Query-scoped resolver for live Scribe streams.
#[async_trait::async_trait]
pub trait TailStreamDiscovery: Send + Sync {
    /// Lists active streams for one binding on exactly the frozen Scribes,
    /// each with the bytes `scope` reads there, and every answering Scribe's
    /// publication revision.
    ///
    /// `scribes` is the query attempt's frozen roster; discovery never rereads
    /// membership, so every route names a participant of that one cut. Each
    /// listing retains a capture bound to `query_id` that expires at
    /// `deadline`. When any listing fails, the captures the others retained
    /// are released before the error returns.
    ///
    /// # Errors
    /// Returns the listing's [`TailReadError`] class:
    /// [`TailReadError::Unavailable`] only when a Scribe cannot be reached or
    /// refuses as unavailable, [`TailReadError::StaleIdentity`] when a listed
    /// stream names another incarnation or epoch than the frozen participant,
    /// [`TailReadError::DeadlineElapsed`] at the deadline, and the fatal
    /// [`TailReadError::Authorization`], [`TailReadError::Binding`], or
    /// [`TailReadError::State`] class for credential, tenant, binding, or
    /// malformed-response failures.
    async fn discover(
        &self,
        binding: &TenantTableBinding,
        scope: &crate::scan_sizing::ScanScope,
        scribes: &[OracleQueryParticipant],
        query_id: uuid::Uuid,
        deadline: Instant,
    ) -> Result<DiscoveredTail, TailReadError>;

    /// Releases captures this discovery retained for `query_id`, waiting no
    /// later than the query's `deadline`.
    ///
    /// Best effort: a capture that cannot be released, or whose Scribe has not
    /// answered by `deadline`, expires there on its Scribe, so a failure is
    /// logged and never returned and a stalled peer cannot keep the caller
    /// waiting past the query.
    async fn release(
        &self,
        binding: &TenantTableBinding,
        query_id: uuid::Uuid,
        captures: &[(OracleQueryParticipant, uuid::Uuid)],
        deadline: Instant,
    );

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
            .strip_prefix("bifrost.")
            .ok_or(BifrostError::QueryVisibilityUnavailable)?
            .to_owned(),
        table: cut.binding.table_name.clone(),
    })
}
