//! One attempt's planning and per-plan execution against the managed core.
//!
//! An attempt is planned once and executed many times. [`ForgeManagedRewrite`]
//! owns the single [`ManagedExecutionContext`] every plan of that attempt
//! shares — the attempt identity, the shared cancellation token, one observer,
//! and therefore the one [`AttemptLedger`] that numbers every object the
//! attempt opens. Planning enumerates the complete real plan set before
//! anything is admitted.
//!
//! Execution runs against a `DataFusion` pool backed by the one shared Bifrost
//! governor and a spill lease over the data root's `forge-spill` directory.
//! Memory is charged as the rewrite's reservations actually grow, not from an
//! estimate; a refused growth spills or fails the attempt with a typed
//! resource error, and the attempt's durable task stays unsettled so a retry
//! replans without publishing anything partial.

use std::path::Path;
use std::sync::Arc;

use iceberg::Catalog;
use iceberg::table::Table;
use iceberg_compaction_core::compaction::CompactionPlan;
use iceberg_compaction_core::config::CompactionPlanningConfig;
use iceberg_compaction_core::managed::{
    AttemptId, ManagedExecutionContext, NonCommittingCompaction, SpillLease,
};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::catalog::TenantTableBinding;
use crate::forge::error::ForgeError;
use crate::forge::scribe_promotion::PROMOTION_BRANCH;
use crate::forge::settings::{ForgeCompactionType, is_copy_on_write};
use crate::forge::{Forge, ForgeCore};

use super::fingerprint::{
    ForgeRewriteEvidence, ForgeUnsettledOutput, debt_fingerprint, policy_fingerprint,
    selection_fingerprint,
};
use super::handoff::RewriteHandoff;
use super::identity::ForgeOutputIdentity;
use super::observer::ForgeRewriteObserver;
use super::policy::ForgeTablePolicy;

/// One planned rewrite and the parallelism it is admitted at.
#[derive(Debug)]
pub struct ForgePlannedRewrite {
    /// Planner ordinal, which is also this plan's queue key component.
    pub plan_index: usize,
    /// The core's plan, executed verbatim by exactly one runner.
    pub plan: CompactionPlan,
    /// Execution parallelism the plan recommends.
    pub required_parallelism: u32,
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
    /// Geometry resolved once for this attempt; every plan's execution,
    /// publication, and audit read it rather than re-resolving the table.
    pub policy: ForgeTablePolicy,
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
        cancel: &CancellationToken,
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
        cancel: &CancellationToken,
    ) -> Result<Self, ForgeError> {
        let observer = Arc::new(ForgeRewriteObserver::new());
        let context = governed_context_for(
            attempt_id,
            cancel,
            &observer,
            &core.resources,
            &core.spill_root,
        )?;
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

    /// Loads the table once and enumerates every real plan for `compaction_type`.
    ///
    /// The worker plans against the current head, as `RisingWave`'s compactor
    /// does after a pull (`iceberg_compactor_runner.rs:592-680` at e23ddf95):
    /// the task type selects `RisingWave`'s planning configuration, and a
    /// copy-on-write table plans `Full` over the whole table. No output is
    /// written, no operation row is created, and no plan is offered to the
    /// queue.
    ///
    /// # Errors
    ///
    /// Returns [`ForgeError::InvalidConfig`] when the table's declared
    /// geometry is unusable, [`ForgeError::Catalog`] when the table or its
    /// manifests cannot be read or the core refuses its configuration, and
    /// [`ForgeError::Invariant`] when the branch has no snapshot or a plan
    /// recommends an execution parallelism outside `u32`.
    pub async fn plan(
        &self,
        compaction_type: ForgeCompactionType,
    ) -> Result<ForgePlannedAttempt, ForgeError> {
        let table = self
            .core
            .catalog
            .load_table(&self.binding.table_ident())
            .await
            .map_err(ForgeError::Catalog)?;
        let policy = ForgeTablePolicy::extract(table.metadata(), &self.core.config)?;
        let copy_on_write = is_copy_on_write(table.metadata().properties());
        let (plans, report) = policy
            .planning(compaction_type, copy_on_write)
            .plan(&table, PROMOTION_BRANCH)
            .await?;
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
        let planned = plans
            .into_iter()
            .enumerate()
            .map(|(plan_index, plan)| {
                let required_parallelism = u32::try_from(plan.recommended_executor_parallelism())
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
                })
            })
            .collect::<Result<Vec<_>, ForgeError>>()?;
        Ok(ForgePlannedAttempt {
            table,
            evidence,
            plans: planned,
            policy,
        })
    }

    /// Rewrites exactly one admitted plan and publishes nothing.
    ///
    /// The returned handoff names objects that exist in storage and belong to
    /// no snapshot, derived from this plan's own inputs and this rewrite's own
    /// outputs. Sibling plans contribute nothing to it: each plan is published
    /// independently, so a combined handoff would ask publication to remove
    /// inputs a different plan is still rewriting. `policy` is the geometry
    /// [`Self::plan`] resolved for this attempt, so every plan writes under
    /// the same file target its publication and audit record.
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
        policy: &ForgeTablePolicy,
    ) -> Result<RewriteHandoff, ForgeError> {
        let table_ident = self.binding.table_ident();
        let bloom_columns = crate::catalog::layout::PhysicalLayout::bloom_columns_from_property(
            table
                .metadata()
                .properties()
                .get(crate::catalog::layout::BLOOM_COLUMNS_PROPERTY),
        )
        .map_err(|detail| ForgeError::InvalidConfig { detail })?;
        let config = policy.to_core_config(
            CompactionPlanningConfig::default(),
            self.attempt_id.to_string(),
            &bloom_columns,
            self.core.config.max_concurrent_reads,
            self.binding.tenant,
        )?;
        let compaction = NonCommittingCompaction::new(
            Arc::clone(&self.core.catalog) as Arc<dyn Catalog>,
            table_ident,
            config,
            Arc::clone(&self.context),
        );
        let base_snapshot_id = table
            .metadata()
            .snapshot_for_ref(PROMOTION_BRANCH)
            .map(|snapshot| snapshot.snapshot_id())
            .ok_or_else(|| ForgeError::Invariant {
                detail: format!(
                    "table {} has no {} snapshot to rewrite",
                    self.binding.table_ident(),
                    PROMOTION_BRANCH
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

/// Builds the attempt's single governed managed execution context.
///
/// The pool is a fresh view of the one shared Bifrost governor, so every
/// reservation the rewrite grows is charged to the same cap Scribe, Oracle,
/// and transport charge, and every byte returns when the attempt's context and
/// reservations drop. No pool capacity or scratch capacity is named: the
/// shared cap is the only memory bound, and `forge-spill` keeps `DataFusion`'s
/// own temp-directory limit. A fresh view per attempt means a failed attempt
/// releases everything before its retry builds the next one.
///
/// # Errors
///
/// Returns [`ForgeError::Invariant`] when the spill root is not an existing
/// directory or the core cannot build the runtime.
fn governed_context_for(
    attempt_id: Uuid,
    cancel: &CancellationToken,
    observer: &Arc<ForgeRewriteObserver>,
    resources: &crate::resources::ForgeResources,
    spill_root: &Path,
) -> Result<Arc<ManagedExecutionContext>, ForgeError> {
    let spill = SpillLease::new(spill_root).map_err(|error| ForgeError::Invariant {
        detail: format!("Forge spill root is unusable: {error}"),
    })?;
    ManagedExecutionContext::builder()
        .with_attempt_id(AttemptId::from_uuid(attempt_id))
        .with_cancellation(cancel.clone())
        .with_observer(Arc::clone(observer) as Arc<_>)
        .with_memory_pool(resources.rewrite_memory_pool(), None)
        .with_spill_lease(spill)
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
    use super::governed_context_for;
    use crate::resources::{
        BifrostRole, BifrostRuntimeResources, ScribeMemoryCategory, ScribeMemoryRequest,
    };
    use datafusion::execution::memory_pool::MemoryConsumer;
    use std::sync::Arc;
    use tokio_util::sync::CancellationToken;
    use uuid::Uuid;

    /// The attempt's pool charges the shared root and releases on cancel.
    ///
    /// The rewrite runtime's pool is a view of the one Bifrost governor shared
    /// with Scribe and Oracle: its growth appears as Forge-attributed governed
    /// memory beside a held Scribe charge, it is refused at the shared cap,
    /// cancellation alone frees nothing the reservation still holds, and
    /// dropping the reservation and context returns every Forge byte. A spill
    /// file lands in the leased root and is removed when the context drops.
    ///
    /// # Panics
    ///
    /// Panics when the context cannot be built, when growth is not charged to
    /// the shared root, when the cap does not refuse, or when cancellation and
    /// drop leave bytes behind.
    #[test]
    fn rewrite_pool_charges_root_and_releases_on_cancel() {
        const MIB: usize = 1024 * 1024;
        let roles = BifrostRuntimeResources::composed_for_test(
            64 * MIB,
            512 * MIB as u64,
            [BifrostRole::Forge, BifrostRole::Scribe, BifrostRole::Oracle],
        );
        let forge = roles.forge().expect("Forge capability");
        let scribe_charge = roles
            .scribe()
            .expect("Scribe capability")
            .try_acquire_memory(ScribeMemoryRequest {
                bytes: 8 * MIB,
                category: ScribeMemoryCategory::Raw,
                shard: Some(0),
            })
            .expect("Scribe charges the shared root");
        let spill = tempfile::tempdir().expect("spill root");
        let cancel = CancellationToken::new();
        let context = governed_context_for(
            Uuid::new_v4(),
            &cancel,
            &Arc::new(super::ForgeRewriteObserver::new()),
            &forge,
            spill.path(),
        )
        .expect("the governed managed context builds");
        let runtime = context.runtime_env();
        let spill_dirs = runtime.disk_manager.temp_dir_paths();
        assert!(
            !spill_dirs.is_empty() && spill_dirs.iter().all(|dir| dir.starts_with(spill.path())),
            "spills land beneath the leased forge-spill root, saw {spill_dirs:?}"
        );
        let spill_file = runtime
            .disk_manager
            .create_tmp_file("forge-rewrite-spill")
            .expect("the disk manager creates a spill file");
        assert!(
            spill_file
                .path()
                .is_some_and(|path| path.starts_with(spill.path())),
            "a spill file lands beneath the leased forge-spill root"
        );
        let reservation = MemoryConsumer::new("forge-rewrite").register(&runtime.memory_pool);
        reservation
            .try_grow(48 * MIB)
            .expect("growth under the cap");
        let charged = roles.snapshot().expect("charged snapshot");
        assert_eq!(charged.forge_memory_used_bytes, 48 * MIB);
        assert_eq!(charged.governed_memory_used_bytes, 56 * MIB);
        assert!(
            reservation.try_grow(16 * MIB).is_err(),
            "the shared cap, not a Forge budget, refuses growth"
        );
        cancel.cancel();
        assert!(context.is_cancelled());
        assert_eq!(
            roles
                .snapshot()
                .expect("cancelled snapshot")
                .forge_memory_used_bytes,
            48 * MIB,
            "cancellation alone releases nothing the reservation still holds"
        );
        drop(reservation);
        drop(spill_file);
        drop(runtime);
        drop(context);
        let released = roles.snapshot().expect("released snapshot");
        assert_eq!(released.forge_memory_used_bytes, 0);
        assert_eq!(released.governed_memory_used_bytes, 8 * MIB);
        assert_eq!(
            std::fs::read_dir(spill.path())
                .expect("the spill root is readable")
                .count(),
            0,
            "dropping the context removes every spill file and directory"
        );
        drop(scribe_charge);
        assert_eq!(
            roles
                .snapshot()
                .expect("idle snapshot")
                .governed_memory_used_bytes,
            0
        );
        assert!(roles.health().reason().is_none());
    }
}
