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

use super::follower::{FollowerSourceResolver, signed_closure_schema};

/// The worker-side assignment a lazily resolved Oracle leaf reads.
///
/// Every byte of IO is deferred to `execute`, which is the only point at which
/// the admitted task — and therefore the query's runtime, pool, deadline, and
/// cancellation — exists.
#[derive(Clone)]
struct AnalyticalScanSource {
    /// Signed assignment naming this task's tenant binding, files, and closure.
    assignment: Arc<FollowerScanAssignment>,
    /// Role this node executes as, selecting the resolver's source family.
    role: ClusterRole,
    /// Process resolver that turns an assignment into a role-local provider.
    resolver: Arc<dyn FollowerSourceResolver>,
}

impl AnalyticalScanSource {
    /// Returns the stable non-secret identity rendered in plan diagnostics.
    fn label(&self) -> &str {
        self.assignment.scan_id.as_str()
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
    /// Literal paths applied to the resolved provider's Struct and Variant columns.
    leaf_paths: super::leaf_paths::LeafPaths,
    /// Advertised physical properties derived from the leader's closure.
    properties: Arc<PlanProperties>,
    /// Provider resolved on first execution and shared by every partition.
    resolved: Arc<tokio::sync::OnceCell<ResolvedAnalyticalSource>>,
}

/// One leaf's resolved provider, memoized for every partition of the leaf.
struct ResolvedAnalyticalSource {
    /// Provider every partition of this leaf executes.
    plan: Arc<dyn ExecutionPlan>,
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
        schema: SchemaRef,
        partitions: usize,
    ) -> Self {
        Self::with_source(
            AnalyticalScanSource {
                assignment: Arc::new(assignment),
                role,
                resolver,
            },
            schema,
            partitions,
        )
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
            leaf_paths: super::leaf_paths::LeafPaths::default(),
            properties,
            resolved: Arc::new(tokio::sync::OnceCell::new()),
        }
    }

    /// Applies the leader's Variant `paths` to the provider this leaf
    /// resolves, so its readers decode only those paths' leaves.
    #[must_use]
    pub fn with_leaf_paths(mut self, paths: super::leaf_paths::LeafPaths) -> Self {
        self.leaf_paths = paths;
        self
    }

    /// Resolves, validates, and reshapes the provider backing this leaf.
    ///
    /// Resolution runs against the executing task rather than a fresh
    /// process default, so the resolved subtree inherits the admitted runtime,
    /// memory pool, session configuration, and registered functions.
    ///
    /// The two schema checks mirror
    /// `PhysicalPlanFollower::decode` exactly: the fingerprint identifies the
    /// table's complete canonical schema, and the leaf must then expose
    /// precisely the closure derived from that schema and the signed column
    /// names. Deriving the closure from the provider's own schema would make
    /// the check circular, so it is derived from the authenticated full schema.
    /// The leader's leaf paths are then applied to the provider's readers.
    /// The provider is finally reshaped to the advertised partition count,
    /// because the leader planned against that count and an operator above this
    /// leaf depends on it.
    ///
    /// # Errors
    ///
    /// Returns [`DataFusionError::Plan`] when resolution fails, when either
    /// schema check fails, or when the leaf paths or repartitioning cannot
    /// be applied to the provider.
    async fn resolve(&self, task: &Arc<TaskContext>) -> Result<ResolvedAnalyticalSource> {
        let resolved = {
            let AnalyticalScanSource {
                assignment,
                role,
                resolver,
            } = &self.source;
            let session = SessionStateBuilder::new()
                .with_config(task.session_config().clone())
                .with_runtime_env(task.runtime_env())
                .with_scalar_functions(task.scalar_functions().values().cloned().collect())
                .with_aggregate_functions(task.aggregate_functions().values().cloned().collect())
                .with_window_functions(task.window_functions().values().cloned().collect())
                .build();
            let resolved = resolver
                .resolve(*role, assignment.as_ref(), &session)
                .await
                .map_err(|error| {
                    DataFusionError::Plan(format!("analytical source resolution failed: {error}"))
                })?;
            let actual = super::assignment_schema_fingerprint(resolved.full_schema.as_ref());
            if actual != assignment.schema_fingerprint {
                return Err(DataFusionError::Plan(
                    "resolved provider schema fingerprint differs from assignment".to_owned(),
                ));
            }
            let expected = signed_closure_schema(resolved.full_schema.as_ref(), assignment)
                .map_err(DataFusionError::Plan)?;
            if resolved.plan.schema() != expected {
                return Err(DataFusionError::Plan(
                    "resolved provider schema differs from the signed projection closure"
                        .to_owned(),
                ));
            }
            super::leaf_paths::LeafPathPushdown::assign(resolved.plan, &self.leaf_paths)?
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
