//! Analytical follower leaf that resolves its authenticated source lazily.
//!
//! The Interactive path resolves a follower's provider before `DataFusion`
//! decode, because its dispatcher carries the signed assignments beside the
//! plan. The Analytical path has no such side channel: the plan *is* the
//! message, and upstream's worker decodes it synchronously from a session that
//! never sees the plan bytes. This leaf closes that gap by carrying its own
//! signed assignment through the codec and performing the catalog and storage
//! IO at `execute()` time, which is strictly after the stage ticket has already
//! authorized the raw message.

use std::sync::Arc;

use arrow::datatypes::SchemaRef;
use datafusion::common::tree_node::{TreeNode as _, TreeNodeRecursion};
use datafusion::common::{DataFusionError, Result};
use datafusion::error::Result as DataFusionResult;
use datafusion::execution::TaskContext;
use datafusion::execution::session_state::SessionStateBuilder;
use datafusion::physical_expr::PhysicalExpr;
use datafusion::physical_plan::metrics::MetricsSet;
use datafusion::physical_plan::stream::RecordBatchStreamAdapter;
use datafusion::physical_plan::{
    DisplayAs, DisplayFormatType, ExecutionPlan, ExecutionPlanProperties as _, Partitioning,
    PlanProperties, SendableRecordBatchStream,
};
use futures_util::TryStreamExt as _;
use wyrd_spec::vala::api::ClusterRole;
use wyrd_spec::vala::api::FollowerScanAssignment;

use super::bindings::OracleSourceKey;
use super::follower::{FollowerSourceResolver, signed_closure_schema};

/// The one source a lazily resolved Oracle leaf reads.
///
/// Both variants defer every byte of IO to `execute`, which is the only point
/// at which the admitted task — and therefore the query's runtime, pool,
/// deadline, cancellation, and bound sources — exists.
#[derive(Clone)]
enum AnalyticalScanSource {
    /// Leader-side leaf projecting the one Fused batch set bound for its table.
    LocalDrained {
        /// Exact planned key this leaf resolves its batches through.
        key: OracleSourceKey,
    },
    /// Worker-side leaf resolving the assignment authenticated on the wire.
    Assigned {
        /// Signed assignment naming this task's tenant binding, files, and closure.
        assignment: Arc<FollowerScanAssignment>,
        /// Role this node executes as, selecting the resolver's source family.
        role: ClusterRole,
        /// Process resolver that turns an assignment into a role-local provider.
        resolver: Arc<dyn FollowerSourceResolver>,
        /// This node's reader epoch, which the assignment's cut is protected under.
        reader_authority: Option<Arc<super::reader_pins::OracleReaderAuthority>>,
        /// Graph that owns the guard this leaf's protection produces.
        guard_sink: Option<Arc<GraphReaderGuardSink>>,
    },
}

impl AnalyticalScanSource {
    /// Returns the stable non-secret identity rendered in plan diagnostics.
    fn label(&self) -> &str {
        match self {
            Self::LocalDrained { key } => key.local_table().unwrap_or("local"),
            Self::Assigned { assignment, .. } => assignment.scan_id.as_str(),
        }
    }
}

/// Leaf that resolves one authenticated source on first execution.
///
/// The leaf advertises the leader's closure schema and partition count so the
/// follower's decoded plan has exactly the shape the leader planned. Resolution
/// is memoized: every partition of one leaf shares a single provider, so a
/// multi-partition stage performs the catalog and storage work once.
#[derive(Clone)]
pub struct AnalyticalScanExec {
    /// Closed source identity this leaf resolves at execution time.
    source: AnalyticalScanSource,
    /// Advertised physical properties derived from the leader's closure.
    properties: Arc<PlanProperties>,
    /// Provider resolved on first execution and shared by every partition.
    resolved: Arc<tokio::sync::OnceCell<ResolvedAnalyticalSource>>,
}

/// One leaf's resolved provider, memoized for every partition of the leaf.
///
/// The reader guard the provider was opened under is deliberately not here: it
/// belongs to the graph, because the decoded plan is dropped from upstream's
/// task cache asynchronously and could otherwise hold the epoch's claim past
/// this node's own teardown. What the provider keeps is the clonable permit,
/// which the graph's guard cancels the moment it is released.
struct ResolvedAnalyticalSource {
    /// Provider every partition of this leaf executes.
    plan: Arc<dyn ExecutionPlan>,
}

/// The one graph a decoded leaf hands its reader-epoch guard to.
///
/// Built per follower session, because the graph identity is only known once a
/// stage operation's headers have been verified. It exists so the guard's owner
/// is a thing this node tears down deterministically, rather than a plan whose
/// drop upstream schedules.
pub(super) struct GraphReaderGuardSink {
    /// Graph the retained guards are keyed under.
    graph: super::analytical::AnalyticalGraphKey,
    /// Node-local registry that releases them when the graph is invalidated.
    registry: Arc<super::analytical::AnalyticalRuntimeRegistry>,
}

impl GraphReaderGuardSink {
    /// Binds a sink to one authenticated graph on this node.
    pub(super) const fn new(
        graph: super::analytical::AnalyticalGraphKey,
        registry: Arc<super::analytical::AnalyticalRuntimeRegistry>,
    ) -> Self {
        Self { graph, registry }
    }

    /// Hands one leaf's guard to the graph that owns its lifetime.
    ///
    /// # Errors
    ///
    /// Returns [`BifrostError::QueryExecutionFailed`] when the graph is
    /// no longer registered, and `Internal` on lock poisoning.
    fn retain(
        &self,
        guard: super::reader_pins::ReaderQueryGuard,
    ) -> Result<(), wyrd_spec::vala::BifrostError> {
        self.registry.retain_reader_guard(self.graph, guard)
    }
}

impl AnalyticalScanExec {
    /// Creates one lazily resolved Analytical leaf.
    ///
    /// `schema` and `partitions` come from the leader's placeholder, not from
    /// the eventual provider, so plan shape is fixed before any IO happens.
    #[must_use]
    pub(super) fn new(
        assignment: FollowerScanAssignment,
        role: ClusterRole,
        resolver: Arc<dyn FollowerSourceResolver>,
        reader_authority: Option<Arc<super::reader_pins::OracleReaderAuthority>>,
        guard_sink: Option<Arc<GraphReaderGuardSink>>,
        schema: SchemaRef,
        partitions: usize,
    ) -> Self {
        Self::with_source(
            AnalyticalScanSource::Assigned {
                assignment: Arc::new(assignment),
                role,
                resolver,
                reader_authority,
                guard_sink,
            },
            schema,
            partitions,
        )
    }

    /// Creates the leader-side leaf that reads one table's bound Fused batches.
    ///
    /// The batch set is bound after admission and the audited drain, so this
    /// leaf retains only its key and the closure schema the plan was built
    /// against. A single partition is advertised because one drained batch set
    /// is projected as one memory source.
    #[must_use]
    pub(super) fn local_drained(key: OracleSourceKey, schema: SchemaRef) -> Self {
        Self::with_source(AnalyticalScanSource::LocalDrained { key }, schema, 1)
    }

    /// Returns the planned key when this leaf is the leader's drained tail.
    ///
    /// Encoding consults this to decide whether the leaf can cross the wire at
    /// all: a drained tail exists only on the node that drained it, so a stage
    /// carrying one may be dispatched only when the tail bound no rows.
    #[must_use]
    pub(super) fn local_drained_key(&self) -> Option<&OracleSourceKey> {
        match &self.source {
            AnalyticalScanSource::LocalDrained { key } => Some(key),
            AnalyticalScanSource::Assigned { .. } => None,
        }
    }

    /// Builds one leaf around an already-chosen source and advertised shape.
    fn with_source(source: AnalyticalScanSource, schema: SchemaRef, partitions: usize) -> Self {
        let properties = Arc::new(PlanProperties::new(
            datafusion::physical_expr::EquivalenceProperties::new(schema),
            Partitioning::UnknownPartitioning(partitions.max(1)),
            datafusion::physical_plan::execution_plan::EmissionType::Incremental,
            datafusion::physical_plan::execution_plan::Boundedness::Bounded,
        ));
        Self {
            source,
            properties,
            resolved: Arc::new(tokio::sync::OnceCell::new()),
        }
    }

    /// Resolves, validates, and reshapes the provider backing this leaf.
    ///
    /// Both variants resolve against the executing task rather than a fresh
    /// process default, so the resolved subtree inherits the admitted runtime,
    /// memory pool, session configuration, and registered functions.
    ///
    /// The assigned variant's two schema checks mirror
    /// `PhysicalPlanFollower::decode` exactly: the fingerprint identifies the
    /// table's complete canonical schema, and the leaf must then expose
    /// precisely the closure derived from that schema and the signed column
    /// names. Deriving the closure from the provider's own schema would make
    /// the check circular, so it is derived from the authenticated full schema.
    /// The provider is finally reshaped to the advertised partition count,
    /// because the leader planned against that count and an operator above this
    /// leaf depends on it.
    ///
    /// # Errors
    ///
    /// Returns [`DataFusionError::Plan`] when resolution fails, when either
    /// schema check fails, or when the provider cannot be repartitioned, and a
    /// [`DataFusionError::Execution`] when a local-drained leaf has no bound
    /// source in the executing task.
    async fn resolve(&self, task: &Arc<TaskContext>) -> Result<ResolvedAnalyticalSource> {
        let resolved = match &self.source {
            AnalyticalScanSource::LocalDrained { key } => {
                let lock = super::bindings::bindings_for_task(task.as_ref())?;
                let bindings = lock.get().ok_or_else(|| {
                    DataFusionError::Execution("Oracle execution bindings are not bound".to_owned())
                })?;
                bindings.grant().ensure_live()?;
                let batches = bindings.local_batches(key)?;
                return Ok(ResolvedAnalyticalSource {
                    plan: super::exec::OracleTableProvider::projected_memory_source(
                        batches,
                        &self.schema(),
                    )?,
                });
            }
            AnalyticalScanSource::Assigned {
                assignment,
                role,
                resolver,
                reader_authority,
                guard_sink,
            } => {
                let session = SessionStateBuilder::new()
                    .with_config(task.session_config().clone())
                    .with_runtime_env(task.runtime_env())
                    .with_scalar_functions(task.scalar_functions().values().cloned().collect())
                    .with_aggregate_functions(
                        task.aggregate_functions().values().cloned().collect(),
                    )
                    .with_window_functions(task.window_functions().values().cloned().collect())
                    .build();
                let permit = graph_owned_permit(
                    reader_authority.as_ref(),
                    guard_sink.as_ref(),
                    assignment.as_ref(),
                )
                .await?;
                let resolved = resolver
                    .resolve(*role, assignment.as_ref(), &session, permit.as_ref())
                    .await
                    .map_err(|error| {
                        DataFusionError::Plan(format!(
                            "analytical source resolution failed: {error}"
                        ))
                    })?;
                let actual = super::assignment_schema_fingerprint(resolved.full_schema.as_ref());
                if actual != assignment.schema_fingerprint {
                    return Err(DataFusionError::Plan(
                        "resolved provider schema fingerprint differs from assignment".to_owned(),
                    ));
                }
                let expected = signed_closure_schema(
                    resolved.full_schema.as_ref(),
                    &assignment.required_columns,
                )
                .map_err(DataFusionError::Plan)?;
                if resolved.plan.schema() != expected {
                    return Err(DataFusionError::Plan(
                        "resolved provider schema differs from the signed projection closure"
                            .to_owned(),
                    ));
                }
                resolved.plan
            }
        };
        if resolved.schema() != self.schema() {
            return Err(DataFusionError::Plan(
                "resolved provider schema differs from the advertised leaf schema".to_owned(),
            ));
        }
        let advertised = self.properties.partitioning.partition_count();
        let plan = if resolved.output_partitioning().partition_count() == advertised {
            resolved
        } else {
            Arc::new(
                datafusion::physical_plan::repartition::RepartitionExec::try_new(
                    resolved,
                    Partitioning::RoundRobinBatch(advertised),
                )?,
            ) as Arc<dyn ExecutionPlan>
        };
        Ok(ResolvedAnalyticalSource { plan })
    }
}

/// Protects one assignment's snapshot and leaves the guard with its graph.
///
/// Protection strictly precedes resolution: this node's own epoch must cover
/// the snapshot the leader signed before any catalog, manifest, or object read
/// happens. The guard it yields is then handed to the graph rather than kept by
/// the caller, because the plan the resolved provider lands in is dropped from
/// upstream's task cache asynchronously and could otherwise still hold this
/// epoch's claim when it retires. What the caller keeps is the clonable permit,
/// which the graph's guard cancels the moment it is released.
///
/// # Errors
///
/// Returns [`DataFusionError::Plan`] when protection is refused, when a
/// protected leaf has no graph owner to hand its guard to, or when the graph is
/// no longer registered to accept one.
async fn graph_owned_permit(
    reader_authority: Option<&Arc<super::reader_pins::OracleReaderAuthority>>,
    guard_sink: Option<&Arc<GraphReaderGuardSink>>,
    assignment: &FollowerScanAssignment,
) -> Result<Option<super::reader_pins::ReaderIoPermit>> {
    let protection = super::follower::protect_reader_cuts(reader_authority, [assignment])
        .await
        .map_err(|error| {
            DataFusionError::Plan(format!("analytical source protection failed: {error}"))
        })?;
    let Some(protection) = protection else {
        return Ok(None);
    };
    let (guard, permit) = protection.into_parts();
    let Some(sink) = guard_sink else {
        return Err(DataFusionError::Plan(
            "analytical leaf has no graph owner for its reader guard".to_owned(),
        ));
    };
    sink.retain(guard).map_err(|error| {
        DataFusionError::Plan(format!(
            "analytical leaf could not retain its reader guard: {error}"
        ))
    })?;
    Ok(Some(permit))
}

impl std::fmt::Debug for AnalyticalScanExec {
    /// Renders only the non-secret scan identity and shape.
    ///
    /// The follower source resolver and the plan it resolves to are
    /// deliberately omitted: neither is `Debug`, and a plan tree rendered
    /// inside an operator's own `Debug` would recurse through the whole stage.
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("AnalyticalScanExec")
            .field("source", &self.source.label())
            .field(
                "partitions",
                &self.properties.partitioning.partition_count(),
            )
            .finish_non_exhaustive()
    }
}

impl DisplayAs for AnalyticalScanExec {
    /// Formats only the non-secret scan identity.
    ///
    /// # Errors
    /// Returns the formatter error if the destination cannot accept the identity.
    fn fmt_as(
        &self,
        _display: DisplayFormatType,
        formatter: &mut std::fmt::Formatter<'_>,
    ) -> std::fmt::Result {
        write!(formatter, "AnalyticalScanExec: {}", self.source.label())
    }
}

impl ExecutionPlan for AnalyticalScanExec {
    /// Visits every physical expression this plan owns.
    ///
    /// This leaf owns no `PhysicalExpr`, so the traversal reports
    /// [`TreeNodeRecursion::Continue`] without invoking `f`.
    ///
    /// # Errors
    /// Never returns an error; the signature is fixed by the trait.
    fn apply_expressions(
        &self,
        _f: &mut dyn FnMut(&Arc<dyn PhysicalExpr>) -> DataFusionResult<TreeNodeRecursion>,
    ) -> DataFusionResult<TreeNodeRecursion> {
        Ok(TreeNodeRecursion::Continue)
    }

    /// Returns the stable diagnostic operator name.
    fn name(&self) -> &'static str {
        "AnalyticalScanExec"
    }

    /// Returns the properties derived from the leader's signed closure.
    fn properties(&self) -> &Arc<PlanProperties> {
        &self.properties
    }

    /// An analytical scan is always a leaf.
    fn children(&self) -> Vec<&Arc<dyn ExecutionPlan>> {
        Vec::new()
    }

    /// Rejects children and preserves the immutable leaf.
    ///
    /// # Errors
    /// Returns a plan error when a caller attempts to attach any child.
    fn with_new_children(
        self: Arc<Self>,
        children: Vec<Arc<dyn ExecutionPlan>>,
    ) -> Result<Arc<dyn ExecutionPlan>> {
        if children.is_empty() {
            Ok(self)
        } else {
            Err(DataFusionError::Plan(
                "analytical scan must remain a leaf".to_owned(),
            ))
        }
    }

    /// Reports the resolved provider's own physical scan metrics as this leaf's.
    ///
    /// The resolved plan is not a child of this node — it lives behind the
    /// memoized cell, so nothing that walks the plan tree can see it. Upstream
    /// collects a follower's metrics by walking exactly that tree and ships
    /// them back to the coordinator, so without this the leader would observe a
    /// distributed query that scanned nothing. Merging the subtree's metric
    /// sets here reports the bytes, row groups, and rows this leaf actually
    /// read, attributed to the node that owns the read.
    ///
    /// Returns `None` before the first execution resolves a provider, which is
    /// the honest answer: no scan has happened yet.
    fn metrics(&self) -> Option<MetricsSet> {
        let resolved = &self.resolved.get()?.plan;
        let mut merged = MetricsSet::new();
        let mut collect = |plan: &Arc<dyn ExecutionPlan>| {
            if let Some(metrics) = plan.metrics() {
                for metric in metrics.iter() {
                    merged.push(Arc::clone(metric));
                }
            }
        };
        collect(resolved);
        let _ = resolved.apply(|plan| {
            collect(plan);
            Ok(TreeNodeRecursion::Continue)
        });
        for metric in super::exec::analytical_leaf_scan_metrics(resolved.as_ref()).iter() {
            merged.push(Arc::clone(metric));
        }
        Some(merged.aggregate_by_name())
    }

    /// Streams one partition, resolving the authenticated provider on first use.
    ///
    /// Resolution happens inside the returned stream rather than in this
    /// synchronous call because the provider needs catalog and storage IO.
    /// Every partition awaits the same [`tokio::sync::OnceCell`], so a stage
    /// resolves once and a failed resolution fails every partition identically.
    ///
    /// # Errors
    /// Returns a plan error when the partition index exceeds the advertised
    /// count. Resolution and downstream execution failures surface on the
    /// returned stream.
    fn execute(
        &self,
        partition: usize,
        context: Arc<TaskContext>,
    ) -> Result<SendableRecordBatchStream> {
        let advertised = self.properties.partitioning.partition_count();
        if partition >= advertised {
            return Err(DataFusionError::Plan(format!(
                "analytical scan has no partition {partition}"
            )));
        }
        let schema = self.schema();
        let this = self.clone();
        let stream = futures_util::stream::once(async move {
            let plan = this
                .resolved
                .get_or_try_init(|| this.resolve(&context))
                .await?
                .plan
                .clone();
            plan.execute(partition, context)
        })
        .try_flatten();
        Ok(Box::pin(RecordBatchStreamAdapter::new(schema, stream)))
    }
}
