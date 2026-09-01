//! Typed failures produced by Forge scheduling and maintenance workflows.

use thiserror::Error;
pub use vala_sql::row_types::forge_tasks::ForgeFailureClass;

/// Boundary at which a capacity refusal occurred.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ForgeCapacityFailurePhase {
    /// Live root capacity was occupied before the attempt began.
    Admission,
    /// An admitted attempt exhausted one persisted envelope term.
    Execution,
}

#[cfg(test)]
mod tests {
    use super::{ForgeCapacityFailurePhase, ForgeError, ForgeFailureClass};

    /// Typed execution errors map to the six durable failure classes without text parsing.
    #[test]
    fn failure_mapping_is_exhaustive_and_capacity_phase_aware() {
        assert_eq!(
            ForgeError::ScratchIo {
                kind: std::io::ErrorKind::PermissionDenied,
                detail: "scratch".to_owned(),
            }
            .failure_class(),
            ForgeFailureClass::StorageHealth
        );
        assert_eq!(
            ForgeError::Capacity {
                detail: "envelope".to_owned()
            }
            .failure_class(),
            ForgeFailureClass::CapacityRefused
        );
        assert_eq!(
            ForgeError::Capacity {
                detail: "occupied".to_owned()
            }
            .capacity_failure_phase(),
            Some(ForgeCapacityFailurePhase::Admission)
        );
        assert_eq!(
            ForgeError::ExecutionEnvelopeExceeded {
                resource: "memory",
                detail: "pool".to_owned()
            }
            .capacity_failure_phase(),
            Some(ForgeCapacityFailurePhase::Execution)
        );
        assert_eq!(
            ForgeError::Timeout {
                operation: "catalog"
            }
            .failure_class(),
            ForgeFailureClass::TransientObjectStore
        );
    }
}

/// Failures that preserve the durable boundary where Forge stopped.
#[derive(Debug, Error)]
pub enum ForgeError {
    /// A construction or runtime limit cannot safely execute Forge.
    #[error("invalid Forge configuration: {detail}")]
    InvalidConfig { detail: String },
    /// Pod-local elastic memory or scratch is temporarily occupied.
    #[error("Forge resources are temporarily unavailable: {detail}")]
    Capacity { detail: String },
    /// An admitted rewrite exhausted one exact persisted execution term.
    #[error("Forge execution envelope exceeded for {resource}: {detail}")]
    ExecutionEnvelopeExceeded {
        /// Closed resource label identifying the exhausted term family.
        resource: &'static str,
        /// Bounded diagnostic detail from the typed refusal boundary.
        detail: String,
    },
    /// A lease acquisition, renewal, fence, or release query failed.
    #[error("Forge lease query failed: {0}")]
    Lease(#[source] vala_sql::SqlError),
    /// A non-lease Forge SQL transition failed.
    #[error("Forge SQL operation failed: {0}")]
    Sql(#[source] vala_sql::SqlError),
    /// An Iceberg catalog, manifest, or transaction operation failed.
    #[error("Forge catalog operation failed: {0}")]
    Catalog(#[source] iceberg::Error),
    /// A staging or rewritten object operation failed.
    #[error("Forge staging object read failed: {0}")]
    ObjectStore(#[source] opendal::Error),
    /// Attempt-local scratch IO failed with its typed operating-system category.
    #[error("Forge scratch IO failed ({kind:?}): {detail}")]
    ScratchIo {
        /// Stable IO category consumed by durable failure classification.
        kind: std::io::ErrorKind,
        /// Path-safe diagnostic detail for tracing and operator evidence.
        detail: String,
    },
    /// Snapshot-expiry planning or reconciliation failed.
    #[error("Forge snapshot expiry failed: {detail}")]
    SnapshotExpiry { detail: String },
    /// The retained Iceberg live set could not be constructed safely.
    #[error("Forge live-set construction failed: {detail}")]
    LiveSet { detail: String },
    /// A table-owned object prefix could not be listed.
    #[error("Forge object listing failed: {0}")]
    ObjectList(#[source] opendal::Error),
    /// A fenced orphan object could not be deleted.
    #[error("Forge object deletion failed: {0}")]
    ObjectDelete(#[source] opendal::Error),
    /// A staging batch did not match the registered physical schema.
    #[error("Forge schema validation failed: {detail}")]
    Schema { detail: String },
    /// Durable candidate identity or values were invalid.
    #[error("Forge candidate group is invalid: {detail}")]
    Group { detail: String },
    /// Another owner acquired the table fence before this operation completed.
    #[error("Forge lost lease fence `{lease_key}`")]
    FenceLost { lease_key: String },
    /// Prepared or terminal durable state could not be reconciled safely.
    #[error("Forge reconciliation failed: {detail}")]
    Reconciliation { detail: String },
    /// Cancellation stopped work at a bounded stage or batch boundary *before*
    /// any durable side effect, so the claim is safe to release.
    ///
    /// This is the pre-effect shutdown marker. `run_slot`'s error path drains
    /// such a claim through `release_cancelled_claim`, whose SQL guard matches
    /// only `claimed`/`running` rows for the owner and attempt; a claim that has
    /// since advanced to `prepared` therefore no-matches and is retained anyway,
    /// so routing every plain `Shutdown` through release is safe.
    #[error("Forge scheduler was shut down")]
    Shutdown,
    /// Cancellation stopped work *after* a durable side effect committed
    /// (a fresh catalog commit or a recovered committed snapshot), so the claim
    /// is conservatively retained for evidence-based or lease-expiry recovery
    /// rather than released.
    ///
    /// This is the post-effect shutdown marker. It exists so `run_slot` can
    /// distinguish a committed-but-not-finalized claim, whose row is still
    /// `running` and would otherwise be matched and wrongly released by
    /// `release_cancelled_claim`, from the pre-effect [`ForgeError::Shutdown`]
    /// case. It carries the same internal control-flow meaning as `Shutdown`
    /// (never a public error) and only changes the retain-vs-release decision.
    #[error("Forge scheduler was shut down after a durable effect")]
    ShutdownRetained,
    /// A configured external-operation timeout elapsed.
    #[error("Forge {operation} timed out")]
    Timeout { operation: &'static str },
    /// An internal invariant failed before a durable transition could proceed.
    #[error("Forge invariant failed: {detail}")]
    Invariant { detail: String },
    /// An admitted managed rewrite failed after it may have produced objects.
    ///
    /// Wraps the typed failure rather than replacing it, so classification and
    /// the capacity phase stay exactly what the underlying boundary declared.
    /// What the wrapper adds is the attempt-global possible-output set: once any
    /// object may exist, discarding that set would leave objects nothing can
    /// name, because the attempt that could have named them has ended. A caller
    /// must treat an unsettled entry as possibly-existing and tolerate its
    /// absence when reclaiming it.
    #[error("{source} (leaving {} possible rewrite output(s) behind)", possible_outputs.len())]
    RewriteUnsettled {
        /// The typed failure that ended the attempt.
        #[source]
        source: Box<ForgeError>,
        /// Every object the failed attempt produced or may have produced.
        possible_outputs: Vec<crate::forge::managed::ForgeUnsettledOutput>,
    },
    /// A second long-lived scheduler attempted to use the same owner.
    #[error("Forge scheduler is already running")]
    AlreadyRunning,
}

impl ForgeError {
    /// Builds a grouping failure from a borrowed diagnostic.
    ///
    /// This exists so row-decoding helpers can be parameterized by the variant
    /// their caller must surface without duplicating the struct literal.
    #[must_use]
    pub fn group(detail: String) -> Self {
        Self::Group { detail }
    }

    /// Builds a reconciliation failure from a borrowed diagnostic.
    ///
    /// Companion to [`Self::group`] for recovery paths that must fail closed as
    /// a reconciliation mismatch rather than a planning-time grouping refusal.
    #[must_use]
    pub fn reconciliation(detail: String) -> Self {
        Self::Reconciliation { detail }
    }

    /// Classifies one execution failure without parsing diagnostic strings.
    #[must_use]
    pub fn failure_class(&self) -> ForgeFailureClass {
        match self {
            Self::RewriteUnsettled { source, .. } => source.failure_class(),
            Self::ScratchIo { .. } => ForgeFailureClass::StorageHealth,
            Self::Capacity { .. } | Self::ExecutionEnvelopeExceeded { .. } => {
                ForgeFailureClass::CapacityRefused
            }
            Self::ObjectStore(_)
            | Self::ObjectList(_)
            | Self::ObjectDelete(_)
            | Self::Catalog(_)
            | Self::Timeout { .. }
            | Self::SnapshotExpiry { .. }
            | Self::LiveSet { .. } => ForgeFailureClass::TransientObjectStore,
            Self::Lease(_)
            | Self::Sql(_)
            | Self::FenceLost { .. }
            | Self::Reconciliation { .. } => ForgeFailureClass::TransientCoordination,
            Self::Schema { .. }
            | Self::Group { .. }
            | Self::Invariant { .. }
            | Self::InvalidConfig { .. }
            | Self::AlreadyRunning
            | Self::Shutdown
            | Self::ShutdownRetained => ForgeFailureClass::InternalInvariant,
        }
    }

    /// Returns the capacity phase for typed capacity failures.
    ///
    /// [`Self::RewriteUnsettled`] delegates to the failure it wraps: the
    /// possible-output set is evidence carried alongside a refusal, never a
    /// refusal of its own, so wrapping must not move a capacity failure out of
    /// the phase its own boundary declared.
    #[must_use]
    pub fn capacity_failure_phase(&self) -> Option<ForgeCapacityFailurePhase> {
        match self {
            Self::Capacity { .. } => Some(ForgeCapacityFailurePhase::Admission),
            Self::ExecutionEnvelopeExceeded { .. } => Some(ForgeCapacityFailurePhase::Execution),
            Self::RewriteUnsettled { source, .. } => source.capacity_failure_phase(),
            _ => None,
        }
    }

    /// Returns the possible-output set a failed rewrite left behind, if any.
    ///
    /// Every other variant returns an empty slice, so a reclaiming caller can
    /// ask any Forge failure what it may have produced without matching on the
    /// wrapper first.
    #[must_use]
    pub fn possible_rewrite_outputs(&self) -> &[crate::forge::managed::ForgeUnsettledOutput] {
        match self {
            Self::RewriteUnsettled {
                possible_outputs, ..
            } => possible_outputs,
            _ => &[],
        }
    }
}
