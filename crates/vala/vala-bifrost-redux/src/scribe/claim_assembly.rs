//! Claim assembly — turning one claim's staged members into hot objects.
//!
//! The assembler ([`crate::scribe::assembly::StagingAssembler`]) decides *which*
//! members become one claim. This module does the work that decision authorizes:
//! it re-validates every member the claim names, merges their runs in one total
//! order, and writes rolling Parquet objects that close around the configured
//! target.
//!
//! The work splits the way the runtime does. Gathering runs is durable async IO
//! against the staged namespace; merging and encoding is blocking CPU work that
//! belongs on the persistence lane. [`ClaimRuns`] is the handoff, and it carries
//! the claim's own row and byte facts so the encode half can prove it wrote
//! exactly the rows the claim promised rather than trusting its own count.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use arrow::datatypes::SchemaRef;
use wyrd_spec::vala::api::AuditEvent;

use crate::catalog::layout::PhysicalLayout;
use crate::contracts::ScribeError;
use crate::scribe::assembly::{StagingClaim, StagingClaimId};
use crate::scribe::claim_merge::StagedRunMerge;
use crate::scribe::hot_stage::{ScribeHotStage, StagedLsnRange, StagedMemberState};
use crate::scribe::parquet_writer::{
    ArtifactPlan, BoundedParquetArtifactSet, RowGroupStats, encode_ordered_claim,
};

/// Rows decoded from each run at a time while merging one claim.
///
/// The merge holds one decoded batch per run, so this is what bounds a claim's
/// Arrow footprint: members, not object size, decide how much memory assembly
/// owns.
const MERGE_BATCH_ROWS: usize = 8 * 1024;

/// One claim's validated runs, ready for the blocking merge and encode.
#[derive(Debug, Clone)]
pub struct ClaimRuns {
    /// Identity of the claim these runs belong to.
    claim: StagingClaimId,
    /// Every member's runs, in claim member order then sort order.
    runs: Vec<PathBuf>,
    /// Rows the claim's members promised between them.
    rows: u64,
    /// Union of the members' WAL ranges, which the objects make retirable.
    wal: StagedLsnRange,
    /// Seal audit event the claim's publication event derives from.
    publication_audit: Option<AuditEvent>,
}

impl ClaimRuns {
    /// Returns the identity of the claim these runs belong to.
    #[must_use]
    pub const fn claim(&self) -> StagingClaimId {
        self.claim
    }

    /// Returns the validated run paths in merge order.
    #[must_use]
    pub fn runs(&self) -> &[PathBuf] {
        &self.runs
    }

    /// Returns the WAL span the claim's published objects will cover.
    #[must_use]
    pub const fn wal(&self) -> StagedLsnRange {
        self.wal
    }

    /// Returns the seal audit event the publication event derives from.
    #[must_use]
    pub const fn publication_audit(&self) -> Option<&AuditEvent> {
        self.publication_audit.as_ref()
    }
}

/// Borrowed inputs for encoding one claim into rolling hot objects.
pub struct AssembleClaimRequest<'a> {
    /// Validated runs gathered for the claim.
    pub runs: &'a ClaimRuns,
    /// Physical schema shared by every run and every emitted object.
    pub schema: SchemaRef,
    /// Registered physical write recipe the merge and writer obey.
    pub layout: &'a PhysicalLayout,
    /// Directory receiving the sealed objects before upload.
    pub scratch_dir: &'a Path,
    /// Deterministic object identity prefix for this claim's objects.
    pub object_base: &'a str,
    /// Approximate encoded size at which one object closes and the next opens.
    pub target_object_bytes: u64,
    /// Move-only footer memory child retained through sealed inspection.
    pub footer_reservation: crate::scribe::memory::EncodedFooterReservation,
}

/// One claim's sealed objects, ready for upload and fenced publication.
#[derive(Debug)]
pub struct AssembledClaim {
    /// Identity of the claim that produced the objects.
    pub claim: StagingClaimId,
    /// Contiguous sealed objects in publication order.
    pub artifacts: BoundedParquetArtifactSet,
    /// Footer-derived statistics for every row group in write order.
    pub row_group_stats: Vec<RowGroupStats>,
    /// Rows written across the objects, equal to the claim's promised rows.
    pub rows: u64,
}

/// Merges and encodes the members of one claim.
#[derive(Debug)]
pub struct ClaimAssembler {
    /// Durable staged namespace holding the members and their runs.
    stage: Arc<ScribeHotStage>,
}

impl ClaimAssembler {
    /// Binds the assembler to the pod's durable staged namespace.
    #[must_use]
    pub const fn new(stage: Arc<ScribeHotStage>) -> Self {
        Self { stage }
    }

    /// Re-validates every member of a claim and returns its runs in merge order.
    ///
    /// Validation is not redundant with the assembler's ready index: the index
    /// records what was true when the member became ready, while this proves the
    /// bytes are still exactly those the record names at the moment they are
    /// about to be published under a new identity.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::Internal`] when a member's record is missing or
    /// unreadable, a run contradicts its recorded length or digest, or the
    /// member's record disagrees with the claim's key.
    pub async fn gather(&self, claim: &StagingClaim) -> Result<ClaimRuns, ScribeError> {
        let mut runs = Vec::new();
        let mut wal: Option<StagedLsnRange> = None;
        let mut publication_audit = None;
        for member in claim.members() {
            let staged = self
                .stage
                .member(claim.key(), member.id())
                .await
                .map_err(|error| ScribeError::Internal {
                    detail: format!(
                        "validate staged member {}-{} for its claim: {error}",
                        member.id().shard(),
                        member.id().generation()
                    ),
                })?;
            if staged.key() != claim.key() {
                return Err(ScribeError::Internal {
                    detail: format!(
                        "staged member {}-{} belongs to another assembly key",
                        member.id().shard(),
                        member.id().generation()
                    ),
                });
            }
            let staged = self.own_member(claim, staged).await?;
            let member_wal = staged.record().wal_range();
            wal = Some(
                wal.map_or(member_wal, |span: StagedLsnRange| StagedLsnRange {
                    min: span.min.min(member_wal.min),
                    max: span.max.max(member_wal.max),
                }),
            );
            if publication_audit.is_none() {
                publication_audit = staged.record().publication_audit().cloned();
            }
            runs.extend(staged.run_paths());
        }
        if runs.is_empty() {
            return Err(ScribeError::Internal {
                detail: "a claim named no runs to assemble".to_owned(),
            });
        }
        let wal = wal.ok_or_else(|| ScribeError::Internal {
            detail: "a claim named no members to assemble".to_owned(),
        })?;
        Ok(ClaimRuns {
            claim: claim.id(),
            runs,
            rows: claim.rows(),
            wal,
            publication_audit,
        })
    }

    /// Records the claim's durable ownership of one member before any merge.
    ///
    /// Membership is durable before the merge begins so an interrupted claim
    /// resumes as itself: the claim identity is derived from its member set, so
    /// a restart re-derives the same claim and finds its members already owned
    /// rather than free to join a different one.
    ///
    /// A member this claim already owns is left as it is, which is what makes
    /// the gather idempotent across a retry.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::Internal`] when the member is owned by another
    /// claim, has already been published, or the durable transition fails.
    async fn own_member(
        &self,
        claim: &StagingClaim,
        staged: crate::scribe::hot_stage::StagedMember,
    ) -> Result<crate::scribe::hot_stage::StagedMember, ScribeError> {
        let member = staged.record().member();
        let claim_id = claim.id().to_string();
        match staged.record().state() {
            StagedMemberState::Ready => {}
            StagedMemberState::Claimed { claim_id: owner }
            | StagedMemberState::Publishing {
                claim_id: owner, ..
            } if *owner == claim_id => return Ok(staged),
            state => {
                return Err(ScribeError::Internal {
                    detail: format!(
                        "staged member {}-{} is {} and cannot join this claim",
                        member.shard(),
                        member.generation(),
                        state.label()
                    ),
                });
            }
        }
        self.stage
            .transition(claim.key(), member, StagedMemberState::Claimed { claim_id })
            .await
            .map_err(|error| ScribeError::Internal {
                detail: format!(
                    "record the claim's ownership of staged member {}-{}: {error}",
                    member.shard(),
                    member.generation()
                ),
            })?;
        self.stage
            .member(claim.key(), member)
            .await
            .map_err(|error| ScribeError::Internal {
                detail: format!(
                    "reload staged member {}-{} after claiming it: {error}",
                    member.shard(),
                    member.generation()
                ),
            })
    }

    /// Merges the gathered runs and encodes them into rolling hot objects.
    ///
    /// This is the blocking half and belongs on the persistence CPU lane. It
    /// refuses to return objects whose rows do not sum to the claim's promised
    /// rows, because publishing fewer rows than a claim owns would retire the
    /// staged members that hold the difference.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::Internal`] when the merge cannot open or decode a
    /// run, encoding refuses a batch or cannot seal an object, or the written
    /// rows are not exactly the rows the claim promised.
    pub fn encode(&self, request: AssembleClaimRequest<'_>) -> Result<AssembledClaim, ScribeError> {
        let merge = StagedRunMerge::open(
            request.runs.runs(),
            request.schema,
            request.layout,
            MERGE_BATCH_ROWS,
        )?;
        let (artifacts, row_group_stats) = encode_ordered_claim(
            ArtifactPlan {
                scratch_dir: request.scratch_dir,
                object_base: request.object_base,
                layout: request.layout,
                first_ordinal: 0,
                target_object_bytes: request.target_object_bytes,
            },
            request.footer_reservation,
            merge,
        )?;
        let rows = artifacts.iter().try_fold(0_u64, |sum, artifact| {
            u64::try_from(artifact.row_count)
                .ok()
                .and_then(|rows| sum.checked_add(rows))
                .ok_or_else(|| ScribeError::Internal {
                    detail: "assembled claim row count overflowed".to_owned(),
                })
        })?;
        if rows != request.runs.rows {
            return Err(ScribeError::Internal {
                detail: format!(
                    "assembled claim wrote {rows} rows but its members promised {}",
                    request.runs.rows
                ),
            });
        }
        Ok(AssembledClaim {
            claim: request.runs.claim,
            artifacts,
            row_group_stats,
            rows,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use arrow::array::{
        FixedSizeBinaryArray, Int32Array, RecordBatch, StringArray, TimestampMicrosecondArray,
    };
    use arrow::datatypes::{DataType, Field, Schema, TimeUnit};
    use wyrd_spec::ids::DataTenantId;

    use crate::catalog::{TableRef, TenantTableBinding};
    use crate::namespaces::BifrostNamespace;
    use crate::scribe::assembly::{
        ClaimCause, StagedMemberId, StagingAssembler, StagingAssemblerConfig,
    };
    use crate::scribe::member_stager::{
        ScribeMemberStager, StageMemberRequest, StagedMemberOrigin,
    };
    use crate::scribe::memtable::FrozenMemtable;
    use crate::scribe::seal_key::SealKey;
    use crate::scribe::stream_identity::{NodeId, WriterEpoch};

    /// Physical schema the fixtures stage, merge, and assemble under.
    fn assembly_schema() -> Arc<Schema> {
        Arc::new(Schema::new(vec![
            Field::new("data_tenant_id", DataType::Utf8, false),
            Field::new(
                "wyrd_event_time",
                DataType::Timestamp(TimeUnit::Microsecond, None),
                false,
            ),
            Field::new("wyrd_batch_id", DataType::FixedSizeBinary(16), false),
            Field::new("wyrd_row_ordinal", DataType::Int32, false),
        ]))
    }

    /// Freezes one member whose rows interleave with the other member's rows.
    ///
    /// `offset` shifts the member's event times by one microsecond so the two
    /// members' rows must interleave in the merged object; a merge that simply
    /// concatenated the runs would produce a different, detectable order.
    fn frozen_member(tenant: DataTenantId, rows: i64, offset: i64, batch: u8) -> FrozenMemtable {
        let schema = assembly_schema();
        let tenant_value = tenant.to_string();
        let count = usize::try_from(rows).expect("fixture row count fits usize");
        let times =
            TimestampMicrosecondArray::from_iter_values((0..rows).map(|row| row * 2 + offset));
        let batch_ids = FixedSizeBinaryArray::try_from_iter((0..rows).map(|_| [batch; 16]))
            .expect("fixture batch identity");
        let ordinals = Int32Array::from_iter_values(
            (0..rows).map(|row| i32::try_from(row).unwrap_or(i32::MAX)),
        );
        let record = RecordBatch::try_new(
            Arc::clone(&schema),
            vec![
                Arc::new(StringArray::from(vec![tenant_value.as_str(); count])),
                Arc::new(times),
                Arc::new(batch_ids),
                Arc::new(ordinals),
            ],
        )
        .expect("fixture member batch");
        FrozenMemtable {
            seal_id: u64::from(batch),
            seal_key: SealKey::new(
                tenant,
                TableRef::new(BifrostNamespace::Bifrost, "assembled_claim"),
                crate::test_support::day_partition(2026, 7, 14),
            ),
            shard_id: usize::from(batch),
            schema,
            batches: vec![record],
            events: vec![],
            metas: vec![],
            opened_at: std::time::Instant::now(),
            closed_at: std::time::Instant::now(),
            arrow_bytes: 0,
        }
    }

    /// Resolves the hourly layout every fixture member is encoded under.
    fn assembly_layout(schema: &Schema) -> PhysicalLayout {
        PhysicalLayout::resolve(
            "vala.bifrost.test",
            schema,
            Some(&crate::tables::hourly_layout(
                vec![crate::tables::sort_asc("wyrd_event_time")],
                &["wyrd_event_time"],
            )),
        )
        .expect("fixture schema carries the built-in hourly layout")
    }

    /// Registers a governed staging volume rooted at the stage's own root.
    fn staging_volume(base: &Path, stage_root: &Path) -> crate::resources::StageVolume {
        let wal = base.join("wal");
        let scribe_output = base.join("scribe-output-scratch");
        let forge = base.join("forge");
        let oracle = base.join("oracle");
        for path in [&wal, &scribe_output, &forge, &oracle] {
            std::fs::create_dir_all(path).expect("registered volume root");
        }
        crate::resources::BifrostVolumeGovernor::register(
            crate::resources::BifrostVolumeRoots {
                wal,
                scribe_stage: stage_root.to_owned(),
                scribe_output_scratch: scribe_output,
                forge_scratch: forge,
                oracle_scratch: oracle,
            },
            1024 * 1024 * 1024,
            crate::resources::BifrostResourceHealth::default(),
        )
        .expect("staging volume registration")
        .capabilities()
        .scribe_stage
    }

    /// Stages two interleaving members of one key and claims them together.
    ///
    /// Returning the claim rather than the members keeps the test's subject —
    /// what assembly does with a claim — separate from the staging that any
    /// claim requires.
    async fn stage_two_members(
        stager: &ScribeMemberStager,
        tenant: DataTenantId,
        layout: &PhysicalLayout,
        ready_at: chrono::DateTime<chrono::Utc>,
    ) -> crate::scribe::assembly::StagingClaim {
        let node_id = NodeId::new(uuid::Uuid::from_u128(0xa55e));
        let writer_epoch = WriterEpoch::new(4);
        let mut assembler = StagingAssembler::new(
            StagingAssemblerConfig::new(512 * 1024 * 1024, std::time::Duration::from_mins(5), 4)
                .expect("assembler controls"),
        );
        let mut key = None;
        for (shard, offset) in [(1_u8, 0_i64), (2_u8, 1_i64)] {
            let frozen = frozen_member(tenant, 1_024, offset, shard);
            let binding = TenantTableBinding::resolve((tenant, frozen.seal_key.table.clone()))
                .expect("tenant binding");
            let member = stager
                .encode_runs(StageMemberRequest {
                    frozen: &frozen,
                    binding: &binding,
                    layout,
                    origin: StagedMemberOrigin {
                        node_id,
                        writer_epoch,
                        shard: u16::from(shard),
                        generation: u64::from(shard),
                        wal: StagedLsnRange {
                            min: u64::from(shard) * 100,
                            max: u64::from(shard) * 100 + 99,
                        },
                    },
                    footer_reservation: crate::scribe::memory::EncodedFooterReservation::for_test(),
                })
                .expect("member stages");
            assert_eq!(
                member.member(),
                StagedMemberId::new(u16::from(shard), u64::from(shard))
            );
            key = Some(member.key().clone());
            let ready = stager
                .publish_ready(member, ready_at)
                .await
                .expect("member publishes ready");
            assembler
                .register_ready(key.as_ref().expect("assembly key"), ready)
                .expect("member joins the ready index");
        }
        assembler
            .claim_residue(&key.expect("assembly key"), ClaimCause::PartitionClosed)
            .expect("residue claim")
            .expect("both members are ready")
    }

    /// Two members of one assembly key become one claim and one merged object:
    /// the rows of both interleave in layout order, the object carries exactly
    /// the rows the claim promised, and a smaller object target rolls the same
    /// rows into several objects without losing or reordering any of them.
    #[tokio::test]
    async fn a_claim_merges_its_members_into_rolling_objects() {
        let root = tempfile::tempdir().expect("assembly root");
        let stage_root = root.path().join("stage");
        let scratch = root.path().join("objects");
        for path in [&stage_root, &scratch] {
            std::fs::create_dir_all(path).expect("fixture directory");
        }
        let stage = Arc::new(ScribeHotStage::new(stage_root.clone()));
        let stager =
            ScribeMemberStager::new(Arc::clone(&stage), staging_volume(root.path(), &stage_root));
        let tenant = DataTenantId::new_v7();
        let schema = assembly_schema();
        let layout = assembly_layout(schema.as_ref());
        let ready_at = chrono::DateTime::from_timestamp(1_800_000_000, 0).expect("fixture instant");

        let claim = stage_two_members(&stager, tenant, &layout, ready_at).await;
        assert_eq!(claim.members().len(), 2);
        let claims = ClaimAssembler::new(Arc::clone(&stage));
        let runs = claims.gather(&claim).await.expect("claim runs");
        assert_eq!(runs.runs().len(), 2, "each member contributed one run");

        let assembled = claims
            .encode(AssembleClaimRequest {
                runs: &runs,
                schema: Arc::clone(&schema),
                layout: &layout,
                scratch_dir: &scratch,
                object_base: "claims/one",
                target_object_bytes: 512 * 1024 * 1024,
                footer_reservation: crate::scribe::memory::EncodedFooterReservation::for_test(),
            })
            .expect("claim assembles");
        assert_eq!(assembled.rows, 2_048);
        assert_eq!(
            assembled.artifacts.iter().count(),
            1,
            "a claim under the target closes as one object"
        );
        let merged = read_event_times(
            &assembled
                .artifacts
                .iter()
                .map(|artifact| artifact.scratch_path.clone())
                .collect::<Vec<_>>(),
        );
        assert_eq!(merged.len(), 2_048);
        assert!(
            merged.windows(2).all(|pair| pair[0] <= pair[1]),
            "the merged object is globally ordered by the layout sort key"
        );

        let rolled_scratch = root.path().join("rolled");
        std::fs::create_dir_all(&rolled_scratch).expect("rolled scratch");
        let rolled = claims
            .encode(AssembleClaimRequest {
                runs: &runs,
                schema,
                layout: &layout,
                scratch_dir: &rolled_scratch,
                object_base: "claims/rolled",
                target_object_bytes: 4 * 1024,
                footer_reservation: crate::scribe::memory::EncodedFooterReservation::for_test(),
            })
            .expect("claim assembles under a small target");
        assert!(
            rolled.artifacts.iter().count() > 1,
            "a small target rolls the same claim into several objects"
        );
        assert_eq!(rolled.rows, assembled.rows);
        assert_eq!(
            read_event_times(
                &rolled
                    .artifacts
                    .iter()
                    .map(|artifact| artifact.scratch_path.clone())
                    .collect::<Vec<_>>(),
            ),
            merged,
            "rolling changes where objects end, never the order of the rows"
        );
    }

    /// Reads `wyrd_event_time` from sealed objects in publication order.
    fn read_event_times(paths: &[PathBuf]) -> Vec<i64> {
        let mut times = Vec::new();
        for path in paths {
            let file = std::fs::File::open(path).expect("sealed object");
            let reader =
                parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder::try_new(file)
                    .expect("sealed object reader")
                    .build()
                    .expect("sealed object read");
            for batch in reader {
                let batch = batch.expect("sealed object batch");
                let column = batch
                    .column_by_name("wyrd_event_time")
                    .expect("event time column")
                    .as_any()
                    .downcast_ref::<TimestampMicrosecondArray>()
                    .expect("event time values")
                    .clone();
                times.extend((0..column.len()).map(|row| column.value(row)));
            }
        }
        times
    }
}
