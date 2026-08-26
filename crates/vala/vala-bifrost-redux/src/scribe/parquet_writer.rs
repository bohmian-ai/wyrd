//! Parquet writer for frozen memtable snapshots.
//!
//! `encode_batch` stamps the authenticated tenant before encoding a
//! `FrozenMemtable` snapshot to Parquet. Every file uses sort order
//! `(data_tenant_id, wyrd_event_time)` and takes its partition day from the
//! seal-key (never from row min/max).

use std::io::{BufReader, BufWriter, Read, Seek, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use arrow::array::{Array, StringArray};
use arrow::compute::SortColumn;
use arrow::compute::concat_batches;
use arrow::compute::lexsort_to_indices;
use arrow::compute::take;
use arrow::datatypes::{DataType, Schema};
use arrow::record_batch::RecordBatch;
use parquet::arrow::ArrowWriter;
use parquet::file::metadata::RowGroupMetaData;
use sha2::{Digest, Sha256};
use wyrd_spec::DataTenantId;
use wyrd_spec::vala::api::AuditEvent;
use wyrd_spec::vala::managed_columns::DATA_TENANT_ID;

use crate::catalog::TenantTableBinding;
use crate::catalog::layout::PhysicalLayout;
use crate::catalog::layout::TimePartition;
use crate::contracts::ScribeError;
use crate::parquet::memory::{
    BifrostArrowLogicalSizer, BifrostParquetMemoryEnvelope, BoundedRowSlice, MAX_FILE_BYTES,
    MAX_LOGICAL_ROW_GROUP_BYTES, validate_writer_v2_structure,
};
use crate::parquet::writer_properties::bifrost_writer_properties_with_metadata;
use crate::resources::ScribeGenerationScratch;
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

impl FileCandidate {
    /// Returns the exact Arrow memory represented by this whole-batch candidate.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::Internal`] when the candidate byte sum overflows.
    pub(crate) fn arrow_bytes(self, batches: &[RecordBatch]) -> Result<usize, ScribeError> {
        batches[self.start..self.end]
            .iter()
            .try_fold(0_usize, |sum, batch| {
                sum.checked_add(batch.get_array_memory_size())
                    .ok_or_else(|| ScribeError::Internal {
                        detail: "Parquet candidate Arrow footprint overflowed".to_owned(),
                    })
            })
    }
}

/// Returns the largest exact Arrow footprint among whole-batch candidates.
///
/// Admission shares the encoder's candidate grouping, ensuring several small
/// batches that merge into one candidate reserve their complete peak.
///
/// # Errors
///
/// Returns [`ScribeError::Internal`] when a candidate byte sum overflows.
pub(crate) fn largest_candidate_bytes(batches: &[RecordBatch]) -> Result<usize, ScribeError> {
    largest_candidate_bytes_from_facts(
        batches
            .iter()
            .map(|batch| (batch.num_rows(), batch.get_array_memory_size())),
    )
}

/// Returns the largest whole-batch candidate from row and Arrow-byte facts.
///
/// This is the allocation-free form of [`largest_candidate_bytes`]. Ingress
/// and the shard owner use it before WAL mutation so their replayability check
/// cannot drift from the encoder's grouping rule.
///
/// # Errors
///
/// Returns [`ScribeError::Internal`] when candidate row or byte arithmetic
/// overflows.
pub(crate) fn largest_candidate_bytes_from_facts(
    facts: impl IntoIterator<Item = (usize, usize)>,
) -> Result<usize, ScribeError> {
    let mut candidate_rows = 0_usize;
    let mut candidate_bytes = 0_usize;
    let mut largest = 0_usize;
    for (rows, bytes) in facts {
        if candidate_rows != 0
            && candidate_rows
                .checked_add(rows)
                .is_none_or(|projected| projected > FILE_CANDIDATE_TARGET_ROWS)
        {
            largest = largest.max(candidate_bytes);
            candidate_rows = 0;
            candidate_bytes = 0;
        }
        candidate_rows = candidate_rows
            .checked_add(rows)
            .ok_or_else(|| ScribeError::Internal {
                detail: "Parquet candidate row count overflowed".to_owned(),
            })?;
        candidate_bytes =
            candidate_bytes
                .checked_add(bytes)
                .ok_or_else(|| ScribeError::Internal {
                    detail: "Parquet candidate Arrow footprint overflowed".to_owned(),
                })?;
    }
    Ok(largest.max(candidate_bytes))
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
    /// `AuditEvent` list threaded forward for 's seal transaction.
    pub audit_events: Vec<AuditEvent>,
    /// `ScribeAppendMeta` list threaded forward for 's `file_list` INSERT.
    pub append_metas: Vec<ScribeAppendMeta>,
}

/// Move-owned nonempty artifact set and its generation scratch authority.
#[derive(Debug)]
pub struct BoundedParquetArtifactSet {
    /// Contiguous writer-v2 artifacts in publication order.
    artifacts: Vec<BoundedParquetArtifact>,
    /// Exact generation directory whose charge follows the artifacts.
    scratch: Option<ScribeGenerationScratch>,
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
                detail: "writer-v2 artifact set must be nonempty and contiguous".to_owned(),
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
        scratch: ScribeGenerationScratch,
    ) -> Result<(), ScribeError> {
        if self.scratch.replace(scratch).is_some() {
            return Err(ScribeError::Internal {
                detail: "writer-v2 artifact set already owns scratch".to_owned(),
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
    /// exhausts its retries and poisons shared volume health.
    pub(crate) fn cleanup(mut self) -> Result<(), ScribeError> {
        let Some(scratch) = self.scratch.take() else {
            return Ok(());
        };
        scratch.cleanup().map_err(|error| ScribeError::Internal {
            detail: format!("writer-v2 scratch cleanup failed: {error}"),
        })
    }

    /// Retains scratch and its charge after an unresolved commit outcome.
    ///
    /// Startup reconciliation owns removal of the exact namespace. Forgetting
    /// the local scratch owner prevents its ordinary cancellation cleanup from
    /// deleting evidence that may already be catalog-visible.
    pub(crate) fn retain_for_reconciliation(mut self) {
        if let Some(scratch) = self.scratch.take() {
            std::mem::forget(scratch);
        }
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

/// One sealed writer-v2 object retained on generation-owned scratch.
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
}

/// File sink that refuses before crossing the 128 MiB physical ceiling.
struct CappedScratchWriter {
    inner: BufWriter<std::fs::File>,
    written: u64,
}

impl CappedScratchWriter {
    /// Wraps one newly created generation-owned file.
    fn new(file: std::fs::File) -> Self {
        Self {
            inner: BufWriter::new(file),
            written: 0,
        }
    }
}

impl Write for CappedScratchWriter {
    /// Writes only complete buffers that fit the closed physical ceiling.
    ///
    /// # Errors
    /// Returns `StorageFull` before mutation when the buffer would cross the cap,
    /// or propagates the underlying scratch-file error.
    fn write(&mut self, buffer: &[u8]) -> std::io::Result<usize> {
        let requested = u64::try_from(buffer.len()).unwrap_or(u64::MAX);
        if self.written.saturating_add(requested) > MAX_FILE_BYTES {
            return Err(std::io::Error::new(
                std::io::ErrorKind::StorageFull,
                "Scribe writer-v2 artifact exceeds 128 MiB",
            ));
        }
        let written = self.inner.write(buffer)?;
        self.written = self
            .written
            .checked_add(u64::try_from(written).unwrap_or(u64::MAX))
            .ok_or_else(|| std::io::Error::other("Scribe scratch byte count overflows"))?;
        Ok(written)
    }

    /// Flushes admitted bytes to the generation-owned file.
    ///
    /// # Errors
    /// Propagates the underlying scratch-file flush error.
    fn flush(&mut self) -> std::io::Result<()> {
        self.inner.flush()
    }
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
/// `AuditEvent` + `ScribeAppendMeta` lists unmodified (threaded forward for 's seal
/// transaction).
///
/// # Errors
/// Returns [`ScribeError::Internal`] when the binding, tenant column, tenant
/// values, sort keys, or Parquet encoding is invalid.
pub(crate) fn encode_batch(
    frozen: &FrozenMemtable,
    binding: &TenantTableBinding,
    seal_tenant: DataTenantId,
    scratch_dir: &Path,
    object_base: &str,
    layout: &PhysicalLayout,
    footer_reservation: crate::scribe::memory::EncodedFooterReservation,
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
        footer_reservation,
    }
    .encode()
}

/// Encodes exactly one whole-batch candidate with generation-global ordinals.
///
/// The caller serially owns candidate workspace and scratch, so this operation
/// never retains encoded bytes from an earlier candidate.
///
/// # Errors
///
/// Returns [`ScribeError::Internal`] for invalid identity, candidate bounds,
/// Arrow materialization, scratch IO, or an invalid encoded artifact.
pub(crate) fn encode_candidate(
    request: CandidateEncodeRequest<'_>,
    footer_reservation: crate::scribe::memory::EncodedFooterReservation,
) -> Result<ParquetEncoded, ScribeError> {
    ParquetBatchEncoder {
        frozen: request.frozen,
        binding: request.binding,
        seal_tenant: request.seal_tenant,
        candidates: vec![request.candidate],
        first_ordinal: request.first_ordinal,
        scratch_dir: request.scratch_dir,
        object_base: request.object_base,
        layout: request.layout,
        footer_reservation,
    }
    .encode()
}

/// Borrowed inputs for one candidate-local Parquet encoding operation.
#[derive(Clone, Copy)]
pub(crate) struct CandidateEncodeRequest<'a> {
    /// Immutable generation containing the candidate's stored batches.
    pub(crate) frozen: &'a FrozenMemtable,
    /// Tenant-qualified physical table binding.
    pub(crate) binding: &'a TenantTableBinding,
    /// Authenticated tenant stamped into physical output.
    pub(crate) seal_tenant: DataTenantId,
    /// Exact whole-batch candidate encoded by this operation.
    pub(crate) candidate: FileCandidate,
    /// First generation-global artifact ordinal.
    pub(crate) first_ordinal: usize,
    /// Candidate-owned scratch directory.
    pub(crate) scratch_dir: &'a Path,
    /// Deterministic generation object prefix.
    pub(crate) object_base: &'a str,
    /// Registered physical write recipe applied to every artifact.
    pub(crate) layout: &'a PhysicalLayout,
}

/// Owns one bounded Parquet encoding workflow and its footer reservation.
struct ParquetBatchEncoder<'a> {
    /// Immutable generation being encoded.
    frozen: &'a FrozenMemtable,
    /// Tenant-qualified physical binding validated before materialization.
    binding: &'a TenantTableBinding,
    /// Authenticated tenant stamped into the physical batch.
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
    /// Move-only memory child retained through footer inspection.
    footer_reservation: crate::scribe::memory::EncodedFooterReservation,
}

/// Result of encoding one candidate logical slice.
enum ArtifactEncodingOutcome {
    /// Candidate satisfied the physical row-group ceiling.
    Accepted(BoundedParquetArtifact),
    /// Candidate must be retried as two smaller ordered slices.
    Bisected {
        /// First half preserving the source row order.
        left: BoundedRowSlice,
        /// Second half preserving the source row order.
        right: BoundedRowSlice,
    },
}

impl ParquetBatchEncoder<'_> {
    /// Executes validation, bounded materialization, and exact artifact encoding.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::Internal`] for an invalid binding, Arrow
    /// materialization failure, or an invalid encoded artifact.
    fn encode(self) -> Result<ParquetEncoded, ScribeError> {
        debug_assert_eq!(
            self.footer_reservation.bytes(),
            crate::scribe::memory::PARQUET_FOOTER_CHILD_BYTES
        );
        let mut artifacts = Vec::new();
        let mut row_group_stats = Vec::new();
        for candidate in &self.candidates {
            let sorted_batch = self.prepare_sorted_candidate(*candidate)?;
            let (mut candidate_artifacts, mut candidate_stats) = self.encode_artifacts(
                &sorted_batch,
                self.first_ordinal.saturating_add(artifacts.len()),
            )?;
            artifacts.append(&mut candidate_artifacts);
            row_group_stats.append(&mut candidate_stats);
        }
        Ok(ParquetEncoded {
            artifacts: BoundedParquetArtifactSet::encoded(artifacts)?,
            row_group_stats,
            partition: self.frozen.seal_key.partition,
            audit_events: self.frozen.events.clone(),
            append_metas: self.frozen.metas.clone(),
        })
    }

    /// Validates physical identity and builds the tenant-stamped sorted batch.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::Internal`] when identity, concatenation, tenant
    /// stamping, or sort-key validation fails.
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
        let stamped_batch = stamp_tenant(&encoder_batch, self.seal_tenant)?;
        sort_batch(&stamped_batch, self.layout)
    }

    /// Encodes logical slices, bisecting any oversized physical row group.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::Internal`] for invalid logical slicing, scratch
    /// IO, Parquet encoding, footer inspection, or artifact identity.
    fn encode_artifacts(
        &self,
        sorted_batch: &RecordBatch,
        first_ordinal: usize,
    ) -> Result<(Vec<BoundedParquetArtifact>, Vec<RowGroupStats>), ScribeError> {
        let slices = BifrostArrowLogicalSizer::slice(sorted_batch).map_err(|detail| {
            ScribeError::Internal {
                detail: format!("writer-v2 logical slicing refused: {detail}"),
            }
        })?;
        if slices.is_empty() {
            return Err(ScribeError::Internal {
                detail: "writer-v2 cannot publish an empty artifact set".to_owned(),
            });
        }
        let mut pending: std::collections::VecDeque<_> = slices.into();
        let mut artifacts = Vec::with_capacity(pending.len());
        let mut row_group_stats = Vec::with_capacity(pending.len());
        while let Some(slice) = pending.pop_front() {
            let ordinal =
                u16::try_from(first_ordinal.saturating_add(artifacts.len())).map_err(|_| {
                    ScribeError::Internal {
                        detail: "writer-v2 artifact ordinal exceeds u16".to_owned(),
                    }
                })?;
            match self.encode_artifact(sorted_batch, slice, ordinal)? {
                ArtifactEncodingOutcome::Accepted(artifact) => {
                    row_group_stats.extend(artifact.row_group_stats.iter().cloned());
                    artifacts.push(artifact);
                }
                ArtifactEncodingOutcome::Bisected { left, right } => {
                    pending.push_front(right);
                    pending.push_front(left);
                }
            }
        }
        Ok((artifacts, row_group_stats))
    }

    /// Encodes and inspects one candidate artifact under the retained footer owner.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::Internal`] for scratch IO, Parquet encoding,
    /// footer validation, checksum, or unsplittable oversize failures.
    fn encode_artifact(
        &self,
        sorted_batch: &RecordBatch,
        slice: BoundedRowSlice,
        ordinal: u16,
    ) -> Result<ArtifactEncodingOutcome, ScribeError> {
        let object_identity = format!("{}-{ordinal:05}.parquet", self.object_base);
        let artifact_batch = sorted_batch.slice(slice.offset, slice.len);
        let metadata =
            BifrostParquetMemoryEnvelope::metadata_for_batch(&artifact_batch, &object_identity)
                .map_err(|detail| ScribeError::Internal { detail })?;
        let scratch_path = self
            .scratch_dir
            .join(format!("artifact-{ordinal:05}.parquet"));
        let file = std::fs::File::create(&scratch_path).map_err(|error| ScribeError::Internal {
            detail: format!("create writer-v2 scratch artifact: {error}"),
        })?;
        let mut writer = ArrowWriter::try_new(
            CappedScratchWriter::new(file),
            artifact_batch.schema(),
            Some(bifrost_writer_properties_with_metadata(
                artifact_batch.num_rows(),
                metadata,
                self.layout.bloom_columns(),
            )),
        )
        .map_err(|error| ScribeError::Internal {
            detail: format!("create writer-v2 Parquet encoder: {error}"),
        })?;
        writer
            .write(&artifact_batch)
            .map_err(|error| ScribeError::Internal {
                detail: format!("write writer-v2 Parquet row group: {error}"),
            })?;
        writer.close().map_err(|error| ScribeError::Internal {
            detail: format!("seal writer-v2 Parquet artifact: {error}"),
        })?;
        let file_size = std::fs::metadata(&scratch_path)
            .map_err(|error| ScribeError::Internal {
                detail: format!("stat writer-v2 scratch artifact: {error}"),
            })?
            .len();
        if file_size == 0 || file_size > MAX_FILE_BYTES {
            return Err(ScribeError::Internal {
                detail: "writer-v2 sealed artifact violates its file ceiling".to_owned(),
            });
        }
        match inspect_sealed_artifact(
            &scratch_path,
            artifact_batch.schema().as_ref(),
            &object_identity,
        )? {
            SealedArtifactInspection::Accepted(stats) => {
                Ok(ArtifactEncodingOutcome::Accepted(BoundedParquetArtifact {
                    ordinal,
                    scratch_path: scratch_path.clone(),
                    object_identity,
                    file_size,
                    checksum: checksum_file(&scratch_path)?,
                    row_count: artifact_batch.num_rows(),
                    row_group_stats: stats,
                }))
            }
            SealedArtifactInspection::OversizedRowGroup => {
                std::fs::remove_file(&scratch_path).map_err(|error| ScribeError::Internal {
                    detail: format!("remove oversized writer-v2 scratch artifact: {error}"),
                })?;
                if slice.len == 1 {
                    return Err(ScribeError::Internal {
                        detail: "one-row writer-v2 artifact exceeds the encoded 32 MiB ceiling"
                            .to_owned(),
                    });
                }
                let (left, right) = BifrostArrowLogicalSizer::bisect(sorted_batch, slice)
                    .map_err(|detail| ScribeError::Internal { detail })?;
                Ok(ArtifactEncodingOutcome::Bisected { left, right })
            }
        }
    }
}

fn stamp_tenant(
    batch: &RecordBatch,
    seal_tenant: DataTenantId,
) -> Result<RecordBatch, ScribeError> {
    let schema = batch.schema();
    let tenant_idx = schema
        .index_of(DATA_TENANT_ID)
        .map_err(|_| ScribeError::Internal {
            detail: format!("{DATA_TENANT_ID} column not found after system-column construction"),
        })?;
    let tenant_array = batch
        .column(tenant_idx)
        .as_any()
        .downcast_ref::<StringArray>()
        .ok_or_else(|| ScribeError::Internal {
            detail: format!("{DATA_TENANT_ID} column must be Utf8"),
        })?;
    let expected = seal_tenant.to_string();

    for row_index in 0..tenant_array.len() {
        if !tenant_array.is_null(row_index) {
            let observed = tenant_array.value(row_index);
            if observed != expected {
                return Err(ScribeError::Internal {
                    detail: format!(
                        "{DATA_TENANT_ID} mismatch at row {row_index}: expected `{expected}`, observed `{observed}`"
                    ),
                });
            }
        }
    }

    let mut fields: Vec<_> = schema.fields.iter().cloned().collect();
    fields[tenant_idx] = Arc::new(
        fields[tenant_idx]
            .as_ref()
            .clone()
            .with_data_type(DataType::Utf8)
            .with_nullable(false),
    );
    let stamped_schema = Arc::new(Schema {
        fields: fields.into(),
        metadata: schema.metadata.clone(),
    });
    let mut columns = batch.columns().to_vec();
    columns[tenant_idx] = Arc::new(StringArray::from(vec![expected; batch.num_rows()]));

    RecordBatch::try_new(stamped_schema, columns).map_err(|error| ScribeError::Internal {
        detail: format!("failed to stamp {DATA_TENANT_ID}: {error}"),
    })
}

/// Sort a `RecordBatch` by the table's registered physical sort order.
///
/// The canonical order always begins with the injected `data_tenant_id`
/// ascending nulls-last prefix and continues with the table's declared keys in
/// declaration order, so the rows Scribe writes match the sort order stamped on
/// the destination Iceberg table.
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

/// Extract row-group statistics from encoded Parquet bytes.
///
/// # Errors
/// Returns [`ScribeError::Internal`] if Parquet metadata parsing fails.
enum SealedArtifactInspection {
    /// The footer satisfies every writer-v2 bound and carries these statistics.
    Accepted(Vec<RowGroupStats>),
    /// At least one encoded row group exceeds the 32 MiB ceiling.
    OversizedRowGroup,
}

/// Inspects one sealed artifact and distinguishes the sole retryable overflow.
///
/// # Errors
/// Returns an internal persistence error for malformed metadata, envelope, or
/// structural limits other than the encoded row-group ceiling.
fn inspect_sealed_artifact(
    path: &Path,
    expected_schema: &Schema,
    expected_object_identity: &str,
) -> Result<SealedArtifactInspection, ScribeError> {
    use parquet::file::reader::{FileReader, SerializedFileReader};

    let mut file = std::fs::File::open(path).map_err(|error| ScribeError::Internal {
        detail: format!("open sealed writer-v2 artifact: {error}"),
    })?;
    let file_bytes = file
        .metadata()
        .map_err(|error| ScribeError::Internal {
            detail: format!("stat sealed writer-v2 artifact: {error}"),
        })?
        .len();
    if file_bytes < 8 {
        return Err(ScribeError::Internal {
            detail: "sealed writer-v2 artifact is shorter than its trailer".to_owned(),
        });
    }
    let mut trailer = [0_u8; 8];
    file.seek(std::io::SeekFrom::Start(file_bytes - 8))
        .and_then(|_| file.read_exact(&mut trailer))
        .map_err(|error| ScribeError::Internal {
            detail: format!("read sealed writer-v2 trailer: {error}"),
        })?;
    if &trailer[4..] != b"PAR1" {
        return Err(ScribeError::Internal {
            detail: "sealed writer-v2 trailer magic is invalid".to_owned(),
        });
    }
    let footer_bytes = u64::from(u32::from_le_bytes(
        trailer[..4]
            .try_into()
            .expect("four-byte Scribe footer length is exact"),
    ));
    crate::parquet::memory::validate_encoded_footer_bytes(footer_bytes)
        .map_err(|detail| ScribeError::Internal { detail })?;
    let footer_start = file_bytes
        .checked_sub(8)
        .and_then(|end| end.checked_sub(footer_bytes))
        .ok_or_else(|| ScribeError::Internal {
            detail: "sealed writer-v2 footer extends before the file start".to_owned(),
        })?;
    let mut encoded_footer =
        vec![
            0_u8;
            usize::try_from(footer_bytes).map_err(|_| ScribeError::Internal {
                detail: "sealed writer-v2 footer exceeds address space".to_owned(),
            })?
        ];
    file.seek(std::io::SeekFrom::Start(footer_start))
        .and_then(|_| file.read_exact(&mut encoded_footer))
        .map_err(|error| ScribeError::Internal {
            detail: format!("read sealed writer-v2 footer preflight bytes: {error}"),
        })?;
    crate::parquet::footer_preflight::preflight_compact_thrift(&encoded_footer)
        .map_err(|detail| ScribeError::Internal { detail })?;
    file.rewind().map_err(|error| ScribeError::Internal {
        detail: format!("rewind sealed writer-v2 artifact: {error}"),
    })?;
    let reader = SerializedFileReader::new(file).map_err(|e| ScribeError::Internal {
        detail: format!("failed to parse Parquet metadata: {e}"),
    })?;

    let metadata = reader.metadata();
    BifrostParquetMemoryEnvelope::from_footer(
        metadata.file_metadata(),
        expected_schema,
        expected_object_identity,
    )
    .map_err(|detail| ScribeError::Internal { detail })?;
    if metadata.row_groups().iter().any(|group| {
        u64::try_from(group.compressed_size()).unwrap_or(u64::MAX) > MAX_LOGICAL_ROW_GROUP_BYTES
    }) {
        return Ok(SealedArtifactInspection::OversizedRowGroup);
    }
    validate_writer_v2_structure(metadata).map_err(|detail| ScribeError::Internal { detail })?;
    let mut stats = Vec::new();

    for rg in metadata.row_groups() {
        stats.push(extract_row_group_time_range(rg)?);
    }

    Ok(SealedArtifactInspection::Accepted(stats))
}

/// Computes the required object checksum without retaining the file in memory.
///
/// # Errors
/// Returns an internal persistence error when the sealed artifact cannot be read.
fn checksum_file(path: &Path) -> Result<String, ScribeError> {
    let file = std::fs::File::open(path).map_err(|error| ScribeError::Internal {
        detail: format!("open writer-v2 artifact for checksum: {error}"),
    })?;
    let mut reader = BufReader::new(file);
    let mut hasher = Sha256::new();
    let mut chunk = vec![0_u8; 8 * 1024 * 1024];
    loop {
        let read = reader
            .read(&mut chunk)
            .map_err(|error| ScribeError::Internal {
                detail: format!("read writer-v2 artifact for checksum: {error}"),
            })?;
        if read == 0 {
            return Ok(hex::encode(hasher.finalize()));
        }
        hasher.update(&chunk[..read]);
    }
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
        assert_eq!(
            crate::parquet::writer_properties::PARQUET_WRITE_BATCH_ROWS,
            8_192
        );
        assert_eq!(crate::parquet::memory::MAX_ROW_GROUP_ROWS, 128 * 1024);
        assert_eq!(
            largest_candidate_bytes_from_facts([
                (60 * 1024, 60),
                (40 * 1024, 40),
                (1, 7),
                (120 * 1024, 120),
                (2, 9),
            ])
            .expect("candidate peak"),
            120,
            "admission reserves the complete largest grouped candidate"
        );
    }

    /// Builds the canonical hourly write recipe for a test schema.
    ///
    /// Tests exercise the same resolution path production uses: declare the
    /// built-in hourly layout, then let `PhysicalLayout::resolve` union the
    /// managed Bloom floor.
    ///
    /// # Panics
    /// Panics when the schema cannot carry the built-in declaration.
    fn test_layout(schema: &Schema) -> PhysicalLayout {
        PhysicalLayout::resolve(
            "vala.bifrost.test",
            schema,
            Some(&crate::tables::hourly_layout(
                vec![crate::tables::sort_asc("wyrd_event_time")],
                &[],
            )),
        )
        .expect("test schema carries the built-in hourly layout")
    }

    fn build_test_frozen(
        seal_partition: TimePartition,
        seal_tenant: DataTenantId,
        tenant_ids: Vec<&str>,
        timestamps: Vec<i64>,
    ) -> FrozenMemtable {
        let schema = Arc::new(Schema::new(vec![
            Field::new("data_tenant_id", DataType::Utf8, false),
            Field::new(
                "wyrd_event_time",
                DataType::Timestamp(TimeUnit::Microsecond, None),
                false,
            ),
        ]));

        let batch = RecordBatch::try_new(
            schema.clone(),
            vec![
                Arc::new(StringArray::from(tenant_ids)),
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
            events: vec![],
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
            crate::scribe::memory::EncodedFooterReservation::for_test(),
        )
        .expect("writer-v2 encode");
        (scratch, encoded)
    }

    /// Builds one whole stored batch retained as one candidate input boundary.
    fn whole_member_batch(tenant: &str, first_timestamp: i64, rows: usize) -> RecordBatch {
        let row_count = i64::try_from(rows).expect("test row count fits i64");
        RecordBatch::try_new(
            Arc::new(Schema::new(vec![
                Field::new("data_tenant_id", DataType::Utf8, false),
                Field::new(
                    "wyrd_event_time",
                    DataType::Timestamp(TimeUnit::Microsecond, None),
                    false,
                ),
            ])),
            vec![
                Arc::new(StringArray::from(vec![tenant; rows])),
                Arc::new(TimestampMicrosecondArray::from_iter_values(
                    first_timestamp..first_timestamp + row_count,
                )),
            ],
        )
        .expect("whole member batch")
    }

    /// Proves forced selective seal publishes multiple candidates as one
    /// contiguous deterministic artifact set.
    #[test]
    fn forced_seal_multi_candidate_member_has_one_deterministic_artifact_set() {
        let tenant = DataTenantId::new_v7();
        let tenant_string = tenant.to_string();
        let day = crate::test_support::day_partition(2026, 7, 14);
        let schema = Arc::new(Schema::new(vec![
            Field::new("data_tenant_id", DataType::Utf8, false),
            Field::new(
                "wyrd_event_time",
                DataType::Timestamp(TimeUnit::Microsecond, None),
                false,
            ),
        ]));
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
                whole_member_batch(&tenant_string, 0, 60 * 1024),
                whole_member_batch(&tenant_string, 60 * 1024, 50 * 1024),
            ],
            events: vec![],
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

    /// Runs one production candidate encode while retaining its scratch owner.
    fn encode_serial_candidate_for_test(
        frozen: &FrozenMemtable,
        binding: &TenantTableBinding,
        tenant: DataTenantId,
        batch_index: usize,
        first_ordinal: usize,
    ) -> (tempfile::TempDir, Result<ParquetEncoded, ScribeError>) {
        let scratch = tempfile::tempdir().expect("candidate scratch");
        let layout = test_layout(frozen.schema.as_ref());
        let encoded = encode_candidate(
            CandidateEncodeRequest {
                frozen,
                binding,
                seal_tenant: tenant,
                candidate: FileCandidate {
                    start: batch_index,
                    end: batch_index + 1,
                    rows: frozen.batches[batch_index].num_rows(),
                },
                first_ordinal,
                scratch_dir: scratch.path(),
                object_base: "tenant/table/member",
                layout: &layout,
            },
            crate::scribe::memory::EncodedFooterReservation::for_test(),
        );
        (scratch, encoded)
    }

    /// Reads one encoded candidate and proves its tenant and timestamp order.
    fn assert_serial_candidate(encoded: &ParquetEncoded, tenant: &str, expected_times: &[i64]) {
        let file =
            std::fs::File::open(&encoded.artifacts[0].scratch_path).expect("candidate artifact");
        let mut reader = ParquetRecordBatchReaderBuilder::try_new(file)
            .expect("candidate parquet")
            .build()
            .expect("candidate reader");
        let batch = reader
            .next()
            .expect("candidate row group")
            .expect("candidate batch");
        let times = batch
            .column_by_name("wyrd_event_time")
            .expect("event time")
            .as_any()
            .downcast_ref::<TimestampMicrosecondArray>()
            .expect("timestamp column")
            .values();
        let tenants = batch
            .column_by_name(DATA_TENANT_ID)
            .expect("tenant column")
            .as_any()
            .downcast_ref::<StringArray>()
            .expect("string tenant");
        assert_eq!(times, expected_times);
        assert!((0..tenants.len()).all(|index| tenants.value(index) == tenant));
    }

    /// Candidate-local production encoding sorts every iteration, stamps the
    /// authenticated tenant, preserves generation-global ordinals, and refuses
    /// a mismatched tenant before materialization.
    #[test]
    fn serial_candidate_encoding_preserves_sort_tenant_and_global_identity() {
        let tenant = DataTenantId::new_v7();
        let tenant_string = tenant.to_string();
        let day = crate::test_support::day_partition(2026, 7, 14);
        let schema = Arc::new(Schema::new(vec![
            Field::new("data_tenant_id", DataType::Utf8, false),
            Field::new(
                "wyrd_event_time",
                DataType::Timestamp(TimeUnit::Microsecond, None),
                false,
            ),
        ]));
        let stored_batch = |timestamps: Vec<i64>| {
            RecordBatch::try_new(
                Arc::clone(&schema),
                vec![
                    Arc::new(StringArray::from(vec![
                        tenant_string.as_str();
                        timestamps.len()
                    ])),
                    Arc::new(TimestampMicrosecondArray::from(timestamps)),
                ],
            )
            .expect("candidate batch")
        };
        let frozen = FrozenMemtable {
            seal_id: 42,
            seal_key: SealKey::new(
                tenant,
                TableRef::new(BifrostNamespace::Bifrost, "serial_candidates"),
                day,
            ),
            shard_id: 4,
            schema: Arc::clone(&schema),
            batches: vec![stored_batch(vec![40, 10]), stored_batch(vec![30, 20])],
            events: vec![],
            metas: vec![],
            opened_at: std::time::Instant::now(),
            closed_at: std::time::Instant::now(),
            arrow_bytes: 0,
        };
        let binding = TenantTableBinding::resolve((tenant, frozen.seal_key.table.clone()))
            .expect("tenant binding");
        let (_first_scratch, first) =
            encode_serial_candidate_for_test(&frozen, &binding, tenant, 0, 0);
        let first = first.expect("first candidate");
        let (_second_scratch, second) =
            encode_serial_candidate_for_test(&frozen, &binding, tenant, 1, first.artifacts.len());
        let second = second.expect("second candidate");
        assert_eq!(first.artifacts[0].ordinal, 0);
        assert_eq!(second.artifacts[0].ordinal, 1);
        assert!(
            first.artifacts[0]
                .object_identity
                .ends_with("-00000.parquet")
        );
        assert!(
            second.artifacts[0]
                .object_identity
                .ends_with("-00001.parquet")
        );
        assert_serial_candidate(&first, &tenant_string, &[10, 40]);
        assert_serial_candidate(&second, &tenant_string, &[20, 30]);
        let (_mismatch_scratch, mismatch) =
            encode_serial_candidate_for_test(&frozen, &binding, DataTenantId::new_v7(), 0, 0);
        let error = mismatch.expect_err("mismatched tenant must fail closed");
        assert!(
            matches!(error, ScribeError::Internal { detail } if detail.contains("tenant-table binding mismatch"))
        );
    }

    #[test]
    fn parquet_writer_partition_matches_seal_key() {
        // Regression test for C2: partition = seal_key.partition, not row min/max
        let seal_partition = crate::test_support::day_partition(2026, 7, 14);
        let tenant = DataTenantId::new_v7();
        let tenant_string = tenant.to_string();
        let frozen = build_test_frozen(
            seal_partition,
            tenant,
            vec![tenant_string.as_str(), tenant_string.as_str()],
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
        let tenant_string = tenant.to_string();

        let frozen = build_test_frozen(
            crate::test_support::day_partition(2026, 7, 14),
            tenant,
            vec![
                tenant_string.as_str(),
                tenant_string.as_str(),
                tenant_string.as_str(),
                tenant_string.as_str(),
            ],
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

        let tenant_col = batch
            .column_by_name("data_tenant_id")
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

        // Every row is stamped with the seal tenant; timestamps are the second sort key.
        assert_eq!(tenant_col.value(0), tenant_string);
        assert_eq!(time_col.value(0), 100);

        assert_eq!(tenant_col.value(1), tenant_string);
        assert_eq!(time_col.value(1), 200);

        assert_eq!(tenant_col.value(2), tenant_string);
        assert_eq!(time_col.value(2), 300);

        assert_eq!(tenant_col.value(3), tenant_string);
        assert_eq!(time_col.value(3), 400);
    }

    #[test]
    fn parquet_writer_writes_bloom_and_page_index() {
        let tenant = DataTenantId::new_v7();
        let tenant_string = tenant.to_string();
        let frozen = build_test_frozen(
            crate::test_support::day_partition(2026, 7, 14),
            tenant,
            vec![tenant_string.as_str(), tenant_string.as_str()],
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
        let tenant_col = rg
            .columns()
            .iter()
            .find(|column| column.column_descr().name() == DATA_TENANT_ID)
            .unwrap();
        assert!(
            tenant_col.bloom_filter_offset().is_some(),
            "allowlisted tenant column must have a bloom filter"
        );

        let col = rg
            .columns()
            .iter()
            .find(|column| column.column_descr().name() == "wyrd_event_time")
            .unwrap();

        // Page index presence is indicated by offset_index_offset being set
        assert!(
            col.offset_index_offset().is_some() || col.column_index_offset().is_some(),
            "page index metadata not found"
        );
    }

    #[test]
    fn parquet_writer_returns_envelopes_unmodified() {
        // Encoding does not touch either the `AuditEvent` list or `ScribeAppendMeta` list
        let tenant = DataTenantId::new_v7();
        let tenant_string = tenant.to_string();
        let frozen = build_test_frozen(
            crate::test_support::day_partition(2026, 7, 14),
            tenant,
            vec![tenant_string.as_str()],
            vec![1_000_000],
        );

        let event = AuditEvent {
            request_id: wyrd_spec::request_id::RequestId::now_v7(),
            trace_id: None,
            operation: "test".to_string(),
            resource: "test".to_string(),
            card_ref: None,
            principal_id: wyrd_spec::auth::PrincipalId::new(uuid::Uuid::now_v7()),
            principal_kind: wyrd_spec::auth::PrincipalKindTag::Service,
            auth_method: wyrd_spec::vala::api::AuthMethod::Jwt,
            permission: "test".to_string(),
            decision: wyrd_spec::vala::api::AuditDecision::Allow,
            result: wyrd_spec::vala::api::AuditResult::Success,
            payload_summary: "test".to_string(),
            detail: None,
        };

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
        frozen_with_envelopes.events = vec![event.clone()];
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

        assert_eq!(encoded.audit_events.len(), 1);
        assert_eq!(encoded.append_metas.len(), 1);

        // Verify pairing preserved (index i ↔ index i)
        assert_eq!(encoded.audit_events[0].request_id, event.request_id);
        assert_eq!(encoded.append_metas[0].batch_id, meta.batch_id);
    }

    #[test]
    fn sort_batch_uses_one_key_for_every_table() {
        let schema = Arc::new(Schema::new(vec![
            arrow::datatypes::Field::new(DATA_TENANT_ID, DataType::Utf8, false),
            arrow::datatypes::Field::new(
                "wyrd_event_time",
                DataType::Timestamp(arrow::datatypes::TimeUnit::Microsecond, None),
                false,
            ),
        ]));
        let batch = RecordBatch::try_new(
            schema,
            vec![
                Arc::new(StringArray::from(vec!["tenant-b", "tenant-a", "tenant-a"])),
                Arc::new(TimestampMicrosecondArray::from(vec![3, 2, 1])),
            ],
        )
        .unwrap();

        let layout = test_layout(batch.schema().as_ref());
        let sorted = sort_batch(&batch, &layout).unwrap();
        let tenants = sorted
            .column_by_name(DATA_TENANT_ID)
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
        assert_eq!((tenants.value(0), times.value(0)), ("tenant-a", 1));
        assert_eq!((tenants.value(1), times.value(1)), ("tenant-a", 2));
        assert_eq!((tenants.value(2), times.value(2)), ("tenant-b", 3));
    }

    #[test]
    fn missing_tenant_column_fails_closed() {
        let tenant = DataTenantId::new_v7();
        let schema = Arc::new(Schema::new(vec![arrow::datatypes::Field::new(
            "wyrd_event_time",
            DataType::Timestamp(arrow::datatypes::TimeUnit::Microsecond, None),
            false,
        )]));
        let batch = RecordBatch::try_new(
            schema.clone(),
            vec![Arc::new(TimestampMicrosecondArray::from(vec![1]))],
        )
        .unwrap();
        let frozen = FrozenMemtable {
            seal_id: 0,
            seal_key: SealKey::new(
                tenant,
                TableRef::new(BifrostNamespace::Bifrost, "events"),
                crate::test_support::day_partition(2026, 7, 14),
            ),
            shard_id: 0,
            schema,
            batches: vec![batch],
            events: vec![],
            metas: vec![],
            opened_at: std::time::Instant::now(),
            closed_at: std::time::Instant::now(),
            arrow_bytes: 0,
        };
        let binding = TenantTableBinding::resolve((tenant, frozen.seal_key.table.clone())).unwrap();

        let scratch = tempfile::tempdir().unwrap();
        let layout = test_layout(frozen.schema.as_ref());
        let error = encode_batch(
            &frozen,
            &binding,
            tenant,
            scratch.path(),
            "object",
            &layout,
            crate::scribe::memory::EncodedFooterReservation::for_test(),
        )
        .unwrap_err();
        assert!(
            matches!(error, ScribeError::Internal { detail } if detail.contains(DATA_TENANT_ID))
        );
    }

    #[test]
    fn mismatched_tenant_value_fails_closed() {
        let expected = DataTenantId::new_v7();
        let observed = DataTenantId::new_v7();
        let expected_string = expected.to_string();
        let observed_string = observed.to_string();
        let frozen = build_test_frozen(
            crate::test_support::day_partition(2026, 7, 14),
            expected,
            vec![expected_string.as_str(), observed_string.as_str()],
            vec![1, 2],
        );
        let binding =
            TenantTableBinding::resolve((expected, frozen.seal_key.table.clone())).unwrap();

        let scratch = tempfile::tempdir().unwrap();
        let layout = test_layout(frozen.schema.as_ref());
        let error = encode_batch(
            &frozen,
            &binding,
            expected,
            scratch.path(),
            "object",
            &layout,
            crate::scribe::memory::EncodedFooterReservation::for_test(),
        )
        .unwrap_err();
        assert!(matches!(error, ScribeError::Internal { detail }
            if detail.contains("row 1")
                && detail.contains(&expected.to_string())
                && detail.contains(&observed.to_string())));
    }

    #[test]
    fn tenant_binding_mismatch_fails_before_encoding() {
        let seal_tenant = DataTenantId::new_v7();
        let binding_tenant = DataTenantId::new_v7();
        let frozen = build_test_frozen(
            crate::test_support::day_partition(2026, 7, 14),
            seal_tenant,
            vec![seal_tenant.to_string().as_str()],
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
            crate::scribe::memory::EncodedFooterReservation::for_test(),
        )
        .unwrap_err();
        assert!(
            matches!(error, ScribeError::Internal { detail } if detail.contains("binding mismatch"))
        );
    }
}
