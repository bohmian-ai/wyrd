//! Post-admission execution bindings shared by one query's planned leaves.
//!
//! A physical root is built before the query is admitted, so nothing a leaf
//! captures at planning time may name a runtime, a memory pool, a drained
//! batch, or a follower assignment. Each planning leaf instead retains one
//! closed [`OracleSourceKey`], and the single
//! `OnceLock<OracleExecutionBindings>` installed in the session configuration
//! before planning receives every concrete value exactly once after admission
//! and the audited source drain.
//!
//! The lock travels with the retained plan and with every `TaskContext` derived
//! from the same configuration, which is what lets planning observe an unbound
//! lock and execution observe the admitted one without rebuilding the root.

use std::collections::HashMap;
use std::sync::Arc;

use arrow::array::RecordBatch;
use chrono::{DateTime, Utc};
use tokio_util::sync::CancellationToken;
use wyrd_spec::vala::api::QueryClass;
use wyrd_spec::vala::error::BifrostError;

use super::{AccountedMemoryReservation, OracleMemoryResources, OracleTelemetry};

/// Closed identity of the one source a planning leaf reads.
///
/// This is everything a leaf may retain about its data before admission: which
/// per-table drained batch set it reads. It deliberately carries no batches,
/// assignment, pool, runtime, or class.
#[derive(Debug, Clone, PartialEq)]
pub(super) enum OracleSourceKey {
    /// The single Fused batch set drained for one canonical table.
    LocalDrained {
        /// Canonical fully-qualified table name whose batches this leaf reads.
        table: String,
    },
}

impl OracleSourceKey {
    /// Returns the canonical table name of a local-drained key.
    pub(super) fn local_table(&self) -> &str {
        match self {
            Self::LocalDrained { table } => table.as_str(),
        }
    }
}

/// The admitted execution governance every leaf resolves from its task.
///
/// Admission supplies the runtime and memory pool through the `TaskContext`
/// itself; this value carries the facts that are not representable there — the
/// root-derived class, the query's cancellation, its absolute deadline, and the
/// leader accounting handles a governed source reservation charges against.
#[derive(Clone)]
pub(super) struct OracleExecutionGrant {
    /// Class derived from the physical root and admitted under.
    pub(super) query_class: QueryClass,
    /// Query cancellation observed before any leaf opens a row source.
    pub(super) cancellation: CancellationToken,
    /// One absolute wall-clock deadline captured at ingress.
    pub(super) deadline: DateTime<Utc>,
    /// Pod allocator and reconciliation handles governed reservations charge.
    pub(super) memory: OracleMemoryResources,
    /// Canonical Oracle memory telemetry owner.
    pub(super) telemetry: Arc<OracleTelemetry>,
}

impl std::fmt::Debug for OracleExecutionGrant {
    /// Renders only the non-secret governance facts.
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("OracleExecutionGrant")
            .field("query_class", &self.query_class)
            .field("deadline", &self.deadline)
            .finish_non_exhaustive()
    }
}

impl OracleExecutionGrant {
    /// Refuses execution when this query can no longer legally read rows.
    ///
    /// Called by every governed leaf before its first byte of row IO, so a
    /// cancelled or expired query fails at the leaf rather than after the
    /// object is already resident.
    ///
    /// # Errors
    ///
    /// Returns a `DataFusion` execution error when the query was cancelled or
    /// its absolute deadline has elapsed.
    pub(super) fn ensure_live(&self) -> datafusion::error::Result<()> {
        if self.cancellation.is_cancelled() {
            return Err(datafusion::error::DataFusionError::Execution(
                "Oracle query was cancelled before row IO".to_owned(),
            ));
        }
        if self.deadline <= Utc::now() {
            return Err(datafusion::error::DataFusionError::Execution(
                "Oracle query deadline elapsed before row IO".to_owned(),
            ));
        }
        Ok(())
    }
}

/// Every concrete value the retained plan's leaves need, bound exactly once.
///
/// Built after admission and the audited drain, validated against the exact set
/// of keys the planned leaves retained, and then published through the
/// session's `OnceLock`. It also owns the drained tails' reservations for the
/// lifetime of the query, so the shallow batches a memory source projects are
/// charged once rather than twice.
pub(super) struct OracleExecutionBindings {
    /// Admitted governance shared by every governed leaf.
    grant: OracleExecutionGrant,
    /// Drained Fused batches keyed by canonical table name.
    local_batches: HashMap<String, Vec<RecordBatch>>,
    /// Reservations retaining those batches until the query settles.
    _reservations: Vec<AccountedMemoryReservation>,
    /// Whether one requested live source was unavailable at drain time.
    degraded: bool,
}

impl std::fmt::Debug for OracleExecutionBindings {
    /// Renders bound cardinality without rendering tenant rows.
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("OracleExecutionBindings")
            .field("local_tables", &self.local_batches.len())
            .field("degraded", &self.degraded)
            .finish_non_exhaustive()
    }
}

/// Complete inputs for binding one admitted query's retained plan.
pub(super) struct OracleExecutionBindingInputs {
    /// Admitted governance derived from the root's class and the query owner.
    pub(super) grant: OracleExecutionGrant,
    /// Drained Fused batches keyed by canonical table name.
    pub(super) local_batches: HashMap<String, Vec<RecordBatch>>,
    /// Reservations retaining those batches until the query settles.
    pub(super) reservations: Vec<AccountedMemoryReservation>,
    /// Whether one requested live source was unavailable at drain time.
    pub(super) degraded: bool,
}

impl OracleExecutionBindings {
    /// Validates and builds the one binding set for a retained plan.
    ///
    /// `planned` is the exact key set the retained leaves carry. Every planned
    /// key must have exactly one binding and no binding may exist for a key no
    /// leaf planned, because either direction means the plan and the admitted
    /// sources describe different reads.
    ///
    /// # Errors
    ///
    /// Returns [`BifrostError::QueryExecutionFailed`] when a planned key is
    /// unbound or when a binding names a key no leaf planned.
    pub(super) fn try_new(
        inputs: OracleExecutionBindingInputs,
        planned: &[OracleSourceKey],
    ) -> Result<Self, BifrostError> {
        let OracleExecutionBindingInputs {
            grant,
            local_batches,
            reservations,
            degraded,
        } = inputs;
        for key in planned {
            if !local_batches.contains_key(key.local_table()) {
                return Err(BifrostError::QueryExecutionFailed);
            }
        }
        if local_batches
            .keys()
            .any(|table| !planned.iter().any(|key| key.local_table() == table))
        {
            return Err(BifrostError::QueryExecutionFailed);
        }
        Ok(Self {
            grant,
            local_batches,
            _reservations: reservations,
            degraded,
        })
    }

    /// Returns the admitted governance shared by every governed leaf.
    pub(super) const fn grant(&self) -> &OracleExecutionGrant {
        &self.grant
    }

    /// Returns whether one requested live source was unavailable at drain time.
    pub(super) const fn degraded(&self) -> bool {
        self.degraded
    }

    /// Resolves the drained batches one local leaf reads.
    ///
    /// # Errors
    ///
    /// Returns a `DataFusion` execution error when the key was never bound,
    /// which is the same refusal a mismatched binding produces.
    pub(super) fn local_batches(
        &self,
        key: &OracleSourceKey,
    ) -> datafusion::error::Result<&[RecordBatch]> {
        self.local_batches
            .get(key.local_table())
            .map(Vec::as_slice)
            .ok_or_else(|| {
                datafusion::error::DataFusionError::Execution(
                    "Oracle plan leaf has no bound local source".to_owned(),
                )
            })
    }
}

/// The session-configuration extension type every leaf resolves through.
pub(super) type OracleExecutionLock = std::sync::OnceLock<OracleExecutionBindings>;

/// Resolves the once-bound execution bindings from an executing task.
///
/// Planning shares the same configuration and therefore the same lock, but
/// observes it unbound, which is what keeps planning free of row IO.
///
/// # Errors
///
/// Returns a `DataFusion` execution error when the extension is absent, which
/// means the leaf is executing outside the session it was planned in, or when
/// the lock has not been bound, which means execution started before admission
/// published its sources.
pub(super) fn bindings_for_task(
    task: &datafusion::execution::TaskContext,
) -> datafusion::error::Result<Arc<OracleExecutionLock>> {
    let lock = task
        .session_config()
        .get_extension::<OracleExecutionLock>()
        .ok_or_else(|| {
            datafusion::error::DataFusionError::Execution(
                "Oracle execution bindings are absent from the task session".to_owned(),
            )
        })?;
    if lock.get().is_none() {
        return Err(datafusion::error::DataFusionError::Execution(
            "Oracle execution bindings are not bound".to_owned(),
        ));
    }
    Ok(lock)
}

/// Builds one bound task context for tests that execute a governed leaf.
///
/// Production binds through admission; a leaf-level test still has to publish a
/// grant, because a `Leader` leaf resolves its class, cancellation, deadline,
/// and accounting handles from the task it is executed with rather than from
/// anything it captured at planning time.
#[cfg(test)]
pub(super) fn bind_test_session(
    config: datafusion::prelude::SessionConfig,
    memory_pool: Arc<dyn datafusion::execution::memory_pool::MemoryPool>,
    grant: OracleExecutionGrant,
    local_batches: HashMap<String, Vec<RecordBatch>>,
) -> Arc<datafusion::execution::TaskContext> {
    let lock = Arc::new(OracleExecutionLock::new());
    assert!(
        lock.set(OracleExecutionBindings {
            grant,
            local_batches,
            _reservations: Vec::new(),
            degraded: false,
        })
        .is_ok(),
        "a fresh execution lock binds exactly once"
    );
    let runtime = Arc::new(
        datafusion::execution::runtime_env::RuntimeEnvBuilder::new()
            .with_memory_pool(memory_pool)
            .build()
            .expect("a runtime environment builds from a memory pool"),
    );
    datafusion::prelude::SessionContext::new_with_config_rt(config.with_extension(lock), runtime)
        .task_ctx()
}

#[cfg(test)]
impl OracleExecutionGrant {
    /// Builds one live grant for a leaf-level test.
    ///
    /// The deadline is far enough out that a governed leaf's liveness check
    /// never fires incidentally; a test that needs expiry constructs its own.
    pub(super) fn for_test(
        query_class: QueryClass,
        memory: OracleMemoryResources,
        telemetry: Arc<OracleTelemetry>,
    ) -> Self {
        Self {
            query_class,
            cancellation: CancellationToken::new(),
            deadline: Utc::now() + chrono::Duration::hours(1),
            memory,
            telemetry,
        }
    }
}
