//! Crash-recoverable local staging and single-mover election for Scribe artifacts.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
use uuid::Uuid;

use crate::contracts::ScribeError;
use crate::parquet::object_uploader::ParquetObjectIdentity;
use crate::scribe::file_list_writer::FileListArtifactInsert;
use crate::scribe::promotion::ScribePublishedHotFileV1;
use crate::scribe::stream_identity::StreamIdentity;
use wyrd_spec::DataTenantId;
use wyrd_spec::vala::api::AuditEvent;

/// Version of the durable Scribe publication-manifest contract.
const PUBLICATION_MANIFEST_VERSION: u8 = 2;
/// Maximum durable metadata accepted during fail-closed startup recovery.
const MAX_PUBLICATION_MANIFEST_BYTES: u64 = 16 * 1024 * 1024;

/// Manifest-fixed claim owned by the one elected Scribe mover.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub(crate) struct StagedArtifactClaim {
    /// Stable logical member and artifact ordinal identity.
    pub(crate) logical_identity: String,
    /// Unique local encoding attempt elected for this identity.
    pub(crate) attempt_id: Uuid,
    /// Finalized local Parquet path retained through catalog publication.
    pub(crate) staged_path: PathBuf,
    /// Deterministic remote key fixed before election.
    pub(crate) object_key: String,
    /// Manifest-fixed SHA-256 digest.
    pub(crate) sha256: [u8; 32],
    /// Manifest-fixed byte length.
    pub(crate) length: u64,
    /// Exact durable local bytes charged to the WAL volume.
    pub(crate) accounted_bytes: u64,
}

/// Durable attempt facts written before competing persistors elect a winner.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
struct AttemptManifest {
    /// Stable logical member and artifact ordinal identity.
    logical_identity: String,
    /// Unique encoding attempt identity.
    attempt_id: Uuid,
    /// Deterministic remote object key.
    object_key: String,
    /// Manifest-fixed SHA-256 digest.
    sha256: [u8; 32],
    /// Manifest-fixed byte length.
    length: u64,
}

/// Durable generation/member transaction facts required for catalog reconciliation.
#[derive(Clone, Debug, Serialize, Deserialize)]
struct PublicationManifest {
    /// Closed manifest format version validated before any external side effect.
    version: u8,
    /// Stable generation/member identity shared by every artifact winner.
    logical_identity: String,
    /// Fenced actor node authorized to publish the rows.
    actor_node_id: Uuid,
    /// Fenced actor epoch authorized to publish the rows.
    actor_writer_epoch: i64,
    /// Deterministically ordered file-list rows published as one transaction.
    rows: Vec<DurableFileListRow>,
    /// Exact canonical audit events appended in the same fenced transaction.
    audit_events: Vec<AuditEvent>,
    /// Elected local artifacts retained until catalog convergence.
    claims: Vec<StagedArtifactClaim>,
}

/// Versioned staging projection of one canonical typed file-list insert.
#[derive(Clone, Debug, Serialize, Deserialize)]
struct DurableFileListRow {
    /// Durable row identity.
    id: Uuid,
    /// Tenant owning the artifact.
    data_tenant_id: DataTenantId,
    /// Logical namespace.
    namespace: String,
    /// Logical table name.
    table_name: String,
    /// Deterministic remote object key.
    file_path: String,
    /// Exact artifact bytes.
    file_size: i64,
    /// Exact represented rows.
    row_count: i64,
    /// Minimum event time.
    min_event_time: chrono::DateTime<chrono::Utc>,
    /// Maximum event time.
    max_event_time: chrono::DateTime<chrono::Utc>,
    /// Exact typed time partition of the staged artifact.
    ///
    /// Stored as the public wire value so the manifest carries the same
    /// granularity/start pair the WAL, object path, and file-list row carry,
    /// and so deserialization re-validates the canonical boundary.
    partition: wyrd_spec::vala::api::TimePartitionWire,
    /// Producing node.
    node_id: Uuid,
    /// Producing fenced epoch.
    writer_epoch: i64,
    /// Inclusive first WAL position.
    wal_lsn_min: i64,
    /// Inclusive final WAL position.
    wal_lsn_max: i64,
    /// Contiguous artifact ordinal.
    file_ordinal: i16,
    /// Lowercase SHA-256 artifact digest.
    file_checksum: String,
    /// Iceberg-ready promotion evidence published with the row.
    ///
    /// The manifest carries it so a recovered publication commits the exact
    /// record the sealing writer derived, rather than one re-derived from an
    /// object the recovering process would have to re-read.
    promotion_record: ScribePublishedHotFileV1,
}

impl From<&FileListArtifactInsert> for DurableFileListRow {
    /// Copies one typed file-list insert into its versioned staging representation.
    ///
    /// The conversion preserves every persisted identity, fence, WAL range, and
    /// artifact checksum so recovery can reconstruct the exact SQL transaction.
    fn from(row: &FileListArtifactInsert) -> Self {
        Self {
            id: row.id,
            data_tenant_id: row.data_tenant_id,
            namespace: row.namespace.clone(),
            table_name: row.table_name.clone(),
            file_path: row.file_path.clone(),
            file_size: row.file_size,
            row_count: row.row_count,
            min_event_time: row.min_event_time,
            max_event_time: row.max_event_time,
            partition: row.partition.to_wire(),
            node_id: row.node_id,
            writer_epoch: row.writer_epoch,
            wal_lsn_min: row.wal_lsn_min,
            wal_lsn_max: row.wal_lsn_max,
            file_ordinal: row.file_ordinal,
            file_checksum: row.file_checksum.clone(),
            promotion_record: row.promotion_record.clone(),
        }
    }
}

impl From<DurableFileListRow> for FileListArtifactInsert {
    /// Restores one typed file-list insert from a validated staging row.
    ///
    /// The conversion is field-for-field and performs no normalization, leaving
    /// publication validation responsible for rejecting contradictory manifests.
    fn from(row: DurableFileListRow) -> Self {
        Self {
            id: row.id,
            data_tenant_id: row.data_tenant_id,
            namespace: row.namespace,
            table_name: row.table_name,
            file_path: row.file_path,
            file_size: row.file_size,
            row_count: row.row_count,
            min_event_time: row.min_event_time,
            max_event_time: row.max_event_time,
            partition: crate::catalog::layout::TimePartition::from_wire(row.partition),
            node_id: row.node_id,
            writer_epoch: row.writer_epoch,
            wal_lsn_min: row.wal_lsn_min,
            wal_lsn_max: row.wal_lsn_max,
            file_ordinal: row.file_ordinal,
            file_checksum: row.file_checksum,
            promotion_record: row.promotion_record,
        }
    }
}

/// Validated startup publication that can reconcile without replaying Arrow.
#[derive(Clone, Debug)]
pub(crate) struct RecoveredPublication {
    /// Stable generation/member identity.
    pub(crate) logical_identity: String,
    /// Fenced actor whose live membership authorizes publication.
    pub(crate) actor_stream: StreamIdentity,
    /// Exact deterministic file-list rows.
    pub(crate) rows: Vec<FileListArtifactInsert>,
    /// Exact audit fan-out paired with the rows.
    pub(crate) audit_events: Vec<AuditEvent>,
    /// Finalized local winners retained through publication.
    pub(crate) claims: Vec<StagedArtifactClaim>,
}

/// Outcome of atomic local-stage election.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum StageElection {
    /// This attempt created the winner marker and owns the mover lifecycle.
    Winner(StagedArtifactClaim),
    /// Another attempt already owns the logical stage; this attempt was cleaned locally.
    ExistingWinner(StagedArtifactClaim),
}

/// Owner of Scribe's durable local stage namespace.
#[derive(Clone, Debug)]
pub(crate) struct ScribeStaging {
    /// WAL-root-relative namespace containing attempt, winner, and publication files.
    root: PathBuf,
    /// Optional WAL owner that charges and retires staged bytes in production.
    wal: Option<Arc<crate::scribe::wal::WalWriter>>,
}

/// Provisional durable-stage charge retained fail-stop if cancellation follows mutation.
struct PendingStageGrowth {
    /// Exact provisional WAL-volume capability reserved before the first write.
    growth: Option<crate::resources::WalVolumeGrowth>,
    /// Whether an async filesystem operation may already have mutated the namespace.
    mutation_possible: bool,
}

impl PendingStageGrowth {
    /// Wraps an optional production growth capability before local IO starts.
    fn new(growth: Option<crate::resources::WalVolumeGrowth>) -> Self {
        Self {
            growth,
            mutation_possible: false,
        }
    }

    /// Marks the capability conservative before the first cancellable write.
    fn arm(&mut self) {
        self.mutation_possible = true;
    }

    /// Commits the exact charge after file and parent-directory fsync complete.
    ///
    /// # Errors
    ///
    /// Returns an internal error when volume accounting contradicts the reserved growth.
    fn commit(mut self) -> Result<(), ScribeError> {
        if let Some(growth) = self.growth.take() {
            growth.commit().map_err(|error| ScribeError::Internal {
                detail: format!("commit Scribe staged WAL-volume growth: {error}"),
            })?;
        }
        self.mutation_possible = false;
        Ok(())
    }
}

impl Drop for PendingStageGrowth {
    /// Retains and poisons a possibly materialized but uncommitted stage charge.
    fn drop(&mut self) {
        if self.mutation_possible
            && let Some(growth) = self.growth.take()
        {
            growth.retain_and_poison();
        }
    }
}

impl ScribeStaging {
    /// Creates the local stage owner under the WAL recovery root.
    pub(crate) fn new(wal: Arc<crate::scribe::wal::WalWriter>) -> Self {
        Self {
            root: wal.base_dir().join("staged"),
            wal: Some(wal),
        }
    }

    /// Creates an ungoverned staging owner for isolated filesystem unit tests.
    #[cfg(test)]
    fn ungoverned_for_test(wal_root: &Path) -> Self {
        Self {
            root: wal_root.join("staged"),
            wal: None,
        }
    }

    /// Copies one encoded attempt with bounded caller storage, fsyncs its manifest,
    /// and atomically elects one mover for the logical artifact.
    ///
    /// # Errors
    ///
    /// Returns an internal error for unsafe identities or any local durable IO failure.
    ///
    /// # Cancellation
    ///
    /// Cancellation may leave an fsynced attempt, winner marker, or temporary
    /// stage. Recovery uses those durable boundaries to finish the elected rename;
    /// losing attempts remain safe to remove on a later election.
    pub(crate) async fn stage_and_elect(
        &self,
        logical_identity: &str,
        source: &Path,
        object_key: &ParquetObjectIdentity,
        sha256: [u8; 32],
        length: u64,
        chunk: &mut [u8],
    ) -> Result<StageElection, ScribeError> {
        if chunk.is_empty() || !safe_component(logical_identity) {
            return Err(ScribeError::Internal {
                detail: "invalid or ungoverned Scribe stage identity".to_owned(),
            });
        }
        tokio::fs::create_dir_all(&self.root)
            .await
            .map_err(stage_io("create staged namespace"))?;
        let attempt_id = Uuid::new_v4();
        let prefix = format!("{logical_identity}.{attempt_id}");
        let temporary = self.root.join(format!("{prefix}.par.tmp"));
        let manifest_path = self.root.join(format!("{prefix}.attempt.json"));
        let manifest = AttemptManifest {
            logical_identity: logical_identity.to_owned(),
            attempt_id,
            object_key: object_key.as_str().to_owned(),
            sha256,
            length,
        };
        let manifest_bytes =
            serde_json::to_vec(&manifest).map_err(|error| ScribeError::Internal {
                detail: format!("encode Scribe attempt manifest: {error}"),
            })?;
        let winner_bytes = manifest_path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| ScribeError::Internal {
                detail: "Scribe attempt manifest has no stable filename".to_owned(),
            })?
            .len() as u64;
        let accounted_bytes = length
            .checked_add(manifest_bytes.len() as u64)
            .and_then(|bytes| bytes.checked_add(winner_bytes))
            .ok_or_else(|| ScribeError::Internal {
                detail: "Scribe staged volume charge overflow".to_owned(),
            })?;
        let growth = self
            .wal
            .as_ref()
            .map(|wal| wal.reserve_staged_growth(accounted_bytes))
            .transpose()?
            .flatten();
        let mut growth = PendingStageGrowth::new(growth);
        growth.arm();
        copy_fsynced(source, &temporary, sha256, length, chunk).await?;
        sync_directory(&self.root).await?;
        write_manifest_fsynced(&manifest_path, &manifest).await?;
        sync_directory(&self.root).await?;

        let winner_path = self.root.join(format!("{logical_identity}.winner"));
        match tokio::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&winner_path)
            .await
        {
            Ok(mut winner) => {
                winner
                    .write_all(
                        manifest_path
                            .file_name()
                            .and_then(|name| name.to_str())
                            .ok_or_else(|| ScribeError::Internal {
                                detail: "Scribe attempt manifest has no stable filename".to_owned(),
                            })?
                            .as_bytes(),
                    )
                    .await
                    .map_err(stage_io("write winner marker"))?;
                winner
                    .sync_all()
                    .await
                    .map_err(stage_io("fsync winner marker"))?;
                sync_directory(&self.root).await?;
                let finalized = self.root.join(format!("{prefix}.parquet"));
                tokio::fs::rename(&temporary, &finalized)
                    .await
                    .map_err(stage_io("finalize elected stage"))?;
                sync_directory(&self.root).await?;
                growth.commit()?;
                Ok(StageElection::Winner(claim(
                    &manifest,
                    finalized,
                    accounted_bytes,
                )))
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                remove_if_present(&temporary).await?;
                remove_if_present(&manifest_path).await?;
                sync_directory(&self.root).await?;
                // No bytes from this attempt remain durable. Dropping the
                // provisional capability releases it without ever creating a
                // competing retirement authority.
                growth.mutation_possible = false;
                Ok(StageElection::ExistingWinner(
                    self.load_winner(&winner_path, chunk).await?,
                ))
            }
            Err(error) => Err(stage_io("elect Scribe stage")(error)),
        }
    }

    /// Recovers elected stages and classifies every incomplete local attempt.
    ///
    /// # Errors
    ///
    /// Returns an internal error when its caller supplies no governed buffer, a
    /// winner or attempt manifest is missing, or finalized bytes contradict the
    /// manifest's exact length or SHA-256.
    ///
    /// # Cancellation
    ///
    /// Pre-manifest temporary files are proven orphans and removed. Complete
    /// manifests without a winner re-enter the same create-new election; elected
    /// attempts finish rename, and non-winning attempts remove only their own
    /// files. Cancellation retains every unresolved durable boundary for retry.
    pub(crate) async fn recover(
        &self,
        chunk: &mut [u8],
    ) -> Result<Vec<StagedArtifactClaim>, ScribeError> {
        if chunk.is_empty() {
            return Err(ScribeError::Internal {
                detail: "Scribe stage recovery requires a governed buffer".to_owned(),
            });
        }
        let mut claims = Vec::new();
        let mut entries = match tokio::fs::read_dir(&self.root).await {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(claims),
            Err(error) => return Err(stage_io("scan staged namespace")(error)),
        };
        let mut paths = Vec::new();
        while let Some(entry) = entries
            .next_entry()
            .await
            .map_err(stage_io("scan stage entry"))?
        {
            paths.push(entry.path());
        }
        paths.sort();

        let winner_paths = paths
            .iter()
            .filter(|path| path.extension().and_then(|value| value.to_str()) == Some("winner"))
            .cloned()
            .collect::<Vec<_>>();
        for winner_path in &winner_paths {
            claims.push(self.load_winner(winner_path, chunk).await?);
        }

        let manifest_paths = paths
            .iter()
            .filter(|path| {
                path.file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name.ends_with(".attempt.json"))
            })
            .cloned()
            .collect::<Vec<_>>();
        for manifest_path in &manifest_paths {
            let manifest = read_attempt_manifest(manifest_path).await?;
            validate_attempt_manifest_path(&manifest, manifest_path)?;
            let winner_path = self
                .root
                .join(format!("{}.winner", manifest.logical_identity));
            let winner_names_manifest = winner_paths.iter().any(|path| path == &winner_path)
                && tokio::fs::read_to_string(&winner_path)
                    .await
                    .map_err(stage_io("read winner marker during orphan classification"))?
                    == manifest_path
                        .file_name()
                        .and_then(|name| name.to_str())
                        .ok_or_else(|| ScribeError::Internal {
                            detail: "attempt manifest lacks a safe filename".to_owned(),
                        })?;
            if winner_names_manifest {
                continue;
            }
            match self
                .elect_recovered_manifest(&manifest, manifest_path, chunk)
                .await?
            {
                Some(claim) => claims.push(claim),
                None => {
                    self.cleanup_recovered_attempt(&manifest, manifest_path)
                        .await?;
                }
            }
        }

        for path in paths {
            let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
                continue;
            };
            let is_attempt_payload = name.ends_with(".par.tmp") || name.ends_with(".parquet");
            if !is_attempt_payload {
                continue;
            }
            let manifest_name = if let Some(prefix) = name.strip_suffix(".par.tmp") {
                format!("{prefix}.attempt.json")
            } else if let Some(prefix) = name.strip_suffix(".parquet") {
                format!("{prefix}.attempt.json")
            } else {
                continue;
            };
            if tokio::fs::metadata(self.root.join(manifest_name))
                .await
                .is_err()
            {
                self.remove_recovered_bytes(&path).await?;
            }
        }
        sync_directory(&self.root).await?;
        claims.sort_by(|left, right| left.logical_identity.cmp(&right.logical_identity));
        claims.dedup_by(|left, right| left.logical_identity == right.logical_identity);
        Ok(claims)
    }

    /// Re-enters create-new election for one fsynced pre-election manifest.
    ///
    /// Returns `Some` for the winner and `None` when another attempt already won.
    ///
    /// # Errors
    ///
    /// Returns an internal error when winner creation, fsync, or elected rename fails.
    async fn elect_recovered_manifest(
        &self,
        manifest: &AttemptManifest,
        manifest_path: &Path,
        chunk: &mut [u8],
    ) -> Result<Option<StagedArtifactClaim>, ScribeError> {
        let winner_path = self
            .root
            .join(format!("{}.winner", manifest.logical_identity));
        let manifest_name = manifest_path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| ScribeError::Internal {
                detail: "recovered attempt manifest lacks a safe filename".to_owned(),
            })?;
        let winner_bytes =
            u64::try_from(manifest_name.len()).map_err(|_| ScribeError::Internal {
                detail: "recovered winner marker length overflow".to_owned(),
            })?;
        let growth = self
            .wal
            .as_ref()
            .map(|wal| wal.reserve_staged_growth(winner_bytes))
            .transpose()?
            .flatten();
        let mut growth = PendingStageGrowth::new(growth);
        growth.arm();
        match tokio::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&winner_path)
            .await
        {
            Ok(mut winner) => {
                winner
                    .write_all(manifest_name.as_bytes())
                    .await
                    .map_err(stage_io("write recovered winner marker"))?;
                winner
                    .sync_all()
                    .await
                    .map_err(stage_io("fsync recovered winner marker"))?;
                sync_directory(&self.root).await?;
                growth.commit()?;
                Ok(Some(self.load_winner(&winner_path, chunk).await?))
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                growth.mutation_possible = false;
                Ok(None)
            }
            Err(error) => Err(stage_io("elect recovered Scribe stage")(error)),
        }
    }

    /// Removes one proven non-winning attempt and retires only its exact local bytes.
    ///
    /// # Errors
    ///
    /// Returns an internal error when metadata, removal, fsync, or volume retirement fails.
    async fn cleanup_recovered_attempt(
        &self,
        manifest: &AttemptManifest,
        manifest_path: &Path,
    ) -> Result<(), ScribeError> {
        let prefix = format!("{}.{}", manifest.logical_identity, manifest.attempt_id);
        for path in [
            self.root.join(format!("{prefix}.par.tmp")),
            self.root.join(format!("{prefix}.parquet")),
            manifest_path.to_path_buf(),
        ] {
            self.remove_recovered_bytes(&path).await?;
        }
        sync_directory(&self.root).await
    }

    /// Removes one exact recovered path and retires its measured durable charge.
    ///
    /// # Errors
    ///
    /// Returns an internal error for unsafe paths, metadata/removal failure, or
    /// volume-accounting contradiction.
    async fn remove_recovered_bytes(&self, path: &Path) -> Result<(), ScribeError> {
        if path.parent() != Some(self.root.as_path()) {
            return Err(ScribeError::Internal {
                detail: "recovered stage cleanup escaped its namespace".to_owned(),
            });
        }
        let bytes = match tokio::fs::metadata(path).await {
            Ok(metadata) => metadata.len(),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(stage_io("stat recovered orphan")(error)),
        };
        remove_if_present(path).await?;
        sync_directory(&self.root).await?;
        if let Some(wal) = &self.wal {
            wal.retire_staged_bytes(bytes)?;
        }
        Ok(())
    }

    /// Atomically persists the complete generation/member publication transaction.
    ///
    /// The manifest is written only after every artifact has won or joined the
    /// same local election. It precedes upload, so an ambiguous PUT or process
    /// loss can be reconciled without reconstructing Arrow state.
    ///
    /// # Errors
    ///
    /// Returns an internal error when publication facts contradict elected
    /// artifacts or the durable atomic write/fsync sequence fails.
    ///
    /// # Cancellation
    ///
    /// Cancellation before the final rename leaves no visible publication;
    /// cancellation after it leaves a complete manifest for startup recovery.
    pub(crate) async fn persist_publication(
        &self,
        logical_identity: &str,
        actor_stream: StreamIdentity,
        rows: &[FileListArtifactInsert],
        audit_events: &[AuditEvent],
        claims: &[StagedArtifactClaim],
    ) -> Result<(), ScribeError> {
        validate_publication(logical_identity, actor_stream, rows, audit_events, claims)?;
        let manifest = PublicationManifest {
            version: PUBLICATION_MANIFEST_VERSION,
            logical_identity: logical_identity.to_owned(),
            actor_node_id: actor_stream.node_id.as_uuid(),
            actor_writer_epoch: actor_stream.writer_epoch.as_i64(),
            rows: rows.iter().map(DurableFileListRow::from).collect(),
            audit_events: audit_events.to_vec(),
            claims: claims.to_vec(),
        };
        let final_path = self
            .root
            .join(format!("{logical_identity}.publication.json"));
        let temporary_path = self
            .root
            .join(format!("{logical_identity}.publication.tmp"));
        let manifest_bytes =
            serde_json::to_vec(&manifest).map_err(|error| ScribeError::Internal {
                detail: format!("encode Scribe publication manifest: {error}"),
            })?;
        let manifest_len =
            u64::try_from(manifest_bytes.len()).map_err(|_| ScribeError::Internal {
                detail: "Scribe publication manifest length overflow".to_owned(),
            })?;
        if manifest_len > MAX_PUBLICATION_MANIFEST_BYTES {
            return Err(ScribeError::Internal {
                detail: "Scribe publication manifest exceeds recovery bound".to_owned(),
            });
        }
        match tokio::fs::metadata(&final_path).await {
            Ok(_) => {
                let retained = read_bounded_publication_manifest(&final_path).await?;
                if retained == manifest_bytes {
                    return Ok(());
                }
                return Err(ScribeError::Internal {
                    detail: "Scribe publication manifest identity contradiction".to_owned(),
                });
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(stage_io("stat Scribe publication manifest")(error)),
        }
        let growth = self
            .wal
            .as_ref()
            .map(|wal| wal.reserve_staged_growth(manifest_len))
            .transpose()?
            .flatten();
        let mut growth = PendingStageGrowth::new(growth);
        growth.arm();
        write_bytes_fsynced(&temporary_path, &manifest_bytes).await?;
        sync_directory(&self.root).await?;
        tokio::fs::rename(&temporary_path, &final_path)
            .await
            .map_err(stage_io("publish Scribe generation manifest"))?;
        sync_directory(&self.root).await?;
        growth.commit()
    }

    /// Loads and validates every durable publication before startup readiness.
    ///
    /// # Errors
    ///
    /// Returns an internal error for a missing caller-owned governed buffer, an
    /// unknown version, malformed JSON, unsafe identity, missing finalized
    /// artifact, or contradictory row, digest, length, ordinal, actor-fence,
    /// or audit facts.
    ///
    /// # Cancellation
    ///
    /// Cancellation stops the scan without mutating staged artifacts or
    /// publication manifests; the full validation pass is safe to retry.
    pub(crate) async fn recover_publications(
        &self,
        chunk: &mut [u8],
    ) -> Result<Vec<RecoveredPublication>, ScribeError> {
        if chunk.is_empty() {
            return Err(ScribeError::Internal {
                detail: "Scribe publication recovery requires a governed buffer".to_owned(),
            });
        }
        let mut recovered = Vec::new();
        let mut entries = match tokio::fs::read_dir(&self.root).await {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(recovered),
            Err(error) => return Err(stage_io("scan publication manifests")(error)),
        };
        while let Some(entry) = entries
            .next_entry()
            .await
            .map_err(stage_io("scan publication manifest entry"))?
        {
            let path = entry.path();
            if path
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.ends_with(".publication.tmp"))
            {
                self.remove_recovered_bytes(&path).await?;
                continue;
            }
            if !path
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.ends_with(".publication.json"))
            {
                continue;
            }
            let bytes = read_bounded_publication_manifest(&path).await?;
            let manifest: PublicationManifest =
                serde_json::from_slice(&bytes).map_err(|error| ScribeError::Internal {
                    detail: format!("decode Scribe publication manifest: {error}"),
                })?;
            if manifest.version != PUBLICATION_MANIFEST_VERSION {
                return Err(ScribeError::Internal {
                    detail: "unsupported Scribe publication manifest version".to_owned(),
                });
            }
            let actor_stream = StreamIdentity::new(
                crate::scribe::stream_identity::NodeId::new(manifest.actor_node_id),
                crate::scribe::stream_identity::WriterEpoch::new(manifest.actor_writer_epoch),
            );
            validate_publication(
                &manifest.logical_identity,
                actor_stream,
                &manifest
                    .rows
                    .iter()
                    .cloned()
                    .map(FileListArtifactInsert::from)
                    .collect::<Vec<_>>(),
                &manifest.audit_events,
                &manifest.claims,
            )?;
            for claim in &manifest.claims {
                verify_staged_artifact(&claim.staged_path, claim.sha256, claim.length, chunk)
                    .await?;
            }
            recovered.push(RecoveredPublication {
                logical_identity: manifest.logical_identity,
                actor_stream,
                rows: manifest
                    .rows
                    .into_iter()
                    .map(FileListArtifactInsert::from)
                    .collect(),
                audit_events: manifest.audit_events,
                claims: manifest.claims,
            });
        }
        recovered.sort_by(|left, right| left.logical_identity.cmp(&right.logical_identity));
        Ok(recovered)
    }

    /// Removes only a finalized local stage after durable catalog convergence.
    ///
    /// # Errors
    ///
    /// Returns an internal error when exact local cleanup or parent fsync fails.
    ///
    /// # Cancellation
    ///
    /// Cancellation may leave a subset of exact local files removed. Repeating
    /// cleanup is safe because absence is accepted and staged-byte retirement
    /// occurs only after the namespace fsync completes.
    pub(crate) async fn cleanup_published(
        &self,
        claim: &StagedArtifactClaim,
    ) -> Result<(), ScribeError> {
        for path in [
            claim.staged_path.clone(),
            self.root.join(format!(
                "{}.{}.attempt.json",
                claim.logical_identity, claim.attempt_id
            )),
            self.root.join(format!("{}.winner", claim.logical_identity)),
        ] {
            self.remove_recovered_bytes(&path).await?;
        }
        Ok(())
    }

    /// Removes a converged generation/member publication manifest.
    ///
    /// # Errors
    ///
    /// Returns an internal error when exact removal or parent fsync fails.
    ///
    /// # Cancellation
    ///
    /// Cancellation may occur after removal but before the directory fsync. A
    /// retry is idempotent and re-establishes the durable namespace boundary.
    pub(crate) async fn cleanup_publication(
        &self,
        logical_identity: &str,
    ) -> Result<(), ScribeError> {
        if !safe_component(logical_identity) {
            return Err(ScribeError::Internal {
                detail: "unsafe Scribe publication cleanup identity".to_owned(),
            });
        }
        self.remove_recovered_bytes(
            &self
                .root
                .join(format!("{logical_identity}.publication.json")),
        )
        .await
    }

    /// Resolves one durable winner marker into its finalized claim.
    ///
    /// # Errors
    ///
    /// Returns an internal error when the marker or manifest is unsafe,
    /// unreadable, malformed, cannot complete its elected rename, or when the
    /// finalized bytes contradict their manifest length or SHA-256.
    ///
    /// # Cancellation
    ///
    /// Cancellation may interrupt the rename/fsync sequence, but preserves the
    /// winner and attempt manifest needed to retry the same recovery.
    async fn load_winner(
        &self,
        winner_path: &Path,
        chunk: &mut [u8],
    ) -> Result<StagedArtifactClaim, ScribeError> {
        let manifest_name = tokio::fs::read_to_string(winner_path)
            .await
            .map_err(stage_io("read winner marker"))?;
        if !safe_component(&manifest_name) {
            return Err(ScribeError::Internal {
                detail: "winner marker references an unsafe manifest".to_owned(),
            });
        }
        let manifest_path = self.root.join(manifest_name);
        let manifest = read_attempt_manifest(&manifest_path).await?;
        validate_attempt_manifest_path(&manifest, &manifest_path)?;
        let prefix = format!("{}.{}", manifest.logical_identity, manifest.attempt_id);
        let temporary = self.root.join(format!("{prefix}.par.tmp"));
        let finalized = self.root.join(format!("{prefix}.parquet"));
        if tokio::fs::metadata(&finalized).await.is_err() {
            tokio::fs::rename(&temporary, &finalized)
                .await
                .map_err(stage_io("recover elected stage rename"))?;
            sync_directory(&self.root).await?;
        }
        verify_staged_artifact(&finalized, manifest.sha256, manifest.length, chunk).await?;
        let accounted_bytes = manifest.length
            + tokio::fs::metadata(&manifest_path)
                .await
                .map_err(stage_io("stat attempt manifest"))?
                .len()
            + tokio::fs::metadata(winner_path)
                .await
                .map_err(stage_io("stat winner marker"))?
                .len();
        Ok(claim(&manifest, finalized, accounted_bytes))
    }
}

/// Reads one publication manifest without permitting unbounded startup allocation.
///
/// # Errors
///
/// Returns an internal error when the file cannot be opened or read, or when
/// its bytes exceed [`MAX_PUBLICATION_MANIFEST_BYTES`].
///
/// # Cancellation
///
/// Cancellation abandons only the in-memory read buffer and leaves the durable
/// manifest unchanged for a later recovery attempt.
async fn read_bounded_publication_manifest(path: &Path) -> Result<Vec<u8>, ScribeError> {
    let file = tokio::fs::File::open(path)
        .await
        .map_err(stage_io("open publication manifest"))?;
    let mut bytes = Vec::new();
    file.take(MAX_PUBLICATION_MANIFEST_BYTES + 1)
        .read_to_end(&mut bytes)
        .await
        .map_err(stage_io("read publication manifest"))?;
    if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > MAX_PUBLICATION_MANIFEST_BYTES {
        return Err(ScribeError::Internal {
            detail: "Scribe publication manifest exceeds recovery bound".to_owned(),
        });
    }
    Ok(bytes)
}

/// Reads one bounded attempt manifest during startup classification.
///
/// # Errors
///
/// Returns an internal error when the file cannot be read within the shared
/// durable-metadata bound or its JSON shape is invalid.
async fn read_attempt_manifest(path: &Path) -> Result<AttemptManifest, ScribeError> {
    let bytes = read_bounded_publication_manifest(path).await?;
    serde_json::from_slice(&bytes).map_err(|error| ScribeError::Internal {
        detail: format!("decode Scribe attempt manifest: {error}"),
    })
}

/// Proves an attempt manifest's durable filename matches its embedded identity.
///
/// # Errors
///
/// Returns an internal error for unsafe identity components or a filename that
/// does not equal `<logical>.<attempt>.attempt.json`.
fn validate_attempt_manifest_path(
    manifest: &AttemptManifest,
    path: &Path,
) -> Result<(), ScribeError> {
    let expected = format!(
        "{}.{}.attempt.json",
        manifest.logical_identity, manifest.attempt_id
    );
    if !safe_component(&manifest.logical_identity)
        || path.file_name().and_then(|name| name.to_str()) != Some(expected.as_str())
    {
        return Err(ScribeError::Internal {
            detail: "attempt manifest path contradicts its embedded identity".to_owned(),
        });
    }
    Ok(())
}

/// Validates one generation/member transaction without performing IO.
///
/// # Errors
///
/// Returns [`ScribeError::Internal`] when the publication is incomplete, uses
/// an unsafe identity, exceeds the durable ordinal range, contradicts an
/// elected artifact, or crosses generation, member, or actor-fence identity.
fn validate_publication(
    logical_identity: &str,
    actor_stream: StreamIdentity,
    rows: &[FileListArtifactInsert],
    audit_events: &[AuditEvent],
    claims: &[StagedArtifactClaim],
) -> Result<(), ScribeError> {
    if !safe_component(logical_identity)
        || rows.is_empty()
        || audit_events.is_empty()
        || rows.len() != claims.len()
    {
        return Err(ScribeError::Internal {
            detail: "incomplete Scribe generation publication manifest".to_owned(),
        });
    }
    for (ordinal, (row, claim)) in rows.iter().zip(claims).enumerate() {
        let expected_ordinal = i16::try_from(ordinal).map_err(|_| ScribeError::Internal {
            detail: "Scribe publication ordinal exceeds durable range".to_owned(),
        })?;
        if row.file_ordinal != expected_ordinal
            || row.node_id != actor_stream.node_id.as_uuid()
            || row.writer_epoch != actor_stream.writer_epoch.as_i64()
            || row.file_path != claim.object_key
            || row.file_checksum != hex::encode(claim.sha256)
            || row.file_size != i64::try_from(claim.length).unwrap_or(-1)
            || !claim.logical_identity.starts_with(logical_identity)
        {
            return Err(ScribeError::Internal {
                detail: "Scribe publication manifest identity contradiction".to_owned(),
            });
        }
    }
    let first = &rows[0];
    if rows.iter().any(|row| {
        row.data_tenant_id != first.data_tenant_id
            || row.namespace != first.namespace
            || row.table_name != first.table_name
            || row.node_id != first.node_id
            || row.writer_epoch != first.writer_epoch
            || row.wal_lsn_min != first.wal_lsn_min
            || row.wal_lsn_max != first.wal_lsn_max
    }) {
        return Err(ScribeError::Internal {
            detail: "Scribe publication manifest crosses generation/member identity".to_owned(),
        });
    }
    Ok(())
}

/// Builds the public claim from one validated durable attempt manifest.
fn claim(
    manifest: &AttemptManifest,
    staged_path: PathBuf,
    accounted_bytes: u64,
) -> StagedArtifactClaim {
    StagedArtifactClaim {
        logical_identity: manifest.logical_identity.clone(),
        attempt_id: manifest.attempt_id,
        staged_path,
        object_key: manifest.object_key.clone(),
        sha256: manifest.sha256,
        length: manifest.length,
        accounted_bytes,
    }
}

/// Copies and hashes one exact local artifact before manifest publication.
///
/// # Errors
///
/// Returns an internal error for source/destination IO, length overflow, or a
/// copied length or SHA-256 digest that contradicts the manifest expectation.
///
/// # Cancellation
///
/// Cancellation may leave an incomplete temporary stage. Because no winner
/// references it yet, a retry or losing-attempt cleanup can safely replace it.
async fn copy_fsynced(
    source: &Path,
    destination: &Path,
    expected_sha256: [u8; 32],
    expected_len: u64,
    chunk: &mut [u8],
) -> Result<(), ScribeError> {
    let mut input = tokio::fs::File::open(source)
        .await
        .map_err(stage_io("open encoded stage"))?;
    let mut output = tokio::fs::File::create(destination)
        .await
        .map_err(stage_io("create attempt stage"))?;
    let mut written = 0_u64;
    let mut digest = Sha256::new();
    loop {
        let read = input
            .read(chunk)
            .await
            .map_err(stage_io("read encoded stage"))?;
        if read == 0 {
            break;
        }
        output
            .write_all(&chunk[..read])
            .await
            .map_err(stage_io("write attempt stage"))?;
        digest.update(&chunk[..read]);
        written = written
            .checked_add(read as u64)
            .ok_or_else(|| ScribeError::Internal {
                detail: "Scribe staged length overflow".to_owned(),
            })?;
    }
    let actual_sha256: [u8; 32] = digest.finalize().into();
    if written != expected_len || actual_sha256 != expected_sha256 {
        return Err(ScribeError::Internal {
            detail: "Scribe staged bytes contradict manifest".to_owned(),
        });
    }
    output
        .sync_all()
        .await
        .map_err(stage_io("fsync attempt stage"))
}

/// Re-streams one finalized stage through the caller-owned bounded buffer.
///
/// The verifier proves both manifest dimensions before an elected claim can
/// become visible to a mover or startup publication recovery.
///
/// # Errors
///
/// Returns an internal error when the artifact cannot be read, the buffer is
/// empty, its length overflows, or its exact length or SHA-256 contradicts the
/// manifest.
///
/// # Cancellation
///
/// Cancellation does not mutate the finalized artifact or its durable markers;
/// a later recovery retries the complete verification before returning a claim.
async fn verify_staged_artifact(
    path: &Path,
    expected_sha256: [u8; 32],
    expected_len: u64,
    chunk: &mut [u8],
) -> Result<(), ScribeError> {
    if chunk.is_empty() {
        return Err(ScribeError::Internal {
            detail: "Scribe stage verification requires a governed buffer".to_owned(),
        });
    }
    let mut input = tokio::fs::File::open(path)
        .await
        .map_err(stage_io("open finalized stage for verification"))?;
    let mut read_bytes = 0_u64;
    let mut digest = Sha256::new();
    loop {
        let read = input
            .read(chunk)
            .await
            .map_err(stage_io("read finalized stage for verification"))?;
        if read == 0 {
            break;
        }
        digest.update(&chunk[..read]);
        read_bytes = read_bytes
            .checked_add(read as u64)
            .ok_or_else(|| ScribeError::Internal {
                detail: "Scribe staged verification length overflow".to_owned(),
            })?;
    }
    let actual_sha256: [u8; 32] = digest.finalize().into();
    if read_bytes != expected_len || actual_sha256 != expected_sha256 {
        return Err(ScribeError::Internal {
            detail: "Scribe finalized stage bytes contradict manifest".to_owned(),
        });
    }
    Ok(())
}

/// Writes and fsyncs one attempt manifest.
///
/// # Errors
///
/// Returns an internal error when serialization, creation, writing, or fsync fails.
///
/// # Cancellation
///
/// Cancellation may leave an incomplete unreferenced manifest; election does
/// not expose it until the subsequent winner marker is durably created.
async fn write_manifest_fsynced<T: Serialize>(
    path: &Path,
    manifest: &T,
) -> Result<(), ScribeError> {
    let bytes = serde_json::to_vec(manifest).map_err(|error| ScribeError::Internal {
        detail: format!("encode Scribe attempt manifest: {error}"),
    })?;
    write_bytes_fsynced(path, &bytes).await
}

/// Writes and fsyncs one already bounded manifest representation.
///
/// # Errors
///
/// Returns an internal error when creation, writing, or fsync fails.
///
/// # Cancellation
///
/// Cancellation may leave a partial file. The caller must retain its preceding
/// growth capability and let startup classification reconcile the exact path.
async fn write_bytes_fsynced(path: &Path, bytes: &[u8]) -> Result<(), ScribeError> {
    let mut file = tokio::fs::File::create(path)
        .await
        .map_err(stage_io("create attempt manifest"))?;
    file.write_all(bytes)
        .await
        .map_err(stage_io("write attempt manifest"))?;
    file.sync_all()
        .await
        .map_err(stage_io("fsync attempt manifest"))
}

/// Fsyncs the staging directory after a namespace mutation.
///
/// # Errors
///
/// Returns an internal error when the directory cannot be opened or fsynced.
///
/// # Cancellation
///
/// Cancellation leaves the preceding namespace mutation potentially
/// non-durable; callers must retry before advancing the workflow boundary.
async fn sync_directory(path: &Path) -> Result<(), ScribeError> {
    tokio::fs::File::open(path)
        .await
        .map_err(stage_io("open stage directory"))?
        .sync_all()
        .await
        .map_err(stage_io("fsync stage directory"))
}

/// Removes one exact local attempt path while accepting restart-idempotent absence.
///
/// # Errors
///
/// Returns an internal error for removal failures other than an absent path.
///
/// # Cancellation
///
/// Cancellation leaves the path either present or absent; repeating the exact
/// removal is safe in both states.
async fn remove_if_present(path: &Path) -> Result<(), ScribeError> {
    match tokio::fs::remove_file(path).await {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(stage_io("remove exact local stage")(error)),
    }
}

/// Rejects path separators and normalization components in local stage identity.
fn safe_component(value: &str) -> bool {
    !value.is_empty()
        && value != "."
        && value != ".."
        && !value.contains(['/', '\\'])
        && !value.chars().any(char::is_control)
}

/// Converts one local staging IO failure into the crate error boundary.
fn stage_io(operation: &'static str) -> impl FnOnce(std::io::Error) -> ScribeError {
    move |error| ScribeError::Internal {
        detail: format!("{operation}: {error}"),
    }
}

#[cfg(test)]
mod tests {

    /// Builds one publication record standing for a fixture object.
    ///
    /// The manifest round-trip only needs a record that encodes and compares
    /// exactly, so the metrics are the empty projection of a one-row object
    /// rather than a footer-derived one.
    fn fixture_promotion_record(
        tenant: DataTenantId,
        object_key: &str,
        checksum: &str,
        file_size: u64,
        partition: crate::catalog::layout::TimePartition,
        file_list_id: Uuid,
    ) -> ScribePublishedHotFileV1 {
        ScribePublishedHotFileV1::from_metrics(
            &crate::scribe::promotion::PublishedHotFileIdentity {
                data_tenant_id: tenant.as_uuid(),
                namespace: "vala.traces",
                table_name: "spans",
                file_list_id,
                object_key,
                file_checksum: checksum,
                partition,
                schema_fingerprint: "00".repeat(32),
                partition_spec_id: crate::catalog::layout::BIFROST_PARTITION_SPEC_ID,
                sort_order_id: crate::catalog::layout::BIFROST_SORT_ORDER_ID,
            },
            crate::scribe::promotion::ScribeDataFileV1 {
                record_count: 1,
                file_size_in_bytes: file_size,
                column_sizes: std::collections::BTreeMap::new(),
                value_counts: std::collections::BTreeMap::new(),
                null_value_counts: std::collections::BTreeMap::new(),
                nan_value_counts: std::collections::BTreeMap::new(),
                lower_bounds: std::collections::BTreeMap::new(),
                upper_bounds: std::collections::BTreeMap::new(),
                split_offsets: vec![4],
            },
        )
    }
    use sha2::{Digest as _, Sha256};
    use wyrd_spec::auth::{PrincipalId, PrincipalKindTag};
    use wyrd_spec::request_id::RequestId;
    use wyrd_spec::vala::api::{AuditDecision, AuditResult, AuthMethod};

    use super::*;
    use crate::resources::{
        BifrostResourceHealth, BifrostVolumeClass, BifrostVolumeGovernor, BifrostVolumeRoots,
    };
    use crate::scribe::wal::{WalConfig, WalWriter};

    /// Proves one competing attempt wins and restart reconstructs the same mover claim.
    #[tokio::test]
    async fn staged_publication_race_has_one_mover_and_recovers_every_manifest_boundary() {
        let directory = tempfile::tempdir().expect("stage root");
        let source = directory.path().join("source.parquet");
        let content = b"one immutable candidate";
        tokio::fs::write(&source, content)
            .await
            .expect("source write");
        let staging = ScribeStaging::ungoverned_for_test(directory.path());
        let identity =
            ParquetObjectIdentity::new("tenant/table/artifact.parquet").expect("object key");
        let digest = Sha256::digest(content).into();
        let first = staging
            .stage_and_elect(
                "member-0",
                &source,
                &identity,
                digest,
                content.len() as u64,
                &mut [0_u8; 3],
            )
            .await
            .expect("first election");
        let second = staging
            .stage_and_elect(
                "member-0",
                &source,
                &identity,
                digest,
                content.len() as u64,
                &mut [0_u8; 5],
            )
            .await
            .expect("second election");
        assert!(matches!(first, StageElection::Winner(_)));
        assert!(matches!(second, StageElection::ExistingWinner(_)));
        let recovered = staging
            .recover(&mut [0_u8; 3])
            .await
            .expect("restart recovery");
        assert_eq!(recovered.len(), 1);
        assert_eq!(
            tokio::fs::read(&recovered[0].staged_path)
                .await
                .expect("stage read"),
            content
        );
        staging
            .cleanup_published(&recovered[0])
            .await
            .expect("post-publication cleanup");
        assert!(
            staging
                .recover(&mut [0_u8; 3])
                .await
                .expect("empty recovery")
                .is_empty()
        );
    }

    /// Proves a same-length source mutation fails before it can elect a winner.
    ///
    /// # Panics
    ///
    /// Panics when isolated filesystem setup fails, a contradictory copy creates
    /// a winner marker, or recovery cannot reclaim its unreferenced temporary.
    #[tokio::test]
    async fn same_length_source_mutation_fails_before_election_and_is_reclaimable() {
        let directory = tempfile::tempdir().expect("stage root");
        let source = directory.path().join("source.parquet");
        let expected = b"original bytes";
        let mutated = b"mutated! bytes";
        assert_eq!(expected.len(), mutated.len(), "same-length fixture");
        tokio::fs::write(&source, mutated)
            .await
            .expect("mutated source write");
        let staging = ScribeStaging::ungoverned_for_test(directory.path());
        let identity =
            ParquetObjectIdentity::new("tenant/table/artifact.parquet").expect("object key");

        let result = staging
            .stage_and_elect(
                "member-0",
                &source,
                &identity,
                Sha256::digest(expected).into(),
                expected.len() as u64,
                &mut [0_u8; 3],
            )
            .await;
        assert!(result.is_err(), "contradictory source must fail");
        assert!(!staging.root.join("member-0.winner").exists());
        assert!(
            staging
                .recover(&mut [0_u8; 3])
                .await
                .expect("reclaim contradictory temporary")
                .is_empty()
        );
        assert!(!staging.root.join("member-0.winner").exists());
    }

    /// Proves recovery rejects a same-length corrupted finalized winner claim.
    ///
    /// # Panics
    ///
    /// Panics when isolated filesystem setup fails, staging does not elect a
    /// winner, or recovery returns a claim for contradictory finalized bytes.
    #[tokio::test]
    async fn same_length_corrupted_finalized_winner_fails_recovery_without_claim() {
        let directory = tempfile::tempdir().expect("stage root");
        let source = directory.path().join("source.parquet");
        let content = b"trusted stage";
        let corrupt = b"corrupt stage";
        assert_eq!(content.len(), corrupt.len(), "same-length fixture");
        tokio::fs::write(&source, content)
            .await
            .expect("source write");
        let staging = ScribeStaging::ungoverned_for_test(directory.path());
        let identity =
            ParquetObjectIdentity::new("tenant/table/artifact.parquet").expect("object key");
        let claim = match staging
            .stage_and_elect(
                "member-0",
                &source,
                &identity,
                Sha256::digest(content).into(),
                content.len() as u64,
                &mut [0_u8; 3],
            )
            .await
            .expect("stage winner")
        {
            StageElection::Winner(claim) => claim,
            StageElection::ExistingWinner(_) => panic!("fresh stage must elect a winner"),
        };
        tokio::fs::write(&claim.staged_path, corrupt)
            .await
            .expect("corrupt finalized stage");

        assert!(staging.recover(&mut [0_u8; 3]).await.is_err());
        assert!(
            claim.staged_path.exists(),
            "failed recovery retains evidence"
        );
        assert!(staging.root.join("member-0.winner").exists());
    }

    /// Proves the four crash cuts classify only pre-manifest data as
    /// orphan and complete every manifest-backed winner lifecycle.
    ///
    /// # Panics
    ///
    /// Panics when isolated filesystem setup fails or recovery deletes a
    /// manifest-backed attempt, retains an orphan, or fails to finish rename.
    #[tokio::test]
    async fn recovery_covers_pre_manifest_manifest_election_and_rename_crash_cuts() {
        let content = b"crash boundary candidate";
        let digest: [u8; 32] = Sha256::digest(content).into();
        for boundary in 0_u8..4 {
            let directory = tempfile::tempdir().expect("stage root");
            let staging = ScribeStaging::ungoverned_for_test(directory.path());
            tokio::fs::create_dir_all(&staging.root)
                .await
                .expect("stage namespace");
            let logical_identity = "member-0";
            let attempt_id = Uuid::now_v7();
            let prefix = format!("{logical_identity}.{attempt_id}");
            let temporary = staging.root.join(format!("{prefix}.par.tmp"));
            let finalized = staging.root.join(format!("{prefix}.parquet"));
            let manifest_path = staging.root.join(format!("{prefix}.attempt.json"));
            let winner_path = staging.root.join(format!("{logical_identity}.winner"));
            let source = directory.path().join("source.parquet");
            tokio::fs::write(&source, content)
                .await
                .expect("source write");
            copy_fsynced(
                &source,
                &temporary,
                digest,
                content.len() as u64,
                &mut [0_u8; 4],
            )
            .await
            .expect("temporary stage");
            if boundary >= 1 {
                write_manifest_fsynced(
                    &manifest_path,
                    &AttemptManifest {
                        logical_identity: logical_identity.to_owned(),
                        attempt_id,
                        object_key: "tenant/table/candidate.parquet".to_owned(),
                        sha256: digest,
                        length: content.len() as u64,
                    },
                )
                .await
                .expect("attempt manifest");
            }
            if boundary >= 2 {
                tokio::fs::write(
                    &winner_path,
                    manifest_path
                        .file_name()
                        .expect("manifest filename")
                        .to_str()
                        .expect("UTF-8 manifest filename"),
                )
                .await
                .expect("winner marker");
                tokio::fs::File::open(&winner_path)
                    .await
                    .expect("open winner")
                    .sync_all()
                    .await
                    .expect("fsync winner");
            }
            if boundary >= 3 {
                tokio::fs::rename(&temporary, &finalized)
                    .await
                    .expect("finalized rename");
            }
            sync_directory(&staging.root).await.expect("boundary fsync");

            let recovered = staging
                .recover(&mut [0_u8; 4])
                .await
                .expect("restart recovery");
            if boundary == 0 {
                assert!(recovered.is_empty());
                assert!(!temporary.exists());
            } else {
                assert_eq!(recovered.len(), 1, "boundary {boundary}");
                assert_eq!(
                    tokio::fs::read(&recovered[0].staged_path)
                        .await
                        .expect("winner bytes"),
                    content
                );
                assert!(!temporary.exists());
                assert!(finalized.exists());
            }
        }
    }

    /// Proves restart loads a typed publication transaction without Arrow replay.
    #[tokio::test]
    async fn publication_manifest_recovers_rows_audit_and_source_fence_fail_closed() {
        let directory = tempfile::tempdir().expect("stage root");
        let source = directory.path().join("source.parquet");
        let content = b"durable publication candidate";
        tokio::fs::write(&source, content)
            .await
            .expect("source write");
        let staging = ScribeStaging::ungoverned_for_test(directory.path());
        let object_key = "tenant/table/generation.parquet";
        let identity = ParquetObjectIdentity::new(object_key).expect("object key");
        let digest: [u8; 32] = Sha256::digest(content).into();
        let claim = match staging
            .stage_and_elect(
                "generation-7-artifact-0",
                &source,
                &identity,
                digest,
                content.len() as u64,
                &mut [0_u8; 8],
            )
            .await
            .expect("stage election")
        {
            StageElection::Winner(claim) | StageElection::ExistingWinner(claim) => claim,
        };
        let node_id = Uuid::now_v7();
        let actor = StreamIdentity::new(
            crate::scribe::stream_identity::NodeId::new(node_id),
            crate::scribe::stream_identity::WriterEpoch::new(11),
        );
        let tenant = DataTenantId::new_v7();
        let now = chrono::Utc::now();
        let row_id = Uuid::now_v7();
        let rows = vec![FileListArtifactInsert {
            id: row_id,
            data_tenant_id: tenant,
            namespace: "vala.traces".to_owned(),
            table_name: "spans".to_owned(),
            file_path: object_key.to_owned(),
            file_size: i64::try_from(content.len()).expect("fixture length"),
            row_count: 1,
            min_event_time: now,
            max_event_time: now,
            partition: crate::catalog::TimeGranularity::Hour
                .bucket(now)
                .expect("fixture instant buckets"),
            node_id,
            writer_epoch: 11,
            wal_lsn_min: 1,
            wal_lsn_max: 2,
            file_ordinal: 0,
            file_checksum: hex::encode(digest),
            promotion_record: fixture_promotion_record(
                tenant,
                object_key,
                &hex::encode(digest),
                content.len() as u64,
                crate::catalog::TimeGranularity::Hour
                    .bucket(now)
                    .expect("fixture instant buckets"),
                row_id,
            ),
        }];
        let events = vec![AuditEvent::new(
            RequestId::now_v7(),
            None,
            "bifrost.scribe.publish".to_owned(),
            "vala.traces.spans".to_owned(),
            None,
            PrincipalId::new(Uuid::now_v7()),
            PrincipalKindTag::User,
            AuthMethod::Internal,
            "bifrost.scribe".to_owned(),
            AuditDecision::Allow,
            AuditResult::Success,
            "redacted".to_owned(),
        )];
        staging
            .persist_publication("generation-7", actor, &rows, &events, &[claim])
            .await
            .expect("publication manifest");

        let recovered = staging
            .recover_publications(&mut [0_u8; 8])
            .await
            .expect("publication recovery");
        assert_eq!(recovered.len(), 1);
        assert_eq!(recovered[0].actor_stream, actor);
        assert_eq!(recovered[0].rows[0].file_path, object_key);
        assert_eq!(recovered[0].audit_events, events);
    }

    /// Publishes the elected artifact and returns its manifest's durable bytes.
    ///
    /// The publication is written through the production `persist_publication`
    /// path so the manifest the restart later recovers is the real one, not a
    /// fixture approximation. The returned length is the on-disk manifest size,
    /// which the caller folds into the expected durable WAL occupancy.
    ///
    /// # Panics
    ///
    /// Panics if the publication cannot be persisted or its manifest cannot be
    /// stat-ed.
    async fn publish_elected_artifact(
        staging: &ScribeStaging,
        object_key: &str,
        content: &[u8],
        digest: [u8; 32],
        claim: &StagedArtifactClaim,
    ) -> u64 {
        let node_id = Uuid::from_bytes([7_u8; 16]);
        let actor = StreamIdentity::new(
            crate::scribe::stream_identity::NodeId::new(node_id),
            crate::scribe::stream_identity::WriterEpoch::new(11),
        );
        let tenant = DataTenantId::new_v7();
        let now = chrono::Utc::now();
        let row_id = Uuid::now_v7();
        let rows = vec![FileListArtifactInsert {
            id: row_id,
            data_tenant_id: tenant,
            namespace: "vala.traces".to_owned(),
            table_name: "spans".to_owned(),
            file_path: object_key.to_owned(),
            file_size: i64::try_from(content.len()).expect("fixture length"),
            row_count: 1,
            min_event_time: now,
            max_event_time: now,
            partition: crate::catalog::TimeGranularity::Hour
                .bucket(now)
                .expect("fixture instant buckets"),
            node_id,
            writer_epoch: 11,
            wal_lsn_min: 1,
            wal_lsn_max: 2,
            file_ordinal: 0,
            file_checksum: hex::encode(digest),
            promotion_record: fixture_promotion_record(
                tenant,
                object_key,
                &hex::encode(digest),
                content.len() as u64,
                crate::catalog::TimeGranularity::Hour
                    .bucket(now)
                    .expect("fixture instant buckets"),
                row_id,
            ),
        }];
        let events = vec![AuditEvent::new(
            RequestId::now_v7(),
            None,
            "bifrost.scribe.publish".to_owned(),
            "vala.traces.spans".to_owned(),
            None,
            PrincipalId::new(Uuid::now_v7()),
            PrincipalKindTag::User,
            AuthMethod::Internal,
            "bifrost.scribe".to_owned(),
            AuditDecision::Allow,
            AuditResult::Success,
            "redacted".to_owned(),
        )];
        staging
            .persist_publication(
                "generation-9",
                actor,
                &rows,
                &events,
                std::slice::from_ref(claim),
            )
            .await
            .expect("publication manifest");
        let publication_path = staging.root.join("generation-9.publication.json");

        tokio::fs::metadata(&publication_path)
            .await
            .expect("publication metadata")
            .len()
    }

    /// Stages one uncommitted attempt and returns its bytes and manifest path.
    ///
    /// The attempt is deliberately left pending — staged and fsynced, with its
    /// attempt manifest written, but never elected — so the restart has an
    /// unresolved election to reconstruct. Growth is reserved and committed
    /// through the production WAL volume so the durable counter reflects it.
    ///
    /// # Panics
    ///
    /// Panics if growth cannot be reserved or committed, or if the staged copy,
    /// manifest write, or directory fsync fails.
    async fn stage_pending_attempt(
        staging: &ScribeStaging,
        writer: &Arc<WalWriter>,
        source: &std::path::Path,
        digest: [u8; 32],
        content: &[u8],
    ) -> (u64, std::path::PathBuf) {
        let pending_identity = "generation-10-artifact-0";
        let pending_attempt = Uuid::now_v7();
        let pending_prefix = format!("{pending_identity}.{pending_attempt}");
        let pending_path = staging.root.join(format!("{pending_prefix}.par.tmp"));
        let pending_manifest_path = staging.root.join(format!("{pending_prefix}.attempt.json"));
        let pending_manifest = AttemptManifest {
            logical_identity: pending_identity.to_owned(),
            attempt_id: pending_attempt,
            object_key: "tenant/table/recovered-election.parquet".to_owned(),
            sha256: digest,
            length: content.len() as u64,
        };
        let pending_manifest_bytes = serde_json::to_vec(&pending_manifest)
            .expect("pending attempt manifest representation")
            .len() as u64;
        let pending_bytes = (content.len() as u64)
            .checked_add(pending_manifest_bytes)
            .expect("pending stage bytes");
        let pending_growth = writer
            .reserve_staged_growth(pending_bytes)
            .expect("pending stage growth")
            .expect("production WAL volume");
        copy_fsynced(
            source,
            &pending_path,
            digest,
            content.len() as u64,
            &mut [0_u8; 8],
        )
        .await
        .expect("pending stage copy");
        sync_directory(&staging.root)
            .await
            .expect("pending stage directory fsync");
        write_manifest_fsynced(&pending_manifest_path, &pending_manifest)
            .await
            .expect("pending attempt manifest");
        sync_directory(&staging.root)
            .await
            .expect("pending manifest directory fsync");
        pending_growth.commit().expect("pending stage commit");
        (pending_bytes, pending_manifest_path)
    }

    /// Asserts a restart reconstructs the exact namespace and retires it fully.
    ///
    /// Registering a fresh governor over the same roots must reconcile to
    /// exactly the durable bytes the pre-restart run accounted for — no more, so
    /// unauthorized files are excluded, and no less, so authorized staged files
    /// are not lost. Recovery must then resolve the pending election, growing
    /// the counter by exactly the winner marker, and cleaning up every recovered
    /// claim and the publication must return the counter to zero.
    ///
    /// # Panics
    ///
    /// Panics if registration, recovery, or cleanup fails, or if any usage
    /// reading differs from the exact expected occupancy.
    async fn assert_restart_reconstructs_and_retires(
        roots: BifrostVolumeRoots,
        wal_root: &std::path::Path,
        expected: u64,
        claim: &StagedArtifactClaim,
        pending_manifest_path: &std::path::Path,
    ) {
        let restarted =
            BifrostVolumeGovernor::register(roots, 1024 * 1024, BifrostResourceHealth::default())
                .expect("restart volume registration");
        assert_eq!(
            restarted
                .usage_for_test(BifrostVolumeClass::Wal)
                .expect("reconciled usage"),
            (expected, 0, 0),
            "restart counts only WAL and authorized staged files"
        );
        let restarted_writer = Arc::new(
            WalWriter::new_with_volume(
                wal_root,
                [7_u8; 16],
                11,
                WalConfig::default(),
                restarted.capabilities().wal,
            )
            .expect("restarted governed WAL writer"),
        );
        let restarted_staging = ScribeStaging::new(restarted_writer);
        let recovered_claims = restarted_staging
            .recover(&mut [0_u8; 8])
            .await
            .expect("stage recovery");
        let recovered_publications = restarted_staging
            .recover_publications(&mut [0_u8; 8])
            .await
            .expect("publication recovery");
        assert_eq!(recovered_claims.len(), 2);
        assert!(recovered_claims.contains(claim));
        assert_eq!(recovered_publications.len(), 1);
        let recovered_winner_bytes = u64::try_from(
            pending_manifest_path
                .file_name()
                .and_then(|name| name.to_str())
                .expect("pending manifest filename")
                .len(),
        )
        .expect("winner marker bytes");
        assert_eq!(
            restarted
                .usage_for_test(BifrostVolumeClass::Wal)
                .expect("post-election usage"),
            (
                expected
                    .checked_add(recovered_winner_bytes)
                    .expect("post-election durable bytes"),
                0,
                0
            )
        );
        for recovered_claim in &recovered_claims {
            restarted_staging
                .cleanup_published(recovered_claim)
                .await
                .expect("stage cleanup");
        }
        restarted_staging
            .cleanup_publication("generation-9")
            .await
            .expect("publication cleanup");
        assert_eq!(
            restarted
                .usage_for_test(BifrostVolumeClass::Wal)
                .expect("settled usage"),
            (0, 0, 0)
        );
    }

    /// Proves restart reconstructs every authorized stage byte and exact cleanup.
    ///
    /// This uses the production WAL-volume capability on both sides of restart,
    /// retains an elected artifact plus its publication manifest, and verifies
    /// that an unrelated file in the same directory neither enters nor leaves
    /// the durable WAL counter.
    ///
    /// # Panics
    ///
    /// Panics when the production-capability fixture cannot stage, reconcile,
    /// or retire its exact durable namespace without residual occupancy.
    #[tokio::test]
    async fn governed_staging_restart_reconstructs_and_retires_exact_namespace_bytes() {
        let directory = tempfile::tempdir().expect("stage volume root");
        let wal_root = directory.path().join("wal");
        let scribe_stage = directory.path().join("scribe-stage");
        let scribe_scratch = directory.path().join("scribe-scratch");
        let forge_scratch = directory.path().join("forge-scratch");
        let oracle_scratch = directory.path().join("oracle-scratch");
        for path in [
            &wal_root,
            &scribe_stage,
            &scribe_scratch,
            &forge_scratch,
            &oracle_scratch,
        ] {
            std::fs::create_dir(path).expect("registered volume root");
        }
        let roots = BifrostVolumeRoots {
            wal: wal_root.clone(),
            scribe_stage: scribe_stage.clone(),
            scribe_output_scratch: scribe_scratch.clone(),
            forge_scratch: forge_scratch.clone(),
            oracle_scratch: oracle_scratch.clone(),
        };
        let governor =
            BifrostVolumeGovernor::register(roots, 1024 * 1024, BifrostResourceHealth::default())
                .expect("initial volume registration");
        let writer = Arc::new(
            WalWriter::new_with_volume(
                &wal_root,
                [7_u8; 16],
                11,
                WalConfig::default(),
                governor.capabilities().wal,
            )
            .expect("governed WAL writer"),
        );
        let staging = ScribeStaging::new(Arc::clone(&writer));
        let source = directory.path().join("source.parquet");
        let content = b"restart-governed publication";
        tokio::fs::write(&source, content)
            .await
            .expect("source write");
        let object_key = "tenant/table/restart.parquet";
        let identity = ParquetObjectIdentity::new(object_key).expect("object key");
        let digest: [u8; 32] = Sha256::digest(content).into();
        let claim = match staging
            .stage_and_elect(
                "generation-9-artifact-0",
                &source,
                &identity,
                digest,
                content.len() as u64,
                &mut [0_u8; 8],
            )
            .await
            .expect("stage election")
        {
            StageElection::Winner(claim) | StageElection::ExistingWinner(claim) => claim,
        };
        let publication_bytes =
            publish_elected_artifact(&staging, object_key, content, digest, &claim).await;
        let (pending_bytes, pending_manifest_path) =
            stage_pending_attempt(&staging, &writer, &source, digest, content).await;
        let expected = claim
            .accounted_bytes
            .checked_add(publication_bytes)
            .and_then(|bytes| bytes.checked_add(pending_bytes))
            .expect("retained stage bytes");
        assert_eq!(
            governor
                .usage_for_test(BifrostVolumeClass::Wal)
                .expect("initial usage"),
            (expected, 0, 0)
        );
        let unrelated = staging.root.join("operator-note.txt");
        tokio::fs::write(&unrelated, b"not a Scribe stage")
            .await
            .expect("unrelated file");
        drop(staging);
        drop(writer);
        drop(governor);

        assert_restart_reconstructs_and_retires(
            BifrostVolumeRoots {
                wal: wal_root.clone(),
                scribe_stage,
                scribe_output_scratch: scribe_scratch,
                forge_scratch,
                oracle_scratch,
            },
            &wal_root,
            expected,
            &claim,
            &pending_manifest_path,
        )
        .await;
        assert!(unrelated.exists(), "unrelated namespace file is untouched");
    }
}
