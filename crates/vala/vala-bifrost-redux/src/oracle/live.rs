//! Leader-owned streaming source over the selected Scribe live routes.
//!
//! Oracle discovers each referenced table's reported Scribe streams before
//! its single physical planning pass, keeps only the routes safe event-time
//! pruning cannot exclude, and plants one [`LiveScribeExec`] per scan whose
//! partitions are those routes. Planning opens nothing. At execution each
//! partition dispatches one signed fragment to its Scribe through the existing
//! peer protocol and hands every validated batch to `DataFusion` as it
//! arrives, so no live fragment is ever materialized on the leader.
//!
//! A route that is unavailable before it yields a row is recorded as a
//! `LiveTail` loss and ends empty; any later loss, and any integrity, tenant,
//! security, resource, cancellation, or deadline fault, fails the query.

use std::sync::Arc;

use arrow::datatypes::SchemaRef;
use arrow::record_batch::RecordBatch;
use datafusion::common::tree_node::TreeNodeRecursion;
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
    ScribeProviderCut, TenantTableBinding, TimePartitionWire, WorkerAttemptFrame, WorkerFooter,
};
use wyrd_spec::vala::assignment_authority::ScanPredicate;

use super::dispatcher::{
    DispatchCandidate, DispatchContext, DispatchError, FragmentDispatcher, PhysicalDispatchFragment,
};
use super::{DegradedPartition, DegradedSourceAccumulator};
use crate::catalog::event_time::EventTimeStatistics;
use crate::catalog::layout::TimePartition;
use crate::oracle::pruning::EventTimeQueryInterval;

/// Largest Arrow batch count one Scribe fragment may retain for its cut.
const LIVE_FRAGMENT_MAX_BATCHES: u32 = 4096;

/// Largest byte total one Scribe fragment may retain for its cut.
///
/// The Scribe charges one follower lease of exactly this size before it opens
/// the fragment, so the cut can never ask the source for more than the grant
/// that backs it.
const LIVE_FRAGMENT_MAX_RETAINED_BYTES: u64 =
    crate::resources::ORACLE_PARTITION_MEMORY_BYTES as u64;

/// Stable degraded-reason label for a live source lost before its first row.
const LIVE_TAIL_UNAVAILABLE: &str = "live_tail_unavailable";

/// One reported Scribe stream partition that may hold a table's live rows.
///
/// Every field comes verbatim from authenticated discovery: the Scribe
/// incarnation that reported the stream, the writer epoch it reported under,
/// and the exact time partition it retains. The fragment later dispatched for
/// this route is signed for exactly this incarnation and epoch, so a Scribe
/// that restarts in between refuses it rather than answering for another cut.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LiveScribeRoute {
    /// Reporting Scribe node.
    pub(crate) node_id: NodeId,
    /// Writer epoch (the Scribe's role fence) the stream was reported under.
    pub(crate) writer_epoch: u64,
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

/// Leader-owned leaf reading one scan's selected Scribe routes, one per partition.
///
/// It is never serialized: the Analytical planner keeps its stage on the
/// leader, and the codec refuses it. What it retains is only planning
/// authority — routes and the signed closure — never a dispatcher or rows.
#[derive(Debug, Clone)]
pub(super) struct LiveScribeExec {
    /// Table binding and the selected routes, one per output partition.
    routes: Arc<LiveTableRoutes>,
    /// Fingerprint of the table's complete physical schema.
    schema_fingerprint: String,
    /// Signed projection closure, in closure order.
    required_columns: Vec<String>,
    /// Closed predicates the Scribe applies before it streams a batch.
    predicates: Vec<ScanPredicate>,
    /// Advertised closure schema and one partition per route.
    properties: Arc<PlanProperties>,
}

impl LiveScribeExec {
    /// Plans one live leaf over already-selected routes.
    ///
    /// `routes.routes` must be non-empty; a scan with nothing selected plants no
    /// live leaf at all.
    pub(super) fn new(
        routes: LiveTableRoutes,
        schema_fingerprint: String,
        required_columns: Vec<String>,
        predicates: Vec<ScanPredicate>,
        schema: SchemaRef,
    ) -> Self {
        let partitions = routes.routes.len().max(1);
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
        }
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
                maximum_batch_count: LIVE_FRAGMENT_MAX_BATCHES,
                maximum_retained_bytes: LIVE_FRAGMENT_MAX_RETAINED_BYTES,
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
                endpoint: None,
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

    /// Returns the closure schema and one partition per selected route.
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

    /// Streams one route's fragment as its validated batches arrive.
    ///
    /// Nothing is opened until the returned stream is first polled, and
    /// dropping it drops the peer stream, which cancels the Scribe fragment and
    /// releases its snapshot and lease. See [`LiveFragmentRead`] for how each
    /// outcome maps to the query terminal.
    ///
    /// # Errors
    /// Returns a plan error for a partition outside the selected routes, and an
    /// execution error when the task carries no bound execution bindings.
    fn execute(
        &self,
        partition: usize,
        context: Arc<TaskContext>,
    ) -> DataFusionResult<SendableRecordBatchStream> {
        let route = self.routes.routes.get(partition).cloned().ok_or_else(|| {
            DataFusionError::Plan(format!("live Scribe source has no partition {partition}"))
        })?;
        let lock = super::bindings::bindings_for_task(context.as_ref())?;
        let read = LiveFragmentRead {
            leaf: self.clone(),
            route,
            lock,
        };
        Ok(Box::pin(RecordBatchStreamAdapter::new(
            self.schema(),
            read.into_stream(),
        )))
    }
}

/// One route's fragment read, from dispatch through footer validation.
struct LiveFragmentRead {
    /// Leaf whose closure the fragment is signed for.
    leaf: LiveScribeExec,
    /// The one route this partition reads.
    route: LiveScribeRoute,
    /// Execution bindings bound after admission.
    lock: Arc<super::bindings::OracleExecutionLock>,
}

impl LiveFragmentRead {
    /// Turns the read into a batch stream `DataFusion` polls incrementally.
    ///
    /// Outcomes, in order of precedence:
    /// - cancellation or an elapsed deadline fails the partition;
    /// - an availability loss (`Unavailable`, `EligibleSourceLoss`,
    ///   `StaleObject`) before any row reached `DataFusion` records one
    ///   `LiveTail` loss and ends the partition empty;
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
            let (fragment, candidate) = self
                .leaf
                .fragment(&self.route, grant.deadline.timestamp_millis())?;
            let mut decoder = LiveFrameDecoder::new(fragment.plan_fingerprint.clone());
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
                        scribe = %self.route.node_id.as_uuid(),
                        ?error,
                        "Oracle omits a live Scribe route unavailable before its first row"
                    );
                    record_live_tail_unavailable(bindings.degraded());
                    return;
                }
            };
            loop {
                let frame = tokio::select! {
                    () = grant.cancellation.cancelled() => Err(DataFusionError::Execution(
                        "Oracle query was cancelled during a live Scribe read".to_owned(),
                    )),
                    () = tokio::time::sleep(remaining(grant.deadline)) => Err(DataFusionError::Execution(
                        "Oracle query deadline elapsed during a live Scribe read".to_owned(),
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
                        break;
                    }
                    Some(Err(error)) => {
                        grant.ensure_live()?;
                        if decoder.rows() > 0 || !is_availability_loss(&error) {
                            Err(live_error("stream", &error))?;
                        }
                        tracing::warn!(
                            scribe = %self.route.node_id.as_uuid(),
                            ?error,
                            "Oracle omits a live Scribe route lost before its first row"
                        );
                        record_live_tail_unavailable(bindings.degraded());
                        return;
                    }
                    Some(Ok(frame)) => {
                        if let Some(batch) = decoder.accept(frame).map_err(|error| {
                            DataFusionError::Execution(format!(
                                "live Scribe fragment violated the attempt protocol: {error}"
                            ))
                        })? {
                            yield super::exec::project_batch(&batch, SchemaRef::clone(&schema))?;
                        }
                    }
                }
            }
        }
    }
}

/// Returns the time left before `deadline`, zero once it has passed.
fn remaining(deadline: chrono::DateTime<chrono::Utc>) -> std::time::Duration {
    deadline
        .signed_duration_since(chrono::Utc::now())
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
    /// A frame arrived after the footer.
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
    /// The footer contradicts the delivered frames or the signed fragment.
    #[error("footer does not validate")]
    Footer,
}

/// Incremental validator for one fragment's attempt frames.
///
/// It checks exactly what the whole-attempt buffer checks — one leading
/// schema, counted bytes and rows, the in-order payload digest, and a footer
/// naming the signed fragment — but releases each batch as soon as it
/// decodes instead of retaining the attempt.
struct LiveFrameDecoder {
    /// Fingerprint the ticket was minted over and the footer must name.
    plan_fingerprint: String,
    /// Whether the leading schema frame arrived.
    schema_seen: bool,
    /// Encoded schema and batch bytes delivered so far.
    encoded_bytes: u64,
    /// Rows decoded from delivered batch frames.
    rows: u64,
    /// Digest over batch frames in stream order.
    payload_hash: Sha256,
    /// The validated footer, once it arrived.
    footer: Option<WorkerFooter>,
}

impl LiveFrameDecoder {
    /// Starts validating one fragment signed under `plan_fingerprint`.
    fn new(plan_fingerprint: String) -> Self {
        Self {
            plan_fingerprint,
            schema_seen: false,
            encoded_bytes: 0,
            rows: 0,
            payload_hash: Sha256::new(),
            footer: None,
        }
    }

    /// Rows already released to `DataFusion`.
    const fn rows(&self) -> u64 {
        self.rows
    }

    /// Reports whether a valid footer closed the fragment.
    const fn completed(&self) -> bool {
        self.footer.is_some()
    }

    /// Validates one frame and returns the batch it carried, if any.
    ///
    /// # Errors
    ///
    /// Returns [`LiveFrameError`] for a frame after the footer, a repeated or
    /// late schema, a batch that is not one Arrow IPC batch, counter overflow,
    /// or a footer whose fragment, digest, byte count, row count, or completion
    /// flag contradicts what was delivered.
    fn accept(&mut self, frame: WorkerAttemptFrame) -> Result<Option<RecordBatch>, LiveFrameError> {
        if self.footer.is_some() {
            return Err(LiveFrameError::AfterFooter);
        }
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
                self.rows = self
                    .rows
                    .checked_add(
                        u64::try_from(batch.num_rows()).map_err(|_| LiveFrameError::Overflow)?,
                    )
                    .ok_or(LiveFrameError::Overflow)?;
                Ok(Some(batch))
            }
            WorkerAttemptFrame::Footer(footer) => {
                let digest = hex::encode(self.payload_hash.clone().finalize());
                if !self.schema_seen
                    || !footer.completed
                    || footer.fragment_id != self.plan_fingerprint
                    || footer.manifest_digest.as_str() != self.plan_fingerprint
                    || footer.encoded_bytes != self.encoded_bytes
                    || footer.row_count != self.rows
                    || footer.payload_digest.as_str() != digest
                {
                    return Err(LiveFrameError::Footer);
                }
                self.footer = Some(footer);
                Ok(None)
            }
        }
    }

    /// Adds one frame's encoded length to the running byte count.
    ///
    /// # Errors
    /// Returns [`LiveFrameError::Overflow`] when the count cannot grow.
    fn add_bytes(&mut self, len: usize) -> Result<(), LiveFrameError> {
        self.encoded_bytes = self
            .encoded_bytes
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
    /// before the schema are each refused.
    #[test]
    fn live_frames_release_batches_incrementally_and_validate_the_footer() {
        let fingerprint = "a".repeat(64);
        let mut valid = frames(&fingerprint, &[1, 2, 3]).into_iter();
        let mut decoder = LiveFrameDecoder::new(fingerprint.clone());
        assert_eq!(decoder.accept(valid.next().expect("schema")), Ok(None));
        let batch = decoder
            .accept(valid.next().expect("batch"))
            .expect("batch validates")
            .expect("batch is released before the footer");
        assert_eq!(
            (batch.num_rows(), decoder.rows(), decoder.completed()),
            (3, 3, false)
        );
        let footer = valid.next().expect("footer");
        assert_eq!(decoder.accept(footer.clone()), Ok(None));
        assert!(decoder.completed());
        assert_eq!(decoder.accept(footer), Err(LiveFrameError::AfterFooter));

        let mut foreign = frames(&"b".repeat(64), &[1]).into_iter();
        let mut decoder = LiveFrameDecoder::new(fingerprint.clone());
        decoder
            .accept(foreign.next().expect("schema"))
            .expect("schema");
        decoder
            .accept(foreign.next().expect("batch"))
            .expect("batch");
        assert_eq!(
            decoder.accept(foreign.next().expect("footer")),
            Err(LiveFrameError::Footer)
        );

        let mut early = frames(&fingerprint, &[1]).into_iter().skip(1);
        let mut decoder = LiveFrameDecoder::new(fingerprint);
        assert_eq!(
            decoder.accept(early.next().expect("batch")),
            Err(LiveFrameError::Schema)
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
