//! Seal state machine — freeze → Parquet → PUT → PG tx → manifest → retire WAL.

use std::sync::Arc;

use bytes::Bytes;
use opendal::{ErrorKind, Operator};
use tracing::info;
use uuid::Uuid;
use vala_sql::TenantConn;

use crate::catalog::TenantTableBinding;
use crate::contracts::ScribeError;
use crate::resources::{ScribeMemoryLease, ScribeResources};
use crate::scribe::execution_lanes::{
    ScribePersistenceCpuOp, ScribePersistenceCpuPool, ScribePersistenceCpuResult,
};
use crate::scribe::file_list_writer;
use crate::scribe::file_list_writer::FileListCommitKey;
use crate::scribe::memory::{MemoryCategory, parquet_candidate_incremental_bytes};
use crate::scribe::memtable::{FrozenMemtable, Memtable};
use crate::scribe::parquet_writer::BoundedParquetArtifactSet;
use crate::scribe::parquet_writer::ParquetEncoded;
use crate::scribe::seal_key::{ScribeArtifactIdentity, SealKey};
use crate::scribe::wal::WalLsn;

/// Cancellation-safe identity owner for objects uploaded before COMMIT.
///
/// Remote bytes are retained for deterministic retry and reconciliation; this
/// owner deliberately has no object-store deletion authority.
struct PreCommitUploads {
    /// Exact successfully uploaded identities in ordinal order.
    paths: Vec<String>,
    /// False only after ownership transfers into [`ScribeCommitAttempt`].
    armed: bool,
}

impl PreCommitUploads {
    /// Creates an armed owner before the first object mutation.
    fn new(capacity: usize) -> Self {
        Self {
            paths: Vec::with_capacity(capacity),
            armed: true,
        }
    }

    /// Records one completed deterministic upload.
    fn record(&mut self, path: String) {
        self.paths.push(path);
    }

    /// Releases process-local attempt identities while retaining remote retry evidence.
    fn cleanup(&mut self) {
        self.paths.clear();
        self.armed = false;
    }

    /// Transfers remote-object ownership to the returned commit attempt.
    fn disarm(mut self) {
        self.armed = false;
    }
}

impl Drop for PreCommitUploads {
    /// Retains remote evidence when cancellation interrupts a known-uncommitted stage.
    fn drop(&mut self) {
        self.paths.clear();
    }
}

/// Capability returned by `pre_commit` and consumed after the SQL transaction commits.
///
/// The `shard_id` records which pod-local shard lane owns the frozen generation.
/// Under batch-spread routing, a seal key may be spread across shards, so the
/// `shard_id` must be threaded from the shard that performed the freeze through
/// `complete_post_commit` and `abort_post_commit` to avoid recomputing the route.
#[derive(Debug)]
pub struct PostCommitToken {
    /// Local immutable generation identity.
    pub seal_id: u64,
    /// Seal scope represented by the generation.
    pub seal_key: SealKey,
    /// Pod-local shard lane that owns this frozen generation.
    pub shard_id: usize,
    /// Exact durable file-list identity.
    pub file_list_key: FileListCommitKey,
    /// Durable file-list row identity.
    pub file_list_row_id: Uuid,
    /// Ordered public batch identities represented by this immutable generation.
    pub batch_ids: Vec<Uuid>,
    /// Inclusive minimum WAL LSN in the generation.
    pub wal_lsn_min: WalLsn,
    /// Inclusive maximum WAL LSN in the generation.
    pub wal_lsn_max: WalLsn,
    /// Arrow bytes transferred from the writable to immutable tier.
    pub memtable_bytes: usize,
    /// Move-owned production visibility trace terminalized with this capability.
    pub(super) visibility: VisibilityPublishGuard,
}

/// Move-owned terminal guard for one durable Scribe visibility publication.
#[derive(Debug)]
pub(super) struct VisibilityPublishGuard {
    /// Production span retained until the durable lifecycle reaches one terminal.
    span: tracing::Span,
    /// Fallback terminal recorded when the owner is dropped before explicit completion.
    drop_outcome: &'static str,
    /// Whether an explicit terminal has already been recorded.
    terminal: bool,
}

impl VisibilityPublishGuard {
    /// Starts one production visibility publication with failure as the pre-commit fallback.
    #[must_use]
    pub(super) fn new() -> Self {
        Self {
            span: tracing::info_span!(
                "bifrost.scribe.visibility.publish",
                outcome = tracing::field::Empty
            ),
            drop_outcome: "failed",
            terminal: false,
        }
    }

    /// Changes abandonment after successful pre-commit into cancellation.
    pub(super) fn arm_cancellation(&mut self) {
        self.drop_outcome = "cancelled";
    }

    /// Records the exact successful lifecycle terminal once.
    pub(super) fn succeed(&mut self) {
        self.finish("success");
    }

    /// Records the exact failed lifecycle terminal once.
    pub(super) fn fail(&mut self) {
        self.finish("failed");
    }

    /// Records the exact cancelled lifecycle terminal once.
    pub(super) fn cancel(&mut self) {
        self.finish("cancelled");
    }

    /// Records one terminal outcome while preventing double terminalization.
    fn finish(&mut self, outcome: &'static str) {
        if !self.terminal {
            self.span.record("outcome", outcome);
            self.terminal = true;
        }
    }
}

impl Drop for VisibilityPublishGuard {
    /// Records abandonment or task cancellation before closing the production span.
    fn drop(&mut self) {
        if !self.terminal {
            self.span.record("outcome", self.drop_outcome);
            self.terminal = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use crate::catalog::TableRef;
    use crate::namespaces::BifrostNamespace;
    use crate::scribe::file_list_writer::FileListCommitKey;
    use crate::scribe::parquet_writer::{BoundedParquetArtifact, BoundedParquetArtifactSet};
    use crate::scribe::seal::{PostCommitBatch, PostCommitToken};
    use crate::scribe::seal_key::SealKey;
    use crate::scribe::wal::WalLsn;

    use wyrd_spec::DataTenantId;

    use crate::test_support::{SpanCaptureSubscriber, has_span_outcome};

    /// Cancellation retains remote evidence and releases only local attempt identities.
    #[tokio::test]
    async fn partial_upload_owner_retains_remote_evidence_on_drop() {
        let operator = Arc::new(
            opendal::Operator::new(opendal::services::Memory::default())
                .expect("memory operator")
                .finish(),
        );
        let first = "tenant/generation/artifact-00000.parquet";
        let second = "tenant/generation/artifact-00001.parquet";
        let unrelated = "tenant/other/artifact.parquet";
        operator
            .write(first, bytes::Bytes::from_static(b"first"))
            .await
            .expect("first upload");
        operator
            .write(second, bytes::Bytes::from_static(b"partial"))
            .await
            .expect("second partial upload");
        operator
            .write(unrelated, bytes::Bytes::from_static(b"unrelated"))
            .await
            .expect("unrelated upload");
        let mut uploads = super::PreCommitUploads::new(2);
        uploads.record(first.to_owned());
        uploads.record(second.to_owned());
        drop(uploads);
        assert!(operator.exists(first).await.expect("first existence"));
        assert!(operator.exists(second).await.expect("second existence"));
        assert!(
            operator
                .exists(unrelated)
                .await
                .expect("unrelated existence")
        );
    }

    /// Force-seal conversion exposes only the post-commit capability batch.
    #[test]
    fn force_seal_result_is_only_a_post_commit_capability_batch() {
        let tenant = DataTenantId::new_v7();
        let table = TableRef::new(BifrostNamespace::Bifrost, "events");
        let key = SealKey::new(
            tenant,
            table,
            crate::test_support::day_partition(2026, 7, 14),
        );
        let token = PostCommitToken {
            seal_id: 9,
            seal_key: key,
            shard_id: 0,
            file_list_key: FileListCommitKey {
                data_tenant_id: tenant,
                namespace: "vala.bifrost".to_owned(),
                table_name: "events".to_owned(),
                node_id: uuid::Uuid::nil(),
                writer_epoch: 1,
                wal_lsn_min: 11,
                wal_lsn_max: 11,
            },
            file_list_row_id: uuid::Uuid::nil(),
            batch_ids: Vec::new(),
            wal_lsn_min: WalLsn::new(11),
            wal_lsn_max: WalLsn::new(11),
            memtable_bytes: 0,
            visibility: super::VisibilityPublishGuard::new(),
        };
        let seal_id = token.seal_id;
        let batch: PostCommitBatch = token.into();
        assert_eq!(batch.0.len(), 1);
        assert_eq!(batch.0[0].seal_id, seal_id);
        assert_eq!(batch.0[0].wal_lsn_max, WalLsn::new(11));
    }

    /// Explicit success closes the exact production visibility span once.
    #[test]
    fn visibility_guard_closes_production_span_on_success() {
        let subscriber = SpanCaptureSubscriber::default();
        let records = Arc::clone(&subscriber.records);
        let closed = Arc::clone(&subscriber.closed);
        tracing::subscriber::with_default(subscriber, || {
            let mut guard = super::VisibilityPublishGuard::new();
            guard.succeed();
            assert!(closed.lock().expect("closed spans").is_empty());
            drop(guard);
        });
        assert!(has_span_outcome(
            &records,
            "bifrost.scribe.visibility.publish",
            "success"
        ));
        assert_eq!(
            *closed.lock().expect("closed spans"),
            vec!["bifrost.scribe.visibility.publish"]
        );
    }

    /// Armed abandonment closes the exact production visibility span as cancelled.
    #[test]
    fn visibility_guard_closes_production_span_on_cancellation() {
        let subscriber = SpanCaptureSubscriber::default();
        let records = Arc::clone(&subscriber.records);
        tracing::subscriber::with_default(subscriber, || {
            let mut guard = super::VisibilityPublishGuard::new();
            guard.arm_cancellation();
            drop(guard);
        });
        assert!(has_span_outcome(
            &records,
            "bifrost.scribe.visibility.publish",
            "cancelled"
        ));
    }

    /// Explicit failure closes the exact production visibility span as failed.
    #[test]
    fn visibility_guard_closes_production_span_on_failure() {
        let subscriber = SpanCaptureSubscriber::default();
        let records = Arc::clone(&subscriber.records);
        tracing::subscriber::with_default(subscriber, || {
            let mut guard = super::VisibilityPublishGuard::new();
            guard.fail();
            drop(guard);
        });
        assert!(has_span_outcome(
            &records,
            "bifrost.scribe.visibility.publish",
            "failed"
        ));
    }

    /// Dropping an unsettled COMMIT attempt retains evidence and poisons Scribe.
    #[test]
    fn dropped_unsettled_scribe_commit_attempt_fail_stops_owner() {
        let memory =
            crate::scribe::embedded_scribe_resources(&crate::scribe::AdmissionConfig::default());
        let tenant = DataTenantId::new_v7();
        let seal_key = SealKey::new(
            tenant,
            TableRef::new(BifrostNamespace::Bifrost, "events"),
            crate::test_support::day_partition(2026, 8, 14),
        );
        let token = PostCommitToken {
            seal_id: 12,
            seal_key,
            shard_id: 0,
            file_list_key: FileListCommitKey {
                data_tenant_id: tenant,
                namespace: "vala.bifrost".to_owned(),
                table_name: "events".to_owned(),
                node_id: uuid::Uuid::nil(),
                writer_epoch: 1,
                wal_lsn_min: 1,
                wal_lsn_max: 1,
            },
            file_list_row_id: uuid::Uuid::nil(),
            batch_ids: Vec::new(),
            wal_lsn_min: WalLsn::new(1),
            wal_lsn_max: WalLsn::new(1),
            memtable_bytes: 0,
            visibility: super::VisibilityPublishGuard::new(),
        };
        let artifacts = BoundedParquetArtifactSet::encoded(vec![BoundedParquetArtifact {
            ordinal: 0,
            scratch_path: std::path::PathBuf::from("unused-test-artifact"),
            object_identity: "s3://bucket/object-00000.parquet".to_owned(),
            file_size: 1,
            checksum: "0".repeat(64),
            row_count: 1,
            row_group_stats: Vec::new(),
        }])
        .expect("nonempty artifact set");
        let retained_bytes = 4_096;
        let parquet_owner = memory
            .try_reserve_maintenance(
                crate::scribe::memory::MemoryCategory::Persistence,
                retained_bytes,
            )
            .expect("ambiguous Parquet owner");
        drop(super::ScribeCommitAttempt {
            token: Some(token),
            rows: Vec::new(),
            audit_events: Vec::new(),
            artifacts: Some(artifacts),
            memory: Some(memory.clone()),
            parquet_owner: Some(parquet_owner),
            completion: None,
            settled: false,
        });
        assert!(memory.is_poisoned());
        assert_eq!(memory.memory_snapshot().total_bytes(), retained_bytes);
    }
}

/// A set of post-commit capabilities produced by one force-seal call.
#[derive(Debug)]
pub struct PostCommitBatch(pub Vec<PostCommitToken>);

impl From<PostCommitToken> for PostCommitBatch {
    fn from(token: PostCommitToken) -> Self {
        Self(vec![token])
    }
}

/// Move-only authority returned before the caller resolves its SQL COMMIT.
#[derive(Debug)]
pub struct ScribeCommitAttempt {
    /// Post-commit lifecycle capability.
    pub(crate) token: Option<PostCommitToken>,
    /// Exact rows used for idempotent full-set reconciliation.
    pub(crate) rows: Vec<file_list_writer::FileListArtifactInsert>,
    /// Exact audit transition paired with the generation publication.
    pub(crate) audit_events: Vec<wyrd_spec::vala::api::AuditEvent>,
    /// Uploaded object identities and generation scratch authority.
    pub(crate) artifacts: Option<BoundedParquetArtifactSet>,
    /// Shared Scribe owner poisoned if the attempt is abandoned unsettled.
    pub(crate) memory: Option<ScribeResources>,
    /// Checked delta completing the immutable charge into one governed immutable owner.
    pub(crate) parquet_owner: Option<ScribeMemoryLease>,
    /// Production owner that completes immutable retirement after reconciliation.
    pub(crate) completion: Option<ScribeCommitCompletion>,
    /// Explicit terminal marker suppressing fail-stop drop behavior.
    pub(crate) settled: bool,
}

/// Dependencies required to complete one reconciled caller-owned publication.
#[derive(Debug, Clone)]
pub(crate) struct ScribeCommitCompletion {
    /// Fixed shard runtime that owns immutable generation retirement.
    pub(crate) shards: Arc<crate::scribe::shards::ScribeShardRuntime>,
    /// Optional local Forge wake-up emitted after retirement.
    pub(crate) staging_file_publisher: Option<crate::maintenance::StagingFilePublisher>,
    /// Test-tier lifecycle observer preserved across asynchronous reconciliation.
    #[cfg(feature = "test-support")]
    pub(crate) publication_observer: super::ScribePublicationObserver,
}

impl ScribeCommitCompletion {
    /// Completes the exact immutable generation after durable publication.
    ///
    /// # Errors
    /// Returns a Scribe error when binding resolution or shard retirement fails.
    async fn complete(&self, mut token: PostCommitToken) -> Result<(), ScribeError> {
        let binding =
            TenantTableBinding::resolve((token.seal_key.tenant, token.seal_key.table.clone()))
                .map_err(|error| ScribeError::Internal {
                    detail: error.to_string(),
                })?;
        self.shards
            .complete_post_commit(
                token.seal_id,
                token.shard_id,
                &token.seal_key,
                token.memtable_bytes,
                token.file_list_key.clone(),
            )
            .await?;
        #[cfg(feature = "test-support")]
        self.publication_observer
            .record(super::ScribePublicationEvent::Published {
                seal_id: token.seal_id,
                file_list_row_id: token.file_list_row_id,
                batch_ids: token.batch_ids.clone(),
                table: token.seal_key.table.fqn(),
            });
        if let Some(publisher) = &self.staging_file_publisher {
            let _ = publisher.try_publish(crate::maintenance::StagingFileCommitted::new(
                binding,
                token.seal_key.partition,
            ));
        }
        token.visibility.succeed();
        Ok(())
    }
}

impl ScribeCommitAttempt {
    /// Attaches the production immutable-retirement owner before COMMIT polling.
    pub(crate) fn attach_completion(&mut self, completion: ScribeCommitCompletion) {
        self.completion = Some(completion);
    }

    /// Reconciles an ambiguous COMMIT until exact publication is observed.
    ///
    /// The runtime owns this future independently of the caller. Cancellation
    /// of the caller therefore cannot drop objects, scratch, or WAL authority.
    ///
    /// # Errors
    /// Returns only after durable publication succeeds but immutable retirement
    /// or exact scratch cleanup fails. SQL errors are retried with bounded delay.
    pub(crate) async fn reconcile(
        mut self,
        reconciler: &crate::scribe::persistence::ScribePublicationReconciler,
    ) -> Result<(), ScribeError> {
        loop {
            if matches!(
                reconciler.publish(&self.rows, &self.audit_events).await,
                crate::scribe::persistence::ScribePublicationOutcome::Committed(_)
            ) {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
        let completion = self
            .completion
            .take()
            .ok_or_else(|| ScribeError::Internal {
                detail: "ambiguous Scribe attempt lacks its completion owner".to_owned(),
            })?;
        let (token, artifacts, parquet_owner) = self.take_terminal();
        completion.complete(token).await?;
        artifacts.cleanup()?;
        drop(parquet_owner);
        Ok(())
    }
    /// Returns the pending post-commit token for test-support telemetry.
    #[cfg(feature = "test-support")]
    #[must_use]
    pub(crate) fn token(&self) -> Option<&PostCommitToken> {
        self.token.as_ref()
    }

    /// Marks the attempt terminal and extracts all move-owned capabilities.
    ///
    /// # Panics
    /// Panics only when an internal caller settles the same move-only attempt
    /// twice, which is prevented by consuming `self` at the public seam.
    pub(crate) fn take_terminal(
        mut self,
    ) -> (
        PostCommitToken,
        BoundedParquetArtifactSet,
        ScribeMemoryLease,
    ) {
        self.settled = true;
        let token = self
            .token
            .take()
            .expect("unsettled Scribe commit attempt owns its token");
        let artifacts = self
            .artifacts
            .take()
            .expect("unsettled Scribe commit attempt owns its artifacts");
        let parquet_owner = self
            .parquet_owner
            .take()
            .expect("unsettled Scribe commit attempt owns its Parquet reservation");
        (token, artifacts, parquet_owner)
    }
}

impl Drop for ScribeCommitAttempt {
    /// Retains ambiguous evidence and fail-stops Scribe if settlement is skipped.
    fn drop(&mut self) {
        if self.settled {
            return;
        }
        if let Some(artifacts) = self.artifacts.take() {
            artifacts.retain_for_reconciliation();
        }
        if let Some(parquet_owner) = self.parquet_owner.take() {
            // An unavailable reconciliation queue is fail-stop, not a release
            // boundary. The process restart path reconstructs the durable
            // generation from WAL; until then this exact scalable owner must
            // remain charged rather than becoming reusable admission capacity.
            std::mem::forget(parquet_owner);
        }
        if let Some(memory) = &self.memory {
            memory.poison();
        }
        if let Some(token) = &mut self.token {
            token.visibility.fail();
        }
        tracing::error!("unsettled Scribe COMMIT attempt retained and poisoned its owner");
    }
}

/// Seal state machine for one seal-key.
///
/// States: `Freeze` → `WriteParquet` → `PutObject` → `AtomicPgTx` → `ManifestUpdate` → `RetireWal`
#[derive(Debug)]
pub struct SealDriver {
    operator: Arc<Operator>,
    persistence_cpu: ScribePersistenceCpuPool,
    /// Generation-owned output scratch authority required before encoding.
    output_scratch: Option<Arc<crate::resources::ScratchVolume>>,
    /// Shared Scribe owner fail-stopped by an abandoned COMMIT attempt.
    memory: Option<ScribeResources>,
}

/// Parquet stage output retained through object upload and SQL staging.
struct PreparedSealEncoding {
    /// Encoded artifacts plus their audit and append metadata.
    encoded: ParquetEncoded,
    /// Aggregate encoded artifact bytes used for stage telemetry.
    encoded_bytes: usize,
    /// Producer reservation retained until the commit attempt settles.
    parquet_owner: ScribeMemoryLease,
}

/// Borrowed inputs one Parquet encode of a frozen generation needs.
///
/// These travel together from [`SealDriver::prepare_seal_encoding`] into the
/// persistence CPU lane and are grouped so the encode entry point stays a
/// single cohesive request rather than a positional argument list.
struct ParquetEncodeRequest<'a> {
    /// Frozen generation whose batches are encoded.
    frozen: &'a FrozenMemtable,
    /// Tenant-qualified destination table binding.
    binding: &'a TenantTableBinding,
    /// Authenticated tenant the encoded rows belong to.
    tenant: wyrd_spec::ids::DataTenantId,
    /// Registered physical layout reproducing the table's writer recipe.
    layout: std::sync::Arc<crate::catalog::layout::PhysicalLayout>,
    /// Admitted scratch directory the encoder writes through.
    scratch_dir: &'a std::path::Path,
    /// Deterministic object-store prefix every artifact is keyed under.
    object_base: &'a str,
}

impl SealDriver {
    /// Encodes one frozen generation under its scratch and memory owners.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError`] when memory or scratch admission, node identity,
    /// artifact identity, encoding, or encoded-size accounting fails.
    async fn prepare_seal_encoding(
        &self,
        frozen: &FrozenMemtable,
        seal_key: &SealKey,
        binding: &TenantTableBinding,
        conn: &mut TenantConn<'_>,
        node_id: &str,
        writer_epoch: i64,
    ) -> Result<PreparedSealEncoding, ScribeError> {
        let layout = crate::scribe::write_recipe::resolve_write_recipe(
            &mut **conn.transaction(),
            binding,
            &frozen.schema,
        )
        .await?;
        let memory = self.memory.as_ref().ok_or_else(|| ScribeError::Internal {
            detail: "Scribe writer-v2 memory owner is unavailable before encoding".to_owned(),
        })?;
        let mut parquet_owner = memory.try_reserve_maintenance(
            MemoryCategory::Persistence,
            parquet_candidate_incremental_bytes(
                crate::scribe::parquet_writer::largest_candidate_bytes(&frozen.batches)?,
            )?,
        )?;
        info!("seal stage: WriteParquet");
        let parquet_started = std::time::Instant::now();
        let output_scratch = self
            .output_scratch
            .as_ref()
            .ok_or_else(|| ScribeError::Internal {
                detail: "Scribe output scratch is unavailable before seal encoding".to_owned(),
            })?;
        let scratch_bytes =
            u64::try_from(parquet_owner.bytes()).map_err(|_| ScribeError::Internal {
                detail: "Scribe seal scratch ownership exceeds u64".to_owned(),
            })?;
        let node_uuid = Uuid::parse_str(node_id).map_err(|error| ScribeError::Internal {
            detail: format!("node_id is not a valid UUID: {error}"),
        })?;
        let scratch = output_scratch
            .create_scribe_generation(
                &node_uuid.simple().to_string(),
                frozen.seal_id,
                scratch_bytes,
            )
            .map_err(|error| ScribeError::Internal {
                detail: format!("Scribe seal scratch admission failed: {error}"),
            })?;
        let object_base = Self::seal_object_base(frozen, seal_key, binding, node_id, writer_epoch)?;
        let footer_reservation = crate::scribe::memory::EncodedFooterReservation::transfer_from(
            &mut parquet_owner,
            frozen.arrow_bytes,
        )?;
        let mut encoded = self
            .encode_parquet(
                ParquetEncodeRequest {
                    frozen,
                    binding,
                    tenant: seal_key.tenant,
                    layout,
                    scratch_dir: scratch.path(),
                    object_base: &object_base,
                },
                footer_reservation,
            )
            .await?;
        encoded.artifacts.attach_scratch(scratch)?;
        encoded.audit_events =
            crate::scribe::audit_envelope::publication_audit_events(&encoded.audit_events);
        let encoded_bytes = encoded
            .artifacts
            .iter()
            .try_fold(0_usize, |sum, artifact| {
                sum.checked_add(usize::try_from(artifact.file_size).ok()?)
            })
            .ok_or_else(|| ScribeError::Internal {
                detail: "Scribe seal artifact-set size overflows".to_owned(),
            })?;
        Self::record(
            "parquet_encode",
            parquet_started.elapsed(),
            frozen.row_count(),
            encoded_bytes,
        );
        Ok(PreparedSealEncoding {
            encoded,
            encoded_bytes,
            parquet_owner,
        })
    }

    /// Construct a new `SealDriver` with the given opendal operator.
    #[must_use]
    pub fn new(operator: Arc<Operator>) -> Self {
        Self::new_with_lane(operator, ScribePersistenceCpuPool::new(1), None, None)
    }

    /// Construct a seal driver using the boot-owned persistence CPU lane.
    #[must_use]
    pub(crate) fn new_with_lane(
        operator: Arc<Operator>,
        persistence_cpu: ScribePersistenceCpuPool,
        output_scratch: Option<Arc<crate::resources::ScratchVolume>>,
        memory: Option<ScribeResources>,
    ) -> Self {
        Self {
            operator,
            persistence_cpu,
            output_scratch,
            memory,
        }
    }

    /// Execute seal stages 1-4 (Freeze → Parquet → PUT → PG tx) and return a
    /// post-commit capability. The caller owns commit/rollback and must
    /// explicitly complete or abort the capability afterward.
    ///
    /// # Errors
    /// Returns [`ScribeError`] if any pre-commit stage fails.
    #[tracing::instrument(skip(self, memtable, conn), fields(seal_key = %seal_key))]
    pub async fn pre_commit(
        &self,
        memtable: &Memtable,
        seal_key: &SealKey,
        binding: &TenantTableBinding,
        conn: &mut TenantConn<'_>,
        node_id: &str,
        writer_epoch: i64,
    ) -> Result<ScribeCommitAttempt, ScribeError> {
        info!("seal stage: Freeze");
        let freeze_started = std::time::Instant::now();
        let frozen = memtable.freeze(seal_key)?;
        Self::record(
            "freeze",
            freeze_started.elapsed(),
            frozen.row_count(),
            frozen.arrow_bytes,
        );
        self.pre_commit_frozen(&frozen, seal_key, binding, conn, node_id, writer_epoch)
            .await
    }

    /// Execute seal stages after the owning shard has detached the frozen
    /// generation from its writable state.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError`] for tenant/binding mismatch, invalid WAL or
    /// schema identity, Parquet planning/encoding/upload failure, capacity
    /// refusal, or an unavailable/ambiguous `PostgreSQL` publication outcome.
    #[tracing::instrument(skip(self, frozen, conn), fields(seal_key = %seal_key))]
    pub async fn pre_commit_frozen(
        &self,
        frozen: &FrozenMemtable,
        seal_key: &SealKey,
        binding: &TenantTableBinding,
        conn: &mut TenantConn<'_>,
        node_id: &str,
        writer_epoch: i64,
    ) -> Result<ScribeCommitAttempt, ScribeError> {
        let mut visibility = VisibilityPublishGuard::new();
        binding
            .validate_authenticated_tenant(conn.data_tenant_id())
            .map_err(|error| ScribeError::Internal {
                detail: error.to_string(),
            })?;
        if binding.table_ref != seal_key.table || binding.tenant != seal_key.tenant {
            return Err(ScribeError::Internal {
                detail: format!(
                    "tenant-table binding does not match seal key: binding tenant/table=({},{}) seal tenant/table=({},{})",
                    binding.tenant, binding.table_ref, seal_key.tenant, seal_key.table
                ),
            });
        }
        let PreparedSealEncoding {
            encoded,
            encoded_bytes,
            parquet_owner,
        } = self
            .prepare_seal_encoding(frozen, seal_key, binding, conn, node_id, writer_epoch)
            .await?;

        // 3. PutObject
        info!("seal stage: PutObject");
        let put_started = std::time::Instant::now();
        let mut uploads = PreCommitUploads::new(encoded.artifacts.len());
        if let Err(error) = self.put_artifacts(&encoded, &mut uploads).await {
            uploads.cleanup();
            return Err(error);
        }
        Self::record(
            "object_store_put",
            put_started.elapsed(),
            frozen.row_count(),
            encoded_bytes,
        );

        // 4. AtomicPgTx
        info!("seal stage: AtomicPgTx");
        let pg_started = std::time::Instant::now();
        let rows = file_list_writer::build_artifact_inserts(
            frozen,
            &encoded,
            binding,
            node_id,
            writer_epoch,
        )?;
        let insert_outcome = match file_list_writer::insert_artifact_set_and_audit(
            conn,
            &rows,
            &encoded.audit_events,
        )
        .await
        {
            Ok(outcome) => outcome,
            Err(error) => {
                uploads.cleanup();
                return Err(ScribeError::from(error));
            }
        };
        Self::record(
            "file_list_transaction",
            pg_started.elapsed(),
            frozen.row_count(),
            encoded_bytes,
        );

        let (wal_lsn_min, wal_lsn_max) = wal_bounds(&encoded.append_metas);

        visibility.arm_cancellation();
        uploads.disarm();
        Ok(ScribeCommitAttempt {
            token: Some(PostCommitToken {
                seal_id: frozen.seal_id,
                seal_key: seal_key.clone(),
                // Carry the recorded shard lane through to post-commit routing.
                shard_id: frozen.shard_id,
                file_list_key: insert_outcome.commit_key,
                file_list_row_id: rows[0].id,
                batch_ids: frozen
                    .metas
                    .iter()
                    .map(|meta| Uuid::from_bytes(meta.batch_id))
                    .collect(),
                wal_lsn_min,
                wal_lsn_max,
                memtable_bytes: frozen.arrow_bytes,
                visibility,
            }),
            rows,
            audit_events: encoded.audit_events,
            artifacts: Some(encoded.artifacts),
            memory: self.memory.clone(),
            parquet_owner: Some(parquet_owner),
            completion: None,
            settled: false,
        })
    }

    /// Derives the deterministic object-base prefix every artifact of this
    /// seal is written under.
    ///
    /// The prefix binds the generation's exact time partition, writing node,
    /// writer epoch, shard, and the inclusive WAL LSN span it covers, so two
    /// generations can never collide on one key and a replayed generation
    /// reproduces the same base byte for byte.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError`] when the artifact identity is not constructible
    /// from the binding, partition, node, or epoch.
    fn seal_object_base(
        frozen: &FrozenMemtable,
        seal_key: &SealKey,
        binding: &TenantTableBinding,
        node_id: &str,
        writer_epoch: i64,
    ) -> Result<String, ScribeError> {
        let wal_lsn_min = frozen
            .metas
            .iter()
            .map(|meta| meta.wal_lsn_min.as_u64())
            .min()
            .unwrap_or(0);
        let wal_lsn_max = frozen
            .metas
            .iter()
            .map(|meta| meta.wal_lsn_max.as_u64())
            .max()
            .unwrap_or(0);
        Ok(ScribeArtifactIdentity::new(
            binding,
            seal_key.partition,
            node_id,
            writer_epoch,
            frozen.shard_id,
            wal_lsn_min,
            wal_lsn_max,
        )?
        .object_base())
    }

    /// Encodes one frozen generation through the admitted persistence CPU lane.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError`] when CPU admission, Parquet encoding, footer
    /// ownership, or detached task completion fails.
    async fn encode_parquet(
        &self,
        request: ParquetEncodeRequest<'_>,
        footer_reservation: crate::scribe::memory::EncodedFooterReservation,
    ) -> Result<ParquetEncoded, ScribeError> {
        let ParquetEncodeRequest {
            frozen,
            binding,
            tenant,
            layout,
            scratch_dir,
            object_base,
        } = request;
        match self
            .persistence_cpu
            .submit(ScribePersistenceCpuOp::EncodeParquet(Box::new(
                crate::scribe::execution_lanes::EncodeParquetOp {
                    frozen: Box::new(frozen.clone()),
                    binding: binding.clone(),
                    tenant,
                    candidate: None,
                    first_ordinal: 0,
                    layout,
                    scratch_dir: scratch_dir.to_path_buf(),
                    object_base: object_base.to_owned(),
                    footer_reservation,
                },
            )))
            .await?
        {
            ScribePersistenceCpuResult::ParquetEncoded(encoded) => Ok(encoded),
            ScribePersistenceCpuResult::Prepared(_) => Err(ScribeError::Internal {
                detail: "persistence lane returned the wrong seal result".to_owned(),
            }),
            ScribePersistenceCpuResult::ReplayRestored(_) => Err(ScribeError::Internal {
                detail: "persistence lane returned replay output during seal".to_owned(),
            }),
        }
    }

    fn record(stage: &str, elapsed: std::time::Duration, rows: usize, bytes: usize) {
        metrics::histogram!("bifrost_scribe_seal_stage_seconds", "stage" => stage.to_owned())
            .record(elapsed.as_secs_f64());
        metrics::counter!("bifrost_scribe_seal_rows_total", "stage" => stage.to_owned())
            .increment(u64::try_from(rows).unwrap_or(u64::MAX));
        metrics::counter!("bifrost_scribe_seal_bytes_total", "stage" => stage.to_owned())
            .increment(u64::try_from(bytes).unwrap_or(u64::MAX));
    }

    /// Streams every ordered artifact to storage with bounded chunks and retries.
    ///
    /// # Errors
    /// Returns [`ScribeError::ObjectStorePutFailed`] on non-transient failures.
    async fn put_artifacts(
        &self,
        encoded: &ParquetEncoded,
        uploads: &mut PreCommitUploads,
    ) -> Result<(), ScribeError> {
        for artifact in &encoded.artifacts {
            let mut terminal = None;
            for attempt in 0..5_u32 {
                let upload = async {
                    use tokio::io::AsyncReadExt;

                    let mut source = tokio::fs::File::open(&artifact.scratch_path)
                        .await
                        .map_err(|error| {
                            opendal::Error::new(ErrorKind::Unexpected, "open Scribe scratch")
                                .set_source(error)
                        })?;
                    let mut writer = self
                        .operator
                        .writer_with(&artifact.object_identity)
                        .chunk(8 * 1024 * 1024)
                        .await?;
                    let mut chunk = vec![0_u8; 8 * 1024 * 1024];
                    let mut uploaded = 0_u64;
                    loop {
                        let read = source.read(&mut chunk).await.map_err(|error| {
                            opendal::Error::new(ErrorKind::Unexpected, "read Scribe scratch")
                                .set_source(error)
                        })?;
                        if read == 0 {
                            break;
                        }
                        writer.write(Bytes::copy_from_slice(&chunk[..read])).await?;
                        uploaded = uploaded.saturating_add(u64::try_from(read).unwrap_or(u64::MAX));
                    }
                    let metadata = writer.close().await?;
                    if uploaded != artifact.file_size
                        || metadata.content_length() != artifact.file_size
                    {
                        return Err(opendal::Error::new(
                            ErrorKind::Unexpected,
                            "Scribe artifact upload length mismatch",
                        ));
                    }
                    Ok(())
                };
                match tokio::time::timeout(std::time::Duration::from_secs(30), upload).await {
                    Ok(Ok(())) => {
                        terminal = None;
                        break;
                    }
                    Ok(Err(error)) => terminal = Some(ScribeError::ObjectStorePutFailed(error)),
                    Err(_) => {
                        terminal = Some(ScribeError::ObjectStorePutFailed(opendal::Error::new(
                            ErrorKind::Unexpected,
                            "Scribe artifact upload timed out",
                        )));
                    }
                }
                if attempt < 4 {
                    tokio::time::sleep(std::time::Duration::from_millis(100 * 2_u64.pow(attempt)))
                        .await;
                }
            }
            if let Some(error) = terminal {
                return Err(error);
            }
            uploads.record(artifact.object_identity.clone());
            info!(path = %artifact.object_identity, bytes = artifact.file_size, "Parquet PUT succeeded");
        }
        Ok(())
    }
}

/// Returns the inclusive WAL range covered by one encoded artifact set.
fn wal_bounds(metas: &[crate::scribe::wal::ScribeAppendMeta]) -> (WalLsn, WalLsn) {
    let minimum = metas
        .iter()
        .map(|meta| meta.wal_lsn_min)
        .min()
        .unwrap_or_else(|| WalLsn::new(0));
    let maximum = metas
        .iter()
        .map(|meta| meta.wal_lsn_max)
        .max()
        .unwrap_or_else(|| WalLsn::new(0));
    (minimum, maximum)
}
