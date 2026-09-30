//! Three-stage authenticated physical-plan follower lifecycle.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use arrow::datatypes::SchemaRef;
use async_trait::async_trait;
use datafusion::common::DataFusionError;
use datafusion::common::tree_node::{Transformed, TreeNode};
use datafusion::datasource::TableProvider;
use datafusion::datasource::memory::MemorySourceConfig;
use datafusion::datasource::physical_plan::{FileGroup, FileScanConfig, FileScanConfigBuilder};
use datafusion::datasource::source::DataSourceExec;
use datafusion::execution::TaskContext;
use datafusion::execution::session_state::SessionState;
use datafusion::physical_plan::stream::RecordBatchStreamAdapter;
use datafusion::physical_plan::streaming::{PartitionStream, StreamingTableExec};
use datafusion::physical_plan::{ExecutionPlan, SendableRecordBatchStream, execute_stream};
use datafusion_proto::bytes::physical_plan_from_bytes_with_extension_codec;
use datafusion_proto::protobuf::{PhysicalPlanNode, physical_plan_node::PhysicalPlanType};
use futures_util::StreamExt as _;
use iceberg_datafusion::physical_plan::IcebergTableScan;
use prost::Message;
use thiserror::Error;
use wyrd_spec::DataTenantId;
use wyrd_spec::vala::api::{
    ClusterRole, ExecuteFragmentRequest, FollowerScanAssignment, OracleRoleFence,
    PersistedFileDescriptor, ReservationId, TenantTableBinding,
};
use wyrd_spec::vala::managed_columns::DATA_TENANT_ID;

use super::codec::{OraclePhysicalExtensionCodec, PreflightExtension, physical_plan_fingerprint};
use crate::catalog::layout::TimePartition;
use crate::catalog::{
    BIFROST_CATALOG_NAME, BifrostCatalog, TableRef, TenantTableBinding as CatalogTableBinding,
};
use crate::contracts::ScribeError;
use crate::oracle::reader_pins::{
    OracleReaderAuthority, ReaderIoPermit, ReaderQueryGuard, local_cut_from_follower,
};
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
/// a Scribe follower degrades only `SourceLoss`, refuses `Capacity` as
/// capacity, and fails `Fault`.
#[derive(Debug, Error)]
pub enum FollowerResolutionError {
    /// Schema, projection, predicate, binding, integrity, or local execution
    /// fault. Every untyped resolver message is this class.
    #[error("{0}")]
    Fault(String),
    /// A bounded local admission ceiling refused the read.
    #[error("resolution refused for capacity: {0}")]
    Capacity(String),
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
/// authenticated schema or names it ambiguously. Neither is repaired: silently
/// deduplicating or reordering a signed assignment would read something other
/// than what the leader signed.
pub(super) fn signed_closure_schema(
    full_schema: &arrow::datatypes::Schema,
    required_columns: &[String],
) -> Result<SchemaRef, String> {
    super::exec::select_schema_by_name(full_schema, required_columns)
        .map(|(schema, _)| schema)
        .map_err(|_| {
            "assignment closure does not resolve against the authenticated schema".to_owned()
        })
}

/// Resolver that refuses every assignment, for fixtures that never scan.
///
/// Analytical decode needs a resolver to exist before a plan arrives, but a
/// transport or session fixture proves ordering rather than data. Refusing is
/// the honest behavior: a fixture that unexpectedly reaches source resolution
/// fails loudly instead of silently reading nothing.
#[cfg(any(test, feature = "test-support"))]
#[derive(Debug, Default)]
pub struct UnresolvableSource;

#[cfg(any(test, feature = "test-support"))]
#[async_trait]
impl FollowerSourceResolver for UnresolvableSource {
    /// Always refuses, naming the fixture rather than a catalog failure.
    async fn resolve(
        &self,
        _target_role: ClusterRole,
        _assignment: &FollowerScanAssignment,
        _session: &SessionState,
        _reader_io_permit: Option<&ReaderIoPermit>,
    ) -> Result<ResolvedFollowerSource, FollowerResolutionError> {
        Err(FollowerResolutionError::Fault(
            "fixture resolver refuses every assignment".to_owned(),
        ))
    }
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
        reader_io_permit: Option<&ReaderIoPermit>,
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
        reader_io_permit: Option<&ReaderIoPermit>,
    ) -> Result<ResolvedFollowerSource, FollowerResolutionError> {
        self.as_ref()
            .resolve(target_role, assignment, session, reader_io_permit)
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
/// Each dispatched persisted file and each assigned Scribe cut is one
/// openable scan target; a fragment with none should never have been
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
    let scannable = assignments.iter().any(|assignment| {
        !assignment.persisted.files.is_empty() || assignment.scribe_provider_cut.is_some()
    });
    if !scannable {
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
    /// Exact pending reservation owned by this receiver for the query.
    pub reservation_id: &'a ReservationId,
    /// Leader incarnation recovered from checked context claims.
    pub leader_fence: OracleRoleFence,
    /// Role incarnation owned by this receiving process.
    pub local_fence: OracleRoleFence,
}

/// Authenticated Oracle resolver backed by the tenant catalog and Iceberg provider.
pub struct OracleCatalogResolver {
    /// Tenant-qualified catalog owner.
    catalog: Arc<BifrostCatalog>,
}

impl std::fmt::Debug for OracleCatalogResolver {
    /// Redacts catalog internals while exposing the non-secret resolver shape.
    ///
    /// # Errors
    /// Returns the formatter error if the destination cannot accept the redacted value.
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("OracleCatalogResolver")
            .field("catalog", &"authenticated")
            .finish()
    }
}

impl OracleCatalogResolver {
    /// Creates an Oracle resolver from already-authenticated process capabilities.
    #[must_use]
    pub fn new(catalog: Arc<BifrostCatalog>) -> Self {
        Self { catalog }
    }

    /// Builds the follower-governed hot leaf for a validated hot assignment.
    ///
    /// Every object identity, size, and assignment fence has been validated by
    /// the caller, so the shared hot leaf is constructed in follower
    /// governance: its reservations are charged to the request-local pool
    /// backed by the retained worker lease. It is built at the closure schema,
    /// so its Parquet column mask decodes only the signed columns, and split
    /// across the session's target partitions by byte range.
    ///
    /// # Errors
    /// Returns a scrubbed message when an assigned descriptor is not a hot
    /// object bound to this assignment.
    fn hot_leaf(
        &self,
        assignment: &FollowerScanAssignment,
        assigned_locations: &[(PersistedFileDescriptor, String, String)],
        required_schema: SchemaRef,
        full_schema: SchemaRef,
        session: &SessionState,
        permit: &ReaderIoPermit,
    ) -> Result<ResolvedFollowerSource, FollowerResolutionError> {
        let files = assigned_locations
            .iter()
            .map(|(descriptor, _, location)| {
                signed_hot_source(descriptor, location, &assignment.binding)
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(ResolvedFollowerSource {
            plan: Arc::new(
                super::exec::HotParquetExec::new(
                    files,
                    self.catalog.gated_file_io(permit),
                    Arc::clone(self.catalog.storage()),
                    required_schema,
                    super::exec::HotParquetPlan::Follower {
                        memory_pool: session.runtime_env().memory_pool.clone(),
                    },
                    Arc::new(super::exec::OracleScanMetricsHandle::default()),
                    assignment.predicates.clone(),
                )
                .with_partitions(session.config().target_partitions()),
            ),
            full_schema,
        })
    }

    /// Resolves every assigned descriptor to its table-relative and absolute
    /// object location.
    ///
    /// Kept separate from `resolve` because it is the one part of that workflow
    /// that only needs the catalog binding: it turns signed paths into
    /// locations this node can open, and refuses any path that does not live
    /// under the authenticated table's own prefix.
    ///
    /// # Errors
    /// Returns a scrubbed message when the binding does not resolve, when a
    /// signed path is not under the table's prefix, or when the catalog cannot
    /// place an object.
    fn assigned_locations(
        &self,
        assignment: &FollowerScanAssignment,
        table: TableRef,
    ) -> Result<Vec<(PersistedFileDescriptor, String, String)>, String> {
        let catalog_binding =
            CatalogTableBinding::resolve((assignment.binding.tenant_id, table))
                .map_err(|_| "authenticated Oracle assignment binding failed".to_owned())?;
        assignment
            .persisted
            .files
            .iter()
            .map(|descriptor| {
                let path = descriptor.path();
                let table_relative = path
                    .strip_prefix(&catalog_binding.object_prefix)
                    .and_then(|suffix| suffix.strip_prefix('/'))
                    .ok_or_else(|| "authenticated Oracle assignment binding failed".to_owned())?
                    .to_owned();
                self.catalog
                    .object_location(&catalog_binding, path)
                    .map(|location| (descriptor.clone(), table_relative, location))
                    .map_err(|_| "authenticated Oracle assignment location failed".to_owned())
            })
            .collect()
    }
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

#[async_trait]
impl FollowerSourceResolver for OracleCatalogResolver {
    /// Builds one tenant-qualified leaf for an Oracle assignment and rejects
    /// Scribe assignments.
    ///
    /// Which leaf depends on the validated variant of the assignment's own
    /// descriptors, never on its scan id. A hot assignment
    /// reads sealed Scribe output directly through [`super::exec::HotParquetExec`],
    /// which takes `assignment.predicates` as its own filter; an Iceberg
    /// assignment reads compacted output through the catalog provider's scan,
    /// which takes the same predicates as logical filters. Both leaves therefore
    /// prune from the closure the leader signed, but only one of them is built
    /// per call — the hot branch returns before the provider scan, so a hot
    /// assignment never plans a catalog scan it would discard.
    ///
    /// Both leaves are also built at the signed closure schema rather than the
    /// full catalog schema, so the physical read is narrowed here rather than
    /// being narrowed by a projection above a leaf that already paid for the
    /// wide columns.
    ///
    /// Cancellation during catalog or scan IO drops all locally acquired state
    /// and exposes no provider; retry restarts the complete resolution.
    ///
    /// # Errors
    /// Returns a redacted resolution error for a role mismatch, invalid table binding,
    /// catalog lookup failure, a closure that does not resolve against the
    /// authenticated schema, hot-file metadata failure, or physical scan
    /// construction failure.
    async fn resolve(
        &self,
        target_role: ClusterRole,
        assignment: &FollowerScanAssignment,
        session: &SessionState,
        reader_io_permit: Option<&ReaderIoPermit>,
    ) -> Result<ResolvedFollowerSource, FollowerResolutionError> {
        // Required, not optional: this resolver is the only one that opens
        // snapshot-dependent objects, so it refuses to build anything without
        // the permit proving its node's epoch already protects that snapshot.
        let permit = reader_io_permit
            .ok_or_else(|| "Oracle resolver has no reader epoch permit".to_owned())?;
        if target_role != ClusterRole::Oracle || assignment.scribe_provider_cut.is_some() {
            return Err(FollowerResolutionError::Fault(
                "Oracle resolver received a non-Oracle assignment".to_owned(),
            ));
        }
        let table = assignment_table(&assignment.binding)?;
        // Classified before any catalog or object I/O. The scan id is a
        // leader-chosen label on the wire; the descriptor variant is what
        // preflight validated and what the assignment-authority digest covers,
        // so it is the only thing this dispatch may read. A list mixing sources
        // or snapshots has no single correct reader and is refused here as well
        // as in preflight, because this resolver is reachable from any
        // authenticated fragment.
        let source = super::AssignedPersistedSource::classify(&assignment.persisted.files)
            .map_err(|_| "authenticated Oracle assignment names mixed sources".to_owned())?;
        // An Iceberg assignment binds to the snapshot it names rather than to
        // whichever one is current here, so a snapshot that has since been
        // replaced fails to resolve instead of serving files and a schema the
        // leader never planned, digested, or signed.
        let provider = match source {
            super::AssignedPersistedSource::Iceberg { snapshot_id } => {
                self.catalog
                    .pinned_provider(&table, assignment.binding.tenant_id, snapshot_id, permit)
                    .await
            }
            super::AssignedPersistedSource::Hot | super::AssignedPersistedSource::Empty => {
                self.catalog
                    .provider(&table, assignment.binding.tenant_id, permit)
                    .await
            }
        }
        .map_err(|_| "authenticated Oracle catalog provider failed".to_owned())?;
        // Validates the full physical schema fingerprint against the
        // authenticated table's actual schema immediately after catalog
        // resolution and before any per-file object I/O (the hot-file branch
        // below issues metadata HEAD requests). `decode` repeats this check
        // once more after every scan id in the request resolves, but that
        // later check alone would let a mismatched hot assignment reach
        // storage first.
        let full_schema = provider.schema();
        let actual = super::assignment_schema_fingerprint(full_schema.as_ref());
        if actual != assignment.schema_fingerprint {
            return Err(FollowerResolutionError::Fault(
                "resolved provider schema fingerprint differs from assignment".to_owned(),
            ));
        }
        // Derived before any object I/O so an assignment naming a column this
        // table does not have is refused rather than partially read.
        let required_schema =
            signed_closure_schema(full_schema.as_ref(), &assignment.required_columns)?;
        if assignment.persisted.files.is_empty() {
            return empty_assignment_leaf(&required_schema, full_schema);
        }
        let assigned_locations = self.assigned_locations(assignment, table)?;
        if source == super::AssignedPersistedSource::Hot {
            return self.hot_leaf(
                assignment,
                &assigned_locations,
                required_schema,
                full_schema,
                session,
                permit,
            );
        }
        // Compacted assignments keep the catalog's own scan. Rebuild the leaf
        // from the closure the leader signed: the follower resolves its own
        // provider, so `assignment.predicates` is the only description of what
        // this cut may skip, and handing it back as logical filters is what
        // lets the Iceberg source prune files and row groups. The signed names
        // are translated to this provider's own indices and pushed as the scan
        // projection, so unrequested columns are never read; the provider is
        // free to return them in its own order, which the name-based
        // normalization below corrects without touching the signed closure.
        let pushdown =
            super::exec::scan_predicate_logical_exprs(&assignment.predicates, &full_schema);
        let (_, indices) =
            super::exec::select_schema_by_name(full_schema.as_ref(), &assignment.required_columns)
                .map_err(|_| {
                    "assignment closure does not resolve against the authenticated schema"
                        .to_owned()
                })?;
        let plan = provider
            .scan(session, Some(&indices), &pushdown, None)
            .await
            .map_err(|_| "authenticated Oracle physical scan failed".to_owned())?;
        // The descriptor is retained on each assigned location for identity-bearing
        // reads; plan restriction itself only needs the canonical path, the
        // table-relative alias, and the resolved object location.
        let restriction = assigned_locations
            .iter()
            .map(|(descriptor, table_relative, location)| {
                (
                    descriptor.path().to_owned(),
                    table_relative.clone(),
                    location.clone(),
                )
            })
            .collect::<Vec<_>>();
        let plan = restrict_plan_to_assigned_files(
            plan,
            &restriction,
            session.config().target_partitions(),
        )?;
        // Footers load through the node's one governed cache, charged to the
        // request-local pool the retained worker lease backs, like hot ranges.
        let plan = super::exec::route_published_footers(
            plan,
            &super::exec::PublishedFooters::new(
                Arc::clone(self.catalog.storage()),
                assignment.binding.tenant_id,
                assignment.binding.table.clone(),
                super::exec::HotParquetPlan::Follower {
                    memory_pool: session.runtime_env().memory_pool.clone(),
                },
            ),
        )
        .map_err(|_| "authenticated Oracle footer routing failed".to_owned())?;
        let plan = super::exec::project_plan_by_name(plan, &assignment.required_columns)
            .map_err(|_| "authenticated Oracle closure normalization failed".to_owned())?;
        Ok(ResolvedFollowerSource { plan, full_schema })
    }
}

/// Builds one hot leaf source from the signed descriptor and its resolved
/// location, without touching the object.
///
/// The descriptor's `size_bytes` is the leader's own durable statement of the
/// object's exact length, taken from the `vala.file_list` row it signed and
/// covered by the assignment-authority digest this follower already verified.
/// Re-reading the length from storage would replace that verified fact with an
/// unauthenticated one — an object whose length disagreed would be read at
/// whatever length storage reported rather than refused — and would spend one
/// round trip per assigned file before the scan begins.
///
/// The event-time interval is deliberately dropped: the leader already applied
/// the query interval to this list and signed the result, so every assigned
/// file is retained by construction and a second local decision here could only
/// disagree with a signed one.
///
/// # Errors
/// Returns a redacted resolution error when the signed length does not fit this
/// platform's `usize`, which is a malformed assignment rather than a large file.
fn signed_hot_source(
    descriptor: &PersistedFileDescriptor,
    location: &str,
    binding: &wyrd_spec::vala::api::TenantTableBinding,
) -> Result<super::exec::HotFileSource, String> {
    let PersistedFileDescriptor::Hot(hot) = descriptor else {
        return Err("authenticated Oracle hot provider failed".to_owned());
    };
    let size_bytes = usize::try_from(descriptor.size_bytes())
        .map_err(|_| "authenticated Oracle hot provider failed".to_owned())?;
    Ok(super::exec::HotFileSource {
        // Keyed on the leader's signed `vala.file_list` identity and decoded
        // checksum, the same identity the leader itself keys on, so a follower
        // reading the same durable object shares the node's one decode.
        metadata_key: crate::storage::ObjectMetadataKey::new(
            binding.tenant_id,
            binding.table.clone(),
            hot.path.clone(),
            hot.file_list_id,
            hot.sha256,
            hot.size_bytes,
        ),
        location: location.to_owned(),
        size_bytes,
        event_time: crate::catalog::event_time::EventTimeStatistics::Unusable(
            crate::catalog::event_time::EventTimeBoundsDefect::Missing,
        ),
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
    /// another stream incarnation, [`FollowerResolutionError::Capacity`] when
    /// the bounded snapshot exceeds its count or byte ceiling, and
    /// [`FollowerResolutionError::Fault`] for every other open failure.
    async fn open(
        &self,
        request: FetchLiveTailRequest,
    ) -> Result<LiveTailBatches, FollowerResolutionError> {
        self.open_live_batches(request)
            .await
            .map_err(|error| live_open_error(&error))
    }
}

/// Classes one Scribe live-read open failure for the fragment's dispatch.
///
/// A request for another stream incarnation means the assigned source is gone;
/// an exhausted count or byte ceiling is a capacity refusal; every other open
/// failure is a fault. The Scribe detail is redacted.
fn live_open_error(error: &ScribeError) -> FollowerResolutionError {
    match error {
        ScribeError::StreamMismatch { .. } => {
            FollowerResolutionError::SourceLoss("Scribe stream incarnation changed".to_owned())
        }
        ScribeError::IngestBusy { .. } => FollowerResolutionError::Capacity(
            "Scribe live-tail snapshot exceeds its bound".to_owned(),
        ),
        _ => FollowerResolutionError::Fault("Scribe live-tail snapshot failed".to_owned()),
    }
}

/// Role-local resolver that snapshots the authenticated Scribe stream exactly once.
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
    /// Creates one IO-free resolver for focused role-local provider tests.
    fn with_schema(tail: Arc<T>, schema: arrow::datatypes::SchemaRef) -> Self {
        Self {
            tail,
            schema_source: ScribeSchemaSource::Fixed(schema),
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

/// One Analytical leaf's local reader-epoch protection.
///
/// The guard keeps this node's epoch protecting the signed snapshots, and the
/// permit is what every object open in the resolved leaf presents. Dropping
/// the guard cancels the permit, so the protection cannot outlive or
/// under-live the reads it exists for.
pub struct FollowerReaderProtection {
    /// This node's durable claim on every snapshot the fragment reads.
    guard: ReaderQueryGuard,
    /// Permission the resolved plan's source IO presents.
    permit: ReaderIoPermit,
}

impl std::fmt::Debug for FollowerReaderProtection {
    /// Prints the owner without any tenant, table, or snapshot payload.
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("FollowerReaderProtection")
            .finish_non_exhaustive()
    }
}

impl FollowerReaderProtection {
    /// Splits this protection into the durable guard and the clonable permit.
    ///
    /// Used only where the two halves need different owners: the Analytical
    /// follower hands the guard to the graph that must outlive the plan, and
    /// keeps the permit with the plan that presents it on every read. The
    /// guard remains strictly stronger — dropping it cancels every permit it
    /// issued — so the split cannot widen what the permit authorizes.
    #[must_use]
    pub(super) fn into_parts(self) -> (ReaderQueryGuard, ReaderIoPermit) {
        (self.guard, self.permit)
    }
}

/// Protects every snapshot a set of signed assignments names, under this epoch.
///
/// The Analytical leaf calls it before opening a provider. The leader signed
/// exactly which snapshot each scan reads, so this re-derives the same cuts and
/// commits them under `authority`. An installed epoch also supplies the IO
/// permit for hot-only or empty assignments, without adding snapshot
/// protection; without an epoch and without snapshots it yields `None`.
///
/// # Errors
///
/// Returns a redacted description when an assignment names a snapshot while the
/// node has no reader epoch, when a signed cut targets a different epoch fence
/// than the node holds, when its binding is not canonical, or when the local
/// authority refuses the protection.
pub(super) async fn protect_reader_cuts<'a, I>(
    authority: Option<&Arc<OracleReaderAuthority>>,
    assignments: I,
) -> Result<Option<FollowerReaderProtection>, String>
where
    I: IntoIterator<Item = &'a FollowerScanAssignment>,
{
    let mut cuts = Vec::new();
    for assignment in assignments {
        if !assignment.reader_cut.protects_a_snapshot() {
            continue;
        }
        let Some(authority) = authority else {
            return Err(
                "follower has no reader epoch for a snapshot-bearing assignment".to_owned(),
            );
        };
        let local_fence = u64::try_from(authority.fencing_token())
            .map_err(|_| "local reader epoch fence is not representable".to_owned())?;
        if assignment.reader_cut.target_epoch_fence != local_fence {
            return Err("assignment reader cut targets a different Oracle epoch fence".to_owned());
        }
        let table = assignment_table(&assignment.binding)?;
        cuts.push((
            vala_sql::row_types::oracle_reader_authority::TableAuthorityIdentity {
                tenant: assignment.binding.tenant_id,
                table_uid: *assignment.reader_cut.table_uid.as_bytes(),
                catalog_name: BIFROST_CATALOG_NAME.to_owned(),
                namespace_name: table.namespace.as_str().to_owned(),
                table_name: table.name.clone(),
            },
            assignment.reader_cut.clone(),
        ));
    }
    if cuts.is_empty() && authority.is_none() {
        return Ok(None);
    }
    // Verified before any widening so a tampered ancestry is refused here
    // rather than turning into a durable protection claim.
    for (identity, cut) in &cuts {
        local_cut_from_follower(identity, cut).map_err(|error| error.to_string())?;
    }
    let Some(authority) = authority else {
        return Err("follower has no reader epoch for a snapshot-bearing assignment".to_owned());
    };
    let (guard, permit) = authority
        .acquire_follower_guard(&cuts)
        .await
        .map_err(|error| error.to_string())?;
    Ok(Some(FollowerReaderProtection { guard, permit }))
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
            row_groups_scanned: self.0.row_groups_scanned,
            row_groups_pruned: self.0.row_groups_pruned,
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
    /// Arrow provider construction failure.
    async fn resolve(
        &self,
        target_role: ClusterRole,
        assignment: &FollowerScanAssignment,
        _session: &SessionState,
        _reader_io_permit: Option<&ReaderIoPermit>,
    ) -> Result<ResolvedFollowerSource, FollowerResolutionError> {
        if target_role != ClusterRole::Scribe || !assignment.persisted.files.is_empty() {
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
        let required_schema =
            signed_closure_schema(full_schema.as_ref(), &assignment.required_columns)?;
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
        let max_batches = usize::try_from(cut.maximum_batch_count)
            .map_err(|_| "Scribe batch bound does not fit this process".to_owned())?;
        let max_retained_bytes = usize::try_from(cut.maximum_retained_bytes)
            .map_err(|_| "Scribe byte bound does not fit this process".to_owned())?;
        let batches = self
            .tail
            .open(FetchLiveTailRequest {
                binding,
                target_stream: stream,
                start_partition,
                end_partition,
                // The signed closure, byte for byte. Scribe projects the
                // memtable and staged runs to exactly these names in exactly
                // this order, so the wide columns are never materialized, and
                // the declared leaf schema below is the same closure — the
                // serialized plan's `Column` indices are closure indices.
                required_columns: assignment.required_columns.clone(),
                predicates: assignment.predicates.clone(),
                max_batches,
                max_retained_bytes,
            })
            .await?;
        let partition = Arc::new(LiveTailPartition {
            schema: Arc::clone(&required_schema),
            batches: std::sync::Mutex::new(Some(batches)),
        });
        StreamingTableExec::try_new(
            Arc::clone(&required_schema),
            vec![partition as Arc<dyn PartitionStream>],
            None,
            Vec::new(),
            false,
            None,
        )
        .map(|plan| ResolvedFollowerSource {
            plan: Arc::new(plan) as Arc<dyn ExecutionPlan>,
            full_schema,
        })
        .map_err(|_| {
            FollowerResolutionError::Fault("Scribe Arrow provider construction failed".to_owned())
        })
    }
}

/// One-shot partition over an opened Scribe live read.
///
/// The fragment executes its single leaf partition once, so the producer is
/// taken on first execution and each batch is projected to the signed closure
/// as it is pulled. Dropping the returned stream drops the producer and with
/// it the memtable references and staged lease.
#[derive(Debug)]
struct LiveTailPartition {
    /// Signed closure schema every produced batch is projected to.
    schema: SchemaRef,
    /// Opened producer, present until the partition is executed.
    batches: std::sync::Mutex<Option<LiveTailBatches>>,
}

impl PartitionStream for LiveTailPartition {
    /// Returns the signed closure schema.
    fn schema(&self) -> &SchemaRef {
        &self.schema
    }

    /// Takes the producer and streams its batches projected to the closure.
    ///
    /// A second execution yields one internal error instead of rows, because
    /// the live cut has already been handed out. Staged reads are charged to
    /// the task's memory pool, which is the follower's granted pool.
    fn execute(&self, ctx: Arc<TaskContext>) -> SendableRecordBatchStream {
        let schema = Arc::clone(&self.schema);
        let taken = self
            .batches
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take();
        let stream = match taken {
            Some(batches) => batches
                .into_stream(Arc::clone(ctx.memory_pool()))
                .map(move |batch| {
                    batch
                        .map_err(|error| DataFusionError::External(Box::new(error)))
                        .and_then(|batch| super::exec::project_batch(&batch, Arc::clone(&schema)))
                })
                .boxed(),
            None => futures_util::stream::once(async {
                Err(DataFusionError::Internal(
                    "Scribe live read was already executed".to_owned(),
                ))
            })
            .boxed(),
        };
        Box::pin(RecordBatchStreamAdapter::new(
            Arc::clone(&self.schema),
            stream,
        ))
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

/// Restricts every file-backed leaf to the exact authenticated assignment.
///
/// The Iceberg provider remains responsible for schema, pruning, and tenant
/// filtering. This final projection removes every unassigned manifest file and
/// rejects assignments that do not correspond to a planned file. Each Iceberg
/// leaf reads across `partitions`, the follower session's target partitions.
///
/// # Errors
/// Returns an error when the provider is not file-backed, an assigned location
/// is absent, or a non-file leaf prevents proving exact file-set equality.
fn restrict_plan_to_assigned_files(
    plan: Arc<dyn ExecutionPlan>,
    assigned_files: &[(String, String, String)],
    partitions: usize,
) -> Result<Arc<dyn ExecutionPlan>, String> {
    let assigned = assigned_files
        .iter()
        .map(|(_, _, location)| location.clone())
        .collect::<std::collections::BTreeSet<_>>();
    let relative_assignments = assigned_files
        .iter()
        .flat_map(|(canonical, table_relative, location)| {
            [
                (canonical.clone(), location.clone()),
                (table_relative.clone(), location.clone()),
            ]
        })
        .collect::<std::collections::BTreeMap<_, _>>();
    if assigned.len() != assigned_files.len() || assigned.iter().any(|file| file.trim().is_empty())
    {
        return Err("authenticated Oracle assignment contains duplicate or empty files".to_owned());
    }
    let mut observed = std::collections::BTreeSet::new();
    let mut file_leaves = 0usize;
    let transformed = plan
        .transform_up(|node| {
            if let Some(exec) = node.downcast_ref::<super::exec::OracleIcebergScanExec>() {
                file_leaves = file_leaves.saturating_add(1);
                observed.extend(assigned.iter().cloned());
                return Ok(Transformed::yes(Arc::new(
                    exec.clone()
                        .with_assigned_files(assigned.clone(), relative_assignments.clone())
                        .with_partitions(partitions),
                )));
            }
            if node.is::<IcebergTableScan>() {
                let restricted = super::exec::OracleIcebergScanExec::from_plan(node.as_ref())
                    .map_err(|_| {
                        datafusion::common::DataFusionError::Plan(
                            "authenticated Oracle Iceberg source failed".to_owned(),
                        )
                    })?
                    .with_assigned_files(assigned.clone(), relative_assignments.clone())
                    .with_partitions(partitions);
                file_leaves = file_leaves.saturating_add(1);
                observed.extend(assigned.iter().cloned());
                return Ok(Transformed::yes(Arc::new(restricted)));
            }
            let Some(exec) = node.downcast_ref::<DataSourceExec>() else {
                return Ok(Transformed::no(node));
            };
            let Some(config) = exec.data_source().downcast_ref::<FileScanConfig>() else {
                return Err(datafusion::common::DataFusionError::Plan(
                    "Oracle assignment encountered a non-file data source".to_owned(),
                ));
            };
            file_leaves = file_leaves.saturating_add(1);
            let groups = config
                .file_groups
                .iter()
                .filter_map(|group| {
                    let files = group
                        .iter()
                        .filter(|file| {
                            let object_path = file.object_meta.location.to_string();
                            let location = relative_assignments
                                .get(&object_path)
                                .cloned()
                                .unwrap_or_else(|| {
                                    physical_file_location(
                                        config.object_store_url.as_str(),
                                        &object_path,
                                    )
                                });
                            if assigned.contains(&location) {
                                observed.insert(location);
                                true
                            } else {
                                false
                            }
                        })
                        .cloned()
                        .collect::<Vec<_>>();
                    (!files.is_empty()).then(|| FileGroup::new(files))
                })
                .collect();
            let restricted = FileScanConfigBuilder::from(config.clone())
                .with_file_groups(groups)
                .build();
            Ok(Transformed::yes(Arc::new(
                exec.clone().with_data_source(Arc::new(restricted)),
            )))
        })
        .map_err(|_| "authenticated Oracle file projection failed".to_owned())?
        .data;
    if file_leaves == 0 || observed != assigned {
        // Counts only: which files a tenant owns is not safe to name in an
        // error that crosses the dispatch boundary, but the shape of the
        // mismatch is what a maintainer needs to tell "no file leaf in this
        // plan" apart from "the leaf lost files the leader signed for".
        return Err(format!(
            "authenticated Oracle assignment differs from planned files: \
             file_leaves={file_leaves} observed={} assigned={}",
            observed.len(),
            assigned.len()
        ));
    }
    Ok(transformed)
}

/// Joins `DataFusion`'s authenticated object-store authority and relative object key.
fn physical_file_location(store: &str, path: &str) -> String {
    if store == "file://" {
        return format!("file:///{}", path.trim_start_matches('/'));
    }
    if store.ends_with('/') || path.starts_with('/') {
        format!("{store}{path}")
    } else {
        format!("{store}/{path}")
    }
}

/// Concrete owner of preflight, provider resolution, and `DataFusion` decode.
pub struct PhysicalPlanFollower<R> {
    /// Authenticated role-local provider constructor.
    resolver: R,
    /// Exact process-owned audit capability reconstructed into tenant tripwires.
    audit: Option<Arc<dyn super::OracleAudit>>,
    /// Maximum accepted protobuf size.
    maximum_plan_bytes: usize,
    /// Observable lifecycle-boundary effects used to prove fail-closed ordering.
    effects: FollowerEffects,
}

impl<R> std::fmt::Debug for PhysicalPlanFollower<R>
where
    R: std::fmt::Debug,
{
    /// Redacts the injected audit capability while retaining follower policy.
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("PhysicalPlanFollower")
            .field("resolver", &self.resolver)
            .field("audit", &self.audit.is_some())
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
            audit: None,
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

    /// Installs the process-owned audit capability required by tenant tripwires.
    #[must_use]
    pub fn with_audit(mut self, audit: Arc<dyn super::OracleAudit>) -> Self {
        self.audit = Some(audit);
        self
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
    /// Returns [`PhysicalPlanFollowerError::Preflight`] when preflight fails or
    /// any assignment names a snapshot, which this epoch-less follower cannot
    /// protect, and other [`PhysicalPlanFollowerError`] variants for provider
    /// resolution or post-resolution native `DataFusion` decode failure.
    pub async fn decode(
        &self,
        request: &ExecuteFragmentRequest,
        authenticated: AuthenticatedFollowerContext<'_>,
        session: &SessionState,
        context: &TaskContext,
    ) -> Result<Arc<dyn ExecutionPlan>, PhysicalPlanFollowerError> {
        let preflight = self.preflight(request, &authenticated)?;
        // This follower holds no reader epoch, so a signed snapshot is refused
        // before any provider, manifest, or object open could read it.
        if preflight
            .assignments
            .values()
            .any(|assignment| assignment.reader_cut.protects_a_snapshot())
        {
            return Err(PhysicalPlanFollowerError::Preflight(
                "follower has no reader epoch for a snapshot-bearing assignment".to_owned(),
            ));
        }
        let mut providers = HashMap::with_capacity(preflight.assignments.len());
        for (scan_id, assignment) in preflight.assignments {
            self.effects.resolver.fetch_add(1, Ordering::SeqCst);
            let resolved = self
                .resolver
                .resolve(request.target_fence.role, &assignment, session, None)
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
            let expected =
                signed_closure_schema(resolved.full_schema.as_ref(), &assignment.required_columns)
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
        let codec = if let Some(audit) = &self.audit {
            OraclePhysicalExtensionCodec::decoder_with_audit(providers, Arc::clone(audit))
        } else {
            OraclePhysicalExtensionCodec::decoder(providers)
        };
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
        let scan_stats = super::exec::OracleQueryScanStats::from_plan(plan.as_ref(), 0);
        let stream = execute_stream(plan, context)
            .map_err(|error| PhysicalPlanFollowerError::Execution(error.to_string()))?;
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
    /// its identity: the persisted file list must be unique, ordered, and name
    /// exactly one persisted source, the
    /// signed projection closure must retain the hidden tenant column, every
    /// closed predicate must reference a column inside that closure, the role
    /// must match the presence or absence of a Scribe provider cut, and a cut
    /// must be internally valid, pinned to `target_fence`, and copy the
    /// top-level closure byte-for-byte.
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
        if !Self::valid_persisted_files(&assignment.persisted.files) {
            return Err(PhysicalPlanFollowerError::Preflight(
                "persisted assignment is not unique and ordered".to_owned(),
            ));
        }
        // Refused here, before any provider resolution or object I/O, because
        // hot and Iceberg descriptors are resolved by different readers under
        // different authority: a mixed list has no single correct leaf, and
        // whichever the resolver picked would silently drop the other source's
        // rows from an otherwise valid, signed result.
        super::AssignedPersistedSource::classify(&assignment.persisted.files)
            .map_err(|error| PhysicalPlanFollowerError::Preflight(error.to_string()))?;
        // Defense in depth: the leader already refuses to sign an
        // assignment whose projection closure drops the hidden tenant
        // column (see `Oracle::ensure_required_columns_closure`), and the
        // assignment-authority digest recomputed above proves this
        // follower's copy matches what was signed byte-for-byte. Still,
        // this follower independently re-derives the same invariant from
        // the authenticated assignment rather than trusting the digest
        // match alone to imply a safe projection was ever validated.
        if assignment.required_columns.is_empty()
            || !assignment
                .required_columns
                .iter()
                .any(|column| column == DATA_TENANT_ID)
        {
            return Err(PhysicalPlanFollowerError::Preflight(
                "assignment projection closure omits the hidden tenant column".to_owned(),
            ));
        }
        // Every closed predicate's column must already be part of the
        // signed projection closure (`required_columns` is defined as
        // `scan_output_names + predicate_names + [data_tenant_id]`), so a
        // predicate referencing a column outside that closure indicates a
        // malformed assignment rather than a merely unusual one.
        if assignment.predicates.iter().any(|predicate| {
            !assignment
                .required_columns
                .iter()
                .any(|column| column == predicate.column())
        }) {
            return Err(PhysicalPlanFollowerError::Preflight(
                "assignment predicate references a column outside the projection closure"
                    .to_owned(),
            ));
        }
        match target_role {
            ClusterRole::Oracle if assignment.scribe_provider_cut.is_some() => {
                return Err(PhysicalPlanFollowerError::Preflight(
                    "Oracle assignment contains a Scribe provider cut".to_owned(),
                ));
            }
            ClusterRole::Scribe
                if assignment.scribe_provider_cut.is_none()
                    || !assignment.persisted.files.is_empty() =>
            {
                return Err(PhysicalPlanFollowerError::Preflight(
                    "Scribe assignment lacks an explicit hot-provider cut".to_owned(),
                ));
            }
            ClusterRole::Oracle | ClusterRole::Scribe => {}
        }
        if let Some(cut) = &assignment.scribe_provider_cut
            && (cut.writer_epoch != target_fence
                || cut.maximum_batch_count == 0
                || cut.maximum_retained_bytes == 0
                || !cut.is_valid())
        {
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
        Self::collect_extensions(&root, authenticated.tenant_id, &mut encoded)?;
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

    /// Requires canonical non-empty object locations with no duplicates.
    fn valid_persisted_files(files: &[PersistedFileDescriptor]) -> bool {
        files
            .iter()
            .all(|file| file.is_valid() && !file.path().trim().is_empty())
            && files.windows(2).all(|pair| pair[0].path() < pair[1].path())
    }

    /// Walks every child of follower-permitted native operators and leaf extensions.
    ///
    /// # Errors
    /// Returns a preflight error for missing inputs, unsupported operators,
    /// malformed extensions, or duplicate scan identities.
    fn collect_extensions(
        node: &PhysicalPlanNode,
        tenant_id: DataTenantId,
        encoded: &mut HashMap<String, String>,
    ) -> Result<(), PhysicalPlanFollowerError> {
        let children: Vec<&PhysicalPlanNode> = match node.physical_plan_type.as_ref() {
            Some(PhysicalPlanType::Extension(extension)) => {
                match OraclePhysicalExtensionCodec::preflight_extension(&extension.node)
                    .map_err(|error| PhysicalPlanFollowerError::Preflight(error.to_string()))?
                {
                    PreflightExtension::RemoteScan(payload) => {
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
                    PreflightExtension::TenantTripwire { context, table } => {
                        if extension.inputs.len() != 1
                            || context.data_tenant_id != tenant_id
                            || table.trim().is_empty()
                        {
                            return Err(PhysicalPlanFollowerError::Preflight(
                                "tenant tripwire differs from authenticated binding".to_owned(),
                            ));
                        }
                        extension.inputs.iter().collect()
                    }
                }
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
            Self::collect_extensions(child, tenant_id, encoded)?;
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
            _reader_io_permit: Option<&ReaderIoPermit>,
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
    use arrow::array::StringArray;
    use arrow::datatypes::{DataType, Field, Schema};
    use datafusion::datasource::listing::PartitionedFile;
    use datafusion::datasource::physical_plan::ParquetSource;
    use datafusion::execution::context::SessionContext;
    use datafusion::execution::object_store::ObjectStoreUrl;
    use datafusion::execution::session_state::SessionStateBuilder;
    use datafusion::physical_plan::collect;
    use datafusion_proto::bytes::physical_plan_to_bytes_with_extension_codec;
    use wyrd_spec::vala::api::{PeerContext, PersistedFileAssignment, ScribeProviderCut};

    use super::*;
    use crate::oracle::codec::RemoteSourcePlaceholderExec;

    /// Builds a file-backed physical source with the exact supplied catalog paths.
    fn file_plan(schema: SchemaRef, files: &[&str]) -> Arc<dyn ExecutionPlan> {
        let source = Arc::new(ParquetSource::new(schema));
        let groups = vec![FileGroup::new(
            files
                .iter()
                .map(|file| PartitionedFile::new((*file).to_owned(), 1))
                .collect(),
        )];
        let config = FileScanConfigBuilder::new(ObjectStoreUrl::local_filesystem(), source)
            .with_file_groups(groups)
            .build();
        DataSourceExec::from_data_source(config)
    }

    /// Returns every file location still reachable from a restricted source.
    ///
    /// # Panics
    /// Panics if the supplied plan is not the file-backed source constructed by
    /// [`file_plan`] or if that source does not contain a [`FileScanConfig`].
    fn plan_files(plan: &Arc<dyn ExecutionPlan>) -> Vec<String> {
        let exec = plan.downcast_ref::<DataSourceExec>().expect("file source");
        let config = exec
            .data_source()
            .downcast_ref::<FileScanConfig>()
            .expect("file scan config");
        config
            .file_groups
            .iter()
            .flat_map(datafusion::datasource::physical_plan::FileGroup::iter)
            .map(|file| file.object_meta.location.to_string())
            .collect()
    }

    /// Exact Oracle projection excludes unassigned catalog files and fails closed.
    ///
    /// # Panics
    /// Panics if the valid fixture cannot be projected or its file-backed plan
    /// violates the test construction invariant.
    #[test]
    fn oracle_projection_is_exactly_the_authenticated_assignment() {
        let schema = Arc::new(Schema::new(vec![Field::new(
            "value",
            DataType::Int64,
            true,
        )]));
        let catalog = file_plan(
            Arc::clone(&schema),
            &["assigned.parquet", "unassigned.parquet"],
        );
        let assigned = physical_file_location(
            ObjectStoreUrl::local_filesystem().as_str(),
            "assigned.parquet",
        );
        let restricted = restrict_plan_to_assigned_files(
            catalog,
            &[(
                ("assigned.parquet").to_owned(),
                "assigned.parquet".to_owned(),
                assigned.clone(),
            )],
            1,
        )
        .expect("exact assigned file remains executable");
        assert_eq!(plan_files(&restricted), vec!["assigned.parquet"]);

        assert!(
            restrict_plan_to_assigned_files(
                file_plan(Arc::clone(&schema), &["assigned.parquet"]),
                &[(
                    "missing.parquet".to_owned(),
                    "missing.parquet".to_owned(),
                    physical_file_location(
                        ObjectStoreUrl::local_filesystem().as_str(),
                        "missing.parquet",
                    ),
                )],
                1
            )
            .is_err()
        );
        assert!(
            restrict_plan_to_assigned_files(
                file_plan(Arc::clone(&schema), &["assigned.parquet"]),
                &[
                    (
                        "assigned.parquet".to_owned(),
                        "assigned.parquet".to_owned(),
                        assigned.clone(),
                    ),
                    (
                        "extra.parquet".to_owned(),
                        "extra.parquet".to_owned(),
                        physical_file_location(
                            ObjectStoreUrl::local_filesystem().as_str(),
                            "extra.parquet",
                        ),
                    ),
                ],
                1
            )
            .is_err()
        );
        assert!(
            restrict_plan_to_assigned_files(
                file_plan(schema, &["assigned.parquet"]),
                &[
                    (
                        "assigned.parquet".to_owned(),
                        "assigned.parquet".to_owned(),
                        assigned.clone(),
                    ),
                    (
                        "assigned.parquet".to_owned(),
                        "assigned.parquet".to_owned(),
                        assigned,
                    ),
                ],
                1
            )
            .is_err()
        );
    }

    /// Builds one otherwise-valid Scribe provider cut for focused tests.
    fn cut() -> ScribeProviderCut {
        ScribeProviderCut {
            writer_epoch: 2,
            start_partition: crate::test_support::day_partition(2026, 8, 19).to_wire(),
            end_partition: crate::test_support::day_partition(2026, 8, 19).to_wire(),
            maximum_batch_count: 8,
            maximum_retained_bytes: 1024,
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
            let predicates = request.predicates.clone();
            self.requests
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .push(request);
            Ok(LiveTailBatches::from_snapshot(
                self.batches.clone(),
                predicates,
            ))
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
            _reader_io_permit: Option<&ReaderIoPermit>,
        ) -> Result<ResolvedFollowerSource, FollowerResolutionError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            let required_schema =
                signed_closure_schema(self.schema.as_ref(), &assignment.required_columns)?;
            Ok(ResolvedFollowerSource {
                plan: Arc::new(datafusion::physical_plan::empty::EmptyExec::new(
                    required_schema,
                )),
                full_schema: Arc::clone(&self.schema),
            })
        }
    }

    /// Creates one valid Oracle-role request and its exact authenticated facts.
    ///
    /// # Errors
    /// Returns a plan error if the extension leaf cannot be serialized.
    fn oracle_request() -> datafusion::common::Result<(ExecuteFragmentRequest, TenantTableBinding)>
    {
        let tenant_id = DataTenantId::new_v7();
        let binding = TenantTableBinding {
            tenant_id,
            namespace: "vala.logs".to_owned(),
            table: "records".to_owned(),
        };
        // The closure a leader would sign for `SELECT value FROM ...`: the
        // requested column plus the hidden tenant column the tripwire needs.
        let schema = Arc::new(Schema::new(vec![
            Field::new("value", DataType::Int64, true),
            Field::new(DATA_TENANT_ID, DataType::Utf8, false),
        ]));
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
            role: ClusterRole::Oracle,
            fencing_token: 12,
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
                    persisted: PersistedFileAssignment {
                        files: vec![crate::oracle::test_persisted_descriptor(
                            "s3://bucket/file.parquet",
                        )],
                    },
                    scribe_provider_cut: None,
                    schema_fingerprint: fingerprint,
                    required_columns: vec!["value".to_owned(), DATA_TENANT_ID.to_owned()],
                    predicates: Vec::new(),
                    reader_cut: wyrd_spec::vala::api::FollowerReaderCut::no_snapshot(
                        uuid::Uuid::nil(),
                        1,
                    ),
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
        let schema = Arc::new(Schema::new(vec![
            Field::new("wyrd_event_time", DataType::Utf8, true),
            Field::new(DATA_TENANT_ID, DataType::Utf8, false),
        ]));
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
            persisted: PersistedFileAssignment { files: Vec::new() },
            scribe_provider_cut: Some(cut()),
            schema_fingerprint: fingerprint.clone(),
            required_columns: vec!["wyrd_event_time".to_owned(), DATA_TENANT_ID.to_owned()],
            predicates: Vec::new(),
            reader_cut: wyrd_spec::vala::api::FollowerReaderCut::no_snapshot(uuid::Uuid::nil(), 1),
        };
        let session = SessionContext::new().state();
        resolver
            .resolve(
                ClusterRole::Scribe,
                &assignment(local_scribe_scan_id(&binding, stream)),
                &session,
                None,
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
            .resolve(ClusterRole::Scribe, &assignment(sibling), &session, None)
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
    /// tail, and live-read open failures keep their incarnation, capacity, or
    /// fault class instead of collapsing into one message.
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
        let schema = Arc::new(Schema::new(vec![
            Field::new("wyrd_event_time", DataType::Utf8, true),
            Field::new(DATA_TENANT_ID, DataType::Utf8, false),
        ]));
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
            persisted: PersistedFileAssignment { files: Vec::new() },
            scribe_provider_cut: Some(cut()),
            schema_fingerprint: super::super::assignment_schema_fingerprint(schema.as_ref()),
            required_columns: vec!["wyrd_event_time".to_owned(), DATA_TENANT_ID.to_owned()],
            predicates: Vec::new(),
            reader_cut: wyrd_spec::vala::api::FollowerReaderCut::no_snapshot(uuid::Uuid::nil(), 1),
        };
        let session = SessionContext::new().state();

        let mut other_epoch = assignment.clone();
        if let Some(cut) = other_epoch.scribe_provider_cut.as_mut() {
            cut.writer_epoch += 1;
        }
        let mut other_schema = assignment.clone();
        other_schema.schema_fingerprint = super::super::assignment_schema_fingerprint(
            &Schema::new(vec![Field::new(DATA_TENANT_ID, DataType::Utf8, false)]),
        );
        for (refused, expected) in [(other_epoch, "source loss"), (other_schema, "fault")] {
            let class = match resolver
                .resolve(ClusterRole::Scribe, &refused, &session, None)
                .await
            {
                Err(FollowerResolutionError::SourceLoss(_)) => "source loss",
                Err(FollowerResolutionError::Fault(_)) => "fault",
                Err(FollowerResolutionError::Capacity(_)) => "capacity",
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
            live_open_error(&ScribeError::IngestBusy {
                table: "records".to_owned(),
            }),
            FollowerResolutionError::Capacity(_)
        ));
        assert!(matches!(
            live_open_error(&ScribeError::Internal {
                detail: "staged schema".to_owned(),
            }),
            FollowerResolutionError::Fault(_)
        ));
    }

    /// Oracle and Scribe assignments remain a closed role-local matrix.
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
        oracle_projection_is_exactly_the_authenticated_assignment();
        let (request, binding) = oracle_request().expect("valid Oracle request");
        let calls = Arc::new(AtomicUsize::new(0));
        let schema = Arc::new(Schema::new(vec![
            Field::new("value", DataType::Int64, true),
            Field::new(DATA_TENANT_ID, DataType::Utf8, false),
        ]));
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
        assert_eq!(stream.split().0.schema().fields().len(), 2);
    }

    /// Follower sessions are shaped only by the admitted grant and assigned work.
    ///
    /// This is the follower half of the resource-parity contract: the session's
    /// A hot leaf source takes its exact length from the signed descriptor, not
    /// from a fresh read of the object.
    ///
    /// The leader signed `size_bytes` from the `vala.file_list` row and the
    /// assignment-authority digest covers it. Re-reading the length from
    /// storage would replace that verified fact with an unauthenticated one and
    /// spend a round trip per assigned file before the scan begins, so the
    /// fixture's descriptor length deliberately differs from anything a real
    /// object would report — nothing here exists in storage at all.
    ///
    /// # Panics
    /// Panics when the built source does not carry the signed length.
    #[test]
    fn hot_leaf_sources_take_their_length_from_the_signed_descriptor() {
        let descriptor = PersistedFileDescriptor::Hot(wyrd_spec::vala::api::HotFileDescriptor {
            path: "tenant/spans/a.parquet".to_owned(),
            size_bytes: 987_654,
            row_count: 3,
            file_list_id: uuid::Uuid::now_v7(),
            sha256: [4_u8; 32],
            min_event_time_micros: Some(1),
            max_event_time_micros: Some(2),
        });
        let binding = wyrd_spec::vala::api::TenantTableBinding {
            tenant_id: wyrd_spec::DataTenantId::new_v7(),
            namespace: "vala.traces".to_owned(),
            table: "spans".to_owned(),
        };
        let source = signed_hot_source(&descriptor, "memory:///tenant/spans/a.parquet", &binding)
            .expect("signed descriptor builds a hot source");
        assert_eq!(source.size_bytes, 987_654);
        assert_eq!(source.location, "memory:///tenant/spans/a.parquet");
        assert!(
            source.event_time.interval().is_none(),
            "a follower carries no local pruning decision: the leader already made and signed it"
        );
    }

    /// Preflight refuses a signed assignment whose file list names both hot and
    /// Iceberg descriptors, before any provider resolution or object I/O.
    ///
    /// The two sources are resolved by different readers under different
    /// authority — a `vala.file_list` row and decoded checksum for hot, the
    /// pinned snapshot for Iceberg — so a mixed list has no single correct
    /// leaf. Silently resolving one variant would return a signed, digest-clean
    /// result that is missing the other source's rows, which is why this is a
    /// refusal rather than a partition.
    ///
    /// # Panics
    /// Panics when a mixed list is accepted or a uniform one is refused.
    #[test]
    fn preflight_refuses_an_assignment_naming_two_persisted_sources() {
        let mut assignment = FollowerScanAssignment {
            scan_id: "mixed-source-scan".to_owned(),
            binding: TenantTableBinding {
                tenant_id: DataTenantId::new_v7(),
                namespace: "vala.bifrost".to_owned(),
                table: "events".to_owned(),
            },
            persisted: PersistedFileAssignment {
                files: vec![crate::oracle::test_persisted_descriptor("a.parquet")],
            },
            scribe_provider_cut: None,
            schema_fingerprint: "mixed".to_owned(),
            required_columns: vec![DATA_TENANT_ID.to_owned()],
            predicates: Vec::new(),
            reader_cut: wyrd_spec::vala::api::FollowerReaderCut::no_snapshot(uuid::Uuid::nil(), 1),
        };
        let fence: wyrd_spec::vala::api::FencingToken = 1;
        assert!(
            PhysicalPlanFollower::<CountingResolver>::validate_assignment(
                &assignment,
                ClusterRole::Oracle,
                fence,
            )
            .is_ok(),
            "a uniform Iceberg assignment is accepted"
        );

        assignment
            .persisted
            .files
            .push(PersistedFileDescriptor::Hot(
                wyrd_spec::vala::api::HotFileDescriptor {
                    path: "b.parquet".to_owned(),
                    size_bytes: 4_096,
                    row_count: 1,
                    file_list_id: uuid::Uuid::now_v7(),
                    sha256: [9_u8; 32],
                    min_event_time_micros: None,
                    max_event_time_micros: None,
                },
            ));
        let refusal = PhysicalPlanFollower::<CountingResolver>::validate_assignment(
            &assignment,
            ClusterRole::Oracle,
            fence,
        )
        .expect_err("a mixed persisted source is refused");
        assert!(
            matches!(&refusal, PhysicalPlanFollowerError::Preflight(detail)
                if detail.contains("more than one persisted source")),
            "{refusal:?}"
        );
    }

    /// Proves a follower refuses empty fragments and that its partition count
    /// and sort-merge reservation come from one
    /// [`OracleSessionShape`](crate::resources::OracleSessionShape) built from
    /// this node's own admission, never from a value the requesting peer
    /// supplied.
    #[test]
    fn admitted_session_shape_contract() {
        let assignment = |files: usize, cut: bool| FollowerScanAssignment {
            scan_id: "shape-contract-scan".to_owned(),
            binding: TenantTableBinding {
                tenant_id: DataTenantId::new_v7(),
                namespace: "vala.bifrost".to_owned(),
                table: "events".to_owned(),
            },
            persisted: PersistedFileAssignment {
                files: (0..files)
                    .map(|index| {
                        crate::oracle::test_persisted_descriptor(&format!("file-{index}.parquet"))
                    })
                    .collect(),
            },
            scribe_provider_cut: cut.then(self::cut),
            schema_fingerprint: "shape".to_owned(),
            required_columns: vec!["data_tenant_id".to_owned()],
            predicates: Vec::new(),
            reader_cut: wyrd_spec::vala::api::FollowerReaderCut::no_snapshot(uuid::Uuid::nil(), 1),
        };

        for scannable in [
            vec![assignment(3, false)],
            vec![assignment(0, true)],
            vec![assignment(2, false), assignment(0, true)],
        ] {
            require_scannable_work(&scannable).expect("scannable fragment");
        }
        assert!(
            require_scannable_work(&[assignment(0, false)]).is_err(),
            "a fragment with no scannable work is a contract failure"
        );
        assert!(
            require_scannable_work(&[]).is_err(),
            "an empty assignment set is a contract failure"
        );

        let granted = crate::resources::ORACLE_PARTITION_MEMORY_BYTES;
        let execution = bounded_execution(granted, 8);
        let expected = crate::resources::OracleSessionShape::for_grant(granted, 8);
        assert_eq!(execution.shape(), expected);
        let (state, _context) = request_session(&execution);
        let options = state.config().options();
        assert_eq!(options.execution.target_partitions, 8);
        assert_eq!(
            options.execution.sort_spill_reservation_bytes,
            expected.sort_spill_reservation_bytes
        );
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
    /// empty Scribe provider cut, an unknown scan id, a wrong schema
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
        case.target_fence.role = ClusterRole::Scribe;
        malformed.push(case);
        let mut case = request.clone();
        case.target_fence.fencing_token += 1;
        malformed.push(case);
        let mut case = request.clone();
        case.assignments[0].scribe_provider_cut = Some(cut());
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
        let session = SessionContext::new().state();
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
                    persisted: PersistedFileAssignment { files: Vec::new() },
                    scribe_provider_cut: Some(cut()),
                    schema_fingerprint: super::super::assignment_schema_fingerprint(
                        schema.as_ref(),
                    ),
                    // The resolver now projects the assignment's own closure,
                    // so this fixture names the single column its memtable
                    // schema actually carries.
                    required_columns: vec!["wyrd_event_time".to_owned()],
                    predicates: Vec::new(),
                    reader_cut: wyrd_spec::vala::api::FollowerReaderCut::no_snapshot(
                        uuid::Uuid::nil(),
                        1,
                    ),
                },
                &session,
                None,
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
            Field::new(DATA_TENANT_ID, DataType::Utf8, false),
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
                Arc::new(StringArray::from(vec![
                    tenant_id.to_string(),
                    tenant_id.to_string(),
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

    /// Builds one Scribe assignment carrying the signed closure and predicate.
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
            persisted: PersistedFileAssignment { files: Vec::new() },
            scribe_provider_cut: Some(cut()),
            schema_fingerprint,
            required_columns,
            predicates: vec![wyrd_spec::vala::assignment_authority::ScanPredicate::Eq(
                "status_code".to_owned(),
                wyrd_spec::vala::assignment_authority::ScanLiteral::Utf8(
                    "STATUS_CODE_ERROR".to_owned(),
                ),
            )],
            reader_cut: wyrd_spec::vala::api::FollowerReaderCut::no_snapshot(uuid::Uuid::nil(), 1),
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
    /// requested output, the predicate-only column, and the hidden tenant
    /// column, in closure order.
    fn signed_closure() -> Vec<String> {
        vec![
            "duration_ms".to_owned(),
            "status_code".to_owned(),
            DATA_TENANT_ID.to_owned(),
        ]
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
    /// 1. the fetch request copies `assignment.required_columns` byte-for-byte
    ///    and retains `assignment.predicates`, so the memtable projects and
    ///    filters rather than the plan above discarding wide columns later;
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
    async fn scribe_provider_projects_and_filters_a_nonzero_ordinal() {
        use arrow::array::Int64Array;

        let (tenant_id, schema, stream, resolver) = closure_fixture_resolver();
        let session = SessionContext::new().state();
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
                None,
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
            vec![41_i64],
            "only the ERROR row survives, with its duration preserved"
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
                None,
            )
            .await
            .expect("a non-owning assignment resolves to an empty branch");
        assert_eq!(
            plan_column_names(&unowned.plan),
            closure,
            "the empty branch declares the same closure schema"
        );

        // Property 1: the fetch request carries the signed closure and its
        // predicates byte for byte, so the memtable projects and filters at the
        // source instead of the plan above discarding wide columns later.
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
            .resolve(ClusterRole::Scribe, &assignment, &session, None)
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
        assert_eq!(
            recorded[0].predicates, assignment.predicates,
            "the fetch request forwards the signed predicates unchanged"
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
        let session = SessionContext::new().state();
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
                    None,
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

    /// Every authenticated request-component contradiction fails before resolution.
    ///
    /// # Panics
    /// Panics if the canonical physical-plan fixtures cannot be encoded or decoded.
    pub(crate) async fn assert_complete_preflight_matrix() {
        let (request, binding) = oracle_request().expect("valid Oracle request");
        let calls = Arc::new(AtomicUsize::new(0));
        let follower = PhysicalPlanFollower::new(CountingResolver {
            calls: Arc::clone(&calls),
            schema: Arc::new(Schema::new(vec![
                Field::new("value", DataType::Int64, true),
                Field::new(DATA_TENANT_ID, DataType::Utf8, false),
            ])),
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
