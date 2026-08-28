//! Freeze-to-staged ownership — turning one frozen bucket into a staged member.
//!
//! A frozen `SealKey` bucket holds the only copy of its rows outside the WAL.
//! [`ScribeMemberStager`] is the owner that ends that condition: it sorts and
//! encodes the bucket into local Parquet runs, fsyncs them, proves they decode,
//! and publishes the [`StagedHotSourceRecordV1`] that makes the member a query
//! source. Only after that record lands may the frozen Arrow be released and
//! the member's WAL be considered for cohort retirement.
//!
//! The work splits in two because the runtime does: encoding is CPU-bound
//! blocking work that belongs on the persistence CPU lane, while publishing the
//! record is durable async IO owned by [`ScribeHotStage`]. [`StagedRuns`] is
//! the handoff between them, and it is deliberately inert — holding it proves
//! nothing about visibility, because a member becomes visible only when its
//! record is renamed into place.
//!
//! Nothing here decides *when* a member is claimed or published. That is the
//! assembler's decision, taken over the ready index this owner feeds.

use std::path::Path;
use std::sync::Arc;

use chrono::{DateTime, Utc};

use crate::catalog::TenantTableBinding;
use crate::catalog::layout::PhysicalLayout;
use crate::contracts::ScribeError;
use crate::scribe::assembly::{ReadyMember, ScribeAssemblyKey, StagedMemberId};
use crate::scribe::hot_stage::{
    ScribeHotStage, StagedHotSourceRecordV1, StagedLsnRange, StagedRunFile,
};
use crate::scribe::memtable::FrozenMemtable;
use crate::scribe::stream_identity::{NodeId, WriterEpoch};

/// Rows decoded by the bounded preflight that proves a run is readable.
const PREFLIGHT_BATCH_ROWS: usize = 1_024;

/// Immutable origin facts one freeze contributes to its staged member.
///
/// These come from the rotating shard, not from the frozen bucket, because the
/// bucket alone cannot say which shard generation or WAL range produced it.
#[derive(Debug, Clone, Copy)]
pub struct StagedMemberOrigin {
    /// Node whose staging volume will hold the runs.
    pub node_id: NodeId,
    /// Fenced writer epoch whose LSN stream produced the rows.
    pub writer_epoch: WriterEpoch,
    /// Shard that froze the member.
    pub shard: u16,
    /// Generation ordinal of that shard.
    pub generation: u64,
    /// Inclusive WAL range the member covers.
    pub wal: StagedLsnRange,
}

/// Borrowed inputs for encoding one frozen bucket into local runs.
pub struct StageMemberRequest<'a> {
    /// Frozen bucket whose rows become the member.
    pub frozen: &'a FrozenMemtable,
    /// Tenant-qualified physical binding revalidated by the encoder.
    pub binding: &'a TenantTableBinding,
    /// Registered physical write recipe the runs are sorted and encoded under.
    pub layout: &'a PhysicalLayout,
    /// Shard, epoch, node, and WAL facts describing where the rows came from.
    pub origin: StagedMemberOrigin,
    /// Move-only footer memory child retained through sealed inspection.
    pub footer_reservation: crate::scribe::memory::EncodedFooterReservation,
}

/// Fsynced, preflighted runs waiting only for their record to be published.
///
/// A value of this type is durable but invisible: recovery ignores a member
/// directory that holds no record, so dropping it strands files that startup
/// cleanup removes rather than exposing a partial member.
#[derive(Debug, Clone)]
pub struct StagedRuns {
    /// Compatibility scope the member may later be claimed under.
    key: ScribeAssemblyKey,
    /// Immutable member identity within that scope.
    member: StagedMemberId,
    /// Inclusive WAL range the runs cover.
    wal: StagedLsnRange,
    /// Sorted runs in sort order, named relative to the member directory.
    runs: Vec<StagedRunFile>,
}

impl StagedRuns {
    /// Returns the compatibility scope the runs were encoded under.
    pub const fn key(&self) -> &ScribeAssemblyKey {
        &self.key
    }

    /// Returns the immutable member identity of these runs.
    pub const fn member(&self) -> StagedMemberId {
        self.member
    }

    /// Returns the fsynced runs in sort order.
    pub fn runs(&self) -> &[StagedRunFile] {
        &self.runs
    }
}

/// Encodes frozen buckets into durable local runs and stages them ready.
#[derive(Debug)]
pub struct ScribeMemberStager {
    /// Durable staged namespace owning member directories and records.
    stage: Arc<ScribeHotStage>,
}

impl ScribeMemberStager {
    /// Binds the stager to the pod's durable staged namespace.
    pub const fn new(stage: Arc<ScribeHotStage>) -> Self {
        Self { stage }
    }

    /// Sorts, encodes, fsyncs, and preflights one frozen bucket's runs.
    ///
    /// This is the blocking half and belongs on the persistence CPU lane. It
    /// writes into the member directory rather than ephemeral scratch, so the
    /// bytes that were validated are the bytes the record will name; a scratch
    /// copy afterwards would revalidate a different file. The directory stays
    /// invisible to recovery until [`Self::publish_ready`] renames the record
    /// in, so a crash here leaves only removable residue.
    ///
    /// Re-encoding an already staged member is refused rather than silently
    /// overwriting its runs: the record that names them is the authority a WAL
    /// segment may already have been retired against.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::Internal`] when the member directory cannot be
    /// created, a member is already staged, encoding or its footer inspection
    /// fails, a run cannot be fsynced or decoded, or a sealed checksum is not
    /// the exact digest the record must carry.
    pub fn encode_runs(&self, request: StageMemberRequest<'_>) -> Result<StagedRuns, ScribeError> {
        let key = ScribeAssemblyKey::new(
            request.frozen.seal_key.tenant,
            request.frozen.seal_key.table.clone(),
            crate::parquet::memory::schema_fingerprint(request.frozen.schema.as_ref()),
            request.layout,
            request.frozen.seal_key.partition,
            request.origin.node_id,
            request.origin.writer_epoch,
        );
        let member = StagedMemberId::new(request.origin.shard, request.origin.generation);
        let directory = self.stage.member_directory(&key, member);
        if directory.exists() {
            return Err(ScribeError::Internal {
                detail: format!(
                    "staged member {}-{} already occupies its durable directory",
                    member.shard(),
                    member.generation()
                ),
            });
        }
        std::fs::create_dir_all(&directory).map_err(|error| ScribeError::Internal {
            detail: format!("create the staged member directory: {error}"),
        })?;
        let encoded = crate::scribe::parquet_writer::encode_batch(
            request.frozen,
            request.binding,
            request.frozen.seal_key.tenant,
            &directory,
            &run_object_base(&key, member),
            request.layout,
            request.footer_reservation,
        )?;
        let mut runs = Vec::with_capacity(encoded.artifacts.len());
        for artifact in &encoded.artifacts {
            let file_name = file_name_of(&artifact.scratch_path)?;
            fsync_file(&artifact.scratch_path)?;
            preflight_run(&artifact.scratch_path)?;
            runs.push(StagedRunFile::new(
                file_name,
                artifact.file_size,
                u64::try_from(artifact.row_count).map_err(|_| ScribeError::Internal {
                    detail: "staged run row count exceeds u64".to_owned(),
                })?,
                run_digest(&artifact.checksum)?,
            ));
        }
        encoded.artifacts.retain_for_reconciliation();
        fsync_directory(&directory)?;
        Ok(StagedRuns {
            key,
            member,
            wal: request.origin.wal,
            runs,
        })
    }

    /// Publishes the record that makes the staged member a query source.
    ///
    /// This is the durable boundary the WAL retires against: when it returns,
    /// the member's rows survive and are servable without the WAL behind them.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::Internal`] when the record cannot be published
    /// durably or the published member carries no bytes or rows.
    pub async fn publish_ready(
        &self,
        staged: StagedRuns,
        ready_at: DateTime<Utc>,
    ) -> Result<ReadyMember, ScribeError> {
        let record = StagedHotSourceRecordV1::ready(
            &staged.key,
            staged.member,
            staged.wal,
            staged.runs,
            ready_at,
        );
        self.stage
            .publish_record(&staged.key, &record)
            .await
            .map_err(|error| ScribeError::Internal {
                detail: format!("publish the staged member record: {error}"),
            })?;
        record
            .ready_member(crate::scribe::hot_stage::RECORD_FILE_NAME)
            .map_err(|error| ScribeError::Internal {
                detail: format!("staged member is not a claimable ready member: {error}"),
            })
    }
}

/// Builds the deterministic local object base stamped into a member's runs.
///
/// Local runs are never uploaded, so their identity only has to be stable
/// across a retry of the same member; deriving it from the assembly key and the
/// member makes a re-encode reproduce the same footer identity.
fn run_object_base(key: &ScribeAssemblyKey, member: StagedMemberId) -> String {
    let digest = key.digest();
    let mut identity = String::with_capacity(2 * digest.len() + 24);
    identity.push_str("staged/");
    for byte in digest {
        use std::fmt::Write as _;
        let _ = write!(identity, "{byte:02x}");
    }
    let _ = {
        use std::fmt::Write as _;
        write!(identity, "/{}-{}", member.shard(), member.generation())
    };
    identity
}

/// Returns one sealed run's file name relative to its member directory.
///
/// # Errors
///
/// Returns [`ScribeError::Internal`] when the path has no final component or
/// that component is not valid UTF-8, neither of which the encoder produces.
fn file_name_of(path: &Path) -> Result<String, ScribeError> {
    path.file_name()
        .and_then(|name| name.to_str())
        .map(str::to_owned)
        .ok_or_else(|| ScribeError::Internal {
            detail: format!(
                "sealed run path `{}` has no usable file name",
                path.display()
            ),
        })
}

/// Decodes one sealed artifact's lowercase hex checksum into record form.
///
/// # Errors
///
/// Returns [`ScribeError::Internal`] when the checksum is not exactly 32 hex
/// bytes, which would let a record describe a digest it cannot verify.
fn run_digest(checksum: &str) -> Result<[u8; 32], ScribeError> {
    let bytes = hex::decode(checksum).map_err(|error| ScribeError::Internal {
        detail: format!("sealed run checksum is not hex: {error}"),
    })?;
    <[u8; 32]>::try_from(bytes.as_slice()).map_err(|_| ScribeError::Internal {
        detail: "sealed run checksum is not a 32-byte digest".to_owned(),
    })
}

/// Proves one fsynced run decodes before it is offered as a query source.
///
/// The preflight is bounded to one small batch: it exists to catch a run that
/// is structurally sealed but undecodable, not to re-read the member.
///
/// # Errors
///
/// Returns [`ScribeError::Internal`] when the run cannot be opened, its reader
/// cannot be built, decoding fails, or it yields no rows.
fn preflight_run(path: &Path) -> Result<(), ScribeError> {
    let file = std::fs::File::open(path).map_err(|error| ScribeError::Internal {
        detail: format!("open the staged run for preflight: {error}"),
    })?;
    let mut reader = parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder::try_new(file)
        .map_err(|error| ScribeError::Internal {
            detail: format!("build the staged run preflight reader: {error}"),
        })?
        .with_batch_size(PREFLIGHT_BATCH_ROWS)
        .build()
        .map_err(|error| ScribeError::Internal {
            detail: format!("start the staged run preflight read: {error}"),
        })?;
    let batch = reader
        .next()
        .transpose()
        .map_err(|error| ScribeError::Internal {
            detail: format!("decode the staged run preflight batch: {error}"),
        })?
        .ok_or_else(|| ScribeError::Internal {
            detail: "staged run decoded no rows".to_owned(),
        })?;
    if batch.num_rows() == 0 {
        return Err(ScribeError::Internal {
            detail: "staged run decoded an empty batch".to_owned(),
        });
    }
    Ok(())
}

/// Flushes one sealed run to stable storage.
///
/// # Errors
///
/// Returns [`ScribeError::Internal`] when the file cannot be opened or synced.
fn fsync_file(path: &Path) -> Result<(), ScribeError> {
    std::fs::File::open(path)
        .and_then(|file| file.sync_all())
        .map_err(|error| ScribeError::Internal {
            detail: format!("fsync the staged run: {error}"),
        })
}

/// Flushes one member directory's entries to stable storage.
///
/// # Errors
///
/// Returns [`ScribeError::Internal`] when the directory cannot be opened or
/// synced.
fn fsync_directory(path: &Path) -> Result<(), ScribeError> {
    std::fs::File::open(path)
        .and_then(|directory| directory.sync_all())
        .map_err(|error| ScribeError::Internal {
            detail: format!("fsync the staged member directory: {error}"),
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use arrow::array::{RecordBatch, StringArray, TimestampMicrosecondArray};
    use arrow::datatypes::{DataType, Field, Schema, TimeUnit};
    use wyrd_spec::ids::DataTenantId;

    use crate::catalog::TableRef;
    use crate::namespaces::BifrostNamespace;
    use crate::scribe::seal_key::SealKey;

    /// Builds one frozen bucket with the managed columns the encoder requires.
    fn frozen_member(tenant: DataTenantId, rows: usize) -> FrozenMemtable {
        let tenant_value = tenant.to_string();
        let row_count = i64::try_from(rows).expect("test row count fits i64");
        let schema = Arc::new(Schema::new(vec![
            Field::new("data_tenant_id", DataType::Utf8, false),
            Field::new(
                "wyrd_event_time",
                DataType::Timestamp(TimeUnit::Microsecond, None),
                false,
            ),
        ]));
        let batch = RecordBatch::try_new(
            Arc::clone(&schema),
            vec![
                Arc::new(StringArray::from(vec![tenant_value.as_str(); rows])),
                Arc::new(TimestampMicrosecondArray::from_iter_values(0..row_count)),
            ],
        )
        .expect("frozen member fixture");
        FrozenMemtable {
            seal_id: 11,
            seal_key: SealKey::new(
                tenant,
                TableRef::new(BifrostNamespace::Bifrost, "staged_member"),
                crate::test_support::day_partition(2026, 7, 14),
            ),
            shard_id: 5,
            schema,
            batches: vec![batch],
            events: vec![],
            metas: vec![],
            opened_at: std::time::Instant::now(),
            closed_at: std::time::Instant::now(),
            arrow_bytes: 0,
        }
    }

    /// Resolves the registered hourly layout every fixture is encoded under.
    fn member_layout(schema: &Schema) -> PhysicalLayout {
        PhysicalLayout::resolve(
            "vala.bifrost.test",
            schema,
            Some(&crate::tables::hourly_layout(
                vec![crate::tables::sort_asc("wyrd_event_time")],
                &["wyrd_event_time"],
            )),
        )
        .expect("test schema carries the built-in hourly layout")
    }

    /// Builds the origin facts a rotating shard contributes to one member.
    fn origin() -> StagedMemberOrigin {
        StagedMemberOrigin {
            node_id: NodeId::new(uuid::Uuid::from_u128(0x5eed)),
            writer_epoch: WriterEpoch::new(3),
            shard: 5,
            generation: 9,
            wal: StagedLsnRange { min: 40, max: 96 },
        }
    }

    /// A staged member survives without its WAL: the runs it names are fsynced
    /// in the member directory, decode, and recover as a claimable ready member
    /// whose bytes and rows equal what was actually written.
    #[tokio::test]
    async fn staged_member_recovers_ready_with_the_runs_it_named() {
        let root = tempfile::tempdir().expect("staged root");
        let stage = Arc::new(ScribeHotStage::new(root.path().to_path_buf()));
        let stager = ScribeMemberStager::new(Arc::clone(&stage));
        let tenant = DataTenantId::new_v7();
        let frozen = frozen_member(tenant, 512);
        let binding = TenantTableBinding::resolve((tenant, frozen.seal_key.table.clone()))
            .expect("tenant binding");
        let layout = member_layout(frozen.schema.as_ref());
        let member = stager
            .encode_runs(StageMemberRequest {
                frozen: &frozen,
                binding: &binding,
                layout: &layout,
                origin: origin(),
                footer_reservation: crate::scribe::memory::EncodedFooterReservation::for_test(),
            })
            .expect("frozen member encodes into local runs");
        assert_eq!(member.member(), StagedMemberId::new(5, 9));
        assert_eq!(member.runs().len(), 1);

        let recovered_before = stage.recover().await.expect("scan before publication");
        assert!(
            recovered_before.is_empty(),
            "runs without a record are never a query authority"
        );

        let ready_at = chrono::DateTime::from_timestamp(1_800_000_000, 0).expect("fixture instant");
        let ready = stager
            .publish_ready(member.clone(), ready_at)
            .await
            .expect("staged member publishes ready");
        assert_eq!(ready.id(), StagedMemberId::new(5, 9));
        assert_eq!(ready.rows(), 512);

        let recovered = stage.recover().await.expect("scan after publication");
        let members = recovered
            .get(member.key())
            .expect("the member recovers under its own assembly key");
        assert_eq!(members.len(), 1);
        let record = members[0].record();
        assert!(record.state().is_ready());
        assert_eq!(record.rows(), 512);
        assert_eq!(record.encoded_bytes(), ready.encoded_bytes());
        assert_eq!(record.wal_range().min, 40);
        assert_eq!(record.wal_range().max, 96);
        for path in members[0].run_paths() {
            assert!(path.exists(), "recovery names a run that exists: {path:?}");
        }
    }

    /// Re-encoding a member that already owns its durable directory is refused:
    /// its record may already have authorized retiring the WAL behind it, so
    /// overwriting the runs it names would remove the only copy of those rows.
    #[tokio::test]
    async fn restaging_an_existing_member_is_refused() {
        let root = tempfile::tempdir().expect("staged root");
        let stage = Arc::new(ScribeHotStage::new(root.path().to_path_buf()));
        let stager = ScribeMemberStager::new(Arc::clone(&stage));
        let tenant = DataTenantId::new_v7();
        let frozen = frozen_member(tenant, 64);
        let binding = TenantTableBinding::resolve((tenant, frozen.seal_key.table.clone()))
            .expect("tenant binding");
        let layout = member_layout(frozen.schema.as_ref());
        let request = || StageMemberRequest {
            frozen: &frozen,
            binding: &binding,
            layout: &layout,
            origin: origin(),
            footer_reservation: crate::scribe::memory::EncodedFooterReservation::for_test(),
        };
        stager
            .encode_runs(request())
            .expect("first staging encodes");
        let refusal = stager
            .encode_runs(request())
            .expect_err("second staging is refused");
        assert!(
            refusal.to_string().contains("already occupies"),
            "refusal names the durable collision: {refusal}"
        );
    }
}
