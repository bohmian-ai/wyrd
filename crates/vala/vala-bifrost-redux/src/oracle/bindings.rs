//! Post-admission execution bindings shared by one query's planned leaves.
//!
//! A physical root is built before the query is admitted, so nothing a leaf
//! captures at planning time may name a runtime, a memory pool, or a
//! dispatcher. The single `OnceLock<OracleExecutionBindings>` installed in the
//! session configuration before planning receives every concrete value exactly
//! once after admission and the audited read decision.
//!
//! The lock travels with the retained plan and with every `TaskContext` derived
//! from the same configuration, which is what lets planning observe an unbound
//! lock and execution observe the admitted one without rebuilding the root.

use std::sync::Arc;

use chrono::{DateTime, Utc};
use datafusion::error::{DataFusionError, Result as DataFusionResult};
use tokio_util::sync::CancellationToken;
use wyrd_spec::vala::api::QueryClass;
use wyrd_spec::vala::error::BifrostError;

use super::live::LiveDispatch;
use super::{DegradedSourceAccumulator, QueryCatalogError};

/// The admitted execution governance every leaf resolves from its task.
///
/// Admission supplies the runtime and memory pool through the `TaskContext`
/// itself; this value carries the facts that are not representable there — the
/// root-derived class, the query's cancellation, and its absolute deadline.
#[derive(Clone)]
pub(super) struct OracleExecutionGrant {
    /// Class derived from the physical root and admitted under.
    pub(super) query_class: QueryClass,
    /// Query cancellation observed before any leaf opens a row source.
    pub(super) cancellation: CancellationToken,
    /// One absolute wall-clock deadline captured at ingress.
    pub(super) deadline: DateTime<Utc>,
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
    /// Returns a `DataFusion` execution error when the query was cancelled, and
    /// a typed [`BifrostError::QueryTimeout`] chain when its absolute deadline
    /// has elapsed.
    pub(super) fn ensure_live(&self) -> datafusion::error::Result<()> {
        if self.cancellation.is_cancelled() {
            return Err(datafusion::error::DataFusionError::Execution(
                "Oracle query was cancelled before row IO".to_owned(),
            ));
        }
        if self.deadline <= Utc::now() {
            return Err(DataFusionError::Context(
                "Oracle query deadline elapsed before row IO".to_owned(),
                Box::new(QueryCatalogError::external(BifrostError::QueryTimeout)),
            ));
        }
        Ok(())
    }
}

/// Every concrete value the retained plan's leaves need, bound exactly once.
///
/// Built after admission and the audited read decision, then published through
/// the session's `OnceLock`.
pub(super) struct OracleExecutionBindings {
    /// Admitted governance shared by every governed leaf.
    grant: OracleExecutionGrant,
    /// Admitted dispatch capability live Scribe leaves open fragments through.
    live: Option<LiveDispatch>,
    /// Known live sources lost before their first row, read at the terminal.
    degraded: DegradedSourceAccumulator,
}

impl std::fmt::Debug for OracleExecutionBindings {
    /// Renders bound cardinality without rendering tenant rows.
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("OracleExecutionBindings")
            .field("live", &self.live)
            .finish_non_exhaustive()
    }
}

/// Complete inputs for binding one admitted query's retained plan.
pub(super) struct OracleExecutionBindingInputs {
    /// Admitted governance derived from the root's class and the query owner.
    pub(super) grant: OracleExecutionGrant,
    /// Admitted dispatch capability, present when live routes were planned.
    pub(super) live: Option<LiveDispatch>,
    /// Accumulator the terminal reads, already holding any listing loss.
    pub(super) degraded: DegradedSourceAccumulator,
}

impl OracleExecutionBindings {
    /// Builds the one binding set for a retained plan.
    #[must_use]
    pub(super) fn new(inputs: OracleExecutionBindingInputs) -> Self {
        let OracleExecutionBindingInputs {
            grant,
            live,
            degraded,
        } = inputs;
        Self {
            grant,
            live,
            degraded,
        }
    }

    /// Returns the admitted governance shared by every governed leaf.
    pub(super) const fn grant(&self) -> &OracleExecutionGrant {
        &self.grant
    }

    /// Returns the accumulator a live leaf records a pre-row source loss on.
    pub(super) const fn degraded(&self) -> &DegradedSourceAccumulator {
        &self.degraded
    }

    /// Returns the admitted dispatch capability a live leaf opens fragments through.
    ///
    /// # Errors
    ///
    /// Returns a `DataFusion` execution error when the query planned a live
    /// leaf but bound no dispatcher, which means plan and bindings disagree.
    pub(super) fn live(&self) -> DataFusionResult<&LiveDispatch> {
        self.live.as_ref().ok_or_else(|| {
            DataFusionError::Execution("Oracle plan leaf has no bound live dispatch".to_owned())
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
) -> Arc<datafusion::execution::TaskContext> {
    let runtime = Arc::new(
        datafusion::execution::runtime_env::RuntimeEnvBuilder::new()
            .with_memory_pool(memory_pool)
            .build()
            .expect("a runtime environment builds from a memory pool"),
    );
    datafusion::prelude::SessionContext::new_with_config_rt(with_test_grant(config, grant), runtime)
        .task_ctx()
}

/// Returns `config` bound with `grant` as an admitted query whose plan
/// delegated no source, the way admission binds a leader-local plan.
///
/// # Panics
///
/// Panics when the fresh lock refuses its first binding, which cannot happen.
#[cfg(any(test, all(feature = "test-support", feature = "bench-support")))]
fn with_test_grant(
    config: datafusion::prelude::SessionConfig,
    grant: OracleExecutionGrant,
) -> datafusion::prelude::SessionConfig {
    let lock = Arc::new(OracleExecutionLock::new());
    assert!(
        lock.set(OracleExecutionBindings {
            grant,
            live: None,
            degraded: DegradedSourceAccumulator::default(),
        })
        .is_ok(),
        "a fresh execution lock binds exactly once"
    );
    config.with_extension(lock)
}

/// Returns a production Oracle session configuration for local files, bound
/// with a live Interactive grant, so a benchmark session executes
/// leader-local Oracle leaves without admission.
///
/// The shape is the one a production leader derives: one partition per core
/// for wholly local data ([`crate::resources::oracle_target_partitions`] at
/// locality 1.0), with Oracle's fixed Parquet read options.
///
/// # Panics
///
/// Panics when the partition count overflows, which a host's core count
/// cannot cause.
#[cfg(all(feature = "test-support", feature = "bench-support"))]
pub(crate) fn bench_session_config() -> datafusion::prelude::SessionConfig {
    let cores = std::thread::available_parallelism().map_or(1, std::num::NonZeroUsize::get);
    let partitions = crate::resources::oracle_target_partitions(cores, 1.0)
        .expect("a host's core count yields a partition count");
    with_test_grant(
        crate::resources::OracleSessionShape::new(partitions).session_config(),
        OracleExecutionGrant::for_test(QueryClass::Interactive),
    )
}

#[cfg(any(test, all(feature = "test-support", feature = "bench-support")))]
impl OracleExecutionGrant {
    /// Builds one live grant for a leaf-level test.
    ///
    /// The deadline is far enough out that a governed leaf's liveness check
    /// never fires incidentally; a test that needs expiry constructs its own.
    pub(super) fn for_test(query_class: QueryClass) -> Self {
        Self {
            query_class,
            cancellation: CancellationToken::new(),
            deadline: Utc::now() + chrono::Duration::hours(1),
        }
    }
}
