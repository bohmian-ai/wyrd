//! Shared sealed-fragment validation and execution for local and peer attempts.

use std::fmt;
use std::fs::File;
use std::io::Read;
use std::path::{Component, Path};
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

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
use async_trait::async_trait;
use chrono::Utc;
use futures_util::{Stream, StreamExt};
use iceberg::arrow::ArrowFileReader;
use iceberg::io::{FileIO, FileRead};
use parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder;
use parquet::arrow::async_reader::ParquetRecordBatchStreamBuilder;
use parquet::file::reader::{ChunkReader, Length};
use sha2::{Digest, Sha256};
use thiserror::Error;
use wyrd_spec::vala::api::{
    QueryAuditDigest, QueryClass, WorkerAttemptFrame, WorkerFooter, WorkerScanStats,
};

use super::fragment::{
    ClosedLeafPredicate, LeafComparison, LeafScalar, SealedScanFile, SealedScanFragment,
};
use super::query_class_label;
use crate::resources::{OracleResources, OracleWorkerClass};

/// Worker-owned physical demand collected for one sealed fragment attempt.
#[derive(Debug)]
struct WorkerScanCollector {
    /// Query class used for the closed production metric label.
    query_class: QueryClass,
    /// Requested physical bytes, including requests that later fail.
    requested_bytes: AtomicU64,
    /// Files that issued at least one physical request.
    files: AtomicU64,
    /// One partition marker for the fragment when any request was issued.
    partitions: AtomicU64,
    /// Prevents duplicate emission when stream and wrappers both drop.
    finalized: AtomicBool,
}

impl WorkerScanCollector {
    /// Creates a collector whose terminal emission is owned by the attempt stream.
    fn new(query_class: QueryClass) -> Arc<Self> {
        Arc::new(Self {
            query_class,
            requested_bytes: AtomicU64::new(0),
            files: AtomicU64::new(0),
            partitions: AtomicU64::new(0),
            finalized: AtomicBool::new(false),
        })
    }

    /// Records one request immediately before the underlying read begins.
    fn record_request(&self, bytes: u64, first_file_request: bool) {
        self.requested_bytes.fetch_add(bytes, Ordering::Relaxed);
        if first_file_request {
            self.files.fetch_add(1, Ordering::Relaxed);
            self.partitions.store(1, Ordering::Release);
        }
    }

    /// Returns the collected values for focused worker tests.
    fn snapshot(&self) -> (u64, u64, u64) {
        (
            self.requested_bytes.load(Ordering::Acquire),
            self.files.load(Ordering::Acquire),
            self.partitions.load(Ordering::Acquire),
        )
    }

    /// Emits terminal counters once for normal completion and drop paths.
    fn finalize(&self) {
        if self.finalized.swap(true, Ordering::AcqRel) {
            return;
        }
        let (bytes, files, partitions) = self.snapshot();
        let class = query_class_label(self.query_class);
        metrics::counter!("oracle_query_bytes_scanned_total", "class" => class).increment(bytes);
        metrics::counter!("oracle_query_files_scanned_total", "class" => class).increment(files);
        metrics::counter!("oracle_query_partitions_scanned_total", "class" => class)
            .increment(partitions);
    }
}

impl Drop for WorkerScanCollector {
    /// Emits requested demand once, including failed, cancelled, and dropped attempts.
    fn drop(&mut self) {
        self.finalize();
    }
}

/// Per-file request marker shared by ranged readers and their returned streams.
#[derive(Debug)]
struct WorkerFileScanCollector {
    /// Attempt-level demand owner.
    attempt: Arc<WorkerScanCollector>,
    /// Ensures a file contributes one file count on its first request.
    started: AtomicBool,
}

impl WorkerFileScanCollector {
    /// Creates one file marker within an attempt collector.
    fn new(attempt: Arc<WorkerScanCollector>) -> Arc<Self> {
        Arc::new(Self {
            attempt,
            started: AtomicBool::new(false),
        })
    }

    /// Records one physical request and marks the file on its first request.
    fn record_request(&self, bytes: u64) {
        let first = !self.started.swap(true, Ordering::AcqRel);
        self.attempt.record_request(bytes, first);
    }
}

/// `FileIO` reader wrapper that records each requested range before awaiting it.
struct CountingFileRead {
    /// Underlying Iceberg reader.
    inner: Box<dyn FileRead>,
    /// Per-file demand marker.
    metrics: Arc<WorkerFileScanCollector>,
}

#[async_trait]
impl FileRead for CountingFileRead {
    /// Records the requested range before delegating to object storage.
    ///
    /// # Errors
    /// Propagates the underlying Iceberg reader's storage or cancellation error.
    async fn read(&self, range: std::ops::Range<u64>) -> iceberg::Result<bytes::Bytes> {
        self.metrics
            .record_request(range.end.saturating_sub(range.start));
        self.inner.read(range).await
    }
}

/// Local Parquet reader wrapper that records random and sequential requests.
#[derive(Debug)]
struct CountingChunkReader<R> {
    /// Underlying concrete Parquet reader.
    inner: R,
    /// Per-file demand marker.
    metrics: Arc<WorkerFileScanCollector>,
}

impl<R> Length for CountingChunkReader<R>
where
    R: Length,
{
    /// Returns the underlying file length without counting metadata access.
    fn len(&self) -> u64 {
        self.inner.len()
    }
}

impl<R> ChunkReader for CountingChunkReader<R>
where
    R: ChunkReader,
{
    type T = CountingRead<R::T>;

    /// Opens a sequential reader while retaining the per-file counter.
    ///
    /// # Errors
    /// Propagates the underlying Parquet reader's seek or storage error.
    fn get_read(&self, start: u64) -> parquet::errors::Result<Self::T> {
        let reader = self.inner.get_read(start)?;
        Ok(CountingRead {
            inner: reader,
            metrics: Arc::clone(&self.metrics),
        })
    }

    /// Records a random-access request before delegating the byte fetch.
    ///
    /// # Errors
    /// Propagates the underlying Parquet reader's random-access or storage error.
    fn get_bytes(&self, start: u64, length: usize) -> parquet::errors::Result<bytes::Bytes> {
        self.metrics
            .record_request(u64::try_from(length).unwrap_or(u64::MAX));
        self.inner.get_bytes(start, length)
    }
}

/// Read wrapper that counts each returned buffer from a local Parquet stream.
#[derive(Debug)]
struct CountingRead<R> {
    /// Underlying sequential reader.
    inner: R,
    /// Per-file demand marker.
    metrics: Arc<WorkerFileScanCollector>,
}

impl<R> Read for CountingRead<R>
where
    R: Read,
{
    /// Counts the requested buffer before delegating the read operation.
    ///
    /// # Errors
    /// Propagates the underlying filesystem or buffered-reader error.
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        self.metrics
            .record_request(u64::try_from(buffer.len()).unwrap_or(u64::MAX));
        self.inner.read(buffer)
    }
}

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
    /// A pinned immutable object disappeared after the Oracle cut was selected.
    #[error("sealed fragment object is stale")]
    StaleObject,
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

/// Incremental decoded batches for one sealed file before worker framing.
type WorkerBatchStream = Pin<Box<dyn Stream<Item = Result<RecordBatch, ExecutorError>> + Send>>;

/// Test-only factory for an existing [`FileRead`] implementation.
#[cfg(test)]
type TestFileReadFactory = Arc<dyn Fn() -> Box<dyn FileRead> + Send + Sync>;

/// Shared executor used by local and remote adapters.
#[derive(Clone, Default)]
pub struct SealedFragmentExecutor {
    /// Optional object-store reader inherited from a pinned Iceberg table.
    file_io: Option<FileIO>,
    /// Optional narrow Oracle worker issuer required by production construction.
    resources: Option<OracleResources>,
    /// Deterministic existing-interface reader used only by focused tests.
    #[cfg(test)]
    reader_override: Option<TestFileReadFactory>,
}

impl fmt::Debug for SealedFragmentExecutor {
    /// Redacts backend configuration while showing whether remote IO is available.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        #[cfg(test)]
        {
            let mut debug = formatter.debug_struct("SealedFragmentExecutor");
            debug.field("object_store_configured", &self.file_io.is_some());
            debug.field("memory_governed", &self.resources.is_some());
            debug.field(
                "reader_override_configured",
                &self.reader_override.is_some(),
            );
            debug.finish()
        }
        #[cfg(not(test))]
        {
            let mut debug = formatter.debug_struct("SealedFragmentExecutor");
            debug.field("object_store_configured", &self.file_io.is_some());
            debug.field("memory_governed", &self.resources.is_some());
            debug.finish()
        }
    }
}

impl SealedFragmentExecutor {
    /// Creates an executor using the pinned table's storage backend.
    #[must_use]
    pub fn new(file_io: FileIO) -> Self {
        Self {
            file_io: Some(file_io),
            resources: None,
            #[cfg(test)]
            reader_override: None,
        }
    }

    /// Creates a production executor with ranged object reads and parent memory admission.
    #[must_use]
    pub fn with_resources(file_io: FileIO, resources: OracleResources) -> Self {
        Self {
            file_io: Some(file_io),
            resources: Some(resources),
            #[cfg(test)]
            reader_override: None,
        }
    }

    /// Installs one deterministic existing-interface reader for focused tests.
    #[cfg(test)]
    fn with_test_reader(mut self, reader: TestFileReadFactory) -> Self {
        self.reader_override = Some(reader);
        self
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

    /// Opens one remote Parquet file as a counted asynchronous batch stream.
    ///
    /// Metadata validation remains before the first physical read. The returned
    /// stream owns the reader and its per-file collector, so dropping it retains
    /// any request recorded immediately before a failed or cancelled read.
    ///
    /// # Errors
    /// Returns storage, size, or Parquet decode errors before the batch stream
    /// is returned; subsequent read failures are yielded by that stream.
    async fn open_remote_batches(
        &self,
        file: &SealedScanFile,
        selected_groups: Vec<usize>,
        collector: Arc<WorkerScanCollector>,
    ) -> Result<WorkerBatchStream, ExecutorError> {
        #[cfg(test)]
        if let Some(factory) = &self.reader_override {
            let reader = CountingFileRead {
                inner: factory(),
                metrics: WorkerFileScanCollector::new(collector),
            };
            let bytes = reader
                .read(0..file.size_bytes)
                .await
                .map_err(|error| classify_storage_error(&error))?;
            if u64::try_from(bytes.len()).map_err(|_| ExecutorError::Size)? != file.size_bytes {
                return Err(ExecutorError::Size);
            }
            let mut builder = ParquetRecordBatchReaderBuilder::try_new(bytes)
                .map_err(|_| ExecutorError::Decode)?;
            if !selected_groups.is_empty() {
                builder = builder.with_row_groups(selected_groups);
            }
            let batches = builder
                .with_batch_size(8_192)
                .build()
                .map_err(|_| ExecutorError::Decode)?;
            let stream = futures_util::stream::iter(
                batches.map(|batch| batch.map_err(|_| ExecutorError::Decode)),
            );
            return Ok(Box::pin(stream));
        }
        let file_io = self.file_io.as_ref().ok_or(ExecutorError::Storage)?;
        let input = file_io
            .new_input(&file.location)
            .map_err(|error| classify_storage_error(&error))?;
        let metadata = input
            .metadata()
            .await
            .map_err(|error| classify_storage_error(&error))?;
        if metadata.size != file.size_bytes {
            return Err(ExecutorError::Size);
        }
        let reader = input
            .reader()
            .await
            .map_err(|error| classify_storage_error(&error))?;
        let arrow_reader = ArrowFileReader::new(
            metadata,
            Box::new(CountingFileRead {
                inner: reader,
                metrics: WorkerFileScanCollector::new(collector),
            }),
        );
        let mut builder = ParquetRecordBatchStreamBuilder::new(arrow_reader)
            .await
            .map_err(|error| classify_decode_error(&error))?;
        if !selected_groups.is_empty() {
            builder = builder.with_row_groups(selected_groups);
        }
        let mut batches = builder
            .with_batch_size(8_192)
            .build()
            .map_err(|_| ExecutorError::Decode)?;
        let stream = async_stream::try_stream! {
            while let Some(batch) = batches.next().await {
                yield batch.map_err(|error| classify_decode_error(&error))?;
            }
        };
        Ok(Box::pin(stream))
    }

    /// Opens one local Parquet file as a counted synchronous batch stream.
    ///
    /// The concrete chunk reader records each requested buffer before delegating
    /// to the filesystem. The iterator is boxed only at this file boundary so
    /// the outer attempt preserves incremental framing and cancellation.
    ///
    /// # Errors
    /// Returns storage, size, or Parquet decode errors before the batch stream
    /// is returned.
    fn open_local_batches(
        file: &SealedScanFile,
        selected_groups: Vec<usize>,
        collector: Arc<WorkerScanCollector>,
    ) -> Result<WorkerBatchStream, ExecutorError> {
        let metadata =
            std::fs::metadata(&file.location).map_err(|error| classify_storage_error(&error))?;
        if metadata.len() != file.size_bytes {
            return Err(ExecutorError::Size);
        }
        let reader = CountingChunkReader {
            inner: File::open(&file.location).map_err(|error| classify_storage_error(&error))?,
            metrics: WorkerFileScanCollector::new(collector),
        };
        let mut builder =
            ParquetRecordBatchReaderBuilder::try_new(reader).map_err(|_| ExecutorError::Decode)?;
        if !selected_groups.is_empty() {
            builder = builder.with_row_groups(selected_groups);
        }
        let batches = builder
            .with_batch_size(8_192)
            .build()
            .map_err(|_| ExecutorError::Decode)?;
        let stream = futures_util::stream::iter(
            batches.map(|batch| batch.map_err(|_| ExecutorError::Decode)),
        );
        Ok(Box::pin(stream))
    }

    /// Opens one sealed file through the configured remote or local backend.
    ///
    /// # Errors
    /// Propagates the selected backend's storage, size, and decode errors.
    async fn open_file_batches(
        &self,
        file: &SealedScanFile,
        selected_groups: Vec<usize>,
        collector: Arc<WorkerScanCollector>,
    ) -> Result<WorkerBatchStream, ExecutorError> {
        if self.file_io.is_some() {
            self.open_remote_batches(file, selected_groups, collector)
                .await
        } else {
            Self::open_local_batches(file, selected_groups, collector)
        }
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
        query_class: QueryClass,
    ) -> Result<WorkerFrameStream, ExecutorError> {
        self.validate(fragment)?;
        let worker_resources = self
            .resources
            .as_ref()
            .map(|resources| {
                let class = match query_class {
                    QueryClass::Interactive => OracleWorkerClass::Interactive,
                    QueryClass::Analytical => OracleWorkerClass::Analytical,
                };
                resources
                    .try_acquire_worker(class)
                    .map_err(|_| ExecutorError::Capacity)
            })
            .transpose()?;
        Ok(self.execute_validated(fragment, query_class, worker_resources))
    }

    /// Builds the lazy attempt stream after validation and capacity selection.
    ///
    /// `worker_resources` is present when the executor owns a configured root
    /// resource capability and remains retained for the complete stream.
    fn execute_validated(
        &self,
        fragment: &SealedScanFragment,
        query_class: QueryClass,
        worker_resources: Option<crate::resources::OracleWorkerResources>,
    ) -> WorkerFrameStream {
        let fragment = fragment.clone();
        let executor = self.clone();
        let metrics = WorkerScanCollector::new(query_class);
        let output = async_stream::try_stream! {
            let _worker_resources = worker_resources;
            let attempt_metrics = metrics;
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
                let mut batches = executor
                    .open_file_batches(file, selected_groups, Arc::clone(&attempt_metrics))
                    .await?;
                while let Some(batch) = batches.next().await {
                    let batch = prepare_batch(batch?, &fragment)?;
                    let (schema, batch) = encoder.encode(&batch)?;
                    if let Some(schema) = schema {
                        yield schema;
                    }
                    yield batch;
                }
            }
            yield encoder.finish(&fragment)?;
        };
        Box::pin(output)
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
            File::open(&file.location).map_err(|error| classify_storage_error(&error))?,
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

/// Classifies a sealed-object access error without treating outages as stale metadata.
fn classify_storage_error(error: &(dyn std::error::Error + 'static)) -> ExecutorError {
    if error_chain_contains_not_found(error) {
        ExecutorError::StaleObject
    } else {
        ExecutorError::Storage
    }
}

/// Preserves an object-not-found race surfaced through Parquet's reader error chain.
fn classify_decode_error(error: &(dyn std::error::Error + 'static)) -> ExecutorError {
    if error_chain_contains_not_found(error) {
        ExecutorError::StaleObject
    } else {
        ExecutorError::Decode
    }
}

/// Returns whether an error chain contains an exact filesystem or `OpenDAL` not-found cause.
pub(super) fn error_chain_contains_not_found(error: &(dyn std::error::Error + 'static)) -> bool {
    let mut current = Some(error);
    while let Some(source) = current {
        if source
            .downcast_ref::<std::io::Error>()
            .is_some_and(|error| error.kind() == std::io::ErrorKind::NotFound)
            || source
                .downcast_ref::<opendal::Error>()
                .is_some_and(|error| error.kind() == opendal::ErrorKind::NotFound)
        {
            return true;
        }
        current = source.source();
    }
    false
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
    let actual = super::assignment_schema_fingerprint(&batch.schema());
    if actual != fragment.schema_fingerprint {
        return Err(ExecutorError::Schema);
    }
    Ok(batch)
}

/// Stateful footer encoder for one incremental worker attempt.
#[derive(Debug, Default)]
pub struct AttemptEncoder {
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
    /// Starts an attempt with its immutable output schema, including empty results.
    ///
    /// # Errors
    /// Returns [`ExecutorError::Decode`] when Arrow IPC schema encoding fails.
    pub fn start(&mut self, schema: SchemaRef) -> Result<WorkerAttemptFrame, ExecutorError> {
        let mut bytes = Vec::new();
        StreamWriter::try_new(&mut bytes, &schema)
            .and_then(|mut writer| writer.finish())
            .map_err(|_| ExecutorError::Decode)?;
        self.encoded_bytes = bytes.len();
        self.schema = Some(schema);
        Ok(WorkerAttemptFrame::Schema(bytes))
    }

    /// Encodes one batch and returns its optional first-schema frame plus batch frame.
    ///
    /// # Errors
    ///
    /// Returns [`ExecutorError::Decode`] when Arrow IPC encoding or checked
    /// counters fail, and [`ExecutorError::Schema`] if later batches change schema.
    pub fn encode(
        &mut self,
        batch: &RecordBatch,
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
                writer.write(batch)?;
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
            // The sealed-fragment protocol carries no executed physical plan of
            // its own, so it reports no scan evidence rather than a fabricated
            // zero-byte scan.
            scan_stats: WorkerScanStats::default(),
        }))
    }

    /// Finalizes a native physical-plan attempt under its immutable fingerprint.
    ///
    /// `scan_stats` is the follower's finalized physical read volume, which the
    /// leader sums across the participant cut because its own plan scans no
    /// storage.
    ///
    /// # Errors
    /// Returns a closed empty, digest, or checked byte-conversion failure.
    pub fn finish_physical(
        self,
        plan_fingerprint: &str,
        scan_stats: WorkerScanStats,
    ) -> Result<WorkerAttemptFrame, ExecutorError> {
        if self.schema.is_none() {
            return Err(ExecutorError::Empty);
        }
        let digest = QueryAuditDigest::new(plan_fingerprint.to_owned())
            .map_err(|_| ExecutorError::Digest)?;
        let payload_digest = QueryAuditDigest::new(hex::encode(self.payload_hash.finalize()))
            .map_err(|_| ExecutorError::Digest)?;
        Ok(WorkerAttemptFrame::Footer(WorkerFooter {
            fragment_id: plan_fingerprint.to_owned(),
            manifest_digest: digest,
            row_count: self.row_count,
            encoded_bytes: u64::try_from(self.encoded_bytes).map_err(|_| ExecutorError::Decode)?,
            payload_digest,
            completed: true,
            scan_stats,
        }))
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;
    use arrow::datatypes::Field;
    use parquet::arrow::ArrowWriter;
    use tempfile::NamedTempFile;
    use tokio::sync::Notify;

    use crate::oracle::exec::OracleQueryScanStats;

    /// Deterministic existing-reader behavior used to drive wrapper outcomes.
    #[derive(Clone)]
    enum DeterministicRead {
        /// Return an exact byte payload immediately.
        Success(bytes::Bytes),
        /// Return one immediate Iceberg storage error.
        Error,
        /// Remain pending until the consumer drops the read future.
        Pending(Arc<Notify>),
    }

    /// Test-only `FileRead` implementation for causal wrapper assertions.
    struct DeterministicFileRead {
        /// Outcome returned by every requested range.
        outcome: DeterministicRead,
    }

    #[async_trait]
    impl FileRead for DeterministicFileRead {
        /// Returns the configured result after the caller has registered demand.
        async fn read(&self, _range: std::ops::Range<u64>) -> iceberg::Result<bytes::Bytes> {
            match &self.outcome {
                DeterministicRead::Success(bytes) => Ok(bytes.clone()),
                DeterministicRead::Error => Err(iceberg::Error::new(
                    iceberg::ErrorKind::Unexpected,
                    "deterministic reader failure",
                )),
                DeterministicRead::Pending(notify) => {
                    notify.notified().await;
                    Err(iceberg::Error::new(
                        iceberg::ErrorKind::Unexpected,
                        "pending reader released",
                    ))
                }
            }
        }
    }

    /// Wraps a deterministic read outcome in the executor's existing reader interface.
    fn deterministic_reader_factory(outcome: DeterministicRead) -> TestFileReadFactory {
        Arc::new(move || {
            Box::new(DeterministicFileRead {
                outcome: outcome.clone(),
            })
        })
    }

    /// Write one temporary Parquet object and return its pinned scan metadata.
    fn local_parquet_fixture() -> (NamedTempFile, SealedScanFile, SchemaRef) {
        let mut file = NamedTempFile::new().expect("temporary Parquet file");
        let schema = Arc::new(Schema::new(vec![Field::new(
            "value",
            DataType::Int64,
            false,
        )]));
        let batch = RecordBatch::try_new(
            Arc::clone(&schema),
            vec![Arc::new(Int64Array::from(vec![1, 2, 3]))],
        )
        .expect("fixture batch matches schema");
        let mut writer = ArrowWriter::try_new(file.as_file_mut(), Arc::clone(&schema), None)
            .expect("Parquet writer");
        writer.write(&batch).expect("Parquet batch");
        writer.close().expect("Parquet close");
        let size_bytes = file.as_file().metadata().expect("file metadata").len();
        let scan_file = SealedScanFile {
            location: file.path().to_string_lossy().into_owned(),
            row_groups: vec![0],
            size_bytes,
            estimated_rows: 3,
        };
        (file, scan_file, schema)
    }

    /// Builds one validated fragment while allowing a deterministic request size.
    fn fragment_for_file(
        scan_file: &SealedScanFile,
        schema: &SchemaRef,
        size_bytes: u64,
    ) -> SealedScanFragment {
        let mut fragment = SealedScanFragment {
            fragment_id: String::new(),
            binding: Path::new(&scan_file.location)
                .parent()
                .expect("fixture parent")
                .to_string_lossy()
                .into_owned(),
            tier: crate::oracle::fragment::SealedSourceTier::Iceberg,
            pinned_digest: "test-digest".to_owned(),
            files: vec![SealedScanFile {
                location: scan_file.location.clone(),
                row_groups: scan_file.row_groups.clone(),
                size_bytes,
                estimated_rows: scan_file.estimated_rows,
            }],
            projection: vec!["value".to_owned()],
            predicates: Vec::new(),
            schema_fingerprint: crate::oracle::assignment_schema_fingerprint(schema),
            estimated_rows: scan_file.estimated_rows,
            estimated_bytes: size_bytes,
            deadline_unix_ms: Utc::now().timestamp_millis() + 60_000,
        };
        fragment.fragment_id = fragment.digest();
        fragment
    }

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

    /// Missing pinned objects are the only local storage failures classified as stale.
    #[test]
    fn oracle_executor_distinguishes_stale_objects_from_storage_outages() {
        let stale = std::io::Error::from(std::io::ErrorKind::NotFound);
        let outage = std::io::Error::from(std::io::ErrorKind::PermissionDenied);

        assert_eq!(classify_storage_error(&stale), ExecutorError::StaleObject);
        assert_eq!(classify_storage_error(&outage), ExecutorError::Storage);
    }

    /// Production worker wrappers retain success, error, pending-drop, retry,
    /// class, and exact requested-demand evidence through the recorder.
    #[tokio::test]
    async fn worker_wrappers_record_causal_outcomes_once() {
        let (_file, scan_file, schema) = local_parquet_fixture();
        let requested = scan_file.size_bytes;
        let success_fragment = fragment_for_file(&scan_file, &schema, requested);
        let failed_fragment = fragment_for_file(&scan_file, &schema, 7);
        let pending_fragment = fragment_for_file(&scan_file, &schema, 11);
        let executor = SealedFragmentExecutor::new(FileIO::new_with_fs());
        let recorder = wyrd_bench::BenchmarkRecorder::default();
        let _guard = metrics::set_default_local_recorder(&recorder);
        let notify = Arc::new(Notify::new());

        validate_worker_success_and_leader_conversion(
            &executor,
            &success_fragment,
            &scan_file,
            &schema,
            &recorder,
        )
        .await;

        let failed_executor = executor
            .clone()
            .with_test_reader(deterministic_reader_factory(DeterministicRead::Error));
        let mut failed_stream = failed_executor
            .execute(&failed_fragment, QueryClass::Interactive)
            .expect("failed execution");
        assert!(failed_stream.next().await.expect("failed frame").is_err());
        drop(failed_stream);

        let pending_executor = executor
            .clone()
            .with_test_reader(deterministic_reader_factory(DeterministicRead::Pending(
                Arc::clone(&notify),
            )));
        let mut pending_stream = pending_executor
            .execute(&pending_fragment, QueryClass::Interactive)
            .expect("pending execution");
        assert!(
            tokio::time::timeout(Duration::from_millis(10), pending_stream.next())
                .await
                .is_err()
        );
        drop(pending_stream);
        drop(notify);

        for _attempt in 0..2 {
            let retry_executor = executor
                .clone()
                .with_test_reader(deterministic_reader_factory(DeterministicRead::Success(
                    bytes::Bytes::from(
                        std::fs::read(&scan_file.location).expect("retry Parquet bytes"),
                    ),
                )));
            let mut retry_stream = retry_executor
                .execute(&success_fragment, QueryClass::Interactive)
                .expect("retry execution");
            while let Some(frame) = retry_stream.next().await {
                frame.expect("retry worker frame");
            }
            drop(retry_stream);
        }

        let snapshot = recorder.snapshot();
        let expected_bytes = requested.saturating_mul(3).saturating_add(18);
        assert_eq!(
            snapshot
                .counters
                .get("oracle_query_bytes_scanned_total{class=\"interactive\"}"),
            Some(&expected_bytes)
        );
        assert_eq!(
            snapshot
                .counters
                .get("oracle_query_files_scanned_total{class=\"interactive\"}"),
            Some(&5)
        );
        assert_eq!(
            snapshot
                .counters
                .get("oracle_query_partitions_scanned_total{class=\"interactive\"}"),
            Some(&5)
        );
    }

    /// Executes a valid worker attempt and converts its validated batches to
    /// the leader memory source without recording a second physical read.
    async fn validate_worker_success_and_leader_conversion(
        executor: &SealedFragmentExecutor,
        fragment: &SealedScanFragment,
        scan_file: &SealedScanFile,
        schema: &SchemaRef,
        recorder: &wyrd_bench::BenchmarkRecorder,
    ) {
        let success_executor = executor
            .clone()
            .with_test_reader(deterministic_reader_factory(DeterministicRead::Success(
                bytes::Bytes::from(
                    std::fs::read(&scan_file.location).expect("success Parquet bytes"),
                ),
            )));
        let mut success_stream = success_executor
            .execute(fragment, QueryClass::Interactive)
            .expect("success execution");
        let mut success_frames = Vec::new();
        while let Some(frame) = success_stream.next().await {
            success_frames.push(frame.expect("success worker frame"));
        }
        drop(success_stream);
        assert!(
            success_frames
                .iter()
                .any(|frame| matches!(frame, WorkerAttemptFrame::Footer(_)))
        );
        let before_conversion = recorder.snapshot().counters.clone();
        let mut attempt_buffer = crate::oracle::attempt::AttemptBuffer::new(1 << 20);
        for frame in success_frames {
            attempt_buffer.push(frame).expect("worker frame validates");
        }
        let validated = attempt_buffer.finish().expect("worker attempt validates");
        let mut validated_batches = Vec::new();
        crate::oracle::decode_attempt_batches(validated, &mut validated_batches)
            .expect("validated batches decode");
        let leader_source =
            crate::oracle::test_validated_memory_source(&validated_batches, Arc::clone(schema))
                .expect("leader memory conversion");
        let after_conversion = recorder.snapshot().counters;
        assert_eq!(after_conversion, before_conversion);
        let leader_stats = OracleQueryScanStats::from_plan(leader_source.as_ref(), 0);
        assert_eq!(
            (
                leader_stats.physical_bytes_scanned,
                leader_stats.files_scanned,
                leader_stats.partitions_scanned
            ),
            (None, 0, 0)
        );
    }

    /// A real local Parquet read records exact bytes, one file, one partition, and class.
    #[tokio::test]
    async fn sealed_executor_local_read_projects_source_demand() {
        let (_file, scan_file, _schema) = local_parquet_fixture();
        let collector = WorkerScanCollector::new(QueryClass::Analytical);
        let executor = SealedFragmentExecutor::default();
        let mut batches = executor
            .open_file_batches(&scan_file, vec![0], Arc::clone(&collector))
            .await
            .expect("local source opens");
        let mut rows = 0;
        while let Some(batch) = batches.next().await {
            rows += batch.expect("local source decodes").num_rows();
        }
        assert_eq!(rows, 3);
        let (bytes, files, partitions) = collector.snapshot();
        assert!(bytes > 0);
        assert_eq!((files, partitions), (1, 1));
        assert_eq!(collector.query_class, QueryClass::Analytical);
    }

    /// Production local reader wrappers retain drop/cancel demand and emit one
    /// class-labelled terminal sample for each retry without a leader duplicate.
    #[tokio::test]
    async fn sealed_executor_wrappers_cover_drop_retry_and_exactly_once() {
        let (_file, scan_file, _schema) = local_parquet_fixture();
        let recorder = wyrd_bench::BenchmarkRecorder::default();
        let executor = SealedFragmentExecutor::default();
        let _guard = metrics::set_default_local_recorder(&recorder);
        for _retry in 0..2 {
            let collector = WorkerScanCollector::new(QueryClass::Interactive);
            let mut batches = executor
                .open_file_batches(&scan_file, vec![0], Arc::clone(&collector))
                .await
                .expect("local source opens through production wrappers");
            let _ = batches.next().await;
            drop(collector);
            drop(batches);
        }
        let snapshot = recorder.snapshot();
        let sample = snapshot
            .counters
            .get("oracle_query_bytes_scanned_total{class=\"interactive\"}")
            .expect("dropped wrapper emits requested bytes");
        assert!(*sample > 0);
        assert_eq!(
            snapshot
                .counters
                .get("oracle_query_files_scanned_total{class=\"interactive\"}"),
            Some(&2)
        );
        assert_eq!(
            snapshot
                .counters
                .get("oracle_query_partitions_scanned_total{class=\"interactive\"}"),
            Some(&2)
        );
    }
}
