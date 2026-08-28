//! Durable local staging — the record that lets the WAL retire.
//!
//! A frozen memtable bucket is the only copy of its rows outside the WAL. The
//! WAL segments behind it may be retired exactly once something else holds
//! those rows durably *and* can serve them to a reader. That "something else"
//! is a staged member: one or more fsynced local sorted Parquet runs plus this
//! module's [`StagedHotSourceRecordV1`], which records every fact recovery
//! needs to rebuild the ready index and the live-tail source without consulting
//! the WAL it authorized retiring.
//!
//! Three properties make the record trustworthy enough to retire a WAL segment
//! against:
//!
//! - **It is never partially visible.** A record is written to a temporary
//!   name, fsynced, renamed into place, and its directory fsynced. A reader or
//!   a restart sees the whole record or no record; a `.tmp` file is by
//!   construction incomplete and is the only thing recovery deletes.
//! - **It is checked against the bytes it describes.** Validation re-stats
//!   every run and compares exact length and SHA-256 before the member is
//!   offered as a query source. A run that was truncated by a crash fails
//!   closed with the WAL still authoritative rather than serving short reads.
//! - **It fails closed on anything it does not understand.** An unknown
//!   version is an operator-visible refusal, never an implicit upgrade. The
//!   record format is unshipped, so there is exactly one version to read and
//!   nothing to migrate from.
//!
//! Member state moves forward only. A claim that is cancelled or interrupted
//! does not send its members back to `Ready`: claim identity is derived from
//! the member set, so the retry re-derives the same claim and resumes it. A
//! back edge would let the same members be published under two identities.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};
use tokio::io::AsyncReadExt as _;
use uuid::Uuid;
use wyrd_spec::ids::DataTenantId;
use wyrd_spec::vala::api::TimePartitionWire;

use crate::catalog::TableRef;
use crate::catalog::layout::TimePartition;
use crate::schema::fingerprint::SchemaFingerprint;
use crate::scribe::assembly::{
    ReadyMember, RecoveredMember, ScribeAssemblyKey, StagedMemberId, StagingClaimId,
};
use crate::scribe::stream_identity::{NodeId, WriterEpoch};

/// Only staged-record version this server reads or writes.
const STAGED_RECORD_VERSION: u16 = 1;
/// Largest staged record accepted during fail-closed recovery.
const MAX_STAGED_RECORD_BYTES: u64 = 1024 * 1024;
/// Bytes read per hash step when validating a run against its record.
const RUN_VALIDATION_CHUNK_BYTES: usize = 256 * 1024;

/// Why the staged namespace refused a write, a transition, or a recovery.
#[derive(Debug, thiserror::Error)]
pub enum HotStageError {
    /// Local durable IO failed.
    #[error("Scribe staged namespace failed to {operation}: {detail}")]
    Io {
        /// Operation being attempted.
        operation: &'static str,
        /// Underlying failure detail, without payload or credential content.
        detail: String,
    },
    /// A record could not be encoded or decoded.
    #[error("Scribe staged record at `{name}` is not readable: {detail}")]
    Malformed {
        /// Record file name, without its absolute path.
        name: String,
        /// Decoding failure detail.
        detail: String,
    },
    /// A record declared a version this server does not implement.
    #[error(
        "Scribe staged record at `{name}` declares version {found}, but this server reads only version {expected}. \
         Start the server binary that wrote this staging volume, or retire the volume with its WAL intact."
    )]
    UnknownVersion {
        /// Record file name, without its absolute path.
        name: String,
        /// Version the record declared.
        found: u16,
        /// Version this server implements.
        expected: u16,
    },
    /// A run's bytes contradict the record that describes them.
    #[error(
        "Scribe staged run `{run}` contradicts its record: expected {expected_bytes} bytes with digest {expected_digest}, found {actual_bytes} bytes with digest {actual_digest}"
    )]
    RunMismatch {
        /// Run file name, without its absolute path.
        run: String,
        /// Bytes the record recorded.
        expected_bytes: u64,
        /// Bytes actually on disk.
        actual_bytes: u64,
        /// Lowercase hex digest the record recorded.
        expected_digest: String,
        /// Lowercase hex digest actually computed.
        actual_digest: String,
    },
    /// A record described a run that is not present.
    #[error("Scribe staged run `{run}` named by its record is missing")]
    MissingRun {
        /// Run file name, without its absolute path.
        run: String,
    },
    /// A record carried facts that cannot form a valid member.
    #[error("Scribe staged record at `{name}` is not a valid member: {detail}")]
    Invalid {
        /// Record file name, without its absolute path.
        name: String,
        /// What made the record invalid.
        detail: String,
    },
    /// A state transition tried to move a member backwards or sideways.
    #[error("Scribe staged member cannot move from `{from}` to `{to}`")]
    Backwards {
        /// Current state label.
        from: &'static str,
        /// Refused state label.
        to: &'static str,
    },
    /// A transition named a member the staged namespace does not hold.
    #[error("Scribe staged member shard {shard} generation {generation} is not staged")]
    UnknownMember {
        /// Shard named by the caller.
        shard: u16,
        /// Generation named by the caller.
        generation: u64,
    },
}

/// Builds an IO error closure naming the operation that failed.
fn stage_io(operation: &'static str) -> impl Fn(std::io::Error) -> HotStageError {
    move |error| HotStageError::Io {
        operation,
        detail: error.to_string(),
    }
}

/// Lifecycle position of one staged member.
///
/// The order of the variants is the order of the lifecycle. `Ready` members are
/// claimable and readable; every later state is still readable, because a
/// member remains the live-tail authority for its rows until the published
/// object that replaces it is committed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum StagedMemberState {
    /// Durable, validated, query-registered, and available for assembly.
    Ready,
    /// Owned by one assembly claim whose membership is already durable.
    Claimed {
        /// Deterministic identity of the owning claim.
        claim_id: String,
    },
    /// The owning claim is publishing; its SQL outcome may still be uncertain.
    Publishing {
        /// Deterministic identity of the owning claim.
        claim_id: String,
        /// Fenced publication operation reconciled against `file_list`.
        operation_id: Uuid,
    },
    /// The claim committed; a published hot object now serves these rows.
    Published {
        /// Commit key of the fenced `file_list` transaction.
        file_list_commit_key: String,
        /// Object identities the claim published, in artifact-ordinal order.
        published_object_identities: Vec<String>,
        /// WAL ranges the published objects now own, used only to subtract
        /// this member's rows from a pinned cut that already includes them.
        persisted_lsn_ranges: Vec<StagedLsnRange>,
    },
    /// Published and awaiting the last staged-reader lease before deletion.
    CleanupPending,
}

impl StagedMemberState {
    /// Returns this state's position in the staged lifecycle.
    const fn position(&self) -> u8 {
        match self {
            Self::Ready => 0,
            Self::Claimed { .. } => 1,
            Self::Publishing { .. } => 2,
            Self::Published { .. } => 3,
            Self::CleanupPending => 4,
        }
    }

    /// Returns the fixed-cardinality label naming this state.
    ///
    /// Safe as a metric label: it carries no tenant, table, claim, or path.
    #[must_use]
    pub const fn label(&self) -> &'static str {
        match self {
            Self::Ready => "ready",
            Self::Claimed { .. } => "claimed",
            Self::Publishing { .. } => "publishing",
            Self::Published { .. } => "published",
            Self::CleanupPending => "cleanup_pending",
        }
    }

    /// Reports whether the member is still available for assembly.
    #[must_use]
    pub const fn is_ready(&self) -> bool {
        matches!(self, Self::Ready)
    }
}

/// One inclusive WAL range a published object now owns.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct StagedLsnRange {
    /// Inclusive first WAL position.
    pub min: u64,
    /// Inclusive final WAL position.
    pub max: u64,
}

/// One fsynced local sorted run belonging to a staged member.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StagedRunFile {
    /// File name inside the member's staged directory.
    file_name: String,
    /// Exact encoded bytes of the run.
    encoded_bytes: u64,
    /// Exact rows the run carries.
    rows: u64,
    /// Lowercase hex SHA-256 of the run's bytes.
    sha256: String,
}

impl StagedRunFile {
    /// Records one run's exact durable facts.
    #[must_use]
    pub fn new(file_name: String, encoded_bytes: u64, rows: u64, sha256: [u8; 32]) -> Self {
        Self {
            file_name,
            encoded_bytes,
            rows,
            sha256: hex(&sha256),
        }
    }

    /// Returns the run's file name inside its member directory.
    #[must_use]
    pub fn file_name(&self) -> &str {
        &self.file_name
    }

    /// Returns the run's exact encoded bytes.
    #[must_use]
    pub const fn encoded_bytes(&self) -> u64 {
        self.encoded_bytes
    }

    /// Returns the run's exact row count.
    #[must_use]
    pub const fn rows(&self) -> u64 {
        self.rows
    }
}

/// Durable description of one staged member.
///
/// Everything the ready index, the live-tail reader, and WAL retirement need is
/// here, because at recovery time this record is all there is: the memtable is
/// gone and the WAL behind the member may already have been retired.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StagedHotSourceRecordV1 {
    /// Closed format version validated before any other field is trusted.
    version: u16,
    /// Tenant owning every row of the member.
    tenant: DataTenantId,
    /// Canonical `<namespace>.<name>` the member belongs to.
    table_fqn: String,
    /// Lowercase hex user-schema fingerprint the runs were encoded against.
    schema_fingerprint: String,
    /// Lowercase hex digest of the canonical physical layout.
    layout_fingerprint: String,
    /// Exact physical time partition of every row.
    partition: TimePartitionWire,
    /// Node whose staging volume holds the runs.
    node_id: Uuid,
    /// Fenced writer epoch whose LSN stream produced the rows.
    writer_epoch: i64,
    /// Shard that froze the member.
    shard: u16,
    /// Generation ordinal of that shard.
    generation: u64,
    /// Inclusive WAL range the member covers.
    wal_range: StagedLsnRange,
    /// Fsynced sorted runs holding the rows, in sort order.
    runs: Vec<StagedRunFile>,
    /// Instant the member became durable and queryable.
    ready_at: DateTime<Utc>,
    /// Current lifecycle position.
    state: StagedMemberState,
}

impl StagedHotSourceRecordV1 {
    /// Describes one member that has just become durable.
    ///
    /// The record is built from the assembly key so the member's compatibility
    /// scope and the key the ready index uses can never disagree.
    #[must_use]
    pub fn ready(
        key: &ScribeAssemblyKey,
        member: StagedMemberId,
        wal_range: StagedLsnRange,
        runs: Vec<StagedRunFile>,
        ready_at: DateTime<Utc>,
    ) -> Self {
        Self {
            version: STAGED_RECORD_VERSION,
            tenant: key.tenant(),
            table_fqn: key.table().fqn(),
            schema_fingerprint: hex(&key.schema_fingerprint().0),
            layout_fingerprint: hex(&key.layout_fingerprint()),
            partition: key.partition().to_wire(),
            node_id: key.node_id().as_uuid(),
            writer_epoch: key.writer_epoch().as_i64(),
            shard: member.shard(),
            generation: member.generation(),
            wal_range,
            runs,
            ready_at,
            state: StagedMemberState::Ready,
        }
    }

    /// Returns the member's immutable identity.
    #[must_use]
    pub const fn member(&self) -> StagedMemberId {
        StagedMemberId::new(self.shard, self.generation)
    }

    /// Returns the member's current lifecycle position.
    #[must_use]
    pub const fn state(&self) -> &StagedMemberState {
        &self.state
    }

    /// Returns the inclusive WAL range the member covers.
    #[must_use]
    pub const fn wal_range(&self) -> StagedLsnRange {
        self.wal_range
    }

    /// Returns the fsynced runs holding the member's rows, in sort order.
    #[must_use]
    pub fn runs(&self) -> &[StagedRunFile] {
        &self.runs
    }

    /// Returns the summed encoded bytes across every run.
    #[must_use]
    pub fn encoded_bytes(&self) -> u64 {
        self.runs
            .iter()
            .fold(0, |total, run| total.saturating_add(run.encoded_bytes))
    }

    /// Returns the summed rows across every run.
    #[must_use]
    pub fn rows(&self) -> u64 {
        self.runs
            .iter()
            .fold(0, |total, run| total.saturating_add(run.rows))
    }

    /// Rebuilds the assembly key this member belongs to.
    ///
    /// Recovery has only the record, so the key is reconstructed from its
    /// recorded fields rather than from a live catalog lookup: a table whose
    /// registered layout changed after the member was staged must not silently
    /// merge into a claim with a different sort order.
    ///
    /// # Errors
    ///
    /// Returns [`HotStageError::Invalid`] when the table name, fingerprints, or
    /// partition recorded in the file are not the canonical forms this server
    /// writes.
    pub fn assembly_key(&self, name: &str) -> Result<ScribeAssemblyKey, HotStageError> {
        let invalid = |detail: String| HotStageError::Invalid {
            name: name.to_owned(),
            detail,
        };
        let table = TableRef::parse_fqn(&self.table_fqn).ok_or_else(|| {
            invalid(format!(
                "`{}` is not a canonical table name",
                self.table_fqn
            ))
        })?;
        let schema_fingerprint =
            SchemaFingerprint(unhex(&self.schema_fingerprint).ok_or_else(|| {
                invalid("schema fingerprint is not a 32-byte hex digest".to_owned())
            })?);
        let layout_fingerprint = unhex(&self.layout_fingerprint)
            .ok_or_else(|| invalid("layout fingerprint is not a 32-byte hex digest".to_owned()))?;
        let partition = TimePartition::from_wire(self.partition);
        Ok(ScribeAssemblyKey::from_parts(
            self.tenant,
            table,
            schema_fingerprint,
            layout_fingerprint,
            partition,
            NodeId::new(self.node_id),
            WriterEpoch::new(self.writer_epoch),
        ))
    }

    /// Projects the record into the value the ready index orders and claims.
    ///
    /// # Errors
    ///
    /// Returns [`HotStageError::Invalid`] when the member carries no bytes or
    /// no rows, which cannot describe a frozen nonempty bucket.
    pub fn ready_member(&self, name: &str) -> Result<ReadyMember, HotStageError> {
        ReadyMember::new(
            self.member(),
            self.encoded_bytes(),
            self.rows(),
            self.ready_at,
        )
        .map_err(|error| HotStageError::Invalid {
            name: name.to_owned(),
            detail: error.to_string(),
        })
    }
}

/// One validated staged member and where its files live.
#[derive(Debug, Clone)]
pub struct StagedMember {
    /// Assembly key rebuilt from the record.
    key: ScribeAssemblyKey,
    /// Durable record describing the member.
    record: StagedHotSourceRecordV1,
    /// Directory holding the record and its runs.
    directory: PathBuf,
}

impl StagedMember {
    /// Returns the assembly key the member may be claimed under.
    #[must_use]
    pub const fn key(&self) -> &ScribeAssemblyKey {
        &self.key
    }

    /// Returns the durable record describing the member.
    #[must_use]
    pub const fn record(&self) -> &StagedHotSourceRecordV1 {
        &self.record
    }

    /// Projects the member into what the assembler must restore for it.
    ///
    /// A published member is no longer assembly work — a hot object already
    /// serves its rows and it is waiting only for reader leases — so it
    /// restores nothing and returns `None`. Everything earlier restores either
    /// as ready or as owned by the claim recorded on it.
    ///
    /// # Errors
    ///
    /// Returns [`HotStageError::Invalid`] when the member carries no bytes or
    /// rows, or when a recorded claim identity is not the exact form the
    /// assembler renders.
    pub fn recovered(&self) -> Result<Option<RecoveredMember>, HotStageError> {
        let name = RECORD_FILE_NAME;
        let member = self.record.ready_member(name)?;
        let claim_id = match self.record.state() {
            StagedMemberState::Ready => return Ok(Some(RecoveredMember::Ready(member))),
            StagedMemberState::Claimed { claim_id }
            | StagedMemberState::Publishing { claim_id, .. } => claim_id,
            StagedMemberState::Published { .. } | StagedMemberState::CleanupPending => {
                return Ok(None);
            }
        };
        let claim = StagingClaimId::from_hex(claim_id).ok_or_else(|| HotStageError::Invalid {
            name: name.to_owned(),
            detail: "the recorded claim identity is not a 32-byte hex digest".to_owned(),
        })?;
        Ok(Some(RecoveredMember::Claimed { claim, member }))
    }

    /// Returns absolute paths to the member's runs, in sort order.
    ///
    /// These paths never leave Scribe: the live-tail service reads them and
    /// returns bounded Arrow batches, and Oracle receives rows rather than a
    /// filesystem handle.
    #[must_use]
    pub fn run_paths(&self) -> Vec<PathBuf> {
        self.record
            .runs
            .iter()
            .map(|run| self.directory.join(&run.file_name))
            .collect()
    }
}

/// Owner of the durable local staged-run namespace.
///
/// One directory per assembly key, one directory per member inside it, one
/// record file per member. The layout is derived from the key digest rather
/// than from tenant or table names so no user-controlled string ever becomes a
/// path component.
#[derive(Debug, Clone)]
pub struct ScribeHotStage {
    /// Root of the governed durable staging namespace.
    root: PathBuf,
}

impl ScribeHotStage {
    /// Binds the owner to one durable staging root.
    #[must_use]
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    /// Returns the governed root every staged file lives under.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Returns the directory holding one member's record and runs.
    ///
    /// Runs are written here *before* the record, so the caller fsyncs its runs
    /// into a directory that a scan will ignore until the record lands.
    #[must_use]
    pub fn member_directory(&self, key: &ScribeAssemblyKey, member: StagedMemberId) -> PathBuf {
        self.root.join(hex(&key.digest())).join(format!(
            "{}-{}",
            member.shard(),
            member.generation()
        ))
    }

    /// Publishes one member's record, making the member visible to recovery.
    ///
    /// The runs it names must already be fsynced in the member directory. The
    /// record is written to `.tmp`, fsynced, renamed, and both the member and
    /// key directories are fsynced, so a crash at any point leaves either no
    /// record or a complete one. This is the write half of the first durable
    /// boundary: after it returns, the member's rows survive without the WAL.
    ///
    /// # Errors
    ///
    /// Returns [`HotStageError::Io`] for any local durable IO failure and
    /// [`HotStageError::Malformed`] when the record cannot be encoded.
    ///
    /// # Cancellation
    ///
    /// Cancellation may leave a `.tmp` record, which recovery removes as a
    /// proven incomplete temporary. It never leaves a partially visible record.
    pub async fn publish_record(
        &self,
        key: &ScribeAssemblyKey,
        record: &StagedHotSourceRecordV1,
    ) -> Result<PathBuf, HotStageError> {
        let directory = self.member_directory(key, record.member());
        tokio::fs::create_dir_all(&directory)
            .await
            .map_err(stage_io("create the member directory"))?;
        let final_path = directory.join(RECORD_FILE_NAME);
        let temporary = directory.join(TEMPORARY_RECORD_FILE_NAME);
        let bytes = serde_json::to_vec(record).map_err(|error| HotStageError::Malformed {
            name: RECORD_FILE_NAME.to_owned(),
            detail: error.to_string(),
        })?;
        tokio::fs::write(&temporary, &bytes)
            .await
            .map_err(stage_io("write the staged record"))?;
        fsync_file(&temporary).await?;
        tokio::fs::rename(&temporary, &final_path)
            .await
            .map_err(stage_io("rename the staged record into place"))?;
        fsync_directory(&directory).await?;
        if let Some(parent) = directory.parent() {
            fsync_directory(parent).await?;
        }
        Ok(final_path)
    }

    /// Moves one staged member forward to its next lifecycle state.
    ///
    /// Only forward moves are accepted. A cancelled or interrupted claim keeps
    /// its members claimed: claim identity is derived from the member set, so
    /// the retry re-derives the same claim and resumes it, while a back edge
    /// would let one member's rows be published under two identities.
    ///
    /// # Errors
    ///
    /// Returns [`HotStageError::UnknownMember`] when no record is staged,
    /// [`HotStageError::Backwards`] for a non-forward move, and
    /// [`HotStageError::Io`] or [`HotStageError::Malformed`] for durable IO and
    /// decoding failures.
    pub async fn transition(
        &self,
        key: &ScribeAssemblyKey,
        member: StagedMemberId,
        next: StagedMemberState,
    ) -> Result<StagedHotSourceRecordV1, HotStageError> {
        let directory = self.member_directory(key, member);
        let path = directory.join(RECORD_FILE_NAME);
        let mut record = match read_record(&path).await {
            Ok(record) => record,
            Err(HotStageError::Io { .. }) if !path_exists(&path).await => {
                return Err(HotStageError::UnknownMember {
                    shard: member.shard(),
                    generation: member.generation(),
                });
            }
            Err(error) => return Err(error),
        };
        if next.position() <= record.state.position() {
            return Err(HotStageError::Backwards {
                from: record.state.label(),
                to: next.label(),
            });
        }
        record.state = next;
        self.publish_record(key, &record).await?;
        Ok(record)
    }

    /// Removes one member's record and runs after publication and lease drain.
    ///
    /// # Errors
    ///
    /// Returns [`HotStageError::Io`] when the directory cannot be removed.
    ///
    /// # Cancellation
    ///
    /// Cancellation may leave part of the member directory; repeating the
    /// removal is safe because the record is deleted with it.
    pub async fn retire(
        &self,
        key: &ScribeAssemblyKey,
        member: StagedMemberId,
    ) -> Result<(), HotStageError> {
        let directory = self.member_directory(key, member);
        match tokio::fs::remove_dir_all(&directory).await {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(stage_io("retire the member directory")(error)),
        }
        if let Some(parent) = directory.parent() {
            fsync_directory(parent).await?;
        }
        Ok(())
    }

    /// Scans and validates the whole staged namespace at startup.
    ///
    /// Every record is decoded, version-checked, rebuilt into its assembly key,
    /// and validated against the exact bytes of every run it names. Proven
    /// incomplete temporaries are removed; anything else that fails is returned
    /// as an error so the caller keeps the WAL authoritative rather than
    /// serving a member it cannot vouch for.
    ///
    /// Members are returned grouped by assembly key so the caller can rebuild
    /// the ready index in one pass.
    ///
    /// # Errors
    ///
    /// Returns [`HotStageError::UnknownVersion`], [`HotStageError::Malformed`],
    /// [`HotStageError::Invalid`], [`HotStageError::MissingRun`], or
    /// [`HotStageError::RunMismatch`] on the first record that cannot be
    /// vouched for, and [`HotStageError::Io`] for durable IO failures.
    pub async fn recover(
        &self,
    ) -> Result<HashMap<ScribeAssemblyKey, Vec<StagedMember>>, HotStageError> {
        let mut recovered: HashMap<ScribeAssemblyKey, Vec<StagedMember>> = HashMap::new();
        let mut keys = match tokio::fs::read_dir(&self.root).await {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(recovered),
            Err(error) => return Err(stage_io("open the staged namespace")(error)),
        };
        while let Some(key_entry) = keys
            .next_entry()
            .await
            .map_err(stage_io("read the staged namespace"))?
        {
            if !is_directory(&key_entry).await? {
                continue;
            }
            let mut members = match tokio::fs::read_dir(key_entry.path()).await {
                Ok(entries) => entries,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                Err(error) => return Err(stage_io("open a staged key directory")(error)),
            };
            while let Some(member_entry) = members
                .next_entry()
                .await
                .map_err(stage_io("read a staged key directory"))?
            {
                if !is_directory(&member_entry).await? {
                    continue;
                }
                let directory = member_entry.path();
                remove_if_present(&directory.join(TEMPORARY_RECORD_FILE_NAME)).await?;
                let path = directory.join(RECORD_FILE_NAME);
                if !path_exists(&path).await {
                    continue;
                }
                let member = self.validate(&directory, &path).await?;
                recovered
                    .entry(member.key.clone())
                    .or_default()
                    .push(member);
            }
        }
        Ok(recovered)
    }

    /// Validates one record and the exact bytes of every run it names.
    ///
    /// # Errors
    ///
    /// Returns the first refusal encountered; see [`Self::recover`].
    async fn validate(&self, directory: &Path, path: &Path) -> Result<StagedMember, HotStageError> {
        let record = read_record(path).await?;
        let name = RECORD_FILE_NAME.to_owned();
        if record.version != STAGED_RECORD_VERSION {
            return Err(HotStageError::UnknownVersion {
                name,
                found: record.version,
                expected: STAGED_RECORD_VERSION,
            });
        }
        if record.runs.is_empty() {
            return Err(HotStageError::Invalid {
                name,
                detail: "a staged member names no runs".to_owned(),
            });
        }
        if record.wal_range.min > record.wal_range.max {
            return Err(HotStageError::Invalid {
                name,
                detail: "the recorded WAL range is reversed".to_owned(),
            });
        }
        let key = record.assembly_key(&name)?;
        // A member is offered as a query source only after its bytes are proven
        // to be the bytes the record describes.
        for run in &record.runs {
            validate_run(&directory.join(&run.file_name), run).await?;
        }
        record.ready_member(&name)?;
        Ok(StagedMember {
            key,
            record,
            directory: directory.to_path_buf(),
        })
    }
}

/// Name of the one durable record file inside a member directory.
pub(crate) const RECORD_FILE_NAME: &str = "member.staged.json";
/// Name of the incomplete record a crash may leave behind.
const TEMPORARY_RECORD_FILE_NAME: &str = "member.staged.json.tmp";

/// Reads and decodes one staged record.
///
/// # Errors
///
/// Returns [`HotStageError::Io`] when the file cannot be read or exceeds the
/// bounded record size, and [`HotStageError::Malformed`] when it cannot decode.
async fn read_record(path: &Path) -> Result<StagedHotSourceRecordV1, HotStageError> {
    let metadata = tokio::fs::metadata(path)
        .await
        .map_err(stage_io("stat a staged record"))?;
    if metadata.len() > MAX_STAGED_RECORD_BYTES {
        return Err(HotStageError::Malformed {
            name: RECORD_FILE_NAME.to_owned(),
            detail: format!(
                "record is {} bytes, above the {MAX_STAGED_RECORD_BYTES}-byte bound",
                metadata.len()
            ),
        });
    }
    let bytes = tokio::fs::read(path)
        .await
        .map_err(stage_io("read a staged record"))?;
    serde_json::from_slice(&bytes).map_err(|error| HotStageError::Malformed {
        name: RECORD_FILE_NAME.to_owned(),
        detail: error.to_string(),
    })
}

/// Confirms one run's exact length and digest against its recorded facts.
///
/// # Errors
///
/// Returns [`HotStageError::MissingRun`] when the run is absent,
/// [`HotStageError::RunMismatch`] when its bytes contradict the record, and
/// [`HotStageError::Io`] for read failures.
async fn validate_run(path: &Path, run: &StagedRunFile) -> Result<(), HotStageError> {
    let metadata = match tokio::fs::metadata(path).await {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Err(HotStageError::MissingRun {
                run: run.file_name.clone(),
            });
        }
        Err(error) => return Err(stage_io("stat a staged run")(error)),
    };
    let mut file = tokio::fs::File::open(path)
        .await
        .map_err(stage_io("open a staged run"))?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0_u8; RUN_VALIDATION_CHUNK_BYTES];
    loop {
        let read = file
            .read(&mut buffer)
            .await
            .map_err(stage_io("read a staged run"))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    let digest: [u8; 32] = hasher.finalize().into();
    let digest = hex(&digest);
    if metadata.len() != run.encoded_bytes || digest != run.sha256 {
        return Err(HotStageError::RunMismatch {
            run: run.file_name.clone(),
            expected_bytes: run.encoded_bytes,
            actual_bytes: metadata.len(),
            expected_digest: run.sha256.clone(),
            actual_digest: digest,
        });
    }
    Ok(())
}

/// Reports whether a directory entry is itself a directory.
///
/// # Errors
///
/// Returns [`HotStageError::Io`] when the entry cannot be typed.
async fn is_directory(entry: &tokio::fs::DirEntry) -> Result<bool, HotStageError> {
    Ok(entry
        .file_type()
        .await
        .map_err(stage_io("type a staged entry"))?
        .is_dir())
}

/// Reports whether a path currently exists.
async fn path_exists(path: &Path) -> bool {
    tokio::fs::metadata(path).await.is_ok()
}

/// Removes a proven incomplete temporary, accepting restart-idempotent absence.
///
/// # Errors
///
/// Returns [`HotStageError::Io`] for removal failures other than absence.
async fn remove_if_present(path: &Path) -> Result<(), HotStageError> {
    match tokio::fs::remove_file(path).await {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(stage_io("remove an incomplete staged temporary")(error)),
    }
}

/// Fsyncs one file's contents.
///
/// # Errors
///
/// Returns [`HotStageError::Io`] when the file cannot be opened or synced.
async fn fsync_file(path: &Path) -> Result<(), HotStageError> {
    tokio::fs::File::open(path)
        .await
        .map_err(stage_io("open a staged file for fsync"))?
        .sync_all()
        .await
        .map_err(stage_io("fsync a staged file"))
}

/// Fsyncs one directory so a rename inside it is durable.
///
/// # Errors
///
/// Returns [`HotStageError::Io`] when the directory cannot be opened or synced.
async fn fsync_directory(path: &Path) -> Result<(), HotStageError> {
    tokio::fs::File::open(path)
        .await
        .map_err(stage_io("open a staged directory for fsync"))?
        .sync_all()
        .await
        .map_err(stage_io("fsync a staged directory"))
}

/// Renders bytes as lowercase hex for durable records and path components.
fn hex(bytes: &[u8; 32]) -> String {
    use std::fmt::Write as _;

    let mut rendered = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        let _ = write!(rendered, "{byte:02x}");
    }
    rendered
}

/// Parses a lowercase 32-byte hex digest written by [`hex`].
fn unhex(value: &str) -> Option<[u8; 32]> {
    if value.len() != 64 {
        return None;
    }
    let mut bytes = [0_u8; 32];
    for (index, byte) in bytes.iter_mut().enumerate() {
        let start = index * 2;
        *byte = u8::from_str_radix(value.get(start..start + 2)?, 16).ok()?;
    }
    Some(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::layout::{PhysicalLayout, TimeGranularity};
    use crate::namespaces::BifrostNamespace;
    use arrow::datatypes::{DataType, Field, Schema, TimeUnit};

    /// Builds one assembly key for the staged-namespace fixtures.
    ///
    /// # Panics
    ///
    /// Panics when the fixed fixture literals do not resolve, which would be a
    /// fixture bug rather than staged-namespace behavior.
    fn fixture_key() -> ScribeAssemblyKey {
        let schema = Schema::new(vec![Field::new(
            "wyrd_event_time",
            DataType::Timestamp(TimeUnit::Microsecond, None),
            false,
        )]);
        let layout = PhysicalLayout::resolve("vala.bifrost.events", &schema, None)
            .expect("fixture layout resolves");
        ScribeAssemblyKey::new(
            DataTenantId::new_v7(),
            TableRef::new(BifrostNamespace::Bifrost, "events"),
            SchemaFingerprint([3; 32]),
            &layout,
            TimePartition::new(
                TimeGranularity::Hour,
                chrono::DateTime::from_timestamp(1_772_150_400, 0)
                    .expect("a fixed representable instant"),
            )
            .expect("a fixed hour boundary"),
            NodeId::new(Uuid::from_u128(11)),
            WriterEpoch::new(6),
        )
    }

    /// Writes one fsynced run into a member directory and describes it.
    ///
    /// # Panics
    ///
    /// Panics when the fixture run cannot be written.
    async fn write_run(directory: &Path, file_name: &str, contents: &[u8]) -> StagedRunFile {
        tokio::fs::create_dir_all(directory)
            .await
            .expect("fixture member directory");
        tokio::fs::write(directory.join(file_name), contents)
            .await
            .expect("fixture run writes");
        let digest: [u8; 32] = Sha256::digest(contents).into();
        StagedRunFile::new(file_name.to_owned(), contents.len() as u64, 4, digest)
    }

    /// Returns the fixed instant fixtures record as their ready time.
    ///
    /// # Panics
    ///
    /// Panics when the fixed instant is not representable.
    fn ready_at() -> DateTime<Utc> {
        chrono::DateTime::from_timestamp(1_772_150_460, 0).expect("a fixed representable instant")
    }

    /// A published record survives a restart and rebuilds its own claim scope.
    ///
    /// Recovery has no memtable and possibly no WAL, so everything the ready
    /// index needs has to come back out of the record itself.
    ///
    /// # Panics
    ///
    /// Panics when a fixture write or recovery is refused.
    #[tokio::test]
    async fn a_staged_record_round_trips_and_rebuilds_its_assembly_key() {
        let directory = tempfile::tempdir().expect("staging root");
        let stage = ScribeHotStage::new(directory.path().join("hot-stage"));
        let key = fixture_key();
        let member = StagedMemberId::new(4, 12);
        let run = write_run(
            &stage.member_directory(&key, member),
            "run-0.parquet",
            b"sorted-run-bytes",
        )
        .await;
        let record = StagedHotSourceRecordV1::ready(
            &key,
            member,
            StagedLsnRange { min: 10, max: 42 },
            vec![run],
            ready_at(),
        );
        stage
            .publish_record(&key, &record)
            .await
            .expect("the record publishes");

        let recovered = stage
            .recover()
            .await
            .expect("recovery validates the member");
        assert_eq!(recovered.len(), 1);
        let members = recovered
            .get(&key)
            .expect("the member recovers under its key");
        assert_eq!(members.len(), 1);
        let recovered_member = &members[0];
        assert_eq!(recovered_member.record(), &record);
        assert_eq!(recovered_member.key(), &key);
        assert!(recovered_member.record().state().is_ready());
        assert_eq!(recovered_member.record().wal_range().max, 42);
        assert_eq!(
            recovered_member.run_paths(),
            vec![stage.member_directory(&key, member).join("run-0.parquet")]
        );

        let ready = recovered_member
            .record()
            .ready_member("member.staged.json")
            .expect("the member projects into the ready index");
        assert_eq!(ready.id(), member);
        assert_eq!(ready.encoded_bytes(), 16);
        assert_eq!(ready.ready_at(), ready_at());
    }

    /// An incomplete record is removed and never counted as a query source.
    ///
    /// A `.tmp` record is the one thing a crash can leave that is provably
    /// worthless: it was never renamed, so nothing ever depended on it.
    ///
    /// # Panics
    ///
    /// Panics when a fixture write or recovery is refused.
    #[tokio::test]
    async fn recovery_removes_incomplete_temporaries_and_keeps_the_wal_authoritative() {
        let directory = tempfile::tempdir().expect("staging root");
        let stage = ScribeHotStage::new(directory.path().join("hot-stage"));
        let key = fixture_key();
        let member = StagedMemberId::new(1, 1);
        let member_directory = stage.member_directory(&key, member);
        let _ = write_run(&member_directory, "run-0.parquet", b"orphaned").await;
        tokio::fs::write(
            member_directory.join("member.staged.json.tmp"),
            b"{\"version\":1",
        )
        .await
        .expect("fixture temporary writes");

        let recovered = stage.recover().await.expect("recovery skips the temporary");
        assert!(recovered.is_empty());
        assert!(
            !path_exists(&member_directory.join("member.staged.json.tmp")).await,
            "a proven incomplete temporary is removed"
        );
        assert!(
            path_exists(&member_directory.join("run-0.parquet")).await,
            "recovery never deletes durable run bytes it did not prove worthless"
        );
    }

    /// A record this server does not understand fails closed with remediation.
    ///
    /// # Panics
    ///
    /// Panics when a fixture write succeeds where it should not, or recovery
    /// unexpectedly succeeds.
    #[tokio::test]
    async fn recovery_fails_closed_on_an_unknown_record_version() {
        let directory = tempfile::tempdir().expect("staging root");
        let stage = ScribeHotStage::new(directory.path().join("hot-stage"));
        let key = fixture_key();
        let member = StagedMemberId::new(2, 3);
        let member_directory = stage.member_directory(&key, member);
        let run = write_run(&member_directory, "run-0.parquet", b"future-bytes").await;
        let record = StagedHotSourceRecordV1::ready(
            &key,
            member,
            StagedLsnRange { min: 1, max: 2 },
            vec![run],
            ready_at(),
        );
        let mut encoded: serde_json::Value =
            serde_json::to_value(&record).expect("the record encodes");
        encoded["version"] = serde_json::json!(2);
        tokio::fs::write(
            member_directory.join("member.staged.json"),
            serde_json::to_vec(&encoded).expect("the altered record encodes"),
        )
        .await
        .expect("fixture record writes");

        let error = stage
            .recover()
            .await
            .expect_err("an unknown version is never silently upgraded");
        assert!(
            matches!(
                error,
                HotStageError::UnknownVersion {
                    found: 2,
                    expected: 1,
                    ..
                }
            ),
            "unexpected refusal: {error}"
        );
    }

    /// A run whose bytes contradict its record is never served.
    ///
    /// This is the check that makes the WAL-retirement boundary safe: a run
    /// truncated by a crash after fsync of the record must not answer reads.
    ///
    /// # Panics
    ///
    /// Panics when a fixture write is refused or recovery unexpectedly passes.
    #[tokio::test]
    async fn recovery_fails_closed_when_a_run_contradicts_its_record() {
        let directory = tempfile::tempdir().expect("staging root");
        let stage = ScribeHotStage::new(directory.path().join("hot-stage"));
        let key = fixture_key();
        let member = StagedMemberId::new(7, 5);
        let member_directory = stage.member_directory(&key, member);
        let run = write_run(&member_directory, "run-0.parquet", b"complete-run").await;
        let record = StagedHotSourceRecordV1::ready(
            &key,
            member,
            StagedLsnRange { min: 3, max: 9 },
            vec![run],
            ready_at(),
        );
        stage
            .publish_record(&key, &record)
            .await
            .expect("the record publishes");
        tokio::fs::write(member_directory.join("run-0.parquet"), b"truncated")
            .await
            .expect("fixture truncation writes");

        let error = stage
            .recover()
            .await
            .expect_err("contradicted bytes are never served");
        assert!(
            matches!(
                error,
                HotStageError::RunMismatch {
                    expected_bytes: 12,
                    actual_bytes: 9,
                    ..
                }
            ),
            "unexpected refusal: {error}"
        );

        tokio::fs::remove_file(member_directory.join("run-0.parquet"))
            .await
            .expect("fixture removal");
        let missing = stage
            .recover()
            .await
            .expect_err("a missing run is never served");
        assert!(
            matches!(missing, HotStageError::MissingRun { .. }),
            "unexpected refusal: {missing}"
        );
    }

    /// Member state moves forward only, and retirement removes the member.
    ///
    /// # Panics
    ///
    /// Panics when a fixture write, transition, or recovery is refused.
    #[tokio::test]
    async fn member_state_moves_forward_only_until_retirement() {
        let directory = tempfile::tempdir().expect("staging root");
        let stage = ScribeHotStage::new(directory.path().join("hot-stage"));
        let key = fixture_key();
        let member = StagedMemberId::new(9, 2);
        let run = write_run(
            &stage.member_directory(&key, member),
            "run-0.parquet",
            b"member-bytes",
        )
        .await;
        let record = StagedHotSourceRecordV1::ready(
            &key,
            member,
            StagedLsnRange { min: 4, max: 4 },
            vec![run],
            ready_at(),
        );
        stage
            .publish_record(&key, &record)
            .await
            .expect("the record publishes");

        let claim = StagedMemberState::Claimed {
            claim_id: "0011".to_owned(),
        };
        let claimed = stage
            .transition(&key, member, claim.clone())
            .await
            .expect("ready moves to claimed");
        assert_eq!(claimed.state(), &claim);
        assert!(!claimed.state().is_ready());

        // Re-claiming, or returning to ready, would let one member's rows be
        // published under two identities.
        let refused = stage
            .transition(&key, member, claim)
            .await
            .expect_err("a member never re-enters the state it holds");
        assert!(
            matches!(
                refused,
                HotStageError::Backwards {
                    from: "claimed",
                    to: "claimed"
                }
            ),
            "unexpected refusal: {refused}"
        );
        let backwards = stage
            .transition(&key, member, StagedMemberState::Ready)
            .await
            .expect_err("a claimed member never returns to ready");
        assert!(
            matches!(backwards, HotStageError::Backwards { to: "ready", .. }),
            "unexpected refusal: {backwards}"
        );
    }

    /// A published member recovers as published and retires exactly once.
    ///
    /// # Panics
    ///
    /// Panics when a fixture write, transition, or recovery is refused.
    #[tokio::test]
    async fn a_published_member_recovers_and_retires() {
        let directory = tempfile::tempdir().expect("staging root");
        let stage = ScribeHotStage::new(directory.path().join("hot-stage"));
        let key = fixture_key();
        let member = StagedMemberId::new(9, 2);
        let run = write_run(
            &stage.member_directory(&key, member),
            "run-0.parquet",
            b"member-bytes",
        )
        .await;
        let record = StagedHotSourceRecordV1::ready(
            &key,
            member,
            StagedLsnRange { min: 4, max: 4 },
            vec![run],
            ready_at(),
        );
        stage
            .publish_record(&key, &record)
            .await
            .expect("the record publishes");
        stage
            .transition(
                &key,
                member,
                StagedMemberState::Claimed {
                    claim_id: "0011".to_owned(),
                },
            )
            .await
            .expect("ready moves to claimed");

        let published = stage
            .transition(
                &key,
                member,
                StagedMemberState::Published {
                    file_list_commit_key: "commit-1".to_owned(),
                    published_object_identities: vec!["objects/hot-0.parquet".to_owned()],
                    persisted_lsn_ranges: vec![StagedLsnRange { min: 4, max: 4 }],
                },
            )
            .await
            .expect("claimed moves forward to published");
        assert_eq!(published.state().label(), "published");

        let recovered = stage
            .recover()
            .await
            .expect("the published member recovers");
        assert_eq!(
            recovered
                .get(&key)
                .expect("the member recovers under its key")[0]
                .record()
                .state()
                .label(),
            "published"
        );

        stage
            .retire(&key, member)
            .await
            .expect("retirement removes it");
        assert!(
            stage
                .recover()
                .await
                .expect("recovery scans an empty root")
                .is_empty()
        );
        let unknown = stage
            .transition(&key, member, StagedMemberState::CleanupPending)
            .await
            .expect_err("a retired member is unknown");
        assert!(
            matches!(
                unknown,
                HotStageError::UnknownMember {
                    shard: 9,
                    generation: 2
                }
            ),
            "unexpected refusal: {unknown}"
        );
        stage
            .retire(&key, member)
            .await
            .expect("retiring an absent member is idempotent");
    }

    /// An unsettled claim's members come back owned by that same claim.
    ///
    /// Restoring them as free would let a restarted pod take a second claim
    /// over rows an interrupted publication may already have written.
    ///
    /// # Panics
    ///
    /// Panics when a fixture write, transition, or recovery is refused.
    #[tokio::test]
    async fn a_claimed_member_recovers_owned_by_its_claim() {
        let directory = tempfile::tempdir().expect("staging root");
        let stage = ScribeHotStage::new(directory.path().join("hot-stage"));
        let key = fixture_key();
        let member = StagedMemberId::new(6, 1);
        let run = write_run(
            &stage.member_directory(&key, member),
            "run-0.parquet",
            b"claimed-bytes",
        )
        .await;
        let record = StagedHotSourceRecordV1::ready(
            &key,
            member,
            StagedLsnRange { min: 8, max: 8 },
            vec![run],
            ready_at(),
        );
        stage
            .publish_record(&key, &record)
            .await
            .expect("the record publishes");
        let claim_id = "a".repeat(64);
        let claim = StagingClaimId::from_hex(&claim_id).expect("the fixture identity parses");
        stage
            .transition(
                &key,
                member,
                StagedMemberState::Claimed {
                    claim_id: claim_id.clone(),
                },
            )
            .await
            .expect("ready moves to claimed");

        let recovered = stage.recover().await.expect("the claimed member recovers");
        let staged = &recovered
            .get(&key)
            .expect("the member recovers under its key")[0];
        assert_eq!(
            staged.recovered().expect("the member projects"),
            Some(RecoveredMember::Claimed {
                claim,
                member: staged
                    .record()
                    .ready_member("member.staged.json")
                    .expect("the member projects into the ready index"),
            })
        );

        // Publishing keeps the same ownership: the operation may still be
        // uncertain, and the claim that wrote it is the one that reconciles it.
        stage
            .transition(
                &key,
                member,
                StagedMemberState::Publishing {
                    claim_id,
                    operation_id: Uuid::from_u128(5),
                },
            )
            .await
            .expect("claimed moves to publishing");
        let recovered = stage
            .recover()
            .await
            .expect("the publishing member recovers");
        let staged = &recovered
            .get(&key)
            .expect("the member recovers under its key")[0];
        assert!(matches!(
            staged.recovered().expect("the member projects"),
            Some(RecoveredMember::Claimed { .. })
        ));

        // Once published, a hot object serves the rows and the member is no
        // longer assembly work.
        stage
            .transition(
                &key,
                member,
                StagedMemberState::Published {
                    file_list_commit_key: "commit-1".to_owned(),
                    published_object_identities: vec!["objects/hot-0.parquet".to_owned()],
                    persisted_lsn_ranges: vec![StagedLsnRange { min: 8, max: 8 }],
                },
            )
            .await
            .expect("publishing moves to published");
        let recovered = stage
            .recover()
            .await
            .expect("the published member recovers");
        let staged = &recovered
            .get(&key)
            .expect("the member recovers under its key")[0];
        assert_eq!(staged.recovered().expect("the member projects"), None);
    }

    /// A claim identity a record cannot have written refuses rather than resumes.
    ///
    /// # Panics
    ///
    /// Panics when a fixture write or recovery is refused.
    #[tokio::test]
    async fn a_malformed_claim_identity_refuses_recovery_projection() {
        let directory = tempfile::tempdir().expect("staging root");
        let stage = ScribeHotStage::new(directory.path().join("hot-stage"));
        let key = fixture_key();
        let member = StagedMemberId::new(3, 4);
        let run = write_run(
            &stage.member_directory(&key, member),
            "run-0.parquet",
            b"claimed-bytes",
        )
        .await;
        let record = StagedHotSourceRecordV1::ready(
            &key,
            member,
            StagedLsnRange { min: 2, max: 2 },
            vec![run],
            ready_at(),
        );
        stage
            .publish_record(&key, &record)
            .await
            .expect("the record publishes");
        stage
            .transition(
                &key,
                member,
                StagedMemberState::Claimed {
                    claim_id: "not-a-digest".to_owned(),
                },
            )
            .await
            .expect("ready moves to claimed");

        let recovered = stage.recover().await.expect("the record itself is valid");
        let staged = &recovered
            .get(&key)
            .expect("the member recovers under its key")[0];
        let error = staged
            .recovered()
            .expect_err("an unrenderable claim identity is never resumed");
        assert!(
            matches!(error, HotStageError::Invalid { .. }),
            "unexpected refusal: {error}"
        );
    }
}
