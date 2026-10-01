//! Leader-owned streaming source over the selected Scribe live routes.
//!
//! Oracle discovers each referenced table's reported Scribe streams before
//! its single physical planning pass, keeps only the routes safe event-time
//! pruning cannot exclude, and plants one [`LiveScribeExec`] per scan at the
//! session's target partitions, dealing its routes across them. Planning opens
//! nothing. At execution each partition dispatches one signed fragment per
//! route it owns, in turn, to that route's Scribe and hands every batch to
//! `DataFusion` as it arrives, so no live fragment is ever materialized on the
//! leader.
//!
//! A route that is unavailable before it yields a row is recorded as a
//! `LiveTail` loss and ends empty; any later loss, and any integrity, tenant,
//! security, resource, cancellation, or deadline fault, fails the query.

use std::sync::Arc;
use std::time::Duration;

use arrow::datatypes::SchemaRef;
use arrow::record_batch::RecordBatch;
use chrono::{DateTime, Utc};
use datafusion::common::tree_node::{Transformed, TreeNode as _, TreeNodeRecursion};
use datafusion::error::{DataFusionError, Result as DataFusionResult};
use datafusion::execution::TaskContext;
use datafusion::physical_expr::PhysicalExpr;
use datafusion::physical_plan::stream::RecordBatchStreamAdapter;
use datafusion::physical_plan::{
    DisplayAs, DisplayFormatType, ExecutionPlan, Partitioning, PlanProperties,
    SendableRecordBatchStream,
};
use futures_util::StreamExt as _;
use sha2::{Digest as _, Sha256};
use wyrd_spec::vala::api::{
    ClusterRole, FollowerReaderCut, FollowerScanAssignment, NodeId, PersistedFileAssignment,
    ScribeProviderCut, TenantTableBinding, TimePartitionWire, WorkerAttemptFrame,
};
use wyrd_spec::vala::assignment_authority::ScanPredicate;

use super::dispatcher::{
    DispatchCandidate, DispatchContext, DispatchError, FragmentDispatcher, LiveFrame,
    PhysicalDispatchFragment,
};
use super::exec::RemoteScanMetrics;
use super::{DegradedPartition, DegradedSourceAccumulator};
use crate::catalog::event_time::EventTimeStatistics;
use crate::catalog::layout::TimePartition;
use crate::oracle::pruning::EventTimeQueryInterval;

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
}

impl LiveScribeRoute {
    /// Reports whether this route can hold a row the query interval keeps.
    ///
    /// Partitions are laid out on `wyrd_event_time`, so a partition's rows all
    /// lie inside `[start, end)`. That interval is decided exactly as a hot
    /// file's recorded bounds are: only a proven empty intersection excludes,
    /// and an unconstrained query retains every route.
    fn is_selected_by(&self, interval: EventTimeQueryInterval) -> bool {
        let partition = TimePartition::from_wire(self.time_partition);
        let statistics = EventTimeStatistics::Bounded {
            min_micros: partition.start_unix_micros(),
            max_micros: partition.end_utc().timestamp_micros().saturating_sub(1),
        };
        !interval.decide(statistics).excludes()
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
    /// Stable table identity carried by each fragment's no-snapshot reader cut.
    pub(crate) table_uid: uuid::Uuid,
    /// Reported routes, in discovery order.
    pub(crate) routes: Vec<LiveScribeRoute>,
}

impl LiveTableRoutes {
    /// Returns the routes one scan must read under its closed predicates.
    ///
    /// Only `wyrd_event_time` leaves narrow the interval; every other predicate
    /// is unsafe for partition pruning and retains every reported route.
    pub(super) fn select(&self, predicates: &[ScanPredicate]) -> Vec<LiveScribeRoute> {
        let interval = EventTimeQueryInterval::from_predicates(predicates);
        self.routes
            .iter()
            .filter(|route| route.is_selected_by(interval))
            .cloned()
            .collect()
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

/// Records one known live source lost before it yielded a row.
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
    /// Fingerprint of the table's complete physical schema.
    schema_fingerprint: String,
    /// Signed projection closure, in closure order.
    required_columns: Vec<String>,
    /// Closed predicates the Scribe applies before it streams a batch.
    predicates: Vec<ScanPredicate>,
    /// Advertised closure schema and the session's target partitions.
    properties: Arc<PlanProperties>,
    /// Scan evidence every validated fragment completion folds in, shared by
    /// every clone of this leaf so the query's collector reads one total.
    scan_metrics: Arc<RemoteScanMetrics>,
}

impl LiveScribeExec {
    /// Plans one live leaf over already-selected routes at `partitions`
    /// output partitions, the planning session's target partitions.
    ///
    /// `routes.routes` must be non-empty; a scan with nothing selected plants no
    /// live leaf at all.
    pub(super) fn new(
        routes: LiveTableRoutes,
        schema_fingerprint: String,
        required_columns: Vec<String>,
        predicates: Vec<ScanPredicate>,
        schema: SchemaRef,
        partitions: usize,
    ) -> Self {
        let partitions = partitions.max(1);
        Self {
            routes: Arc::new(routes),
            schema_fingerprint,
            required_columns,
            predicates,
            properties: Arc::new(PlanProperties::new(
                datafusion::physical_expr::EquivalenceProperties::new(schema),
                Partitioning::UnknownPartitioning(partitions),
                datafusion::physical_plan::execution_plan::EmissionType::Incremental,
                datafusion::physical_plan::execution_plan::Boundedness::Bounded,
            )),
            scan_metrics: Arc::default(),
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

    /// Builds the signed fragment and destination for one route.
    ///
    /// The encoded plan is the same identity-bound placeholder a Scribe
    /// follower decodes for any hot-provider assignment; its fingerprint binds
    /// the ticket, the request, and the footer to one another.
    ///
    /// # Errors
    ///
    /// Returns a `DataFusion` plan error when the placeholder cannot be
    /// encoded.
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
        let physical_plan_bytes =
            datafusion_proto::bytes::physical_plan_to_bytes_with_extension_codec(
                Arc::new(super::codec::RemoteSourcePlaceholderExec::new(
                    scan_id.clone(),
                    &self.schema_fingerprint,
                    self.schema(),
                )),
                &super::codec::OraclePhysicalExtensionCodec::encoder(),
            )?
            .to_vec();
        let plan_fingerprint = super::codec::physical_plan_fingerprint(&physical_plan_bytes);
        let assignment = FollowerScanAssignment {
            scan_id,
            binding: binding.clone(),
            reader_cut: FollowerReaderCut::no_snapshot(self.routes.table_uid, route.writer_epoch),
            persisted: PersistedFileAssignment { files: Vec::new() },
            scribe_provider_cut: Some(ScribeProviderCut {
                writer_epoch: route.writer_epoch,
                start_partition: route.time_partition,
                end_partition: route.time_partition,
            }),
            schema_fingerprint: self.schema_fingerprint.clone(),
            required_columns: self.required_columns.clone(),
            predicates: self.predicates.clone(),
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
    /// This leaf owns no physical expression.
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
        "LiveScribeExec"
    }

    /// Returns the closure schema and the session's target partitions.
    fn properties(&self) -> &Arc<PlanProperties> {
        &self.properties
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
    /// Nothing is opened until the returned stream is first polled, and
    /// dropping it drops the open fragment stream, which cancels the Scribe
    /// fragment and releases its snapshot and lease. See [`LiveFragmentRead`]
    /// for how each outcome maps to the query terminal.
    ///
    /// # Errors
    /// Returns a plan error for a partition this leaf does not advertise, and
    /// an execution error when the task carries no bound execution bindings.
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
        let routes = self
            .routes
            .routes
            .iter()
            .skip(partition)
            .step_by(partitions)
            .cloned()
            .collect();
        let lock = super::bindings::bindings_for_task(context.as_ref())?;
        let read = LiveFragmentRead {
            leaf: self.clone(),
            routes,
            lock,
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
    /// Leaf whose closure the fragments are signed for.
    leaf: LiveScribeExec,
    /// The routes this partition reads, in order.
    routes: Vec<LiveScribeRoute>,
    /// Execution bindings bound after admission.
    lock: Arc<super::bindings::OracleExecutionLock>,
}

impl LiveFragmentRead {
    /// Turns the read into a batch stream `DataFusion` polls incrementally,
    /// reading this partition's routes one after another.
    ///
    /// Outcomes per route, in order of precedence:
    /// - cancellation or an elapsed deadline fails the partition;
    /// - an availability loss (`Unavailable`, `EligibleSourceLoss`,
    ///   `StaleObject`) before that route yielded a row records one
    ///   `LiveTail` loss and moves on to the next route;
    /// - every other dispatch failure, any loss after a row, a malformed or
    ///   out-of-order frame, and end of stream without a valid footer fail it.
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
            let schema = self.leaf.schema();
            'routes: for route in &self.routes {
                let (fragment, candidate) = self
                    .leaf
                    .fragment(route, grant.deadline.timestamp_millis())?;
                let mut decoder = LiveFrameDecoder::new(
                    fragment.plan_fingerprint.clone(),
                    Arc::clone(&self.leaf.scan_metrics),
                );
                let mut frames = match live
                    .dispatcher
                    .open_stream(&live.context, &fragment, &candidate)
                    .await
                {
                    Ok(frames) => frames,
                    Err(error) => {
                        grant.ensure_live()?;
                        if !is_availability_loss(&error) {
                            Err(live_error("open", &error))?;
                        }
                        tracing::warn!(
                            scribe = %route.node_id.as_uuid(),
                            ?error,
                            "Oracle omits a live Scribe route unavailable before its first row"
                        );
                        record_live_tail_unavailable(bindings.degraded());
                        continue 'routes;
                    }
                };
                loop {
                    let frame = tokio::select! {
                        () = grant.cancellation.cancelled() => Err(DataFusionError::Execution(
                            "Oracle query was cancelled during a live Scribe read".to_owned(),
                        )),
                        // Typed, so the terminal reports a timeout whichever of
                        // this timer and the leader's own deadline fires first.
                        () = tokio::time::sleep(remaining(grant.deadline)) => Err(DataFusionError::Context(
                            "Oracle query deadline elapsed during a live Scribe read".to_owned(),
                            Box::new(DataFusionError::External(Box::new(
                                wyrd_spec::vala::error::BifrostError::QueryTimeout,
                            ))),
                        )),
                        frame = frames.next() => Ok(frame),
                    };
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
                            if decoder.rows() > 0 || !is_availability_loss(&error) {
                                Err(live_error("stream", &error))?;
                            }
                            tracing::warn!(
                                scribe = %route.node_id.as_uuid(),
                                ?error,
                                "Oracle omits a live Scribe route lost before its first row"
                            );
                            record_live_tail_unavailable(bindings.degraded());
                            continue 'routes;
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

/// Reports whether a dispatch failure is a known live source's availability loss.
///
/// Only these may become a `Degraded` omission, and only before the source
/// yielded a row. Security, tenant, protocol, and resource failures are never
/// availability: they fail the query wherever they occur.
const fn is_availability_loss(error: &DispatchError) -> bool {
    matches!(
        error,
        DispatchError::Unavailable
            | DispatchError::EligibleSourceLoss { .. }
            | DispatchError::StaleObject
    )
}

/// Maps a fatal live-source failure to the `DataFusion` error the terminal maps.
///
/// A foreign-tenant refusal keeps its typed tenant-invariant identity and a
/// Scribe capacity refusal becomes a resource refusal; every other failure is
/// an execution failure.
fn live_error(stage: &str, error: &DispatchError) -> DataFusionError {
    match error {
        DispatchError::TenantInvariant => DataFusionError::External(Box::new(
            wyrd_spec::vala::BifrostError::QueryTenantInvariant,
        )),
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
        let leaf = || {
            LiveScribeExec::new(
                LiveTableRoutes {
                    binding: TenantTableBinding {
                        tenant_id: wyrd_spec::DataTenantId::new_v7(),
                        namespace: "bifrost".to_owned(),
                        table: "events".to_owned(),
                    },
                    table_uid: uuid::Uuid::nil(),
                    routes: Vec::new(),
                },
                "f".repeat(64),
                vec!["v".to_owned()],
                Vec::new(),
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
        let mut stats = crate::oracle::exec::OracleQueryScanStats::from_plan(&live, 0);
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
        let mut stats = crate::oracle::exec::OracleQueryScanStats::from_plan(&memtable, 0);
        stats.finalize();
        assert_eq!(
            stats.physical_bytes_scanned, None,
            "a memtable read scans no Parquet"
        );
    }

    /// An event-time floor excludes a route whose whole partition precedes
    /// it, while a predicate on any other column retains every route.
    #[test]
    fn live_routes_prune_only_on_event_time() {
        let hour = |start: i64| {
            TimePartitionWire::new(
                wyrd_spec::vala::api::TimeGranularityWire::Hour,
                chrono::DateTime::from_timestamp_micros(start).expect("instant"),
            )
            .expect("hour boundary")
        };
        let route = |start| LiveScribeRoute {
            node_id: NodeId::new(uuid::Uuid::now_v7()),
            writer_epoch: 1,
            endpoint: "https://scribe.internal".to_owned(),
            time_partition: hour(start),
        };
        let early = route(1_787_493_600_000_000);
        let late = route(1_787_497_200_000_000);
        let table = LiveTableRoutes {
            binding: TenantTableBinding {
                tenant_id: wyrd_spec::DataTenantId::new_v7(),
                namespace: "bifrost".to_owned(),
                table: "events".to_owned(),
            },
            table_uid: uuid::Uuid::nil(),
            routes: vec![early, late.clone()],
        };
        let floor = ScanPredicate::GtEq(
            wyrd_spec::vala::WYRD_EVENT_TIME.to_owned(),
            wyrd_spec::vala::assignment_authority::ScanLiteral::TimestampMicros(
                1_787_497_200_000_000,
            ),
        );
        assert_eq!(table.select(std::slice::from_ref(&floor)), vec![late]);
        let other = ScanPredicate::GtEq(
            "id".to_owned(),
            wyrd_spec::vala::assignment_authority::ScanLiteral::I64(0),
        );
        assert_eq!(table.select(&[other]).len(), 2);
    }
}
