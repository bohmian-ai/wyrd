//! Bounded immutable-generation persistence for Scribe.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use bytes::Bytes;
use num_traits::ToPrimitive;
use tokio::runtime::Handle;
use tokio::sync::{Notify, mpsc};
use uuid::Uuid;
use vala_sql::ValaPostgres;
use wyrd_spec::vala::api::AuditEvent;

use crate::catalog::{TenantTableBinding, TenantTableKey};
use crate::contracts::ScribeError;
use crate::scribe::execution_lanes::{
    ScribePostAckCpuOp, ScribePostAckCpuPool, ScribePostAckCpuResult, ScribeWalIoOp,
    ScribeWalIoPool, ScribeWalIoResult,
};
use crate::scribe::file_list_writer::{self, FileListCommitKey};
use crate::scribe::memtable::FrozenMemtable;
use crate::scribe::seal_key::SealKey;
use crate::scribe::stream_identity::StreamIdentity;
use crate::scribe::wal::{ScribeAppendMeta, WalLsn, WalSegmentRef, WalWriter};

/// Stable identity for one detached generation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct GenerationId(pub u64);

/// Immutable state transferred from a writer consumer to persistence.
#[derive(Debug, Clone)]
pub struct ImmutableGeneration {
    /// Logical tenant/table owning the generation.
    pub writer_key: TenantTableKey,
    /// Writer instance that detached the generation.
    pub writer_instance_id: Uuid,
    /// Event-day partition represented by the generation.
    pub seal_key: SealKey,
    /// Local generation identity.
    pub generation_id: GenerationId,
    /// WAL stream identity used for file-list and manifest publication.
    pub stream: StreamIdentity,
    /// Inclusive minimum WAL LSN.
    pub wal_lsn_min: WalLsn,
    /// Inclusive maximum WAL LSN.
    pub wal_lsn_max: WalLsn,
    /// WAL segments retained until publication and grace expiry.
    pub wal_segments: Vec<WalSegmentRef>,
    /// WAL handle retained for grace-ordered segment retirement.
    pub wal: crate::scribe::wal::WalHandle,
    /// Original Arrow batches, shared by tail and persistence readers.
    pub rows: Vec<arrow::record_batch::RecordBatch>,
    /// Merged Arrow batch used by the encoder.
    pub batch: arrow::record_batch::RecordBatch,
    /// Canonical audit events paired with the rows.
    pub audit_events: Vec<AuditEvent>,
    /// WAL-derived append metadata paired with the batches.
    pub append_metas: Vec<ScribeAppendMeta>,
    /// Number of rows in the generation.
    pub row_count: usize,
    /// Estimated Arrow bytes retained by the generation.
    pub arrow_bytes: usize,
    /// Monotonic active-generation open time.
    pub opened_at: std::time::Instant,
    /// Monotonic detach time.
    pub closed_at: std::time::Instant,
}

impl ImmutableGeneration {
    /// Detach a frozen memtable snapshot into a persistence payload.
    #[must_use]
    pub fn from_frozen(
        frozen: &FrozenMemtable,
        writer_key: TenantTableKey,
        writer_instance_id: Uuid,
        stream: StreamIdentity,
        wal_segments: Vec<WalSegmentRef>,
        wal: crate::scribe::wal::WalHandle,
    ) -> Self {
        let wal_lsn_min = frozen
            .metas
            .iter()
            .map(|meta| meta.wal_lsn_min)
            .min()
            .unwrap_or(WalLsn::ZERO);
        let wal_lsn_max = frozen
            .metas
            .iter()
            .map(|meta| meta.wal_lsn_max)
            .max()
            .unwrap_or(WalLsn::ZERO);
        Self {
            writer_key,
            writer_instance_id,
            seal_key: frozen.seal_key.clone(),
            generation_id: GenerationId(frozen.seal_id),
            stream,
            wal_lsn_min,
            wal_lsn_max,
            wal_segments,
            wal,
            rows: frozen.batches.clone(),
            batch: frozen.batch.clone(),
            audit_events: frozen.events.clone(),
            append_metas: frozen.metas.clone(),
            row_count: frozen.batch.num_rows(),
            arrow_bytes: frozen.arrow_bytes,
            opened_at: frozen.opened_at,
            closed_at: frozen.closed_at,
        }
    }

    fn frozen_snapshot(&self) -> FrozenMemtable {
        FrozenMemtable {
            seal_id: self.generation_id.0,
            seal_key: self.seal_key.clone(),
            schema: self.batch.schema(),
            batch: self.batch.clone(),
            batches: self.rows.clone(),
            events: self.audit_events.clone(),
            metas: self.append_metas.clone(),
            opened_at: self.opened_at,
            closed_at: self.closed_at,
            arrow_bytes: self.arrow_bytes,
        }
    }
}

/// Completion sent through a writer's reliable control queue.
#[derive(Debug, Clone)]
pub(crate) struct PersistenceCompletion {
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "legacy writer identity is used by test-only lifecycle probes"
        )
    )]
    pub(crate) writer_instance_id: Uuid,
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "legacy writer identity is used by test-only lifecycle probes"
        )
    )]
    pub(crate) seal_key: SealKey,
    pub(crate) generation_id: GenerationId,
    pub(crate) file_list_key: Option<FileListCommitKey>,
    pub(crate) wal_segments: Vec<WalSegmentRef>,
    pub(crate) wal: crate::scribe::wal::WalHandle,
    pub(crate) arrow_bytes: usize,
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "legacy writer timing is used by test-only lifecycle probes"
        )
    )]
    pub(crate) published_at: std::time::Instant,
    pub(crate) error: Option<String>,
}

/// Completion controls shared by shard owners and persistence workers.
#[derive(Debug)]
pub(crate) enum WriterControl {
    PersistenceComplete(PersistenceCompletion),
    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "legacy writer grace control is test-only")
    )]
    GraceExpired(PersistenceCompletion),
    Shutdown,
}

/// One immutable generation submitted to the bounded persistence queue.
#[derive(Debug, Clone)]
pub(crate) struct PersistenceJob {
    pub(crate) generation: Arc<ImmutableGeneration>,
    pub(crate) binding: TenantTableBinding,
    pub(crate) completion_tx: mpsc::Sender<WriterControl>,
}

/// Server-provisioned persistence dependencies.
pub struct ScribePersistenceConfig {
    /// Tenant-scoped Vala Postgres pool owner.
    pub postgres: Arc<ValaPostgres>,
    /// Maximum queued immutable generations.
    pub queue_items: usize,
    /// Number of asynchronous persistence workers.
    pub workers: usize,
}

impl std::fmt::Debug for ScribePersistenceConfig {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ScribePersistenceConfig")
            .field("queue_items", &self.queue_items)
            .field("workers", &self.workers)
            .finish_non_exhaustive()
    }
}

impl ScribePersistenceConfig {
    /// Construct bounded persistence configuration.
    #[must_use]
    pub fn new(postgres: Arc<ValaPostgres>, queue_items: usize, workers: usize) -> Self {
        Self {
            postgres,
            queue_items: queue_items.max(1),
            workers: workers.max(1),
        }
    }
}

pub(crate) struct PersistenceRuntimeContext {
    pub(crate) operator: Arc<opendal::Operator>,
    pub(crate) wal: Arc<WalWriter>,
    pub(crate) post_ack_cpu: ScribePostAckCpuPool,
    pub(crate) wal_io: ScribeWalIoPool,
    pub(crate) node_id: String,
    pub(crate) writer_epoch: i64,
}

/// Bounded persistence queue and worker set.
#[derive(Clone)]
pub struct PersistenceRuntime {
    sender: Arc<Mutex<Option<mpsc::Sender<Box<PersistenceJob>>>>>,
    queued: Arc<AtomicUsize>,
    queued_bytes: Arc<AtomicUsize>,
    drained: Arc<Notify>,
}

impl std::fmt::Debug for PersistenceRuntime {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("PersistenceRuntime")
            .field("queued", &self.queued.load(Ordering::Acquire))
            .field("queued_bytes", &self.queued_bytes.load(Ordering::Acquire))
            .finish_non_exhaustive()
    }
}

impl PersistenceRuntime {
    /// Start bounded persistence workers on the Scribe coordination runtime.
    #[must_use]
    pub(crate) fn start(
        config: ScribePersistenceConfig,
        context: PersistenceRuntimeContext,
        runtime: &Handle,
    ) -> Arc<Self> {
        let (sender, receiver) = mpsc::channel::<Box<PersistenceJob>>(config.queue_items);
        let runtime_state = Arc::new(Self {
            sender: Arc::new(Mutex::new(Some(sender))),
            queued: Arc::new(AtomicUsize::new(0)),
            queued_bytes: Arc::new(AtomicUsize::new(0)),
            drained: Arc::new(Notify::new()),
        });
        let receiver = Arc::new(tokio::sync::Mutex::new(receiver));
        let dependencies = Arc::new(PersistenceDependencies {
            postgres: config.postgres,
            operator: context.operator,
            wal: context.wal,
            post_ack_cpu: context.post_ack_cpu,
            wal_io: context.wal_io,
            node_id: context.node_id,
            writer_epoch: context.writer_epoch,
            manifest_guard: tokio::sync::Mutex::new(()),
        });
        for _ in 0..config.workers {
            let receiver = Arc::clone(&receiver);
            let dependencies = Arc::clone(&dependencies);
            let state = Arc::clone(&runtime_state);
            runtime.spawn(async move {
                loop {
                    let job = receiver.lock().await.recv().await;
                    let Some(job) = job else { break };
                    let queued_bytes = job.generation.arrow_bytes;
                    process_job(*job, &dependencies).await;
                    state.queued.fetch_sub(1, Ordering::AcqRel);
                    state.queued_bytes.fetch_sub(queued_bytes, Ordering::AcqRel);
                    metrics::gauge!("bifrost_scribe_persistence_queue_depth").set(
                        state
                            .queued
                            .load(Ordering::Acquire)
                            .to_f64()
                            .unwrap_or(f64::MAX),
                    );
                    metrics::gauge!("bifrost_scribe_persistence_queue_bytes").set(
                        state
                            .queued_bytes
                            .load(Ordering::Acquire)
                            .to_f64()
                            .unwrap_or(f64::MAX),
                    );
                    state.drained.notify_waiters();
                }
            });
        }
        runtime_state
    }

    /// Try to enqueue a job without waiting in the writer consumer.
    pub(crate) fn try_submit(&self, job: PersistenceJob) -> Result<(), Box<PersistenceJob>> {
        let sender = match self.sender.lock() {
            Ok(sender) => sender.clone(),
            Err(_) => return Err(Box::new(job)),
        };
        let Some(sender) = sender else {
            return Err(Box::new(job));
        };
        let queued_bytes = job.generation.arrow_bytes;
        self.queued.fetch_add(1, Ordering::AcqRel);
        self.queued_bytes.fetch_add(queued_bytes, Ordering::AcqRel);
        match sender.try_send(Box::new(job)) {
            Ok(()) => {
                metrics::gauge!("bifrost_scribe_persistence_queue_depth").set(
                    self.queued
                        .load(Ordering::Acquire)
                        .to_f64()
                        .unwrap_or(f64::MAX),
                );
                metrics::gauge!("bifrost_scribe_persistence_queue_bytes").set(
                    self.queued_bytes
                        .load(Ordering::Acquire)
                        .to_f64()
                        .unwrap_or(f64::MAX),
                );
                Ok(())
            }
            Err(mpsc::error::TrySendError::Full(job) | mpsc::error::TrySendError::Closed(job)) => {
                self.queued.fetch_sub(1, Ordering::AcqRel);
                self.queued_bytes.fetch_sub(queued_bytes, Ordering::AcqRel);
                metrics::counter!("bifrost_scribe_persistence_queue_rejections_total").increment(1);
                Err(job)
            }
        }
    }

    /// Stop accepting jobs and wait for queued jobs to finish.
    pub(crate) async fn close_and_drain(&self) {
        if let Ok(mut sender) = self.sender.lock() {
            sender.take();
        }
        loop {
            let notified = self.drained.notified();
            if self.queued.load(Ordering::Acquire) == 0 {
                return;
            }
            notified.await;
        }
    }

    /// Current queued/in-flight persistence jobs.
    #[must_use]
    pub fn queue_depth(&self) -> usize {
        self.queued.load(Ordering::Acquire)
    }

    /// Current Arrow bytes retained by queued or executing persistence jobs.
    #[must_use]
    pub fn queue_bytes(&self) -> usize {
        self.queued_bytes.load(Ordering::Acquire)
    }
}

struct PersistenceDependencies {
    postgres: Arc<ValaPostgres>,
    operator: Arc<opendal::Operator>,
    wal: Arc<WalWriter>,
    post_ack_cpu: ScribePostAckCpuPool,
    wal_io: ScribeWalIoPool,
    node_id: String,
    writer_epoch: i64,
    manifest_guard: tokio::sync::Mutex<()>,
}

async fn process_job(job: PersistenceJob, dependencies: &PersistenceDependencies) {
    let generation = Arc::clone(&job.generation);
    tracing::debug!(
        writer_instance_id = %generation.writer_instance_id,
        generation_id = generation.generation_id.0,
        seal_key = %generation.seal_key,
        wal_lsn_min = generation.wal_lsn_min.as_u64(),
        wal_lsn_max = generation.wal_lsn_max.as_u64(),
        "persisting immutable Scribe generation"
    );
    let result = persist_with_retries(&generation, &job.binding, dependencies).await;
    let published_at = std::time::Instant::now();
    metrics::histogram!("bifrost_scribe_persistence_publication_seconds").record(
        published_at
            .duration_since(generation.closed_at)
            .as_secs_f64(),
    );
    let completion = match result {
        Ok(file_list_key) => PersistenceCompletion {
            writer_instance_id: generation.writer_instance_id,
            seal_key: generation.seal_key.clone(),
            generation_id: generation.generation_id,
            file_list_key: Some(file_list_key),
            wal_segments: generation.wal_segments.clone(),
            wal: generation.wal.clone(),
            arrow_bytes: generation.arrow_bytes,
            published_at,
            error: None,
        },
        Err(error) => PersistenceCompletion {
            writer_instance_id: generation.writer_instance_id,
            seal_key: generation.seal_key.clone(),
            generation_id: generation.generation_id,
            file_list_key: None,
            wal_segments: generation.wal_segments.clone(),
            wal: generation.wal.clone(),
            arrow_bytes: generation.arrow_bytes,
            published_at,
            error: Some(error.to_string()),
        },
    };
    if job
        .completion_tx
        .send(WriterControl::PersistenceComplete(completion))
        .await
        .is_err()
    {
        metrics::counter!("bifrost_scribe_persistence_completion_dropped_total").increment(1);
    }
}

async fn persist_with_retries(
    generation: &ImmutableGeneration,
    binding: &TenantTableBinding,
    dependencies: &PersistenceDependencies,
) -> Result<FileListCommitKey, ScribeError> {
    let mut last_error = None;
    for attempt in 0..5_u32 {
        match persist_once(generation, binding, dependencies).await {
            Ok(key) => {
                metrics::counter!("bifrost_scribe_persistence_jobs_total", "status" => "published")
                    .increment(1);
                return Ok(key);
            }
            Err(error) => {
                last_error = Some(error);
                metrics::counter!("bifrost_scribe_persistence_retries_total").increment(1);
                if attempt < 4 {
                    tokio::time::sleep(Duration::from_millis(100 * 2_u64.pow(attempt))).await;
                }
            }
        }
    }
    metrics::counter!("bifrost_scribe_persistence_jobs_total", "status" => "failed").increment(1);
    Err(last_error.unwrap_or_else(|| ScribeError::Internal {
        detail: "persistence failed without an error".to_owned(),
    }))
}

async fn persist_once(
    generation: &ImmutableGeneration,
    binding: &TenantTableBinding,
    dependencies: &PersistenceDependencies,
) -> Result<FileListCommitKey, ScribeError> {
    let frozen = generation.frozen_snapshot();
    let mut encoded = match dependencies
        .post_ack_cpu
        .submit(ScribePostAckCpuOp::EncodeParquet {
            frozen: Box::new(frozen.clone()),
            binding: binding.clone(),
            tenant: binding.tenant,
        })
        .await?
    {
        ScribePostAckCpuResult::ParquetEncoded(encoded) => encoded,
        ScribePostAckCpuResult::Prepared(_) | ScribePostAckCpuResult::ReplayRestored => {
            return Err(ScribeError::Internal {
                detail: "post-ACK lane returned the wrong persistence result".to_owned(),
            });
        }
    };
    let path = object_path(binding, &dependencies.node_id)?;
    let path = put_object(
        &dependencies.operator,
        &path,
        std::mem::take(&mut encoded.bytes),
    )
    .await?;
    let row = file_list_writer::build_insert(
        &frozen,
        &encoded,
        binding,
        &dependencies.node_id,
        dependencies.writer_epoch,
        &path,
    )?;
    let mut conn = dependencies
        .postgres
        .tenant_conn(binding.tenant)
        .await
        .map_err(ScribeError::from)?;
    let outcome = file_list_writer::insert_and_audit(&mut conn, &row, &encoded.audit_events)
        .await
        .map_err(ScribeError::from)?;
    conn.commit().await.map_err(ScribeError::from)?;

    let manifest_path = dependencies.wal.base_dir().join("manifest");
    let lsn = generation.wal_lsn_max;
    let _manifest_guard = dependencies.manifest_guard.lock().await;
    let result = dependencies
        .wal_io
        .submit(ScribeWalIoOp::AdvanceManifest {
            path: manifest_path,
            stream: generation.stream,
            seal_key: generation.seal_key.clone(),
            sealed_lsn: lsn,
        })
        .await?;
    if !matches!(result, ScribeWalIoResult::Completed) {
        return Err(ScribeError::Internal {
            detail: "WAL IO lane returned the wrong manifest result".to_owned(),
        });
    }
    Ok(outcome.commit_key)
}

fn object_path(binding: &TenantTableBinding, node_id: &str) -> Result<String, ScribeError> {
    let node_uuid = Uuid::parse_str(node_id).map_err(|error| ScribeError::Internal {
        detail: format!("node_id is not a valid UUID: {error}"),
    })?;
    let pod_id =
        wyrd_spec::ids::PodId::new(format!("pod-{}", node_uuid.simple())).map_err(|error| {
            ScribeError::Internal {
                detail: format!("derived node PodId is invalid: {error}"),
            }
        })?;
    Ok(format!(
        "{}/{}",
        binding.object_prefix,
        crate::scribe::filename::seal_filename(&pod_id)
    ))
}

async fn put_object(
    operator: &opendal::Operator,
    path: &str,
    bytes: Vec<u8>,
) -> Result<String, ScribeError> {
    let bytes = Bytes::from(bytes);
    let mut last_error = None;
    for attempt in 0..5_u32 {
        match tokio::time::timeout(Duration::from_secs(30), operator.write(path, bytes.clone()))
            .await
        {
            Ok(Ok(_)) => return Ok(path.to_owned()),
            Ok(Err(error)) => {
                let retryable = matches!(error.kind(), opendal::ErrorKind::RateLimited)
                    || (matches!(error.kind(), opendal::ErrorKind::Unexpected)
                        && error.is_temporary());
                last_error = Some(ScribeError::ObjectStorePutFailed(error));
                if !retryable || attempt == 4 {
                    break;
                }
            }
            Err(_) => {
                last_error = Some(ScribeError::Internal {
                    detail: "object-store PUT timed out".to_owned(),
                });
                if attempt == 4 {
                    break;
                }
            }
        }
        tokio::time::sleep(Duration::from_millis(100 * 2_u64.pow(attempt))).await;
    }
    Err(last_error.unwrap_or_else(|| ScribeError::Internal {
        detail: "object-store PUT failed without an error".to_owned(),
    }))
}
