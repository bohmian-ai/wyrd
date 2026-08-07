//! Production-path qualification dataset materialization.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use arrow::array::{ArrayRef, Float64Array, Int64Array, StringArray, TimestampMicrosecondArray};
use arrow::datatypes::{DataType, Field, Schema, TimeUnit};
use arrow::ipc::writer::StreamWriter;
use arrow::record_batch::RecordBatch;
use async_trait::async_trait;
use bytes::Bytes;
use tokio_util::sync::CancellationToken;
use vala_bifrost_redux::catalog::{CreateTableRequest, TableRef};
use vala_bifrost_redux::namespaces::BifrostNamespace;
use vala_bifrost_redux::scribe::telemetry::ScribeInspectionSnapshot;
use vala_sdk::{BifrostFrame, BifrostGrpcTransport, CollectedQueryLimits, QueryClient};
use wyrd_bench::{BenchmarkMetricSnapshot, BenchmarkRecorder};
use wyrd_client::WyrdClient;
use wyrd_client::config::ClientConfig;
use wyrd_client::transport::{GrpcConfig, HttpConfig};
use wyrd_spec::DataTenantId;
use wyrd_spec::error::WyrdError;
use wyrd_spec::vala::api::{BifrostQueryRequest, FreshnessPolicy, VisibilityMode};
use wyrd_spec::vala::error::BifrostError;
use wyrd_spec::vala::managed_columns::WYRD_EVENT_TIME;

use crate::bifrost::bench_dataset::BifrostQualificationDataset;
use crate::server::WyrdTestServer;

/// Logical table populated by the qualification materializer.
pub const QUALIFICATION_TABLE: &str = "bifrost_qualification_telemetry";
/// Number of rows in every complete ingest batch.
pub const MATERIALIZER_BATCH_ROWS: u64 = 4_096;

/// Bounded retry policy for setup and visibility operations.
#[derive(Debug, Clone, Copy)]
pub struct BackpressurePolicy {
    /// Initial delay after a retryable capacity rejection.
    pub initial_backoff: Duration,
    /// Maximum delay between retries.
    pub max_backoff: Duration,
    /// Absolute deadline for the complete setup operation.
    pub setup_deadline: Instant,
}

impl BackpressurePolicy {
    /// Construct the D71 policy with a caller-owned absolute deadline.
    #[must_use]
    pub fn with_deadline(setup_deadline: Instant) -> Self {
        Self {
            initial_backoff: Duration::from_millis(50),
            max_backoff: Duration::from_secs(5),
            setup_deadline,
        }
    }
}

/// Closed set of D84 ceiling-labelled rejection reasons.
///
/// These are the exact `reason` label values the write path attaches to the
/// `bifrost_scribe_rejections_total` counter when a memory ceiling trips
/// (see `vala_bifrost_redux::scribe::ScribeRejectionCeiling::as_metric_label`).
/// The bench harness consumes them by name to name the binding ceiling of a
/// failing capacity run; it never defines or relabels them (T40 owns emission).
pub const CEILING_REJECTION_REASONS: [&str; 5] = [
    "cgroup_breaker",
    "scribe_child",
    "ingress_sublimit",
    "bifrost_parent",
    "cgroup_parent",
];

/// Governor memory high-water captured from a server inspection snapshot.
///
/// Records the parent Bifrost and Scribe-child occupancy against their ceilings
/// plus retained WAL bytes at the instant a capacity run fails its deadline (or
/// completes). When no ceiling-labelled rejection was observed, the ceiling
/// closest to its limit here is what names the run's binding ceiling (D84).
/// Additive server-side evidence only; it carries no tenant, table, or request
/// identity.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct GovernorHighWater {
    /// Parent Bifrost bytes reserved across all roles.
    pub parent_used_memory: usize,
    /// Parent Bifrost memory ceiling.
    pub parent_memory_limit: usize,
    /// Scribe-child bytes reserved.
    pub scribe_used_memory: usize,
    /// Scribe-child memory ceiling.
    pub scribe_memory_limit: usize,
    /// WAL bytes retained on disk.
    pub wal_disk_bytes: u64,
}

impl GovernorHighWater {
    /// Read the governor high-water values from a server inspection snapshot.
    #[must_use]
    pub fn from_snapshot(snapshot: &ScribeInspectionSnapshot) -> Self {
        Self {
            parent_used_memory: snapshot.parent_used_memory,
            parent_memory_limit: snapshot.parent_memory_limit,
            scribe_used_memory: snapshot.scribe_used_memory,
            scribe_memory_limit: snapshot.scribe_memory_limit,
            wal_disk_bytes: snapshot.wal_disk_bytes,
        }
    }

    /// Return the ceiling label whose occupancy is closest to its own limit.
    ///
    /// Compares the Scribe-child and parent-Bifrost occupancy fractions and
    /// returns the ceiling label of the larger fraction, ignoring a ceiling
    /// whose limit is zero (unconfigured). Returns `None` when neither ceiling
    /// has a positive limit. This is the fallback that names the binding
    /// ceiling when no ceiling-labelled rejection was recorded (D84).
    #[must_use]
    fn nearest_ceiling(&self) -> Option<&'static str> {
        let mut nearest: Option<(&'static str, f64)> = None;
        for (label, used, limit) in [
            (
                "scribe_child",
                self.scribe_used_memory,
                self.scribe_memory_limit,
            ),
            (
                "bifrost_parent",
                self.parent_used_memory,
                self.parent_memory_limit,
            ),
        ] {
            if limit == 0 {
                continue;
            }
            let fraction = used as f64 / limit as f64;
            if nearest.is_none_or(|(_, best)| fraction > best) {
                nearest = Some((label, fraction));
            }
        }
        nearest.map(|(label, _)| label)
    }
}

/// Extract the ceiling-labelled rejection counts from a benchmark metric snapshot.
///
/// Reads the `bifrost_scribe_rejections_total{reason="<ceiling>"}` series for
/// each of the five [`CEILING_REJECTION_REASONS`], keyed by the closed ceiling
/// label, and retains only ceilings that actually rejected work (count `> 0`).
/// Returns an empty map when the recorder observed no ceiling-labelled
/// rejection, which is the AC2 default that leaves `binding_ceilings` empty.
#[must_use]
pub fn ceiling_rejection_counts(snapshot: &BenchmarkMetricSnapshot) -> BTreeMap<String, u64> {
    let mut counts = BTreeMap::new();
    for reason in CEILING_REJECTION_REASONS {
        let key = format!("bifrost_scribe_rejections_total{{reason=\"{reason}\"}}");
        if let Some(count) = snapshot.counters.get(&key).copied()
            && count > 0
        {
            counts.insert(reason.to_owned(), count);
        }
    }
    counts
}

/// Derive the binding ceiling name from captured ceiling rejections and governor high-water.
///
/// The binding ceiling is the ceiling-labelled rejection with the largest count
/// in `binding_ceilings`; ties resolve to the lexicographically smallest ceiling
/// label so the choice is deterministic. When `binding_ceilings` is empty, the
/// binding ceiling falls back to the governor high-water ceiling closest to its
/// limit. Returns `None` only when neither source is populated (D84).
#[must_use]
pub fn derive_binding_ceiling(
    binding_ceilings: &BTreeMap<String, u64>,
    governor: Option<&GovernorHighWater>,
) -> Option<String> {
    if let Some((ceiling, _)) = binding_ceilings
        .iter()
        .max_by(|left, right| left.1.cmp(right.1).then_with(|| right.0.cmp(left.0)))
    {
        return Some(ceiling.clone());
    }
    governor
        .and_then(GovernorHighWater::nearest_ceiling)
        .map(str::to_owned)
}

/// Admission pressure observed while materializing a dataset.
///
/// The client-side counters `rejections` and `waited` are the unchanged D71
/// contract. `binding_ceilings` and `governor_high_water` are the additive D84
/// server-side evidence, populated from captured harness telemetry at the
/// failure/completion instant and left empty/`None` when no telemetry source
/// was injected.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PressureEvidence {
    /// Number of retryable capacity responses.
    pub rejections: u64,
    /// Total time spent waiting, including a final partial wait truncated by
    /// the absolute deadline.
    pub waited: Duration,
    /// Which binding ceilings rejected work and how many times, keyed by the
    /// ceiling-labelled T40 rejection `reason`. Empty when no ceiling-labelled
    /// rejection was observed. (D84, additive)
    pub binding_ceilings: BTreeMap<String, u64>,
    /// Governor high-water at the failure/completion instant, from the server
    /// inspection snapshot. `None` when no snapshot was taken. (D84, additive)
    pub governor_high_water: Option<GovernorHighWater>,
}

impl PressureEvidence {
    /// Name the binding ceiling from this run's own captured evidence.
    ///
    /// Delegates to [`derive_binding_ceiling`]: the max-count ceiling-labelled
    /// rejection, or the governor high-water ceiling nearest its limit when no
    /// ceiling-labelled rejection was recorded. `None` when no server-side
    /// evidence was captured.
    #[must_use]
    pub fn binding_ceiling(&self) -> Option<String> {
        derive_binding_ceiling(&self.binding_ceilings, self.governor_high_water.as_ref())
    }
}

/// Progress retained when setup stops before the complete dataset is visible.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PartialProgress {
    /// Rows acknowledged by Gate.
    pub accepted_rows: u64,
    /// Batches acknowledged by Gate.
    pub accepted_batches: u64,
    /// Number of complete tenant/day partitions confirmed visible.
    pub visible_partitions: u64,
}

/// One tenant/day visibility confirmation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MaterializedPartition {
    /// Tenant whose rows were counted.
    pub tenant: DataTenantId,
    /// Zero-based dataset day.
    pub day: u32,
    /// Exact count returned by public Oracle.
    pub rows: u64,
}

/// Successful materialization result and measured production-path telemetry.
#[derive(Debug, Clone)]
pub struct MaterializedDataset {
    /// Confirmed tenant/day layout.
    pub layout: Vec<MaterializedPartition>,
    /// Wall-clock duration from first setup operation to final visibility.
    pub duration: Duration,
    /// Acknowledged rows divided by wall-clock seconds.
    pub rows_per_second: f64,
    /// Encoded Arrow bytes divided by wall-clock seconds.
    pub bytes_per_second: f64,
    /// Admission pressure recorded during setup.
    pub pressure: PressureEvidence,
}

/// Typed materializer failures; cleanup is attempted before each failure is returned.
#[derive(Debug, thiserror::Error)]
pub enum MaterializationError {
    /// Setup deadline expired while retrying admission or polling visibility.
    #[error("dataset materialization setup deadline exceeded")]
    SetupDeadlineExceeded {
        /// Pressure collected before the deadline.
        pressure: PressureEvidence,
        /// Accepted and visible work before the deadline.
        progress: PartialProgress,
    },
    /// A table binding already existed before this run.
    #[error("qualification table already exists for tenant {tenant}")]
    TableAlreadyExists {
        /// Tenant with the conflicting binding.
        tenant: DataTenantId,
    },
    /// A public ingest request failed without a retryable capacity code.
    #[error("qualification ingest failed: {source}")]
    Ingest {
        /// Public error returned by Gate/Scribe.
        #[source]
        source: WyrdError,
    },
    /// Public Oracle did not expose the expected partition count by deadline.
    #[error(
        "qualification visibility failed for tenant {tenant}, day {day}: expected {expected}, observed {observed}"
    )]
    Visibility {
        /// Tenant whose count was short.
        tenant: DataTenantId,
        /// Dataset day whose count was short.
        day: u32,
        /// Expected logical row count.
        expected: u64,
        /// Last public count observed.
        observed: u64,
    },
    /// Caller cancellation stopped setup after accepted work drained.
    #[error("dataset materialization cancelled")]
    Cancelled {
        /// Accepted and visible work before cancellation.
        progress: PartialProgress,
    },
    /// A typed materialization failure whose owned run directory also failed
    /// to clean up.
    #[error("materializer cleanup failed for {run_dir}: {cleanup_error}; cause: {cause}")]
    Cleanup {
        /// Run directory owned by the failed materialization.
        run_dir: PathBuf,
        /// Original typed materialization failure.
        cause: Box<MaterializationError>,
        /// Filesystem cleanup failure.
        cleanup_error: String,
    },
    /// Harness setup, query, or cleanup failed outside the public ingest error catalog.
    #[error("materializer backend failed: {0}")]
    Backend(String),
}

/// Private public-surface adapter used by the orchestration owner and tests.
#[async_trait]
pub(crate) trait MaterializerBackend: Send + Sync {
    /// Register a tenant-owned table binding.
    async fn provision_table(&self, tenant: DataTenantId) -> Result<(), String>;
    /// Send one immutable Arrow batch through Gate.
    async fn ingest(
        &self,
        tenant: DataTenantId,
        batch_id: [u8; 16],
        payload: Bytes,
    ) -> Result<(), WyrdError>;
    /// Trigger the documented tenant flush surface.
    async fn flush(&self, tenant: DataTenantId) -> Result<(), String>;
    /// Return the public Oracle count for one logical day range.
    async fn visible_count(
        &self,
        tenant: DataTenantId,
        day: u32,
        rows_per_day: u64,
    ) -> Result<u64, String>;
}

/// One cohesive owner for deterministic qualification materialization.
pub struct BifrostDatasetMaterializer<'a> {
    /// Public client retained by the owner for the production path identity.
    client: Option<WyrdClient>,
    /// Backend containing the harness provisioning/flush and public clients.
    backend: Arc<dyn MaterializerBackend + 'a>,
    /// Deterministic rows and partition shape.
    dataset: BifrostQualificationDataset,
    /// Tenant order used for stable batch identities.
    tenants: Vec<DataTenantId>,
    /// D71 retry and absolute setup deadline.
    policy: BackpressurePolicy,
    /// Parent under which this run owns one temporary directory.
    run_root: PathBuf,
    /// Cancellation observed between public operations.
    cancellation: CancellationToken,
    /// Live server borrowed for pre-failure governor high-water capture.
    ///
    /// Set by [`Self::from_server`]; `None` under [`Self::with_backend`]. Read
    /// through `scribe_inspection_snapshot` at the failure/completion instant to
    /// populate [`PressureEvidence::governor_high_water`].
    server: Option<&'a WyrdTestServer>,
    /// Process recorder installed by the capacity entrypoint, if injected.
    ///
    /// Set by [`Self::with_recorder`]. Read at the failure/completion instant to
    /// populate [`PressureEvidence::binding_ceilings`] with the ceiling-labelled
    /// rejection counts. `None` leaves `binding_ceilings` empty (AC2).
    recorder: Option<Arc<BenchmarkRecorder>>,
}

impl<'a> BifrostDatasetMaterializer<'a> {
    /// Construct a materializer over an injected backend seam.
    #[cfg(test)]
    #[must_use]
    pub(crate) fn with_backend(
        backend: Arc<dyn MaterializerBackend + 'a>,
        dataset: BifrostQualificationDataset,
        tenants: Vec<DataTenantId>,
        policy: BackpressurePolicy,
        run_root: impl Into<PathBuf>,
        cancellation: CancellationToken,
    ) -> Self {
        Self {
            client: None,
            backend,
            dataset,
            tenants,
            policy,
            run_root: run_root.into(),
            cancellation,
            server: None,
            recorder: None,
        }
    }

    /// Attach the run's installed recorder so failure evidence can name
    /// ceiling-labelled rejection counts.
    ///
    /// The recorder is the process-global handle installed by the capacity
    /// entrypoint (this method never installs a second one). When absent,
    /// [`PressureEvidence::binding_ceilings`] stays empty (AC2).
    #[must_use]
    pub fn with_recorder(mut self, recorder: Arc<BenchmarkRecorder>) -> Self {
        self.recorder = Some(recorder);
        self
    }

    /// Construct a real materializer from a bound test server and clients.
    ///
    /// # Errors
    /// Returns an error when no tenant client can be assembled from the server
    /// bootstrap credentials or a public transport cannot connect.
    pub async fn from_server(
        server: &'a WyrdTestServer,
        dataset: BifrostQualificationDataset,
        tenants: Vec<DataTenantId>,
        policy: BackpressurePolicy,
        run_root: impl Into<PathBuf>,
        cancellation: CancellationToken,
    ) -> Result<Self, MaterializationError> {
        let mut clients = Vec::with_capacity(tenants.len());
        let mut transports = Vec::with_capacity(tenants.len());
        let mut queries = Vec::with_capacity(tenants.len());
        for (index, tenant) in tenants.iter().copied().enumerate() {
            let bootstrap = server
                .bootstrap_service_in_tenant(tenant, &format!("materializer-{index}"), &["admin"])
                .await
                .map_err(|error| MaterializationError::Backend(error.to_string()))?;
            let api_key = match bootstrap {
                crate::Bootstrap::Machine { api_key, .. } => api_key,
                crate::Bootstrap::User { .. } => {
                    return Err(MaterializationError::Backend(
                        "materializer bootstrap returned a user".to_owned(),
                    ));
                }
            };
            let client = WyrdClient::with_config(ClientConfig {
                grpc: GrpcConfig {
                    endpoint: server.grpc_url().ok_or_else(|| {
                        MaterializationError::Backend("server has no gRPC URL".to_owned())
                    })?,
                    connect_retries: 0,
                    ..GrpcConfig::default()
                },
                http: HttpConfig {
                    base_url: server
                        .base_url()
                        .ok_or_else(|| {
                            MaterializationError::Backend("server has no HTTP URL".to_owned())
                        })?
                        .to_owned(),
                    ..HttpConfig::default()
                },
                api_key: Some(api_key),
                ..ClientConfig::default()
            })
            .map_err(|error| MaterializationError::Backend(error.to_string()))?;
            transports.push(
                BifrostGrpcTransport::connect(&client)
                    .await
                    .map_err(|error| MaterializationError::Backend(error.to_string()))?,
            );
            queries.push(QueryClient::new(&client));
            clients.push(client);
        }
        let backend = Arc::new(RealMaterializerBackend {
            server,
            tenants: tenants.clone(),
            transports,
            queries,
        });
        Ok(Self {
            client: clients.into_iter().next(),
            backend,
            dataset,
            tenants,
            policy,
            run_root: run_root.into(),
            cancellation,
            server: Some(server),
            recorder: None,
        })
    }

    /// Populate the additive server-side pressure evidence from injected telemetry.
    ///
    /// Reads the ceiling-labelled rejection counts from the injected recorder and
    /// the governor high-water from the injected server snapshot, writing both
    /// into `pressure`. Each source is optional: an absent recorder leaves
    /// `binding_ceilings` empty and an absent server (or a snapshot read error)
    /// leaves `governor_high_water` `None` (AC2). Synchronous and infallible;
    /// a snapshot error is treated as an absent snapshot and never panics.
    fn capture_server_pressure(&self, pressure: &mut PressureEvidence) {
        if let Some(recorder) = self.recorder.as_ref() {
            pressure.binding_ceilings = ceiling_rejection_counts(&recorder.snapshot());
        }
        if let Some(server) = self.server {
            pressure.governor_high_water = server
                .scribe_inspection_snapshot()
                .ok()
                .map(|snapshot| GovernorHighWater::from_snapshot(&snapshot));
        }
    }

    /// Materialize every tenant/day, retrying only public capacity responses.
    ///
    /// # Errors
    /// Returns a typed setup, ingest, visibility, cancellation, or backend
    /// error. Failed and cancelled runs remove the owned run directory.
    pub async fn materialize(&self) -> Result<MaterializedDataset, MaterializationError> {
        let _ = self.client.as_ref();
        let started = Instant::now();
        let run_dir = self.create_run_dir()?;
        let mut progress = PartialProgress::default();
        let mut pressure = PressureEvidence::default();
        let mut layout = Vec::new();
        let mut encoded_bytes = 0_u64;
        let shape = self.dataset.shape();

        let result = async {
            for tenant in &self.tenants {
                self.backend
                    .provision_table(*tenant)
                    .await
                    .map_err(|error| {
                        if error.to_ascii_lowercase().contains("already")
                            || error.to_ascii_lowercase().contains("exist")
                        {
                            MaterializationError::TableAlreadyExists { tenant: *tenant }
                        } else {
                            MaterializationError::Backend(error)
                        }
                    })?;
                if self.cancellation.is_cancelled() {
                    return Err(MaterializationError::Cancelled { progress });
                }
            }
            for (tenant_index, tenant) in self.tenants.iter().copied().enumerate() {
                for day in 0..shape.days {
                    let start_row = u64::from(day).saturating_mul(shape.rows_per_day);
                    let end_row = start_row.saturating_add(shape.rows_per_day);
                    let mut row = start_row;
                    while row < end_row {
                        if self.cancellation.is_cancelled() {
                            return Err(MaterializationError::Cancelled { progress });
                        }
                        let count = (end_row - row).min(MATERIALIZER_BATCH_ROWS);
                        let batch = self.dataset_batch(row, count)?;
                        encoded_bytes = encoded_bytes.saturating_add(batch.payload.len() as u64);
                        self.send_with_backoff(
                            BatchRequest {
                                tenant,
                                tenant_index,
                                row,
                                count,
                            },
                            batch.payload,
                            &mut pressure,
                            &mut progress,
                        )
                        .await?;
                        row = row.saturating_add(count);
                    }
                    self.backend
                        .flush(tenant)
                        .await
                        .map_err(MaterializationError::Backend)?;
                    if self.cancellation.is_cancelled() {
                        return Err(MaterializationError::Cancelled { progress });
                    }
                    let expected = shape.rows_per_day;
                    let observed = self
                        .wait_for_visibility(
                            tenant,
                            day,
                            expected,
                            shape.rows_per_day,
                            &mut progress,
                        )
                        .await?;
                    progress.visible_partitions = progress.visible_partitions.saturating_add(1);
                    layout.push(MaterializedPartition {
                        tenant,
                        day,
                        rows: observed,
                    });
                }
            }
            Ok(())
        }
        .await;

        match result {
            Ok(()) => {
                self.capture_server_pressure(&mut pressure);
                let elapsed = started.elapsed();
                let seconds = elapsed.as_secs_f64().max(f64::MIN_POSITIVE);
                Ok(MaterializedDataset {
                    layout,
                    duration: elapsed,
                    rows_per_second: progress.accepted_rows as f64 / seconds,
                    bytes_per_second: encoded_bytes as f64 / seconds,
                    pressure,
                })
            }
            Err(MaterializationError::Cancelled { .. }) => {
                Err(self.cleanup_failure(&run_dir, MaterializationError::Cancelled { progress }))
            }
            Err(error) => Err(self.cleanup_failure(&run_dir, error)),
        }
    }

    /// Encode one deterministic batch without touching the async boundary.
    fn dataset_batch(
        &self,
        first_row: u64,
        count: u64,
    ) -> Result<EncodedBatch, MaterializationError> {
        let rows = (0..count)
            .map(|offset| self.dataset.row((first_row + offset) as i64))
            .collect::<Vec<_>>();
        let schema = Arc::new(Schema::new(vec![
            Field::new("row_id", DataType::Int64, false),
            Field::new("device_id", DataType::Int64, false),
            Field::new("metric", DataType::Utf8, false),
            Field::new("value", DataType::Float64, false),
            Field::new("payload", DataType::Utf8, false),
            Field::new(
                WYRD_EVENT_TIME,
                DataType::Timestamp(TimeUnit::Microsecond, Some("UTC".into())),
                false,
            ),
        ]));
        let columns: Vec<ArrayRef> = vec![
            Arc::new(Int64Array::from(
                rows.iter().map(|row| row.row_id).collect::<Vec<_>>(),
            )),
            Arc::new(Int64Array::from(
                rows.iter().map(|row| row.device_id).collect::<Vec<_>>(),
            )),
            Arc::new(StringArray::from(
                rows.iter()
                    .map(|row| row.metric.as_str())
                    .collect::<Vec<_>>(),
            )),
            Arc::new(Float64Array::from(
                rows.iter().map(|row| row.value).collect::<Vec<_>>(),
            )),
            Arc::new(StringArray::from(
                rows.iter()
                    .map(|row| row.payload.as_str())
                    .collect::<Vec<_>>(),
            )),
            Arc::new(
                TimestampMicrosecondArray::from(
                    rows.iter()
                        .map(|row| row.wyrd_event_time_micros)
                        .collect::<Vec<_>>(),
                )
                .with_timezone("UTC".to_owned()),
            ),
        ];
        let record = RecordBatch::try_new(schema.clone(), columns)
            .map_err(|error| MaterializationError::Backend(error.to_string()))?;
        let mut payload = Vec::new();
        let mut writer = StreamWriter::try_new(&mut payload, schema.as_ref())
            .map_err(|error| MaterializationError::Backend(error.to_string()))?;
        writer
            .write(&record)
            .and_then(|()| writer.finish())
            .map_err(|error| MaterializationError::Backend(error.to_string()))?;
        Ok(EncodedBatch {
            payload: Bytes::from(payload),
        })
    }

    /// Retry one immutable batch until it is accepted or the absolute deadline wins.
    async fn send_with_backoff(
        &self,
        request: BatchRequest,
        payload: Bytes,
        pressure: &mut PressureEvidence,
        progress: &mut PartialProgress,
    ) -> Result<(), MaterializationError> {
        let batch_id =
            deterministic_batch_id(request.tenant_index, request.row / MATERIALIZER_BATCH_ROWS);
        let mut backoff = self.policy.initial_backoff;
        loop {
            let outcome = self
                .backend
                .ingest(request.tenant, batch_id, payload.clone())
                .await;
            match outcome {
                Ok(()) => {
                    progress.accepted_rows = progress.accepted_rows.saturating_add(request.count);
                    progress.accepted_batches = progress.accepted_batches.saturating_add(1);
                    if self.cancellation.is_cancelled() {
                        return Err(MaterializationError::Cancelled {
                            progress: *progress,
                        });
                    }
                    return Ok(());
                }
                Err(error) if is_retryable_capacity(&error) => {
                    pressure.rejections = pressure.rejections.saturating_add(1);
                    if self.cancellation.is_cancelled() {
                        return Err(MaterializationError::Cancelled {
                            progress: *progress,
                        });
                    }
                    let now = Instant::now();
                    let remaining = self.policy.setup_deadline.saturating_duration_since(now);
                    // Clamp the intended backoff to whatever time is left; a
                    // zero remainder means the deadline is already at or past.
                    let wait = backoff.min(remaining);
                    // Record the truncated wait as pressure evidence so that every
                    // rejection appears in the waited total. When the deadline has
                    // already passed (wait == 0), record 1 ns to distinguish a
                    // deadline-expired rejection from a rejection that never
                    // attempted to wait (task execution record requirement).
                    let evidence_wait = if wait.is_zero() {
                        Duration::from_nanos(1)
                    } else {
                        wait
                    };
                    pressure.waited = pressure.waited.saturating_add(evidence_wait);
                    if wait.is_zero() {
                        self.capture_server_pressure(pressure);
                        return Err(MaterializationError::SetupDeadlineExceeded {
                            pressure: pressure.clone(),
                            progress: *progress,
                        });
                    }
                    tokio::select! {
                        _ = self.cancellation.cancelled() => {
                            return Err(MaterializationError::Cancelled { progress: *progress });
                        }
                        _ = tokio::time::sleep(wait) => {}
                    }
                    if Instant::now() >= self.policy.setup_deadline {
                        self.capture_server_pressure(pressure);
                        return Err(MaterializationError::SetupDeadlineExceeded {
                            pressure: pressure.clone(),
                            progress: *progress,
                        });
                    }
                    backoff = backoff.saturating_mul(2).min(self.policy.max_backoff);
                }
                Err(error) => {
                    if self.cancellation.is_cancelled() {
                        return Err(MaterializationError::Cancelled {
                            progress: *progress,
                        });
                    }
                    return Err(MaterializationError::Ingest { source: error });
                }
            }
        }
    }

    /// Poll public Oracle until one logical day reaches its expected count.
    async fn wait_for_visibility(
        &self,
        tenant: DataTenantId,
        day: u32,
        expected: u64,
        rows_per_day: u64,
        progress: &mut PartialProgress,
    ) -> Result<u64, MaterializationError> {
        let mut backoff = self.policy.initial_backoff;
        loop {
            let observed = self
                .backend
                .visible_count(tenant, day, rows_per_day)
                .await
                .map_err(MaterializationError::Backend)?;
            if self.cancellation.is_cancelled() {
                return Err(MaterializationError::Cancelled {
                    progress: *progress,
                });
            }
            if observed >= expected {
                return Ok(observed);
            }
            let now = Instant::now();
            if now >= self.policy.setup_deadline {
                return Err(MaterializationError::Visibility {
                    tenant,
                    day,
                    expected,
                    observed,
                });
            }
            let remaining = self.policy.setup_deadline.saturating_duration_since(now);
            let wait = backoff.min(remaining);
            tokio::select! {
                _ = self.cancellation.cancelled() => {
                    return Err(MaterializationError::Cancelled { progress: *progress });
                }
                _ = tokio::time::sleep(wait) => {}
            }
            if Instant::now() >= self.policy.setup_deadline {
                return Err(MaterializationError::Visibility {
                    tenant,
                    day,
                    expected,
                    observed,
                });
            }
            backoff = backoff.saturating_mul(2).min(self.policy.max_backoff);
        }
    }

    /// Create the one directory owned by this materialization run.
    fn create_run_dir(&self) -> Result<PathBuf, MaterializationError> {
        std::fs::create_dir_all(&self.run_root)
            .map_err(|error| MaterializationError::Backend(error.to_string()))?;
        let run_dir = self
            .run_root
            .join(format!("materializer-{}", uuid::Uuid::now_v7()));
        std::fs::create_dir(&run_dir)
            .map_err(|error| MaterializationError::Backend(error.to_string()))?;
        Ok(run_dir)
    }

    /// Remove the run directory, retaining harness-owned tables and servers.
    fn cleanup_run_dir(&self, run_dir: &Path) -> Result<(), std::io::Error> {
        std::fs::remove_dir_all(run_dir)
    }

    /// Preserve the causal failure when cleanup itself cannot complete.
    fn cleanup_failure(&self, run_dir: &Path, cause: MaterializationError) -> MaterializationError {
        match self.cleanup_run_dir(run_dir) {
            Ok(()) => cause,
            Err(error) => MaterializationError::Cleanup {
                run_dir: run_dir.to_owned(),
                cause: Box::new(cause),
                cleanup_error: error.to_string(),
            },
        }
    }
}

/// Arrow bytes retained for one immutable retry identity.
struct EncodedBatch {
    /// Complete Arrow IPC stream.
    payload: Bytes,
}

/// Identity and row count for one immutable ingest request.
#[derive(Debug, Clone, Copy)]
struct BatchRequest {
    /// Tenant receiving this request.
    tenant: DataTenantId,
    /// Stable tenant ordinal used in the batch identity.
    tenant_index: usize,
    /// First dataset row in this batch.
    row: u64,
    /// Number of rows acknowledged when the request succeeds.
    count: u64,
}

/// Public production adapter over the real test server and client transports.
struct RealMaterializerBackend<'a> {
    /// Harness server that owns table creation and tenant flush.
    server: &'a WyrdTestServer,
    /// Stable tenant ordering matching transport/query vectors.
    tenants: Vec<DataTenantId>,
    /// Authenticated public Gate transports.
    transports: Vec<BifrostGrpcTransport>,
    /// Authenticated public Oracle query handles.
    queries: Vec<QueryClient>,
}

#[async_trait]
impl MaterializerBackend for RealMaterializerBackend<'_> {
    async fn provision_table(&self, tenant: DataTenantId) -> Result<(), String> {
        let catalog = self
            .server
            .state()
            .bifrost_redux
            .as_ref()
            .ok_or_else(|| "missing Redux catalog".to_owned())?;
        catalog
            .create_table(CreateTableRequest {
                table: TableRef::new(BifrostNamespace::Bifrost, QUALIFICATION_TABLE),
                user_fields: vec![
                    Field::new("row_id", DataType::Int64, false),
                    Field::new("device_id", DataType::Int64, false),
                    Field::new("metric", DataType::Utf8, false),
                    Field::new("value", DataType::Float64, false),
                    Field::new("payload", DataType::Utf8, false),
                ],
                tenant,
                audit: None,
            })
            .await
            .map(|_| ())
            .map_err(|error| error.to_string())
    }

    async fn ingest(
        &self,
        tenant: DataTenantId,
        batch_id: [u8; 16],
        payload: Bytes,
    ) -> Result<(), WyrdError> {
        let index = self
            .tenants
            .iter()
            .position(|candidate| *candidate == tenant)
            .ok_or_else(|| WyrdError::Validation {
                message: "materializer tenant is not provisioned".to_owned(),
                details: serde_json::json!({}),
            })?;
        self.transports[index]
            .send_frame(BifrostFrame {
                table: format!("vala.bifrost.{QUALIFICATION_TABLE}"),
                batch_id,
                arrow_ipc: payload,
            })
            .await
    }

    async fn flush(&self, tenant: DataTenantId) -> Result<(), String> {
        self.server
            .flush_bifrost_for_tenant(tenant)
            .await
            .map_err(|error| error.to_string())
    }

    async fn visible_count(
        &self,
        tenant: DataTenantId,
        day: u32,
        rows_per_day: u64,
    ) -> Result<u64, String> {
        let index = self
            .tenants
            .iter()
            .position(|candidate| *candidate == tenant)
            .ok_or_else(|| "materializer tenant is not provisioned".to_owned())?;
        let start = u64::from(day).saturating_mul(rows_per_day);
        let end = start.saturating_add(rows_per_day);
        let request = BifrostQueryRequest {
            sql: format!(
                "SELECT COUNT(*) AS row_count FROM vala.bifrost.{QUALIFICATION_TABLE} WHERE row_id >= {start} AND row_id < {end}"
            ),
            visibility: VisibilityMode::PublishedOnly,
            freshness: FreshnessPolicy::Strict,
            deadline_ms: Some(5_000),
        };
        let result = self.queries[index]
            .collect_bounded(
                &request,
                CollectedQueryLimits {
                    max_rows: 1,
                    max_encoded_bytes: 1_024 * 1_024,
                },
            )
            .await
            .map_err(|error| error.to_string())?;
        let batch = result
            .batches
            .first()
            .ok_or_else(|| "Oracle count returned no batch".to_owned())?;
        let count = batch
            .column(0)
            .as_any()
            .downcast_ref::<Int64Array>()
            .ok_or_else(|| "Oracle count was not Int64".to_owned())?;
        u64::try_from(count.value(0)).map_err(|error| error.to_string())
    }
}

/// Return whether an error is one of the documented capacity responses.
fn is_retryable_capacity(error: &WyrdError) -> bool {
    if matches!(
        error,
        WyrdError::Vala {
            error: BifrostError::IngestBusy { .. } | BifrostError::WalDiskFull
        }
    ) {
        return true;
    }
    let details = match error {
        WyrdError::UpstreamFailure { details, .. } => details,
        _ => return false,
    };
    matches!(
        details
            .get("original_code")
            .and_then(serde_json::Value::as_str),
        Some("WYRD_VALA_429_INGEST_BUSY" | "WYRD_VALA_507_WAL_DISK_FULL")
    )
}

/// Derive a stable UUID-shaped identity from tenant and batch ordinal.
fn deterministic_batch_id(tenant_index: usize, ordinal: u64) -> [u8; 16] {
    let mut bytes = [0_u8; 16];
    bytes[..8].copy_from_slice(&(0xB1_F057_u64 ^ ordinal).to_be_bytes());
    bytes[8..].copy_from_slice(&(tenant_index as u64 ^ ordinal.rotate_left(17)).to_be_bytes());
    bytes[6] = (bytes[6] & 0x0f) | 0x70;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    bytes
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;
    use std::io::Cursor;
    use std::sync::Mutex;

    use crate::bifrost::bench_dataset::DatasetShape;
    use arrow::array::{Array, TimestampMicrosecondArray};
    use arrow::ipc::reader::StreamReader;
    use tokio::sync::Notify;

    #[derive(Default)]
    struct MockBackend {
        outcomes: Mutex<VecDeque<Result<(), WyrdError>>>,
        ids: Mutex<Vec<[u8; 16]>>,
        counts: Mutex<VecDeque<u64>>,
        retry_signal: Mutex<Option<Arc<Notify>>>,
    }

    #[async_trait]
    impl MaterializerBackend for MockBackend {
        async fn provision_table(&self, _tenant: DataTenantId) -> Result<(), String> {
            Ok(())
        }

        async fn ingest(
            &self,
            _tenant: DataTenantId,
            batch_id: [u8; 16],
            _payload: Bytes,
        ) -> Result<(), WyrdError> {
            self.ids.lock().expect("mock ids lock").push(batch_id);
            let outcome = self
                .outcomes
                .lock()
                .expect("mock outcomes lock")
                .pop_front()
                .unwrap_or(Ok(()));
            if matches!(&outcome, Err(error) if is_retryable_capacity(error))
                && let Some(signal) = self
                    .retry_signal
                    .lock()
                    .expect("retry signal lock")
                    .as_ref()
            {
                signal.notify_one();
            }
            outcome
        }

        async fn flush(&self, _tenant: DataTenantId) -> Result<(), String> {
            Ok(())
        }

        async fn visible_count(
            &self,
            _tenant: DataTenantId,
            _day: u32,
            rows_per_day: u64,
        ) -> Result<u64, String> {
            Ok(self
                .counts
                .lock()
                .expect("mock count lock")
                .pop_front()
                .unwrap_or(rows_per_day))
        }
    }

    fn dataset() -> BifrostQualificationDataset {
        BifrostQualificationDataset::new(DatasetShape::new(2, 4_096).expect("shape"))
            .expect("dataset")
    }

    fn busy() -> WyrdError {
        BifrostError::IngestBusy {
            table: QUALIFICATION_TABLE.to_owned(),
        }
        .into()
    }

    fn materializer(
        backend: Arc<MockBackend>,
        deadline: Instant,
    ) -> (BifrostDatasetMaterializer<'static>, PathBuf) {
        let run_root = tempfile::tempdir().expect("run root").keep();
        (
            BifrostDatasetMaterializer::with_backend(
                backend,
                dataset(),
                vec![DataTenantId::SYSTEM_OWNER],
                BackpressurePolicy::with_deadline(deadline),
                run_root.clone(),
                CancellationToken::new(),
            ),
            run_root,
        )
    }

    /// Retry capacity responses while retaining one immutable batch identity.
    #[tokio::test]
    async fn retryable_capacity_backs_off_and_retains_batch_identity() {
        let backend = Arc::new(MockBackend::default());
        backend
            .outcomes
            .lock()
            .expect("outcomes")
            .extend([Err(busy()), Err(busy()), Ok(())]);
        let (materializer, _run_root) =
            materializer(backend.clone(), Instant::now() + Duration::from_secs(5));
        let result = materializer
            .materialize()
            .await
            .expect("materialization succeeds");
        assert_eq!(result.pressure.rejections, 2);
        let ids = backend.ids.lock().expect("ids");
        assert_eq!(ids[0], ids[1]);
        assert_eq!(ids[1], ids[2]);
    }

    /// Preserve a truncated final retry wait as bounded admission evidence.
    #[tokio::test]
    async fn setup_deadline_fails_with_pressure_evidence() {
        let backend = Arc::new(MockBackend::default());
        backend
            .outcomes
            .lock()
            .expect("outcomes")
            .push_back(Err(busy()));
        let (materializer, run_root) =
            materializer(backend.clone(), Instant::now() + Duration::from_millis(10));
        let error = materializer
            .materialize()
            .await
            .expect_err("deadline fails");
        match error {
            MaterializationError::SetupDeadlineExceeded { pressure, progress } => {
                assert_eq!(pressure.rejections, 1);
                assert!(!pressure.waited.is_zero());
                assert!(pressure.waited < Duration::from_millis(50));
                assert_eq!(progress.accepted_batches, 0);
            }
            other => panic!("unexpected error: {other:?}"),
        }
        assert_eq!(backend.ids.lock().expect("ids").len(), 1);
        assert_eq!(std::fs::read_dir(run_root).expect("run root").count(), 0);
    }

    /// Return one non-retryable ingest error without issuing another request.
    #[tokio::test]
    async fn non_retryable_error_fails_immediately() {
        let backend = Arc::new(MockBackend::default());
        backend
            .outcomes
            .lock()
            .expect("outcomes")
            .push_back(Err(WyrdError::Validation {
                message: "bad frame".to_owned(),
                details: serde_json::json!({}),
            }));
        let (materializer, run_root) =
            materializer(backend.clone(), Instant::now() + Duration::from_secs(5));
        let error = materializer
            .materialize()
            .await
            .expect_err("non-retryable fails");
        assert!(matches!(error, MaterializationError::Ingest { .. }));
        assert_eq!(backend.ids.lock().expect("ids").len(), 1);
        assert_eq!(std::fs::read_dir(run_root).expect("run root").count(), 0);
    }

    /// Cancel an active retry wait after the backend has emitted a busy signal.
    #[tokio::test]
    async fn cancellation_reports_partial_progress() {
        let backend = Arc::new(MockBackend::default());
        let token = CancellationToken::new();
        let signal = Arc::new(Notify::new());
        *backend.retry_signal.lock().expect("retry signal") = Some(signal.clone());
        backend
            .outcomes
            .lock()
            .expect("outcomes")
            .push_back(Err(busy()));
        let cancel_token = token.clone();
        let canceller = tokio::spawn(async move {
            signal.notified().await;
            cancel_token.cancel();
        });
        let run_root = tempfile::tempdir().expect("run root").keep();
        let materializer = BifrostDatasetMaterializer::with_backend(
            backend.clone(),
            dataset(),
            vec![DataTenantId::SYSTEM_OWNER],
            BackpressurePolicy::with_deadline(Instant::now() + Duration::from_secs(5)),
            run_root.clone(),
            token.clone(),
        );
        let error = materializer.materialize().await.expect_err("cancelled");
        canceller.await.expect("canceller");
        assert!(
            matches!(error, MaterializationError::Cancelled { progress } if progress.accepted_rows == 0)
        );
        assert_eq!(backend.ids.lock().expect("ids").len(), 1);
        assert_eq!(std::fs::read_dir(run_root).expect("run root").count(), 0);
    }

    /// Encode the managed event-time column with the exact UTC microsecond values.
    #[test]
    fn dataset_batch_encodes_managed_event_time() {
        let backend = Arc::new(MockBackend::default());
        let (materializer, _run_root) =
            materializer(backend, Instant::now() + Duration::from_secs(5));
        let encoded = materializer.dataset_batch(0, 4).expect("batch encodes");
        let mut reader =
            StreamReader::try_new(Cursor::new(encoded.payload), None).expect("IPC reader");
        let batch = reader
            .next()
            .expect("one IPC batch")
            .expect("IPC batch decodes");
        let schema = batch.schema();
        let index = schema
            .index_of(WYRD_EVENT_TIME)
            .expect("managed event time");
        let field = schema.field(index);
        assert_eq!(
            field.data_type(),
            &DataType::Timestamp(TimeUnit::Microsecond, Some("UTC".into()))
        );
        let timestamps = batch
            .column(index)
            .as_any()
            .downcast_ref::<TimestampMicrosecondArray>()
            .expect("timestamp array");
        assert_eq!(timestamps.len(), 4);
        assert!((0..timestamps.len()).all(|index| timestamps.is_valid(index)));
        assert_eq!(
            (0..4)
                .map(|index| timestamps.value(index))
                .collect::<Vec<_>>(),
            (0..4)
                .map(|index| materializer.dataset.row(index).wyrd_event_time_micros)
                .collect::<Vec<_>>()
        );
    }

    /// Keep visibility polling out of admission pressure evidence.
    #[tokio::test]
    async fn visibility_wait_does_not_increment_pressure_waited() {
        let backend = Arc::new(MockBackend::default());
        backend
            .counts
            .lock()
            .expect("counts")
            .extend([0, 4_096, 4_096]);
        let (materializer, _run_root) =
            materializer(backend, Instant::now() + Duration::from_secs(5));
        let result = materializer
            .materialize()
            .await
            .expect("visibility succeeds");
        assert_eq!(result.pressure.waited, Duration::ZERO);
    }

    /// Derive the binding ceiling as the max-count ceiling, then as the governor fallback.
    ///
    /// Proves both halves of [`derive_binding_ceiling`]: a populated
    /// `binding_ceilings` map names its largest-count ceiling, and an empty map
    /// falls back to the governor high-water ceiling nearest its limit. A
    /// default (uncaptured) `PressureEvidence` names no ceiling.
    #[test]
    fn pressure_evidence_populates_binding_ceiling() {
        let governor = GovernorHighWater {
            parent_used_memory: 10,
            parent_memory_limit: 100,
            scribe_used_memory: 90,
            scribe_memory_limit: 100,
            wal_disk_bytes: 0,
        };
        let mut binding_ceilings = BTreeMap::new();
        binding_ceilings.insert("scribe_child".to_owned(), 2);
        binding_ceilings.insert("bifrost_parent".to_owned(), 5);
        let evidence = PressureEvidence {
            rejections: 7,
            waited: Duration::from_millis(5),
            binding_ceilings,
            governor_high_water: Some(governor),
        };
        assert_eq!(
            evidence.binding_ceiling().as_deref(),
            Some("bifrost_parent")
        );

        let fallback = PressureEvidence {
            binding_ceilings: BTreeMap::new(),
            governor_high_water: Some(governor),
            ..PressureEvidence::default()
        };
        assert_eq!(fallback.binding_ceiling().as_deref(), Some("scribe_child"));

        assert_eq!(PressureEvidence::default().binding_ceiling(), None);
    }

    /// Keep the client-side `{ rejections, waited }` contract and default the new fields.
    ///
    /// Proves the D71 client-side counters are set exactly as before and the
    /// additive D84 fields default empty/`None` when no telemetry was captured
    /// (AC2).
    #[test]
    fn pressure_evidence_client_contract_unchanged() {
        let evidence = PressureEvidence {
            rejections: 3,
            waited: Duration::from_millis(42),
            ..PressureEvidence::default()
        };
        assert_eq!(evidence.rejections, 3);
        assert_eq!(evidence.waited, Duration::from_millis(42));
        assert!(evidence.binding_ceilings.is_empty());
        assert!(evidence.governor_high_water.is_none());
    }
}
