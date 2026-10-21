//! Leader-owned streaming source over the selected Scribe live routes.
//!
//! Oracle discovers each referenced table's reported Scribe streams before
//! its single physical planning pass and plants one [`LiveScribeExec`] per
//! scan at the session's target partitions, dealing its routes across them.
//! When `DataFusion`'s `FilterPushdown` offers the leaf its filters, the leaf
//! keeps only the routes a `PruningPredicate` over each route's event-time
//! bounds cannot exclude. Planning opens
//! nothing. At execution each partition dispatches one signed fragment per
//! route it owns, in turn, to that route's Scribe and hands every batch to
//! `DataFusion` as it arrives, so no live fragment is ever materialized on the
//! leader.
//!
//! Every route reads the capture its Scribe retained for the query at
//! discovery; the captures are released when the last plan holding the
//! table's routes drops. A capture lost during execution, and any integrity,
//! tenant, security, resource, cancellation, or deadline fault, fails the
//! query: no live source is ever reopened underneath it.

use std::sync::Arc;
use std::time::Duration;

use arrow::datatypes::SchemaRef;
use arrow::record_batch::RecordBatch;
use chrono::{DateTime, Utc};
use datafusion::common::Statistics;
use datafusion::common::pruning::PrunableStatistics;
use datafusion::common::tree_node::{Transformed, TreeNode as _, TreeNodeRecursion};
use datafusion::config::ConfigOptions;
use datafusion::error::{DataFusionError, Result as DataFusionResult};
use datafusion::execution::TaskContext;
use datafusion::physical_expr::DynamicFilterTracking;
use datafusion::physical_expr::PhysicalExpr;
use datafusion::physical_expr::utils::{conjunction, split_conjunction};
use datafusion::physical_expr_common::physical_expr::snapshot_physical_expr;
use datafusion::physical_optimizer::pruning::PruningPredicateBuilder;
use datafusion::physical_plan::filter::FilterExec;
use datafusion::physical_plan::filter_pushdown::{
    ChildPushdownResult, FilterPushdownPhase, FilterPushdownPropagation,
};
use datafusion::physical_plan::metrics::{
    ExecutionPlanMetricsSet, MetricBuilder, MetricsSet, Time,
};
use datafusion::physical_plan::stream::RecordBatchStreamAdapter;
use datafusion::physical_plan::{
    DisplayAs, DisplayFormatType, ExecutionPlan, Partitioning, PlanProperties,
    SendableRecordBatchStream,
};
use futures_util::StreamExt as _;
use sha2::{Digest as _, Sha256};
use wyrd_spec::vala::BifrostError;
use wyrd_spec::vala::api::{
    ClusterRole, FollowerScanAssignment, NodeId, ScribeProviderCut, TenantTableBinding,
    TimePartitionWire, WorkerAttemptFrame,
};

use super::dispatcher::{
    DispatchCandidate, DispatchContext, DispatchError, FragmentDispatcher, LiveFrame,
    PhysicalDispatchFragment,
};
use super::exec::RemoteScanMetrics;
use super::{DegradedPartition, DegradedSourceAccumulator, QueryCatalogError};
use crate::catalog::TimeAxis;

/// Stable degraded-reason label for a live source lost before its first row.
const LIVE_TAIL_UNAVAILABLE: &str = "live_tail_unavailable";

/// One reported Scribe stream partition that may hold a table's live rows.
///
/// Every field comes from discovery on the attempt's frozen roster: the Scribe
/// incarnation that reported the stream, the writer epoch it reported under,
/// that participant's endpoint, and the exact time partition it retains. The fragment later dispatched for
/// this route is signed for exactly this incarnation and epoch, so a Scribe
/// that restarts in between refuses it rather than answering for another cut.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LiveScribeRoute {
    /// Reporting Scribe node.
    pub(crate) node_id: NodeId,
    /// Writer epoch (the Scribe's role fence) the stream was reported under.
    pub(crate) writer_epoch: u64,
    /// Private endpoint of the frozen roster participant that reported it.
    ///
    /// The fragment dials this exact endpoint, so listing and row reads name
    /// one participant of one frozen cut.
    pub(crate) endpoint: String,
    /// Exact time partition the stream retains.
    pub(crate) time_partition: TimePartitionWire,
    /// Live bytes the listing reported this query reads in the partition;
    /// sizes the query's partitions and nothing else.
    pub(crate) bytes: u64,
    /// Capture the reporting Scribe retains for this query.
    pub(crate) capture_id: uuid::Uuid,
}

/// The captures one table's discovery retained, released on drop.
///
/// Shared by every clone of the table's routes, so the captures stay retained
/// while any plan can still read them and are released once, by the last
/// drop, on completion, cancellation, failure, or a discarded selection. The
/// detached release waits no later than the query deadline. A drop outside a
/// Tokio runtime releases nothing; each capture then expires at that
/// deadline on its Scribe.
pub(crate) struct LiveCaptures {
    /// Discovery that retained the captures and releases them.
    discovery: Arc<dyn super::TailStreamDiscovery>,
    /// Binding every capture was retained for.
    binding: TenantTableBinding,
    /// Query attempt every capture is bound to.
    query_id: uuid::Uuid,
    /// Each retaining Scribe with its capture.
    captures: Vec<(super::OracleQueryParticipant, uuid::Uuid)>,
    /// The query's absolute deadline, which is every capture's expiry and the
    /// latest instant the release waits for a Scribe.
    deadline: std::time::Instant,
}

impl LiveCaptures {
    /// Takes ownership of the captures `discovery` retained for one table
    /// until the query's `deadline`.
    pub(crate) fn new(
        discovery: Arc<dyn super::TailStreamDiscovery>,
        binding: TenantTableBinding,
        query_id: uuid::Uuid,
        captures: Vec<(super::OracleQueryParticipant, uuid::Uuid)>,
        deadline: std::time::Instant,
    ) -> Self {
        Self {
            discovery,
            binding,
            query_id,
            captures,
            deadline,
        }
    }
}

/// Discovery for fixtures whose routes are planned but never dialed.
#[cfg(test)]
struct NoCaptureDiscovery;

#[cfg(test)]
#[async_trait::async_trait]
impl super::TailStreamDiscovery for NoCaptureDiscovery {
    /// Discovers nothing; fixtures construct their routes directly.
    ///
    /// # Errors
    ///
    /// Never fails: the fixture always returns an empty discovery.
    async fn discover(
        &self,
        _binding: &TenantTableBinding,
        _scope: &crate::scan_sizing::ScanScope,
        _scribes: &[super::OracleQueryParticipant],
        _query_id: uuid::Uuid,
        _deadline: std::time::Instant,
    ) -> Result<super::DiscoveredTail, crate::scribe::tail_rpc::TailReadError> {
        Ok(super::DiscoveredTail::default())
    }

    /// Holds no capture, so there is nothing to release.
    async fn release(
        &self,
        _binding: &TenantTableBinding,
        _query_id: uuid::Uuid,
        _captures: &[(super::OracleQueryParticipant, uuid::Uuid)],
        _deadline: std::time::Instant,
    ) {
    }
}

#[cfg(test)]
impl LiveCaptures {
    /// Returns an empty capture set for fixture routes that are never dialed.
    pub(crate) fn none_for_test(binding: TenantTableBinding) -> Arc<Self> {
        Arc::new(Self::new(
            Arc::new(NoCaptureDiscovery),
            binding,
            uuid::Uuid::nil(),
            Vec::new(),
            std::time::Instant::now(),
        ))
    }
}

impl Drop for LiveCaptures {
    /// Spawns the release of every capture on the current runtime, bounded
    /// by the query deadline so a stalled Scribe cannot keep the detached task
    /// alive past it; a capture left unreleased expires there.
    fn drop(&mut self) {
        let captures = std::mem::take(&mut self.captures);
        if captures.is_empty() {
            return;
        }
        let Ok(runtime) = tokio::runtime::Handle::try_current() else {
            return;
        };
        let discovery = Arc::clone(&self.discovery);
        let binding = self.binding.clone();
        let query_id = self.query_id;
        let deadline = self.deadline;
        runtime.spawn(async move {
            discovery
                .release(&binding, query_id, &captures, deadline)
                .await;
        });
    }
}

impl std::fmt::Debug for LiveCaptures {
    /// Renders the query and capture count; the discovery holds peer channels.
    ///
    /// # Errors
    ///
    /// Returns the formatter's error unchanged.
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("LiveCaptures")
            .field("query_id", &self.query_id)
            .field("captures", &self.captures.len())
            .finish_non_exhaustive()
    }
}

/// Every live route discovered for one pinned table, with its signing identity.
///
/// Built once per query, after the published cut is pinned and before the
/// physical plan exists. It retains no rows and names no transport.
#[derive(Debug, Clone)]
pub(crate) struct LiveTableRoutes {
    /// Authenticated wire binding every fragment for this table is signed for.
    pub(crate) binding: TenantTableBinding,
    /// Reported routes, in discovery order.
    pub(crate) routes: Vec<LiveScribeRoute>,
    /// Captures the routes read, released when the last clone drops.
    pub(crate) captures: Arc<LiveCaptures>,
}

impl LiveTableRoutes {
    /// Returns the routes `predicate` cannot exclude, in discovery order.
    ///
    /// Each route is one container whose exact `time_column` bounds are its
    /// time partition, inclusive start to exclusive end, so `DataFusion`'s
    /// `PruningPredicate` decides it as it decides a file. A predicate that
    /// does not plan as a pruning predicate, or does not constrain the time
    /// column, keeps every route.
    pub(super) fn selected_by(
        &self,
        predicate: &Arc<dyn PhysicalExpr>,
        schema: &SchemaRef,
        time_column: &str,
    ) -> Self {
        let statistics = self
            .routes
            .iter()
            .map(|route| {
                let partition =
                    crate::catalog::layout::TimePartition::from_wire(route.time_partition);
                let mut statistics = Statistics::new_unknown(schema);
                super::exec::set_time_bounds(
                    &mut statistics,
                    schema,
                    time_column,
                    partition.start_unix_micros(),
                    partition.end_utc().timestamp_micros().saturating_sub(1),
                );
                Arc::new(statistics)
            })
            .collect();
        let keep = PruningPredicateBuilder::new()
            .with_file_schema(Arc::clone(schema))
            .try_build(Arc::clone(predicate))
            .and_then(|pruning| {
                pruning.prune(&PrunableStatistics::new(statistics, Arc::clone(schema)))
            })
            .unwrap_or_else(|_| vec![true; self.routes.len()]);
        Self {
            binding: self.binding.clone(),
            routes: self
                .routes
                .iter()
                .zip(keep)
                .filter(|(_, keep)| *keep)
                .map(|(route, _)| route.clone())
                .collect(),
            captures: Arc::clone(&self.captures),
        }
    }
}

/// Post-admission capability every live partition dispatches through.
///
/// Bound once with the query's execution bindings, so a live leaf never holds a
/// dispatcher, deadline, or permission digest from before admission.
pub(super) struct LiveDispatch {
    /// Signing, reserving, and transport owner for peer fragments.
    dispatcher: Arc<FragmentDispatcher>,
    /// Admitted query identity, authority, deadline, and cancellation.
    context: DispatchContext,
}

impl LiveDispatch {
    /// Binds the dispatcher to one admitted query's dispatch context.
    pub(super) const fn new(dispatcher: Arc<FragmentDispatcher>, context: DispatchContext) -> Self {
        Self {
            dispatcher,
            context,
        }
    }
}

impl std::fmt::Debug for LiveDispatch {
    /// Renders only the query identity; the dispatcher holds signing authority.
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("LiveDispatch")
            .field("query_id", &self.context.query_id)
            .finish_non_exhaustive()
    }
}

/// Records that a table's live streams could not be listed.
///
/// Only listing loss degrades: a query that never captured a table's live
/// sources reads it from published files alone. A capture lost after it was
/// accepted fails the query instead.
///
/// The loss uses the non-partition ordinal, so it sits beside the
/// participant losses on the same accumulator the terminal reads. A poisoned
/// accumulator drops the note rather than failing a query that otherwise
/// completed.
pub(super) fn record_live_tail_unavailable(degraded: &DegradedSourceAccumulator) {
    if let Ok(mut degraded) = degraded.lock() {
        degraded.push(DegradedPartition {
            ordinal: u32::MAX,
            reason: LIVE_TAIL_UNAVAILABLE,
            sources: vec![wyrd_spec::vala::api::QuerySource::LiveTail],
        });
    }
}

/// Leader-owned leaf reading one scan's selected Scribe routes.
///
/// It runs at the session's target partitions; route `i` belongs to partition
/// `i % partitions`, so a partition reads its routes one after another and a
/// partition that owns none ends empty.
///
/// It is never serialized: the Analytical planner keeps its stage on the
/// leader, and the codec refuses it. What it retains is only planning
/// authority — routes and the signed closure — never a dispatcher or rows.
#[derive(Debug, Clone)]
pub(super) struct LiveScribeExec {
    /// Table binding and the selected routes, dealt across output partitions.
    routes: Arc<LiveTableRoutes>,
    /// The table's time axis, whose column pushed filters prune routes by.
    time_axis: TimeAxis,
    /// Fingerprint of the table's complete physical schema.
    schema_fingerprint: String,
    /// Signed projection closure, in closure order.
    required_columns: Vec<String>,
    /// Advertised closure schema and the session's target partitions.
    properties: Arc<PlanProperties>,
    /// Conjunction of the query filters `DataFusion`'s `FilterPushdown`
    /// offered this leaf, shipped to every Scribe above its placeholder.
    ///
    /// The Scribe pushes it into its own staged scan, so a fragment returns
    /// only the rows that can pass. The leader keeps its `FilterExec`, so the
    /// result never depends on how much the Scribe narrowed.
    predicate: Option<Arc<dyn PhysicalExpr>>,
    /// Scan evidence every validated fragment completion folds in, shared by
    /// every clone of this leaf so the query's collector reads one total.
    scan_metrics: Arc<RemoteScanMetrics>,
    /// `DataFusion` metrics for opening and awaiting remote fragments,
    /// registered per partition when it executes and read through
    /// [`ExecutionPlan::metrics`] like every other operator's.
    metrics: ExecutionPlanMetricsSet,
}

impl LiveScribeExec {
    /// Plans one live leaf over already-selected routes at `partitions`
    /// output partitions, the planning session's target partitions.
    ///
    /// `routes.routes` must be non-empty; a scan with nothing discovered plants
    /// no live leaf at all. Pushed filters may later prune every route, and
    /// each partition then ends empty.
    pub(super) fn new(
        routes: LiveTableRoutes,
        time_axis: TimeAxis,
        schema_fingerprint: String,
        required_columns: Vec<String>,
        schema: SchemaRef,
        partitions: usize,
    ) -> Self {
        let partitions = partitions.max(1);
        Self {
            routes: Arc::new(routes),
            time_axis,
            schema_fingerprint,
            required_columns,
            properties: Arc::new(PlanProperties::new(
                datafusion::physical_expr::EquivalenceProperties::new(schema),
                Partitioning::UnknownPartitioning(partitions),
                datafusion::physical_plan::execution_plan::EmissionType::Incremental,
                datafusion::physical_plan::execution_plan::Boundedness::Bounded,
            )),
            predicate: None,
            scan_metrics: Arc::default(),
            metrics: ExecutionPlanMetricsSet::new(),
        }
    }

    /// Returns the accumulator the Scribe's reported scan evidence lands in.
    ///
    /// The Scribe reads staged runs through its own hot-Parquet leaf, so the
    /// leader's physical bytes for this source exist only as each fragment's
    /// footer or in-process completion reports them.
    pub(super) const fn scan_metrics(&self) -> &Arc<RemoteScanMetrics> {
        &self.scan_metrics
    }

    /// Returns this leaf with its predicate snapshotted to static
    /// expressions, and the routes output `partition` reads under that
    /// snapshot.
    ///
    /// Route `i` of the planning-time selection belongs to partition
    /// `i % partitions`, so every route is dealt exactly once whatever each
    /// partition's snapshot says; the snapshot then only drops routes from
    /// this partition's share. Called when the partition's stream is first
    /// polled: a `ScalarSubqueryExec` above has populated its values by then,
    /// and a join has finished its build side before polling its probe side,
    /// so a resolved scalar or a join's dynamic filter becomes a static
    /// predicate that both prunes routes and ships in every fragment. `self`
    /// keeps the runtime-bound predicate for re-execution.
    ///
    /// # Errors
    ///
    /// Returns the snapshot error when the predicate reads execution state
    /// that is not yet populated.
    fn resolved(&self, partition: usize) -> DataFusionResult<(Self, Vec<LiveScribeRoute>)> {
        let routes = LiveTableRoutes {
            binding: self.routes.binding.clone(),
            routes: self
                .routes
                .routes
                .iter()
                .skip(partition)
                .step_by(self.properties.partitioning.partition_count())
                .cloned()
                .collect(),
            captures: Arc::clone(&self.routes.captures),
        };
        let mut leaf = self.clone();
        let Some(predicate) = &self.predicate else {
            return Ok((leaf, routes.routes));
        };
        let predicate = snapshot_physical_expr(Arc::clone(predicate))?;
        let routes = routes.selected_by(&predicate, &self.schema(), self.time_axis.column());
        leaf.predicate = Some(predicate);
        Ok((leaf, routes.routes))
    }

    /// Builds the signed fragment and destination for one route.
    ///
    /// The encoded plan is the same identity-bound placeholder a Scribe
    /// follower decodes for any hot-provider assignment, under a `FilterExec`
    /// of the pushed predicate when there is one; its fingerprint binds the
    /// ticket, the request, and the footer to one another.
    ///
    /// # Errors
    ///
    /// Returns a `DataFusion` plan error when the placeholder or its filter
    /// cannot be built or encoded.
    fn fragment(
        &self,
        route: &LiveScribeRoute,
        deadline_unix_ms: i64,
    ) -> DataFusionResult<(PhysicalDispatchFragment, DispatchCandidate)> {
        let binding = &self.routes.binding;
        let scan_id = super::scribe_follower_scan_id(
            &format!("{}.{}", binding.namespace, binding.table),
            route.node_id,
            route.writer_epoch,
        );
        let mut plan: Arc<dyn ExecutionPlan> =
            Arc::new(super::codec::RemoteSourcePlaceholderExec::new(
                scan_id.clone(),
                &self.schema_fingerprint,
                self.schema(),
            ));
        if let Some(predicate) = &self.predicate {
            plan = Arc::new(FilterExec::try_new(Arc::clone(predicate), plan)?);
        }
        let physical_plan_bytes =
            datafusion_proto::bytes::physical_plan_to_bytes_with_extension_codec(
                plan,
                &super::codec::OraclePhysicalExtensionCodec::encoder(),
            )?
            .to_vec();
        let plan_fingerprint = super::codec::physical_plan_fingerprint(&physical_plan_bytes);
        let assignment = FollowerScanAssignment {
            scan_id,
            binding: binding.clone(),
            scribe_provider_cut: Some(ScribeProviderCut {
                writer_epoch: route.writer_epoch,
                start_partition: route.time_partition,
                end_partition: route.time_partition,
                capture_id: route.capture_id,
            }),
            schema_fingerprint: self.schema_fingerprint.clone(),
            required_columns: self.required_columns.clone(),
        };
        Ok((
            PhysicalDispatchFragment {
                physical_plan_bytes,
                assignments: vec![assignment],
                binding: binding.clone(),
                target_role: ClusterRole::Scribe,
                plan_fingerprint,
                deadline_unix_ms,
            },
            DispatchCandidate {
                node_id: route.node_id,
                role: ClusterRole::Scribe,
                worker_fence: route.writer_epoch,
                endpoint: Some(route.endpoint.clone()),
            },
        ))
    }
}

impl DisplayAs for LiveScribeExec {
    /// Formats the table and selected route count, never tenant rows.
    ///
    /// # Errors
    /// Returns the formatter error if the destination cannot accept the text.
    fn fmt_as(
        &self,
        _display: DisplayFormatType,
        formatter: &mut std::fmt::Formatter<'_>,
    ) -> std::fmt::Result {
        write!(
            formatter,
            "LiveScribeExec: {}.{} routes={}",
            self.routes.binding.namespace,
            self.routes.binding.table,
            self.routes.routes.len()
        )
    }
}

impl ExecutionPlan for LiveScribeExec {
    /// Visits the pushed predicate, the only physical expression this leaf owns.
    ///
    /// # Errors
    /// Returns the visitor's error.
    fn apply_expressions(
        &self,
        f: &mut dyn FnMut(&Arc<dyn PhysicalExpr>) -> DataFusionResult<TreeNodeRecursion>,
    ) -> DataFusionResult<TreeNodeRecursion> {
        match &self.predicate {
            Some(predicate) => f(predicate),
            None => Ok(TreeNodeRecursion::Continue),
        }
    }

    /// Takes the filters `DataFusion` pushes to this leaf as the predicate
    /// every fragment ships with: the static filters of the `Pre` phase, then
    /// the dynamic filters of the `Post` phase that joins and `TopK`
    /// operators update while the query runs.
    ///
    /// Every filter is reported unsupported, so the leader's `FilterExec` and
    /// the producing operator stay the authority over correctness above the
    /// leaf.
    ///
    /// Filters bound to execution state — a dynamic filter, or
    /// `id = (SELECT max(id) ...)` whose value `ScalarSubqueryExec` supplies
    /// before this leaf executes — are kept in the predicate but cannot prune
    /// routes yet: planning prunes by the conjuncts that already snapshot to
    /// static expressions, and each partition narrows its share further once
    /// the rest resolve, when its stream is first polled.
    ///
    /// # Errors
    /// Never returns an error; the signature is fixed by the trait.
    fn handle_child_pushdown_result(
        &self,
        phase: FilterPushdownPhase,
        child_pushdown_result: ChildPushdownResult,
        _config: &ConfigOptions,
    ) -> DataFusionResult<FilterPushdownPropagation<Arc<dyn ExecutionPlan>>> {
        let pushed = child_pushdown_result
            .parent_filters
            .iter()
            .map(|filter| &filter.filter)
            .filter(|filter| {
                DynamicFilterTracking::classify(filter).contains_dynamic_filter()
                    == (phase == FilterPushdownPhase::Post)
            })
            .cloned()
            .collect::<Vec<_>>();
        let propagation = FilterPushdownPropagation::all_unsupported(child_pushdown_result);
        if pushed.is_empty() {
            return Ok(propagation);
        }
        let filters = self.predicate.iter().cloned().chain(pushed);
        let predicate = conjunction(filters);
        let static_predicate = conjunction(
            split_conjunction(&predicate)
                .into_iter()
                .filter(|conjunct| snapshot_physical_expr(Arc::clone(conjunct)).is_ok())
                .cloned(),
        );
        let mut leaf = self.clone();
        leaf.routes = Arc::new(self.routes.selected_by(
            &static_predicate,
            &self.schema(),
            self.time_axis.column(),
        ));
        leaf.predicate = Some(predicate);
        Ok(propagation.with_updated_node(Arc::new(leaf)))
    }

    /// Returns the stable diagnostic operator name.
    fn name(&self) -> &'static str {
        "LiveScribeExec"
    }

    /// Returns the closure schema and the session's target partitions.
    fn properties(&self) -> &Arc<PlanProperties> {
        &self.properties
    }

    /// Returns the fragment open and wait times every executed partition recorded.
    fn metrics(&self) -> Option<MetricsSet> {
        Some(self.metrics.clone_inner())
    }

    /// A live source is always a leaf.
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
    ) -> DataFusionResult<Arc<dyn ExecutionPlan>> {
        if children.is_empty() {
            Ok(self)
        } else {
            Err(DataFusionError::Plan(
                "live Scribe source must remain a leaf".to_owned(),
            ))
        }
    }

    /// Streams this partition's routes' fragments, one route after another,
    /// as their batches arrive.
    ///
    /// Nothing is opened until the returned stream is first polled. Then the
    /// pushed predicate is snapshotted once, by [`LiveScribeExec::resolved`],
    /// so a resolved scalar or a join's completed dynamic filter ships to
    /// every Scribe as a literal and narrows this partition's routes. The
    /// plan keeps the runtime-bound predicate, so a re-execution snapshots its
    /// new values. Dropping the stream drops the open fragment stream, which
    /// cancels the Scribe fragment and releases its snapshot and lease. See
    /// [`LiveFragmentRead`] for how each outcome maps to the query terminal.
    ///
    /// # Errors
    /// Returns a plan error for a partition this leaf does not advertise and
    /// an execution error when the task carries no bound execution bindings;
    /// the stream fails with the snapshot error when the predicate reads
    /// execution state not yet populated.
    fn execute(
        &self,
        partition: usize,
        context: Arc<TaskContext>,
    ) -> DataFusionResult<SendableRecordBatchStream> {
        let partitions = self.properties.partitioning.partition_count();
        if partition >= partitions {
            return Err(DataFusionError::Plan(format!(
                "live Scribe source has no partition {partition}"
            )));
        }
        let lock = super::bindings::bindings_for_task(context.as_ref())?;
        let read = LiveFragmentRead {
            leaf: self.clone(),
            partition,
            lock,
            open_time: MetricBuilder::new(&self.metrics)
                .subset_time("fragment_open_time", partition),
            wait_time: MetricBuilder::new(&self.metrics)
                .subset_time("fragment_wait_time", partition),
        };
        Ok(Box::pin(RecordBatchStreamAdapter::new(
            self.schema(),
            read.into_stream(),
        )))
    }
}

/// Physical optimizer rule that keeps published work distributable beside a live leaf.
///
/// A [`LiveScribeExec`] is capped at one leader task, and the distributed
/// planner reconciles a whole stage to its smallest cap, so a union holding
/// the live leaf would pull every published sibling onto the leader. Placing
/// a `CoalescePartitionsExec` above each sibling without a live leaf makes the
/// planner open a separate coalesce stage for it: published scans keep their
/// cut-derived worker tasks, while the union, the live leaf, and everything
/// above stay in the leader stage. A sibling whose stage is one task is
/// elided back into the leader by the planner itself.
///
/// Only the analytical planning session installs it; the rule is a no-op for
/// plans without a live leaf.
#[derive(Debug)]
pub(super) struct LiveUnionBoundary;

impl datafusion::physical_optimizer::PhysicalOptimizerRule for LiveUnionBoundary {
    /// Wraps every non-live sibling of each union that holds a live leaf.
    ///
    /// # Errors
    ///
    /// Returns a `DataFusion` error when a union cannot be rebuilt over its
    /// wrapped children.
    fn optimize(
        &self,
        plan: Arc<dyn ExecutionPlan>,
        _config: &datafusion::config::ConfigOptions,
    ) -> DataFusionResult<Arc<dyn ExecutionPlan>> {
        let holds_live = |plan: &Arc<dyn ExecutionPlan>| {
            plan.exists(|node| Ok(node.downcast_ref::<LiveScribeExec>().is_some()))
        };
        plan.transform_up(|node| {
            if node
                .downcast_ref::<datafusion::physical_plan::union::UnionExec>()
                .is_none()
            {
                return Ok(Transformed::no(node));
            }
            let mut live = false;
            let mut children = Vec::with_capacity(node.children().len());
            for child in node.children() {
                if holds_live(child)? {
                    live = true;
                    children.push(Arc::clone(child));
                } else {
                    children.push(Arc::new(
                        datafusion::physical_plan::coalesce_partitions::CoalescePartitionsExec::new(
                            Arc::clone(child),
                        ),
                    ) as Arc<dyn ExecutionPlan>);
                }
            }
            if !live {
                return Ok(Transformed::no(node));
            }
            node.replace_children(
                children,
                datafusion::physical_plan::ReplaceChildrenOptions::new(
                    datafusion::physical_plan::ChildrenPropertiesMode::Recompute,
                ),
            )
            .map(Transformed::yes)
        })
        .map(|transformed| transformed.data)
    }

    /// Names the rule in `DataFusion` optimizer traces.
    fn name(&self) -> &'static str {
        "bifrost_live_union_boundary"
    }

    /// Asks `DataFusion` to verify the rewrite preserved the plan schema.
    fn schema_check(&self) -> bool {
        true
    }
}

/// One partition's fragment reads, from dispatch through footer validation.
struct LiveFragmentRead {
    /// Leaf whose closure the fragments are signed for, its predicate not
    /// yet snapshotted.
    leaf: LiveScribeExec,
    /// The output partition whose share of the routes this read covers.
    partition: usize,
    /// Execution bindings bound after admission.
    lock: Arc<super::bindings::OracleExecutionLock>,
    /// Time spent opening fragments until each peer accepted its stream,
    /// channel connects included.
    open_time: Time,
    /// Time spent awaiting the next frame of an opened fragment.
    wait_time: Time,
}

impl LiveFragmentRead {
    /// Turns the read into a batch stream `DataFusion` polls incrementally,
    /// reading this partition's routes one after another.
    ///
    /// Cancellation, an elapsed deadline, any dispatch failure — including a
    /// lost capture, before or after a row — a malformed or out-of-order
    /// frame, and end of stream without a valid footer fail the partition.
    /// A route's capture is never reopened or replaced.
    ///
    /// A partition dropped before its end — because the plan no longer needs
    /// it — owes no footer; the drop alone cancels the Scribe fragment.
    fn into_stream(self) -> impl futures_util::Stream<Item = DataFusionResult<RecordBatch>> {
        async_stream::try_stream! {
            let bindings = self.lock.get().ok_or_else(|| {
                DataFusionError::Execution("Oracle execution bindings are not bound".to_owned())
            })?;
            let grant = bindings.grant();
            grant.ensure_live()?;
            let live = bindings.live()?;
            let (leaf, routes) = self.leaf.resolved(self.partition)?;
            let schema = leaf.schema();
            'routes: for route in &routes {
                let (fragment, candidate) = leaf
                    .fragment(route, grant.deadline.timestamp_millis())?;
                let mut decoder = LiveFrameDecoder::new(
                    fragment.plan_fingerprint.clone(),
                    Arc::clone(&leaf.scan_metrics),
                );
                let open = self.open_time.timer();
                let opened = live
                    .dispatcher
                    .open_stream(&live.context, &fragment, &candidate)
                    .await;
                open.done();
                let mut frames = match opened {
                    Ok(frames) => frames,
                    Err(error) => {
                        grant.ensure_live()?;
                        Err(live_error("open", &error))?
                    }
                };
                loop {
                    let wait = self.wait_time.timer();
                    let frame = tokio::select! {
                        () = grant.cancellation.cancelled() => Err(DataFusionError::Execution(
                            "Oracle query was cancelled during a live Scribe read".to_owned(),
                        )),
                        // Typed, so the terminal reports a timeout whichever of
                        // this timer and the leader's own deadline fires first.
                        () = tokio::time::sleep(remaining(grant.deadline)) => Err(DataFusionError::Context(
                            "Oracle query deadline elapsed during a live Scribe read".to_owned(),
                            Box::new(QueryCatalogError::external(BifrostError::QueryTimeout)),
                        )),
                        frame = frames.next() => Ok(frame),
                    };
                    wait.done();
                    let frame = frame?;
                    match frame {
                        None => {
                            if !decoder.completed() {
                                Err(DataFusionError::Execution(
                                    "live Scribe fragment ended without a valid footer".to_owned(),
                                ))?;
                            }
                            continue 'routes;
                        }
                        Some(Err(error)) => {
                            grant.ensure_live()?;
                            Err(live_error("stream", &error))?;
                        }
                        Some(Ok(frame)) => {
                            if let Some(batch) = decoder.accept(frame).map_err(|error| {
                                DataFusionError::Execution(format!(
                                    "live Scribe fragment violated the attempt protocol: {error}"
                                ))
                            })? {
                                let batch = super::exec::project_batch(&batch, SchemaRef::clone(&schema))?;
                                #[cfg(feature = "test-support")]
                                LIVE_SOURCE_BATCHES.fetch_add(1, std::sync::atomic::Ordering::AcqRel);
                                yield batch;
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Validated live batches this process's Oracle live sources have handed to
/// `DataFusion`, across every query.
#[cfg(feature = "test-support")]
static LIVE_SOURCE_BATCHES: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// Returns how many validated live batches have reached an Oracle live source
/// in this process.
#[cfg(feature = "test-support")]
#[must_use]
pub fn live_source_batches_for_test() -> usize {
    LIVE_SOURCE_BATCHES.load(std::sync::atomic::Ordering::Acquire)
}

/// Returns the time left before `deadline`, zero once it has passed.
fn remaining(deadline: DateTime<Utc>) -> Duration {
    deadline
        .signed_duration_since(Utc::now())
        .to_std()
        .unwrap_or_default()
}

/// Maps a fatal live-source failure to the `DataFusion` error the terminal maps.
///
/// A foreign-tenant refusal keeps its typed tenant-invariant identity and a
/// Scribe capacity refusal becomes a resource refusal; every other failure is
/// an execution failure.
fn live_error(stage: &str, error: &DispatchError) -> DataFusionError {
    match error {
        DispatchError::TenantInvariant => {
            QueryCatalogError::external(BifrostError::QueryTenantInvariant)
        }
        DispatchError::Capacity => DataFusionError::ResourcesExhausted(format!(
            "live Scribe fragment {stage} was refused for capacity"
        )),
        _ => DataFusionError::Execution(format!("live Scribe fragment {stage} failed: {error}")),
    }
}

/// Closed reason one live frame sequence failed validation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
enum LiveFrameError {
    /// A frame arrived after the footer or in-process completion.
    #[error("frame after footer")]
    AfterFooter,
    /// The schema frame was repeated, or a batch preceded it.
    #[error("schema frame out of order")]
    Schema,
    /// A batch frame was not exactly one valid Arrow IPC batch.
    #[error("batch frame is not one Arrow IPC batch")]
    Batch,
    /// A running counter overflowed.
    #[error("counter overflow")]
    Overflow,
    /// The footer or in-process completion contradicts the delivered output
    /// or the signed fragment.
    #[error("footer does not validate")]
    Footer,
}

/// Incremental validator for one fragment's frames.
///
/// Remote frames are checked exactly as the whole-attempt buffer checks them —
/// one leading schema, counted bytes and rows, the in-order payload digest,
/// and a footer naming the signed fragment — but each batch is released as
/// soon as it decodes instead of retaining the attempt. In-process batches are
/// already Arrow: their rows and native bytes are counted and they are
/// released as they are, and the in-process completion closes the fragment
/// only when its fingerprint and totals match, as a footer does.
struct LiveFrameDecoder {
    /// Fingerprint the ticket was minted over and the footer must name.
    plan_fingerprint: String,
    /// Whether the leading schema frame arrived.
    schema_seen: bool,
    /// Bytes delivered so far: encoded frame bytes from a remote fragment,
    /// native Arrow bytes from an in-process one.
    bytes: u64,
    /// Rows decoded from delivered batch frames.
    rows: u64,
    /// Digest over batch frames in stream order.
    payload_hash: Sha256,
    /// Whether a valid footer or the in-process completion closed the fragment.
    completed: bool,
    /// Leaf accumulator the validated completion's scan evidence folds into.
    scan_metrics: Arc<RemoteScanMetrics>,
}

impl LiveFrameDecoder {
    /// Starts validating one fragment signed under `plan_fingerprint`, folding
    /// its validated scan evidence into `scan_metrics`.
    fn new(plan_fingerprint: String, scan_metrics: Arc<RemoteScanMetrics>) -> Self {
        Self {
            plan_fingerprint,
            schema_seen: false,
            bytes: 0,
            rows: 0,
            payload_hash: Sha256::new(),
            completed: false,
            scan_metrics,
        }
    }

    /// Rows already released to `DataFusion`.
    #[cfg(test)]
    const fn rows(&self) -> u64 {
        self.rows
    }

    /// Reports whether a valid footer or the in-process completion closed
    /// the fragment.
    const fn completed(&self) -> bool {
        self.completed
    }

    /// Validates one frame and returns the batch it carried, if any.
    ///
    /// A validated footer or in-process completion closes the fragment and
    /// folds its scan evidence into the leaf's accumulator exactly once;
    /// every later frame is refused, so the fold cannot repeat.
    ///
    /// # Errors
    ///
    /// Returns [`LiveFrameError`] for a frame after completion, a repeated or
    /// late schema, a batch that is not one Arrow IPC batch, counter overflow,
    /// or a footer or in-process completion whose fragment, digest, byte
    /// count, row count, or completion flag contradicts what was delivered.
    fn accept(&mut self, frame: LiveFrame) -> Result<Option<RecordBatch>, LiveFrameError> {
        if self.completed {
            return Err(LiveFrameError::AfterFooter);
        }
        let frame = match frame {
            LiveFrame::Wire(frame) => frame,
            LiveFrame::Batch(batch) => {
                self.add_rows(batch.num_rows())?;
                self.add_bytes(batch.get_array_memory_size())?;
                return Ok(Some(batch));
            }
            LiveFrame::Complete(completion) => {
                if completion.plan_fingerprint != self.plan_fingerprint
                    || completion.rows != self.rows
                    || completion.bytes != self.bytes
                {
                    return Err(LiveFrameError::Footer);
                }
                self.completed = true;
                self.scan_metrics.record_footer(completion.scan_stats);
                return Ok(None);
            }
        };
        match frame {
            WorkerAttemptFrame::Schema(bytes) => {
                if self.schema_seen {
                    return Err(LiveFrameError::Schema);
                }
                self.schema_seen = true;
                self.add_bytes(bytes.len())?;
                Ok(None)
            }
            WorkerAttemptFrame::Batch(bytes) => {
                if !self.schema_seen {
                    return Err(LiveFrameError::Schema);
                }
                self.add_bytes(bytes.len())?;
                self.payload_hash.update(&bytes);
                let mut reader =
                    arrow::ipc::reader::StreamReader::try_new(std::io::Cursor::new(bytes), None)
                        .map_err(|_| LiveFrameError::Batch)?;
                let batch = reader
                    .next()
                    .ok_or(LiveFrameError::Batch)?
                    .map_err(|_| LiveFrameError::Batch)?;
                if reader.next().is_some() {
                    return Err(LiveFrameError::Batch);
                }
                self.add_rows(batch.num_rows())?;
                Ok(Some(batch))
            }
            WorkerAttemptFrame::Footer(footer) => {
                let digest = hex::encode(self.payload_hash.clone().finalize());
                if !self.schema_seen
                    || !footer.completed
                    || footer.fragment_id != self.plan_fingerprint
                    || footer.manifest_digest.as_str() != self.plan_fingerprint
                    || footer.encoded_bytes != self.bytes
                    || footer.row_count != self.rows
                    || footer.payload_digest.as_str() != digest
                {
                    return Err(LiveFrameError::Footer);
                }
                self.completed = true;
                self.scan_metrics.record_footer(footer.scan_stats);
                Ok(None)
            }
        }
    }

    /// Adds one released batch's rows to the running row count.
    ///
    /// # Errors
    /// Returns [`LiveFrameError::Overflow`] when the count cannot grow.
    fn add_rows(&mut self, rows: usize) -> Result<(), LiveFrameError> {
        self.rows = self
            .rows
            .checked_add(u64::try_from(rows).map_err(|_| LiveFrameError::Overflow)?)
            .ok_or(LiveFrameError::Overflow)?;
        Ok(())
    }

    /// Adds one frame's encoded length, or one native batch's Arrow size, to
    /// the running byte count.
    ///
    /// # Errors
    /// Returns [`LiveFrameError::Overflow`] when the count cannot grow.
    fn add_bytes(&mut self, len: usize) -> Result<(), LiveFrameError> {
        self.bytes = self
            .bytes
            .checked_add(u64::try_from(len).map_err(|_| LiveFrameError::Overflow)?)
            .ok_or(LiveFrameError::Overflow)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use arrow::array::Int64Array;
    use arrow::datatypes::{DataType, Field, Schema};
    use wyrd_spec::vala::api::WorkerScanStats;

    use super::*;
    use crate::oracle::dispatcher::{NativeCompletion, NativeOutputTally};

    /// A query filter pushed to the live leaf ships on every fragment above
    /// the placeholder, while the leader keeps its own `FilterExec`.
    ///
    /// # Panics
    /// Panics when pushdown drops the leader's filter, leaves the leaf without
    /// the predicate, or the fragment's encoded root is not a filter.
    #[test]
    fn pushed_filter_ships_on_the_fragment_and_stays_on_the_leader() {
        use datafusion::physical_expr::expressions::{BinaryExpr, Column, Literal};
        use datafusion::physical_optimizer::PhysicalOptimizerRule as _;
        use datafusion::physical_optimizer::filter_pushdown::FilterPushdown;
        use datafusion_proto::protobuf::{PhysicalPlanNode, physical_plan_node::PhysicalPlanType};
        use prost::Message as _;

        let schema = Arc::new(Schema::new(vec![Field::new("v", DataType::Int64, false)]));
        let leaf = Arc::new(LiveScribeExec::new(
            crate::oracle::exec::tests::one_live_route(wyrd_spec::DataTenantId::new_v7()),
            crate::catalog::TimeAxis::new(
                crate::oracle::exec::tests::FIXTURE_TIME_COLUMN,
                crate::catalog::TimeGranularity::Hour,
            ),
            "f".repeat(64),
            vec!["v".to_owned()],
            Arc::clone(&schema),
            1,
        ));
        let predicate: Arc<dyn PhysicalExpr> = Arc::new(BinaryExpr::new(
            Arc::new(Column::new("v", 0)),
            datafusion::logical_expr::Operator::Gt,
            Arc::new(Literal::new(3_i64.into())),
        ));
        let plan = FilterPushdown::new()
            .optimize(
                Arc::new(FilterExec::try_new(predicate, leaf).expect("filter plans")),
                &ConfigOptions::default(),
            )
            .expect("pushdown runs");

        let filter = plan
            .downcast_ref::<FilterExec>()
            .expect("the leader keeps its filter");
        let live = filter
            .input()
            .downcast_ref::<LiveScribeExec>()
            .expect("the live leaf stays under the filter");
        assert!(live.predicate.is_some(), "the leaf took the pushed filter");
        let (fragment, _) = live
            .fragment(&live.routes.routes[0], 0)
            .expect("the fragment encodes");
        let root = PhysicalPlanNode::decode(fragment.physical_plan_bytes.as_slice())
            .expect("the fragment plan decodes");
        assert!(
            matches!(root.physical_plan_type, Some(PhysicalPlanType::Filter(_))),
            "the fragment ships the filter above its placeholder"
        );
    }

    /// A scalar-subquery conjunct stays on the live leaf without deferring
    /// static pruning, and each execution ships its resolved value.
    ///
    /// Over four hourly routes, planning prunes by the static
    /// `event_time >= H1` alone while the scalar is pending. Execution then
    /// fails while the scalar is pending, and once populated with H2 snapshots
    /// it to a literal: the routes narrow to H2 and H3, are dealt across two
    /// partitions exactly once each, and the fragment carries no scalar
    /// subquery. A reset and re-execution with H3 resolves anew, because the
    /// planned leaf keeps the runtime-bound expression.
    ///
    /// # Panics
    /// Panics when a fixture cannot be built, or pruning, snapshotting,
    /// dealing, or the encoded fragment differ from the above.
    #[test]
    fn scalar_subquery_resolves_per_execution_after_static_pruning() {
        use super::super::exec::tests::FIXTURE_TIME_COLUMN;
        use arrow::datatypes::TimeUnit;
        use datafusion::logical_expr::Operator;
        use datafusion::logical_expr::physical_planning_context::{
            ScalarSubqueryResults, SubqueryIndex,
        };
        use datafusion::physical_expr::expressions::{BinaryExpr, Column, Literal};
        use datafusion::physical_expr::scalar_subquery::ScalarSubqueryExpr;
        use datafusion::physical_optimizer::PhysicalOptimizerRule as _;
        use datafusion::physical_optimizer::filter_pushdown::FilterPushdown;
        use datafusion::scalar::ScalarValue;
        use datafusion_proto::protobuf::PhysicalPlanNode;
        use prost::Message as _;

        /// One hour in microseconds, the width of each route's partition.
        const HOUR_MICROS: i64 = 3_600_000_000;
        let first_hour = 1_787_493_600_000_000_i64;
        let zone: Option<Arc<str>> = Some("UTC".into());
        let hour = |n: i64| {
            ScalarValue::TimestampMicrosecond(Some(first_hour + n * HOUR_MICROS), zone.clone())
        };
        let mut routes =
            crate::oracle::exec::tests::one_live_route(wyrd_spec::DataTenantId::new_v7());
        let template = routes.routes[0].clone();
        routes.routes = (0..4)
            .map(|n| LiveScribeRoute {
                time_partition: TimePartitionWire::new(
                    wyrd_spec::vala::api::TimeGranularityWire::Hour,
                    DateTime::from_timestamp_micros(first_hour + n * HOUR_MICROS).expect("instant"),
                )
                .expect("hour boundary"),
                ..template.clone()
            })
            .collect();
        let all = routes.routes.clone();
        let time_type = DataType::Timestamp(TimeUnit::Microsecond, zone.clone());
        let schema: SchemaRef = Arc::new(Schema::new(vec![
            Field::new("v", DataType::Int64, false),
            Field::new(FIXTURE_TIME_COLUMN, time_type.clone(), false),
        ]));
        let leaf = Arc::new(LiveScribeExec::new(
            routes,
            crate::catalog::TimeAxis::new(
                FIXTURE_TIME_COLUMN,
                crate::catalog::TimeGranularity::Hour,
            ),
            "f".repeat(64),
            vec!["v".to_owned(), FIXTURE_TIME_COLUMN.to_owned()],
            Arc::clone(&schema),
            2,
        ));
        let results = ScalarSubqueryResults::new(1);
        let at_least = |right: Arc<dyn PhysicalExpr>| -> Arc<dyn PhysicalExpr> {
            Arc::new(BinaryExpr::new(
                Arc::new(Column::new(FIXTURE_TIME_COLUMN, 1)),
                Operator::GtEq,
                right,
            ))
        };
        let predicate = Arc::new(BinaryExpr::new(
            at_least(Arc::new(Literal::new(hour(1)))),
            Operator::And,
            at_least(Arc::new(ScalarSubqueryExpr::new(
                time_type,
                false,
                SubqueryIndex::new(0),
                results.clone(),
            ))),
        ));
        let plan = FilterPushdown::new()
            .optimize(
                Arc::new(FilterExec::try_new(predicate, leaf).expect("filter plans")),
                &ConfigOptions::default(),
            )
            .expect("pushdown runs");
        let live = plan
            .downcast_ref::<FilterExec>()
            .expect("the leader keeps its filter")
            .input()
            .downcast_ref::<LiveScribeExec>()
            .expect("the live leaf stays under the filter");
        assert_eq!(
            live.routes.routes,
            all[1..],
            "planning prunes by the static conjunct"
        );
        assert!(live.resolved(0).is_err(), "a pending scalar cannot resolve");

        for (value, expected) in [(2, &all[2..]), (3, &all[3..])] {
            results.clear();
            results
                .set(SubqueryIndex::new(0), hour(value))
                .expect("scalar populates");
            let (resolved, mut dealt) = live.resolved(0).expect("the populated scalar resolves");
            dealt.extend(live.resolved(1).expect("the populated scalar resolves").1);
            dealt.sort_by_key(|route| route.time_partition.start_unix_micros());
            assert_eq!(
                dealt, expected,
                "the scalar narrows routes, each dealt exactly once"
            );
            let (fragment, _) = resolved
                .fragment(&expected[0], 0)
                .expect("the fragment encodes");
            let encoded = format!(
                "{:?}",
                PhysicalPlanNode::decode(fragment.physical_plan_bytes.as_slice())
                    .expect("the fragment plan decodes")
            );
            assert!(
                !encoded.contains("ScalarSubquery"),
                "no scalar subquery ships"
            );
            let micros = first_hour + value * HOUR_MICROS;
            assert!(
                encoded.contains(&micros.to_string()),
                "the resolved literal ships"
            );
        }
    }

    /// A time-column predicate keeps only the routes whose hour it can match,
    /// at microsecond and nanosecond precision, and a predicate on any other
    /// column keeps every route.
    ///
    /// # Panics
    /// Panics when a fixture partition cannot be built or the selection keeps
    /// the wrong routes.
    #[test]
    fn time_predicate_selects_only_overlapping_routes() {
        use super::super::exec::tests::FIXTURE_TIME_COLUMN;
        use arrow::datatypes::TimeUnit;
        use datafusion::physical_expr::expressions::{BinaryExpr, Column, Literal};
        use datafusion::scalar::ScalarValue;

        /// One hour in microseconds, the width of each fixture route's
        /// time partition.
        const HOUR_MICROS: i64 = 3_600_000_000;
        let first_hour = 1_787_493_600_000_000_i64;
        let mut routes =
            crate::oracle::exec::tests::one_live_route(wyrd_spec::DataTenantId::new_v7());
        let mut later = routes.routes[0].clone();
        later.time_partition = TimePartitionWire::new(
            wyrd_spec::vala::api::TimeGranularityWire::Hour,
            DateTime::from_timestamp_micros(first_hour + HOUR_MICROS).expect("instant"),
        )
        .expect("hour boundary");
        routes.routes.push(later.clone());
        let zone: Option<Arc<str>> = Some("UTC".into());
        let schema: SchemaRef = Arc::new(Schema::new(vec![
            Field::new("v", DataType::Int64, false),
            Field::new(
                FIXTURE_TIME_COLUMN,
                DataType::Timestamp(TimeUnit::Microsecond, zone.clone()),
                false,
            ),
        ]));
        let at_least = |column: &str, index: usize, value: ScalarValue| -> Arc<dyn PhysicalExpr> {
            Arc::new(BinaryExpr::new(
                Arc::new(Column::new(column, index)),
                datafusion::logical_expr::Operator::GtEq,
                Arc::new(Literal::new(value)),
            ))
        };

        let in_later_hour = at_least(
            FIXTURE_TIME_COLUMN,
            1,
            ScalarValue::TimestampMicrosecond(Some(first_hour + HOUR_MICROS), zone.clone()),
        );
        assert_eq!(
            routes
                .selected_by(&in_later_hour, &schema, FIXTURE_TIME_COLUMN)
                .routes,
            vec![later.clone()]
        );

        // The last nanosecond of the first hour still matches it, so the
        // nanosecond bounds cover the whole partition.
        let nanos_schema: SchemaRef = Arc::new(Schema::new(vec![
            Field::new("v", DataType::Int64, false),
            Field::new(
                FIXTURE_TIME_COLUMN,
                DataType::Timestamp(TimeUnit::Nanosecond, zone.clone()),
                false,
            ),
        ]));
        let last_nano_of_first_hour = at_least(
            FIXTURE_TIME_COLUMN,
            1,
            ScalarValue::TimestampNanosecond(
                Some((first_hour + HOUR_MICROS) * 1_000 - 1),
                zone.clone(),
            ),
        );
        assert_eq!(
            routes
                .selected_by(&last_nano_of_first_hour, &nanos_schema, FIXTURE_TIME_COLUMN)
                .routes,
            routes.routes
        );
        let in_later_hour_nanos = at_least(
            FIXTURE_TIME_COLUMN,
            1,
            ScalarValue::TimestampNanosecond(Some((first_hour + HOUR_MICROS) * 1_000), zone),
        );
        assert_eq!(
            routes
                .selected_by(&in_later_hour_nanos, &nanos_schema, FIXTURE_TIME_COLUMN)
                .routes,
            vec![later]
        );

        let unrelated = at_least("v", 0, ScalarValue::Int64(Some(3)));
        assert_eq!(
            routes
                .selected_by(&unrelated, &schema, FIXTURE_TIME_COLUMN)
                .routes,
            routes.routes
        );
    }

    /// Encodes `rows` through the Scribe's own attempt encoder.
    fn frames(fingerprint: &str, rows: &[i64]) -> Vec<WorkerAttemptFrame> {
        let schema = Arc::new(Schema::new(vec![Field::new("v", DataType::Int64, false)]));
        let batch = RecordBatch::try_new(
            Arc::clone(&schema),
            vec![Arc::new(Int64Array::from(rows.to_vec()))],
        )
        .expect("fixture batch");
        let mut encoder = super::super::dispatcher::AttemptEncoder::default();
        let mut out = vec![encoder.start(schema).expect("schema frame")];
        out.push(encoder.encode(&batch).expect("batch frame").1);
        out.push(
            encoder
                .finish_physical(fingerprint, WorkerScanStats::default())
                .expect("footer"),
        );
        out
    }

    /// A valid fragment releases its batch before the footer and then
    /// completes; a tampered footer, a frame after the footer, and a batch
    /// before the schema are each refused. An in-process fragment releases
    /// its Arrow batch as is, completes on its completion item, and refuses
    /// anything after it.
    ///
    /// # Panics
    /// Panics when a fixture batch or attempt frame cannot be built, when a
    /// valid frame is refused or not released, or when a tampered footer, a
    /// frame after completion, or a batch before the schema is accepted.
    #[test]
    fn live_frames_release_batches_incrementally_and_validate_the_footer() {
        let wire = |frame: Option<WorkerAttemptFrame>| LiveFrame::Wire(frame.expect("frame"));
        let fingerprint = "a".repeat(64);
        let mut valid = frames(&fingerprint, &[1, 2, 3]).into_iter();
        let mut decoder = LiveFrameDecoder::new(fingerprint.clone(), Arc::default());
        assert_eq!(decoder.accept(wire(valid.next())), Ok(None));
        let batch = decoder
            .accept(wire(valid.next()))
            .expect("batch validates")
            .expect("batch is released before the footer");
        assert_eq!(
            (batch.num_rows(), decoder.rows(), decoder.completed()),
            (3, 3, false)
        );
        let footer = valid.next().expect("footer");
        assert_eq!(decoder.accept(LiveFrame::Wire(footer.clone())), Ok(None));
        assert!(decoder.completed());
        assert_eq!(
            decoder.accept(LiveFrame::Wire(footer)),
            Err(LiveFrameError::AfterFooter)
        );

        let mut foreign = frames(&"b".repeat(64), &[1]).into_iter();
        let mut decoder = LiveFrameDecoder::new(fingerprint.clone(), Arc::default());
        decoder.accept(wire(foreign.next())).expect("schema");
        decoder.accept(wire(foreign.next())).expect("batch");
        assert_eq!(
            decoder.accept(wire(foreign.next())),
            Err(LiveFrameError::Footer)
        );

        let mut early = frames(&fingerprint, &[1]).into_iter().skip(1);
        let mut decoder = LiveFrameDecoder::new(fingerprint.clone(), Arc::default());
        assert_eq!(
            decoder.accept(wire(early.next())),
            Err(LiveFrameError::Schema)
        );

        let schema = Arc::new(Schema::new(vec![Field::new("v", DataType::Int64, false)]));
        let local = RecordBatch::try_new(schema, vec![Arc::new(Int64Array::from(vec![4, 5]))])
            .expect("fixture batch");
        let mut decoder = LiveFrameDecoder::new(fingerprint.clone(), Arc::default());
        let mut tally = NativeOutputTally::default();
        tally.record(&local).expect("tally grows");
        let released = decoder
            .accept(LiveFrame::Batch(local.clone()))
            .expect("in-process batch is accepted")
            .expect("in-process batch is released");
        assert_eq!(released, local);
        assert_eq!((decoder.rows(), decoder.completed()), (2, false));
        assert_eq!(
            decoder.accept(tally.complete(fingerprint, WorkerScanStats::default())),
            Ok(None)
        );
        assert!(decoder.completed());
        assert_eq!(
            decoder.accept(LiveFrame::Batch(local)),
            Err(LiveFrameError::AfterFooter)
        );
    }

    /// An in-process completion is accepted only when it matches what the
    /// decoder itself counted.
    ///
    /// The producer's own tally closes an empty and a nonempty fragment, and
    /// the decoder accepts both, with nonzero native bytes for the nonempty
    /// one. A completion whose rows, bytes, or fingerprint were altered is
    /// refused as a contradicting footer, and a repeated completion or a
    /// trailing batch after completion is refused as out of order.
    ///
    /// # Panics
    /// Panics when the fixture batch or tally cannot be built, a matching
    /// completion is refused, or a contradicting or out-of-order frame is
    /// accepted.
    #[test]
    fn native_completion_reconciles_the_delivered_output() {
        let fingerprint = "a".repeat(64);
        let schema = Arc::new(Schema::new(vec![Field::new("v", DataType::Int64, false)]));
        let batch = RecordBatch::try_new(schema, vec![Arc::new(Int64Array::from(vec![4, 5, 6]))])
            .expect("fixture batch");
        let completion = |batches: &[&RecordBatch]| {
            let mut tally = NativeOutputTally::default();
            for batch in batches {
                tally.record(batch).expect("tally grows");
            }
            match tally.complete(fingerprint.clone(), WorkerScanStats::default()) {
                LiveFrame::Complete(completion) => completion,
                _ => panic!("the tally closes with a completion"),
            }
        };

        let mut decoder = LiveFrameDecoder::new(fingerprint.clone(), Arc::default());
        let empty = completion(&[]);
        assert_eq!((empty.rows, empty.bytes), (0, 0));
        assert_eq!(decoder.accept(LiveFrame::Complete(empty.clone())), Ok(None));
        assert!(decoder.completed());
        assert_eq!(
            decoder.accept(LiveFrame::Complete(empty)),
            Err(LiveFrameError::AfterFooter),
            "a repeated completion is refused"
        );

        let full = completion(&[&batch]);
        assert_eq!(full.rows, 3);
        assert!(full.bytes > 0, "native bytes are counted");
        let delivered = || {
            let mut decoder = LiveFrameDecoder::new(fingerprint.clone(), Arc::default());
            decoder
                .accept(LiveFrame::Batch(batch.clone()))
                .expect("batch is accepted")
                .expect("batch is released");
            decoder
        };
        let mut decoder = delivered();
        assert_eq!(decoder.accept(LiveFrame::Complete(full.clone())), Ok(None));
        assert_eq!(
            decoder.accept(LiveFrame::Batch(batch.clone())),
            Err(LiveFrameError::AfterFooter),
            "a trailing batch is refused"
        );

        let altered = [
            NativeCompletion {
                rows: full.rows + 1,
                ..full.clone()
            },
            NativeCompletion {
                bytes: full.bytes - 1,
                ..full.clone()
            },
            NativeCompletion {
                plan_fingerprint: "b".repeat(64),
                ..full
            },
        ];
        for completion in altered {
            let mut decoder = delivered();
            assert_eq!(
                decoder.accept(LiveFrame::Complete(completion)),
                Err(LiveFrameError::Footer)
            );
            assert!(
                !decoder.completed(),
                "a contradicting completion never closes"
            );
        }
    }

    /// A live read over a staged run reports its staged bytes to the query's
    /// scan stats, over both the remote footer and the in-process completion,
    /// while a memtable-only read reports none.
    ///
    /// The Scribe scans staged runs through its own hot-Parquet leaf and
    /// reports the bytes on the fragment's completion. The leader's collector
    /// walks the plan holding the live leaf, so each validated completion must
    /// reach it exactly once; a read whose Scribe scanned no Parquet leaves
    /// physical bytes absent rather than zero.
    ///
    /// # Panics
    /// Panics when a fixture batch or frame cannot be built, a valid frame is
    /// refused, or the collected physical bytes or file count are wrong.
    #[test]
    fn live_staged_read_reports_scanned_bytes_to_the_query() {
        let fingerprint = "a".repeat(64);
        let schema = Arc::new(Schema::new(vec![Field::new("v", DataType::Int64, false)]));
        let batch = RecordBatch::try_new(
            Arc::clone(&schema),
            vec![Arc::new(Int64Array::from(vec![1, 2, 3]))],
        )
        .expect("fixture batch");
        let binding = TenantTableBinding {
            tenant_id: wyrd_spec::DataTenantId::new_v7(),
            namespace: "bifrost".to_owned(),
            table: "events".to_owned(),
        };
        let leaf = || {
            LiveScribeExec::new(
                LiveTableRoutes {
                    captures: LiveCaptures::none_for_test(binding.clone()),
                    binding: binding.clone(),
                    routes: Vec::new(),
                },
                crate::catalog::TimeAxis::new(
                    crate::oracle::exec::tests::FIXTURE_TIME_COLUMN,
                    crate::catalog::TimeGranularity::Hour,
                ),
                "f".repeat(64),
                vec!["v".to_owned()],
                Arc::clone(&schema),
                1,
            )
        };
        let staged = |bytes| WorkerScanStats {
            bytes_scanned: Some(bytes),
            files_scanned: 1,
            partitions_scanned: 1,
            ..WorkerScanStats::default()
        };

        let live = leaf();
        let mut encoder = super::super::dispatcher::AttemptEncoder::default();
        let mut decoder =
            LiveFrameDecoder::new(fingerprint.clone(), Arc::clone(live.scan_metrics()));
        for frame in [
            encoder.start(Arc::clone(&schema)).expect("schema frame"),
            encoder.encode(&batch).expect("batch frame").1,
            encoder
                .finish_physical(&fingerprint, staged(4096))
                .expect("footer"),
        ] {
            decoder
                .accept(LiveFrame::Wire(frame))
                .expect("remote frame validates");
        }
        let mut decoder =
            LiveFrameDecoder::new(fingerprint.clone(), Arc::clone(live.scan_metrics()));
        let mut tally = NativeOutputTally::default();
        tally.record(&batch).expect("tally grows");
        decoder
            .accept(LiveFrame::Batch(batch.clone()))
            .expect("in-process batch validates");
        decoder
            .accept(tally.complete(fingerprint.clone(), staged(1024)))
            .expect("in-process completion validates");
        let mut stats = crate::oracle::exec::OracleQueryScanStats::from_plan(&live);
        stats.finalize();
        assert_eq!(
            (stats.physical_bytes_scanned, stats.files_scanned),
            (Some(5120), 2),
            "each staged fragment's bytes and file reach the query once"
        );

        let memtable = leaf();
        let mut decoder =
            LiveFrameDecoder::new(fingerprint.clone(), Arc::clone(memtable.scan_metrics()));
        decoder
            .accept(NativeOutputTally::default().complete(fingerprint, WorkerScanStats::default()))
            .expect("empty completion validates");
        let mut stats = crate::oracle::exec::OracleQueryScanStats::from_plan(&memtable);
        stats.finalize();
        assert_eq!(
            stats.physical_bytes_scanned, None,
            "a memtable read scans no Parquet"
        );
    }
}
