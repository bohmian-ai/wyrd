//! Three-stage authenticated physical-plan follower lifecycle.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use arrow::datatypes::SchemaRef;
use async_trait::async_trait;
use datafusion::common::tree_node::{Transformed, TreeNode};
use datafusion::datasource::TableProvider;
use datafusion::datasource::physical_plan::{FileGroup, FileScanConfig, FileScanConfigBuilder};
use datafusion::datasource::source::DataSourceExec;
use datafusion::execution::{TaskContext, context::SessionState};
use datafusion::physical_plan::{ExecutionPlan, SendableRecordBatchStream};
use datafusion_proto::bytes::physical_plan_from_bytes_with_extension_codec;
use datafusion_proto::protobuf::{PhysicalPlanNode, physical_plan_node::PhysicalPlanType};
use prost::Message;
use thiserror::Error;
use wyrd_spec::DataTenantId;
use wyrd_spec::vala::api::{
    ClusterRole, FollowerScanAssignment, OracleRoleFence, PhysicalExecuteFragmentRequest,
    ReservationId, TenantTableBinding,
};

use super::codec::{OraclePhysicalExtensionCodec, physical_plan_fingerprint};
use crate::catalog::{BifrostCatalog, TableRef};
use crate::scribe::seal_key::EventDay;
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
    ) -> Result<Arc<dyn ExecutionPlan>, String>;
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
    /// Session state used only to build the physical scan provider.
    session: SessionState,
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
            .field("session", &"configured")
            .finish()
    }
}

impl OracleCatalogResolver {
    /// Creates an Oracle resolver from already-authenticated process capabilities.
    #[must_use]
    pub fn new(catalog: Arc<BifrostCatalog>, session: SessionState) -> Self {
        Self { catalog, session }
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
        let plan = provider
            .scan(&self.session, None, &[], None)
            .await
            .map_err(|_| "authenticated Oracle physical scan failed".to_owned())?;
        restrict_plan_to_assigned_files(plan, &assignment.persisted.files)
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
#[derive(Debug)]
pub struct ScribeTailResolver<T = FetchLiveTailService> {
    /// Existing Scribe snapshot service; this owner calls it once per assignment.
    tail: Arc<T>,
    /// Authenticated schema expected for the projected Arrow cohort.
    schema: SchemaRef,
}

impl ScribeTailResolver<FetchLiveTailService> {
    /// Creates a Scribe resolver from the local tail capability and authenticated schema.
    #[must_use]
    pub fn new(tail: Arc<FetchLiveTailService>, schema: SchemaRef) -> Self {
        Self { tail, schema }
    }
}

#[async_trait]
impl<T> FollowerSourceResolver for ScribeTailResolver<T>
where
    T: LiveTailSource,
{
    /// Fetches one ordered active-plus-unretired-immutable cohort and builds its provider.
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
        let start_day = parse_event_day(&cut.start_event_day)?;
        let end_day = parse_event_day(&cut.end_event_day)?;
        let max_batches = usize::try_from(cut.maximum_batch_count)
            .map_err(|_| "Scribe batch bound does not fit this process".to_owned())?;
        let max_retained_bytes = usize::try_from(cut.maximum_retained_bytes)
            .map_err(|_| "Scribe byte bound does not fit this process".to_owned())?;
        let batches = self
            .tail
            .fetch(FetchLiveTailRequest {
                binding,
                target_stream: stream,
                start_day,
                end_day,
                after_lsn: checked_wal_lsn(cut.persisted_cursor)?,
                persisted_lsn_ranges,
                required_columns: cut.required_columns.clone(),
                max_batches,
                max_retained_bytes,
            })
            .await?;
        let rows = batches
            .into_iter()
            .map(|batch| batch.rows)
            .collect::<Vec<_>>();
        super::exec::OracleTableProvider::validated_memory_source(&rows, Arc::clone(&self.schema))
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
    assigned_files: &[String],
) -> Result<Arc<dyn ExecutionPlan>, String> {
    let assigned = assigned_files
        .iter()
        .cloned()
        .collect::<std::collections::BTreeSet<_>>();
    if assigned.len() != assigned_files.len() || assigned.iter().any(|file| file.trim().is_empty())
    {
        return Err("authenticated Oracle assignment contains duplicate or empty files".to_owned());
    }
    let mut observed = std::collections::BTreeSet::new();
    let mut file_leaves = 0usize;
    let transformed = plan
        .transform_up(|node| {
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
                            let location = file.object_meta.location.to_string();
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

/// Parses one canonical UTC event-day endpoint.
///
/// # Errors
/// Returns an error when the endpoint is not an ISO calendar date.
fn parse_event_day(value: &str) -> Result<EventDay, String> {
    chrono::NaiveDate::parse_from_str(value, "%Y-%m-%d")
        .map(EventDay::new)
        .map_err(|_| "Scribe event-day endpoint is invalid".to_owned())
}

/// Concrete owner of preflight, provider resolution, and `DataFusion` decode.
#[derive(Debug)]
pub struct PhysicalPlanFollower<R> {
    /// Authenticated role-local provider constructor.
    resolver: R,
    /// Maximum accepted protobuf size.
    maximum_plan_bytes: usize,
    /// Observable lifecycle-boundary effects used to prove fail-closed ordering.
    effects: FollowerEffects,
}

/// Observable counts for the follower's effect-bearing lifecycle boundaries.
#[derive(Debug, Default)]
struct FollowerEffects {
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
    pub const fn new(resolver: R) -> Self {
        Self {
            resolver,
            maximum_plan_bytes: DEFAULT_MAX_PHYSICAL_PLAN_BYTES,
            effects: FollowerEffects {
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
    /// Returns [`PhysicalPlanFollowerError`] for preflight, provider resolution,
    /// or post-resolution native `DataFusion` decode failure.
    pub async fn decode(
        &self,
        request: &PhysicalExecuteFragmentRequest,
        authenticated: AuthenticatedFollowerContext<'_>,
        context: &TaskContext,
    ) -> Result<Arc<dyn ExecutionPlan>, PhysicalPlanFollowerError> {
        let preflight = self.preflight(request, &authenticated)?;
        let mut providers = HashMap::with_capacity(preflight.assignments.len());
        for (scan_id, assignment) in preflight.assignments {
            self.effects.resolver.fetch_add(1, Ordering::SeqCst);
            let provider = self
                .resolver
                .resolve(request.target_fence.role, &assignment)
                .await
                .map_err(PhysicalPlanFollowerError::Resolution)?;
            let actual = super::sealed_fragment_schema_fingerprint(provider.schema().as_ref());
            if actual != assignment.schema_fingerprint {
                return Err(PhysicalPlanFollowerError::Resolution(
                    "resolved provider schema fingerprint differs from assignment".to_owned(),
                ));
            }
            providers.insert(scan_id, provider);
        }
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
        request: &PhysicalExecuteFragmentRequest,
        authenticated: AuthenticatedFollowerContext<'_>,
        context: Arc<TaskContext>,
    ) -> Result<SendableRecordBatchStream, PhysicalPlanFollowerError> {
        let plan = self.decode(request, authenticated, &context).await?;
        self.effects.execution.fetch_add(1, Ordering::SeqCst);
        let stream = plan
            .execute(0, context)
            .map_err(|error| PhysicalPlanFollowerError::Execution(error.to_string()))?;
        self.effects.output.fetch_add(1, Ordering::SeqCst);
        Ok(stream)
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

    /// Performs the complete IO-free validation phase.
    ///
    /// # Errors
    /// Returns a preflight error for any malformed request, contradictory
    /// authenticated binding, unsupported plan node, or inconsistent scan set.
    fn preflight(
        &self,
        request: &PhysicalExecuteFragmentRequest,
        authenticated: &AuthenticatedFollowerContext<'_>,
    ) -> Result<PreflightRequest, PhysicalPlanFollowerError> {
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
            if !Self::valid_persisted_files(&assignment.persisted.files) {
                return Err(PhysicalPlanFollowerError::Preflight(
                    "persisted assignment is not unique and ordered".to_owned(),
                ));
            }
            match request.target_fence.role {
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
                && (cut.writer_epoch != request.target_fence.fencing_token
                    || cut.start_event_day.is_empty()
                    || cut.end_event_day.is_empty()
                    || cut.start_event_day > cut.end_event_day
                    || cut.required_columns.is_empty()
                    || cut.maximum_batch_count == 0
                    || cut.maximum_retained_bytes == 0
                    || !cut.is_valid())
            {
                return Err(PhysicalPlanFollowerError::Preflight(
                    "invalid Scribe provider cut".to_owned(),
                ));
            }
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
        encoded: &mut HashMap<String, String>,
    ) -> Result<(), PhysicalPlanFollowerError> {
        let children: Vec<&PhysicalPlanNode> = match node.physical_plan_type.as_ref() {
            Some(PhysicalPlanType::Extension(extension)) => {
                if !extension.inputs.is_empty() {
                    return Err(PhysicalPlanFollowerError::Preflight(
                        "remote scan extension has inputs".to_owned(),
                    ));
                }
                let payload = OraclePhysicalExtensionCodec::decode_payload(&extension.node)
                    .map_err(|error| PhysicalPlanFollowerError::Preflight(error.to_string()))?;
                if payload.scan_id.is_empty()
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
            Some(PhysicalPlanType::HashJoin(value)) => match (&value.left, &value.right) {
                (Some(left), Some(right)) => vec![left.as_ref(), right.as_ref()],
                _ => {
                    return Err(PhysicalPlanFollowerError::Preflight(
                        "binary join is missing a required input".to_owned(),
                    ));
                }
            },
            Some(PhysicalPlanType::NestedLoopJoin(value)) => match (&value.left, &value.right) {
                (Some(left), Some(right)) => vec![left.as_ref(), right.as_ref()],
                _ => {
                    return Err(PhysicalPlanFollowerError::Preflight(
                        "binary join is missing a required input".to_owned(),
                    ));
                }
            },
            Some(PhysicalPlanType::CrossJoin(value)) => match (&value.left, &value.right) {
                (Some(left), Some(right)) => vec![left.as_ref(), right.as_ref()],
                _ => {
                    return Err(PhysicalPlanFollowerError::Preflight(
                        "binary join is missing a required input".to_owned(),
                    ));
                }
            },
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
    request: &PhysicalExecuteFragmentRequest,
    authenticated: AuthenticatedFollowerContext<'_>,
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
        ) -> Result<Arc<dyn ExecutionPlan>, String> {
            Err("preflight resolver must not run".to_owned())
        }
    }
    PhysicalPlanFollower::new(NoResolver)
        .preflight(request, &authenticated)
        .map(|_| ())
}

#[cfg(test)]
pub(crate) mod tests {
    //! Authenticated preflight and role-local provider behavior proofs.
    use std::sync::atomic::{AtomicUsize, Ordering};

    use crate::scribe::memtable::Memtable;
    use crate::scribe::seal_key::SealKey;
    use crate::scribe::stream_identity::WriterEpoch;
    use crate::scribe::wal::ScribeAppendMeta;
    use arrow::array::StringArray;
    use arrow::datatypes::{DataType, Field, Schema};
    use arrow::record_batch::RecordBatch;
    use datafusion::datasource::listing::PartitionedFile;
    use datafusion::datasource::physical_plan::ParquetSource;
    use datafusion::execution::object_store::ObjectStoreUrl;
    use datafusion::physical_plan::collect;
    use datafusion_proto::bytes::physical_plan_to_bytes_with_extension_codec;
    use wyrd_spec::vala::api::{
        PersistedFileAssignment, PersistedWalRange, ScribeProviderCut, SignedPeerTicket,
    };

    use super::*;
    use crate::oracle::codec::RemoteScanExec;

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
            .flat_map(|group| group.iter())
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
        let restricted = restrict_plan_to_assigned_files(catalog, &["assigned.parquet".to_owned()])
            .expect("exact assigned file remains executable");
        assert_eq!(plan_files(&restricted), vec!["assigned.parquet"]);

        assert!(
            restrict_plan_to_assigned_files(
                file_plan(Arc::clone(&schema), &["assigned.parquet"]),
                &["missing.parquet".to_owned()],
            )
            .is_err()
        );
        assert!(
            restrict_plan_to_assigned_files(
                file_plan(Arc::clone(&schema), &["assigned.parquet"]),
                &["assigned.parquet".to_owned(), "extra.parquet".to_owned()],
            )
            .is_err()
        );
        assert!(
            restrict_plan_to_assigned_files(
                file_plan(schema, &["assigned.parquet"]),
                &["assigned.parquet".to_owned(), "assigned.parquet".to_owned()],
            )
            .is_err()
        );
    }

    /// Builds one otherwise-valid Scribe provider cut for focused range tests.
    fn cut(ranges: Vec<PersistedWalRange>) -> ScribeProviderCut {
        ScribeProviderCut {
            writer_epoch: 2,
            start_event_day: "2026-08-19".to_owned(),
            end_event_day: "2026-08-19".to_owned(),
            required_columns: vec!["wyrd_event_time".to_owned()],
            persisted_cursor: 7,
            persisted_ranges: ranges,
            maximum_batch_count: 8,
            maximum_retained_bytes: 1024,
        }
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
        let fingerprint = super::super::sealed_fragment_schema_fingerprint(schema.as_ref());
        let bytes = physical_plan_to_bytes_with_extension_codec(
            Arc::new(RemoteScanExec::new("scan", &fingerprint, schema)),
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
        let tail = Arc::new(RecordingTail {
            stream: StreamIdentity::new(
                crate::scribe::stream_identity::NodeId::new(uuid::Uuid::now_v7()),
                WriterEpoch::new(2),
            ),
            requests: Arc::clone(&requests),
            batches: Vec::new(),
        });
        let schema = Arc::new(Schema::new(vec![Field::new(
            "wyrd_event_time",
            DataType::Utf8,
            true,
        )]));
        let resolver = ScribeTailResolver {
            tail,
            schema: Arc::clone(&schema),
        };
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
        resolver
            .resolve(
                ClusterRole::Scribe,
                &FollowerScanAssignment {
                    scan_id: "scan".to_owned(),
                    binding: TenantTableBinding {
                        tenant_id,
                        namespace: "vala.logs".to_owned(),
                        table: "records".to_owned(),
                    },
                    persisted: PersistedFileAssignment { files: Vec::new() },
                    scribe_provider_cut: Some(cut(ranges)),
                    schema_fingerprint: super::super::sealed_fragment_schema_fingerprint(
                        schema.as_ref(),
                    ),
                },
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

    /// Oracle and Scribe assignments remain a closed role-local matrix.
    ///
    /// # Panics
    /// Panics if the canonical request cannot be built or the validated plan
    /// cannot decode and execute.
    #[tokio::test]
    async fn role_local_providers_are_isolated_and_consumed_once() {
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
                Arc::new(TaskContext::default()),
            )
            .await
            .expect("validated provider decodes and executes");
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert_eq!(stream.schema().fields().len(), 1);
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
        let day = EventDay::new(chrono::NaiveDate::from_ymd_opt(2026, 8, 19).expect("valid day"));
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
        let role_resources = crate::resources::BifrostRuntimeResources::composed_for_test(
            crate::resources::MIN_UNMANAGED_RESERVE_BYTES
                + crate::resources::ROLE_MEMORY_FLOOR_BYTES,
            crate::resources::MIN_SCRATCH_FREE_BYTES,
            [crate::resources::BifrostRole::Scribe],
        );
        let service = Arc::new(FetchLiveTailService::new(
            StreamIdentity::new(
                crate::scribe::stream_identity::NodeId::new(uuid::Uuid::now_v7()),
                WriterEpoch::new(2),
            ),
            memtable,
            role_resources.scribe().expect("Scribe capability"),
        ));
        let resolver = ScribeTailResolver {
            tail: service,
            schema: Arc::clone(&schema),
        };
        let provider = resolver
            .resolve(
                ClusterRole::Scribe,
                &FollowerScanAssignment {
                    scan_id: "scan".to_owned(),
                    binding: TenantTableBinding {
                        tenant_id,
                        namespace: "vala.logs".to_owned(),
                        table: "records".to_owned(),
                    },
                    persisted: PersistedFileAssignment { files: Vec::new() },
                    scribe_provider_cut: Some(cut(Vec::new())),
                    schema_fingerprint: super::super::sealed_fragment_schema_fingerprint(
                        schema.as_ref(),
                    ),
                },
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
            Arc::new(RemoteScanExec::new(
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
        for case in malformed {
            assert!(
                follower
                    .decode(
                        &case,
                        authenticated(&request, &binding),
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
