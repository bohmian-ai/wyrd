//! Bounded live-tail reads over durable staged runs.
//!
//! Once a generation's rows are staged, its Arrow is gone: the runs on the
//! staging volume are the only local copy until the claim that owns them
//! publishes. A live-tail reader must therefore be able to read Parquet, but it
//! must read it under the same bounds the memtable path obeys — the caller's
//! projection, its batch count, and its retained-byte ceiling — because a
//! staged member is large by construction and an unbounded decode would move a
//! whole 512 MiB object's worth of rows into one response.
//!
//! The reader decides nothing about authority. It is handed the staged sources
//! a [`ScribeHotSourceRegistry`](crate::scribe::hot_source::ScribeHotSourceRegistry)
//! resolved and returns rows from exactly those.

use arrow::array::RecordBatch;
use arrow::datatypes::SchemaRef;
use parquet::arrow::ProjectionMask;
use parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder;

use crate::contracts::ScribeError;
use crate::scribe::hot_source::StagedSource;
use crate::scribe::memtable::ReadableBatchLimits;
use crate::scribe::tail_rpc::{HotBatch, HotBatchOrigin};
use crate::scribe::wal::WalLsn;

/// Rows decoded per Parquet read before the reader re-checks its bounds.
///
/// Small enough that a member far above the caller's retained-byte ceiling
/// stops after one batch, large enough that a normal read is not dominated by
/// per-batch overhead.
const STAGED_READ_BATCH_ROWS: usize = 8 * 1024;

/// Caller-owned bounds one staged live-tail read must respect.
pub(crate) struct StagedTailRead<'a> {
    /// Requested projection in caller order; empty means the whole schema.
    pub(crate) required_columns: &'a [String],
    /// Count and retained-byte ceilings shared with the memtable path.
    pub(crate) limits: ReadableBatchLimits,
    /// Inclusive persisted prefix owned by the pinned manifest.
    pub(crate) persisted_cursor: WalLsn,
    /// Independently published non-prefix ranges owned by that manifest.
    pub(crate) persisted_ranges: &'a [(WalLsn, WalLsn)],
}

/// Reads bounded Arrow batches out of durable staged runs.
#[derive(Debug, Clone, Copy)]
pub(crate) struct StagedTailReader {
    /// Rows decoded per read before the bounds are re-checked.
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
    /// Returns rows from the staged sources the caller's cut does not own.
    ///
    /// Sources arrive oldest first and are read in that order, so a truncated
    /// read returns the oldest rows rather than an arbitrary subset. A member
    /// whose complete WAL range the pinned cut already owns is skipped without
    /// being opened: the published object serves those rows, and returning them
    /// here would double-count them.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::Internal`] when a run cannot be opened, its
    /// projection cannot be resolved against the run's own schema, or a batch
    /// cannot be decoded. A refusal names the run, because a staged run that
    /// cannot be read is a durable-evidence problem, not a query problem.
    pub(crate) fn read(
        self,
        sources: &[StagedSource],
        read: &StagedTailRead<'_>,
    ) -> Result<Vec<HotBatch>, ScribeError> {
        let mut batches = Vec::new();
        let mut retained_bytes = 0_usize;
        for source in sources {
            if cut_owns(source.wal, read.persisted_cursor, read.persisted_ranges) {
                continue;
            }
            for run in &source.runs {
                if batches.len() >= read.limits.max_batches
                    || retained_bytes >= read.limits.max_retained_bytes
                {
                    return Ok(batches);
                }
                self.read_run(run, source, read, &mut batches, &mut retained_bytes)?;
            }
        }
        Ok(batches)
    }

    /// Decodes one run until the caller's bounds are reached.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::Internal`] when the run cannot be opened, its
    /// projection cannot be resolved, or a batch cannot be decoded.
    fn read_run(
        self,
        run: &std::path::Path,
        source: &StagedSource,
        read: &StagedTailRead<'_>,
        batches: &mut Vec<HotBatch>,
        retained_bytes: &mut usize,
    ) -> Result<(), ScribeError> {
        let file = std::fs::File::open(run).map_err(run_failure(run, "open"))?;
        let builder = ParquetRecordBatchReaderBuilder::try_new(file)
            .map_err(run_failure(run, "read the metadata of"))?
            .with_batch_size(self.batch_rows);
        let projection = projection_mask(&builder, read.required_columns, run)?;
        let schema = builder.schema().clone();
        let reader = builder
            .with_projection(projection)
            .build()
            .map_err(run_failure(run, "start reading"))?;
        for batch in reader {
            let batch = batch.map_err(run_failure(run, "decode a batch from"))?;
            let batch = reorder(&batch, &schema, read.required_columns, run)?;
            *retained_bytes = retained_bytes.saturating_add(batch.get_array_memory_size());
            batches.push(HotBatch {
                partition_day: source.key.partition,
                wal_lsn: source.wal.1,
                origin: HotBatchOrigin::StagedMember {
                    member: source.member,
                },
                rows: batch,
            });
            if batches.len() >= read.limits.max_batches
                || *retained_bytes >= read.limits.max_retained_bytes
            {
                return Ok(());
            }
        }
        Ok(())
    }
}

/// Reports whether a pinned cut already owns a member's complete WAL range.
///
/// Partial coverage is deliberately not enough. A member is one indivisible
/// staged unit: suppressing it because part of its range was published would
/// hide the rows in the rest of it, so the member stays readable until the cut
/// owns all of it.
fn cut_owns(
    wal: (WalLsn, WalLsn),
    persisted_cursor: WalLsn,
    persisted_ranges: &[(WalLsn, WalLsn)],
) -> bool {
    let (min, max) = wal;
    if persisted_cursor != WalLsn::ZERO && max <= persisted_cursor {
        return true;
    }
    persisted_ranges
        .iter()
        .any(|(range_min, range_max)| *range_min <= min && max <= *range_max)
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
