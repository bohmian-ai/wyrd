//! Three-stage authenticated physical-plan follower lifecycle.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use arrow::datatypes::SchemaRef;
use async_trait::async_trait;
use datafusion::datasource::memory::MemorySourceConfig;
use datafusion::execution::TaskContext;
use datafusion::execution::session_state::SessionState;
use datafusion::physical_optimizer::PhysicalOptimizerRule as _;
use datafusion::physical_optimizer::filter_pushdown::FilterPushdown;
use datafusion::physical_plan::{ExecutionPlan, SendableRecordBatchStream, execute_stream};
use datafusion_proto::bytes::physical_plan_from_bytes_with_extension_codec;
use datafusion_proto::protobuf::{PhysicalPlanNode, physical_plan_node::PhysicalPlanType};
use prost::Message;
use thiserror::Error;
use wyrd_spec::DataTenantId;
use wyrd_spec::vala::api::{
    ClusterRole, ExecuteFragmentRequest, FollowerScanAssignment, OracleRoleFence, ReservationId,
    TenantTableBinding,
};

use super::codec::{OraclePhysicalExtensionCodec, physical_plan_fingerprint};
use crate::catalog::layout::TimePartition;
use crate::catalog::{BifrostCatalog, TableRef};
use crate::contracts::ScribeError;
use crate::scribe::stream_identity::StreamIdentity;
use crate::scribe::tail_rpc::{FetchLiveTailRequest, FetchLiveTailService, LiveTailBatches};

/// Hard private plan-size default enforced before protobuf decoding.
pub const DEFAULT_MAX_PHYSICAL_PLAN_BYTES: usize = 8 * 1024 * 1024;

/// Fail-closed follower lifecycle error.
#[derive(Debug, Error)]
pub enum PhysicalPlanFollowerError {
    /// The request or physical protobuf tree failed IO-free validation.
    #[error("physical-plan preflight rejected the request: {0}")]
    Preflight(String),
    /// A role-bound source could not be constructed.
    #[error("role-bound provider resolution failed: {0}")]
    Resolution(FollowerResolutionError),
    /// `DataFusion` rejected native physical fields after complete resolution.
    #[error("physical-plan decode failed after provider resolution: {0}")]
    PostResolutionDecode(String),
    /// The decoded plan could not create its output stream.
    #[error("physical-plan execution failed after decode: {0}")]
    Execution(String),
}

/// Why one role-bound source could not be resolved.
///
/// The class is decided where the cause is known and carried unchanged to the
/// role's dispatch mapping, which is the only place it becomes a wire
/// outcome. A published follower treats every class as eligible source loss;
/// a Scribe follower degrades only `SourceLoss` and fails `Fault`.
#[derive(Debug, Error)]
pub enum FollowerResolutionError {
    /// Schema, projection, predicate, binding, integrity, or local execution
    /// fault. Every untyped resolver message is this class.
    #[error("{0}")]
    Fault(String),
    /// The assigned source no longer exists at the signed incarnation.
    #[error("assigned source is gone: {0}")]
    SourceLoss(String),
}

impl From<String> for FollowerResolutionError {
    /// Treats an unclassified resolver message as a fail-closed fault.
    fn from(detail: String) -> Self {
        Self::Fault(detail)
    }
}

/// One resolved role-local source and the authenticated schema it came from.
///
/// A follower has to keep two schemas distinct, and returning only the plan
/// would collapse them. `full_schema` is the authenticated catalog schema whose
/// fingerprint must equal `assignment.schema_fingerprint`; `plan` exposes the
/// projection of that schema by the signed ordered `assignment.required_columns`
/// and nothing wider. Pairing them makes it impossible for a resolver to
/// publish a narrowed plan without also naming the schema its narrowing was
/// derived from.
pub struct ResolvedFollowerSource {
    /// Role-local leaf exposing exactly the signed closure schema.
    pub plan: Arc<dyn ExecutionPlan>,
    /// Complete authenticated schema the closure was projected out of.
    pub full_schema: SchemaRef,
}

impl std::fmt::Debug for ResolvedFollowerSource {
    /// Renders only the two schemas' shapes, never catalog or storage internals.
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ResolvedFollowerSource")
            .field("plan_schema", &self.plan.schema())
            .field("full_schema", &self.full_schema)
            .finish()
    }
}

/// Projects one authenticated full schema by a signed ordered closure.
///
/// This is the single derivation of a follower's leaf schema. Both the resolver
/// (before its own source IO) and [`PhysicalPlanFollower::decode`] (after every
/// scan id resolves) call it, so the plan a follower publishes and the schema
/// decode requires of it can never be derived two different ways.
///
/// # Errors
///
/// Returns a redacted message when a signed name is absent from the
/// authenticated schema or names it ambiguously. None is repaired: silently deduplicating or
/// reordering a signed assignment would read something other than what the
/// leader signed.
pub(super) fn signed_closure_schema(
    full_schema: &arrow::datatypes::Schema,
    assignment: &FollowerScanAssignment,
) -> Result<SchemaRef, String> {
    super::exec::select_schema_by_name(full_schema, &assignment.required_columns)
        .map(|(schema, _)| schema)
        .map_err(|_| {
            "assignment closure does not resolve against the authenticated schema".to_owned()
        })
}

/// Async boundary that constructs one authenticated role-local scan provider.
#[async_trait]
pub trait FollowerSourceResolver: Send + Sync {
    /// Resolves exactly one validated assignment for the signed target role.
    ///
    /// The returned plan must expose exactly the signed closure, and the
    /// returned `full_schema` must be the authenticated schema that closure was
    /// projected out of; `decode` revalidates both. Cancellation may discard a
    /// locally acquired provider; no provider is published to decode until this
    /// future returns successfully. Callers may retry only by restarting the
    /// complete authenticated follower request.
    ///
    /// # Errors
    /// Returns a redacted [`FollowerResolutionError`] when catalog, storage, or
    /// Scribe snapshot IO fails, classed by its known cause.
    async fn resolve(
        &self,
        target_role: ClusterRole,
        assignment: &FollowerScanAssignment,
        session: &SessionState,
    ) -> Result<ResolvedFollowerSource, FollowerResolutionError>;
}

#[async_trait]
impl<T> FollowerSourceResolver for Arc<T>
where
    T: FollowerSourceResolver + ?Sized,
{
    /// Delegates to the one injected process resolver allocation.
    async fn resolve(
        &self,
        target_role: ClusterRole,
        assignment: &FollowerScanAssignment,
        session: &SessionState,
    ) -> Result<ResolvedFollowerSource, FollowerResolutionError> {
        self.as_ref()
            .resolve(target_role, assignment, session)
            .await
    }
}

/// Creates one request-local session and task context under `execution`.
///
/// Every follower session — leader-local Oracle, remote Oracle peer, or Scribe
/// hot-tail peer — runs under the execution its own admission issued, never a
/// value the requesting peer supplied, so it opens the dispatched files with
/// the same shape and Parquet read options the leader's session uses.
fn request_session(
    execution: &crate::resources::OracleExecution,
) -> (SessionState, Arc<TaskContext>) {
    let state = execution.session_state(datafusion::prelude::SessionConfig::new());
    let task = Arc::new(TaskContext::from(&state));
    (state, task)
}

/// Refuses a follower fragment that was assigned nothing to scan.
///
/// Each assigned Scribe cut is one openable scan target; a fragment with none should never have been
/// dispatched.
///
/// # Errors
///
/// Returns [`PhysicalPlanFollowerError::Preflight`] when the assignments carry
/// no scannable work at all, which is a contract failure rather than an empty
/// result.
fn require_scannable_work(
    assignments: &[FollowerScanAssignment],
) -> Result<(), PhysicalPlanFollowerError> {
    if !assignments
        .iter()
        .any(|assignment| assignment.scribe_provider_cut.is_some())
    {
        return Err(PhysicalPlanFollowerError::Preflight(
            "dispatched fragment carries no scannable work".to_owned(),
        ));
    }
    Ok(())
}

/// Validated IO-free request projection passed into provider resolution.
#[derive(Debug)]
struct PreflightRequest {
    /// Assignments keyed by exact encoded scan identity.
    assignments: HashMap<String, FollowerScanAssignment>,
}

/// Checked peer-context and local-incarnation facts used by IO-free preflight.
#[derive(Debug, Clone)]
pub struct AuthenticatedFollowerContext<'a> {
    /// Tenant recovered only from checked context claims.
    pub tenant_id: DataTenantId,
    /// Exact table binding recovered from checked context claims.
    pub table_binding: &'a TenantTableBinding,
    /// Exact grant this receiver holds for the query on its leader's stream.
    pub reservation_id: &'a ReservationId,
    /// Leader incarnation recovered from checked context claims.
    pub leader_fence: OracleRoleFence,
    /// Role incarnation owned by this receiving process.
    pub local_fence: OracleRoleFence,
}

/// Builds the leaf for an assignment that names no files at all.
///
/// A signed closure can legitimately select nothing once the leader has pruned
/// it, and that is a correct empty result rather than an error: the follower
/// still returns a plan at the closure schema so the union above it sees the
/// same columns every other fragment produces. A Scribe follower uses it for a
/// sibling Scribe's placeholder, which this node must not read.
///
/// # Errors
/// Returns a scrubbed message when the empty in-memory source cannot be built.
fn empty_assignment_leaf(
    required_schema: &SchemaRef,
    full_schema: SchemaRef,
) -> Result<ResolvedFollowerSource, FollowerResolutionError> {
    let batch = arrow::record_batch::RecordBatch::new_empty(Arc::clone(required_schema));
    MemorySourceConfig::try_new_exec(&[vec![batch]], Arc::clone(required_schema), None)
        .map(|plan| ResolvedFollowerSource {
            plan: plan as Arc<dyn ExecutionPlan>,
            full_schema,
        })
        .map_err(|_| {
            FollowerResolutionError::Fault("authenticated empty provider failed".to_owned())
        })
}

/// Authenticated Scribe resolver backed by the unchanged pod-local live-tail service.
#[async_trait]
trait LiveTailSource: Send + Sync + std::fmt::Debug {
    /// Returns the exact stream incarnation served by this source.
    fn stream(&self) -> StreamIdentity;

    /// Opens one lazily produced live read over the bounded memtable cut and
    /// the staged members it did not serve.
    /// Cancellation before the open returns exposes no producer; a retry opens
    /// a new cut.
    ///
    /// # Errors
    /// Returns a redacted [`FollowerResolutionError`] when Scribe rejects or
    /// cannot open the read.
    async fn open(
        &self,
        request: FetchLiveTailRequest,
    ) -> Result<LiveTailBatches, FollowerResolutionError>;
}

#[async_trait]
impl LiveTailSource for FetchLiveTailService {
    /// Returns the unchanged service's local stream identity.
    fn stream(&self) -> StreamIdentity {
        FetchLiveTailService::stream(self)
    }

    /// Delegates exactly once to the Scribe live-read open.
    ///
    /// # Errors
    /// Returns [`FollowerResolutionError::SourceLoss`] when the request names
    /// another stream incarnation and [`FollowerResolutionError::Fault`] for
    /// every other open failure.
    async fn open(
        &self,
        request: FetchLiveTailRequest,
    ) -> Result<LiveTailBatches, FollowerResolutionError> {
        self.open_live_batches(&request)
            .map_err(|error| live_open_error(&error))
    }
}

/// Classes one Scribe live-read open failure for the fragment's dispatch.
///
/// A request for another stream incarnation means the assigned source is gone;
/// every other open failure is a fault. The Scribe detail is redacted.
fn live_open_error(error: &ScribeError) -> FollowerResolutionError {
    match error {
        ScribeError::StreamMismatch { .. } => {
            FollowerResolutionError::SourceLoss("Scribe stream incarnation changed".to_owned())
        }
        _ => FollowerResolutionError::Fault("Scribe live-tail snapshot failed".to_owned()),
    }
}

/// Role-local resolver that snapshots the authenticated Scribe stream exactly once.
///
/// It turns the opened live read into one `DataFusion` leaf: the memtable cut
/// as an in-memory source and the unserved staged runs through Oracle's own
/// hot-Parquet scan, the memtable split across the session's target
/// partitions and the staged runs by `DataFusion`'s Parquet scan, both filtered by
/// the signed closure.
pub struct ScribeTailResolver<T = FetchLiveTailService> {
    /// Existing Scribe snapshot service; this owner calls it once per assignment.
    tail: Arc<T>,
    /// Closed source for the authenticated role-local table schema.
    schema_source: ScribeSchemaSource,
}

/// Closed Scribe schema source separating production catalog IO from unit fixtures.
enum ScribeSchemaSource {
    /// Shared tenant-qualified production catalog.
    Catalog(Arc<BifrostCatalog>),
    /// Focused unit-test schema with no external catalog.
    #[cfg(test)]
    Fixed(arrow::datatypes::SchemaRef),
}

impl<T> std::fmt::Debug for ScribeTailResolver<T> {
    /// Redacts the injected tail and catalog dependencies.
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_struct("ScribeTailResolver").finish()
    }
}

impl ScribeTailResolver<FetchLiveTailService> {
    /// Creates a Scribe resolver from the local tail capability and shared catalog.
    #[must_use]
    pub fn new(tail: Arc<FetchLiveTailService>, catalog: Arc<BifrostCatalog>) -> Self {
        Self {
            tail,
            schema_source: ScribeSchemaSource::Catalog(catalog),
        }
    }
}

#[cfg(test)]
impl<T> ScribeTailResolver<T> {
    /// Creates one catalog-free resolver for focused role-local provider tests.
    pub(crate) fn with_schema(tail: Arc<T>, schema: SchemaRef) -> Self {
        Self {
            tail,
            schema_source: ScribeSchemaSource::Fixed(schema),
        }
    }
}

impl<T> ScribeTailResolver<T> {
    /// Builds the Scribe follower's leaf over one opened live read.
    ///
    /// Memtable rows are projected to the signed closure and dealt round-robin
    /// across exactly the session's target partitions of one in-memory source,
    /// leaving unused partitions empty, so a sparse cut keeps the session
    /// shape. The unserved staged runs are read by
    /// `DataFusion`'s Parquet scan, so they get the row-group and page pruning
    /// every Oracle hot read gets, with no footer caching because a staged run
    /// is rewritable; the scan's files hold the staged lease until the plan
    /// drops. A read with neither
    /// rows nor runs is an in-memory source of empty partitions at the
    /// closure schema.
    ///
    /// # Errors
    /// Returns [`FollowerResolutionError::Fault`] when a memtable batch does
    /// not project to the closure, a staged run's size cannot be read or its
    /// path is not UTF-8, or a plan node cannot be built.
    async fn live_leaf(
        &self,
        read: LiveTailBatches,
        required_schema: &SchemaRef,
        assignment: &FollowerScanAssignment,
        session: &SessionState,
    ) -> Result<Arc<dyn ExecutionPlan>, FollowerResolutionError> {
        let fault = |detail: &str| FollowerResolutionError::Fault(detail.to_owned());
        let partitions = session.config().target_partitions().max(1);
        let (rows, runs, lease) = read.into_parts();
        let mut leaves: Vec<Arc<dyn ExecutionPlan>> = Vec::with_capacity(2);
        if !rows.is_empty() || runs.is_empty() {
            let mut groups = vec![Vec::new(); partitions];
            for (index, batch) in rows.iter().enumerate() {
                groups[index % partitions].push(
                    super::exec::project_batch(batch, Arc::clone(required_schema))
                        .map_err(|_| fault("Scribe memtable rows do not match the closure"))?,
                );
            }
            leaves.push(
                MemorySourceConfig::try_new_exec(&groups, Arc::clone(required_schema), None)
                    .map_err(|_| fault("Scribe memtable source construction failed"))?,
            );
        }
        if !runs.is_empty() {
            let mut files = Vec::with_capacity(runs.len());
            for run in runs {
                let size = tokio::fs::metadata(&run)
                    .await
                    .map_err(|_| fault("Scribe staged run is unreadable"))?
                    .len();
                let location = run
                    .into_os_string()
                    .into_string()
                    .map_err(|_| fault("Scribe staged run path is not UTF-8"))?;
                files.push(super::exec::HotFileSource {
                    size_bytes: usize::try_from(size)
                        .map_err(|_| fault("Scribe staged run size does not fit"))?,
                    row_count: None,
                    location,
                    event_time: crate::catalog::event_time::EventTimeStatistics::Unusable(
                        crate::catalog::event_time::EventTimeBoundsDefect::Missing,
                    ),
                });
            }
            leaves.push(
                super::exec::hot_parquet_scan(
                    files,
                    super::exec::HotObjects::Staged(lease),
                    assignment.binding.tenant_id,
                    required_schema,
                    session.config().options(),
                )
                .map_err(|_| fault("Scribe staged scan construction failed"))?,
            );
        }
        match leaves.len() {
            1 => Ok(leaves.remove(0)),
            _ => datafusion::physical_plan::union::UnionExec::try_new(leaves)
                .map_err(|_| fault("Scribe live source union failed")),
        }
    }
}

/// One follower fragment execution: its output stream and its scan evidence.
///
/// The two are returned together because the scan metric sets must be captured
/// from the physical plan before execution consumes it, yet can only be read
/// for a total after the stream has drained. Pairing them makes it impossible
/// for a caller to take the stream and silently lose the evidence.
pub struct FollowerExecution {
    /// Ordered record batches this fragment produces.
    stream: SendableRecordBatchStream,
    /// Scan metric sets pinned before execution began.
    scan_stats: FollowerScanEvidence,
}

impl FollowerExecution {
    /// Splits this execution into its output stream and deferred scan evidence.
    ///
    /// The caller drains the stream, then finalizes the evidence; the split
    /// exists because those two steps happen in different scopes.
    #[must_use]
    pub fn split(self) -> (SendableRecordBatchStream, FollowerScanEvidence) {
        (self.stream, self.scan_stats)
    }
}

/// Scan metric sets pinned from one follower plan, read after its stream drains.
///
/// `DataFusion` populates scan metrics during execution, so this owner exists to
/// make the ordering explicit: it is created before execution and consumed only
/// once the stream is finished.
pub struct FollowerScanEvidence(super::exec::OracleQueryScanStats);

impl FollowerScanEvidence {
    /// Reads the pinned metric sets into the wire-shaped follower totals.
    ///
    /// Call this only after the paired stream has drained. Finalizing earlier
    /// reports a partial scan, and reports no bytes at all for sources whose
    /// counters are written on their final poll.
    #[must_use]
    pub fn finalize(mut self) -> wyrd_spec::vala::api::WorkerScanStats {
        self.0.finalize();
        wyrd_spec::vala::api::WorkerScanStats {
            bytes_scanned: self.0.physical_bytes_scanned,
            files_scanned: self.0.files_scanned,
            partitions_scanned: self.0.partitions_scanned,
            row_groups_scanned: self.0.pruning.row_groups_scanned,
            row_groups_pruned: self.0.pruning.row_groups_pruned,
            row_groups_pruned_bloom: self.0.pruning.row_groups_pruned_bloom,
            rows_pruned_page_index: self.0.pruning.rows_pruned_page_index,
        }
    }
}

#[async_trait]
impl<T> FollowerSourceResolver for ScribeTailResolver<T>
where
    T: LiveTailSource,
{
    /// Fetches one ordered active-plus-unretired-immutable cohort and builds its provider.
    ///
    /// The single bounded snapshot is observed as one `remote`-locality
    /// live-tail page so a distributed query, whose leader never drains a local
    /// fence, still reports the live-tail page families an operator reads.
    ///
    /// Cancellation before the single snapshot returns exposes no batches or
    /// provider. Cancellation during provider construction drops the complete
    /// local cohort. A retry begins again from the authenticated assignment.
    ///
    /// # Errors
    /// Returns a redacted resolution error for role/binding/range conversion, a
    /// schema fingerprint that differs from the assignment, a closure that does
    /// not resolve against the authenticated schema, live-tail snapshot, or
    /// leaf construction failure (see [`Self::live_leaf`]).
    async fn resolve(
        &self,
        target_role: ClusterRole,
        assignment: &FollowerScanAssignment,
        session: &SessionState,
    ) -> Result<ResolvedFollowerSource, FollowerResolutionError> {
        if target_role != ClusterRole::Scribe {
            return Err(FollowerResolutionError::Fault(
                "Scribe resolver received a non-Scribe assignment".to_owned(),
            ));
        }
        let cut = assignment
            .scribe_provider_cut
            .as_ref()
            .ok_or_else(|| "Scribe assignment is missing its provider cut".to_owned())?;
        let stream = self.tail.stream();
        if u64::try_from(stream.writer_epoch.as_i64()).ok() != Some(cut.writer_epoch) {
            return Err(FollowerResolutionError::SourceLoss(
                "Scribe writer epoch differs from the authenticated cut".to_owned(),
            ));
        }
        let binding = crate::catalog::TenantTableBinding::resolve((
            assignment.binding.tenant_id,
            assignment_table(&assignment.binding)?,
        ))
        .map_err(|_| "Scribe tenant/table binding is invalid".to_owned())?;
        let full_schema = match &self.schema_source {
            ScribeSchemaSource::Catalog(catalog) => catalog
                .assignment_schema(
                    &assignment_table(&assignment.binding)?,
                    assignment.binding.tenant_id,
                )
                .await
                .map_err(|_| "authenticated Scribe schema resolution failed".to_owned())?,
            #[cfg(test)]
            ScribeSchemaSource::Fixed(schema) => Arc::clone(schema),
        };
        // Both checks run before the live-tail snapshot, not only in `decode`
        // after it: a mismatched assignment must never reach the tail source.
        if super::assignment_schema_fingerprint(full_schema.as_ref())
            != assignment.schema_fingerprint
        {
            return Err(FollowerResolutionError::Fault(
                "resolved provider schema fingerprint differs from assignment".to_owned(),
            ));
        }
        let required_schema = signed_closure_schema(full_schema.as_ref(), assignment)?;
        let table_name = format!(
            "{}.{}",
            assignment.binding.namespace, assignment.binding.table
        );
        let local_scan_id = super::scribe_follower_scan_id(
            &table_name,
            wyrd_spec::vala::api::NodeId::new(stream.node_id.as_uuid()),
            cut.writer_epoch,
        );
        if assignment.scan_id != local_scan_id {
            return empty_assignment_leaf(&required_schema, full_schema);
        }
        let start_partition = TimePartition::from_wire(cut.start_partition);
        let end_partition = TimePartition::from_wire(cut.end_partition);
        let read = self
            .tail
            .open(FetchLiveTailRequest {
                binding,
                target_stream: stream,
                start_partition,
                end_partition,
                // The signed closure, byte for byte. Scribe projects the
                // memtable to exactly these names in exactly this order, so
                // the wide columns are never materialized, and the declared
                // leaf schema below is the same closure — the serialized
                // plan's `Column` indices are closure indices.
                required_columns: assignment.required_columns.clone(),
            })
            .await?;
        let plan = self
            .live_leaf(read, &required_schema, assignment, session)
            .await?;
        Ok(ResolvedFollowerSource { plan, full_schema })
    }
}

/// Parses one exact domain table binding into the catalog's closed namespace type.
///
/// A wire binding may carry its namespace with or without the `vala.` root, so
/// the name is normalized through the shared canonicalizer rather than parsed
/// as-is — one spelling of it names nothing the closed namespace set contains.
///
/// # Errors
/// Returns an error when the namespace/table pair is not a canonical Wyrd FQN.
fn assignment_table(binding: &TenantTableBinding) -> Result<TableRef, String> {
    TableRef::parse_fqn(&crate::namespaces::canonical_table_name(
        &binding.namespace,
        &binding.table,
    ))
    .ok_or_else(|| "assignment table binding is not canonical".to_owned())
}

/// Concrete owner of preflight, provider resolution, and `DataFusion` decode.
pub struct PhysicalPlanFollower<R> {
    /// Authenticated role-local provider constructor.
    resolver: R,
    /// Maximum accepted protobuf size.
    maximum_plan_bytes: usize,
    /// Observable lifecycle-boundary effects used to prove fail-closed ordering.
    effects: FollowerEffects,
}

impl<R> std::fmt::Debug for PhysicalPlanFollower<R>
where
    R: std::fmt::Debug,
{
    /// Renders the resolver and follower policy.
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("PhysicalPlanFollower")
            .field("resolver", &self.resolver)
            .field("maximum_plan_bytes", &self.maximum_plan_bytes)
            .field("effects", &self.effects)
            .finish()
    }
}

/// Observable counts for the follower's effect-bearing lifecycle boundaries.
#[derive(Debug, Default)]
struct FollowerEffects {
    /// Common-plan placeholder visits performed before and after source resolution.
    preflight: AtomicUsize,
    /// Provider resolution attempts begun after successful preflight.
    resolver: AtomicUsize,
    /// Native physical decode attempts begun after complete resolution.
    decode: AtomicUsize,
    /// Physical execution attempts begun after successful decode.
    execution: AtomicUsize,
    /// Output streams successfully constructed for the caller.
    output: AtomicUsize,
}

impl<R> PhysicalPlanFollower<R>
where
    R: FollowerSourceResolver,
{
    /// Creates a follower with the production private plan bound.
    #[must_use]
    pub fn new(resolver: R) -> Self {
        Self {
            resolver,
            maximum_plan_bytes: DEFAULT_MAX_PHYSICAL_PLAN_BYTES,
            effects: FollowerEffects {
                preflight: AtomicUsize::new(0),
                resolver: AtomicUsize::new(0),
                decode: AtomicUsize::new(0),
                execution: AtomicUsize::new(0),
                output: AtomicUsize::new(0),
            },
        }
    }

    /// Preflights, resolves, then synchronously decodes one authenticated plan.
    ///
    /// No resolver call occurs until the complete wire shape, role constraints,
    /// extension set, and assignment set validate. Cancellation while resolving
    /// drops already-built providers and does not begin `DataFusion` decode.
    /// Cancellation during later resolution likewise drops all partial provider
    /// progress. Retry restarts preflight and every provider resolution.
    ///
    /// # Errors
    /// Returns [`PhysicalPlanFollowerError::Preflight`] when preflight fails,
    /// and other [`PhysicalPlanFollowerError`] variants for provider
    /// resolution or post-resolution native `DataFusion` decode failure.
    pub async fn decode(
        &self,
        request: &ExecuteFragmentRequest,
        authenticated: AuthenticatedFollowerContext<'_>,
        session: &SessionState,
        context: &TaskContext,
    ) -> Result<Arc<dyn ExecutionPlan>, PhysicalPlanFollowerError> {
        let preflight = self.preflight(request, &authenticated)?;
        let mut providers = HashMap::with_capacity(preflight.assignments.len());
        for (scan_id, assignment) in preflight.assignments {
            self.effects.resolver.fetch_add(1, Ordering::SeqCst);
            let resolved = self
                .resolver
                .resolve(request.target_fence.role, &assignment, session)
                .await
                .map_err(PhysicalPlanFollowerError::Resolution)?;
            // Two distinct schemas, checked in order. The fingerprint identifies
            // the table's complete canonical schema, so it is checked against
            // `full_schema`; the leaf the plan will actually read from must then
            // expose exactly the closure derived from that same schema and the
            // signed names. Checking only the fingerprint would admit a leaf of
            // any width, and deriving the closure from the leaf's own schema
            // would make the check circular.
            let actual = super::assignment_schema_fingerprint(resolved.full_schema.as_ref());
            if actual != assignment.schema_fingerprint {
                return Err(PhysicalPlanFollowerError::Resolution(
                    FollowerResolutionError::Fault(
                        "resolved provider schema fingerprint differs from assignment".to_owned(),
                    ),
                ));
            }
            let expected = signed_closure_schema(resolved.full_schema.as_ref(), &assignment)
                .map_err(|detail| PhysicalPlanFollowerError::Resolution(detail.into()))?;
            if resolved.plan.schema() != expected {
                return Err(PhysicalPlanFollowerError::Resolution(
                    FollowerResolutionError::Fault(
                        "resolved provider schema differs from the signed projection closure"
                            .to_owned(),
                    ),
                ));
            }
            providers.insert(scan_id, resolved.plan);
        }
        // Re-read the immutable common plan after every source-resolution await.
        // This closes the same fail-closed boundary as the initial preflight:
        // source construction cannot make a missing or malformed placeholder
        // eligible for union/substitution or output.
        self.preflight(request, &authenticated)?;
        let codec = OraclePhysicalExtensionCodec::decoder(providers);
        self.effects.decode.fetch_add(1, Ordering::SeqCst);
        let plan = physical_plan_from_bytes_with_extension_codec(
            &request.physical_plan_bytes,
            context,
            &codec,
        )
        .map_err(|error| PhysicalPlanFollowerError::PostResolutionDecode(error.to_string()))?;
        codec
            .require_complete_consumption()
            .map_err(|error| PhysicalPlanFollowerError::PostResolutionDecode(error.to_string()))?;
        Ok(plan)
    }

    /// Preflights, resolves, synchronously decodes, and creates one output stream.
    /// Cancellation before stream construction exposes no output. After this
    /// method returns, stream polling and partial output are owned by the caller;
    /// retry requires a fresh authenticated request and cannot resume that stream.
    ///
    /// # Errors
    /// Returns a lifecycle error before output when preflight, resolution, decode,
    /// provider consumption, or physical execution fails.
    pub async fn execute(
        &self,
        request: &ExecuteFragmentRequest,
        authenticated: AuthenticatedFollowerContext<'_>,
        execution: &crate::resources::OracleExecution,
    ) -> Result<FollowerExecution, PhysicalPlanFollowerError> {
        require_scannable_work(&request.assignments)?;
        let (session, context) = request_session(execution);
        let plan = self
            .decode(request, authenticated, &session, &context)
            .await?;
        self.effects.execution.fetch_add(1, Ordering::SeqCst);
        // Snapshot the scan metric sets before execution consumes the plan.
        // The leader's own plan has only remote leaves, so this follower-side
        // capture is the only place a distributed query can observe physical
        // read volume at all.
        // A live fragment ships its predicate as a `FilterExec` above the
        // placeholder; `DataFusion`'s own pushdown moves it into the
        // substituted staged scan, where it prunes row groups and pages.
        let plan = FilterPushdown::new()
            .optimize(plan, session.config().options())
            .map_err(|error| PhysicalPlanFollowerError::Execution(error.to_string()))?;
        let scan_stats = super::exec::OracleQueryScanStats::from_plan(plan.as_ref());
        let stream = execute_stream(plan, context)
            .map_err(|error| PhysicalPlanFollowerError::Execution(error.to_string()))?;
        #[cfg(feature = "test-support")]
        let stream = if request.target_fence.role == ClusterRole::Scribe {
            crate::scribe::tail_rpc::observe_live_production_for_test(stream)
        } else {
            stream
        };
        self.effects.output.fetch_add(1, Ordering::SeqCst);
        Ok(FollowerExecution {
            stream,
            scan_stats: FollowerScanEvidence(scan_stats),
        })
    }

    /// Returns the directly observed resolver/decode/execution/output counts.
    #[cfg(test)]
    fn effect_counts(&self) -> (usize, usize, usize, usize) {
        (
            self.effects.resolver.load(Ordering::SeqCst),
            self.effects.decode.load(Ordering::SeqCst),
            self.effects.execution.load(Ordering::SeqCst),
            self.effects.output.load(Ordering::SeqCst),
        )
    }

    /// Returns the exact number of fail-closed common-plan placeholder visits.
    #[cfg(test)]
    fn preflight_count(&self) -> usize {
        self.effects.preflight.load(Ordering::SeqCst)
    }

    /// Performs the complete IO-free validation phase.
    ///
    /// # Errors
    /// Returns a preflight error for any malformed request, contradictory
    /// authenticated binding, unsupported plan node, or inconsistent scan set.
    /// Validates one authenticated assignment beyond its identity and binding.
    ///
    /// Runs the checks that are about the assignment's *content* rather than
    /// its identity: the signed projection closure must be non-empty, the
    /// target must be a Scribe, and its provider cut must be present,
    /// internally valid, and pinned to `target_fence`.
    ///
    /// # Errors
    ///
    /// Returns [`PhysicalPlanFollowerError::Preflight`] naming the refused
    /// invariant. Every refusal happens before any provider resolution or
    /// object I/O.
    fn validate_assignment(
        assignment: &FollowerScanAssignment,
        target_role: ClusterRole,
        target_fence: wyrd_spec::vala::api::FencingToken,
    ) -> Result<(), PhysicalPlanFollowerError> {
        // The leader never signs an empty closure: a scan that reads no
        // column still reads `wyrd_event_time` for its row counts.
        if assignment.required_columns.is_empty() {
            return Err(PhysicalPlanFollowerError::Preflight(
                "assignment projection closure is empty".to_owned(),
            ));
        }
        let Some(cut) = assignment
            .scribe_provider_cut
            .as_ref()
            .filter(|_| target_role == ClusterRole::Scribe)
        else {
            return Err(PhysicalPlanFollowerError::Preflight(
                "assignment is not a Scribe hot-provider cut".to_owned(),
            ));
        };
        if cut.writer_epoch != target_fence || !cut.is_valid() {
            return Err(PhysicalPlanFollowerError::Preflight(
                "invalid Scribe provider cut".to_owned(),
            ));
        }
        Ok(())
    }

    fn preflight(
        &self,
        request: &ExecuteFragmentRequest,
        authenticated: &AuthenticatedFollowerContext<'_>,
    ) -> Result<PreflightRequest, PhysicalPlanFollowerError> {
        self.effects.preflight.fetch_add(1, Ordering::SeqCst);
        if request.physical_plan_bytes.is_empty()
            || request.physical_plan_bytes.len() > self.maximum_plan_bytes
            || request.plan_fingerprint.is_empty()
            || request.plan_fingerprint != physical_plan_fingerprint(&request.physical_plan_bytes)
            || request.leader_fence.role != ClusterRole::Oracle
            || request.leader_fence.fencing_token == 0
            || request.target_fence.fencing_token == 0
            || request.leader_fence != authenticated.leader_fence
            || request.target_fence != authenticated.local_fence
            || &request.reservation_id != authenticated.reservation_id
        {
            return Err(PhysicalPlanFollowerError::Preflight(
                "invalid request shape or role fence".to_owned(),
            ));
        }
        let root = PhysicalPlanNode::decode(request.physical_plan_bytes.as_slice())
            .map_err(|error| PhysicalPlanFollowerError::Preflight(error.to_string()))?;
        let mut encoded = HashMap::new();
        Self::collect_extensions(&root, &mut encoded)?;
        let mut assignments = HashMap::with_capacity(request.assignments.len());
        for assignment in &request.assignments {
            if assignment.scan_id.is_empty()
                || assignment.schema_fingerprint.is_empty()
                || assignment.binding.tenant_id == DataTenantId::SYSTEM_OWNER
                || assignment.binding.tenant_id != authenticated.tenant_id
                || assignment.binding.namespace.trim().is_empty()
                || assignment.binding.table.trim().is_empty()
                || &assignment.binding != authenticated.table_binding
                || assignments
                    .insert(assignment.scan_id.clone(), assignment.clone())
                    .is_some()
            {
                return Err(PhysicalPlanFollowerError::Preflight(
                    "invalid or duplicate assignment".to_owned(),
                ));
            }
            Self::validate_assignment(
                assignment,
                request.target_fence.role,
                request.target_fence.fencing_token,
            )?;
        }
        if encoded.len() != assignments.len()
            || encoded.iter().any(|(scan_id, fingerprint)| {
                assignments
                    .get(scan_id)
                    .is_none_or(|assignment| &assignment.schema_fingerprint != fingerprint)
            })
        {
            return Err(PhysicalPlanFollowerError::Preflight(
                "encoded scan set differs from assignments".to_owned(),
            ));
        }
        Ok(PreflightRequest { assignments })
    }

    /// Walks every child of follower-permitted native operators and leaf extensions.
    ///
    /// # Errors
    /// Returns a preflight error for missing inputs, unsupported operators,
    /// malformed extensions, or duplicate scan identities.
    fn collect_extensions(
        node: &PhysicalPlanNode,
        encoded: &mut HashMap<String, String>,
    ) -> Result<(), PhysicalPlanFollowerError> {
        let children: Vec<&PhysicalPlanNode> = match node.physical_plan_type.as_ref() {
            Some(PhysicalPlanType::Extension(extension)) => {
                let payload = OraclePhysicalExtensionCodec::preflight_extension(&extension.node)
                    .map_err(|error| PhysicalPlanFollowerError::Preflight(error.to_string()))?;
                if !extension.inputs.is_empty()
                    || payload.scan_id.is_empty()
                    || payload.schema_fingerprint.is_empty()
                    || encoded
                        .insert(payload.scan_id, payload.schema_fingerprint)
                        .is_some()
                {
                    return Err(PhysicalPlanFollowerError::Preflight(
                        "invalid or duplicate encoded scan".to_owned(),
                    ));
                }
                return Ok(());
            }
            Some(PhysicalPlanType::Projection(value)) => {
                value.input.iter().map(AsRef::as_ref).collect()
            }
            Some(PhysicalPlanType::Filter(value)) => {
                value.input.iter().map(AsRef::as_ref).collect()
            }
            Some(PhysicalPlanType::Aggregate(value)) => {
                value.input.iter().map(AsRef::as_ref).collect()
            }
            Some(PhysicalPlanType::GlobalLimit(value)) => {
                value.input.iter().map(AsRef::as_ref).collect()
            }
            Some(PhysicalPlanType::LocalLimit(value)) => {
                value.input.iter().map(AsRef::as_ref).collect()
            }
            Some(PhysicalPlanType::Sort(value)) => value.input.iter().map(AsRef::as_ref).collect(),
            Some(PhysicalPlanType::SortPreservingMerge(value)) => {
                value.input.iter().map(AsRef::as_ref).collect()
            }
            Some(PhysicalPlanType::CoalesceBatches(value)) => {
                value.input.iter().map(AsRef::as_ref).collect()
            }
            Some(PhysicalPlanType::Merge(value)) => value.input.iter().map(AsRef::as_ref).collect(),
            Some(PhysicalPlanType::Repartition(value)) => {
                value.input.iter().map(AsRef::as_ref).collect()
            }
            Some(PhysicalPlanType::Union(value)) => value.inputs.iter().collect(),
            Some(
                PhysicalPlanType::HashJoin(_)
                | PhysicalPlanType::NestedLoopJoin(_)
                | PhysicalPlanType::CrossJoin(_),
            ) => {
                return Err(PhysicalPlanFollowerError::Preflight(
                    "joins are not supported by the distributed Oracle follower".to_owned(),
                ));
            }
            _ => {
                return Err(PhysicalPlanFollowerError::Preflight(
                    "unsupported follower physical operator".to_owned(),
                ));
            }
        };
        if children.is_empty() {
            return Err(PhysicalPlanFollowerError::Preflight(
                "native operator is missing its input".to_owned(),
            ));
        }
        for child in children {
            Self::collect_extensions(child, encoded)?;
        }
        Ok(())
    }
}

/// Runs the complete authenticated IO-free follower preflight without resolver IO.
///
/// This is the production security gate used before source-fragment decoding.
/// Provider resolution and native decode remain owned by [`PhysicalPlanFollower::decode`].
///
/// # Errors
/// Returns [`PhysicalPlanFollowerError::Preflight`] for any malformed or contradictory binding.
pub fn authenticated_preflight(
    request: &ExecuteFragmentRequest,
    authenticated: &AuthenticatedFollowerContext<'_>,
) -> Result<(), PhysicalPlanFollowerError> {
    /// Resolver that proves the public preflight entry point performs no IO.
    struct NoResolver;
    #[async_trait]
    impl FollowerSourceResolver for NoResolver {
        /// Rejects any accidental resolver call made by the IO-free gate.
        ///
        /// # Errors
        /// Always returns an invariant error because preflight must never call it.
        async fn resolve(
            &self,
            _target_role: ClusterRole,
            _assignment: &FollowerScanAssignment,
            _session: &SessionState,
        ) -> Result<ResolvedFollowerSource, FollowerResolutionError> {
            Err(FollowerResolutionError::Fault(
                "preflight resolver must not run".to_owned(),
            ))
        }
    }
    PhysicalPlanFollower::new(NoResolver)
        .preflight(request, authenticated)
        .map(|_| ())
}

#[cfg(test)]
pub(crate) mod tests {
    //! Authenticated preflight and role-local provider behavior proofs.

    use crate::scribe::tail_rpc::HotBatch;
    use crate::scribe::tail_rpc::tests::StagedGenerationFixture;
    use crate::scribe::wal::WalLsn;
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// Builds a non-spilling execution over a pool bounded to `granted_memory_bytes`.
    ///
    /// # Panics
    /// Panics when `DataFusion` cannot construct the bounded runtime.
    fn bounded_execution(
        granted_memory_bytes: usize,
        target_partitions: usize,
    ) -> crate::resources::OracleExecution {
        crate::resources::OracleExecution::new(
            super::super::spill::build_query_runtime(
                None,
                Arc::new(datafusion::execution::memory_pool::GreedyMemoryPool::new(
                    granted_memory_bytes,
                )),
                0,
            )
            .expect("governed follower runtime"),
            granted_memory_bytes,
            target_partitions,
        )
    }

    use crate::scribe::memtable::Memtable;
    use crate::scribe::seal_key::SealKey;
    use crate::scribe::stream_identity::WriterEpoch;
    use crate::scribe::wal::ScribeAppendMeta;
    use arrow::array::{Int64Array, StringArray};
    use arrow::datatypes::{DataType, Field, Schema};
    use arrow::record_batch::RecordBatch;
    use datafusion::execution::context::SessionContext;
    use datafusion::execution::session_state::SessionStateBuilder;
    use datafusion::physical_plan::{ExecutionPlanProperties, collect};
    use datafusion_proto::bytes::physical_plan_to_bytes_with_extension_codec;
    use wyrd_spec::vala::api::{PeerContext, ScribeProviderCut};

    use super::*;
    use crate::oracle::codec::RemoteSourcePlaceholderExec;

    /// Builds one otherwise-valid Scribe provider cut for focused tests.
    fn cut() -> ScribeProviderCut {
        ScribeProviderCut {
            writer_epoch: 2,
            start_partition: crate::test_support::day_partition(2026, 8, 19).to_wire(),
            end_partition: crate::test_support::day_partition(2026, 8, 19).to_wire(),
        }
    }

    /// Derives the production identity-bound scan ID for one local test stream.
    fn local_scribe_scan_id(binding: &TenantTableBinding, stream: StreamIdentity) -> String {
        super::super::scribe_follower_scan_id(
            &format!("{}.{}", binding.namespace, binding.table),
            wyrd_spec::vala::api::NodeId::new(stream.node_id.as_uuid()),
            u64::try_from(stream.writer_epoch.as_i64()).expect("positive test writer epoch"),
        )
    }

    /// Resolver whose call counter proves the complete preflight gate precedes IO.
    #[derive(Debug)]
    struct CountingResolver {
        /// Number of provider-construction calls.
        calls: Arc<AtomicUsize>,
        /// Schema returned by the isolated provider.
        schema: SchemaRef,
    }

    /// Tail source that records the exact single request received by the resolver.
    #[derive(Debug)]
    struct RecordingTail {
        /// Local stream incarnation.
        stream: StreamIdentity,
        /// Captured requests, expected to contain exactly one member.
        requests: Arc<std::sync::Mutex<Vec<FetchLiveTailRequest>>>,
        /// Active and unretired immutable cohorts returned by the snapshot seam.
        batches: Vec<HotBatch>,
    }

    #[async_trait]
    impl LiveTailSource for RecordingTail {
        /// Returns the configured test stream.
        fn stream(&self) -> StreamIdentity {
            self.stream
        }

        /// Records the unchanged request shape and returns the configured live cohort.
        ///
        /// # Errors
        /// This focused source is infallible.
        async fn open(
            &self,
            request: FetchLiveTailRequest,
        ) -> Result<LiveTailBatches, FollowerResolutionError> {
            self.requests
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .push(request);
            Ok(LiveTailBatches::from_snapshot(self.batches.clone()))
        }
    }

    #[async_trait]
    impl FollowerSourceResolver for CountingResolver {
        /// Returns one isolated empty provider after recording resolution.
        ///
        /// # Errors
        /// This focused resolver is infallible.
        async fn resolve(
            &self,
            _target_role: ClusterRole,
            assignment: &FollowerScanAssignment,
            _session: &SessionState,
        ) -> Result<ResolvedFollowerSource, FollowerResolutionError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            let required_schema = signed_closure_schema(self.schema.as_ref(), assignment)?;
            Ok(ResolvedFollowerSource {
                plan: Arc::new(datafusion::physical_plan::empty::EmptyExec::new(
                    required_schema,
                )),
                full_schema: Arc::clone(&self.schema),
            })
        }
    }

    /// Creates one valid Scribe-role request and its exact authenticated facts.
    ///
    /// # Errors
    /// Returns a plan error if the extension leaf cannot be serialized.
    fn scribe_request() -> datafusion::common::Result<(ExecuteFragmentRequest, TenantTableBinding)>
    {
        let tenant_id = DataTenantId::new_v7();
        let binding = TenantTableBinding {
            tenant_id,
            namespace: "vala.logs".to_owned(),
            table: "records".to_owned(),
        };
        // The closure a leader would sign for `SELECT value FROM ...`.
        let schema = Arc::new(Schema::new(vec![Field::new(
            "value",
            DataType::Int64,
            true,
        )]));
        let fingerprint = super::super::assignment_schema_fingerprint(schema.as_ref());
        let bytes = physical_plan_to_bytes_with_extension_codec(
            Arc::new(RemoteSourcePlaceholderExec::new(
                "scan",
                &fingerprint,
                schema,
            )),
            &OraclePhysicalExtensionCodec::encoder(),
        )?
        .to_vec();
        let leader_fence = OracleRoleFence {
            node_id: wyrd_spec::vala::api::NodeId::new(uuid::Uuid::now_v7()),
            role: ClusterRole::Oracle,
            fencing_token: 11,
        };
        let target_fence = OracleRoleFence {
            node_id: wyrd_spec::vala::api::NodeId::new(uuid::Uuid::now_v7()),
            role: ClusterRole::Scribe,
            fencing_token: cut().writer_epoch,
        };
        Ok((
            ExecuteFragmentRequest {
                context: PeerContext {
                    claims_bytes: vec![1],
                },
                physical_plan_bytes: bytes.clone(),
                reservation_id: ReservationId::new(uuid::Uuid::now_v7()),
                leader_fence,
                target_fence,
                assignments: vec![FollowerScanAssignment {
                    scan_id: "scan".to_owned(),
                    binding: binding.clone(),
                    scribe_provider_cut: Some(cut()),
                    schema_fingerprint: fingerprint,
                    required_columns: vec!["value".to_owned()],
                }],
                plan_fingerprint: physical_plan_fingerprint(&bytes),
            },
            binding,
        ))
    }

    /// Builds the authenticated context tied exactly to a request.
    fn authenticated<'a>(
        request: &'a ExecuteFragmentRequest,
        binding: &'a TenantTableBinding,
    ) -> AuthenticatedFollowerContext<'a> {
        AuthenticatedFollowerContext {
            tenant_id: binding.tenant_id,
            table_binding: binding,
            reservation_id: &request.reservation_id,
            leader_fence: request.leader_fence.clone(),
            local_fence: request.target_fence.clone(),
        }
    }

    /// The local identity fetches hot data while a sibling placeholder resolves empty.
    ///
    /// # Panics
    ///
    /// Panics when either assignment fails to resolve, when the local identity does
    /// not fetch the hot stream exactly once, or when the sibling placeholder
    /// fetches it again.
    #[tokio::test]
    async fn scribe_provider_executes_only_its_identity_bound_scan() {
        let tenant_id = DataTenantId::new_v7();
        let requests = Arc::new(std::sync::Mutex::new(Vec::new()));
        let stream = StreamIdentity::new(
            crate::scribe::stream_identity::NodeId::new(uuid::Uuid::from_u128(11)),
            WriterEpoch::new(2),
        );
        let tail = Arc::new(RecordingTail {
            stream,
            requests: Arc::clone(&requests),
            batches: Vec::new(),
        });
        let schema = Arc::new(Schema::new(vec![Field::new(
            "wyrd_event_time",
            DataType::Utf8,
            true,
        )]));
        let fingerprint = super::super::assignment_schema_fingerprint(schema.as_ref());
        let resolver = ScribeTailResolver::with_schema(tail, schema);
        let binding = TenantTableBinding {
            tenant_id,
            namespace: "vala.logs".to_owned(),
            table: "records".to_owned(),
        };
        let assignment = |scan_id| FollowerScanAssignment {
            scan_id,
            binding: binding.clone(),
            scribe_provider_cut: Some(cut()),
            schema_fingerprint: fingerprint.clone(),
            required_columns: vec!["wyrd_event_time".to_owned()],
        };
        let session = SessionContext::new_with_config(
            crate::resources::OracleSessionShape::for_test().session_config(),
        )
        .state();
        resolver
            .resolve(
                ClusterRole::Scribe,
                &assignment(local_scribe_scan_id(&binding, stream)),
                &session,
            )
            .await
            .expect("the exact local identity resolves its hot provider");
        assert_eq!(
            requests
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .len(),
            1
        );
        let sibling = super::super::scribe_follower_scan_id(
            "vala.logs.records",
            wyrd_spec::vala::api::NodeId::new(uuid::Uuid::from_u128(12)),
            3,
        );
        resolver
            .resolve(ClusterRole::Scribe, &assignment(sibling), &session)
            .await
            .expect("a sibling placeholder resolves as an explicit empty provider");
        assert_eq!(
            requests
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .len(),
            1,
            "the sibling placeholder must not fetch the local hot stream"
        );
    }

    /// Scribe resolution keeps the cause class the dispatch decision needs.
    ///
    /// A cut for another writer epoch is source loss, a fingerprint that
    /// differs from the authenticated schema is a fault that never reaches the
    /// tail, and live-read open failures keep their incarnation or fault
    /// class instead of collapsing into one message.
    ///
    /// # Panics
    /// Panics when a class differs or a refused assignment reaches the tail.
    #[tokio::test]
    async fn scribe_resolution_failures_keep_their_class() {
        let requests = Arc::new(std::sync::Mutex::new(Vec::new()));
        let stream = StreamIdentity::new(
            crate::scribe::stream_identity::NodeId::new(uuid::Uuid::from_u128(11)),
            WriterEpoch::new(2),
        );
        let schema = Arc::new(Schema::new(vec![Field::new(
            "wyrd_event_time",
            DataType::Utf8,
            true,
        )]));
        let resolver = ScribeTailResolver::with_schema(
            Arc::new(RecordingTail {
                stream,
                requests: Arc::clone(&requests),
                batches: Vec::new(),
            }),
            Arc::clone(&schema),
        );
        let binding = TenantTableBinding {
            tenant_id: DataTenantId::new_v7(),
            namespace: "vala.logs".to_owned(),
            table: "records".to_owned(),
        };
        let assignment = FollowerScanAssignment {
            scan_id: local_scribe_scan_id(&binding, stream),
            binding: binding.clone(),
            scribe_provider_cut: Some(cut()),
            schema_fingerprint: super::super::assignment_schema_fingerprint(schema.as_ref()),
            required_columns: vec!["wyrd_event_time".to_owned()],
        };
        let session = SessionContext::new_with_config(
            crate::resources::OracleSessionShape::for_test().session_config(),
        )
        .state();

        let mut other_epoch = assignment.clone();
        if let Some(cut) = other_epoch.scribe_provider_cut.as_mut() {
            cut.writer_epoch += 1;
        }
        let mut other_schema = assignment.clone();
        other_schema.schema_fingerprint = super::super::assignment_schema_fingerprint(
            &Schema::new(vec![Field::new("other", DataType::Utf8, false)]),
        );
        for (refused, expected) in [(other_epoch, "source loss"), (other_schema, "fault")] {
            let class = match resolver
                .resolve(ClusterRole::Scribe, &refused, &session)
                .await
            {
                Err(FollowerResolutionError::SourceLoss(_)) => "source loss",
                Err(FollowerResolutionError::Fault(_)) => "fault",
                Ok(_) => "resolved",
            };
            assert_eq!(class, expected);
        }
        assert!(
            requests
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .is_empty(),
            "a refused assignment must never reach the live tail"
        );

        assert!(matches!(
            live_open_error(&ScribeError::StreamMismatch {
                requested: stream,
                actual: stream,
            }),
            FollowerResolutionError::SourceLoss(_)
        ));
        assert!(matches!(
            live_open_error(&ScribeError::Internal {
                detail: "staged schema".to_owned(),
            }),
            FollowerResolutionError::Fault(_)
        ));
    }

    /// A Scribe assignment resolves through its role-local provider exactly once.
    ///
    /// # Panics
    /// Panics if the canonical request cannot be built or the validated plan
    /// cannot decode and execute.
    #[tokio::test]
    async fn role_local_providers_are_isolated_and_consumed_once() {
        prove_role_local_providers_are_isolated_and_consumed_once().await;
    }

    /// Exercises one complete role-local provider resolution and execution.
    async fn prove_role_local_providers_are_isolated_and_consumed_once() {
        let (request, binding) = scribe_request().expect("valid Scribe request");
        let calls = Arc::new(AtomicUsize::new(0));
        let schema = Arc::new(Schema::new(vec![Field::new(
            "value",
            DataType::Int64,
            true,
        )]));
        let follower = PhysicalPlanFollower::new(CountingResolver {
            calls: Arc::clone(&calls),
            schema,
        });
        let stream = follower
            .execute(
                &request,
                authenticated(&request, &binding),
                &bounded_execution(1024 * 1024, 1),
            )
            .await
            .expect("validated provider decodes and executes");
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert_eq!(follower.preflight_count(), 2);
        assert_eq!(stream.split().0.schema().fields().len(), 1);
    }

    /// Proves a follower refuses empty fragments and that its partition count
    /// and sort-merge reservation come from one
    /// [`OracleSessionShape`](crate::resources::OracleSessionShape) built from
    /// this node's own admission, never from a value the requesting peer
    /// supplied.
    #[test]
    fn admitted_session_shape_contract() {
        let assignment = |cut: bool| FollowerScanAssignment {
            scan_id: "shape-contract-scan".to_owned(),
            binding: TenantTableBinding {
                tenant_id: DataTenantId::new_v7(),
                namespace: "vala.bifrost".to_owned(),
                table: "events".to_owned(),
            },
            scribe_provider_cut: cut.then(self::cut),
            schema_fingerprint: "shape".to_owned(),
            required_columns: vec!["wyrd_event_time".to_owned()],
        };

        require_scannable_work(&[assignment(true)]).expect("scannable fragment");
        assert!(
            require_scannable_work(&[assignment(false)]).is_err(),
            "a fragment with no scannable work is a contract failure"
        );
        assert!(
            require_scannable_work(&[]).is_err(),
            "an empty assignment set is a contract failure"
        );

        let granted = crate::resources::ORACLE_PARTITION_MEMORY_BYTES;
        let execution = bounded_execution(granted, 8);
        assert_eq!(
            execution.shape(),
            crate::resources::OracleSessionShape::new(8)
        );
        let (state, _context) = request_session(&execution);
        assert_eq!(state.config().options().execution.target_partitions, 8);
    }

    /// Every follower session enables Parquet-level predicate and index
    /// pushdown, so a closed leaf predicate actually prunes row groups and
    /// pages at the reader rather than only being re-applied by the
    /// residual `FilterExec` `DataFusion` keeps above the provider.
    #[tokio::test]
    async fn oracle_reader_session_options_contract() {
        let (state, _context) = request_session(&bounded_execution(1024, 1));
        let parquet_options = &state.config().options().execution.parquet;
        assert!(parquet_options.pushdown_filters);
        assert!(parquet_options.reorder_filters);
        assert!(parquet_options.bloom_filter_on_read);
        assert!(parquet_options.enable_page_index);
    }

    /// Proves every Oracle follower request rebuilds its governed `DataFusion` context.
    ///
    /// # Panics
    /// Panics if either independent request fails provider resolution, native
    /// decode, unsupported-operator preflight, or stream construction.
    #[tokio::test]
    async fn oracle_resolver_builds_fresh_request_local_context() {
        let execution = bounded_execution(1024, 1);
        let (first_state, first_context) = request_session(&execution);
        let (second_state, second_context) = request_session(&execution);
        assert_ne!(first_state.session_id(), second_state.session_id());
        assert!(!Arc::ptr_eq(&first_context, &second_context));
        prove_role_local_providers_are_isolated_and_consumed_once().await;
        prove_role_local_providers_are_isolated_and_consumed_once().await;
        assert_complete_preflight_matrix().await;
    }

    /// Builds every request mutation a follower decode must refuse.
    ///
    /// The cases cover each independent authority the decode checks: a physical
    /// plan whose join loses a child, a reservation the leader never issued, an
    /// assignment whose namespace, table, or tenant does not match the
    /// authenticated binding, a fence naming the wrong role or a stale token, an
    /// missing Scribe provider cut, an unknown scan id, a wrong schema
    /// fingerprint, a duplicated assignment, and undecodable plan bytes. Each
    /// case mutates exactly one field so a refusal cannot be attributed to a
    /// second defect.
    ///
    /// # Panics
    ///
    /// Panics if the fixture request has no assignments to mutate.
    fn malformed_follower_requests(
        request: &ExecuteFragmentRequest,
        first: &PhysicalPlanNode,
    ) -> Vec<ExecuteFragmentRequest> {
        let mut malformed = Vec::new();
        let mut case = request.clone();
        case.physical_plan_bytes = PhysicalPlanNode {
            physical_plan_type: Some(PhysicalPlanType::HashJoin(Box::new(
                datafusion_proto::protobuf::HashJoinExecNode {
                    left: Some(Box::new(first.clone())),
                    ..Default::default()
                },
            ))),
        }
        .encode_to_vec();
        case.plan_fingerprint = physical_plan_fingerprint(&case.physical_plan_bytes);
        malformed.push(case);
        let mut case = request.clone();
        case.reservation_id = ReservationId::new(uuid::Uuid::now_v7());
        malformed.push(case);
        let mut case = request.clone();
        case.assignments[0].binding.namespace = "vala.traces".to_owned();
        malformed.push(case);
        let mut case = request.clone();
        case.assignments[0].binding.table = "other".to_owned();
        malformed.push(case);
        let mut case = request.clone();
        case.assignments[0].binding.tenant_id = DataTenantId::new_v7();
        malformed.push(case);
        let mut case = request.clone();
        case.target_fence.role = ClusterRole::Oracle;
        malformed.push(case);
        let mut case = request.clone();
        case.target_fence.fencing_token += 1;
        malformed.push(case);
        let mut case = request.clone();
        case.assignments[0].scribe_provider_cut = None;
        malformed.push(case);
        let mut case = request.clone();
        case.assignments[0].scan_id = "unknown".to_owned();
        malformed.push(case);
        let mut case = request.clone();
        case.assignments[0].schema_fingerprint = "sha256:wrong".to_owned();
        malformed.push(case);
        let mut case = request.clone();
        case.assignments.push(case.assignments[0].clone());
        malformed.push(case);
        let mut case = request.clone();
        case.physical_plan_bytes = vec![0];
        case.plan_fingerprint = physical_plan_fingerprint(&case.physical_plan_bytes);
        malformed.push(case);
        malformed
    }

    /// Builds the Scribe cohort fixture: two frozen generations plus a live one.
    ///
    /// Two rows are inserted and frozen individually so the memtable holds two
    /// distinct unretired immutable generations, then a third row is left in the
    /// active generation. This is the exact shape the resolver must project in
    /// full — active plus every unretired immutable — so a resolver that
    /// returned only the active generation, or only the newest immutable one,
    /// would be caught.
    ///
    /// # Panics
    ///
    /// Panics if any insert or freeze fails, which would mean the fixture no
    /// longer has the cohort shape the test asserts against.
    fn scribe_cohort_memtable(
        tenant_id: DataTenantId,
        day: TimePartition,
        schema: &Arc<Schema>,
    ) -> (SealKey, Arc<Memtable>) {
        let schema = Arc::clone(schema);
        let batch = |value: &str| {
            arrow::record_batch::RecordBatch::try_new(
                Arc::clone(&schema),
                vec![Arc::new(StringArray::from(vec![value]))],
            )
            .expect("valid cohort batch")
        };
        let table = TableRef::parse_fqn("vala.logs.records").expect("canonical table");
        let key = SealKey::new(tenant_id, table, day);
        let memtable = Arc::new(Memtable::new());
        let meta = |lsn| ScribeAppendMeta {
            batch_id: *uuid::Uuid::now_v7().as_bytes(),
            schema_fingerprint: [0; 32],
            data_digest: [0; 32],
            data_len: 0,
            payload_digest: [0; 32],
            payload_len: 0,
            slice_index: 0,
            slice_count: 1,
            rows_accepted: 1,
            wal_lsn_min: WalLsn::new(lsn),
            wal_lsn_max: WalLsn::new(lsn),
            seal_key: key.to_string(),
        };
        memtable
            .insert(&key, meta(7), batch("immutable-one"))
            .expect("first immutable row inserts");
        memtable
            .freeze(&key)
            .expect("first immutable generation freezes");
        memtable
            .insert(&key, meta(8), batch("immutable-two"))
            .expect("second immutable row inserts");
        memtable
            .freeze(&key)
            .expect("second immutable generation freezes");
        memtable
            .insert(&key, meta(9), batch("active"))
            .expect("active row inserts");
        (key, memtable)
    }

    /// The Scribe resolver projects the complete active-plus-unretired snapshot cohort.
    ///
    /// # Panics
    /// Panics if canonical fixture construction, Memtable generation freezing,
    /// provider resolution, or cohort execution violates its test invariant.
    #[tokio::test]
    async fn scribe_provider_is_active_plus_all_unretired_immutable() {
        let tenant_id = DataTenantId::new_v7();
        let schema = Arc::new(Schema::new(vec![Field::new(
            "wyrd_event_time",
            DataType::Utf8,
            true,
        )]));
        let day = crate::test_support::day_partition(2026, 8, 19);
        let (_key, memtable) = scribe_cohort_memtable(tenant_id, day, &schema);
        let stream = StreamIdentity::new(
            crate::scribe::stream_identity::NodeId::new(uuid::Uuid::now_v7()),
            WriterEpoch::new(2),
        );
        let service = Arc::new(FetchLiveTailService::new(stream, memtable));
        let resolver = ScribeTailResolver::with_schema(service, Arc::clone(&schema));
        let session = SessionContext::new_with_config(
            crate::resources::OracleSessionShape::for_test().session_config(),
        )
        .state();
        let binding = TenantTableBinding {
            tenant_id,
            namespace: "vala.logs".to_owned(),
            table: "records".to_owned(),
        };
        let provider = resolver
            .resolve(
                ClusterRole::Scribe,
                &FollowerScanAssignment {
                    scan_id: local_scribe_scan_id(&binding, stream),
                    binding,
                    scribe_provider_cut: Some(cut()),
                    schema_fingerprint: super::super::assignment_schema_fingerprint(
                        schema.as_ref(),
                    ),
                    // The resolver now projects the assignment's own closure,
                    // so this fixture names the single column its memtable
                    // schema actually carries.
                    required_columns: vec!["wyrd_event_time".to_owned()],
                },
                &session,
            )
            .await
            .expect("snapshot cohort resolves");
        let rows = collect(provider.plan, Arc::new(TaskContext::default()))
            .await
            .expect("snapshot cohort executes");
        let mut cohort = rows
            .iter()
            .flat_map(|batch| {
                batch
                    .column(0)
                    .as_any()
                    .downcast_ref::<StringArray>()
                    .expect("cohort preserves its authenticated schema")
                    .iter()
                    .flatten()
                    .map(str::to_owned)
            })
            .collect::<Vec<_>>();
        cohort.sort();
        assert_eq!(cohort, vec!["active", "immutable-one", "immutable-two"]);
    }

    /// The four-column closure fixture schema shared by the Scribe follower owner.
    ///
    /// `unused_payload` is the wide column no part of the query requests, so a
    /// resolver that ignores the signed closure is visible in the resolved
    /// schema rather than only in byte counters.
    fn closure_fixture_schema() -> Arc<Schema> {
        Arc::new(Schema::new(vec![
            Field::new("unused_payload", DataType::Utf8, true),
            Field::new("duration_ms", DataType::Int64, true),
            Field::new("status_code", DataType::Utf8, true),
        ]))
    }

    /// Builds a memtable holding one ERROR row and one OK row at that schema.
    ///
    /// Both rows are inserted into the active generation: this owner is about
    /// projection and predicate application, not generation retirement, which
    /// [`scribe_provider_is_active_plus_all_unretired_immutable`] already pins.
    ///
    /// # Panics
    ///
    /// Panics if the fixture rows cannot be built or inserted, which would mean
    /// the memtable no longer has the shape this owner asserts against.
    fn closure_fixture_memtable(
        tenant_id: DataTenantId,
        day: TimePartition,
        schema: &Arc<Schema>,
    ) -> (SealKey, Arc<Memtable>) {
        use arrow::array::Int64Array;

        let table = TableRef::parse_fqn("vala.traces.spans").expect("canonical table");
        let key = SealKey::new(tenant_id, table, day);
        let memtable = Arc::new(Memtable::new());
        let batch = arrow::record_batch::RecordBatch::try_new(
            Arc::clone(schema),
            vec![
                Arc::new(StringArray::from(vec!["wide-error", "wide-ok"])),
                Arc::new(Int64Array::from(vec![41_i64, 97_i64])),
                Arc::new(StringArray::from(vec![
                    "STATUS_CODE_ERROR",
                    "STATUS_CODE_OK",
                ])),
            ],
        )
        .expect("closure fixture batch");
        let meta = ScribeAppendMeta {
            batch_id: *uuid::Uuid::now_v7().as_bytes(),
            schema_fingerprint: [0; 32],
            data_digest: [0; 32],
            data_len: 0,
            payload_digest: [0; 32],
            payload_len: 0,
            slice_index: 0,
            slice_count: 1,
            rows_accepted: 2,
            wal_lsn_min: WalLsn::new(7),
            wal_lsn_max: WalLsn::new(7),
            seal_key: key.to_string(),
        };
        memtable
            .insert(&key, meta, batch)
            .expect("closure fixture rows insert");
        (key, memtable)
    }

    /// Builds one Scribe assignment carrying the signed closure.
    fn closure_assignment(
        scan_id: String,
        binding: TenantTableBinding,
        schema: &Schema,
        required_columns: Vec<String>,
        schema_fingerprint: String,
    ) -> FollowerScanAssignment {
        let _ = schema;
        FollowerScanAssignment {
            scan_id,
            binding,
            scribe_provider_cut: Some(cut()),
            schema_fingerprint,
            required_columns,
        }
    }

    /// Returns the tenant-scoped binding the closure fixtures resolve against.
    fn closure_fixture_binding(tenant_id: DataTenantId) -> TenantTableBinding {
        TenantTableBinding {
            tenant_id,
            namespace: "vala.traces".to_owned(),
            table: "spans".to_owned(),
        }
    }

    /// Returns the signed closure for
    /// `SELECT duration_ms ... WHERE status_code = 'STATUS_CODE_ERROR'`: the
    /// requested output and the predicate-only column, in closure order.
    fn signed_closure() -> Vec<String> {
        vec!["duration_ms".to_owned(), "status_code".to_owned()]
    }

    /// Returns one plan's output field names in closure order.
    fn plan_column_names(plan: &Arc<dyn ExecutionPlan>) -> Vec<String> {
        plan.schema()
            .fields()
            .iter()
            .map(|field| field.name().clone())
            .collect()
    }

    /// Stands up the Scribe live-tail resolver the closure owner interrogates.
    ///
    /// Returns the owning tenant, the complete fixture schema, the writer
    /// stream whose identity names the local scan id, and a resolver backed by
    /// a real `FetchLiveTailService` over the seeded memtable. Keeping this
    /// construction here leaves the owning test to assert only closure
    /// behavior.
    ///
    /// # Panics
    /// Panics if the Scribe role capability cannot be composed, since role
    /// composition is not the behavior under test.
    fn closure_fixture_resolver() -> (
        DataTenantId,
        Arc<Schema>,
        StreamIdentity,
        ScribeTailResolver,
    ) {
        let tenant_id = DataTenantId::new_v7();
        let schema = closure_fixture_schema();
        let day = crate::test_support::day_partition(2026, 8, 19);
        let (_key, memtable) = closure_fixture_memtable(tenant_id, day, &schema);
        let stream = StreamIdentity::new(
            crate::scribe::stream_identity::NodeId::new(uuid::Uuid::now_v7()),
            WriterEpoch::new(2),
        );
        let service = Arc::new(FetchLiveTailService::new(stream, memtable));
        let resolver = ScribeTailResolver::with_schema(service, Arc::clone(&schema));
        (tenant_id, schema, stream, resolver)
    }

    /// The Scribe follower fetches and exposes exactly the signed closure, at a
    /// non-zero ordinal, and validates the full schema before any source IO.
    ///
    /// This is the production live-tail leaf for
    /// `SELECT duration_ms ... WHERE status_code = 'STATUS_CODE_ERROR'`. Two
    /// distinct properties are pinned together because they are one contract:
    ///
    /// 1. the fetch request copies `assignment.required_columns` byte-for-byte,
    ///    so the memtable projects at the source;
    /// 2. the resolved leaf — and the non-owning empty branch, which produces
    ///    no rows at all — declares that same closure schema, so the serialized
    ///    plan's `Column` indices are closure indices on every branch.
    ///
    /// The refusal half of the contract — an assignment whose *full* schema
    /// fingerprint does not match — is pinned by
    /// `scribe_provider_refuses_a_mismatched_full_schema_fingerprint`.
    ///
    /// `duration_ms` sits at closure ordinal 0 but complete-schema ordinal 1,
    /// so a leaf that declares the closure while returning complete-schema rows
    /// would read the wrong column rather than merely a wide one.
    ///
    /// # Panics
    ///
    /// Panics if resolution, execution, or the recorded request violates the
    /// closure contract this owner pins.
    #[tokio::test]
    async fn scribe_provider_projects_a_nonzero_ordinal() {
        use arrow::array::Int64Array;

        let (tenant_id, schema, stream, resolver) = closure_fixture_resolver();
        let session = SessionContext::new_with_config(
            crate::resources::OracleSessionShape::for_test().session_config(),
        )
        .state();
        let binding = closure_fixture_binding(tenant_id);
        let closure = signed_closure();
        let fingerprint = super::super::assignment_schema_fingerprint(schema.as_ref());

        let owned = resolver
            .resolve(
                ClusterRole::Scribe,
                &closure_assignment(
                    local_scribe_scan_id(&binding, stream),
                    binding.clone(),
                    schema.as_ref(),
                    closure.clone(),
                    fingerprint.clone(),
                ),
                &session,
            )
            .await
            .expect("signed closure resolves");
        assert_eq!(
            plan_column_names(&owned.plan),
            closure,
            "the live-tail leaf exposes exactly the signed closure"
        );

        let rows = collect(owned.plan, Arc::new(TaskContext::default()))
            .await
            .expect("closure cohort executes");
        let durations = rows
            .iter()
            .flat_map(|batch| {
                batch
                    .column(0)
                    .as_any()
                    .downcast_ref::<Int64Array>()
                    .expect("closure ordinal 0 is duration_ms")
                    .iter()
                    .flatten()
            })
            .collect::<Vec<_>>();
        assert_eq!(
            durations,
            vec![41_i64, 97],
            "every row reads its duration from closure ordinal 0"
        );

        // The non-owning branch produces no rows but must still declare the
        // same closure schema as the owning branch.
        let unowned = resolver
            .resolve(
                ClusterRole::Scribe,
                &closure_assignment(
                    "vala.traces.spans:scribe:someone-else".to_owned(),
                    binding.clone(),
                    schema.as_ref(),
                    closure.clone(),
                    fingerprint.clone(),
                ),
                &session,
            )
            .await
            .expect("a non-owning assignment resolves to an empty branch");
        assert_eq!(
            plan_column_names(&unowned.plan),
            closure,
            "the empty branch declares the same closure schema"
        );

        // Property 1: the fetch request carries the signed closure byte for
        // byte, so the memtable projects at the source instead of the plan
        // above discarding wide columns later.
        let requests = Arc::new(std::sync::Mutex::new(Vec::new()));
        let recording = ScribeTailResolver::with_schema(
            Arc::new(RecordingTail {
                stream,
                requests: Arc::clone(&requests),
                batches: Vec::new(),
            }),
            Arc::clone(&schema),
        );
        let assignment = closure_assignment(
            local_scribe_scan_id(&binding, stream),
            binding,
            schema.as_ref(),
            closure.clone(),
            fingerprint,
        );
        recording
            .resolve(ClusterRole::Scribe, &assignment, &session)
            .await
            .expect("the recording source resolves the same signed closure");
        let recorded = requests
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone();
        assert_eq!(recorded.len(), 1, "exactly one live-tail fetch is issued");
        assert_eq!(
            recorded[0].required_columns, closure,
            "the fetch request forwards the signed closure unchanged"
        );
    }

    /// A mismatched *full* schema fingerprint refuses before any source IO.
    ///
    /// The closure a leader signs narrows the columns a leaf returns; it never
    /// narrows the identity the leaf authenticates against. This owner pins the
    /// negative half of that rule: when the assignment's complete-schema
    /// fingerprint disagrees with the follower's pinned schema, resolution
    /// refuses, and the live-tail service is never asked for a single batch, so
    /// a forged or stale assignment cannot cause a read at all.
    ///
    /// # Panics
    ///
    /// Panics if resolution accepts the mismatch or if the live-tail source was
    /// touched despite the refusal.
    #[tokio::test]
    async fn scribe_provider_refuses_a_mismatched_full_schema_fingerprint() {
        let (tenant_id, schema, stream, _resolver) = closure_fixture_resolver();
        let session = SessionContext::new_with_config(
            crate::resources::OracleSessionShape::for_test().session_config(),
        )
        .state();
        let binding = closure_fixture_binding(tenant_id);
        let closure = signed_closure();

        let requests = Arc::new(std::sync::Mutex::new(Vec::new()));
        let counting = ScribeTailResolver::with_schema(
            Arc::new(RecordingTail {
                stream,
                requests: Arc::clone(&requests),
                batches: Vec::new(),
            }),
            Arc::clone(&schema),
        );
        assert!(
            counting
                .resolve(
                    ClusterRole::Scribe,
                    &closure_assignment(
                        local_scribe_scan_id(&binding, stream),
                        binding,
                        schema.as_ref(),
                        closure,
                        "sha256:wrong".to_owned(),
                    ),
                    &session,
                )
                .await
                .is_err(),
            "a mismatched full-schema fingerprint refuses resolution"
        );
        assert_eq!(
            requests
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .len(),
            0,
            "the live-tail source is never touched by a refused assignment"
        );
    }

    /// Resolves the staged-generation fixture's own Scribe assignment.
    ///
    /// The fixture's generation is marked durable first, so its rows are
    /// served only by its staged runs and the resolved leaf must read them.
    ///
    /// # Panics
    /// Panics when the fixture cannot be marked durable or resolution fails.
    async fn resolve_staged_fixture(
        fixture: &StagedGenerationFixture,
        service: Arc<FetchLiveTailService>,
        session: &SessionState,
    ) -> Arc<dyn ExecutionPlan> {
        fixture
            .memtable
            .complete_staged(fixture.generation.get(), fixture.member)
            .expect("the shard marks the generation durable");
        let schema = Arc::new(Schema::new(vec![Field::new(
            "value",
            DataType::Int64,
            false,
        )]));
        let stream = service.stream();
        let binding = TenantTableBinding {
            tenant_id: fixture.key.tenant,
            namespace: "vala.bifrost".to_owned(),
            table: "events".to_owned(),
        };
        let resolver = ScribeTailResolver::with_schema(service, Arc::clone(&schema));
        let assignment = FollowerScanAssignment {
            scan_id: local_scribe_scan_id(&binding, stream),
            binding,
            scribe_provider_cut: Some(ScribeProviderCut {
                writer_epoch: u64::try_from(stream.writer_epoch.as_i64())
                    .expect("positive fixture epoch"),
                start_partition: fixture.request.start_partition.to_wire(),
                end_partition: fixture.request.end_partition.to_wire(),
            }),
            schema_fingerprint: super::super::assignment_schema_fingerprint(schema.as_ref()),
            required_columns: vec!["value".to_owned()],
        };
        resolver
            .resolve(ClusterRole::Scribe, &assignment, session)
            .await
            .expect("the staged assignment resolves")
            .plan
    }

    /// A selective filter skips the staged row groups that cannot match.
    ///
    /// The staged run holds `[1, 2, 3]` one row per row group. A `value = 2`
    /// `FilterExec` over the resolved Scribe leaf, pushed into its scan by
    /// `DataFusion`'s own `FilterPushdown`, must return only `2` and record two
    /// pruned row groups in the scan evidence a follower footer reports: staged
    /// runs prune natively rather than being decoded whole and filtered after.
    ///
    /// # Panics
    /// Panics when resolution or execution fails, the wrong rows survive, or
    /// the scan evidence records no scanned bytes or no pruned row group.
    #[tokio::test]
    async fn scribe_staged_scan_prunes_non_matching_row_groups() {
        use datafusion::physical_optimizer::PhysicalOptimizerRule as _;

        let fixture = crate::scribe::tail_rpc::tests::staged_generation_fixture_with_runs(1);
        let service = Arc::clone(&fixture.service);
        let session = SessionContext::new_with_config(
            crate::resources::OracleSessionShape::for_test().session_config(),
        )
        .state();
        let leaf = resolve_staged_fixture(&fixture, service, &session).await;
        let predicate = datafusion::physical_expr::expressions::binary(
            Arc::new(datafusion::physical_expr::expressions::Column::new(
                "value", 0,
            )),
            datafusion::logical_expr::Operator::Eq,
            datafusion::physical_expr::expressions::lit(2_i64),
            &leaf.schema(),
        )
        .expect("value = 2");
        let plan = datafusion::physical_optimizer::filter_pushdown::FilterPushdown::new()
            .optimize(
                Arc::new(
                    datafusion::physical_plan::filter::FilterExec::try_new(predicate, leaf)
                        .expect("filter plans"),
                ),
                session.config_options(),
            )
            .expect("filter pushdown");
        let evidence = FollowerScanEvidence(super::super::exec::OracleQueryScanStats::from_plan(
            plan.as_ref(),
        ));
        let rows = collect(plan, Arc::new(TaskContext::default()))
            .await
            .expect("the staged scan executes");
        let values = rows
            .iter()
            .flat_map(|batch| {
                batch
                    .column(0)
                    .as_any()
                    .downcast_ref::<Int64Array>()
                    .expect("Int64 value column")
                    .values()
                    .to_vec()
            })
            .collect::<Vec<_>>();
        assert_eq!(values, vec![2], "only the matching staged row survives");
        let stats = evidence.finalize();
        assert_eq!(stats.files_scanned, 1, "the staged run is opened once");
        assert!(
            stats.bytes_scanned.is_some_and(|bytes| bytes > 0),
            "the staged run's Parquet bytes reach the footer evidence"
        );
        assert_eq!(stats.row_groups_scanned, 1, "one row group can match");
        assert_eq!(
            stats.row_groups_pruned, 2,
            "the other row groups are skipped"
        );
    }

    /// The staged lease lives exactly as long as the Scribe leaf reading it.
    ///
    /// The generation is staged as two runs. A stream that yielded its first
    /// batch keeps the lease; dropping the stream and the plan releases it at
    /// once, so nothing keeps reading on a dropped read's behalf.
    ///
    /// # Panics
    /// Panics when the stream fails or the lease outlives the dropped read.
    #[tokio::test]
    async fn a_dropped_staged_scan_releases_its_lease_immediately() {
        let fixture = crate::scribe::tail_rpc::tests::staged_generation_fixture_with_runs(2);
        let service = Arc::clone(&fixture.service);
        // One partition, so the stream is the leaf's own rather than a
        // coalescing stream whose spawned partition tasks end asynchronously.
        let session = SessionContext::new_with_config(
            crate::resources::OracleSessionShape::new(1).session_config(),
        )
        .state();
        let plan = resolve_staged_fixture(&fixture, service, &session).await;
        let leases = || {
            fixture
                .registry
                .leases(&fixture.key, fixture.generation)
                .expect("registry is readable")
        };
        assert_eq!(leases(), 1, "the resolved leaf holds the lease");
        let mut stream = execute_stream(Arc::clone(&plan), Arc::new(TaskContext::default()))
            .expect("the staged scan executes");
        let first = futures_util::StreamExt::next(&mut stream)
            .await
            .expect("the staged scan yields")
            .expect("the staged run reads");
        assert!(first.num_rows() > 0);
        assert_eq!(leases(), 1, "an open stream keeps the lease");
        drop(stream);
        drop(plan);
        assert_eq!(leases(), 0, "dropping the read releases the lease");
    }

    /// Returns the partition count each live source under `plan` advertises.
    ///
    /// Walks past a mixed `UnionExec` to the memory and staged leaves, in the
    /// order the resolver built them.
    fn live_source_partitions(plan: &Arc<dyn ExecutionPlan>) -> Vec<usize> {
        if plan.children().is_empty() {
            return vec![plan.output_partitioning().partition_count()];
        }
        plan.children()
            .into_iter()
            .flat_map(live_source_partitions)
            .collect()
    }

    /// Returns every value of each batch's first column, which must be `Int64`.
    ///
    /// # Panics
    /// Panics when a batch's first column is not `Int64`.
    fn first_int64_values(rows: &[RecordBatch]) -> Vec<i64> {
        rows.iter()
            .flat_map(|batch| {
                batch
                    .column(0)
                    .as_any()
                    .downcast_ref::<Int64Array>()
                    .expect("first column is Int64")
                    .values()
                    .to_vec()
            })
            .collect()
    }

    /// Each live source reads at its own partition count and every row once.
    ///
    /// At four target partitions, an empty snapshot and a one-batch memtable
    /// cut (fewer batches than partitions) give the memtable four partitions,
    /// with unused groups left empty, while a small staged run reads as one
    /// partition by `DataFusion`'s scan rules. Draining every partition returns
    /// each row exactly once; the query predicate is the fragment's own
    /// `FilterExec`, not the resolver's.
    ///
    /// # Panics
    /// Panics when resolution or execution fails, a source advertises a
    /// different partition count, or the drained rows are not exactly once.
    #[tokio::test]
    async fn scribe_live_sources_read_every_row_once_at_their_partition_count() {
        let session = SessionContext::new_with_config(
            crate::resources::OracleSessionShape::new(4).session_config(),
        )
        .state();

        // One memtable batch.
        let (tenant_id, schema, stream, resolver) = closure_fixture_resolver();
        let binding = closure_fixture_binding(tenant_id);
        let fingerprint = super::super::assignment_schema_fingerprint(schema.as_ref());
        let assignment = closure_assignment(
            local_scribe_scan_id(&binding, stream),
            binding.clone(),
            schema.as_ref(),
            signed_closure(),
            fingerprint.clone(),
        );
        let plan = resolver
            .resolve(ClusterRole::Scribe, &assignment, &session)
            .await
            .expect("the one-batch cut resolves")
            .plan;
        assert_eq!(live_source_partitions(&plan), vec![4]);
        let rows = collect(plan, Arc::new(TaskContext::default()))
            .await
            .expect("the one-batch cut executes");
        let mut values = first_int64_values(&rows);
        values.sort_unstable();
        assert_eq!(
            values,
            vec![41, 97],
            "every memtable row is read exactly once"
        );

        // An empty snapshot.
        let empty = ScribeTailResolver::with_schema(
            Arc::new(RecordingTail {
                stream,
                requests: Arc::new(std::sync::Mutex::new(Vec::new())),
                batches: Vec::new(),
            }),
            Arc::clone(&schema),
        );
        let plan = empty
            .resolve(ClusterRole::Scribe, &assignment, &session)
            .await
            .expect("the empty cut resolves")
            .plan;
        assert_eq!(live_source_partitions(&plan), vec![4]);
        let rows = collect(plan, Arc::new(TaskContext::default()))
            .await
            .expect("the empty cut executes");
        assert_eq!(rows.iter().map(RecordBatch::num_rows).sum::<usize>(), 0);

        // A mixed cut: staged runs `[1, 2, 3]` plus one newer memtable batch.
        let fixture = crate::scribe::tail_rpc::tests::staged_generation_fixture_with_runs(1);
        let newer = RecordBatch::try_new(
            Arc::new(Schema::new(vec![Field::new(
                "value",
                DataType::Int64,
                false,
            )])),
            vec![Arc::new(Int64Array::from(vec![10_i64]))],
        )
        .expect("newer memtable batch");
        fixture
            .memtable
            .insert(
                &fixture.key,
                ScribeAppendMeta {
                    batch_id: *uuid::Uuid::now_v7().as_bytes(),
                    schema_fingerprint: [0; 32],
                    data_digest: [0; 32],
                    data_len: 0,
                    payload_digest: [0; 32],
                    payload_len: 0,
                    slice_index: 0,
                    slice_count: 1,
                    rows_accepted: 1,
                    wal_lsn_min: WalLsn::new(2),
                    wal_lsn_max: WalLsn::new(2),
                    seal_key: fixture.key.to_string(),
                },
                newer,
            )
            .expect("the newer batch inserts");
        let service = Arc::clone(&fixture.service);
        let plan = resolve_staged_fixture(&fixture, service, &session).await;
        assert_eq!(live_source_partitions(&plan), vec![4, 1]);
        let rows = collect(plan, Arc::new(TaskContext::default()))
            .await
            .expect("the mixed cut executes");
        let mut values = first_int64_values(&rows);
        values.sort_unstable();
        assert_eq!(values, vec![1, 2, 3, 10], "every row is read exactly once");
    }

    /// Every authenticated request-component contradiction fails before resolution.
    ///
    /// # Panics
    /// Panics if the canonical physical-plan fixtures cannot be encoded or decoded.
    pub(crate) async fn assert_complete_preflight_matrix() {
        let (request, binding) = scribe_request().expect("valid Scribe request");
        let calls = Arc::new(AtomicUsize::new(0));
        let follower = PhysicalPlanFollower::new(CountingResolver {
            calls: Arc::clone(&calls),
            schema: Arc::new(Schema::new(vec![Field::new(
                "value",
                DataType::Int64,
                true,
            )])),
        });
        let first = PhysicalPlanNode::decode(request.physical_plan_bytes.as_slice())
            .expect("first extension decodes");
        let second_bytes = physical_plan_to_bytes_with_extension_codec(
            Arc::new(RemoteSourcePlaceholderExec::new(
                "scan-two",
                &request.assignments[0].schema_fingerprint,
                Arc::new(Schema::new(vec![Field::new(
                    "value",
                    DataType::Int64,
                    true,
                )])),
            )),
            &OraclePhysicalExtensionCodec::encoder(),
        )
        .expect("second extension encodes");
        let second =
            PhysicalPlanNode::decode(second_bytes.as_ref()).expect("second extension decodes");
        let mut multiple = request.clone();
        multiple.assignments.push(FollowerScanAssignment {
            scan_id: "scan-two".to_owned(),
            ..multiple.assignments[0].clone()
        });
        multiple.physical_plan_bytes = PhysicalPlanNode {
            physical_plan_type: Some(PhysicalPlanType::Union(
                datafusion_proto::protobuf::UnionExecNode {
                    inputs: vec![first.clone(), second],
                },
            )),
        }
        .encode_to_vec();
        multiple.plan_fingerprint = physical_plan_fingerprint(&multiple.physical_plan_bytes);
        assert!(
            follower
                .preflight(&multiple, &authenticated(&request, &binding))
                .is_ok()
        );
        let malformed = malformed_follower_requests(&request, &first);
        for case in malformed {
            let state = SessionStateBuilder::new().with_default_features().build();
            assert!(
                follower
                    .decode(
                        &case,
                        authenticated(&request, &binding),
                        &state,
                        &TaskContext::default()
                    )
                    .await
                    .is_err()
            );
        }
        assert_eq!(calls.load(Ordering::SeqCst), 0);
        assert_eq!(follower.effect_counts(), (0, 0, 0, 0));
    }
}
