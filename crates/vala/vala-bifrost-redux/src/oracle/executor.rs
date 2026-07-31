//! Shared sealed-fragment validation and execution for local and peer attempts.

use std::fmt;
use std::fs::File;
use std::path::{Component, Path};
use std::pin::Pin;
use std::sync::Arc;

use arrow::array::{
    ArrayRef, BooleanArray, Float64Array, Int64Array, Scalar, StringArray,
    TimestampMicrosecondArray, UInt64Array,
};
use arrow::compute::kernels::boolean::{and, is_not_null, is_null};
use arrow::compute::kernels::cmp;
use arrow::compute::kernels::filter::filter_record_batch;
use arrow::datatypes::{DataType, Schema, SchemaRef, TimeUnit};
use arrow::ipc::writer::StreamWriter;
use arrow::record_batch::RecordBatch;
use chrono::Utc;
use futures_util::{Stream, StreamExt};
use iceberg::arrow::ArrowFileReader;
use iceberg::io::FileIO;
use parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder;
use parquet::arrow::async_reader::ParquetRecordBatchStreamBuilder;
use sha2::{Digest, Sha256};
use thiserror::Error;
use wyrd_spec::vala::api::{QueryAuditDigest, WorkerAttemptFrame, WorkerFooter};

use super::fragment::{ClosedLeafPredicate, LeafComparison, LeafScalar, SealedScanFragment};
use crate::schema::SchemaFingerprint;
use crate::scribe::memory::BifrostMemoryGovernor;

/// Executor validation or sealed-read failure.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum ExecutorError {
    /// Fragment identity does not match its closed fields.
    #[error("sealed fragment digest mismatch")]
    Digest,
    /// Fragment has no immutable object work.
    #[error("sealed fragment has no files")]
    Empty,
    /// The hard attempt deadline elapsed before object access or decode.
    #[error("sealed fragment deadline elapsed")]
    Deadline,
    /// An object path escapes the authenticated tenant-qualified binding.
    #[error("sealed fragment path escapes its binding")]
    Path,
    /// An object did not match its pinned byte size.
    #[error("sealed fragment object size mismatch")]
    Size,
    /// The decoded or projected Arrow schema differs from the pinned fingerprint.
    #[error("sealed fragment schema mismatch")]
    Schema,
    /// The configured storage backend could not read an immutable object.
    #[error("sealed fragment storage read failed")]
    Storage,
    /// Parquet decode, projection, or Arrow encoding failed.
    #[error("sealed fragment decode failed")]
    Decode,
    /// The shared Bifrost parent rejected the worker's bounded decode reservation.
    #[error("sealed fragment memory budget unavailable")]
    Capacity,
    /// The fragment contains a leaf predicate not supported by this closed executor.
    #[error("sealed fragment predicate is unsupported")]
    Predicate,
}

/// Incremental worker frame stream; dropping it cancels reads and releases memory.
pub type WorkerFrameStream =
    Pin<Box<dyn Stream<Item = Result<WorkerAttemptFrame, ExecutorError>> + Send>>;

/// Shared executor used by local and remote adapters.
#[derive(Clone, Default)]
pub struct SealedFragmentExecutor {
    /// Optional object-store reader inherited from a pinned Iceberg table.
    file_io: Option<FileIO>,
    /// Optional process-wide governor required by production worker construction.
    memory_governor: Option<BifrostMemoryGovernor>,
}

impl fmt::Debug for SealedFragmentExecutor {
    /// Redacts backend configuration while showing whether remote IO is available.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SealedFragmentExecutor")
            .field("object_store_configured", &self.file_io.is_some())
            .field("memory_governed", &self.memory_governor.is_some())
            .finish()
    }
}

impl SealedFragmentExecutor {
    /// Creates an executor using the pinned table's storage backend.
    #[must_use]
    pub fn new(file_io: FileIO) -> Self {
        Self {
            file_io: Some(file_io),
            memory_governor: None,
        }
    }

    /// Creates a production executor with ranged object reads and parent memory admission.
    #[must_use]
    pub fn with_memory_governor(file_io: FileIO, memory_governor: BifrostMemoryGovernor) -> Self {
        Self {
            file_io: Some(file_io),
            memory_governor: Some(memory_governor),
        }
    }

    /// Validates every closed field required before object-store or Parquet access.
    ///
    /// # Errors
    /// Returns a closed [`ExecutorError`] for malformed, expired, or escaping work.
    pub fn validate(&self, fragment: &SealedScanFragment) -> Result<(), ExecutorError> {
        if fragment.files.is_empty() {
            return Err(ExecutorError::Empty);
        }
        if fragment.fragment_id != fragment.digest() {
            return Err(ExecutorError::Digest);
        }
        if fragment.deadline_unix_ms <= Utc::now().timestamp_millis() {
            return Err(ExecutorError::Deadline);
        }
        if fragment.binding.is_empty()
            || fragment
                .files
                .iter()
                .any(|file| !binding_contains(&fragment.binding, &file.location))
        {
            return Err(ExecutorError::Path);
        }
        let bytes = fragment
            .files
            .iter()
            .try_fold(0_u64, |total, file| total.checked_add(file.size_bytes))
            .ok_or(ExecutorError::Size)?;
        let rows = fragment
            .files
            .iter()
            .try_fold(0_u64, |total, file| total.checked_add(file.estimated_rows))
            .ok_or(ExecutorError::Size)?;
        if bytes != fragment.estimated_bytes || rows != fragment.estimated_rows {
            return Err(ExecutorError::Size);
        }
        Ok(())
    }

    /// Opens an incremental immutable fragment attempt.
    ///
    /// Production remote reads use Parquet's async ranged reader over Iceberg
    /// `FileIO`. Each projected batch is encoded and yielded before the next
    /// batch is requested. The returned stream owns the worker memory
    /// reservation, so cancellation releases capacity and stops further reads.
    ///
    /// # Errors
    /// Returns a closed validation or memory-admission error before any IO.
    /// Storage, decode, schema, size, and deadline errors are emitted by the
    /// stream because they can arise after earlier frames were transported.
    pub fn execute(
        &self,
        fragment: &SealedScanFragment,
    ) -> Result<WorkerFrameStream, ExecutorError> {
        self.validate(fragment)?;
        let memory_reservation = self
            .memory_governor
            .as_ref()
            .map(|governor| {
                let estimate = usize::try_from(fragment.estimated_bytes)
                    .map_err(|_| ExecutorError::Capacity)?;
                governor
                    .try_reserve_parent(estimate)
                    .map_err(|_| ExecutorError::Capacity)
            })
            .transpose()?;
        let fragment = fragment.clone();
        let file_io = self.file_io.clone();
        let output = async_stream::try_stream! {
            let _memory_reservation = memory_reservation;
            let mut encoder = AttemptEncoder::default();
            for file in &fragment.files {
                if fragment.deadline_unix_ms <= Utc::now().timestamp_millis() {
                    Err(ExecutorError::Deadline)?;
                }
                let selected_groups = file
                    .row_groups
                    .iter()
                    .copied()
                    .map(|group| usize::try_from(group).map_err(|_| ExecutorError::Decode))
                    .collect::<Result<Vec<_>, _>>()?;
                if let Some(file_io) = &file_io {
                    let input = file_io
                        .new_input(&file.location)
                        .map_err(|_| ExecutorError::Storage)?;
                    let metadata = input.metadata().await.map_err(|_| ExecutorError::Storage)?;
                    if metadata.size != file.size_bytes {
                        Err(ExecutorError::Size)?;
                    }
                    let reader = input.reader().await.map_err(|_| ExecutorError::Storage)?;
                    let arrow_reader = ArrowFileReader::new(metadata, reader);
                    let mut builder = ParquetRecordBatchStreamBuilder::new(arrow_reader)
                        .await
                        .map_err(|_| ExecutorError::Decode)?;
                    if !selected_groups.is_empty() {
                        builder = builder.with_row_groups(selected_groups);
                    }
                    let mut batches = builder
                        .with_batch_size(8_192)
                        .build()
                        .map_err(|_| ExecutorError::Decode)?;
                    while let Some(batch) = batches.next().await {
                        let batch = prepare_batch(
                            batch.map_err(|_| ExecutorError::Decode)?,
                            &fragment,
                        )?;
                        let (schema, batch) = encoder.encode(batch)?;
                        if let Some(schema) = schema {
                            yield schema;
                        }
                        yield batch;
                    }
                } else {
                    let metadata =
                        std::fs::metadata(&file.location).map_err(|_| ExecutorError::Storage)?;
                    if metadata.len() != file.size_bytes {
                        Err(ExecutorError::Size)?;
                    }
                    let mut builder = ParquetRecordBatchReaderBuilder::try_new(
                        File::open(&file.location).map_err(|_| ExecutorError::Storage)?,
                    )
                    .map_err(|_| ExecutorError::Decode)?;
                    if !selected_groups.is_empty() {
                        builder = builder.with_row_groups(selected_groups);
                    }
                    let batches = builder
                        .with_batch_size(8_192)
                        .build()
                        .map_err(|_| ExecutorError::Decode)?;
                    for batch in batches {
                        let batch = prepare_batch(
                            batch.map_err(|_| ExecutorError::Decode)?,
                            &fragment,
                        )?;
                        let (schema, batch) = encoder.encode(batch)?;
                        if let Some(schema) = schema {
                            yield schema;
                        }
                        yield batch;
                    }
                }
            }
            yield encoder.finish(&fragment)?;
        };
        Ok(Box::pin(output))
    }

    /// Executes one file from a fragment for narrow filesystem tests.
    ///
    /// # Errors
    /// Returns a closed validation, storage, or Parquet decode failure.
    pub fn execute_local_file(
        &self,
        fragment: &SealedScanFragment,
        file_index: usize,
    ) -> Result<Vec<RecordBatch>, ExecutorError> {
        self.validate(fragment)?;
        let file = fragment.files.get(file_index).ok_or(ExecutorError::Empty)?;
        let groups = file
            .row_groups
            .iter()
            .copied()
            .map(|group| usize::try_from(group).map_err(|_| ExecutorError::Decode))
            .collect::<Result<Vec<_>, _>>()?;
        let mut builder = ParquetRecordBatchReaderBuilder::try_new(
            File::open(&file.location).map_err(|_| ExecutorError::Storage)?,
        )
        .map_err(|_| ExecutorError::Decode)?;
        if !groups.is_empty() {
            builder = builder.with_row_groups(groups);
        }
        builder
            .build()
            .map_err(|_| ExecutorError::Decode)?
            .map(|batch| {
                let batch = apply_predicates(
                    batch.map_err(|_| ExecutorError::Decode)?,
                    &fragment.predicates,
                )?;
                project(batch, &fragment.projection)
            })
            .collect()
    }
}

/// Verifies one object location remains inside its authenticated binding.
///
/// URI bindings compare the normalized scheme and exact authority before
/// segment containment. Filesystem bindings reject dot components and compare
/// parsed path components. Percent escapes and backslashes are rejected so the
/// storage backend cannot reinterpret a value after this check.
fn binding_contains(binding: &str, location: &str) -> bool {
    if has_unsafe_path_syntax(binding) || has_unsafe_path_syntax(location) {
        return false;
    }
    match (split_uri(binding), split_uri(location)) {
        (
            Some((binding_scheme, binding_authority, binding_path)),
            Some((location_scheme, location_authority, location_path)),
        ) => {
            binding_scheme.eq_ignore_ascii_case(location_scheme)
                && binding_authority == location_authority
                && path_segments_contain(binding_path, location_path)
        }
        (None, None) if !binding.contains("://") && !location.contains("://") => {
            filesystem_path_contains(binding, location)
        }
        _ => false,
    }
}

/// Detects syntax that a downstream URI or storage parser could reinterpret.
fn has_unsafe_path_syntax(value: &str) -> bool {
    value
        .chars()
        .any(|character| matches!(character, '?' | '#' | '\\' | '\0' | '%'))
}

/// Splits a canonical authority-bearing URI without decoding its path.
fn split_uri(value: &str) -> Option<(&str, &str, &str)> {
    let (scheme, remainder) = value.split_once("://")?;
    if scheme.is_empty()
        || !scheme.bytes().enumerate().all(|(index, byte)| {
            byte.is_ascii_alphabetic()
                || (index > 0 && matches!(byte, b'+' | b'-' | b'.' | b'0'..=b'9'))
        })
    {
        return None;
    }
    let (authority, path) = remainder
        .split_once('/')
        .map_or((remainder, ""), |(authority, path)| (authority, path));
    if authority.contains('@') || (authority.is_empty() && !scheme.eq_ignore_ascii_case("file")) {
        return None;
    }
    Some((scheme, authority, path))
}

/// Checks URI path containment on canonical non-dot path segments.
fn path_segments_contain(binding: &str, location: &str) -> bool {
    let binding = canonical_segments(binding);
    let location = canonical_segments(location);
    matches!((binding, location), (Some(binding), Some(location)) if !binding.is_empty() && location.starts_with(&binding) && location.len() > binding.len())
}

/// Returns canonical path segments, rejecting empty and dot components.
fn canonical_segments(path: &str) -> Option<Vec<&str>> {
    let path = path.trim_matches('/');
    if path.is_empty() {
        return Some(Vec::new());
    }
    path.split('/')
        .map(|segment| {
            (!segment.is_empty() && segment != "." && segment != "..").then_some(segment)
        })
        .collect()
}

/// Checks filesystem containment without consulting or canonicalizing the filesystem.
fn filesystem_path_contains(binding: &str, location: &str) -> bool {
    let binding = Path::new(binding);
    let location = Path::new(location);
    if binding.is_absolute() != location.is_absolute()
        || binding
            .components()
            .chain(location.components())
            .any(|component| matches!(component, Component::CurDir | Component::ParentDir))
    {
        return false;
    }
    let binding_components = binding.components().collect::<Vec<_>>();
    let location_components = location.components().collect::<Vec<_>>();
    !binding_components.is_empty()
        && location_components.starts_with(&binding_components)
        && location_components.len() > binding_components.len()
}

/// Evaluates the conjunction of closed typed leaf predicates.
///
/// # Errors
/// Returns [`ExecutorError::Predicate`] for absent columns, type mismatch, or kernel failure.
fn apply_predicates(
    batch: RecordBatch,
    predicates: &[ClosedLeafPredicate],
) -> Result<RecordBatch, ExecutorError> {
    let mut combined: Option<BooleanArray> = None;
    for predicate in predicates {
        let mask = match predicate {
            ClosedLeafPredicate::IsNull { column } => {
                let array = predicate_column(&batch, column)?;
                is_null(array.as_ref()).map_err(|_| ExecutorError::Predicate)?
            }
            ClosedLeafPredicate::IsNotNull { column } => {
                let array = predicate_column(&batch, column)?;
                is_not_null(array.as_ref()).map_err(|_| ExecutorError::Predicate)?
            }
            ClosedLeafPredicate::Compare {
                column,
                operation,
                value,
            } => {
                let array = predicate_column(&batch, column)?;
                let scalar = predicate_scalar(value, array.data_type())?;
                match operation {
                    LeafComparison::Eq => cmp::eq(&array, &scalar),
                    LeafComparison::NotEq => cmp::neq(&array, &scalar),
                    LeafComparison::Lt => cmp::lt(&array, &scalar),
                    LeafComparison::LtEq => cmp::lt_eq(&array, &scalar),
                    LeafComparison::Gt => cmp::gt(&array, &scalar),
                    LeafComparison::GtEq => cmp::gt_eq(&array, &scalar),
                }
                .map_err(|_| ExecutorError::Predicate)?
            }
        };
        combined = Some(match combined {
            Some(current) => and(&current, &mask).map_err(|_| ExecutorError::Predicate)?,
            None => mask,
        });
    }
    match combined {
        Some(mask) => filter_record_batch(&batch, &mask).map_err(|_| ExecutorError::Predicate),
        None => Ok(batch),
    }
}

/// Finds one authorized predicate column by exact physical name.
///
/// # Errors
/// Returns [`ExecutorError::Predicate`] when the column is absent.
fn predicate_column(batch: &RecordBatch, name: &str) -> Result<ArrayRef, ExecutorError> {
    let index = batch
        .schema()
        .index_of(name)
        .map_err(|_| ExecutorError::Predicate)?;
    Ok(Arc::clone(batch.column(index)))
}

/// Creates one exact Arrow scalar matching the predicate column type.
///
/// # Errors
/// Returns [`ExecutorError::Predicate`] when the literal and column types differ.
fn predicate_scalar(
    value: &LeafScalar,
    data_type: &DataType,
) -> Result<Scalar<ArrayRef>, ExecutorError> {
    let array: ArrayRef = match (value, data_type) {
        (LeafScalar::Boolean(value), DataType::Boolean) => {
            Arc::new(BooleanArray::from(vec![*value]))
        }
        (LeafScalar::Int64(value), DataType::Int64) => Arc::new(Int64Array::from(vec![*value])),
        (LeafScalar::UInt64(value), DataType::UInt64) => Arc::new(UInt64Array::from(vec![*value])),
        (LeafScalar::Float64Bits(value), DataType::Float64) => {
            Arc::new(Float64Array::from(vec![f64::from_bits(*value)]))
        }
        (LeafScalar::Utf8(value), DataType::Utf8) => {
            Arc::new(StringArray::from(vec![value.as_str()]))
        }
        (LeafScalar::TimestampMicros(value), DataType::Timestamp(TimeUnit::Microsecond, None)) => {
            Arc::new(TimestampMicrosecondArray::from(vec![*value]))
        }
        _ => return Err(ExecutorError::Predicate),
    };
    Ok(Scalar::new(array))
}

/// Applies the authorized name-based projection without positional assumptions.
///
/// # Errors
/// Returns [`ExecutorError::Schema`] when a projected column is absent.
fn project(batch: RecordBatch, projection: &[String]) -> Result<RecordBatch, ExecutorError> {
    if projection.is_empty() {
        return Ok(batch);
    }
    let schema = batch.schema();
    let indexes = projection
        .iter()
        .map(|name| schema.index_of(name).map_err(|_| ExecutorError::Schema))
        .collect::<Result<Vec<_>, _>>()?;
    let fields = indexes
        .iter()
        .map(|index| Arc::clone(&schema.fields()[*index]))
        .collect::<Vec<_>>();
    let columns = indexes
        .iter()
        .map(|index| Arc::clone(batch.column(*index)))
        .collect::<Vec<_>>();
    RecordBatch::try_new(Arc::new(Schema::new(fields)), columns).map_err(|_| ExecutorError::Decode)
}

/// Applies the fragment's closed predicates, projection, and pinned schema check.
///
/// # Errors
/// Returns a closed predicate, projection, or schema failure.
fn prepare_batch(
    batch: RecordBatch,
    fragment: &SealedScanFragment,
) -> Result<RecordBatch, ExecutorError> {
    let batch = apply_predicates(batch, &fragment.predicates)?;
    let batch = project(batch, &fragment.projection)?;
    let actual = hex::encode(SchemaFingerprint::from_arrow_schema(&batch.schema()));
    if actual != fragment.schema_fingerprint {
        return Err(ExecutorError::Schema);
    }
    Ok(batch)
}

/// Stateful footer encoder for one incremental worker attempt.
#[derive(Debug, Default)]
struct AttemptEncoder {
    /// Schema emitted exactly once before the first batch.
    schema: Option<SchemaRef>,
    /// Incremental hash over encoded batch payloads in stream order.
    payload_hash: Sha256,
    /// Total encoded schema and batch bytes.
    encoded_bytes: usize,
    /// Total rows emitted across batch frames.
    row_count: u64,
}

impl AttemptEncoder {
    /// Encodes one batch and returns its optional first-schema frame plus batch frame.
    ///
    /// # Errors
    ///
    /// Returns [`ExecutorError::Decode`] when Arrow IPC encoding or checked
    /// counters fail, and [`ExecutorError::Schema`] if later batches change schema.
    fn encode(
        &mut self,
        batch: RecordBatch,
    ) -> Result<(Option<WorkerAttemptFrame>, WorkerAttemptFrame), ExecutorError> {
        let schema_frame = if let Some(schema) = &self.schema {
            if schema.as_ref() != batch.schema().as_ref() {
                return Err(ExecutorError::Schema);
            }
            None
        } else {
            let schema = batch.schema();
            let mut bytes = Vec::new();
            StreamWriter::try_new(&mut bytes, &schema)
                .and_then(|mut writer| writer.finish())
                .map_err(|_| ExecutorError::Decode)?;
            self.encoded_bytes = bytes.len();
            self.schema = Some(schema);
            Some(WorkerAttemptFrame::Schema(bytes))
        };
        let mut bytes = Vec::new();
        StreamWriter::try_new(&mut bytes, &batch.schema())
            .and_then(|mut writer| {
                writer.write(&batch)?;
                writer.finish()
            })
            .map_err(|_| ExecutorError::Decode)?;
        self.payload_hash.update(&bytes);
        self.encoded_bytes = self
            .encoded_bytes
            .checked_add(bytes.len())
            .ok_or(ExecutorError::Decode)?;
        self.row_count = self
            .row_count
            .checked_add(u64::try_from(batch.num_rows()).map_err(|_| ExecutorError::Decode)?)
            .ok_or(ExecutorError::Decode)?;
        Ok((schema_frame, WorkerAttemptFrame::Batch(bytes)))
    }

    /// Finalizes a non-empty attempt into its footer-last protocol frame.
    ///
    /// # Errors
    ///
    /// Returns a closed empty, digest, or checked byte-conversion failure.
    fn finish(self, fragment: &SealedScanFragment) -> Result<WorkerAttemptFrame, ExecutorError> {
        if self.schema.is_none() {
            return Err(ExecutorError::Empty);
        }
        let manifest_digest = QueryAuditDigest::new(fragment.pinned_digest.clone())
            .map_err(|_| ExecutorError::Digest)?;
        let payload_digest = QueryAuditDigest::new(hex::encode(self.payload_hash.finalize()))
            .map_err(|_| ExecutorError::Digest)?;
        Ok(WorkerAttemptFrame::Footer(WorkerFooter {
            fragment_id: fragment.fragment_id.clone(),
            manifest_digest,
            row_count: self.row_count,
            encoded_bytes: u64::try_from(self.encoded_bytes).map_err(|_| ExecutorError::Decode)?,
            payload_digest,
            completed: true,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use arrow::datatypes::Field;

    /// Closed predicates filter before projection without admitting SQL text.
    #[test]
    fn oracle_executor_evaluates_closed_leaf_predicates() {
        let schema = Arc::new(Schema::new(vec![
            Field::new("filter_value", DataType::Int64, false),
            Field::new("projected", DataType::Utf8, false),
        ]));
        let batch = RecordBatch::try_new(
            schema,
            vec![
                Arc::new(Int64Array::from(vec![1, 2, 3])),
                Arc::new(StringArray::from(vec!["one", "two", "three"])),
            ],
        )
        .expect("test batch");
        let filtered = apply_predicates(
            batch,
            &[ClosedLeafPredicate::Compare {
                column: "filter_value".to_owned(),
                operation: LeafComparison::GtEq,
                value: LeafScalar::Int64(2),
            }],
        )
        .expect("closed predicate");
        let projected =
            project(filtered, &["projected".to_owned()]).expect("authorized projection");

        assert_eq!(projected.num_rows(), 2);
        let values = projected
            .column(0)
            .as_any()
            .downcast_ref::<StringArray>()
            .expect("utf8 projection");
        assert_eq!(values.value(0), "two");
        assert_eq!(values.value(1), "three");
    }

    /// URI binding validation rejects traversal, parser ambiguity, and authority changes.
    #[test]
    fn oracle_executor_rejects_noncanonical_or_cross_authority_paths() {
        assert!(binding_contains(
            "s3://bucket/tenants/one/table",
            "s3://bucket/tenants/one/table/data.parquet"
        ));
        assert!(!binding_contains(
            "s3://bucket/tenants/one/table",
            "s3://bucket/tenants/one/table/../two/data.parquet"
        ));
        assert!(!binding_contains(
            "s3://bucket/tenants/one/table",
            "s3://other/tenants/one/table/data.parquet"
        ));
        assert!(!binding_contains(
            "s3://bucket/tenants/one/table",
            "s3://bucket/tenants/one/table/%2e%2e/two/data.parquet"
        ));
    }

    /// Filesystem binding validation compares components without touching disk.
    #[test]
    fn oracle_executor_rejects_filesystem_dot_segments_before_io() {
        assert!(binding_contains(
            "/warehouse/tenants/one",
            "/warehouse/tenants/one/data.parquet"
        ));
        assert!(!binding_contains(
            "/warehouse/tenants/one",
            "/warehouse/tenants/one/../two/data.parquet"
        ));
        assert!(!binding_contains(
            "/warehouse/tenants/one",
            "warehouse/tenants/one/data.parquet"
        ));
    }
}
