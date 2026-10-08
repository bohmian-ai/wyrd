//! Parquet writer for frozen memtable snapshots.
//!
//! `encode_batch` encodes a `FrozenMemtable` snapshot to Parquet. Every file
//! is sorted by the table's registered physical layout, records the
//! authenticated tenant in its footer rather than in any column, and takes its
//! partition day from the seal-key (never from row min/max).

use std::io::{BufReader, BufWriter};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use arrow::compute::SortColumn;
use arrow::compute::concat_batches;
use arrow::compute::lexsort_to_indices;
use arrow::compute::take;
use arrow::datatypes::Schema;
use arrow::record_batch::RecordBatch;
use parquet::arrow::ArrowWriter;
use parquet::file::metadata::RowGroupMetaData;
use sha2::{Digest, Sha256};
use wyrd_spec::DataTenantId;

use crate::catalog::TenantTableBinding;
use crate::catalog::layout::PhysicalLayout;
use crate::catalog::layout::TimePartition;
use crate::contracts::ScribeError;
use crate::parquet::footer::BifrostFooterIdentity;
use crate::parquet::writer_properties::bifrost_writer_properties_with_metadata;
use crate::resources::ScribeClaimScratch;
use crate::scribe::geometry::DEFAULT_STAGING_TARGET_FILE_SIZE_BYTES;
use crate::scribe::memtable::FrozenMemtable;
use crate::scribe::wal::ScribeAppendMeta;

/// Target rows for one whole-batch file candidate.
const FILE_CANDIDATE_TARGET_ROWS: usize = 100 * 1024;

/// One serial, seal-key-local candidate expressed as a stored-batch range.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct FileCandidate {
    /// First stored batch included in the candidate.
    start: usize,
    /// Exclusive stored-batch end.
    end: usize,
    /// Exact rows in the complete stored batches.
    rows: usize,
}

/// Closes candidates before an exceeding whole batch without splitting it.
pub(crate) fn file_candidates(batches: &[RecordBatch]) -> Vec<FileCandidate> {
    let mut candidates = Vec::new();
    let mut start = 0;
    let mut rows = 0_usize;
    for (index, batch) in batches.iter().enumerate() {
        if rows != 0 && rows.saturating_add(batch.num_rows()) > FILE_CANDIDATE_TARGET_ROWS {
            candidates.push(FileCandidate {
                start,
                end: index,
                rows,
            });
            start = index;
            rows = 0;
        }
        rows = rows.saturating_add(batch.num_rows());
    }
    if start < batches.len() {
        candidates.push(FileCandidate {
            start,
            end: batches.len(),
            rows,
        });
    }
    candidates
}

/// Result of encoding a frozen memtable to Parquet.
#[derive(Debug)]
pub struct ParquetEncoded {
    /// Nonempty ordered scratch-backed artifacts ready for chunked upload.
    pub artifacts: BoundedParquetArtifactSet,
    /// Row group statistics.
    pub row_group_stats: Vec<RowGroupStats>,
    /// Partition day (from seal-key, not row min/max).
    pub partition: TimePartition,
    /// `ScribeAppendMeta` list threaded forward for 's `file_list` INSERT.
    pub append_metas: Vec<ScribeAppendMeta>,
}

/// Move-owned nonempty artifact set and its generation scratch authority.
#[derive(Debug)]
pub struct BoundedParquetArtifactSet {
    /// Contiguous Scribe Parquet artifacts in publication order.
    artifacts: Vec<BoundedParquetArtifact>,
    /// Exact claim directory the artifacts were written into.
    scratch: Option<ScribeClaimScratch>,
}

impl BoundedParquetArtifactSet {
    /// Creates an encoded set before the caller transfers its scratch owner.
    ///
    /// # Errors
    /// Returns an internal error when the encoder produced no artifacts or
    /// noncontiguous ordinals.
    pub(crate) fn encoded(artifacts: Vec<BoundedParquetArtifact>) -> Result<Self, ScribeError> {
        let first_ordinal = artifacts.first().map_or(0, |artifact| artifact.ordinal);
        if artifacts.is_empty()
            || artifacts.iter().enumerate().any(|(offset, artifact)| {
                usize::from(artifact.ordinal) != usize::from(first_ordinal) + offset
            })
        {
            return Err(ScribeError::Internal {
                detail: "Scribe Parquet artifact set must be nonempty and contiguous".to_owned(),
            });
        }
        Ok(Self {
            artifacts,
            scratch: None,
        })
    }

    /// Transfers the exact generation scratch capability into the sealed set.
    ///
    /// # Errors
    /// Returns an internal error if scratch authority was already attached.
    pub(crate) fn attach_scratch(
        &mut self,
        scratch: ScribeClaimScratch,
    ) -> Result<(), ScribeError> {
        if self.scratch.replace(scratch).is_some() {
            return Err(ScribeError::Internal {
                detail: "Scribe Parquet artifact set already owns scratch".to_owned(),
            });
        }
        Ok(())
    }

    /// Returns the ordered sealed artifacts without exposing scratch ownership.
    #[must_use]
    pub fn as_slice(&self) -> &[BoundedParquetArtifact] {
        &self.artifacts
    }

    /// Cleans the exact generation prefix after a positively known terminal.
    ///
    /// # Errors
    /// Returns an internal error when the bounded scratch cleanup protocol
    /// exhausts its retries.
    pub(crate) async fn cleanup(mut self) -> Result<(), ScribeError> {
        let Some(scratch) = self.scratch.take() else {
            return Ok(());
        };
        tokio::task::spawn_blocking(move || scratch.cleanup())
            .await
            .map_err(|error| ScribeError::Internal {
                detail: format!("Scribe Parquet scratch cleanup task failed: {error}"),
            })?
            .map_err(|error| ScribeError::Internal {
                detail: format!("Scribe Parquet scratch cleanup failed: {error}"),
            })
    }

    /// Retains scratch after an unresolved commit outcome.
    ///
    /// Startup reconciliation owns removal of the exact namespace; dropping
    /// the owner without cleanup leaves evidence that may already be
    /// catalog-visible in place.
    pub(crate) fn retain_for_reconciliation(mut self) {
        self.scratch.take();
    }
}

impl std::ops::Deref for BoundedParquetArtifactSet {
    type Target = [BoundedParquetArtifact];

    /// Exposes ordered read-only artifact facts to upload and publication code.
    fn deref(&self) -> &Self::Target {
        self.as_slice()
    }
}

impl<'a> IntoIterator for &'a BoundedParquetArtifactSet {
    type Item = &'a BoundedParquetArtifact;
    type IntoIter = std::slice::Iter<'a, BoundedParquetArtifact>;

    /// Iterates sealed artifacts in their durable ordinal order.
    fn into_iter(self) -> Self::IntoIter {
        self.artifacts.iter()
    }
}

/// One sealed Scribe Parquet object retained on generation-owned scratch.
#[derive(Debug, Clone)]
pub struct BoundedParquetArtifact {
    /// Contiguous zero-based position within the generation.
    pub ordinal: u16,
    /// Exact scratch file read only through bounded chunks.
    pub scratch_path: PathBuf,
    /// Deterministic committed object identity stamped into the footer.
    pub object_identity: String,
    /// Exact sealed file size.
    pub file_size: u64,
    /// Lowercase SHA-256 checksum over the sealed bytes.
    pub checksum: String,
    /// Rows represented by this physical artifact.
    pub row_count: usize,
    /// Footer-derived row-group statistics.
    pub row_group_stats: Vec<RowGroupStats>,
    /// Iceberg-ready metrics derived once from this artifact's closed footer.
    ///
    /// Publication turns this into the object's
    /// [`ScribePublishedHotFileV1`](crate::scribe::promotion::ScribePublishedHotFileV1)
    /// by adding the facts only the fenced transaction knows. Deriving it here,
    /// from the footer the writer just closed and re-read, is what makes the
    /// persisted record the writer's own evidence rather than a later
    /// reconstruction.
    pub data_file_metrics: crate::scribe::promotion::ScribeDataFileV1,
    /// Lowercase hex Wyrd schema fingerprint the footer was sealed with.
    pub schema_fingerprint: String,
}

/// Statistics for a single row group.
#[derive(Debug, Clone)]
pub struct RowGroupStats {
    /// Number of rows in this row group.
    pub row_count: usize,
    /// Minimum `wyrd_event_time` timestamp (microseconds since epoch).
    pub min_event_time: Option<i64>,
    /// Maximum `wyrd_event_time` timestamp (microseconds since epoch).
    pub max_event_time: Option<i64>,
}

/// Encode a frozen memtable snapshot to ordered scratch-backed Parquet artifacts.
///
/// Returns encoded bytes, row-group stats, `partition` (from seal-key), and the paired
/// `ScribeAppendMeta` list unmodified (threaded forward for 's seal
/// transaction).
///
/// # Errors
/// Returns [`ScribeError::Internal`] when the binding, sort keys, or Parquet
/// encoding is invalid.
pub(crate) fn encode_batch(
    frozen: &FrozenMemtable,
    binding: &TenantTableBinding,
    seal_tenant: DataTenantId,
    scratch_dir: &Path,
    object_base: &str,
    layout: &PhysicalLayout,
    memory: crate::resources::ScribeResources,
) -> Result<ParquetEncoded, ScribeError> {
    ParquetBatchEncoder {
        frozen,
        binding,
        seal_tenant,
        candidates: file_candidates(&frozen.batches),
        first_ordinal: 0,
        scratch_dir,
        object_base,
        layout,
        memory,
        target_object_bytes: DEFAULT_STAGING_TARGET_FILE_SIZE_BYTES,
    }
    .encode()
}

/// Everything a rolling artifact writer needs that is not the rows themselves.
///
/// The plan exists so one writer serves both producers of ordered rows: the
/// frozen-generation encoder, whose rows come from Arrow, and claim assembly,
/// whose rows come from a bounded merge over staged runs. Neither owns the
/// rolling rules, so neither can drift from them.
#[derive(Debug, Clone, Copy)]
pub struct ArtifactPlan<'a> {
    /// Directory receiving the sealed artifacts.
    pub scratch_dir: &'a Path,
    /// Deterministic object identity prefix for artifact ordinals.
    pub object_base: &'a str,
    /// Registered physical write recipe governing Bloom columns.
    pub layout: &'a PhysicalLayout,
    /// First artifact ordinal this operation may assign.
    pub first_ordinal: usize,
    /// Approximate encoded size at which one artifact closes and the next opens.
    pub target_object_bytes: u64,
    /// Authenticated tenant recorded in every sealed artifact's footer.
    pub tenant: DataTenantId,
}

/// Encodes one claim's already ordered rows into rolling hot objects.
///
/// This is the assembly half of the writer: the rows arrive globally ordered
/// from the claim's bounded merge, so there is nothing to sort and nothing to
/// concatenate. Artifacts roll strictly between completed row groups once the
/// running encoded size reaches the plan's target, and the claim's remainder is
/// a valid smaller final object rather than a defect.
///
/// Unlike a frozen generation, a claim has no candidate boundaries: sealing
/// happens only at the target and at the end, which is what lets several
/// members share one object.
///
/// # Errors
///
/// Returns [`ScribeError::Internal`] when the merge fails, a batch is empty, an
/// artifact cannot be opened, written, sealed, inspected, or checksummed, or
/// the claim produced no rows at all.
pub fn encode_ordered_claim(
    plan: ArtifactPlan<'_>,
    memory: &crate::resources::ScribeResources,
    ordered: impl Iterator<Item = Result<RecordBatch, ScribeError>>,
) -> Result<(BoundedParquetArtifactSet, Vec<RowGroupStats>), ScribeError> {
    let mut roller = RollingArtifactWriter::new(plan);
    for batch in ordered {
        roller.append_charged_batch(memory, &batch?)?;
    }
    let (artifacts, row_group_stats) = roller.finish()?;
    Ok((
        BoundedParquetArtifactSet::encoded(artifacts)?,
        row_group_stats,
    ))
}

/// Owns one bounded Parquet encoding workflow and the capability it charges.
struct ParquetBatchEncoder<'a> {
    /// Immutable generation being encoded.
    frozen: &'a FrozenMemtable,
    /// Tenant-qualified physical binding validated before materialization.
    binding: &'a TenantTableBinding,
    /// Authenticated tenant recorded in every sealed artifact's footer.
    seal_tenant: DataTenantId,
    /// Ordered whole-batch candidates owned by this operation.
    candidates: Vec<FileCandidate>,
    /// First generation-global artifact ordinal assigned to this candidate.
    first_ordinal: usize,
    /// Generation-owned scratch directory for encoded artifacts.
    scratch_dir: &'a Path,
    /// Deterministic object identity prefix for artifact ordinals.
    object_base: &'a str,
    /// Registered physical write recipe governing sort order and Bloom columns.
    layout: &'a PhysicalLayout,
    /// Scribe capability charged for each sorted candidate while it is held.
    memory: crate::resources::ScribeResources,
    /// Approximate encoded size at which one artifact closes and the next opens.
    target_object_bytes: u64,
}

impl<'a> ParquetBatchEncoder<'a> {
    /// Returns the rolling rules and identity this encoding writes under.
    fn plan(&self) -> ArtifactPlan<'a> {
        ArtifactPlan {
            scratch_dir: self.scratch_dir,
            object_base: self.object_base,
            layout: self.layout,
            first_ordinal: self.first_ordinal,
            target_object_bytes: self.target_object_bytes,
            tenant: self.seal_tenant,
        }
    }

    /// Executes validation, bounded materialization, and exact artifact encoding.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::Internal`] for an invalid binding, Arrow
    /// materialization failure, or an invalid encoded artifact.
    fn encode(self) -> Result<ParquetEncoded, ScribeError> {
        let mut roller = RollingArtifactWriter::new(self.plan());
        for candidate in &self.candidates {
            let sorted_batch = self.prepare_sorted_candidate(*candidate)?;
            roller.append_charged_batch(&self.memory, &sorted_batch)?;
            roller.seal_open_artifact()?;
        }
        let (artifacts, row_group_stats) = roller.finish()?;
        Ok(ParquetEncoded {
            artifacts: BoundedParquetArtifactSet::encoded(artifacts)?,
            row_group_stats,
            partition: self.frozen.seal_key.partition,
            append_metas: self.frozen.metas.clone(),
        })
    }

    /// Validates physical identity and builds the sorted candidate batch.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::Internal`] when identity, concatenation, or
    /// sort-key validation fails.
    fn prepare_sorted_candidate(
        &self,
        candidate: FileCandidate,
    ) -> Result<RecordBatch, ScribeError> {
        if self.binding.tenant != self.seal_tenant
            || self.binding.tenant != self.frozen.seal_key.tenant
        {
            return Err(ScribeError::Internal {
                detail: format!(
                    "tenant-table binding mismatch before encoding: binding tenant `{}`; seal tenant `{}`; frozen tenant `{}`",
                    self.binding.tenant, self.seal_tenant, self.frozen.seal_key.tenant
                ),
            });
        }
        if self.binding.table_ref != self.frozen.seal_key.table {
            return Err(ScribeError::Internal {
                detail: format!(
                    "tenant-table binding table mismatch before encoding: binding `{}`; seal table `{}`",
                    self.binding.table_ref, self.frozen.seal_key.table
                ),
            });
        }
        let encoder_batch = concat_batches(
            &self.frozen.schema,
            &self.frozen.batches[candidate.start..candidate.end],
        )
        .map_err(|error| ScribeError::Internal {
            detail: format!("failed to materialize frozen memtable for encoding: {error}"),
        })?;
        debug_assert_eq!(encoder_batch.num_rows(), candidate.rows);
        sort_batch(&encoder_batch, self.layout)
    }
}

/// One artifact that is open for appended ordered batches.
struct OpenArtifact {
    /// Contiguous generation-global ordinal already assigned to this artifact.
    ordinal: u16,
    /// Deterministic committed object identity stamped into the footer.
    object_identity: String,
    /// Scratch file receiving the appended row groups.
    scratch_path: PathBuf,
    /// Arrow schema every appended row group must carry.
    schema: Arc<Schema>,
    /// Open Parquet encoder owning the buffered sink.
    writer: ArrowWriter<BufWriter<std::fs::File>>,
    /// Rows appended so far across every batch.
    rows: usize,
}

/// Appends ordered batches to one artifact and rolls at the object target.
///
/// Row groups are shaped by parquet-rs under the shared writer recipe's soft
/// encoded row-group target, so any accepted batch is writable: a row larger
/// than the target lands in a group of its own. Rolling is the only size
/// decision this writer makes, from flushed encoded bytes; it is taken only
/// after a batch has been handed to the encoder, it is approximate, and it
/// never deletes, retries, or refuses rows that were already written.
struct RollingArtifactWriter<'a> {
    /// Scratch, identity, layout, and the object target this writer obeys.
    plan: ArtifactPlan<'a>,
    /// Next generation-global ordinal to assign.
    next_ordinal: usize,
    /// Artifact currently accepting batches, if one is open.
    open: Option<OpenArtifact>,
    /// Sealed artifacts in contiguous publication order.
    artifacts: Vec<BoundedParquetArtifact>,
    /// Footer-derived statistics for every sealed row group in write order.
    row_group_stats: Vec<RowGroupStats>,
}

impl<'a> RollingArtifactWriter<'a> {
    /// Starts a rolling writer at the plan's first generation-global ordinal.
    fn new(plan: ArtifactPlan<'a>) -> Self {
        Self {
            plan,
            next_ordinal: plan.first_ordinal,
            open: None,
            artifacts: Vec::new(),
            row_group_stats: Vec::new(),
        }
    }

    /// Charges one materialized batch's retained bytes while the encoder writes it.
    ///
    /// The batch is a Wyrd-owned copy (sorted candidate or merge output), so it
    /// holds a Scribe lease for exactly its distinct allocations and no more;
    /// nothing is admitted for encoder or footer bytes it may later need.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::IngestBusy`] when the shared root cannot hold the
    /// batch, which fails this stage attempt for retry, or any
    /// [`Self::append_ordered_batch`] error.
    fn append_charged_batch(
        &mut self,
        memory: &crate::resources::ScribeResources,
        batch: &RecordBatch,
    ) -> Result<(), ScribeError> {
        let _held = memory.try_reserve_maintenance(
            crate::scribe::memory::MemoryCategory::Persistence,
            crate::scribe::memory::retained_arrow_bytes(batch),
        )?;
        self.append_ordered_batch(batch)
    }

    /// Writes one already ordered batch and rolls if the object target is met.
    ///
    /// The encoder buffers rows toward its row-group target and flushes
    /// completed groups itself, so small inputs from many staged runs share
    /// groups rather than each becoming one. Once the artifact's flushed bytes
    /// plus the encoder's estimate for its buffered group reach the plan's
    /// soft object target it is sealed; the next batch opens another. The
    /// estimate is a pure function of the rows written, so a retry of the same
    /// input rolls at the same batches.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::Internal`] when the batch is empty, the artifact
    /// cannot be opened, the batch cannot be written, or sealing a due
    /// artifact fails.
    fn append_ordered_batch(&mut self, ordered_batch: &RecordBatch) -> Result<(), ScribeError> {
        if ordered_batch.num_rows() == 0 {
            return Err(ScribeError::Internal {
                detail: "Scribe Parquet writer cannot publish an empty batch".to_owned(),
            });
        }
        if self.open.is_none() {
            self.open = Some(self.open_artifact(ordered_batch)?);
        }
        let Some(open) = self.open.as_mut() else {
            return Err(ScribeError::Internal {
                detail: "Scribe Parquet rolling writer lost its open artifact".to_owned(),
            });
        };
        open.writer
            .write(ordered_batch)
            .map_err(|error| ScribeError::Internal {
                detail: format!("write Scribe Parquet batch: {error}"),
            })?;
        open.rows = open.rows.saturating_add(ordered_batch.num_rows());
        let written = open
            .writer
            .bytes_written()
            .saturating_add(open.writer.in_progress_size());
        let written = u64::try_from(written).unwrap_or(u64::MAX);
        if written >= self.plan.target_object_bytes {
            self.seal_open_artifact()?;
        }
        Ok(())
    }

    /// Opens the next artifact for the batch that is about to be written.
    ///
    /// Bloom sizing is a per-row-group hint, so it is derived from the batch
    /// that opens the artifact.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::Internal`] when the ordinal exceeds `u16`, the
    /// scratch file cannot be created, or the Parquet encoder cannot be built.
    fn open_artifact(&mut self, group: &RecordBatch) -> Result<OpenArtifact, ScribeError> {
        let ordinal = u16::try_from(self.next_ordinal).map_err(|_| ScribeError::Internal {
            detail: "Scribe Parquet artifact ordinal exceeds u16".to_owned(),
        })?;
        self.next_ordinal = self.next_ordinal.saturating_add(1);
        let object_identity = format!("{}-{ordinal:05}.parquet", self.plan.object_base);
        let scratch_path = self
            .plan
            .scratch_dir
            .join(format!("artifact-{ordinal:05}.parquet"));
        let file = std::fs::File::create(&scratch_path).map_err(|error| ScribeError::Internal {
            detail: format!("create Scribe Parquet scratch artifact: {error}"),
        })?;
        let schema = group.schema();
        let writer = ArrowWriter::try_new(
            BufWriter::new(file),
            Arc::clone(&schema),
            Some(bifrost_writer_properties_with_metadata(
                group.num_rows(),
                Vec::new(),
                self.plan.layout.bloom_columns(),
            )),
        )
        .map_err(|error| ScribeError::Internal {
            detail: format!("create Scribe Parquet Parquet encoder: {error}"),
        })?;
        Ok(OpenArtifact {
            ordinal,
            object_identity,
            scratch_path,
            schema,
            writer,
            rows: 0,
        })
    }

    /// Seals the open artifact, stamping its footer identity.
    ///
    /// Sealing with nothing open is the ordinary boundary case — a candidate
    /// whose last batch already reached the target — and is not an error.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::Internal`] when the object identity is empty, the
    /// Parquet artifact cannot be sealed or measured, the sealed file is empty,
    /// or footer inspection or checksumming refuses it.
    fn seal_open_artifact(&mut self) -> Result<(), ScribeError> {
        let Some(open) = self.open.take() else {
            return Ok(());
        };
        let OpenArtifact {
            ordinal,
            object_identity,
            scratch_path,
            schema,
            mut writer,
            rows,
        } = open;
        for field in BifrostFooterIdentity::new(schema.as_ref(), &object_identity, self.plan.tenant)
            .map_err(|detail| ScribeError::Internal { detail })?
            .key_values()
        {
            writer.append_key_value_metadata(field);
        }
        writer.close().map_err(|error| ScribeError::Internal {
            detail: format!("seal Scribe Parquet Parquet artifact: {error}"),
        })?;
        let file_size = std::fs::metadata(&scratch_path)
            .map_err(|error| ScribeError::Internal {
                detail: format!("stat Scribe Parquet scratch artifact: {error}"),
            })?
            .len();
        if file_size == 0 {
            return Err(ScribeError::Internal {
                detail: "Scribe Parquet sealed artifact is empty".to_owned(),
            });
        }
        let evidence = inspect_sealed_artifact(
            &scratch_path,
            schema.as_ref(),
            &object_identity,
            self.plan.tenant,
            file_size,
        )?;
        let SealedArtifactEvidence {
            row_group_stats,
            data_file_metrics,
            schema_fingerprint,
        } = evidence;
        self.row_group_stats.extend(row_group_stats.iter().cloned());
        self.artifacts.push(BoundedParquetArtifact {
            ordinal,
            scratch_path: scratch_path.clone(),
            object_identity,
            file_size,
            checksum: checksum_file(&scratch_path)?,
            row_count: rows,
            row_group_stats,
            data_file_metrics,
            schema_fingerprint,
        });
        Ok(())
    }

    /// Seals any residue and returns the ordered artifacts and their statistics.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::Internal`] when the residual artifact cannot be
    /// sealed or validated.
    fn finish(mut self) -> Result<(Vec<BoundedParquetArtifact>, Vec<RowGroupStats>), ScribeError> {
        self.seal_open_artifact()?;
        Ok((self.artifacts, self.row_group_stats))
    }
}

/// Sort a `RecordBatch` by the table's registered physical sort order.
///
/// The canonical order is the table's declared keys in declaration order, so
/// the rows Scribe writes match the sort order stamped on the destination
/// Iceberg table.
///
/// # Errors
/// Returns [`ScribeError::Internal`] when a sort column named by the resolved
/// layout is absent from the batch schema, or when the Arrow sort fails.
fn sort_batch(batch: &RecordBatch, layout: &PhysicalLayout) -> Result<RecordBatch, ScribeError> {
    let schema = batch.schema();
    let sort_columns = layout
        .sort_keys()
        .iter()
        .map(|key| {
            let index = schema
                .index_of(key.column())
                .map_err(|_| ScribeError::Internal {
                    detail: format!("sort column {} not found in sealed schema", key.column()),
                })?;
            Ok(SortColumn {
                values: batch.column(index).clone(),
                options: Some(arrow::compute::SortOptions {
                    descending: key.is_descending(),
                    nulls_first: key.nulls_first(),
                }),
            })
        })
        .collect::<Result<Vec<_>, ScribeError>>()?;

    // Compute sort indices
    let indices = lexsort_to_indices(&sort_columns, None).map_err(|e| ScribeError::Internal {
        detail: format!("failed to compute sort indices: {e}"),
    })?;

    // Reorder all columns using the indices
    let sorted_columns: Result<Vec<_>, _> = batch
        .columns()
        .iter()
        .map(|col| {
            take(col.as_ref(), &indices, None).map_err(|e| ScribeError::Internal {
                detail: format!("failed to reorder column during sort: {e}"),
            })
        })
        .collect();

    RecordBatch::try_new(batch.schema(), sorted_columns?).map_err(|e| ScribeError::Internal {
        detail: format!("failed to rebuild sorted RecordBatch: {e}"),
    })
}

/// What re-reading one sealed artifact's footer proved about it.
///
/// The writer re-opens every artifact it seals, so this is the one place that
/// holds the parsed footer. Both the live-tail statistics and the promotion
/// metrics are taken from that single read rather than from two passes that
/// could disagree.
struct SealedArtifactEvidence {
    /// Per-row-group event-time statistics in write order.
    row_group_stats: Vec<RowGroupStats>,
    /// Iceberg-ready metrics for the whole object.
    data_file_metrics: crate::scribe::promotion::ScribeDataFileV1,
    /// Lowercase hex Wyrd schema fingerprint the footer carries.
    schema_fingerprint: String,
}

/// Validates one sealed artifact and returns everything its footer proves.
///
/// The footer must parse, carry this artifact's schema fingerprint and object
/// identity, and give every row group event-time statistics. Row-group and
/// footer sizes are not checked: they follow from soft writer targets, and a
/// large accepted row is valid data rather than a defect.
///
/// # Errors
/// Returns an internal persistence error for unreadable Parquet metadata, a
/// footer identity that contradicts the expected schema or object, a row
/// group with missing statistics, or a footer whose Iceberg projection is not
/// representable.
fn inspect_sealed_artifact(
    path: &Path,
    expected_schema: &Schema,
    expected_object_identity: &str,
    tenant: DataTenantId,
    file_size: u64,
) -> Result<SealedArtifactEvidence, ScribeError> {
    use parquet::file::reader::{FileReader, SerializedFileReader};

    let file = std::fs::File::open(path).map_err(|error| ScribeError::Internal {
        detail: format!("open sealed Scribe Parquet artifact: {error}"),
    })?;
    let reader = SerializedFileReader::new(file).map_err(|e| ScribeError::Internal {
        detail: format!("failed to parse Parquet metadata: {e}"),
    })?;

    let metadata = reader.metadata();
    BifrostFooterIdentity::new(expected_schema, expected_object_identity, tenant)
        .and_then(|identity| identity.verify(metadata.file_metadata()))
        .map_err(|detail| ScribeError::Internal { detail })?;
    let mut row_group_stats = Vec::new();

    for rg in metadata.row_groups() {
        row_group_stats.push(extract_row_group_time_range(rg)?);
    }

    let data_file_metrics = derive_data_file_metrics(
        expected_schema,
        Arc::new(reader.metadata().clone()),
        expected_object_identity,
        file_size,
    )?;
    Ok(SealedArtifactEvidence {
        row_group_stats,
        data_file_metrics,
        schema_fingerprint: hex::encode(
            crate::schema::SchemaFingerprint::from_arrow_schema_exact(expected_schema).0,
        ),
    })
}

/// Projects one sealed artifact's closed footer into Iceberg-ready metrics.
///
/// The projection is the managed Iceberg writer's own footer-to-`DataFile`
/// conversion, so the per-column sizes, counts, and bounds Scribe persists are
/// exactly the ones a catalog promoter would compute from the same object.
/// Field ids are the ones Scribe stamped from the registered Iceberg table at
/// ingress, which the conversion adopts unchanged, so the ids in this
/// projection are the table's own rather than a private numbering. A schema
/// without ids — only the catalog-less engine seam writes one — falls back to
/// Iceberg's automatic assignment.
///
/// # Errors
///
/// Returns [`ScribeError::Internal`] when the Arrow schema has no Iceberg
/// projection, the footer cannot be converted, or the resulting file carries a
/// bound with no binary single-value serialization.
fn derive_data_file_metrics(
    expected_schema: &Schema,
    metadata: Arc<parquet::file::metadata::ParquetMetaData>,
    object_identity: &str,
    file_size: u64,
) -> Result<crate::scribe::promotion::ScribeDataFileV1, ScribeError> {
    let iceberg_schema = crate::tables::iceberg_schema_for(expected_schema).map_err(|error| {
        ScribeError::Internal {
            detail: format!("sealed artifact schema has no Iceberg projection: {error}"),
        }
    })?;
    let written = usize::try_from(file_size).map_err(|_| ScribeError::Internal {
        detail: "sealed artifact size exceeds address space".to_owned(),
    })?;
    let data_file = iceberg::writer::file_writer::ParquetWriter::parquet_to_data_file_builder(
        Arc::new(iceberg_schema),
        metadata,
        written,
        object_identity.to_owned(),
        std::collections::HashMap::new(),
    )
    .map_err(|error| ScribeError::Internal {
        detail: format!("sealed artifact footer has no Iceberg projection: {error}"),
    })?
    .build()
    .map_err(|error| ScribeError::Internal {
        detail: format!("sealed artifact footer does not assemble a data file: {error}"),
    })?;
    crate::scribe::promotion::ScribeDataFileV1::from_data_file(&data_file)
}

/// Computes the required object checksum without retaining the file in memory.
///
/// # Errors
/// Returns an internal persistence error when the sealed artifact cannot be read.
fn checksum_file(path: &Path) -> Result<String, ScribeError> {
    let file = std::fs::File::open(path).map_err(|error| ScribeError::Internal {
        detail: format!("open Scribe Parquet artifact for checksum: {error}"),
    })?;
    let mut hasher = Sha256::new();
    std::io::copy(&mut BufReader::new(file), &mut hasher).map_err(|error| {
        ScribeError::Internal {
            detail: format!("read Scribe Parquet artifact for checksum: {error}"),
        }
    })?;
    Ok(hex::encode(hasher.finalize()))
}

/// Extract min/max event time from a single row group.
///
/// # Errors
/// Returns [`ScribeError::Internal`] if column stats are missing.
fn extract_row_group_time_range(rg: &RowGroupMetaData) -> Result<RowGroupStats, ScribeError> {
    // Parquet row-group `num_rows()` is i64; RowGroupStats.row_count is usize.
    // A negative row count would be a Parquet-writer invariant violation.
    let row_count = usize::try_from(rg.num_rows()).map_err(|_| ScribeError::Internal {
        detail: format!("row group has invalid num_rows: {}", rg.num_rows()),
    })?;

    // Find wyrd_event_time column index (column order matches Arrow schema order)
    let time_col = rg
        .columns()
        .iter()
        .find(|col| col.column_descr().name() == "wyrd_event_time")
        .ok_or_else(|| ScribeError::Internal {
            detail: "wyrd_event_time column not found in row group".to_string(),
        })?;

    let stats = time_col.statistics().ok_or_else(|| ScribeError::Internal {
        detail: "wyrd_event_time column has no statistics".to_string(),
    })?;

    // Extract min/max as i64 (TimestampMicrosecondType)
    let min = if let Some(min_val) = stats.min_bytes_opt() {
        min_val.get(0..8).and_then(|b| {
            let arr: [u8; 8] = b.try_into().ok()?;
            Some(i64::from_le_bytes(arr))
        })
    } else {
        None
    };

    let max = if let Some(max_val) = stats.max_bytes_opt() {
        max_val.get(0..8).and_then(|b| {
            let arr: [u8; 8] = b.try_into().ok()?;
            Some(i64::from_le_bytes(arr))
        })
    } else {
        None
    };

    Ok(RowGroupStats {
        row_count,
        min_event_time: min,
        max_event_time: max,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use arrow::array::{NullArray, RecordBatch, StringArray, TimestampMicrosecondArray};
    use arrow::datatypes::{DataType, Field, Schema, TimeUnit};

    use parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder;
    use parquet::file::reader::{FileReader, SerializedFileReader};
    use std::sync::Arc;
    use wyrd_spec::ids::DataTenantId;

    use crate::catalog::TableRef;
    use crate::namespaces::BifrostNamespace;
    use crate::scribe::seal_key::{SealKey, TimePartition};

    /// Builds one metadata-light stored batch with the requested row count.
    fn candidate_batch(rows: usize) -> RecordBatch {
        RecordBatch::try_new(
            Arc::new(Schema::new(vec![Field::new("value", DataType::Null, true)])),
            vec![Arc::new(NullArray::new(rows))],
        )
        .expect("candidate fixture")
    }

    /// Asserts one candidate list exactly partitions its batches within bounds.
    ///
    /// Each candidate must be non-empty, its row count must equal the sum of the
    /// batches it spans — so grouping can neither lose nor double-count rows —
    /// and it must stay under the target row ceiling unless it is a single
    /// oversized batch, which has no smaller legal grouping.
    ///
    /// # Panics
    ///
    /// Panics if any candidate is empty, miscounts its rows, or exceeds the
    /// target without being a lone oversized batch.
    fn assert_candidates_cover_batches_within_bounds(
        batches: &[RecordBatch],
        candidates: &[FileCandidate],
    ) {
        for candidate in candidates {
            assert!(candidate.start < candidate.end);
            assert_eq!(
                candidate.rows,
                batches[candidate.start..candidate.end]
                    .iter()
                    .map(RecordBatch::num_rows)
                    .sum::<usize>()
            );
            assert!(
                candidate.rows <= FILE_CANDIDATE_TARGET_ROWS
                    || candidate.end - candidate.start == 1
            );
        }
    }

    /// Whole stored batches close before exceeding the row target independently
    /// for each seal key, while an oversized first batch remains whole.
    #[test]
    fn whole_batch_file_candidates_preserve_writer_bounds() {
        let first = (
            SealKey::new(
                DataTenantId::new_v7(),
                TableRef::new(BifrostNamespace::Bifrost, "candidate-first"),
                crate::test_support::day_partition(2026, 7, 14),
            ),
            vec![
                candidate_batch(60 * 1024),
                candidate_batch(40 * 1024),
                candidate_batch(1),
                candidate_batch(120 * 1024),
                candidate_batch(2),
            ],
        );
        let second = (
            SealKey::new(
                DataTenantId::new_v7(),
                TableRef::new(BifrostNamespace::Bifrost, "candidate-second"),
                crate::test_support::day_partition(2026, 7, 15),
            ),
            vec![
                candidate_batch(100 * 1024 - 1),
                candidate_batch(1),
                candidate_batch(120 * 1024),
            ],
        );
        assert_ne!(first.0, second.0);

        let first_candidates = file_candidates(&first.1);
        assert_eq!(
            first_candidates,
            vec![
                FileCandidate {
                    start: 0,
                    end: 2,
                    rows: 100 * 1024,
                },
                FileCandidate {
                    start: 2,
                    end: 3,
                    rows: 1,
                },
                FileCandidate {
                    start: 3,
                    end: 4,
                    rows: 120 * 1024,
                },
                FileCandidate {
                    start: 4,
                    end: 5,
                    rows: 2,
                },
            ]
        );
        let second_candidates = file_candidates(&second.1);
        assert_eq!(
            second_candidates,
            vec![
                FileCandidate {
                    start: 0,
                    end: 2,
                    rows: 100 * 1024,
                },
                FileCandidate {
                    start: 2,
                    end: 3,
                    rows: 120 * 1024,
                },
            ]
        );
        assert_eq!(first_candidates[0].start, 0);
        assert_eq!(second_candidates[0].start, 0);
        assert_eq!(first_candidates[2].rows, 120 * 1024);
        assert_eq!(second_candidates[1].rows, 120 * 1024);
        assert_candidates_cover_batches_within_bounds(&first.1, &first_candidates);
        assert_candidates_cover_batches_within_bounds(&second.1, &second_candidates);
    }

    /// Builds the canonical hourly write recipe for a test schema.
    ///
    /// Tests exercise the same resolution path production uses: declare the
    /// built-in hourly layout, then let `PhysicalLayout::resolve` union the
    /// managed Bloom floor.
    ///
    /// The fixture schema carries none of the floor columns, so the Bloom
    /// intent is declared explicitly. `wyrd_event_time` is a legal declared
    /// Bloom column and is not constant within a file.
    ///
    /// # Panics
    /// Panics when the schema cannot carry the built-in declaration.
    fn test_layout(schema: &Schema) -> PhysicalLayout {
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

    fn build_test_frozen(
        seal_partition: TimePartition,
        seal_tenant: DataTenantId,
        services: Vec<&str>,
        timestamps: Vec<i64>,
    ) -> FrozenMemtable {
        let schema = Arc::new(Schema::new(vec![
            Field::new("service", DataType::Utf8, false),
            Field::new(
                "wyrd_event_time",
                DataType::Timestamp(TimeUnit::Microsecond, None),
                false,
            ),
        ]));

        let batch = RecordBatch::try_new(
            schema.clone(),
            vec![
                Arc::new(StringArray::from(services)),
                Arc::new(TimestampMicrosecondArray::from(timestamps)),
            ],
        )
        .unwrap();

        let seal_key = SealKey::new(
            seal_tenant,
            TableRef::new(BifrostNamespace::Bifrost, "events"),
            seal_partition,
        );

        FrozenMemtable {
            seal_id: 0,
            seal_key,
            shard_id: 0,
            schema: Arc::clone(&schema),
            batches: vec![batch],
            metas: vec![],
            opened_at: std::time::Instant::now(),
            closed_at: std::time::Instant::now(),
            arrow_bytes: 0,
        }
    }

    /// Encodes through a real scratch directory and retains it for assertions.
    fn encode_for_test(
        frozen: &FrozenMemtable,
        binding: &TenantTableBinding,
        tenant: DataTenantId,
    ) -> (tempfile::TempDir, ParquetEncoded) {
        let scratch = tempfile::tempdir().expect("test scratch");
        let layout = test_layout(frozen.schema.as_ref());
        let encoded = encode_batch(
            frozen,
            binding,
            tenant,
            scratch.path(),
            "s3://bucket/table/day=2026-07-14/scribe-test-0",
            &layout,
            crate::resources::ScribeResources::for_test(),
        )
        .expect("Scribe Parquet encode");
        (scratch, encoded)
    }

    /// Builds one whole stored batch retained as one candidate input boundary.
    fn whole_member_batch(first_timestamp: i64, rows: usize) -> RecordBatch {
        let row_count = i64::try_from(rows).expect("test row count fits i64");
        RecordBatch::try_new(
            Arc::new(Schema::new(vec![Field::new(
                "wyrd_event_time",
                DataType::Timestamp(TimeUnit::Microsecond, None),
                false,
            )])),
            vec![Arc::new(TimestampMicrosecondArray::from_iter_values(
                first_timestamp..first_timestamp + row_count,
            ))],
        )
        .expect("whole member batch")
    }

    /// Proves forced selective seal publishes multiple candidates as one
    /// contiguous deterministic artifact set.
    #[test]
    fn forced_seal_multi_candidate_member_has_one_deterministic_artifact_set() {
        let tenant = DataTenantId::new_v7();
        let day = crate::test_support::day_partition(2026, 7, 14);
        let schema = Arc::new(Schema::new(vec![Field::new(
            "wyrd_event_time",
            DataType::Timestamp(TimeUnit::Microsecond, None),
            false,
        )]));
        let frozen = FrozenMemtable {
            seal_id: 41,
            seal_key: SealKey::new(
                tenant,
                TableRef::new(BifrostNamespace::Bifrost, "forced_member"),
                day,
            ),
            shard_id: 3,
            schema,
            batches: vec![
                whole_member_batch(0, 60 * 1024),
                whole_member_batch(60 * 1024, 50 * 1024),
            ],
            metas: vec![],
            opened_at: std::time::Instant::now(),
            closed_at: std::time::Instant::now(),
            arrow_bytes: 0,
        };
        let binding = TenantTableBinding::resolve((tenant, frozen.seal_key.table.clone()))
            .expect("tenant binding");
        let (_scratch, encoded) = encode_for_test(&frozen, &binding, tenant);
        let facts = encoded
            .artifacts
            .iter()
            .map(|artifact| (artifact.ordinal, artifact.row_count))
            .collect::<Vec<_>>();
        assert_eq!(facts, vec![(0, 60 * 1024), (1, 50 * 1024)]);
    }

    /// Number of high-cardinality payload columns in the low-compressibility
    /// fixture, chosen so one batch encodes to many MiB.
    const INCOMPRESSIBLE_PAYLOAD_COLUMNS: usize = 48;

    /// Builds one deterministic low-compressibility frozen member.
    ///
    /// Every payload column holds distinct pseudo-random 32-bit values, so the
    /// dictionary the writer recipe enables never collapses: the encoder writes
    /// both a dictionary page and its index stream, and ZSTD cannot recover
    /// either, so the encoded size tracks the row count.
    fn incompressible_frozen(tenant: DataTenantId, rows: usize) -> FrozenMemtable {
        let mut fields = vec![Field::new(
            "wyrd_event_time",
            DataType::Timestamp(TimeUnit::Microsecond, None),
            false,
        )];
        let row_count = i64::try_from(rows).expect("test row count fits i64");
        let mut columns: Vec<arrow::array::ArrayRef> = vec![Arc::new(
            TimestampMicrosecondArray::from_iter_values(0..row_count),
        )];
        let mut state = 0x9E37_79B9_7F4A_7C15_u64;
        for column in 0..INCOMPRESSIBLE_PAYLOAD_COLUMNS {
            fields.push(Field::new(
                format!("payload_{column:02}"),
                DataType::Int32,
                false,
            ));
            let values = (0..rows)
                .map(|_| {
                    state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
                    let mut mixed = state;
                    mixed = (mixed ^ (mixed >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
                    let bytes = (mixed ^ (mixed >> 31)).to_le_bytes();
                    i32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])
                })
                .collect::<Vec<_>>();
            columns.push(Arc::new(arrow::array::Int32Array::from(values)));
        }
        let schema = Arc::new(Schema::new(fields));
        let batch = RecordBatch::try_new(Arc::clone(&schema), columns)
            .expect("low-compressibility fixture batch");
        FrozenMemtable {
            seal_id: 7,
            seal_key: SealKey::new(
                tenant,
                TableRef::new(BifrostNamespace::Bifrost, "incompressible"),
                crate::test_support::day_partition(2026, 7, 14),
            ),
            shard_id: 1,
            schema,
            batches: vec![batch],
            metas: vec![],
            opened_at: std::time::Instant::now(),
            closed_at: std::time::Instant::now(),
            arrow_bytes: 0,
        }
    }

    /// Encodes one frozen member through the rolling writer at an exact target.
    fn encode_with_target(
        frozen: &FrozenMemtable,
        binding: &TenantTableBinding,
        tenant: DataTenantId,
        scratch_dir: &Path,
        target_object_bytes: u64,
    ) -> Result<ParquetEncoded, ScribeError> {
        let layout = test_layout(frozen.schema.as_ref());
        ParquetBatchEncoder {
            frozen,
            binding,
            seal_tenant: tenant,
            candidates: file_candidates(&frozen.batches),
            first_ordinal: 0,
            scratch_dir,
            object_base: "tenant/table/incompressible",
            layout: &layout,
            memory: crate::resources::ScribeResources::for_test(),
            target_object_bytes,
        }
        .encode()
    }

    /// Reads one sealed artifact's row-group compressed sizes.
    fn compressed_row_group_sizes(path: &Path) -> Vec<i64> {
        let file = std::fs::File::open(path).expect("sealed artifact");
        let reader = SerializedFileReader::new(file).expect("sealed artifact metadata");
        reader
            .metadata()
            .row_groups()
            .iter()
            .map(RowGroupMetaData::compressed_size)
            .collect()
    }

    /// A batch whose encoding passes the object target is written whole into
    /// one artifact, never refused, split, or re-encoded, and a retry
    /// reproduces it byte for byte.
    ///
    /// Rolling is decided only after a batch is written, so a target far below
    /// one batch's encoded size is a soft overshoot, not a failure.
    ///
    /// # Panics
    ///
    /// Panics when the batch is refused, split across artifacts, fails to
    /// overshoot the target, loses statistics, or encodes differently on retry.
    #[test]
    fn a_batch_encoding_past_the_object_target_is_one_deterministic_artifact() {
        let tenant = DataTenantId::new_v7();
        let rows = 150_000;
        let target = 1024 * 1024;
        let frozen = incompressible_frozen(tenant, rows);
        let binding = TenantTableBinding::resolve((tenant, frozen.seal_key.table.clone()))
            .expect("tenant binding");
        let scratch = tempfile::tempdir().expect("rolling scratch");
        let encoded = encode_with_target(&frozen, &binding, tenant, scratch.path(), target)
            .expect("an encoding past the target is accepted");

        let facts = encoded
            .artifacts
            .iter()
            .map(|artifact| (artifact.ordinal, artifact.row_count))
            .collect::<Vec<_>>();
        assert_eq!(facts, vec![(0, rows)], "one batch is one artifact");
        let artifact = &encoded.artifacts[0];
        assert!(
            artifact.file_size > target,
            "the fixture must overshoot the target, observed {} bytes",
            artifact.file_size
        );
        let sizes = compressed_row_group_sizes(&artifact.scratch_path);
        assert_eq!(sizes.len(), artifact.row_group_stats.len());
        assert!(
            artifact
                .row_group_stats
                .iter()
                .all(|stats| stats.min_event_time.is_some() && stats.max_event_time.is_some()),
            "statistics must survive every group"
        );

        let mut sealed = std::fs::read_dir(scratch.path())
            .expect("scratch listing")
            .map(|entry| entry.expect("scratch entry").file_name())
            .collect::<Vec<_>>();
        sealed.sort_unstable();
        assert_eq!(
            sealed.len(),
            1,
            "no artifact may be deleted or re-encoded: {sealed:?}"
        );

        let retry_scratch = tempfile::tempdir().expect("retry scratch");
        let retried = encode_with_target(&frozen, &binding, tenant, retry_scratch.path(), target)
            .expect("deterministic retry is accepted");
        let identity = |encoded: &ParquetEncoded| {
            encoded
                .artifacts
                .iter()
                .map(|artifact| {
                    (
                        artifact.object_identity.clone(),
                        artifact.file_size,
                        artifact.checksum.clone(),
                        artifact.row_count,
                    )
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(
            identity(&retried),
            identity(&encoded),
            "retry must reproduce identical artifacts byte for byte"
        );
    }

    /// Asserts the row-group statistics the writer reported are the footer's
    /// own, group for group, with event-time bounds preserved and ordered.
    ///
    /// A claim prunes on these bounds without reopening the data pages, so a
    /// reported statistic that does not correspond to a flushed row group, or
    /// that lost its event-time bounds, would silently break pruning.
    ///
    /// # Panics
    ///
    /// Panics when the counts disagree, a group's row count differs from the
    /// reported one, or an event-time bound is absent or misordered.
    fn assert_row_group_stats_match_footer(
        metadata: &parquet::file::metadata::ParquetMetaData,
        artifact: &BoundedParquetArtifact,
    ) {
        assert_eq!(
            artifact.row_group_stats.len(),
            metadata.num_row_groups(),
            "one reported statistic per flushed row group"
        );
        for (group, stats) in metadata
            .row_groups()
            .iter()
            .zip(artifact.row_group_stats.iter())
        {
            assert_eq!(
                u64::try_from(group.num_rows()).expect("nonnegative row count"),
                u64::try_from(stats.row_count).expect("row count fits u64")
            );
            assert!(
                stats.min_event_time.is_some() && stats.max_event_time.is_some(),
                "event-time bounds must survive into the claim's evidence"
            );
            assert!(
                stats.min_event_time <= stats.max_event_time,
                "event-time bounds must be ordered"
            );
        }
    }

    /// Asserts one sealed footer's identity fields, by their exact wire names,
    /// describe the artifact that carries them.
    ///
    /// A claim parses these fields from a file it did not write, so the
    /// fingerprint and object identity must be the artifact's own rather than a
    /// sibling's, and no retired size-policy field may reappear.
    ///
    /// The footer's fingerprint is the exact Arrow layout identity, which is a
    /// different question from the catalog identity the artifact reports: it
    /// must separate spellings — `Utf8` from `LargeUtf8`, nullable from
    /// required — that the catalog deliberately normalizes across an Iceberg
    /// round trip, so `sealed_fingerprint` is computed through the footer
    /// owner's own entry point.
    ///
    /// # Panics
    ///
    /// Panics when a field is missing, names another artifact or tenant, or a
    /// retired field is present.
    fn assert_footer_identity_names_this_artifact(
        fields: &std::collections::BTreeMap<String, String>,
        artifact: &BoundedParquetArtifact,
        sealed_fingerprint: &str,
        tenant: DataTenantId,
    ) {
        assert_eq!(
            fields
                .get(crate::parquet::footer::KEY_SCHEMA)
                .map(String::as_str),
            Some(sealed_fingerprint),
            "the footer fingerprint is the exact layout the writer sealed"
        );
        assert_eq!(
            fields
                .get(crate::parquet::footer::KEY_OBJECT)
                .map(String::as_str),
            Some(artifact.object_identity.as_str()),
            "each artifact names itself, never its sibling"
        );
        assert_eq!(
            fields
                .get(crate::parquet::footer::KEY_TENANT)
                .map(String::as_str),
            Some(tenant.to_string().as_str()),
            "the footer records the authenticated seal tenant"
        );
        assert_eq!(
            fields
                .keys()
                .filter(|key| key.starts_with("wyrd.bifrost."))
                .count(),
            3,
            "only the identity fields are stamped: {fields:?}"
        );
    }

    /// AC22/AC3 unit owner: every sealed artifact's footer carries the complete
    /// evidence a later claim needs, per artifact, across a rolled object.
    ///
    /// A claim reads objects it did not write. Everything it must decide —
    /// which schema it holds, which committed object it is, and where its row
    /// groups and their event-time bounds are — has to be readable from that
    /// file's own footer. This owner encodes a member that rolls into two
    /// artifacts and proves each footer stands alone: identities are per
    /// artifact and never shared, the identity fields are exact, the returned
    /// row-group statistics
    /// are the footer's own, and the physical size, checksum and row count
    /// agree with the bytes on disk.
    ///
    /// # Panics
    ///
    /// Panics when an artifact's footer is missing a field, carries another
    /// artifact's identity, disagrees with the writer's reported statistics or
    /// fingerprint, or does not decode to the rows the artifact claims.
    #[test]
    fn parquet_footer_preserves_complete_claim_evidence() {
        let tenant = DataTenantId::new_v7();
        let mut frozen = incompressible_frozen(tenant, FILE_CANDIDATE_TARGET_ROWS + 16);
        let whole = frozen.batches.remove(0);
        frozen.batches = vec![
            whole.slice(0, FILE_CANDIDATE_TARGET_ROWS),
            whole.slice(FILE_CANDIDATE_TARGET_ROWS, 16),
        ];
        let binding = TenantTableBinding::resolve((tenant, frozen.seal_key.table.clone()))
            .expect("tenant binding");
        let scratch = tempfile::tempdir().expect("rolling scratch");
        let encoded = encode_with_target(
            &frozen,
            &binding,
            tenant,
            scratch.path(),
            DEFAULT_STAGING_TARGET_FILE_SIZE_BYTES,
        )
        .expect("the rolling writer seals the member");
        assert_eq!(
            encoded.artifacts.len(),
            2,
            "the fixture must roll so per-artifact footer evidence is proven, not shared"
        );

        let mut identities = Vec::new();
        for (offset, artifact) in encoded.artifacts.iter().enumerate() {
            assert_eq!(
                usize::from(artifact.ordinal),
                offset,
                "artifact ordinals are contiguous from zero"
            );
            identities.push(artifact.object_identity.clone());

            let reader = SerializedFileReader::new(
                std::fs::File::open(&artifact.scratch_path).expect("sealed artifact"),
            )
            .expect("sealed footer");
            let metadata = reader.metadata();
            let fields: std::collections::BTreeMap<String, String> = metadata
                .file_metadata()
                .key_value_metadata()
                .expect("a sealed footer carries its identity")
                .iter()
                .map(|field| {
                    (
                        field.key.clone(),
                        field.value.clone().unwrap_or_else(|| {
                            panic!("footer field {} must carry a value", field.key)
                        }),
                    )
                })
                .collect();

            assert_footer_identity_names_this_artifact(
                &fields,
                artifact,
                &hex::encode(crate::parquet::footer::schema_fingerprint(frozen.schema.as_ref()).0),
                tenant,
            );

            assert_row_group_stats_match_footer(metadata, artifact);

            // The physical facts agree with the bytes on disk.
            let on_disk = std::fs::metadata(&artifact.scratch_path).expect("sealed artifact stat");
            assert_eq!(artifact.file_size, on_disk.len());
            assert_eq!(
                artifact.checksum,
                checksum_file(&artifact.scratch_path).expect("checksum")
            );
            let decoded: usize = ParquetRecordBatchReaderBuilder::try_new(
                std::fs::File::open(&artifact.scratch_path).expect("reopen artifact"),
            )
            .expect("decode builder")
            .build()
            .expect("decode reader")
            .map(|batch| batch.expect("decoded batch").num_rows())
            .sum();
            assert_eq!(
                decoded, artifact.row_count,
                "the artifact decodes to exactly the rows it claims"
            );

            assert_promotion_record_describes_this_artifact(
                artifact,
                on_disk.len(),
                metadata.num_row_groups(),
            );
        }

        identities.sort_unstable();
        identities.dedup();
        assert_eq!(
            identities.len(),
            encoded.artifacts.len(),
            "object identities are unique across the rolled claim"
        );
    }

    /// Asserts one artifact's promotion record describes that artifact's bytes.
    ///
    /// The promotion record is the evidence a later claim and the catalog
    /// promoter actually consume, so it is part of the footer contract rather
    /// than a by-product: it must describe *this* artifact's own bytes, carry
    /// one split offset per row group in ascending order, and hold bounds that
    /// rebuild losslessly. A record derived from a sibling's size or a stale
    /// row count would promote an object the table cannot read.
    ///
    /// # Panics
    ///
    /// Panics when the record contradicts the artifact's row count or on-disk
    /// size, when its split offsets do not match the footer's row groups in
    /// ascending order, when its statistics are empty or disagree, or when its
    /// wire form is not byte-stable across a round trip.
    fn assert_promotion_record_describes_this_artifact(
        artifact: &BoundedParquetArtifact,
        on_disk_len: u64,
        row_groups: usize,
    ) {
        // The promotion record is the evidence a later claim and the
        // catalog promoter actually consume, so it is part of the footer
        // contract rather than a by-product: it must describe *this*
        // artifact's own bytes, carry one split offset per row group in
        // ascending order, and hold bounds that rebuild losslessly. A
        // record derived from a sibling's size or a stale row count would
        // promote an object the table cannot read.
        let metrics = &artifact.data_file_metrics;
        assert_eq!(
            metrics.record_count,
            u64::try_from(artifact.row_count).expect("row count fits u64"),
            "the promotion record counts this artifact's own rows"
        );
        assert_eq!(
            metrics.file_size_in_bytes, on_disk_len,
            "the promotion record states the bytes actually on disk"
        );
        assert_eq!(
            metrics.split_offsets.len(),
            row_groups,
            "one split offset per row group the footer holds"
        );
        assert!(
            metrics
                .split_offsets
                .windows(2)
                .all(|pair| pair[0] < pair[1]),
            "split offsets must ascend so a reader can bound a group"
        );
        assert!(
            metrics
                .split_offsets
                .first()
                .is_some_and(|first| *first >= 0),
            "a split offset is a position inside the object"
        );
        assert!(
            !metrics.column_sizes.is_empty() && !metrics.value_counts.is_empty(),
            "per-column footer statistics must survive into the record"
        );
        for (field, count) in &metrics.value_counts {
            assert_eq!(
                *count, metrics.record_count,
                "field {field} must count every row of the artifact"
            );
        }
        assert_eq!(
            metrics.lower_bounds.keys().collect::<Vec<_>>(),
            metrics.upper_bounds.keys().collect::<Vec<_>>(),
            "a bounded column carries both of its bounds"
        );
        assert!(
            !metrics.lower_bounds.is_empty(),
            "an artifact with rows has at least one bounded column"
        );
        // The record travels to publication, S6 and Forge as bytes, so a
        // projection that only holds together in this process proves
        // nothing. Round-tripping it here is the writer's half of that
        // handoff: the wire form must be lossless and, because every map is
        // ordered, byte-identical for the same footer on every replay.
        let wire = serde_json::to_vec(metrics).expect("the promotion record serializes");
        let returned: crate::scribe::promotion::ScribeDataFileV1 =
            serde_json::from_slice(&wire).expect("the promotion record deserializes");
        assert_eq!(
            &returned, metrics,
            "the promotion record's wire form must be lossless"
        );
        assert_eq!(
            serde_json::to_vec(&returned).expect("the returned record serializes"),
            wire,
            "the same footer must encode to the same bytes on every replay"
        );
    }

    /// Reads one encoded artifact and proves its footer tenant and timestamp
    /// order.
    ///
    /// # Panics
    ///
    /// Panics when the artifact cannot be decoded, its footer does not record
    /// `tenant`, or its event times differ from `expected_times`.
    fn assert_encoded_rows(encoded: &ParquetEncoded, tenant: DataTenantId, expected_times: &[i64]) {
        let file =
            std::fs::File::open(&encoded.artifacts[0].scratch_path).expect("encoded artifact");
        let reader = ParquetRecordBatchReaderBuilder::try_new(file).expect("encoded parquet");
        crate::parquet::footer::verify_footer_tenant(reader.metadata().file_metadata(), tenant)
            .expect("the footer records the seal tenant");
        let mut reader = reader.build().expect("encoded reader");
        let batch = reader
            .next()
            .expect("encoded row group")
            .expect("encoded batch");
        let times = batch
            .column_by_name("wyrd_event_time")
            .expect("event time")
            .as_any()
            .downcast_ref::<TimestampMicrosecondArray>()
            .expect("timestamp column")
            .values();
        assert_eq!(times, expected_times);
        assert!(
            batch.schema().index_of("data_tenant_id").is_err(),
            "no per-row tenant column is written"
        );
    }

    /// Every sealed artifact carries Iceberg metrics that agree with the exact
    /// footer it closed, plus the fingerprint of the schema it was sealed with.
    ///
    /// The metrics exist so a promoter never has to reopen the object. That is
    /// only safe if they are the footer's own numbers, so this compares each
    /// projected value against the footer read back from the file rather than
    /// against the values the encoder was handed.
    ///
    /// # Panics
    ///
    /// Panics when a projected metric contradicts the footer, when the split
    /// offsets do not name the artifact's row groups, or when the recorded
    /// fingerprint is not the sealed schema's.
    #[test]
    fn sealed_artifacts_carry_footer_agreeing_iceberg_metrics() {
        let tenant = DataTenantId::new_v7();
        let day = crate::test_support::day_partition(2026, 7, 14);
        let schema = Arc::new(Schema::new(vec![
            Field::new("service", DataType::Utf8, false),
            Field::new(
                "wyrd_event_time",
                DataType::Timestamp(TimeUnit::Microsecond, None),
                false,
            ),
        ]));
        let batch = RecordBatch::try_new(
            Arc::clone(&schema),
            vec![
                Arc::new(StringArray::from(vec!["api"; 4])),
                Arc::new(TimestampMicrosecondArray::from(vec![10, 20, 30, 40])),
            ],
        )
        .expect("stored batch");
        let frozen = FrozenMemtable {
            seal_id: 7,
            seal_key: SealKey::new(
                tenant,
                TableRef::new(BifrostNamespace::Bifrost, "promotion_metrics"),
                day,
            ),
            shard_id: 0,
            schema: Arc::clone(&schema),
            batches: vec![batch],
            metas: vec![],
            opened_at: std::time::Instant::now(),
            closed_at: std::time::Instant::now(),
            arrow_bytes: 0,
        };
        let binding = TenantTableBinding::resolve((tenant, frozen.seal_key.table.clone()))
            .expect("tenant binding");
        let (_scratch, encoded) = encode_for_test(&frozen, &binding, tenant);
        let artifact = &encoded.artifacts[0];

        let reader = SerializedFileReader::new(
            std::fs::File::open(&artifact.scratch_path).expect("sealed artifact"),
        )
        .expect("sealed footer");
        let metadata = reader.metadata();
        let footer_rows: u64 = metadata
            .row_groups()
            .iter()
            .map(|group| u64::try_from(group.num_rows()).expect("nonnegative row count"))
            .sum();

        let metrics = &artifact.data_file_metrics;
        assert_eq!(metrics.record_count, footer_rows);
        assert_eq!(metrics.file_size_in_bytes, artifact.file_size);
        assert_eq!(metrics.split_offsets.len(), metadata.num_row_groups());
        assert_eq!(
            metrics.split_offsets[0],
            metadata.row_group(0).file_offset().unwrap_or(4)
        );
        for (field_id, size) in &metrics.column_sizes {
            let footer_size: u64 = metadata
                .row_groups()
                .iter()
                .flat_map(RowGroupMetaData::columns)
                .filter(|column| {
                    column.column_descr().name()
                        == schema
                            .field(usize::try_from(*field_id - 1).expect("field id is positive"))
                            .name()
                })
                .map(|column| u64::try_from(column.compressed_size()).expect("nonnegative size"))
                .sum();
            assert_eq!(*size, footer_size);
        }
        assert_eq!(metrics.value_counts.len(), schema.fields().len());
        assert!(
            metrics
                .value_counts
                .values()
                .all(|count| *count == footer_rows)
        );
        assert_eq!(metrics.lower_bounds.len(), schema.fields().len());
        assert_eq!(metrics.upper_bounds.len(), schema.fields().len());
        assert_eq!(
            artifact.schema_fingerprint,
            hex::encode(
                crate::schema::SchemaFingerprint::from_arrow_schema_exact(schema.as_ref()).0
            )
        );
    }

    /// Generation encoding sorts every stored batch together, records the
    /// authenticated tenant in the footer, numbers artifacts from zero, and refuses a
    /// mismatched tenant before materialization.
    ///
    /// The generation is the encoding unit: its stored batches are one sorted
    /// output, not one artifact per batch. Sorting across the whole generation
    /// is what a claim's merge later relies on, so proving the interleaved
    /// timestamps come back globally ordered is proving that contract, not
    /// merely that one batch sorted.
    ///
    /// # Panics
    ///
    /// Panics when the rows are not globally ordered, when an artifact is not
    /// numbered from zero, or when a tenant that does not match the binding is
    /// allowed to materialize rows.
    #[test]
    fn generation_encoding_preserves_sort_tenant_and_artifact_identity() {
        let tenant = DataTenantId::new_v7();
        let day = crate::test_support::day_partition(2026, 7, 14);
        let schema = Arc::new(Schema::new(vec![Field::new(
            "wyrd_event_time",
            DataType::Timestamp(TimeUnit::Microsecond, None),
            false,
        )]));
        let stored_batch = |timestamps: Vec<i64>| {
            RecordBatch::try_new(
                Arc::clone(&schema),
                vec![Arc::new(TimestampMicrosecondArray::from(timestamps))],
            )
            .expect("stored batch")
        };
        let frozen = FrozenMemtable {
            seal_id: 42,
            seal_key: SealKey::new(
                tenant,
                TableRef::new(BifrostNamespace::Bifrost, "sorted_generations"),
                day,
            ),
            shard_id: 4,
            schema: Arc::clone(&schema),
            batches: vec![stored_batch(vec![40, 10]), stored_batch(vec![30, 20])],
            metas: vec![],
            opened_at: std::time::Instant::now(),
            closed_at: std::time::Instant::now(),
            arrow_bytes: 0,
        };
        let binding = TenantTableBinding::resolve((tenant, frozen.seal_key.table.clone()))
            .expect("tenant binding");

        let (_scratch, encoded) = encode_for_test(&frozen, &binding, tenant);
        assert_eq!(encoded.artifacts.len(), 1);
        assert_eq!(encoded.artifacts[0].ordinal, 0);
        assert!(
            encoded.artifacts[0]
                .object_identity
                .ends_with("-00000.parquet")
        );
        assert_encoded_rows(&encoded, tenant, &[10, 20, 30, 40]);

        let mismatch_scratch = tempfile::tempdir().expect("mismatch scratch");
        let layout = test_layout(frozen.schema.as_ref());
        let error = encode_batch(
            &frozen,
            &binding,
            DataTenantId::new_v7(),
            mismatch_scratch.path(),
            "tenant/table/member",
            &layout,
            crate::resources::ScribeResources::for_test(),
        )
        .expect_err("mismatched tenant must fail closed");
        assert!(
            matches!(error, ScribeError::Internal { detail } if detail.contains("tenant-table binding mismatch"))
        );
    }

    #[test]
    fn parquet_writer_partition_matches_seal_key() {
        // Regression test for C2: partition = seal_key.partition, not row min/max
        let seal_partition = crate::test_support::day_partition(2026, 7, 14);
        let tenant = DataTenantId::new_v7();
        let frozen = build_test_frozen(
            seal_partition,
            tenant,
            vec!["api", "api"],
            vec![1_000_000, 2_000_000], // timestamps don't matter for the partition
        );

        let binding =
            TenantTableBinding::resolve((frozen.seal_key.tenant, frozen.seal_key.table.clone()))
                .unwrap();
        let (_scratch, encoded) = encode_for_test(&frozen, &binding, frozen.seal_key.tenant);
        assert_eq!(encoded.partition, seal_partition);
    }

    #[test]
    fn parquet_writer_honors_sort_order() {
        // Round-trip: write shuffled input, verify sort order in column stats
        let tenant = DataTenantId::new_v7();

        let frozen = build_test_frozen(
            crate::test_support::day_partition(2026, 7, 14),
            tenant,
            vec!["a", "b", "c", "d"],
            vec![400, 100, 300, 200], // Shuffled timestamps
        );

        let binding =
            TenantTableBinding::resolve((frozen.seal_key.tenant, frozen.seal_key.table.clone()))
                .unwrap();
        let (_scratch, encoded) = encode_for_test(&frozen, &binding, frozen.seal_key.tenant);

        // Re-read and verify sorted order
        let file = std::fs::File::open(&encoded.artifacts[0].scratch_path).unwrap();
        let builder = ParquetRecordBatchReaderBuilder::try_new(file).unwrap();
        let mut reader = builder.build().unwrap();
        let batch = reader.next().unwrap().unwrap();

        let service_col = batch
            .column_by_name("service")
            .unwrap()
            .as_any()
            .downcast_ref::<StringArray>()
            .unwrap();

        let time_col = batch
            .column_by_name("wyrd_event_time")
            .unwrap()
            .as_any()
            .downcast_ref::<TimestampMicrosecondArray>()
            .unwrap();

        // Rows travel with their event time, the declared sort key.
        assert_eq!((service_col.value(0), time_col.value(0)), ("b", 100));
        assert_eq!((service_col.value(1), time_col.value(1)), ("d", 200));
        assert_eq!((service_col.value(2), time_col.value(2)), ("c", 300));
        assert_eq!((service_col.value(3), time_col.value(3)), ("a", 400));
    }

    #[test]
    fn parquet_writer_writes_bloom_and_page_index() {
        let tenant = DataTenantId::new_v7();
        let frozen = build_test_frozen(
            crate::test_support::day_partition(2026, 7, 14),
            tenant,
            vec!["api", "api"],
            vec![1_000_000, 2_000_000],
        );

        let binding =
            TenantTableBinding::resolve((frozen.seal_key.tenant, frozen.seal_key.table.clone()))
                .unwrap();
        let (_scratch, encoded) = encode_for_test(&frozen, &binding, frozen.seal_key.tenant);

        let reader = SerializedFileReader::new(
            std::fs::File::open(&encoded.artifacts[0].scratch_path).unwrap(),
        )
        .unwrap();
        let metadata = reader.metadata();

        // Verify page index (offset index) is present
        let rg = metadata.row_groups().first().unwrap();
        let service_col = rg
            .columns()
            .iter()
            .find(|column| column.column_descr().name() == "service")
            .unwrap();
        assert!(
            service_col.bloom_filter_offset().is_none(),
            "an undeclared column must not be Bloomed"
        );

        let col = rg
            .columns()
            .iter()
            .find(|column| column.column_descr().name() == "wyrd_event_time")
            .unwrap();
        assert!(
            col.bloom_filter_offset().is_some(),
            "a declared Bloom column must have a bloom filter"
        );

        // Page index presence is indicated by offset_index_offset being set
        assert!(
            col.offset_index_offset().is_some() || col.column_index_offset().is_some(),
            "page index metadata not found"
        );
    }

    #[test]
    fn parquet_writer_returns_envelopes_unmodified() {
        // Encoding does not touch the `ScribeAppendMeta` list
        let tenant = DataTenantId::new_v7();
        let frozen = build_test_frozen(
            crate::test_support::day_partition(2026, 7, 14),
            tenant,
            vec!["api"],
            vec![1_000_000],
        );

        let meta = crate::scribe::wal::ScribeAppendMeta {
            batch_id: [0u8; 16],
            schema_fingerprint: [0; 32],
            data_digest: [0; 32],
            data_len: 0,
            payload_digest: [0; 32],
            payload_len: 0,
            slice_index: 0,
            slice_count: 1,
            rows_accepted: 1,
            wal_lsn_min: crate::scribe::wal::WalLsn::new(0),
            wal_lsn_max: crate::scribe::wal::WalLsn::new(0),
            seal_key: "test".to_string(),
        };

        let mut frozen_with_envelopes = frozen;
        frozen_with_envelopes.metas = vec![meta.clone()];

        let binding = TenantTableBinding::resolve((
            frozen_with_envelopes.seal_key.tenant,
            frozen_with_envelopes.seal_key.table.clone(),
        ))
        .unwrap();
        let (_scratch, encoded) = encode_for_test(
            &frozen_with_envelopes,
            &binding,
            frozen_with_envelopes.seal_key.tenant,
        );

        assert_eq!(encoded.append_metas.len(), 1);
        assert_eq!(encoded.append_metas[0].batch_id, meta.batch_id);
    }

    #[test]
    fn sort_batch_uses_one_key_for_every_table() {
        let schema = Arc::new(Schema::new(vec![
            arrow::datatypes::Field::new("service", DataType::Utf8, false),
            arrow::datatypes::Field::new(
                "wyrd_event_time",
                DataType::Timestamp(arrow::datatypes::TimeUnit::Microsecond, None),
                false,
            ),
        ]));
        let batch = RecordBatch::try_new(
            schema,
            vec![
                Arc::new(StringArray::from(vec!["c", "b", "a"])),
                Arc::new(TimestampMicrosecondArray::from(vec![3, 2, 1])),
            ],
        )
        .unwrap();

        let layout = test_layout(batch.schema().as_ref());
        let sorted = sort_batch(&batch, &layout).unwrap();
        let services = sorted
            .column_by_name("service")
            .unwrap()
            .as_any()
            .downcast_ref::<StringArray>()
            .unwrap();
        let times = sorted
            .column_by_name("wyrd_event_time")
            .unwrap()
            .as_any()
            .downcast_ref::<TimestampMicrosecondArray>()
            .unwrap();
        assert_eq!((services.value(0), times.value(0)), ("a", 1));
        assert_eq!((services.value(1), times.value(1)), ("b", 2));
        assert_eq!((services.value(2), times.value(2)), ("c", 3));
    }

    /// A real sealed file proves the writer recipe on its own footer rather
    /// than on the builder that produced it: the low-cardinality
    /// `service` column encodes through a dictionary, `wyrd_event_time`
    /// encodes as `DELTA_BINARY_PACKED` with no dictionary page, and every row
    /// decodes back exactly as written.
    #[test]
    fn sealed_file_footer_encoding_contract() {
        let tenant = DataTenantId::new_v7();
        let rows = 4_096;
        let timestamps: Vec<i64> = (0..rows).map(|row| 1_767_312_000_000_000 + row).collect();
        let frozen = build_test_frozen(
            crate::test_support::day_partition(2026, 7, 14),
            tenant,
            vec!["api"; usize::try_from(rows).expect("row count fits usize")],
            timestamps.clone(),
        );
        let binding =
            TenantTableBinding::resolve((tenant, frozen.seal_key.table.clone())).expect("binding");
        let (_scratch, encoded) = encode_for_test(&frozen, &binding, tenant);

        let artifact = encoded
            .artifacts
            .as_slice()
            .first()
            .expect("seal produced one artifact");
        let file = std::fs::File::open(&artifact.scratch_path).expect("open sealed artifact");
        let reader = SerializedFileReader::new(file).expect("read sealed footer");
        let metadata = reader.metadata();

        let mut saw_dictionary = false;
        let mut saw_event_time = false;
        for group in metadata.row_groups() {
            for column in group.columns() {
                let name = column.column_path().string();
                let encodings = column.encodings().collect::<Vec<_>>();
                if name == "service" {
                    saw_dictionary = true;
                    assert!(
                        encodings.contains(&parquet::basic::Encoding::RLE_DICTIONARY)
                            || encodings.contains(&parquet::basic::Encoding::PLAIN_DICTIONARY),
                        "low-cardinality {name} must be dictionary-encoded, saw {encodings:?}"
                    );
                }
                if name == "wyrd_event_time" {
                    saw_event_time = true;
                    assert!(
                        encodings.contains(&parquet::basic::Encoding::DELTA_BINARY_PACKED),
                        "wyrd_event_time must be DELTA_BINARY_PACKED, saw {encodings:?}"
                    );
                    assert!(
                        !encodings.contains(&parquet::basic::Encoding::RLE_DICTIONARY)
                            && !encodings.contains(&parquet::basic::Encoding::PLAIN_DICTIONARY),
                        "wyrd_event_time must not carry a dictionary page, saw {encodings:?}"
                    );
                }
            }
        }
        assert!(saw_dictionary && saw_event_time);

        let decoded_file = std::fs::File::open(&artifact.scratch_path).expect("reopen artifact");
        let batches = ParquetRecordBatchReaderBuilder::try_new(decoded_file)
            .expect("decode builder")
            .build()
            .expect("decode reader")
            .collect::<Result<Vec<_>, _>>()
            .expect("decode rows");
        let decoded_rows: usize = batches.iter().map(RecordBatch::num_rows).sum();
        assert_eq!(
            decoded_rows,
            usize::try_from(rows).expect("row count fits usize")
        );
        let mut decoded_times = Vec::with_capacity(decoded_rows);
        for batch in &batches {
            let column = batch
                .column_by_name("wyrd_event_time")
                .expect("decoded event-time column")
                .as_any()
                .downcast_ref::<TimestampMicrosecondArray>()
                .expect("decoded event-time is microsecond timestamps");
            decoded_times.extend(column.values().iter().copied());
        }
        assert_eq!(decoded_times, timestamps);
    }

    #[test]
    fn tenant_binding_mismatch_fails_before_encoding() {
        let seal_tenant = DataTenantId::new_v7();
        let binding_tenant = DataTenantId::new_v7();
        let frozen = build_test_frozen(
            crate::test_support::day_partition(2026, 7, 14),
            seal_tenant,
            vec!["api"],
            vec![1],
        );
        let binding =
            TenantTableBinding::resolve((binding_tenant, frozen.seal_key.table.clone())).unwrap();

        let scratch = tempfile::tempdir().unwrap();
        let layout = test_layout(frozen.schema.as_ref());
        let error = encode_batch(
            &frozen,
            &binding,
            seal_tenant,
            scratch.path(),
            "object",
            &layout,
            crate::resources::ScribeResources::for_test(),
        )
        .unwrap_err();
        assert!(
            matches!(error, ScribeError::Internal { detail } if detail.contains("binding mismatch"))
        );
    }
}
