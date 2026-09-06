//! The one owner that turns an admitted attempt into candidate objects.
//!
//! Ordering is the whole design. Resources are leased *first*, before the
//! catalog is touched and long before any object is opened, so a refusal is a
//! refusal and not a partially-executed rewrite. The table is loaded once and
//! every later decision — policy, selection, grouping, writing — is made
//! against that one snapshot. The managed core owns all of it; this module
//! contributes no selector, no packer, no writer, and no commit.

use std::sync::Arc;

use iceberg::Catalog;
use iceberg_compaction_core::compaction::CompactionPlan;
use iceberg_compaction_core::managed::{
    AttemptId, ManagedExecutionContext, NonCommittingCompaction, SpillLease,
};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::catalog::TenantTableBinding;
use crate::forge::error::ForgeError;
use crate::forge::rewrite::ForgeAttemptResources;
use crate::forge::{Forge, ForgeCore};

use super::fingerprint::{
    ForgeRewriteEvidence, ForgeRewriteOutcome, ForgeUnsettledOutput, debt_fingerprint,
    policy_fingerprint, selection_fingerprint,
};
use super::handoff::RewriteHandoff;
use super::identity::ForgeOutputIdentity;
use super::observer::ForgeRewriteObserver;
use super::policy::ForgeTablePolicy;

/// Everything one managed rewrite attempt needs that is not a Forge dependency.
///
/// Every field is caller-owned on purpose. The identities come from the durable
/// claim so the objects an attempt writes are attributable to it without a
/// second registry; the resource request comes from planning estimates that
/// were already validated; the previous evidence comes from the last attempt on
/// this table; and the cancellation token is the caller's shutdown boundary,
/// which the core observes directly.
#[derive(Debug)]
pub struct ForgeRewriteAttempt<'evidence> {
    /// Durable task this attempt belongs to.
    pub task_id: Uuid,
    /// Identity every object this attempt writes is named for.
    pub attempt_id: Uuid,
    /// Exact validated resource demand to lease before any IO.
    pub request: crate::resources::ForgeRewriteRequest,
    /// Canonical Bloom column union the table registered.
    pub bloom_columns: &'evidence [String],
    /// Evidence the previous attempt on this table produced, when there was one.
    pub previous: Option<&'evidence ForgeRewriteEvidence>,
    /// Shutdown boundary the managed core observes while it executes.
    pub cancel: CancellationToken,
}

impl Forge {
    /// Runs exactly one managed rewrite attempt and publishes nothing.
    ///
    /// The attempt can end in three honest ways. It refuses without object IO
    /// when the table has not advanced past the previous attempt's base
    /// snapshot, or when its semantic debt is unchanged — both are
    /// [`ForgeRewriteOutcome::NoProgress`], because burning a resource lease to
    /// rewrite the same rows into differently named objects is not maintenance.
    /// It drains to [`ForgeRewriteOutcome::Cancelled`] when shutdown reaches it,
    /// reporting every object it may have produced so the caller can reclaim
    /// them. Otherwise it returns [`ForgeRewriteOutcome::Rewritten`]: objects
    /// that exist in storage, belong to no snapshot, and are described by an
    /// exact five-field handoff.
    ///
    /// # Errors
    ///
    /// Returns [`ForgeError::Capacity`] when the root governor refuses the
    /// exact request, [`ForgeError::InvalidConfig`] when the table's declared
    /// geometry is not executable, [`ForgeError::Catalog`] when the table or
    /// its manifests cannot be read, [`ForgeError::Invariant`] when the core
    /// produced an object this attempt cannot attribute to itself or a handoff
    /// that contradicts itself, and [`ForgeError::ExecutionEnvelopeExceeded`]
    /// when an admitted attempt exhausted its leased memory envelope. Any of those
    /// raised after the attempt may already have produced an object arrives
    /// wrapped in [`ForgeError::RewriteUnsettled`], which carries the complete
    /// attempt-global possible-output set so nothing becomes unreclaimable.
    ///
    /// # Panics
    ///
    /// Never: every fallible step returns an error, and the attempt's runtime
    /// accessors are used only while the attempt is unfinished.
    pub async fn execute_rewrite_attempt(
        &self,
        binding: &TenantTableBinding,
        attempt: ForgeRewriteAttempt<'_>,
    ) -> Result<ForgeRewriteOutcome, ForgeError> {
        ForgeManagedRewrite::new(Arc::clone(&self.core), binding, attempt)?
            .run()
            .await
    }
}

/// One attempt's leased resources, bound core, and derived policy.
///
/// Constructed only by [`Forge::execute_rewrite_attempt`], and only after the
/// lease is held: the constructor's first act is the acquisition, so a
/// `ForgeManagedRewrite` that exists is an attempt that was admitted.
struct ForgeManagedRewrite<'attempt> {
    /// Shared Forge dependency graph, borrowed for the attempt's lifetime.
    core: Arc<ForgeCore>,
    /// Physical and logical identity of the table being rewritten.
    binding: &'attempt TenantTableBinding,
    /// Caller-owned attempt terms.
    attempt: ForgeRewriteAttempt<'attempt>,
    /// Exact memory, scratch, reader, and runtime lease held for this attempt.
    resources: ForgeAttemptResources,
    /// Observer accumulating peaks and possibly-produced objects.
    observer: Arc<ForgeRewriteObserver>,
    /// Resources this attempt leased to the core for its whole lifetime.
    context: Arc<ManagedExecutionContext>,
}

impl<'attempt> ForgeManagedRewrite<'attempt> {
    /// Leases the attempt's resources, then binds them to a managed context.
    ///
    /// Nothing here reads an object or a manifest. That is the point: a
    /// refusal at this boundary is provably free of side effects, which is what
    /// lets the scheduler retry it against a different worker without first
    /// reconciling anything.
    ///
    /// # Errors
    ///
    /// Returns [`ForgeError::Capacity`] when the root governor refuses the
    /// request, [`ForgeError::ScratchIo`] when the attempt's scratch child
    /// cannot be created, and [`ForgeError::Invariant`] when the leased scratch
    /// root is not usable as a spill lease.
    fn new(
        core: Arc<ForgeCore>,
        binding: &'attempt TenantTableBinding,
        attempt: ForgeRewriteAttempt<'attempt>,
    ) -> Result<Self, ForgeError> {
        let resources = ForgeAttemptResources::acquire(
            &core.resources,
            attempt.request,
            binding,
            &core.rewrite_spill_root,
            attempt.task_id,
            attempt.attempt_id,
        )?;
        let observer = Arc::new(ForgeRewriteObserver::new());
        let context =
            managed_context_for(&resources, attempt.attempt_id, &attempt.cancel, &observer)?;
        Ok(Self {
            core,
            binding,
            attempt,
            resources,
            observer,
            context,
        })
    }

    /// Executes the admitted attempt against one loaded snapshot.
    ///
    /// # Errors
    ///
    /// See [`Forge::execute_rewrite_attempt`], whose error contract this
    /// method implements.
    async fn run(mut self) -> Result<ForgeRewriteOutcome, ForgeError> {
        let table_ident = self.binding.table_ident();
        let table = self
            .core
            .catalog
            .load_table(&table_ident)
            .await
            .map_err(ForgeError::Catalog)?;
        let base_snapshot_id = table
            .metadata()
            .snapshot_for_ref(crate::forge::scribe_promotion::PROMOTION_BRANCH)
            .map(|snapshot| snapshot.snapshot_id())
            .ok_or_else(|| ForgeError::Invariant {
                detail: format!(
                    "table {table_ident} has no {} snapshot to rewrite",
                    crate::forge::scribe_promotion::PROMOTION_BRANCH
                ),
            })?;
        if let Some(previous) = self.attempt.previous
            && previous.base_snapshot_id == base_snapshot_id
        {
            return Ok(ForgeRewriteOutcome::NoProgress {
                base_snapshot_id,
                debt_fingerprint: previous.debt_fingerprint.clone(),
            });
        }

        let policy = ForgeTablePolicy::extract(
            table.metadata(),
            &self.core.config,
            self.resources.memory_bytes(),
        )?;
        let config = policy.to_core_config(
            self.attempt.attempt_id.to_string(),
            self.attempt.bloom_columns,
            self.core.config.max_concurrent_reads,
            usize::try_from(self.resources.memory_bytes()).unwrap_or(usize::MAX),
            self.resources.spill_root().to_path_buf(),
        )?;
        let compaction = NonCommittingCompaction::new(
            Arc::clone(&self.core.catalog) as Arc<dyn Catalog>,
            table_ident,
            config,
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
        if plans.is_empty()
            || self
                .attempt
                .previous
                .is_some_and(|previous| previous.debt_fingerprint == evidence.debt_fingerprint)
        {
            return Ok(ForgeRewriteOutcome::NoProgress {
                base_snapshot_id: evidence.base_snapshot_id,
                debt_fingerprint: evidence.debt_fingerprint,
            });
        }

        self.execute_admitted_plans(&compaction, &table, &policy, evidence, plans)
            .await
    }

    /// Executes the admitted plans and assembles the publication-free handoff.
    ///
    /// The admitted set is exactly the plan set the core returned alongside its
    /// selection report; the per-attempt plan budget is applied inside the core
    /// so plans and report can never describe different selections. This owner
    /// never narrows that set.
    ///
    /// One plan at a time, in the order the core produced them: the plan's
    /// consumed inputs are recorded before the rewrite runs so a failure still
    /// names what was in flight, and every produced object is validated against
    /// the attempt's output identity. Peaks are folded on both exit paths.
    ///
    /// # Errors
    ///
    /// Returns [`ForgeError::Invariant`] when two objects share one writer
    /// identity, the classified drain or failure from [`Self::drain_or_fail`]
    /// when the core fails, and the handoff's own consistency error otherwise.
    /// Every failing path is routed through [`Self::attach_possible_outputs`],
    /// so a failure that follows a produced object carries the attempt-global
    /// set inside [`ForgeError::RewriteUnsettled`] rather than dropping it.
    async fn execute_admitted_plans(
        &mut self,
        compaction: &NonCommittingCompaction,
        table: &iceberg::table::Table,
        policy: &ForgeTablePolicy,
        evidence: ForgeRewriteEvidence,
        admitted: Vec<CompactionPlan>,
    ) -> Result<ForgeRewriteOutcome, ForgeError> {
        let mut rewritten_data_files = Vec::new();
        let mut applied_position_delete_files = std::collections::BTreeSet::new();
        let mut applied_equality_delete_files = std::collections::BTreeSet::new();
        let mut output_data_files = Vec::new();
        let mut writer_keys = std::collections::HashSet::new();
        for plan in admitted {
            record_consumed(
                &plan,
                &mut rewritten_data_files,
                &mut applied_position_delete_files,
                &mut applied_equality_delete_files,
            );
            let result = match compaction.rewrite(plan, table).await {
                Ok(result) => result,
                Err(error) => {
                    self.fold_peaks();
                    return self.drain_or_fail(evidence.base_snapshot_id, &error);
                }
            };
            for file in result.output_data_files {
                let identity = match ForgeOutputIdentity::validate(
                    file.file_path(),
                    &policy.data_location,
                    self.attempt.attempt_id,
                ) {
                    Ok(identity) => identity,
                    Err(error) => {
                        self.fold_peaks();
                        return Err(self.attach_possible_outputs(error));
                    }
                };
                if !writer_keys.insert(identity.writer_key()) {
                    self.fold_peaks();
                    return Err(self.attach_possible_outputs(ForgeError::Invariant {
                        detail: format!(
                            "Forge attempt produced two objects with the same writer identity: {}",
                            file.file_path()
                        ),
                    }));
                }
                output_data_files.push(file);
            }
        }
        self.fold_peaks();
        let handoff = RewriteHandoff::try_new(
            evidence.base_snapshot_id,
            rewritten_data_files,
            applied_position_delete_files.into_iter().collect(),
            applied_equality_delete_files.into_iter().collect(),
            output_data_files,
        )
        .map_err(|error| self.attach_possible_outputs(error))?;
        Ok(ForgeRewriteOutcome::Rewritten {
            evidence,
            handoff: Box::new(handoff),
        })
    }

    /// Records the core's own peak measurements against the attempt's lease.
    ///
    /// Called on every exit path, including the failing one: an attempt that
    /// exhausted its budget is exactly the attempt whose peaks an operator most
    /// wants to see.
    fn fold_peaks(&mut self) {
        self.resources
            .observe_memory_peak(self.observer.peak_memory_bytes());
        self.resources
            .observe_scratch_peak(self.observer.peak_scratch_bytes());
    }

    /// Classifies a core failure as a drain or a real failure.
    ///
    /// The attempt-global possible-output set is read once, before either
    /// branch, because both branches need it. A cancelled attempt is not an
    /// error — it did what it was told — so it returns that set as an outcome
    /// rather than a failure the scheduler would have to interpret. A real
    /// failure is still a failure, but once any object may exist the set
    /// travels with it inside [`ForgeError::RewriteUnsettled`]: the attempt
    /// that could name those objects is over, so discarding the set here is
    /// what would make them unreclaimable, not the failure itself. A failure
    /// with no possible outputs is returned bare, so the wrapper only ever
    /// appears when it carries something.
    ///
    /// # Errors
    ///
    /// Returns [`ForgeError::ExecutionEnvelopeExceeded`] when the attempt's own
    /// leased memory envelope caused the failure, and [`ForgeError::Catalog`] for every
    /// other core failure, whose cause is a manifest, scan, or writer
    /// operation. Either is wrapped in [`ForgeError::RewriteUnsettled`] when
    /// the attempt may already have produced an object.
    fn drain_or_fail(
        &self,
        base_snapshot_id: i64,
        error: &iceberg_compaction_core::error::CompactionError,
    ) -> Result<ForgeRewriteOutcome, ForgeError> {
        if self.attempt.cancel.is_cancelled() {
            return Ok(ForgeRewriteOutcome::Cancelled {
                base_snapshot_id,
                possible_outputs: self.possible_outputs(),
            });
        }
        let failure = if let Some(resource) =
            execution_envelope_resource(error, self.resources.memory_was_refused())
        {
            ForgeError::ExecutionEnvelopeExceeded {
                resource,
                detail: format!("Forge managed rewrite exhausted its {resource} lease: {error}"),
            }
        } else {
            ForgeError::Catalog(iceberg::Error::new(
                iceberg::ErrorKind::Unexpected,
                format!("Forge managed rewrite failed: {error}"),
            ))
        };
        Err(self.attach_possible_outputs(failure))
    }

    /// Projects the observer's attempt-global accumulator onto public evidence.
    ///
    /// Read from the one observer this attempt installed, so the set spans
    /// every plan the attempt executed rather than the plan that happened to
    /// fail.
    fn possible_outputs(&self) -> Vec<ForgeUnsettledOutput> {
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
    /// Returns `failure` unchanged when the attempt cannot have produced
    /// anything, so [`ForgeError::RewriteUnsettled`] only ever appears when it
    /// carries objects a caller has to reclaim.
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

/// Names the execution-envelope term that causally terminated this attempt.
///
/// Classification needs two independent facts, because neither alone is
/// causal. The terminal error must have a typed
/// [`DataFusionError::ResourcesExhausted`] root — a resource refusal actually
/// ended the attempt — and this attempt's own memory envelope must have
/// refused a reservation, so the exhaustion belongs to the lease Forge granted
/// rather than to unrelated work. A resource peak proves only successful
/// ownership and grants no classification authority, and a recoverable memory
/// refusal that the plan spilled past never reclassifies a later unrelated
/// failure. Scratch is deliberately unclassified: the pinned managed core
/// rebuilds its runtime without Forge's scratch ceiling, and `DataFusion`
/// reports its own temp-directory limit as an `io::Error`, so no causal
/// scratch signal exists to read.
fn execution_envelope_resource(
    error: &iceberg_compaction_core::error::CompactionError,
    memory_was_refused: bool,
) -> Option<&'static str> {
    let iceberg_compaction_core::error::CompactionError::DataFusion(error) = error else {
        return None;
    };
    matches!(
        error.find_root(),
        datafusion::error::DataFusionError::ResourcesExhausted(_)
    )
    .then_some(())
    .filter(|()| memory_was_refused)
    .map(|()| "memory")
}

/// Appends one plan's consumed live paths to the handoff's accumulators.
///
/// Data files are accumulated in order because a live data file belongs to
/// exactly one group; a repeat would be a real contradiction and the handoff
/// refuses it. Delete files are accumulated as sets because one delete file
/// legitimately covers several groups — an equality delete scoped to a
/// partition applies to every data file in it — and naming it once per group
/// would describe the same applied delete more than once.
///
/// Kept free-standing because it is a pure projection of the core's own file
/// group: it owns no state and reads nothing the plan does not already carry.
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

/// Binds one attempt's exact lease into the core's managed execution context.
///
/// Every execution term the core is allowed comes from the lease and nothing
/// else: the pool the governor issued, the resident ceiling it was sized to,
/// the attempt-owned scratch root, and the exact scratch byte figure that root
/// was granted. The core builds the single `DataFusion` runtime from these, so
/// an attempt cannot spill past the disk the governor actually accounted for.
///
/// # Errors
///
/// Returns [`ForgeError::Invariant`] when the leased scratch root is not
/// usable as a spill lease or the context refuses the leased terms.
fn managed_context_for(
    resources: &ForgeAttemptResources,
    attempt_id: Uuid,
    cancel: &CancellationToken,
    observer: &Arc<ForgeRewriteObserver>,
) -> Result<Arc<ManagedExecutionContext>, ForgeError> {
    let spill = SpillLease::new(resources.spill_root()).map_err(|error| ForgeError::Invariant {
        detail: format!("Forge attempt scratch root is not leasable: {error}"),
    })?;
    let memory_bytes = usize::try_from(resources.memory_bytes()).unwrap_or(usize::MAX);
    ManagedExecutionContext::builder()
        .with_attempt_id(AttemptId::from_uuid(attempt_id))
        .with_memory_pool(resources.memory_pool(), Some(memory_bytes))
        .with_spill_lease(spill)
        .with_scratch_capacity_bytes(resources.scratch_bytes())
        .with_cancellation(cancel.clone())
        .with_observer(Arc::clone(observer) as Arc<_>)
        .build()
        .map_err(|error| ForgeError::Invariant {
            detail: format!("Forge managed execution context is unusable: {error}"),
        })
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
    use super::{execution_envelope_resource, managed_context_for};
    use crate::catalog::TenantTableBinding;
    use crate::catalog::table_ref::TableRef;
    use crate::forge::rewrite::ForgeAttemptResources;
    use crate::namespaces::BifrostNamespace;
    use crate::resources::{BifrostRole, BifrostRuntimeResources, ForgeRewriteRequest};
    use datafusion::error::DataFusionError;
    use iceberg_compaction_core::error::CompactionError;
    use std::sync::Arc;
    use tokio_util::sync::CancellationToken;
    use uuid::Uuid;
    use wyrd_spec::DataTenantId;

    /// The core's runtime is bounded by exactly the scratch the governor issued.
    ///
    /// A context built from a lease must carry that lease's scratch figure into
    /// the single execution runtime, because that limit is the only thing
    /// stopping an admitted attempt from spilling past its accounted share of
    /// the pod's disk.
    ///
    /// # Panics
    ///
    /// Panics when the lease cannot be acquired or the context does not carry
    /// the leased scratch figure.
    #[test]
    fn managed_context_receives_exact_scratch_lease() {
        const MIB: u64 = 1024 * 1024;
        const MIB_USIZE: usize = 1024 * 1024;
        let roles = BifrostRuntimeResources::composed_for_test(
            1024 * MIB_USIZE,
            512 * MIB,
            [BifrostRole::Forge],
        );
        let forge = roles.forge().expect("Forge capability");
        let binding = TenantTableBinding::resolve((
            DataTenantId::new_v7(),
            TableRef::new(BifrostNamespace::Traces, "spans"),
        ))
        .expect("binding");
        let root = tempfile::tempdir().expect("pod scratch root");
        let scratch_bytes = 64 * MIB;
        let resources = ForgeAttemptResources::acquire(
            &forge,
            ForgeRewriteRequest {
                envelope: vala_sql::row_types::forge_tasks::ForgeTaskEnvelope {
                    version: vala_sql::row_types::forge_tasks::FORGE_ENVELOPE_VERSION,
                    reader_permits: 1,
                    decoded_batch_bytes: MIB,
                    decoded_input_bytes: MIB,
                    sort_working_bytes: 3 * MIB,
                    sort_merge_reservation_bytes: MIB,
                    encoder_buffer_bytes: 2 * MIB,
                    upload_chunk_bytes: MIB,
                    footer_encoded_bytes: 8 * MIB,
                    footer_decode_workspace_bytes: 32 * MIB,
                    sort_spill_bytes: MIB,
                },
                memory_bytes: 128 * MIB_USIZE,
                scratch_bytes,
                reader_permits: 1,
            },
            &binding,
            root.path(),
            Uuid::new_v4(),
            Uuid::new_v4(),
        )
        .expect("Forge attempt lease");
        assert_eq!(
            resources.scratch_bytes(),
            scratch_bytes,
            "the lease reports the exact scratch the governor issued"
        );
        let context = managed_context_for(
            &resources,
            Uuid::new_v4(),
            &CancellationToken::new(),
            &Arc::new(super::ForgeRewriteObserver::new()),
        )
        .expect("managed execution context");
        assert_eq!(
            context.runtime_env().disk_manager.max_temp_directory_size(),
            scratch_bytes,
            "the core's only runtime is bounded by the leased scratch figure"
        );
    }

    /// Only a typed resource failure caused by this attempt's own memory
    /// refusal is an execution-envelope outcome; a peak never classifies.
    #[test]
    fn execution_envelope_classification_requires_causal_exhaustion() {
        let exhausted = CompactionError::DataFusion(DataFusionError::ResourcesExhausted(
            "Forge sort allowance exhausted".to_owned(),
        ));
        assert_eq!(
            execution_envelope_resource(&exhausted, true),
            Some("memory"),
            "a typed resource failure with a recorded refusal names memory"
        );
        assert_eq!(
            execution_envelope_resource(&exhausted, false),
            None,
            "a typed resource failure this attempt did not cause is not an envelope outcome"
        );
        let unrelated = CompactionError::Execution("manifest scan failed".to_owned());
        assert_eq!(
            execution_envelope_resource(&unrelated, true),
            None,
            "a recoverable memory refusal cannot reclassify an unrelated failure"
        );
    }
}
