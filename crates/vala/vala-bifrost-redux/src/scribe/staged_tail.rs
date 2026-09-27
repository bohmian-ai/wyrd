//! Lazy live-tail reads over durable staged runs.
//!
//! Once a generation's rows are staged, its Arrow is gone: the runs on the
//! staging volume are the only local copy until the claim that owns them
//! publishes. A live-tail reader must therefore be able to read Parquet, but it
//! decodes one bounded window at a time under the caller's projection and
//! signed predicate, so a staged member that is large by construction never
//! moves into memory whole; the consumer's pull rate bounds what is resident.
//!
//! The reader decides nothing about authority. It is handed the staged sources
//! a [`ScribeHotSourceRegistry`](crate::scribe::hot_source::ScribeHotSourceRegistry)
//! resolved and returns rows from exactly those.

use std::path::PathBuf;
use std::sync::Arc;

use arrow::array::RecordBatch;
use arrow::datatypes::SchemaRef;
use parquet::arrow::ProjectionMask;
use parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder;
use tokio::sync::oneshot;
use wyrd_spec::vala::assignment_authority::ScanPredicate;

use crate::contracts::ScribeError;
use crate::scribe::hot_source::StagedSourceLease;

/// Rows decoded per Parquet window yielded to a live producer.
///
/// Small enough that one resident window stays modest however large the member
/// is, large enough that a normal read is not dominated by per-batch overhead.
const STAGED_READ_BATCH_ROWS: usize = 8 * 1024;

/// Reads bounded Arrow batches out of durable staged runs.
#[derive(Debug, Clone, Copy)]
pub(crate) struct StagedTailReader {
    /// Rows decoded per window yielded to the live producer.
    batch_rows: usize,
}

impl Default for StagedTailReader {
    /// Builds a reader with the module's decode window.
    fn default() -> Self {
        Self {
            batch_rows: STAGED_READ_BATCH_ROWS,
        }
    }
}

impl StagedTailReader {
    /// Opens one run and yields its decode windows lazily, retaining only the
    /// rows its signed predicate authorizes.
    ///
    /// Each window is decoded only when the iterator is advanced, so a live
    /// producer that yields between windows never holds more than one decoded
    /// window. The predicate is compiled once per run against the projected
    /// schema and reused for every window; a window retaining no row is
    /// skipped.
    ///
    /// This is synchronous file IO; an async caller reaches it only through
    /// [`StagedRunWindows`], which runs it on the blocking pool.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::Internal`] when the run cannot be opened or its
    /// projection cannot be resolved; the iterator yields
    /// [`ScribeError::Internal`] when a batch cannot be decoded or reordered,
    /// or the signed predicate cannot be compiled or evaluated.
    pub(crate) fn run_batches(
        self,
        run: PathBuf,
        required_columns: Arc<[String]>,
        predicates: Arc<[ScanPredicate]>,
    ) -> Result<impl Iterator<Item = Result<RecordBatch, ScribeError>> + Send + 'static, ScribeError>
    {
        let file = std::fs::File::open(&run).map_err(run_failure(&run, "open"))?;
        let builder = ParquetRecordBatchReaderBuilder::try_new(file)
            .map_err(run_failure(&run, "read the metadata of"))?
            .with_batch_size(self.batch_rows);
        let projection = projection_mask(&builder, &required_columns, &run)?;
        let schema = builder.schema().clone();
        let reader = builder
            .with_projection(projection)
            .build()
            .map_err(run_failure(&run, "start reading"))?;
        let mut filter = None;
        Ok(reader
            .map(move |batch| {
                #[cfg(test)]
                STAGED_WINDOW_GATE.pass();
                let batch = batch.map_err(run_failure(&run, "decode a batch from"))?;
                let batch = reorder(&batch, &schema, &required_columns, &run)?;
                if predicates.is_empty() {
                    return Ok(batch);
                }
                let filter = match &filter {
                    Some(filter) => filter,
                    None => filter.insert(
                        crate::oracle::exec::ScanPredicateFilter::compile(
                            &batch.schema(),
                            &predicates,
                        )
                        .map_err(|error| ScribeError::Internal {
                            detail: format!(
                                "live-tail predicate is invalid for this snapshot: {error}"
                            ),
                        })?,
                    ),
                };
                filter.retain(batch).map_err(|error| ScribeError::Internal {
                    detail: format!("live-tail predicate evaluation failed: {error}"),
                })
            })
            .filter(|batch| !matches!(batch, Ok(batch) if batch.num_rows() == 0)))
    }
}

/// Boxed window iterator one staged run decodes on the blocking pool.
type StagedWindows = Box<dyn Iterator<Item = Result<RecordBatch, ScribeError>> + Send>;

/// One staged run decoded window by window off the async runtime.
///
/// Opening and every window decode are synchronous Parquet IO, so each runs
/// as one blocking-pool task that takes this value, advances it once, and
/// hands it back through a oneshot. Exactly one window is requested per pull,
/// so demand and residency stay one window.
///
/// The value carries the staged lease protecting its file. If the async
/// consumer is dropped while a window is being read, the blocking task finds
/// the receiver gone and drops this value, and with it the lease, itself —
/// only after its protected read has exited, and without starting another
/// window.
pub(crate) struct StagedRunWindows {
    /// Remaining decode windows of the run.
    windows: StagedWindows,
    /// Staged lease keeping the run's file readable while it is decoded.
    _lease: Option<Arc<StagedSourceLease>>,
}

impl StagedRunWindows {
    /// Opens `run` on the blocking pool under `lease`.
    ///
    /// # Errors
    ///
    /// Returns the reader's open or projection failure, and
    /// [`ScribeError::Internal`] when the blocking task cannot report back.
    /// Cancellation before this returns leaves the blocking task to drop the
    /// opened run and its lease when the open finishes.
    pub(crate) async fn open(
        reader: StagedTailReader,
        run: PathBuf,
        required_columns: Arc<[String]>,
        predicates: Arc<[ScanPredicate]>,
        lease: Option<Arc<StagedSourceLease>>,
    ) -> Result<Self, ScribeError> {
        Self::on_blocking_pool(move || {
            let windows = reader.run_batches(run, required_columns, predicates)?;
            Ok(Self {
                windows: Box::new(windows),
                _lease: lease,
            })
        })
        .await
    }

    /// Decodes the next window on the blocking pool.
    ///
    /// Returns `None` once the run is exhausted, which drops this value.
    ///
    /// # Errors
    ///
    /// Returns the window's decode, projection, or predicate failure, and
    /// [`ScribeError::Internal`] when the blocking task cannot report back.
    /// Cancellation while the window is read stops production after it.
    pub(crate) async fn next(mut self) -> Result<Option<(Self, RecordBatch)>, ScribeError> {
        Self::on_blocking_pool(move || match self.windows.next() {
            None => Ok(None),
            Some(window) => window.map(|batch| Some((self, batch))),
        })
        .await
    }

    /// Runs one synchronous step on the blocking pool and awaits its result.
    ///
    /// The result travels through a oneshot rather than the join handle, so
    /// when the awaiting future has been dropped the result, and any run and
    /// lease it holds, is dropped inside the blocking task as soon as the
    /// step returns.
    ///
    /// # Errors
    ///
    /// Returns the step's own error, and [`ScribeError::Internal`] when the
    /// blocking task ended without reporting, such as by panicking.
    async fn on_blocking_pool<T: Send + 'static>(
        step: impl FnOnce() -> Result<T, ScribeError> + Send + 'static,
    ) -> Result<T, ScribeError> {
        let (sender, receiver) = oneshot::channel();
        tokio::task::spawn_blocking(move || {
            // A closed receiver means the consumer is gone; the returned
            // result is dropped right here, releasing what it holds.
            drop(sender.send(step()));
            #[cfg(test)]
            STAGED_WINDOW_GATE.exit();
        });
        receiver.await.map_err(|_| ScribeError::Internal {
            detail: "staged run read ended without a result".to_owned(),
        })?
    }
}

/// Test gate that holds one staged window read on its blocking thread.
///
/// Armed, the next window read blocks inside its blocking task until the test
/// releases it, so a test can prove the async runtime stays responsive and
/// observe what a cancelled read still holds. Every window read is counted.
#[cfg(test)]
pub(crate) struct StagedWindowGate {
    /// Gate state shared with the blocking reader.
    state: std::sync::Mutex<StagedWindowGateState>,
    /// Signals every state change.
    changed: std::sync::Condvar,
}

/// Observable state of [`StagedWindowGate`].
#[cfg(test)]
#[derive(Debug, Default)]
pub(crate) struct StagedWindowGateState {
    /// Whether the next window read should block.
    pub(crate) armed: bool,
    /// Whether a read is blocked at the gate.
    pub(crate) held: bool,
    /// Window reads started.
    pub(crate) windows: usize,
    /// Blocking steps that have finished and dropped any unreceived result.
    pub(crate) exits: usize,
}

/// The one process gate staged window reads pass under test.
#[cfg(test)]
pub(crate) static STAGED_WINDOW_GATE: StagedWindowGate = StagedWindowGate {
    state: std::sync::Mutex::new(StagedWindowGateState {
        armed: false,
        held: false,
        windows: 0,
        exits: 0,
    }),
    changed: std::sync::Condvar::new(),
};

#[cfg(test)]
impl StagedWindowGate {
    /// Locks the state, recovering it from a poisoned test.
    fn lock(&self) -> std::sync::MutexGuard<'_, StagedWindowGateState> {
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Arms the gate for the next window read.
    pub(crate) fn arm(&self) {
        self.lock().armed = true;
    }

    /// Counts a window read and blocks it while the gate is armed.
    fn pass(&self) {
        let mut state = self.lock();
        state.windows += 1;
        if state.armed {
            state.held = true;
            self.changed.notify_all();
            while state.armed {
                state = self
                    .changed
                    .wait(state)
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
            }
            state.held = false;
        }
    }

    /// Records one finished blocking step.
    fn exit(&self) {
        self.lock().exits += 1;
        self.changed.notify_all();
    }

    /// Releases a held read.
    pub(crate) fn release(&self) {
        self.lock().armed = false;
        self.changed.notify_all();
    }

    /// Blocks the calling thread until `ready` holds, returning a snapshot.
    pub(crate) fn wait_until(
        &self,
        ready: impl Fn(&StagedWindowGateState) -> bool,
    ) -> (usize, usize) {
        let mut state = self.lock();
        while !ready(&state) {
            state = self
                .changed
                .wait(state)
                .unwrap_or_else(std::sync::PoisonError::into_inner);
        }
        (state.windows, state.exits)
    }
}

/// Builds the Parquet projection for the caller's required columns.
///
/// # Errors
///
/// Returns [`ScribeError::Internal`] when a required column is not in the run's
/// own schema, which means the run was written under a different schema than
/// the reader was told to expect.
fn projection_mask<T: parquet::file::reader::ChunkReader>(
    builder: &ParquetRecordBatchReaderBuilder<T>,
    required_columns: &[String],
    run: &std::path::Path,
) -> Result<ProjectionMask, ScribeError> {
    if required_columns.is_empty() {
        return Ok(ProjectionMask::all());
    }
    let schema = builder.schema();
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
    Ok(ProjectionMask::roots(builder.parquet_schema(), leaves))
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
    run: &std::path::Path,
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
        columns.push(std::sync::Arc::clone(batch.column(index)));
    }
    RecordBatch::try_new(
        std::sync::Arc::new(arrow::datatypes::Schema::new(fields)),
        columns,
    )
    .map_err(|error| ScribeError::Internal {
        detail: format!(
            "staged run {} could not be projected to the requested columns: {error}",
            run.display()
        ),
    })
}

/// Builds the refusal describing one failed staged-run read step.
fn run_failure<E: std::fmt::Display>(
    run: &std::path::Path,
    action: &'static str,
) -> impl Fn(E) -> ScribeError {
    let run = run.display().to_string();
    move |error| ScribeError::Internal {
        detail: format!("could not {action} staged run {run}: {error}"),
    }
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};
    use std::sync::Arc;

    use arrow::array::{Int64Array, StringArray};
    use arrow::datatypes::{DataType, Field, Schema};

    use super::*;

    /// Builds the two-column schema every staged fixture run is written under.
    fn fixture_schema() -> SchemaRef {
        Arc::new(Schema::new(vec![
            Field::new("ordinal", DataType::Int64, false),
            Field::new("label", DataType::Utf8, false),
        ]))
    }

    /// Writes one staged run holding `rows` sequential ordinals and labels.
    ///
    /// # Panics
    ///
    /// Panics when the fixture cannot write its own run, which would be a
    /// fixture bug rather than reader behavior.
    fn write_run(directory: &Path, name: &str, rows: i64) -> PathBuf {
        let schema = fixture_schema();
        let batch = RecordBatch::try_new(
            Arc::clone(&schema),
            vec![
                Arc::new(Int64Array::from_iter_values(0..rows)),
                Arc::new(StringArray::from_iter_values(
                    (0..rows).map(|ordinal| format!("row-{ordinal}")),
                )),
            ],
        )
        .expect("fixture staged batch");
        let path = directory.join(name);
        let file = std::fs::File::create(&path).expect("fixture staged run file");
        let mut writer =
            parquet::arrow::ArrowWriter::try_new(file, schema, None).expect("fixture run writer");
        writer.write(&batch).expect("fixture run rows");
        writer.close().expect("fixture run footer");
        path
    }

    /// Drains every window [`StagedTailReader::run_batches`] yields for `run`.
    ///
    /// # Errors
    ///
    /// Returns the first open or decode failure the reader reports.
    fn drain(
        run: &Path,
        columns: &[String],
        predicates: &[wyrd_spec::vala::assignment_authority::ScanPredicate],
    ) -> Result<Vec<RecordBatch>, ScribeError> {
        StagedTailReader::default()
            .run_batches(run.to_path_buf(), columns.into(), predicates.into())?
            .collect()
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
    #[test]
    fn a_staged_read_projects_exactly_the_requested_columns_in_caller_order() {
        let root = tempfile::tempdir().expect("staged root");
        let run = write_run(root.path(), "run-1.parquet", 4);
        let columns = vec!["label".to_owned(), "ordinal".to_owned()];
        let batches = drain(&run, &columns, &[]).expect("the staged run reads");

        assert_eq!(batches.len(), 1);
        let rows = &batches[0];
        assert_eq!(rows.num_columns(), 2);
        assert_eq!(rows.schema().field(0).name(), "label");
        assert_eq!(rows.schema().field(1).name(), "ordinal");
    }

    /// A staged read yields only the rows its signed predicate authorizes and
    /// skips a window retaining none of them.
    ///
    /// # Panics
    ///
    /// Panics when an empty window is yielded or a matching row is dropped.
    #[test]
    fn a_staged_read_filters_and_skips_windows_retaining_no_row() {
        use wyrd_spec::vala::assignment_authority::{ScanLiteral, ScanPredicate};

        let root = tempfile::tempdir().expect("staged root");
        let older = write_run(root.path(), "older.parquet", 1);
        let later = write_run(root.path(), "later.parquet", 2);
        let columns = vec!["ordinal".to_owned()];
        let predicates = vec![ScanPredicate::Eq("ordinal".to_owned(), ScanLiteral::I64(1))];

        assert!(
            drain(&older, &columns, &predicates)
                .expect("the older run reads")
                .is_empty(),
            "a window retaining no row is skipped, not yielded empty"
        );
        let batches = drain(&later, &columns, &predicates).expect("the later run reads");
        assert_eq!(batches.len(), 1);
        assert_eq!(batches[0].num_rows(), 1);
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
    #[test]
    fn a_missing_required_column_refuses_and_names_the_run() {
        let root = tempfile::tempdir().expect("staged root");
        let run = write_run(root.path(), "run-1.parquet", 2);
        let columns = vec!["absent".to_owned()];
        let error =
            drain(&run, &columns, &[]).expect_err("a column the run does not carry is refused");

        let detail = error.to_string();
        assert!(detail.contains("absent"), "{detail}");
        assert!(detail.contains("run-1.parquet"), "{detail}");
    }
}
