//! Three-stage authenticated physical-plan follower lifecycle.

use std::any::Any;
use std::collections::HashMap;
use std::fmt;
use std::ops::Range;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use arrow::compute::cast;
#[cfg(any(test, feature = "test-support"))]
use arrow::datatypes::SchemaRef;
use arrow::record_batch::RecordBatch;
use async_trait::async_trait;
use datafusion::common::tree_node::{Transformed, TreeNode};
use datafusion::datasource::TableProvider;
use datafusion::datasource::memory::MemorySourceConfig;
use datafusion::datasource::physical_plan::{FileGroup, FileScanConfig, FileScanConfigBuilder};
use datafusion::datasource::source::DataSourceExec;
use datafusion::error::{DataFusionError, Result as DataFusionResult};
use datafusion::execution::TaskContext;
use datafusion::execution::memory_pool::{MemoryConsumer, MemoryPool, MemoryReservation};
use datafusion::execution::runtime_env::RuntimeEnvBuilder;
use datafusion::execution::session_state::{SessionState, SessionStateBuilder};
use datafusion::physical_expr::EquivalenceProperties;
use datafusion::physical_plan::execution_plan::{Boundedness, EmissionType, PlanProperties};
use datafusion::physical_plan::stream::RecordBatchStreamAdapter;
use datafusion::physical_plan::{
    DisplayAs, DisplayFormatType, ExecutionPlan, Partitioning, SendableRecordBatchStream,
    execute_stream,
};
use datafusion_proto::bytes::physical_plan_from_bytes_with_extension_codec;
use datafusion_proto::protobuf::{PhysicalPlanNode, physical_plan_node::PhysicalPlanType};
use futures_util::StreamExt;
use iceberg::io::FileRead;
use iceberg_datafusion::physical_plan::IcebergTableScan;
use parquet::arrow::arrow_reader::ArrowReaderOptions;
use parquet::arrow::async_reader::{AsyncFileReader, ParquetRecordBatchStreamBuilder};
use parquet::errors::ParquetError;
use parquet::file::metadata::{ParquetMetaData, ParquetMetaDataReader};
use prost::Message;
use thiserror::Error;
use wyrd_spec::DataTenantId;
use wyrd_spec::vala::api::{
    ClusterRole, FollowerScanAssignment, OracleRoleFence, PhysicalExecuteFragmentRequest,
    ReservationId, TenantTableBinding,
};
use wyrd_spec::vala::managed_columns::DATA_TENANT_ID;

use super::codec::{OraclePhysicalExtensionCodec, PreflightExtension, physical_plan_fingerprint};
use crate::catalog::layout::TimePartition;
use crate::catalog::{BifrostCatalog, TableRef, TenantTableBinding as CatalogTableBinding};
use crate::scribe::stream_identity::StreamIdentity;
use crate::scribe::tail_rpc::{FetchLiveTailRequest, FetchLiveTailService, HotBatch};
use crate::scribe::wal::WalLsn;

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
    Resolution(String),
    /// `DataFusion` rejected native physical fields after complete resolution.
    #[error("physical-plan decode failed after provider resolution: {0}")]
    PostResolutionDecode(String),
    /// The decoded plan could not create its output stream.
    #[error("physical-plan execution failed after decode: {0}")]
    Execution(String),
}

/// Async boundary that constructs one authenticated role-local scan provider.
#[async_trait]
pub trait FollowerSourceResolver: Send + Sync {
    /// Resolves exactly one validated assignment for the signed target role.
    /// Cancellation may discard a locally acquired provider; no provider is
    /// published to decode until this future returns successfully. Callers may
    /// retry only by restarting the complete authenticated follower request.
    ///
    /// # Errors
    /// Returns a redacted message when catalog, storage, or Scribe snapshot IO fails.
    async fn resolve(
        &self,
        target_role: ClusterRole,
        assignment: &FollowerScanAssignment,
        session: &SessionState,
    ) -> Result<Arc<dyn ExecutionPlan>, String>;
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
    ) -> Result<Arc<dyn ExecutionPlan>, String> {
        self.as_ref()
            .resolve(target_role, assignment, session)
            .await
    }
}

/// Request-local session policy derived from one trusted admitted grant.
///
/// Every follower session — leader-local Oracle, remote Oracle peer, or Scribe
/// hot-tail peer — is shaped by the grant its own admission produced, never by
/// a process-wide constant and never by a value the requesting peer supplied.
/// The three knobs come from one
/// [`OracleSessionShape`](crate::resources::OracleSessionShape), the same type
/// the leader's session uses, so a follower cannot end up with a partition
/// count sized for one ceiling and a batch size sized for another.
#[derive(Clone)]
pub struct FollowerSessionFactory {
    /// Bounded pool nested under the admission that granted this execution.
    memory_pool: Arc<dyn MemoryPool>,
    /// Trusted grant bytes backing `memory_pool`.
    granted_memory_bytes: usize,
    /// Partition ceiling admitted alongside that grant.
    admitted_target_partitions: usize,
}

impl std::fmt::Debug for FollowerSessionFactory {
    /// Formats only the non-sensitive admitted execution bounds.
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("FollowerSessionFactory")
            .field("granted_memory_bytes", &self.granted_memory_bytes)
            .field(
                "admitted_target_partitions",
                &self.admitted_target_partitions,
            )
            .finish()
    }
}

impl FollowerSessionFactory {
    /// Binds one admitted grant to the session it is allowed to shape.
    #[must_use]
    pub fn for_grant(
        memory_pool: Arc<dyn MemoryPool>,
        granted_memory_bytes: usize,
        admitted_target_partitions: usize,
    ) -> Self {
        Self {
            memory_pool,
            granted_memory_bytes,
            admitted_target_partitions: admitted_target_partitions.max(1),
        }
    }

    /// Returns the session shape this grant produces for `work_units`.
    ///
    /// Partitions narrow to the work this fragment was actually assigned, so a
    /// one-file fragment does not open a wide plan it cannot fill.
    #[must_use]
    pub fn shape(&self, work_units: usize) -> crate::resources::OracleSessionShape {
        crate::resources::OracleSessionShape::for_grant(
            self.granted_memory_bytes,
            self.admitted_target_partitions,
            work_units,
        )
    }

    /// Creates one request-local runtime, session state, and task context.
    ///
    /// # Errors
    /// Returns a redacted error when `DataFusion` cannot construct the bounded runtime.
    fn create(&self, work_units: usize) -> Result<(SessionState, Arc<TaskContext>), String> {
        let runtime = Arc::new(
            RuntimeEnvBuilder::new()
                .with_memory_pool(Arc::clone(&self.memory_pool))
                .build()
                .map_err(|_| "governed follower runtime construction failed".to_owned())?,
        );
        // `session_config` applies the same fixed Parquet pushdown and indexing
        // options the leader's session uses: this is the session that actually
        // opens the dispatched files and prunes their row groups and pages.
        let config = self.shape(work_units).session_config();
        let state = SessionStateBuilder::new()
            .with_default_features()
            .with_config(config)
            .with_runtime_env(runtime)
            .build();
        let task = Arc::new(TaskContext::from(&state));
        Ok((state, task))
    }
}

/// Counts the independently scannable units one follower fragment was assigned.
///
/// This is the follower's half of the leader's `scannable_work_units`: each
/// dispatched persisted file and each assigned Scribe cut is one independently
/// openable scan target, and `DataFusion` cannot usefully spread a fragment
/// across more partitions than it has targets to read.
///
/// # Errors
///
/// Returns [`PhysicalPlanFollowerError::Preflight`] when the assignments carry
/// no scannable work at all, which is a contract failure rather than an empty
/// result: a fragment with nothing to scan should never have been dispatched.
pub fn oracle_assigned_work_units(
    assignments: &[FollowerScanAssignment],
) -> Result<usize, PhysicalPlanFollowerError> {
    let units = assignments.iter().fold(0_usize, |total, assignment| {
        total
            .saturating_add(assignment.persisted.files.len())
            .saturating_add(usize::from(assignment.scribe_provider_cut.is_some()))
    });
    if units == 0 {
        return Err(PhysicalPlanFollowerError::Preflight(
            "dispatched fragment carries no scannable work".to_owned(),
        ));
    }
    Ok(units)
}

/// Validated IO-free request projection passed into provider resolution.
#[derive(Debug)]
struct PreflightRequest {
    /// Assignments keyed by exact encoded scan identity.
    assignments: HashMap<String, FollowerScanAssignment>,
}

/// Authenticated ticket and local-incarnation facts used by IO-free preflight.
#[derive(Debug, Clone)]
pub struct AuthenticatedFollowerContext<'a> {
    /// Tenant recovered only from verified ticket claims.
    pub tenant_id: DataTenantId,
    /// Exact table binding recovered from verified ticket claims.
    pub table_binding: &'a TenantTableBinding,
    /// Exact pending reservation recovered from the verified ticket.
    pub reservation_id: &'a ReservationId,
    /// Leader incarnation recovered from verified ticket claims.
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
}

/// One canonical hot object admitted to a follower's lazy ranged reader.
#[derive(Debug, Clone)]
struct FollowerHotFile {
    /// Exact storage-qualified object identity derived from the tenant binding.
    location: String,
    /// Storage metadata size used to reject invalid ranges before IO.
    size: u64,
}

/// Bounded lazy Parquet leaf for an authenticated Oracle hot-file assignment.
///
/// Visible to sibling `oracle` submodules so [`super::exec::OracleQueryScanStats`]
/// can fold this follower-local leaf into the same closed physical
/// scan-evidence counters produced for a local `HotParquetExec` read.
#[derive(Debug)]
pub(super) struct FollowerHotParquetExec {
    /// Canonical assigned objects in deterministic assignment order.
    files: Vec<FollowerHotFile>,
    /// Shared tenant-qualified Iceberg storage reader.
    file_io: iceberg::io::FileIO,
    /// Exact physical table schema expected from every assigned object.
    schema: arrow::datatypes::SchemaRef,
    /// Request-local pool backed by the retained Oracle worker lease.
    memory_pool: Arc<dyn MemoryPool>,
    /// Shared terminal metric owner retained by query telemetry.
    metrics: Arc<super::exec::OracleScanMetricsHandle>,
    /// Closed predicate conjunction bound to this assignment, used to skip a
    /// file whose footer statistics prove no row group can satisfy every leaf.
    predicates: Vec<wyrd_spec::vala::assignment_authority::ScanPredicate>,
    /// Cached bounded single-partition leaf properties.
    properties: Arc<PlanProperties>,
}

impl FollowerHotParquetExec {
    /// Creates one lazy follower leaf after every object identity and size is validated.
    fn new(
        files: Vec<FollowerHotFile>,
        file_io: iceberg::io::FileIO,
        schema: arrow::datatypes::SchemaRef,
        memory_pool: Arc<dyn MemoryPool>,
        predicates: Vec<wyrd_spec::vala::assignment_authority::ScanPredicate>,
    ) -> Self {
        let properties = Arc::new(PlanProperties::new(
            EquivalenceProperties::new(Arc::clone(&schema)),
            Partitioning::UnknownPartitioning(1),
            EmissionType::Incremental,
            Boundedness::Bounded,
        ));
        Self {
            files,
            file_io,
            schema,
            memory_pool,
            predicates,
            metrics: Arc::new(super::exec::OracleScanMetricsHandle::default()),
            properties,
        }
    }

    /// Returns the shared terminal metric owner for this follower leaf.
    pub(super) fn metrics(&self) -> &Arc<super::exec::OracleScanMetricsHandle> {
        &self.metrics
    }
}

impl DisplayAs for FollowerHotParquetExec {
    /// Renders only the authenticated object count, never tenant storage paths.
    fn fmt_as(
        &self,
        _format: DisplayFormatType,
        formatter: &mut fmt::Formatter<'_>,
    ) -> fmt::Result {
        write!(
            formatter,
            "FollowerHotParquetExec files={}",
            self.files.len()
        )
    }
}

impl ExecutionPlan for FollowerHotParquetExec {
    /// Returns the stable native follower leaf name.
    fn name(&self) -> &'static str {
        "FollowerHotParquetExec"
    }

    /// Exposes this concrete leaf for native plan inspection.
    fn as_any(&self) -> &dyn Any {
        self
    }

    /// Returns the cached single-partition bounded properties.
    fn properties(&self) -> &Arc<PlanProperties> {
        &self.properties
    }

    /// This source has no child plans.
    fn children(&self) -> Vec<&Arc<dyn ExecutionPlan>> {
        Vec::new()
    }

    /// Reuses this source only when no children are supplied.
    fn with_new_children(
        self: Arc<Self>,
        children: Vec<Arc<dyn ExecutionPlan>>,
    ) -> DataFusionResult<Arc<dyn ExecutionPlan>> {
        if children.is_empty() {
            Ok(self)
        } else {
            Err(DataFusionError::Plan(
                "FollowerHotParquetExec is a leaf plan".to_owned(),
            ))
        }
    }

    /// Starts lazy sequential ranged reads within the request-local worker pool.
    fn execute(
        &self,
        partition: usize,
        context: Arc<TaskContext>,
    ) -> DataFusionResult<SendableRecordBatchStream> {
        if partition != 0 {
            return Err(DataFusionError::Execution(format!(
                "FollowerHotParquetExec has no partition {partition}"
            )));
        }
        // The hot leaf reads at the admitted session's batch size, so this path
        // is shaped by the same grant as every other follower operator.
        let batch_size = context.session_config().batch_size();
        let files = self.files.clone();
        let file_io = self.file_io.clone();
        let schema = Arc::clone(&self.schema);
        let output_schema = Arc::clone(&schema);
        let memory_pool = Arc::clone(&self.memory_pool);
        let metrics = Arc::clone(&self.metrics);
        let predicates = self.predicates.clone();
        let stream = async_stream::try_stream! {
            for file in files {
                let input = file_io
                    .new_input(&file.location)
                    .map_err(|error| DataFusionError::External(Box::new(error)))?;
                let reader = input
                    .reader()
                    .await
                    .map_err(|error| DataFusionError::External(Box::new(error)))?;
                let reader = FollowerParquetReader::new(
                    reader,
                    file.size,
                    Arc::clone(&memory_pool),
                    Arc::clone(&metrics),
                );
                // Recorded before the footer is read, matching the leader's
                // hot path: every attempt on this file publishes one file
                // observation even when the reader fails mid-open, and
                // row-group pruning is reported separately.
                metrics.record_hot_file();
                let builder = ParquetRecordBatchStreamBuilder::new(reader)
                    .await
                    .map_err(|error| DataFusionError::External(Box::new(error)))?;
                let selection =
                    super::exec::select_row_groups_for_predicates(builder.metadata(), &predicates);
                metrics.record_row_groups(&selection);
                if selection.excludes_file() {
                    continue;
                }
                let mut batches = builder
                    .with_row_groups(selection.retained)
                    .with_batch_size(batch_size)
                    .build()
                    .map_err(|error| DataFusionError::External(Box::new(error)))?;
                while let Some(batch) = batches.next().await {
                    let batch = batch.map_err(|error| DataFusionError::External(Box::new(error)))?;
                    let batch = project_follower_batch(&batch, Arc::clone(&schema))?;
                    let decoded = MemoryConsumer::new("oracle-follower-hot-decoded")
                        .register(&memory_pool);
                    decoded
                        .try_grow(batch.get_array_memory_size())
                        .map_err(|error| DataFusionError::ResourcesExhausted(error.to_string()))?;
                    yield batch;
                    drop(decoded);
                }
            }
        };
        Ok(Box::pin(RecordBatchStreamAdapter::new(
            output_schema,
            stream,
        )))
    }
}

/// Projects one decoded hot batch to the authenticated provider schema by field name.
///
/// # Errors
/// Returns a closed execution failure when a required field is absent, cannot be cast, or the
/// projected Arrow batch is invalid.
fn project_follower_batch(
    batch: &RecordBatch,
    schema: arrow::datatypes::SchemaRef,
) -> DataFusionResult<RecordBatch> {
    let columns = schema
        .fields()
        .iter()
        .map(|field| {
            let index = batch.schema().index_of(field.name()).map_err(|_| {
                DataFusionError::Execution(
                    "authenticated Oracle hot provider omitted a required field".to_owned(),
                )
            })?;
            let column = batch.column(index);
            if column.data_type() == field.data_type() {
                Ok(Arc::clone(column))
            } else {
                cast(column, field.data_type()).map_err(DataFusionError::from)
            }
        })
        .collect::<DataFusionResult<Vec<_>>>()?;
    RecordBatch::try_new(schema, columns).map_err(DataFusionError::from)
}

/// Exact range bytes coupled to their request-local pool reservation.
struct FollowerRangeOwner {
    /// Immutable bytes returned by the storage range read.
    bytes: bytes::Bytes,
    /// Capacity retained until the final bytes clone or slice drops.
    _reservation: MemoryReservation,
}

impl AsRef<[u8]> for FollowerRangeOwner {
    /// Borrows the retained storage bytes without copying them.
    fn as_ref(&self) -> &[u8] {
        self.bytes.as_ref()
    }
}

/// Parquet adapter that reserves every storage range before performing IO.
struct FollowerParquetReader {
    /// Pinned Iceberg ranged reader for one canonical object.
    reader: Box<dyn FileRead>,
    /// Metadata size used to reject out-of-bounds requests.
    size: u64,
    /// Request-local worker pool backing every range reservation.
    memory_pool: Arc<dyn MemoryPool>,
    /// Shared terminal metric owner recording each accepted range read.
    metrics: Arc<super::exec::OracleScanMetricsHandle>,
}

impl FollowerParquetReader {
    /// Creates one bounded reader for a metadata-validated object.
    fn new(
        reader: Box<dyn FileRead>,
        size: u64,
        memory_pool: Arc<dyn MemoryPool>,
        metrics: Arc<super::exec::OracleScanMetricsHandle>,
    ) -> Self {
        Self {
            reader,
            size,
            memory_pool,
            metrics,
        }
    }
}

impl AsyncFileReader for FollowerParquetReader {
    /// Reserves an exact range before IO and retains the charge with returned bytes.
    fn get_bytes(
        &mut self,
        range: Range<u64>,
    ) -> futures_util::future::BoxFuture<'_, parquet::errors::Result<bytes::Bytes>> {
        Box::pin(async move {
            let requested = range.end.checked_sub(range.start).ok_or_else(|| {
                ParquetError::General("hot Parquet range start exceeds end".to_owned())
            })?;
            if range.end > self.size {
                return Err(ParquetError::General(
                    "hot Parquet range exceeds authenticated object size".to_owned(),
                ));
            }
            let requested = usize::try_from(requested)
                .map_err(|_| ParquetError::General("hot Parquet range exceeds usize".to_owned()))?;
            let reservation =
                MemoryConsumer::new("oracle-follower-hot-range").register(&self.memory_pool);
            reservation
                .try_grow(requested)
                .map_err(|error| ParquetError::General(error.to_string()))?;
            let bytes = self
                .reader
                .read(range)
                .await
                .map_err(|error| ParquetError::General(error.to_string()))?;
            self.metrics.record_hot_range(bytes.len());
            if bytes.len() != requested {
                return Err(ParquetError::General(
                    "hot Parquet range returned a short read".to_owned(),
                ));
            }
            Ok(bytes::Bytes::from_owner(FollowerRangeOwner {
                bytes,
                _reservation: reservation,
            }))
        })
    }

    /// Loads footer/page metadata through the same pre-reserved range path.
    fn get_metadata<'a>(
        &'a mut self,
        _options: Option<&'a ArrowReaderOptions>,
    ) -> futures_util::future::BoxFuture<'a, parquet::errors::Result<Arc<ParquetMetaData>>> {
        Box::pin(async move {
            let size = self.size;
            ParquetMetaDataReader::new()
                .load_and_finish(self, size)
                .await
                .map(Arc::new)
        })
    }
}

#[async_trait]
impl FollowerSourceResolver for OracleCatalogResolver {
    /// Builds one tenant-qualified catalog/Iceberg scan and rejects Scribe assignments.
    /// Cancellation during catalog or scan IO drops all locally acquired state
    /// and exposes no provider; retry restarts the complete resolution.
    ///
    /// # Errors
    /// Returns a redacted resolution error for a role mismatch, invalid table binding,
    /// catalog lookup failure, or physical scan construction failure.
    async fn resolve(
        &self,
        target_role: ClusterRole,
        assignment: &FollowerScanAssignment,
        session: &SessionState,
    ) -> Result<Arc<dyn ExecutionPlan>, String> {
        if target_role != ClusterRole::Oracle || assignment.scribe_provider_cut.is_some() {
            return Err("Oracle resolver received a non-Oracle assignment".to_owned());
        }
        let table = assignment_table(&assignment.binding)?;
        let provider = self
            .catalog
            .provider(&table, assignment.binding.tenant_id)
            .await
            .map_err(|_| "authenticated Oracle catalog provider failed".to_owned())?;
        // Validates the full physical schema fingerprint against the
        // authenticated table's actual schema immediately after catalog
        // resolution and before any per-file object I/O (the hot-file branch
        // below issues metadata HEAD requests). `decode` repeats this check
        // once more after every scan id in the request resolves, but that
        // later check alone would let a mismatched hot assignment reach
        // storage first.
        let actual = super::assignment_schema_fingerprint(provider.schema().as_ref());
        if actual != assignment.schema_fingerprint {
            return Err("resolved provider schema fingerprint differs from assignment".to_owned());
        }
        if assignment.persisted.files.is_empty() {
            let schema = provider.schema();
            let batch = arrow::record_batch::RecordBatch::new_empty(Arc::clone(&schema));
            return MemorySourceConfig::try_new_exec(&[vec![batch]], schema, None)
                .map(|plan| plan as Arc<dyn ExecutionPlan>)
                .map_err(|_| "authenticated Oracle empty provider failed".to_owned());
        }
        let plan = provider
            .scan(session, None, &[], None)
            .await
            .map_err(|_| "authenticated Oracle physical scan failed".to_owned())?;
        let catalog_binding =
            CatalogTableBinding::resolve((assignment.binding.tenant_id, table))
                .map_err(|_| "authenticated Oracle assignment binding failed".to_owned())?;
        let assigned_locations = assignment
            .persisted
            .files
            .iter()
            .map(|file| {
                let table_relative = file
                    .strip_prefix(&catalog_binding.object_prefix)
                    .and_then(|suffix| suffix.strip_prefix('/'))
                    .ok_or_else(|| "authenticated Oracle assignment binding failed".to_owned())?
                    .to_owned();
                self.catalog
                    .object_location(&catalog_binding, file)
                    .map(|location| (file.clone(), table_relative, location))
                    .map_err(|_| "authenticated Oracle assignment location failed".to_owned())
            })
            .collect::<Result<Vec<_>, _>>()?;
        if assignment.scan_id.ends_with(":hot") {
            let mut files = Vec::with_capacity(assigned_locations.len());
            for (_, _, location) in &assigned_locations {
                let input = self
                    .catalog
                    .file_io()
                    .new_input(location)
                    .map_err(|_| "authenticated Oracle hot provider failed".to_owned())?;
                let metadata = input
                    .metadata()
                    .await
                    .map_err(|_| "authenticated Oracle hot provider failed".to_owned())?;
                files.push(FollowerHotFile {
                    location: location.clone(),
                    size: metadata.size,
                });
            }
            return Ok(Arc::new(FollowerHotParquetExec::new(
                files,
                self.catalog.file_io().clone(),
                provider.schema(),
                session.runtime_env().memory_pool.clone(),
                assignment.predicates.clone(),
            )));
        }
        restrict_plan_to_assigned_files(plan, &assigned_locations)
    }
}

/// Follower resolver that serves one fixed in-memory cohort for every
/// assignment it receives.
///
/// Peer transport proofs — fencing, ticket verification, TLS, footer framing,
/// reservation release — need a follower that produces a deterministic,
/// schema-stable result without a tenant catalog or object storage behind it.
/// The worker still runs the real decode, tenant tripwire, and footer path over
/// whatever this returns, so only provider acquisition is short-circuited.
#[cfg(feature = "test-support")]
#[derive(Debug)]
pub struct FixedCohortResolver {
    /// Schema every resolved source reports; must match the fingerprint the
    /// dispatched assignment carries or decode rejects the plan.
    schema: SchemaRef,
    /// Cohort replayed for each resolution, in one partition.
    batches: Vec<RecordBatch>,
}

#[cfg(feature = "test-support")]
impl FixedCohortResolver {
    /// Binds the cohort this resolver replays.
    #[must_use]
    pub fn new(schema: SchemaRef, batches: Vec<RecordBatch>) -> Self {
        Self { schema, batches }
    }
}

#[cfg(feature = "test-support")]
#[async_trait]
impl FollowerSourceResolver for FixedCohortResolver {
    /// Returns the bound cohort as a single-partition in-memory source.
    ///
    /// # Errors
    /// Returns a redacted message when `DataFusion` rejects the cohort against
    /// the bound schema.
    async fn resolve(
        &self,
        _target_role: ClusterRole,
        _assignment: &FollowerScanAssignment,
        _session: &SessionState,
    ) -> Result<Arc<dyn ExecutionPlan>, String> {
        MemorySourceConfig::try_new_exec(
            std::slice::from_ref(&self.batches),
            Arc::clone(&self.schema),
            None,
        )
        .map(|plan| plan as Arc<dyn ExecutionPlan>)
        .map_err(|_| "fixed cohort resolver rejected its bound cohort".to_owned())
    }
}

/// Authenticated Scribe resolver backed by the unchanged pod-local live-tail service.
#[async_trait]
trait LiveTailSource: Send + Sync + std::fmt::Debug {
    /// Returns the exact stream incarnation served by this source.
    fn stream(&self) -> StreamIdentity;

    /// Captures one bounded active-plus-unretired-immutable Arrow cohort.
    /// Cancellation may abandon the local snapshot attempt before it is returned;
    /// no partial cohort is exposed and a retry takes a new atomic snapshot.
    ///
    /// # Errors
    /// Returns a redacted error when the unchanged Scribe snapshot call fails.
    async fn fetch(&self, request: FetchLiveTailRequest) -> Result<Vec<HotBatch>, String>;
}

#[async_trait]
impl LiveTailSource for FetchLiveTailService {
    /// Returns the unchanged service's local stream identity.
    fn stream(&self) -> StreamIdentity {
        FetchLiveTailService::stream(self)
    }

    /// Delegates exactly once to the unchanged Scribe production snapshot operation.
    /// Cancellation drops the in-progress snapshot future and exposes no partial
    /// batch vector; a retry invokes a new complete snapshot.
    ///
    /// # Errors
    /// Returns a redacted error when Scribe rejects or cannot build the snapshot.
    async fn fetch(&self, request: FetchLiveTailRequest) -> Result<Vec<HotBatch>, String> {
        self.fetch_hot_batches(request)
            .await
            .map_err(|_| "Scribe live-tail snapshot failed".to_owned())
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
    /// Returns a redacted resolution error for role/binding/range conversion,
    /// live-tail snapshot, or Arrow provider construction failure.
    async fn resolve(
        &self,
        target_role: ClusterRole,
        assignment: &FollowerScanAssignment,
        _session: &SessionState,
    ) -> Result<Arc<dyn ExecutionPlan>, String> {
        if target_role != ClusterRole::Scribe || !assignment.persisted.files.is_empty() {
            return Err("Scribe resolver received a non-Scribe assignment".to_owned());
        }
        let cut = assignment
            .scribe_provider_cut
            .as_ref()
            .ok_or_else(|| "Scribe assignment is missing its provider cut".to_owned())?;
        let stream = self.tail.stream();
        if u64::try_from(stream.writer_epoch.as_i64()).ok() != Some(cut.writer_epoch) {
            return Err("Scribe writer epoch differs from the authenticated cut".to_owned());
        }
        let persisted_lsn_ranges = cut
            .persisted_ranges
            .iter()
            .map(|range| {
                Ok((
                    checked_wal_lsn(range.start_lsn)?,
                    checked_wal_lsn(range.end_lsn)?,
                ))
            })
            .collect::<Result<Vec<_>, String>>()?;
        let binding = crate::catalog::TenantTableBinding::resolve((
            assignment.binding.tenant_id,
            assignment_table(&assignment.binding)?,
        ))
        .map_err(|_| "Scribe tenant/table binding is invalid".to_owned())?;
        let schema = match &self.schema_source {
            ScribeSchemaSource::Catalog(catalog) => catalog
                .provider(
                    &assignment_table(&assignment.binding)?,
                    assignment.binding.tenant_id,
                )
                .await
                .map_err(|_| "authenticated Scribe schema resolution failed".to_owned())?
                .schema(),
            #[cfg(test)]
            ScribeSchemaSource::Fixed(schema) => Arc::clone(schema),
        };
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
            let batch = RecordBatch::new_empty(Arc::clone(&schema));
            return MemorySourceConfig::try_new_exec(&[vec![batch]], schema, None)
                .map(|plan| plan as Arc<dyn ExecutionPlan>)
                .map_err(|_| "authenticated Scribe empty provider failed".to_owned());
        }
        let start_partition = TimePartition::from_wire(cut.start_partition);
        let end_partition = TimePartition::from_wire(cut.end_partition);
        let max_batches = usize::try_from(cut.maximum_batch_count)
            .map_err(|_| "Scribe batch bound does not fit this process".to_owned())?;
        let max_retained_bytes = usize::try_from(cut.maximum_retained_bytes)
            .map_err(|_| "Scribe byte bound does not fit this process".to_owned())?;
        let fetch_started = std::time::Instant::now();
        let fetched = self
            .tail
            .fetch(FetchLiveTailRequest {
                binding,
                target_stream: stream,
                start_partition,
                end_partition,
                after_lsn: checked_wal_lsn(cut.persisted_cursor)?,
                persisted_lsn_ranges,
                required_columns: cut.required_columns.clone(),
                max_batches,
                max_retained_bytes,
            })
            .await;
        // The leader skips its own fence drain whenever followers are dispatched,
        // so this bounded snapshot is the only live-tail read a distributed query
        // performs. It carries the `remote` locality of the same page families the
        // single-node drain emits, keeping live-tail observation continuous across
        // both execution shapes.
        let page_outcome = if fetched.is_ok() { "success" } else { "failed" };
        metrics::counter!(
            "bifrost_oracle_tail_pages_total",
            "locality" => "remote",
            "outcome" => page_outcome
        )
        .increment(1);
        metrics::histogram!(
            "bifrost_oracle_tail_page_seconds",
            "locality" => "remote",
            "outcome" => page_outcome
        )
        .record(fetch_started.elapsed().as_secs_f64());
        let batches = fetched?;
        let rows = batches
            .into_iter()
            .map(|batch| batch.rows)
            .collect::<Vec<_>>();
        super::exec::OracleTableProvider::validated_memory_source(&rows, schema)
            .map_err(|_| "Scribe Arrow provider construction failed".to_owned())
    }
}

/// Parses one exact domain table binding into the catalog's closed namespace type.
///
/// # Errors
/// Returns an error when the namespace/table pair is not a canonical Wyrd FQN.
fn assignment_table(binding: &TenantTableBinding) -> Result<TableRef, String> {
    TableRef::parse_fqn(&format!("{}.{}", binding.namespace, binding.table))
        .ok_or_else(|| "assignment table binding is not canonical".to_owned())
}

/// Restricts every file-backed leaf to the exact authenticated assignment.
///
/// The Iceberg provider remains responsible for schema, pruning, and tenant
/// filtering. This final projection removes every unassigned manifest file and
/// rejects assignments that do not correspond to a planned file.
///
/// # Errors
/// Returns an error when the provider is not file-backed, an assigned location
/// is absent, or a non-file leaf prevents proving exact file-set equality.
fn restrict_plan_to_assigned_files(
    plan: Arc<dyn ExecutionPlan>,
    assigned_files: &[(String, String, String)],
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
            if let Some(exec) = node
                .as_any()
                .downcast_ref::<super::exec::OracleIcebergScanExec>()
            {
                file_leaves = file_leaves.saturating_add(1);
                observed.extend(assigned.iter().cloned());
                return Ok(Transformed::yes(Arc::new(
                    exec.clone().with_assigned_files(assigned.clone()),
                )));
            }
            if node.as_any().is::<IcebergTableScan>() {
                let restricted = super::exec::OracleIcebergScanExec::from_plan(node.as_ref())
                    .map_err(|_| {
                        datafusion::common::DataFusionError::Plan(
                            "authenticated Oracle Iceberg source failed".to_owned(),
                        )
                    })?
                    .with_assigned_files(assigned.clone());
                file_leaves = file_leaves.saturating_add(1);
                observed.extend(assigned.iter().cloned());
                return Ok(Transformed::yes(Arc::new(restricted)));
            }
            let Some(exec) = node.as_any().downcast_ref::<DataSourceExec>() else {
                return Ok(Transformed::no(node));
            };
            let Some(config) = exec.data_source().as_any().downcast_ref::<FileScanConfig>() else {
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
        return Err("authenticated Oracle assignment differs from planned files".to_owned());
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

/// Converts one validated wire endpoint into the Redux WAL owner type.
///
/// # Errors
/// Returns an error when the endpoint exceeds Redux's signed persistence bound.
fn checked_wal_lsn(value: u64) -> Result<WalLsn, String> {
    if value > i64::MAX as u64 {
        Err("WAL endpoint exceeds the Redux persistence bound".to_owned())
    } else {
        Ok(WalLsn::new(value))
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
    /// Returns [`PhysicalPlanFollowerError`] for preflight, provider resolution,
    /// or post-resolution native `DataFusion` decode failure.
    pub async fn decode(
        &self,
        request: &PhysicalExecuteFragmentRequest,
        authenticated: AuthenticatedFollowerContext<'_>,
        session: &SessionState,
        context: &TaskContext,
    ) -> Result<Arc<dyn ExecutionPlan>, PhysicalPlanFollowerError> {
        let preflight = self.preflight(request, &authenticated)?;
        let mut providers = HashMap::with_capacity(preflight.assignments.len());
        for (scan_id, assignment) in preflight.assignments {
            self.effects.resolver.fetch_add(1, Ordering::SeqCst);
            let provider = self
                .resolver
                .resolve(request.target_fence.role, &assignment, session)
                .await
                .map_err(PhysicalPlanFollowerError::Resolution)?;
            let actual = super::assignment_schema_fingerprint(provider.schema().as_ref());
            if actual != assignment.schema_fingerprint {
                return Err(PhysicalPlanFollowerError::Resolution(
                    "resolved provider schema fingerprint differs from assignment".to_owned(),
                ));
            }
            providers.insert(scan_id, provider);
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
        request: &PhysicalExecuteFragmentRequest,
        authenticated: AuthenticatedFollowerContext<'_>,
        sessions: &FollowerSessionFactory,
    ) -> Result<FollowerExecution, PhysicalPlanFollowerError> {
        let work_units = oracle_assigned_work_units(&request.assignments)?;
        let (session, context) = sessions
            .create(work_units)
            .map_err(PhysicalPlanFollowerError::Execution)?;
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
    /// its identity: the persisted file list must be unique and ordered, the
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
                || cut.required_columns.is_empty()
                || cut.maximum_batch_count == 0
                || cut.maximum_retained_bytes == 0
                || !cut.is_valid())
        {
            return Err(PhysicalPlanFollowerError::Preflight(
                "invalid Scribe provider cut".to_owned(),
            ));
        }
        // The top-level assignment projection is authoritative; a Scribe
        // cut carries its own `required_columns` for the memory-provider
        // wire shape, but it must copy the signed top-level closure
        // byte-for-byte rather than union or narrow it independently.
        if let Some(cut) = &assignment.scribe_provider_cut
            && cut.required_columns != assignment.required_columns
        {
            return Err(PhysicalPlanFollowerError::Preflight(
                "Scribe provider cut projection differs from the assignment closure".to_owned(),
            ));
        }
        Ok(())
    }

    fn preflight(
        &self,
        request: &PhysicalExecuteFragmentRequest,
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
                || assignment.binding.tenant_id.as_uuid().is_nil()
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
    fn valid_persisted_files(files: &[String]) -> bool {
        files.iter().all(|file| !file.trim().is_empty())
            && files.windows(2).all(|pair| pair[0] < pair[1])
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
    request: &PhysicalExecuteFragmentRequest,
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
        ) -> Result<Arc<dyn ExecutionPlan>, String> {
            Err("preflight resolver must not run".to_owned())
        }
    }
    PhysicalPlanFollower::new(NoResolver)
        .preflight(request, authenticated)
        .map(|_| ())
}

#[cfg(test)]
pub(crate) mod tests {
    //! Authenticated preflight and role-local provider behavior proofs.
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// Builds one admitted follower session grant for in-process tests.
    fn test_sessions(granted_memory_bytes: usize) -> FollowerSessionFactory {
        FollowerSessionFactory::for_grant(
            Arc::new(datafusion::execution::memory_pool::GreedyMemoryPool::new(
                granted_memory_bytes,
            )),
            granted_memory_bytes,
            1,
        )
    }

    use crate::scribe::memtable::Memtable;
    use crate::scribe::seal_key::SealKey;
    use crate::scribe::stream_identity::WriterEpoch;
    use crate::scribe::wal::ScribeAppendMeta;
    use arrow::array::StringArray;
    use arrow::datatypes::{DataType, Field, Schema};
    use arrow::record_batch::RecordBatch;
    use datafusion::datasource::listing::PartitionedFile;
    use datafusion::datasource::physical_plan::ParquetSource;
    use datafusion::execution::context::SessionContext;
    use datafusion::execution::object_store::ObjectStoreUrl;
    use datafusion::physical_plan::collect;
    use datafusion_proto::bytes::physical_plan_to_bytes_with_extension_codec;
    use wyrd_spec::vala::api::{
        PersistedFileAssignment, PersistedWalRange, ScribeProviderCut, SignedPeerTicket,
    };

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
        let exec = plan
            .as_any()
            .downcast_ref::<DataSourceExec>()
            .expect("file source");
        let config = exec
            .data_source()
            .as_any()
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
            )
            .is_err()
        );
    }

    /// Builds one otherwise-valid Scribe provider cut for focused range tests.
    fn cut(ranges: Vec<PersistedWalRange>) -> ScribeProviderCut {
        ScribeProviderCut {
            writer_epoch: 2,
            start_partition: crate::test_support::day_partition(2026, 8, 19).to_wire(),
            end_partition: crate::test_support::day_partition(2026, 8, 19).to_wire(),
            required_columns: vec!["wyrd_event_time".to_owned()],
            persisted_cursor: 7,
            persisted_ranges: ranges,
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
        async fn fetch(&self, request: FetchLiveTailRequest) -> Result<Vec<HotBatch>, String> {
            self.requests
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .push(request);
            Ok(self.batches.clone())
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
            _assignment: &FollowerScanAssignment,
            _session: &SessionState,
        ) -> Result<Arc<dyn ExecutionPlan>, String> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Ok(Arc::new(datafusion::physical_plan::empty::EmptyExec::new(
                Arc::clone(&self.schema),
            )))
        }
    }

    /// Creates one valid Oracle-role request and its exact authenticated facts.
    ///
    /// # Errors
    /// Returns a plan error if the extension leaf cannot be serialized.
    fn oracle_request()
    -> datafusion::common::Result<(PhysicalExecuteFragmentRequest, TenantTableBinding)> {
        let tenant_id = DataTenantId::new_v7();
        let binding = TenantTableBinding {
            tenant_id,
            namespace: "vala.logs".to_owned(),
            table: "records".to_owned(),
        };
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
            role: ClusterRole::Oracle,
            fencing_token: 12,
        };
        Ok((
            PhysicalExecuteFragmentRequest {
                ticket: SignedPeerTicket {
                    key_id: "test".to_owned(),
                    claims_bytes: vec![1],
                    signature: vec![2; 64],
                },
                physical_plan_bytes: bytes.clone(),
                reservation_id: ReservationId::new(uuid::Uuid::now_v7()),
                leader_fence,
                target_fence,
                assignments: vec![FollowerScanAssignment {
                    scan_id: "scan".to_owned(),
                    binding: binding.clone(),
                    persisted: PersistedFileAssignment {
                        files: vec!["s3://bucket/file.parquet".to_owned()],
                    },
                    scribe_provider_cut: None,
                    schema_fingerprint: fingerprint,
                    required_columns: vec!["data_tenant_id".to_owned()],
                    predicates: Vec::new(),
                }],
                plan_fingerprint: physical_plan_fingerprint(&bytes),
            },
            binding,
        ))
    }

    /// Builds the authenticated context tied exactly to a request.
    fn authenticated<'a>(
        request: &'a PhysicalExecuteFragmentRequest,
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

    /// The resolver boundary preserves caller-projected WAL range order.
    ///
    /// # Panics
    /// Panics if canonical fixture construction, provider resolution, or the
    /// recording seam violates its test invariant.
    #[tokio::test]
    async fn scribe_provider_preserves_ordered_ranges_in_one_tail_call() {
        let tenant_id = DataTenantId::new_v7();
        let requests = Arc::new(std::sync::Mutex::new(Vec::new()));
        let stream = StreamIdentity::new(
            crate::scribe::stream_identity::NodeId::new(uuid::Uuid::now_v7()),
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
        let resolver = ScribeTailResolver::with_schema(tail, Arc::clone(&schema));
        let ranges = vec![
            PersistedWalRange {
                start_lsn: 8,
                end_lsn: 9,
            },
            PersistedWalRange {
                start_lsn: 12,
                end_lsn: 13,
            },
        ];
        let session = SessionContext::new().state();
        let binding = TenantTableBinding {
            tenant_id,
            namespace: "vala.logs".to_owned(),
            table: "records".to_owned(),
        };
        resolver
            .resolve(
                ClusterRole::Scribe,
                &FollowerScanAssignment {
                    scan_id: local_scribe_scan_id(&binding, stream),
                    binding,
                    persisted: PersistedFileAssignment { files: Vec::new() },
                    scribe_provider_cut: Some(cut(ranges)),
                    schema_fingerprint: super::super::assignment_schema_fingerprint(
                        schema.as_ref(),
                    ),
                    required_columns: vec!["data_tenant_id".to_owned()],
                    predicates: Vec::new(),
                },
                &session,
            )
            .await
            .expect("Scribe provider resolves");
        let requests = requests
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].after_lsn.as_u64(), 7);
        assert_eq!(
            requests[0]
                .persisted_lsn_ranges
                .iter()
                .map(|(start, end)| (start.as_u64(), end.as_u64()))
                .collect::<Vec<_>>(),
            vec![(8, 9), (12, 13)]
        );
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
        let schema = Arc::new(Schema::new(vec![Field::new(
            "wyrd_event_time",
            DataType::Utf8,
            true,
        )]));
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
            scribe_provider_cut: Some(cut(Vec::new())),
            schema_fingerprint: "schema".to_owned(),
            required_columns: vec!["data_tenant_id".to_owned()],
            predicates: Vec::new(),
        };
        let session = SessionContext::new().state();
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
                &test_sessions(1024 * 1024),
            )
            .await
            .expect("validated provider decodes and executes");
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert_eq!(follower.preflight_count(), 2);
        assert_eq!(stream.split().0.schema().fields().len(), 1);
    }

    /// Every follower session enables Parquet-level predicate and index
    /// pushdown, so a closed leaf predicate actually prunes row groups and
    /// pages at the reader rather than only being re-applied by the
    /// residual `FilterExec` `DataFusion` keeps above the provider.
    #[tokio::test]
    async fn oracle_reader_session_options_contract() {
        let (state, _context) = test_sessions(1024)
            .create(1)
            .expect("governed follower session context");
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
        let sessions = test_sessions(1024);
        let (first_state, first_context) = sessions
            .create(2)
            .expect("first governed request context");
        let (second_state, second_context) = sessions
            .create(2)
            .expect("second governed request context");
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
        request: &PhysicalExecuteFragmentRequest,
        first: &PhysicalPlanNode,
    ) -> Vec<PhysicalExecuteFragmentRequest> {
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
        case.assignments[0].scribe_provider_cut = Some(cut(Vec::new()));
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
            RecordBatch::try_new(
                Arc::clone(&schema),
                vec![Arc::new(StringArray::from(vec![value]))],
            )
            .expect("valid cohort batch")
        };
        let table = TableRef::parse_fqn("vala.logs.records").expect("canonical table");
        let key = SealKey::new(tenant_id, table, day);
        let memtable = Arc::new(Memtable::new());
        let event = || wyrd_spec::vala::api::AuditEvent {
            request_id: wyrd_spec::request_id::RequestId::now_v7(),
            trace_id: None,
            operation: "test".to_owned(),
            resource: "vala.logs.records".to_owned(),
            card_ref: None,
            principal_id: wyrd_spec::auth::PrincipalId::new(uuid::Uuid::now_v7()),
            principal_kind: wyrd_spec::auth::PrincipalKindTag::User,
            auth_method: wyrd_spec::vala::api::AuthMethod::Jwt,
            permission: "test".to_owned(),
            decision: wyrd_spec::vala::api::AuditDecision::Allow,
            result: wyrd_spec::vala::api::AuditResult::Success,
            payload_summary: "test".to_owned(),
            detail: None,
        };
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
            .insert(&key, event(), meta(7), batch("immutable-one"))
            .expect("first immutable row inserts");
        memtable
            .freeze(&key)
            .expect("first immutable generation freezes");
        memtable
            .insert(&key, event(), meta(8), batch("immutable-two"))
            .expect("second immutable row inserts");
        memtable
            .freeze(&key)
            .expect("second immutable generation freezes");
        memtable
            .insert(&key, event(), meta(9), batch("active"))
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
        let role_resources = crate::resources::BifrostRuntimeResources::composed_for_test(
            crate::resources::MIN_UNMANAGED_RESERVE_BYTES
                + crate::resources::ROLE_MEMORY_FLOOR_BYTES,
            crate::resources::MIN_SCRATCH_FREE_BYTES,
            [crate::resources::BifrostRole::Scribe],
        );
        let stream = StreamIdentity::new(
            crate::scribe::stream_identity::NodeId::new(uuid::Uuid::now_v7()),
            WriterEpoch::new(2),
        );
        let service = Arc::new(FetchLiveTailService::new(
            stream,
            memtable,
            role_resources.scribe().expect("Scribe capability"),
        ));
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
                    scribe_provider_cut: Some(cut(Vec::new())),
                    schema_fingerprint: super::super::assignment_schema_fingerprint(
                        schema.as_ref(),
                    ),
                    required_columns: vec!["data_tenant_id".to_owned()],
                    predicates: Vec::new(),
                },
                &session,
            )
            .await
            .expect("snapshot cohort resolves");
        let rows = collect(provider, Arc::new(TaskContext::default()))
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

    /// Every authenticated request-component contradiction fails before resolution.
    ///
    /// # Panics
    /// Panics if the canonical physical-plan fixtures cannot be encoded or decoded.
    pub(crate) async fn assert_complete_preflight_matrix() {
        let (request, binding) = oracle_request().expect("valid Oracle request");
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
