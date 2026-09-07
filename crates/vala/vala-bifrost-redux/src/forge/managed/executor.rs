//! One attempt's planning and per-plan execution against the managed core.
//!
//! An attempt is planned once and executed many times. [`ForgeManagedRewrite`]
//! owns the single [`ManagedExecutionContext`] every plan of that attempt
//! shares — the attempt identity, the shared cancellation token, one observer,
//! and therefore the one [`AttemptLedger`] that numbers every object the
//! attempt opens. Planning enumerates the complete real plan set and estimates
//! each plan before anything is admitted, so admission decides against real
//! figures rather than a synthetic envelope.
//!
//! Execution is deliberately unbounded: the context is built with no memory
//! pool and no spill lease, which selects `DataFusion`'s unbounded pool and no
redacted
//! makes the queue's estimated-memory admission the only thing standing between
//! a worker and over-commitment. A hard pool here would turn an admitted plan
//! into a mid-write failure instead of a plan that was never admitted.

use std::sync::Arc;

use iceberg::Catalog;
use iceberg::spec::Schema;
use iceberg::table::Table;
use iceberg_compaction_core::compaction::CompactionPlan;
use iceberg_compaction_core::managed::{
    AttemptId, ManagedExecutionContext, NonCommittingCompaction,
};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::catalog::TenantTableBinding;
use crate::forge::error::ForgeError;
use crate::forge::{Forge, ForgeCore};

use super::fingerprint::{
    ForgeRewriteEvidence, ForgeUnsettledOutput, debt_fingerprint, policy_fingerprint,
    selection_fingerprint,
};
use super::handoff::RewriteHandoff;
use super::identity::ForgeOutputIdentity;
use super::memory::estimate_plan_memory;
use super::observer::ForgeRewriteObserver;
use super::policy::ForgeTablePolicy;

/// One planned rewrite and the exact admission terms it was estimated at.
///
/// The estimate is computed once, here, from the plan the core produced and the
/// table's own schema and format version. Recomputing it at admission time
/// would let a plan be admitted against different figures from the ones it was
/// planned with.
#[derive(Debug)]
pub struct ForgePlannedRewrite {
    /// Planner ordinal, which is also this plan's queue key component.
    pub plan_index: usize,
    /// The core's plan, executed verbatim by exactly one runner.
    pub plan: CompactionPlan,
    /// Execution parallelism the plan recommends.
    pub required_parallelism: u32,
redacted
    pub memory_reservation_bytes: usize,
}

/// The complete result of planning one attempt, before any admission.
///
/// Planning has no output writes, no SQL or audit mutation, no catalog commit,
/// and no queue offer. An empty `plans` set is a legitimate outcome — the table
/// carries no compaction debt — and is settled as an audited no-op rather than
/// as a failure.
#[derive(Debug)]
pub struct ForgePlannedAttempt {
    /// The table snapshot every plan was produced against.
    pub table: Table,
    /// Evidence fingerprints derived from the core's selection report.
    pub evidence: ForgeRewriteEvidence,
    /// Every real plan the core produced, in planner order.
    pub plans: Vec<ForgePlannedRewrite>,
}

impl Forge {
    /// Binds one attempt's shared context and managed core seam.
    ///
    /// Nothing here reads an object or a manifest, so a failure at this
    /// boundary is provably free of side effects.
    ///
    /// # Errors
    ///
    /// Returns [`ForgeError::Invariant`] when the managed execution context
    /// cannot be built from the attempt's identity, cancellation token, and
    /// observer.
    pub fn managed_rewrite(
        &self,
        binding: &TenantTableBinding,
        task_id: Uuid,
        attempt_id: Uuid,
        cancel: CancellationToken,
    ) -> Result<ForgeManagedRewrite, ForgeError> {
        ForgeManagedRewrite::new(Arc::clone(&self.core), binding, task_id, attempt_id, cancel)
    }
}

/// One attempt's shared execution context, observer, and managed core seam.
///
/// Shared behind an `Arc` by every plan runner the attempt admits, which is
/// what makes the attempt — not the plan — the unit that owns cancellation,
/// observation, and the output ledger. A per-plan context would restart the
/// ledger's ordinals for every plan, so two objects from one attempt could
/// carry the same ordinal and a failure could name only the last plan's
/// objects.
pub struct ForgeManagedRewrite {
    /// Shared Forge dependency graph.
    core: Arc<ForgeCore>,
    /// Physical and logical identity of the table being rewritten.
    binding: TenantTableBinding,
    /// Durable task this attempt belongs to.
    task_id: Uuid,
    /// Identity every object this attempt writes is named for.
    attempt_id: Uuid,
    /// Observer accumulating the attempt's possibly-produced objects.
    observer: Arc<ForgeRewriteObserver>,
    /// The one context every plan of this attempt executes under.
    context: Arc<ManagedExecutionContext>,
}

impl std::fmt::Debug for ForgeManagedRewrite {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ForgeManagedRewrite")
            .field("task_id", &self.task_id)
            .field("attempt_id", &self.attempt_id)
            .finish_non_exhaustive()
    }
}

impl ForgeManagedRewrite {
    /// Creates the attempt-shared observer and execution context.
    ///
    /// # Errors
    ///
    /// Returns [`ForgeError::Invariant`] when the core refuses the leased
    /// terms, which can only happen if the runtime itself cannot be built.
    fn new(
        core: Arc<ForgeCore>,
        binding: &TenantTableBinding,
        task_id: Uuid,
        attempt_id: Uuid,
        cancel: CancellationToken,
    ) -> Result<Self, ForgeError> {
        let observer = Arc::new(ForgeRewriteObserver::new());
        let context = unbounded_context_for(attempt_id, &cancel, &observer)?;
        Ok(Self {
            core,
            binding: binding.clone(),
            task_id,
            attempt_id,
            observer,
            context,
        })
    }

    /// Returns the durable task this attempt belongs to.
    pub fn task_id(&self) -> Uuid {
        self.task_id
    }

    /// Returns the attempt identity every produced object is named for.
    pub fn attempt_id(&self) -> Uuid {
        self.attempt_id
    }

    /// Returns the one context every plan of this attempt shares.
    pub(crate) fn context(&self) -> &Arc<ManagedExecutionContext> {
        &self.context
    }

    /// Loads the table, enumerates every real plan, and estimates each one.
    ///
    /// The core's selection limit is set to `usize::MAX` so its default cap
    /// cannot silently truncate the plan set: deferral is the queue's job, and
    /// a truncated plan set would make the selection report describe work the
    /// plans do not contain. No output is written, no operation row is created,
    /// and no plan is offered to the queue.
    ///
    /// # Errors
    ///
    /// Returns [`ForgeError::InvalidConfig`] when the table's declared bloom
    /// columns or geometry are unusable, [`ForgeError::Catalog`] when the table
    /// or its manifests cannot be read, and [`ForgeError::Invariant`] when a
    /// plan recommends an execution parallelism outside `u32`.
    pub async fn plan(&self) -> Result<ForgePlannedAttempt, ForgeError> {
        let table_ident = self.binding.table_ident();
        let table = self
            .core
            .catalog
            .load_table(&table_ident)
            .await
            .map_err(ForgeError::Catalog)?;
        let bloom_columns = crate::catalog::layout::PhysicalLayout::bloom_columns_from_property(
            table
                .metadata()
                .properties()
                .get(crate::catalog::layout::BLOOM_COLUMNS_PROPERTY),
        )
        .map_err(|detail| ForgeError::InvalidConfig { detail })?;
        let policy = ForgeTablePolicy::extract(table.metadata(), &self.core.config)?;
        let config = policy.to_core_config(
            self.attempt_id.to_string(),
            &bloom_columns,
            self.core.config.max_concurrent_reads,
        )?;
        let compaction = NonCommittingCompaction::new(
            Arc::clone(&self.core.catalog) as Arc<dyn Catalog>,
            table_ident,
            Arc::clone(&config),
            Arc::clone(&self.context),
        );
        let (plans, report) = compaction.plan_with_report().await.map_err(|error| {
            ForgeError::Catalog(iceberg::Error::new(
                iceberg::ErrorKind::Unexpected,
                format!("Forge managed planning failed: {error}"),
            ))
        })?;
        let evidence = ForgeRewriteEvidence {
            base_snapshot_id: report.base_snapshot_id,
            selection_fingerprint: selection_fingerprint(&report),
            debt_fingerprint: debt_fingerprint(
                &report,
                total_position_deletes(&plans),
                total_equality_deletes(&plans),
            ),
            policy_fingerprint: policy_fingerprint(&report),
        };
        let schema: &Schema = table.metadata().current_schema();
        let format_version = table.metadata().format_version();
        let requires_sort = policy.sort_order_id != 0;
        let planned = plans
            .into_iter()
            .enumerate()
            .map(|(plan_index, plan)| {
                let memory_reservation_bytes = estimate_plan_memory(
                    &plan,
                    schema,
                    format_version,
                    config.execution.max_record_batch_rows,
                    config.execution.enable_prefetch,
                    requires_sort,
                );
                let required_parallelism = u32::try_from(
                    plan.recommended_executor_parallelism().max(1),
                )
                .map_err(|_| ForgeError::Invariant {
                    detail: format!(
                        "plan {plan_index} recommends an execution parallelism \
                                 outside the admissible range"
                    ),
                })?;
                Ok(ForgePlannedRewrite {
                    plan_index,
                    plan,
                    required_parallelism,
                    memory_reservation_bytes,
                })
            })
            .collect::<Result<Vec<_>, ForgeError>>()?;
        Ok(ForgePlannedAttempt {
            table,
            evidence,
            plans: planned,
        })
    }

    /// Rewrites exactly one admitted plan and publishes nothing.
    ///
    /// The returned handoff names objects that exist in storage and belong to
    /// no snapshot, derived from this plan's own inputs and this rewrite's own
    /// outputs. Sibling plans contribute nothing to it: each plan is published
    /// independently, so a combined handoff would ask publication to remove
    /// inputs a different plan is still rewriting.
    ///
    /// # Errors
    ///
    /// Returns [`ForgeError::Shutdown`] when the attempt was cancelled before
    /// the core finished, [`ForgeError::Catalog`] for a core execution failure,
    /// and [`ForgeError::Invariant`] when the core produced an object this
    /// attempt cannot attribute to itself or a handoff that contradicts itself.
    /// Any of those raised after an object may exist arrives wrapped in
    /// [`ForgeError::RewriteUnsettled`], carrying the attempt-global
    /// possible-output set so nothing becomes unreclaimable.
    pub async fn rewrite_plan(
        &self,
        planned: ForgePlannedRewrite,
        table: &Table,
    ) -> Result<RewriteHandoff, ForgeError> {
        let table_ident = self.binding.table_ident();
        let bloom_columns = crate::catalog::layout::PhysicalLayout::bloom_columns_from_property(
            table
                .metadata()
                .properties()
                .get(crate::catalog::layout::BLOOM_COLUMNS_PROPERTY),
        )
        .map_err(|detail| ForgeError::InvalidConfig { detail })?;
        let policy = ForgeTablePolicy::extract(table.metadata(), &self.core.config)?;
        let config = policy.to_core_config(
            self.attempt_id.to_string(),
            &bloom_columns,
            self.core.config.max_concurrent_reads,
        )?;
        let compaction = NonCommittingCompaction::new(
            Arc::clone(&self.core.catalog) as Arc<dyn Catalog>,
            table_ident,
            config,
            Arc::clone(&self.context),
        );
        let base_snapshot_id = table
            .metadata()
            .snapshot_for_ref(crate::forge::scribe_promotion::PROMOTION_BRANCH)
            .map(|snapshot| snapshot.snapshot_id())
            .ok_or_else(|| ForgeError::Invariant {
                detail: format!(
                    "table {} has no {} snapshot to rewrite",
                    self.binding.table_ident(),
                    crate::forge::scribe_promotion::PROMOTION_BRANCH
                ),
            })?;
        let mut rewritten_data_files = Vec::new();
        let mut position_deletes = std::collections::BTreeSet::new();
        let mut equality_deletes = std::collections::BTreeSet::new();
        record_consumed(
            &planned.plan,
            &mut rewritten_data_files,
            &mut position_deletes,
            &mut equality_deletes,
        );
        let result = match compaction.rewrite(planned.plan, table).await {
            Ok(result) => result,
            Err(error) => return Err(self.classify_core_failure(&error)),
        };
        let mut output_data_files = Vec::new();
        let mut writer_keys = std::collections::HashSet::new();
        for file in result.output_data_files {
            let identity = ForgeOutputIdentity::validate(
                file.file_path(),
                &policy.data_location,
                self.attempt_id,
            )
            .map_err(|error| self.attach_possible_outputs(error))?;
            if !writer_keys.insert(identity.writer_key()) {
                return Err(self.attach_possible_outputs(ForgeError::Invariant {
                    detail: format!(
                        "Forge attempt produced two objects with the same writer identity: {}",
                        file.file_path()
                    ),
                }));
            }
            output_data_files.push(file);
        }
        RewriteHandoff::try_new(
            base_snapshot_id,
            rewritten_data_files,
            position_deletes.into_iter().collect(),
            equality_deletes.into_iter().collect(),
            output_data_files,
        )
        .map_err(|error| self.attach_possible_outputs(error))
    }

    /// Classifies one core failure for a single plan runner.
    ///
    /// A cancelled attempt is not an error of this plan — it did what it was
    /// told — so it becomes [`ForgeError::Shutdown`], which the worker reduces
    /// as a cooperative, non-consuming outcome. Everything else is a real
    /// failure, carried out with the attempt-global possible-output set when
    /// any object may already exist.
    fn classify_core_failure(
        &self,
        error: &iceberg_compaction_core::error::CompactionError,
    ) -> ForgeError {
        if self.context.is_cancelled() {
            return self.attach_possible_outputs(ForgeError::Shutdown);
        }
        self.attach_possible_outputs(ForgeError::Catalog(iceberg::Error::new(
            iceberg::ErrorKind::Unexpected,
            format!("Forge managed rewrite failed: {error}"),
        )))
    }

    /// Projects the attempt-global accumulator onto public evidence.
    ///
    /// Read from the one observer the attempt installed, so the set spans every
    /// plan the attempt executed rather than the plan that happened to fail.
    /// Callers snapshot it only after every runner has drained, because a
    /// running sibling can still add to it.
    pub fn possible_outputs(&self) -> Vec<ForgeUnsettledOutput> {
        self.observer
            .outputs()
            .into_iter()
            .map(|output| ForgeUnsettledOutput {
                logical_ordinal: output.logical_ordinal,
                path: output.path,
                settled: output.settled,
            })
            .collect()
    }

    /// Attaches the attempt-global possible-output set to a failure.
    ///
    /// Returns `failure` unchanged when nothing can have been produced, so
    /// [`ForgeError::RewriteUnsettled`] only ever appears when it carries
    /// objects a caller has to reclaim.
    fn attach_possible_outputs(&self, failure: ForgeError) -> ForgeError {
        let possible_outputs = self.possible_outputs();
        if possible_outputs.is_empty() {
            return failure;
        }
        ForgeError::RewriteUnsettled {
            source: Box::new(failure),
            possible_outputs,
        }
    }
}

/// Builds the attempt's single unbounded managed execution context.
///
/// No memory pool and no spill lease are supplied, which is the whole point:
/// the core's builder then selects `DataFusion`'s unbounded pool and leaves the
/// runtime without a disk manager, so an admitted plan executes exactly as
redacted
/// inside its budget.
///
/// # Errors
///
/// Returns [`ForgeError::Invariant`] when the core cannot build the runtime.
fn unbounded_context_for(
    attempt_id: Uuid,
    cancel: &CancellationToken,
    observer: &Arc<ForgeRewriteObserver>,
) -> Result<Arc<ManagedExecutionContext>, ForgeError> {
    ManagedExecutionContext::builder()
        .with_attempt_id(AttemptId::from_uuid(attempt_id))
        .with_cancellation(cancel.clone())
        .with_observer(Arc::clone(observer) as Arc<_>)
        .build()
        .map_err(|error| ForgeError::Invariant {
            detail: format!("Forge managed execution context is unusable: {error}"),
        })
}

/// Appends one plan's consumed live paths to the handoff's accumulators.
///
/// Data files are accumulated in order because a live data file belongs to
/// exactly one group; a repeat would be a real contradiction and the handoff
/// refuses it. Delete files are accumulated as sets because one delete file
/// legitimately covers several groups — an equality delete scoped to a
/// partition applies to every data file in it — and naming it once per group
/// would describe the same applied delete more than once.
fn record_consumed(
    plan: &CompactionPlan,
    data: &mut Vec<String>,
    position_deletes: &mut std::collections::BTreeSet<String>,
    equality_deletes: &mut std::collections::BTreeSet<String>,
) {
    data.extend(
        plan.file_group
            .data_files
            .iter()
            .map(|task| task.data_file_path.clone()),
    );
    position_deletes.extend(
        plan.file_group
            .position_delete_files
            .iter()
            .map(|task| task.data_file_path.clone()),
    );
    equality_deletes.extend(
        plan.file_group
            .equality_delete_files
            .iter()
            .map(|task| task.data_file_path.clone()),
    );
}

/// Counts the position-delete files the whole selection covers.
fn total_position_deletes(plans: &[CompactionPlan]) -> usize {
    plans
        .iter()
        .map(|plan| plan.file_group.position_delete_files.len())
        .sum()
}

/// Counts the equality-delete files the whole selection covers.
fn total_equality_deletes(plans: &[CompactionPlan]) -> usize {
    plans
        .iter()
        .map(|plan| plan.file_group.equality_delete_files.len())
        .sum()
}

#[cfg(test)]
mod tests {
    use super::unbounded_context_for;
    use std::sync::Arc;
    use tokio_util::sync::CancellationToken;
    use uuid::Uuid;

    /// The attempt's context executes unbounded and spills nowhere.
    ///
    /// Forge admits plans by estimate, so the executing runtime must not also
    /// impose a hard ceiling: a bounded pool would convert an admitted plan
    /// into a mid-write resource failure, and a disk manager would reintroduce
    /// the scratch dependency this design removed. Both facts are read from the
    /// runtime the core actually built.
    ///
    /// # Panics
    ///
    /// Panics when the context cannot be built, when its pool reports a bound,
    /// or when its runtime has a temp-directory limit.
    #[test]
    fn unbounded_context_has_no_disk_manager() {
        let context = unbounded_context_for(
            Uuid::new_v4(),
            &CancellationToken::new(),
            &Arc::new(super::ForgeRewriteObserver::new()),
        )
        .expect("the unbounded managed context builds");
        let runtime = context.runtime_env();
        assert!(
            matches!(
                runtime.memory_pool.memory_limit(),
                datafusion::execution::memory_pool::MemoryLimit::Infinite
            ),
            "an admitted plan executes against DataFusion's unbounded pool"
        );
        assert!(
            runtime.disk_manager.temp_dir_paths().is_empty(),
            "no spill directory is registered, so no Forge scratch is required"
        );
        assert_eq!(
            context.pool_capacity_bytes(),
            None,
            "the context leases no bounded pool capacity"
        );
        assert!(
            context.spill().is_none(),
            "the context leases no scratch root"
        );
    }
}
