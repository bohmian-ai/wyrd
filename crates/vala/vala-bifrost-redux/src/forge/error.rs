//! Typed failures produced by Forge scheduling and maintenance workflows.

use thiserror::Error;

/// Closed durable class driving Forge retry, quarantine, and terminal policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ForgeFailureClass {
    /// Deterministic input shape cannot safely execute.
    DataRefusal,
    /// Remote object or catalog storage may recover after backoff.
    TransientObjectStore,
    /// Pod-local scratch health is unsafe and requires quarantine.
    StorageHealth,
    /// The live governor cannot currently fit the persisted envelope.
    CapacityRefused,
}

impl ForgeFailureClass {
    /// Returns the closed SQL and telemetry spelling.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::DataRefusal => "data_refusal",
            Self::TransientObjectStore => "transient_object_store",
            Self::StorageHealth => "storage_health",
            Self::CapacityRefused => "capacity_refused",
        }
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
    /// Deterministic source metadata proves the input cannot fit the decoded allowance.
    #[error("Forge refused unsafe input data: {detail}")]
    DataRefusal { detail: String },
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
    /// Parquet decoding, encoding, or owned spill-path setup failed.
    #[error("Forge parquet operation failed: {detail}")]
    Parquet { detail: String },
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
    /// A second long-lived scheduler attempted to use the same owner.
    #[error("Forge scheduler is already running")]
    AlreadyRunning,
    /// `DataFusion` failed while executing the spillable physical rewrite.
    #[error("Forge DataFusion execution failed: {0}")]
    DataFusion(#[source] datafusion::error::DataFusionError),
    /// `DataFusion` exhausted the configured operation spill ceiling.
    #[error("Forge spill limit of {limit_bytes} bytes was exceeded")]
    SpillLimitExceeded { limit_bytes: u64 },
}

impl ForgeError {
    /// Classifies one execution failure without parsing diagnostic strings.
    #[must_use]
    pub const fn failure_class(&self) -> ForgeFailureClass {
        match self {
            Self::DataRefusal { .. } => ForgeFailureClass::DataRefusal,
            Self::ScratchIo { .. } => ForgeFailureClass::StorageHealth,
            Self::Capacity { .. } => ForgeFailureClass::CapacityRefused,
            Self::ObjectStore(_)
            | Self::ObjectList(_)
            | Self::ObjectDelete(_)
            | Self::Catalog(_)
            | Self::Timeout { .. }
            | Self::Lease(_)
            | Self::Sql(_)
            | Self::SnapshotExpiry { .. }
            | Self::LiveSet { .. }
            | Self::Parquet { .. }
            | Self::Schema { .. }
            | Self::Group { .. }
            | Self::FenceLost { .. }
            | Self::Reconciliation { .. }
            | Self::Shutdown
            | Self::ShutdownRetained
            | Self::Invariant { .. }
            | Self::InvalidConfig { .. }
            | Self::AlreadyRunning
            | Self::DataFusion(_)
            | Self::SpillLimitExceeded { .. } => ForgeFailureClass::TransientObjectStore,
        }
    }
}
