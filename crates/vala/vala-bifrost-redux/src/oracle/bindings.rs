//! Post-admission execution bindings shared by one query's planned leaves.
//!
//! A physical root is built before the query is admitted, so nothing a leaf
//! captures at planning time may name a runtime, a memory pool, a dispatcher,
//! or a follower assignment. Each remote planning leaf instead retains one
//! [`FollowerSourceKey`], and the single `OnceLock<OracleExecutionBindings>`
//! installed in the session configuration before planning receives every
//! concrete value exactly once after admission and the audited read decision.
//!
//! The lock travels with the retained plan and with every `TaskContext` derived
//! from the same configuration, which is what lets planning observe an unbound
//! lock and execution observe the admitted one without rebuilding the root.

use std::collections::HashMap;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use datafusion::error::{DataFusionError, Result as DataFusionResult};
use tokio_util::sync::CancellationToken;
use wyrd_spec::vala::api::QueryClass;
use wyrd_spec::vala::error::BifrostError;

use super::live::LiveDispatch;
use super::{DegradedSourceAccumulator, OracleMemoryResources, OracleTelemetry};

/// Complete planned authority of one physical remote-scan occurrence.
///
/// Every fact here is fixed while the plan is built, before the query is
/// admitted, and every one of them is compared against the assignment bound
/// after admission. Carrying them together is what makes a repeated occurrence
/// of the same table and tier — a self-join, a repeated CTE — bind to its own
/// projection closure instead of silently inheriting a sibling's.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct FollowerSourceKey {
    /// Request-local scan identity, unique per physical scan occurrence.
    pub(super) scan_id: String,
    /// Frozen peer endpoint, node identity, role, and role fence.
    pub(super) destination: super::dispatcher::DispatchCandidate,
    /// Authenticated data tenant this occurrence was planned for.
    pub(super) tenant: wyrd_spec::DataTenantId,
    /// Canonical fully-qualified table name of the pinned cut it reads.
    pub(super) table: String,
    /// The one persisted tier of that cut this occurrence delegates.
    pub(super) tier: super::RemotePersistedTier,
    /// Fingerprint of the table's complete physical schema.
    pub(super) schema_fingerprint: String,
    /// Closed projection closure, in signed order.
    pub(super) required_columns: Vec<String>,
    /// Closed leaf predicates, in filter order.
    pub(super) predicates: Vec<wyrd_spec::vala::assignment_authority::ScanPredicate>,
}

impl FollowerSourceKey {
    /// Reports whether `assignment` carries exactly this occurrence's authority.
    ///
    /// The binder mints the assignment from this key's own cut and tier, so this
    /// is the check that the value published under a key never describes a
    /// different read from the one the plan retained. The tier is covered by the
    /// scan identity, which is minted from the table, the tier, and the
    /// occurrence at one site.
    pub(super) fn matches(
        &self,
        assignment: &wyrd_spec::vala::api::FollowerScanAssignment,
    ) -> bool {
        assignment.scan_id == self.scan_id
            && assignment.binding.tenant_id == self.tenant
            && format!(
                "vala.{}.{}",
                assignment.binding.namespace, assignment.binding.table
            ) == self.table
            && assignment.schema_fingerprint == self.schema_fingerprint
            && assignment.required_columns == self.required_columns
            && assignment.predicates == self.predicates
    }
}

impl std::hash::Hash for FollowerSourceKey {
    /// Hashes only the request-local scan identity every equal key shares.
    ///
    /// A scan identity already separates every distinct occurrence in one
    /// query, so hashing the remaining authority would cost a full closure walk
    /// per lookup without removing a collision. Equal keys still hash equally,
    /// which is the contract.
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.scan_id.hash(state);
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
/// Built after admission and the audited read decision, validated against the
/// exact set of keys the planned leaves retained, and then published through
/// the session's `OnceLock`.
pub(super) struct OracleExecutionBindings {
    /// Admitted governance shared by every governed leaf.
    grant: OracleExecutionGrant,
    /// Completed follower assignments keyed by their full planned occurrence.
    follower_assignments: HashMap<FollowerSourceKey, wyrd_spec::vala::api::FollowerScanAssignment>,
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
            .field("follower_assignments", &self.follower_assignments.len())
            .field("live", &self.live)
            .finish_non_exhaustive()
    }
}

/// Complete inputs for binding one admitted query's retained plan.
pub(super) struct OracleExecutionBindingInputs {
    /// Admitted governance derived from the root's class and the query owner.
    pub(super) grant: OracleExecutionGrant,
    /// Completed follower assignments keyed by their full planned occurrence.
    pub(super) follower_assignments:
        HashMap<FollowerSourceKey, wyrd_spec::vala::api::FollowerScanAssignment>,
    /// Admitted dispatch capability, present when live routes were planned.
    pub(super) live: Option<LiveDispatch>,
    /// Accumulator the terminal reads, already holding any listing loss.
    pub(super) degraded: DegradedSourceAccumulator,
}

impl OracleExecutionBindings {
    /// Validates and builds the one binding set for a retained plan.
    ///
    /// `planned` is the exact canonical key set the retained leaves carry, one
    /// entry per physical scan occurrence. Every planned key must have exactly
    /// one binding whose whole authority matches that key, and no binding may
    /// exist for a key no leaf planned, because either direction means the plan
    /// and the admitted sources describe different reads.
    ///
    /// # Errors
    ///
    /// Returns [`BifrostError::QueryExecutionFailed`] when a planned key is
    /// unbound, when `planned` repeats one canonical remote occurrence, when a
    /// binding names a key no leaf planned, or when a bound assignment's scan
    /// identity, tenant/table binding, schema fingerprint, projection closure,
    /// or predicates differ from the occurrence that planned it.
    pub(super) fn try_new(
        inputs: OracleExecutionBindingInputs,
        planned: &[FollowerSourceKey],
    ) -> Result<Self, BifrostError> {
        let OracleExecutionBindingInputs {
            grant,
            follower_assignments,
            live,
            degraded,
        } = inputs;
        if planned.iter().any(|key| {
            !follower_assignments
                .get(key)
                .is_some_and(|assignment| key.matches(assignment))
        }) {
            return Err(BifrostError::QueryExecutionFailed);
        }
        // Exact cardinality both ways. Every planned occurrence resolved above,
        // so equal counts leave no unplanned binding and no repeated canonical
        // occurrence — a duplicate would be counted twice against one entry.
        if planned.len() != follower_assignments.len() {
            return Err(BifrostError::QueryExecutionFailed);
        }
        Ok(Self {
            grant,
            follower_assignments,
            live,
            degraded,
        })
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

    /// Resolves the one completed assignment a remote leaf reads.
    ///
    /// The lookup is by the leaf's whole planned occurrence — scan identity,
    /// destination, tenant/table binding, tier, schema, and closure — so a leaf
    /// can only ever reach the assignment the binder validated against exactly
    /// that occurrence, never a same-table sibling's.
    ///
    /// # Errors
    ///
    /// Returns a `DataFusion` execution error when nothing was bound under the
    /// key.
    pub(super) fn follower_assignment(
        &self,
        key: &FollowerSourceKey,
    ) -> datafusion::error::Result<&wyrd_spec::vala::api::FollowerScanAssignment> {
        self.follower_assignments.get(key).ok_or_else(|| {
            datafusion::error::DataFusionError::Execution(
                "Oracle plan leaf has no bound follower assignment".to_owned(),
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
) -> Arc<datafusion::execution::TaskContext> {
    let lock = Arc::new(OracleExecutionLock::new());
    assert!(
        lock.set(OracleExecutionBindings {
            grant,
            follower_assignments: HashMap::new(),
            live: None,
            degraded: DegradedSourceAccumulator::default(),
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
