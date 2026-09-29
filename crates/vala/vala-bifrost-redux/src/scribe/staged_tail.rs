//! Lazy live-tail reads over durable staged runs.
//!
//! Once a generation's rows are staged, its Arrow is gone: the runs on the
//! staging volume are the only local copy until the claim that owns them
//! publishes. A live-tail reader must therefore be able to read Parquet, but it
//! reads through Parquet's async stream one row group and one decoded batch per
//! pull under the caller's projection and signed predicate, so a staged member
//! that is large by construction never moves into memory whole; the consumer's
//! pull rate bounds what is resident.
//!
//! Memory is charged to the follower's query grant, not to a private ceiling:
//! a row group's projected encoded bytes are reserved before they are fetched
//! and held until the group is done, and each decoded batch is charged while it
//! is retained. The accounting is best effort — Parquet's decoder allocates
//! beyond what it reports — but it refuses a read the grant plainly cannot
//! hold before touching the file.
//!
//! The reader decides nothing about authority. It is handed the staged sources
//! a [`ScribeHotSourceRegistry`](crate::scribe::hot_source::ScribeHotSourceRegistry)
//! resolved and returns rows from exactly those.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use arrow::array::RecordBatch;
use arrow::datatypes::{Schema, SchemaRef};
use datafusion::execution::memory_pool::{MemoryConsumer, MemoryPool, MemoryReservation};
use futures_util::StreamExt;
use parquet::arrow::arrow_reader::{ArrowReaderMetadata, ArrowReaderOptions};
use parquet::arrow::async_reader::ParquetRecordBatchStream;
use parquet::arrow::{ParquetRecordBatchStreamBuilder, ProjectionMask};
use parquet::file::metadata::RowGroupMetaData;
use parquet::schema::types::SchemaDescriptor;
use wyrd_spec::vala::assignment_authority::ScanPredicate;

use crate::contracts::ScribeError;
use crate::oracle::exec::ScanPredicateFilter;

/// Rows decoded per Parquet batch yielded to a live producer.
///
/// Small enough that one resident batch stays modest however large the member
/// is, large enough that a normal read is not dominated by per-batch overhead.
const STAGED_READ_BATCH_ROWS: usize = 8 * 1024;

/// Opens durable staged runs as grant-accounted async Arrow batch streams.
#[derive(Debug, Clone, Copy)]
pub(crate) struct StagedTailReader {
    /// Rows decoded per batch yielded to the live producer.
    batch_rows: usize,
}

impl Default for StagedTailReader {
    /// Builds a reader with the module's output batch size.
    fn default() -> Self {
        Self {
            batch_rows: STAGED_READ_BATCH_ROWS,
        }
    }
}

impl StagedTailReader {
    /// Opens one staged run for a live read charged to `memory_pool`.
    ///
    /// Reads only the footer, asynchronously, and projects the caller's
    /// required columns by name. No row group is fetched until the returned
    /// run is pulled; each is then reserved against `memory_pool` first.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::Internal`] when the run cannot be opened, its
    /// footer is unreadable, or a required column is absent.
    pub(crate) async fn open(
        self,
        run: PathBuf,
        required_columns: Arc<[String]>,
        predicates: Arc<[ScanPredicate]>,
        memory_pool: &Arc<dyn MemoryPool>,
    ) -> Result<StagedRun, ScribeError> {
        let mut file = tokio::fs::File::open(&run)
            .await
            .map_err(run_failure(&run, "open"))?;
        let metadata = ArrowReaderMetadata::load_async(&mut file, ArrowReaderOptions::new())
            .await
            .map_err(run_failure(&run, "read the metadata of"))?;
        let schema = Arc::clone(metadata.schema());
        let projection =
            projection_mask(&schema, metadata.parquet_schema(), &required_columns, &run)?;
        Ok(StagedRun {
            file,
            metadata,
            projection,
            batch_rows: self.batch_rows,
            next_group: 0,
            group: None,
            fetched: MemoryConsumer::new("scribe-staged-row-group").register(memory_pool),
            decoded: MemoryConsumer::new("scribe-staged-decoded-batch").register(memory_pool),
            schema,
            required_columns,
            predicates,
            filter: None,
            run,
        })
    }
}

/// One opened staged run producing signed, projected rows one batch per pull.
///
/// Owns the open file, the current row group's async Parquet stream, and both
/// grant reservations. Dropping it stops the read and releases the
/// reservations: no later batch or group is decoded, and no Wyrd task outlives
/// it. A file operation Tokio already started keeps its own descriptor open
/// until it returns, so the run stays readable for it even if retirement
/// unlinks the path.
pub(crate) struct StagedRun {
    /// Open run file; each row group reads through its own clone of it.
    file: tokio::fs::File,
    /// Footer read once at open and shared by every row-group stream.
    metadata: ArrowReaderMetadata,
    /// Parquet projection of the caller's required columns.
    projection: ProjectionMask,
    /// Rows decoded per output batch.
    batch_rows: usize,
    /// Index of the next row group to read.
    next_group: usize,
    /// Stream over the row group being read, if one is open.
    group: Option<ParquetRecordBatchStream<tokio::fs::File>>,
    /// Grant reservation for the open row group's projected encoded bytes.
    fetched: MemoryReservation,
    /// Grant reservation for the decoded batch last returned.
    decoded: MemoryReservation,
    /// The run's own Arrow schema, which carries each column's field.
    schema: SchemaRef,
    /// Signed projection closure, in caller order.
    required_columns: Arc<[String]>,
    /// Signed predicate conjunction every returned row must satisfy.
    predicates: Arc<[ScanPredicate]>,
    /// Predicate compiled against the first decoded batch and then reused.
    filter: Option<ScanPredicateFilter>,
    /// Path of the run, named in every failure.
    run: PathBuf,
}

impl StagedRun {
    /// Pulls batches until one retains a signed row, returning it.
    ///
    /// The charge for the previously returned batch is released first, since
    /// the caller has consumed it by pulling again. Row groups are read one at
    /// a time: before a group is fetched its projected encoded bytes are
    /// reserved, and the reservation is released when the group is exhausted.
    /// Each decoded batch is charged, reordered to the caller's column order,
    /// and filtered before anything else is decoded. A batch retaining no row
    /// is dropped only after it was awaited, and the task yields to the
    /// runtime before the next pull, so a long zero-match run neither emits
    /// empty batches nor holds a worker for the whole run. Returns `None` once
    /// the run is exhausted.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::Internal`] when a row group's projected fetch or
    /// a decoded batch does not fit the remaining grant, a batch cannot be
    /// decoded or reordered, or the signed predicate cannot be compiled or
    /// evaluated. The run must not be pulled again after an error.
    pub(crate) async fn next_rows(&mut self) -> Result<Option<RecordBatch>, ScribeError> {
        self.decoded.free();
        loop {
            let Some(group) = self.group.as_mut() else {
                if !self.open_next_group().await? {
                    return Ok(None);
                }
                continue;
            };
            let Some(batch) = group.next().await else {
                self.group = None;
                self.fetched.free();
                continue;
            };
            let batch = batch.map_err(run_failure(&self.run, "decode a batch from"))?;
            self.decoded
                .try_resize(batch.get_array_memory_size())
                .map_err(grant_refusal(&self.run, "a decoded batch"))?;
            let rows = self.retain(reorder(
                &batch,
                &self.schema,
                &self.required_columns,
                &self.run,
            )?)?;
            if rows.num_rows() > 0 {
                return Ok(Some(rows));
            }
            self.decoded.free();
            tokio::task::yield_now().await;
        }
    }

    /// Reserves and opens the next row group, returning `false` at the end.
    ///
    /// The group's projected encoded bytes are reserved before its stream is
    /// built, so a group the grant cannot hold is refused before any of it is
    /// fetched.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::Internal`] when the projected bytes overflow or
    /// exceed the remaining grant, or the file cannot be cloned or read.
    async fn open_next_group(&mut self) -> Result<bool, ScribeError> {
        let Some(group) = self.metadata.metadata().row_groups().get(self.next_group) else {
            return Ok(false);
        };
        let fetched = projected_encoded_bytes(group, &self.projection).ok_or_else(|| {
            ScribeError::Internal {
                detail: format!(
                    "staged run {} row group {} projects more bytes than this platform can address",
                    self.run.display(),
                    self.next_group
                ),
            }
        })?;
        self.fetched
            .try_resize(fetched)
            .map_err(grant_refusal(&self.run, "a row group fetch"))?;
        let file = self
            .file
            .try_clone()
            .await
            .map_err(run_failure(&self.run, "reopen"))?;
        let stream =
            ParquetRecordBatchStreamBuilder::new_with_metadata(file, self.metadata.clone())
                .with_batch_size(self.batch_rows)
                .with_projection(self.projection.clone())
                .with_row_groups(vec![self.next_group])
                .build()
                .map_err(run_failure(&self.run, "start reading"))?;
        self.group = Some(stream);
        self.next_group += 1;
        Ok(true)
    }

    /// Keeps the rows of `batch` the signed predicate authorizes.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::Internal`] when the predicate cannot be compiled
    /// against the projected schema or cannot be evaluated.
    fn retain(&mut self, batch: RecordBatch) -> Result<RecordBatch, ScribeError> {
        if self.predicates.is_empty() {
            return Ok(batch);
        }
        let filter = match &self.filter {
            Some(filter) => filter,
            None => self.filter.insert(
                ScanPredicateFilter::compile(&batch.schema(), &self.predicates).map_err(
                    |error| ScribeError::Internal {
                        detail: format!(
                            "live-tail predicate is invalid for this snapshot: {error}"
                        ),
                    },
                )?,
            ),
        };
        filter.retain(batch).map_err(|error| ScribeError::Internal {
            detail: format!("live-tail predicate evaluation failed: {error}"),
        })
    }
}

/// Sums the encoded bytes of `group`'s projected column chunks.
///
/// This is what the async reader fetches for the group. Returns `None` when
/// the sum overflows or does not fit `usize`.
fn projected_encoded_bytes(group: &RowGroupMetaData, projection: &ProjectionMask) -> Option<usize> {
    group
        .columns()
        .iter()
        .enumerate()
        .filter(|(leaf, _)| projection.leaf_included(*leaf))
        .try_fold(0_u64, |total, (_, chunk)| {
            total.checked_add(chunk.byte_range().1)
        })
        .and_then(|total| usize::try_from(total).ok())
}

/// Builds the Parquet projection for the caller's required columns.
///
/// # Errors
///
/// Returns [`ScribeError::Internal`] when a required column is not in the run's
/// own schema, which means the run was written under a different schema than
/// the reader was told to expect.
fn projection_mask(
    schema: &Schema,
    parquet_schema: &SchemaDescriptor,
    required_columns: &[String],
    run: &Path,
) -> Result<ProjectionMask, ScribeError> {
    if required_columns.is_empty() {
        return Ok(ProjectionMask::all());
    }
    let mut leaves = Vec::with_capacity(required_columns.len());
    for name in required_columns {
        let index = schema.index_of(name).map_err(|_| ScribeError::Internal {
            detail: format!(
                "staged run {} does not carry the required column {name}",
                run.display()
            ),
        })?;
        leaves.push(index);
    }
    Ok(ProjectionMask::roots(parquet_schema, leaves))
}

/// Returns the projected batch with columns in the caller's requested order.
///
/// A Parquet projection preserves file order, but the caller's projection is
/// positional: Oracle binds the returned columns by index, so a batch whose
/// columns are in file order would bind the wrong data to the right name.
///
/// # Errors
///
/// Returns [`ScribeError::Internal`] when a required column is missing from the
/// decoded batch.
fn reorder(
    batch: &RecordBatch,
    schema: &SchemaRef,
    required_columns: &[String],
    run: &Path,
) -> Result<RecordBatch, ScribeError> {
    if required_columns.is_empty() {
        return Ok(batch.clone());
    }
    let mut fields = Vec::with_capacity(required_columns.len());
    let mut columns = Vec::with_capacity(required_columns.len());
    for name in required_columns {
        let index = batch
            .schema()
            .index_of(name)
            .map_err(|_| ScribeError::Internal {
                detail: format!("staged run {} projected no column {name}", run.display()),
            })?;
        fields.push(
            schema
                .field_with_name(name)
                .cloned()
                .map_err(|_| ScribeError::Internal {
                    detail: format!(
                        "staged run {} does not carry the required column {name}",
                        run.display()
                    ),
                })?,
        );
        columns.push(Arc::clone(batch.column(index)));
    }
    RecordBatch::try_new(Arc::new(Schema::new(fields)), columns).map_err(|error| {
        ScribeError::Internal {
            detail: format!(
                "staged run {} could not be projected to the requested columns: {error}",
                run.display()
            ),
        }
    })
}

/// Builds the refusal for a read step the follower grant cannot hold.
fn grant_refusal<E: std::fmt::Display>(
    run: &Path,
    subject: &'static str,
) -> impl Fn(E) -> ScribeError {
    let run = run.display().to_string();
    move |error| ScribeError::Internal {
        detail: format!(
            "resources exhausted: {subject} of staged run {run} exceeds the follower grant: {error}"
        ),
    }
}

/// Builds the refusal describing one failed staged-run read step.
fn run_failure<E: std::fmt::Display>(
    run: &Path,
    action: &'static str,
) -> impl Fn(E) -> ScribeError {
    let run = run.display().to_string();
    move |error| ScribeError::Internal {
        detail: format!("could not {action} staged run {run}: {error}"),
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use std::path::{Path, PathBuf};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use arrow::array::{Int64Array, StringArray};
    use arrow::datatypes::{DataType, Field, Schema};
    use datafusion::execution::memory_pool::{GreedyMemoryPool, UnboundedMemoryPool};
    use parquet::file::properties::WriterProperties;
    use wyrd_spec::vala::assignment_authority::{ScanLiteral, ScanPredicate};

    use super::*;

    /// Returns a pool that admits every reservation, for reads whose grant is
    /// not under test.
    pub(crate) fn unbounded_pool() -> Arc<dyn MemoryPool> {
        Arc::new(UnboundedMemoryPool::default())
    }

    /// Builds the two-column schema every staged fixture run is written under.
    fn fixture_schema() -> SchemaRef {
        Arc::new(Schema::new(vec![
            Field::new("ordinal", DataType::Int64, false),
            Field::new("label", DataType::Utf8, false),
        ]))
    }

    /// Writes one staged run of `rows` ordinals labelled by `label`, closing a
    /// row group every `group_rows` rows.
    ///
    /// # Panics
    ///
    /// Panics when the fixture cannot write its own run, which would be a
    /// fixture bug rather than reader behavior.
    fn write_run_with(
        directory: &Path,
        name: &str,
        rows: i64,
        group_rows: usize,
        label: impl Fn(i64) -> String,
    ) -> PathBuf {
        let schema = fixture_schema();
        let batch = RecordBatch::try_new(
            Arc::clone(&schema),
            vec![
                Arc::new(Int64Array::from_iter_values(0..rows)),
                Arc::new(StringArray::from_iter_values((0..rows).map(label))),
            ],
        )
        .expect("fixture staged batch");
        let path = directory.join(name);
        let file = std::fs::File::create(&path).expect("fixture staged run file");
        let properties = WriterProperties::builder()
            .set_max_row_group_row_count(Some(group_rows))
            .build();
        let mut writer = parquet::arrow::ArrowWriter::try_new(file, schema, Some(properties))
            .expect("fixture run writer");
        writer.write(&batch).expect("fixture run rows");
        writer.close().expect("fixture run footer");
        path
    }

    /// Writes one single-group staged run of `rows` sequential ordinals.
    ///
    /// # Panics
    ///
    /// Panics when the fixture cannot write its own run.
    fn write_run(directory: &Path, name: &str, rows: i64) -> PathBuf {
        write_run_with(directory, name, rows, 1024 * 1024, |ordinal| {
            format!("row-{ordinal}")
        })
    }

    /// Writes one row group whose 64 KiB rows repeat a single label.
    ///
    /// The group encodes to a few KiB but decodes to `rows * 64 KiB`, the
    /// shape of many accepted compressible writes staged together.
    ///
    /// # Panics
    ///
    /// Panics when the fixture cannot write its own run.
    fn write_compressible_run(directory: &Path, rows: i64) -> PathBuf {
        let label = "x".repeat(64 * 1024);
        write_run_with(
            directory,
            "compressible.parquet",
            rows,
            1024 * 1024,
            move |_| label.clone(),
        )
    }

    /// Writes `requests` batches of 1 KiB constant-label rows into one row group.
    ///
    /// Each batch stands for one accepted compressible request; together they
    /// encode to a few KiB yet decode to about `requests * rows_per_request`
    /// KiB, the shape of many accepted writes staged into a single group.
    ///
    /// # Panics
    ///
    /// Panics when the fixture cannot write its own run.
    fn write_compressible_requests(
        directory: &Path,
        requests: i64,
        rows_per_request: i64,
    ) -> PathBuf {
        let schema = fixture_schema();
        let label = "x".repeat(1024);
        let path = directory.join("compressible-requests.parquet");
        let file = std::fs::File::create(&path).expect("fixture staged run file");
        let properties = WriterProperties::builder()
            .set_max_row_group_row_count(Some(1024 * 1024))
            .build();
        let mut writer =
            parquet::arrow::ArrowWriter::try_new(file, Arc::clone(&schema), Some(properties))
                .expect("fixture run writer");
        for request in 0..requests {
            let start = request * rows_per_request;
            let batch = RecordBatch::try_new(
                Arc::clone(&schema),
                vec![
                    Arc::new(Int64Array::from_iter_values(
                        start..start + rows_per_request,
                    )),
                    Arc::new(StringArray::from_iter_values(
                        (0..rows_per_request).map(|_| label.as_str()),
                    )),
                ],
            )
            .expect("fixture request batch");
            writer.write(&batch).expect("fixture request rows");
        }
        let metadata = writer.close().expect("fixture run footer");
        assert_eq!(
            metadata.num_row_groups(),
            1,
            "every request shares one group"
        );
        path
    }

    /// Opens `run` charged to `pool` and drains every batch it returns.
    ///
    /// # Errors
    ///
    /// Returns the first open, grant, decode, or predicate failure the reader
    /// reports.
    async fn drain_in(
        pool: &Arc<dyn MemoryPool>,
        run: &Path,
        columns: &[String],
        predicates: &[ScanPredicate],
    ) -> Result<Vec<RecordBatch>, ScribeError> {
        let mut run = StagedTailReader::default()
            .open(run.to_path_buf(), columns.into(), predicates.into(), pool)
            .await?;
        let mut batches = Vec::new();
        while let Some(rows) = run.next_rows().await? {
            batches.push(rows);
        }
        Ok(batches)
    }

    /// Drains `run` under an unbounded grant.
    ///
    /// # Errors
    ///
    /// Returns the first open, decode, or predicate failure the reader reports.
    async fn drain(
        run: &Path,
        columns: &[String],
        predicates: &[ScanPredicate],
    ) -> Result<Vec<RecordBatch>, ScribeError> {
        drain_in(&unbounded_pool(), run, columns, predicates).await
    }

    /// A staged read returns the requested columns in the caller's order.
    ///
    /// Oracle binds a projected batch positionally, so file order is not good
    /// enough: a run written `(ordinal, label)` must come back `(label,
    /// ordinal)` when that is what the caller asked for, and must carry no
    /// column the caller did not ask for.
    ///
    /// # Panics
    ///
    /// Panics when the projection is dropped, reordered wrongly, or widened.
    #[tokio::test]
    async fn a_staged_read_projects_exactly_the_requested_columns_in_caller_order() {
        let root = tempfile::tempdir().expect("staged root");
        let run = write_run(root.path(), "run-1.parquet", 4);
        let columns = vec!["label".to_owned(), "ordinal".to_owned()];
        let batches = drain(&run, &columns, &[])
            .await
            .expect("the staged run reads");

        assert_eq!(batches.len(), 1);
        let rows = &batches[0];
        assert_eq!(rows.num_columns(), 2);
        assert_eq!(rows.schema().field(0).name(), "label");
        assert_eq!(rows.schema().field(1).name(), "ordinal");
    }

    /// A staged read over many row groups yields only the rows its signed
    /// predicate authorizes, in order, skips every batch retaining none, and
    /// releases its grant once drained.
    ///
    /// # Panics
    ///
    /// Panics when an empty batch is yielded, a matching row is dropped or
    /// reordered, or grant bytes remain charged after the read.
    #[tokio::test]
    async fn a_staged_read_filters_and_skips_batches_retaining_no_row() {
        let root = tempfile::tempdir().expect("staged root");
        let run = write_run_with(root.path(), "groups.parquet", 40_000, 5_000, |ordinal| {
            format!("row-{ordinal}")
        });
        let pool = unbounded_pool();
        let columns = vec!["ordinal".to_owned()];
        let none = vec![ScanPredicate::Eq(
            "ordinal".to_owned(),
            ScanLiteral::I64(-1),
        )];
        assert!(
            drain_in(&pool, &run, &columns, &none)
                .await
                .expect("the zero-match run reads")
                .is_empty(),
            "a batch retaining no row is skipped, not yielded empty"
        );
        assert_eq!(pool.reserved(), 0, "a drained read holds no grant");

        let some = vec![ScanPredicate::Gt(
            "ordinal".to_owned(),
            ScanLiteral::I64(9_990),
        )];
        let batches = drain_in(&pool, &run, &columns, &some)
            .await
            .expect("the selective run reads");
        assert!(batches.iter().all(|batch| batch.num_rows() > 0));
        let ordinals = batches
            .iter()
            .flat_map(|batch| {
                batch
                    .column(0)
                    .as_any()
                    .downcast_ref::<Int64Array>()
                    .expect("ordinal column")
                    .values()
                    .to_vec()
            })
            .collect::<Vec<_>>();
        assert_eq!(ordinals, (9_991..40_000).collect::<Vec<_>>());
        assert_eq!(pool.reserved(), 0, "a drained read holds no grant");
    }

    /// Dropping a staged read mid-run releases every grant byte it charged.
    ///
    /// # Panics
    ///
    /// Panics when the open read holds no charge or its drop leaves one.
    #[tokio::test]
    async fn a_dropped_staged_read_releases_its_grant() {
        let root = tempfile::tempdir().expect("staged root");
        let run = write_run_with(root.path(), "groups.parquet", 40_000, 5_000, |ordinal| {
            format!("row-{ordinal}")
        });
        let pool = unbounded_pool();
        let mut read = StagedTailReader::default()
            .open(run, Arc::from(Vec::new()), Arc::from(Vec::new()), &pool)
            .await
            .expect("the staged run opens");
        read.next_rows()
            .await
            .expect("the first batch reads")
            .expect("the run has rows");
        assert!(
            pool.reserved() > 0,
            "a mid-run read holds its group and batch"
        );
        drop(read);
        assert_eq!(pool.reserved(), 0);
    }

    /// A required column the run does not carry is refused, naming the run.
    ///
    /// A staged run written under a different schema than the reader was told
    /// to expect is durable-evidence drift, so the read fails closed instead of
    /// silently returning a narrower batch.
    ///
    /// # Panics
    ///
    /// Panics when the read succeeds or refuses without naming the run.
    #[tokio::test]
    async fn a_missing_required_column_refuses_and_names_the_run() {
        let root = tempfile::tempdir().expect("staged root");
        let run = write_run(root.path(), "run-1.parquet", 2);
        let columns = vec!["absent".to_owned()];
        let error = drain(&run, &columns, &[])
            .await
            .expect_err("a column the run does not carry is refused");

        let detail = error.to_string();
        assert!(detail.contains("absent"), "{detail}");
        assert!(detail.contains("run-1.parquet"), "{detail}");
    }

    /// A zero-match predicate over a many-group run returns nothing, decodes
    /// it one batch per pull, and lets other tasks on the same worker run
    /// between batches.
    ///
    /// On a single-threaded runtime a ticking task can only advance while the
    /// read yields, so it advancing once per skipped batch proves no batch —
    /// and no group — is decoded without handing the worker back.
    ///
    /// # Panics
    ///
    /// Panics when a row is returned or the ticking task is starved.
    #[tokio::test(flavor = "current_thread")]
    async fn a_zero_match_staged_read_returns_nothing_and_does_not_starve_the_runtime() {
        let root = tempfile::tempdir().expect("staged root");
        let rows = 16 * STAGED_READ_BATCH_ROWS;
        let run = write_run_with(
            root.path(),
            "groups.parquet",
            i64::try_from(rows).expect("fixture rows fit i64"),
            4 * STAGED_READ_BATCH_ROWS,
            |ordinal| format!("row-{ordinal}"),
        );
        let ticks = Arc::new(AtomicUsize::new(0));
        let ticker = tokio::spawn({
            let ticks = Arc::clone(&ticks);
            async move {
                loop {
                    ticks.fetch_add(1, Ordering::Relaxed);
                    tokio::task::yield_now().await;
                }
            }
        });
        let predicates = vec![ScanPredicate::Eq(
            "ordinal".to_owned(),
            ScanLiteral::I64(-1),
        )];
        let batches = drain(&run, &["ordinal".to_owned()], &predicates)
            .await
            .expect("the zero-match run reads");
        ticker.abort();

        assert!(batches.is_empty(), "no empty batch is returned");
        assert!(
            ticks.load(Ordering::Relaxed) >= rows / STAGED_READ_BATCH_ROWS,
            "another task ran fewer than once per decoded batch"
        );
    }

    /// A row group whose projected encoded bytes exceed the grant is refused
    /// before it is fetched.
    ///
    /// The run's data pages are corrupted, so any fetch and decode would fail
    /// with a decode error; getting the grant refusal instead proves the
    /// refusal came first.
    ///
    /// # Panics
    ///
    /// Panics when the read opens a group, fails for another reason, or does
    /// not name the run.
    #[tokio::test]
    async fn a_staged_row_group_fetch_above_the_grant_is_refused_before_fetch() {
        let root = tempfile::tempdir().expect("staged root");
        let run = write_run(root.path(), "run-1.parquet", 4_096);
        let mut bytes = std::fs::read(&run).expect("run bytes");
        for byte in &mut bytes[4..64] {
            *byte = 0xFF;
        }
        std::fs::write(&run, bytes).expect("corrupt run");
        let pool: Arc<dyn MemoryPool> = Arc::new(GreedyMemoryPool::new(1_024));

        let detail = drain_in(&pool, &run, &[], &[])
            .await
            .expect_err("a group above the grant is refused")
            .to_string();
        assert!(detail.contains("row group fetch"), "{detail}");
        assert!(detail.contains("run-1.parquet"), "{detail}");
        assert_eq!(pool.reserved(), 0);
    }

    /// A compressible row group whose decoded total exceeds the grant still
    /// reads, because each decoded batch is charged and released on its own.
    ///
    /// 512 rows of 64 KiB decode to 32 MiB; the grant is 16 MiB, which holds
    /// the few-KiB encoded fetch plus one 8,192-row-capped batch at a time.
    ///
    /// # Panics
    ///
    /// Panics when the read is refused or loses a row.
    #[tokio::test]
    async fn a_compressible_group_above_the_grant_reads_in_charged_batches() {
        let root = tempfile::tempdir().expect("staged root");
        let run = write_compressible_run(root.path(), 512);
        let grant = 16 * 1024 * 1024;
        let pool: Arc<dyn MemoryPool> = Arc::new(GreedyMemoryPool::new(grant));
        let mut read = StagedTailReader { batch_rows: 64 }
            .open(run, Arc::from(Vec::new()), Arc::from(Vec::new()), &pool)
            .await
            .expect("the compressible run opens");
        let mut rows = 0;
        let mut decoded = 0;
        while let Some(batch) = read.next_rows().await.expect("each batch fits the grant") {
            rows += batch.num_rows();
            decoded += batch.get_array_memory_size();
        }
        assert_eq!(rows, 512);
        assert!(
            decoded > grant,
            "the decoded total {decoded} exceeds the grant"
        );
    }

    /// Several accepted compressible requests staged into one row group that
    /// decodes above 256 MiB read back whole under a 32 MiB grant.
    ///
    /// Only the group's few-KiB encoded fetch and one 8,192-row, 8 MiB output
    /// batch are charged at a time, so the group's uncompressed total never
    /// has to fit the grant.
    ///
    /// # Panics
    ///
    /// Panics when the read is refused, loses a row, or retains its grant.
    #[tokio::test]
    async fn compressible_requests_in_one_group_above_256_mib_read_under_a_small_grant() {
        let root = tempfile::tempdir().expect("staged root");
        let run = write_compressible_requests(root.path(), 5, 60_000);
        let grant = 32 * 1024 * 1024;
        let pool: Arc<dyn MemoryPool> = Arc::new(GreedyMemoryPool::new(grant));
        let mut read = StagedTailReader::default()
            .open(run, Arc::from(Vec::new()), Arc::from(Vec::new()), &pool)
            .await
            .expect("the compressible run opens");
        let mut rows = 0;
        let mut decoded = 0;
        while let Some(batch) = read.next_rows().await.expect("each batch fits the grant") {
            rows += batch.num_rows();
            decoded += batch.get_array_memory_size();
        }
        drop(read);
        assert_eq!(rows, 300_000);
        assert!(
            decoded > 256 * 1024 * 1024,
            "the decoded total {decoded} exceeds 256 MiB"
        );
        assert_eq!(pool.reserved(), 0, "the drained read returns its grant");
    }

    /// A decoded batch larger than the remaining grant fails the read at that
    /// batch.
    ///
    /// # Panics
    ///
    /// Panics when the read succeeds or fails for another reason.
    #[tokio::test]
    async fn a_decoded_staged_batch_above_the_grant_fails_the_read() {
        let root = tempfile::tempdir().expect("staged root");
        let run = write_compressible_run(root.path(), 512);
        let pool: Arc<dyn MemoryPool> = Arc::new(GreedyMemoryPool::new(4 * 1024 * 1024));
        let detail = drain_in(&pool, &run, &[], &[])
            .await
            .expect_err("a 32 MiB batch does not fit a 4 MiB grant")
            .to_string();
        assert!(detail.contains("decoded batch"), "{detail}");
        assert!(detail.contains("resources exhausted"), "{detail}");
    }

    /// A staged run whose data pages are corrupt fails the read instead of
    /// returning partial or empty rows.
    ///
    /// # Panics
    ///
    /// Panics when the corrupt run reads successfully.
    #[tokio::test]
    async fn a_corrupt_staged_run_fails_the_read() {
        let root = tempfile::tempdir().expect("staged root");
        let run = write_run(root.path(), "run-1.parquet", 64);
        let mut bytes = std::fs::read(&run).expect("run bytes");
        for byte in &mut bytes[4..64] {
            *byte = 0xFF;
        }
        std::fs::write(&run, bytes).expect("corrupt run");

        drain(&run, &[], &[])
            .await
            .expect_err("a corrupt data page fails the read");
    }
}
