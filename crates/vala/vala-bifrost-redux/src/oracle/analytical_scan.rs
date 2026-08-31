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

/// Leaf that resolves one signed Analytical assignment on first execution.
///
/// The leaf advertises the leader's closure schema and partition count so the
/// follower's decoded plan has exactly the shape the leader planned. Resolution
/// is memoized: every partition of one leaf shares a single provider, so a
/// multi-partition stage performs the catalog and storage work once.
pub struct AnalyticalScanExec {
    /// Signed assignment naming this task's tenant binding, files, and closure.
    assignment: Arc<FollowerScanAssignment>,
    /// Role this node executes as, used to select the resolver's source family.
    role: ClusterRole,
    /// Process resolver that turns an assignment into a role-local provider.
    resolver: Arc<dyn FollowerSourceResolver>,
    /// Advertised physical properties derived from the leader's closure.
    properties: Arc<PlanProperties>,
    /// Provider resolved on first execution and shared by every partition.
    resolved: Arc<tokio::sync::OnceCell<Arc<dyn ExecutionPlan>>>,
}

impl AnalyticalScanExec {
    /// Creates one lazily resolved Analytical leaf.
    ///
    /// `schema` and `partitions` come from the leader's placeholder, not from
    /// the eventual provider, so plan shape is fixed before any IO happens.
    #[must_use]
    pub fn new(
        assignment: FollowerScanAssignment,
        role: ClusterRole,
        resolver: Arc<dyn FollowerSourceResolver>,
        schema: SchemaRef,
        partitions: usize,
    ) -> Self {
        let properties = Arc::new(PlanProperties::new(
            datafusion::physical_expr::EquivalenceProperties::new(schema),
            Partitioning::UnknownPartitioning(partitions.max(1)),
            datafusion::physical_plan::execution_plan::EmissionType::Incremental,
            datafusion::physical_plan::execution_plan::Boundedness::Bounded,
        ));
        Self {
            assignment: Arc::new(assignment),
            role,
            resolver,
            properties,
            resolved: Arc::new(tokio::sync::OnceCell::new()),
        }
    }

    /// Resolves, validates, and reshapes the provider backing this leaf.
    ///
    /// The two schema checks mirror `PhysicalPlanFollower::decode` exactly: the
    /// fingerprint identifies the table's complete canonical schema, and the
    /// leaf must then expose precisely the closure derived from that schema and
    /// the signed column names. Deriving the closure from the provider's own
    /// schema would make the check circular, so it is derived from the
    /// authenticated full schema. The provider is finally reshaped to the
    /// advertised partition count, because the leader planned against that
    /// count and an operator above this leaf depends on it.
    ///
    /// # Errors
    ///
    /// Returns [`DataFusionError::Plan`] when resolution fails, when either
    /// schema check fails, or when the provider cannot be repartitioned.
    async fn resolve(&self) -> Result<Arc<dyn ExecutionPlan>> {
        let session = SessionStateBuilder::new().with_default_features().build();
        let resolved = self
            .resolver
            .resolve(self.role, self.assignment.as_ref(), &session)
            .await
            .map_err(|error| {
                DataFusionError::Plan(format!("analytical source resolution failed: {error}"))
            })?;
        let actual = super::assignment_schema_fingerprint(resolved.full_schema.as_ref());
        if actual != self.assignment.schema_fingerprint {
            return Err(DataFusionError::Plan(
                "resolved provider schema fingerprint differs from assignment".to_owned(),
            ));
        }
        let expected = signed_closure_schema(
            resolved.full_schema.as_ref(),
            &self.assignment.required_columns,
        )
        .map_err(DataFusionError::Plan)?;
        if resolved.plan.schema() != expected {
            return Err(DataFusionError::Plan(
                "resolved provider schema differs from the signed projection closure".to_owned(),
            ));
        }
        if resolved.plan.schema() != self.schema() {
            return Err(DataFusionError::Plan(
                "resolved provider schema differs from the advertised leaf schema".to_owned(),
            ));
        }
        let advertised = self.properties.partitioning.partition_count();
        if resolved.plan.output_partitioning().partition_count() == advertised {
            return Ok(resolved.plan);
        }
        datafusion::physical_plan::repartition::RepartitionExec::try_new(
            resolved.plan,
            Partitioning::RoundRobinBatch(advertised),
        )
        .map(|plan| Arc::new(plan) as Arc<dyn ExecutionPlan>)
    }
}

impl std::fmt::Debug for AnalyticalScanExec {
    /// Renders only the non-secret scan identity and shape.
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("AnalyticalScanExec")
            .field("scan_id", &self.assignment.scan_id)
            .field("role", &self.role)
            .field(
                "partitions",
                &self.properties.partitioning.partition_count(),
            )
            .finish()
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
        write!(formatter, "AnalyticalScanExec: {}", self.assignment.scan_id)
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
        let resolved = self.resolved.get()?;
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
        let this = AnalyticalScanExec {
            assignment: Arc::clone(&self.assignment),
            role: self.role,
            resolver: Arc::clone(&self.resolver),
            properties: Arc::clone(&self.properties),
            resolved: Arc::clone(&self.resolved),
        };
        let stream = futures_util::stream::once(async move {
            let plan = this
                .resolved
                .get_or_try_init(|| this.resolve())
                .await?
                .clone();
            plan.execute(partition, context)
        })
        .try_flatten();
        Ok(Box::pin(RecordBatchStreamAdapter::new(schema, stream)))
    }
}
