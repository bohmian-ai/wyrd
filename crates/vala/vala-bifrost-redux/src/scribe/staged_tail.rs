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

use arrow::array::RecordBatch;
use arrow::datatypes::SchemaRef;
use parquet::arrow::ProjectionMask;
use parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder;

use crate::contracts::ScribeError;

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
    /// # Errors
    ///
    /// Returns [`ScribeError::Internal`] when the run cannot be opened or its
    /// projection cannot be resolved; the iterator yields
    /// [`ScribeError::Internal`] when a batch cannot be decoded or reordered,
    /// or the signed predicate cannot be compiled or evaluated.
    pub(crate) fn run_batches<'a>(
        self,
        run: &'a std::path::Path,
        required_columns: &'a [String],
        predicates: &'a [wyrd_spec::vala::assignment_authority::ScanPredicate],
    ) -> Result<impl Iterator<Item = Result<RecordBatch, ScribeError>> + 'a, ScribeError> {
        let file = std::fs::File::open(run).map_err(run_failure(run, "open"))?;
        let builder = ParquetRecordBatchReaderBuilder::try_new(file)
            .map_err(run_failure(run, "read the metadata of"))?
            .with_batch_size(self.batch_rows);
        let projection = projection_mask(&builder, required_columns, run)?;
        let schema = builder.schema().clone();
        let reader = builder
            .with_projection(projection)
            .build()
            .map_err(run_failure(run, "start reading"))?;
        let mut filter = None;
        Ok(reader
            .map(move |batch| {
                let batch = batch.map_err(run_failure(run, "decode a batch from"))?;
                let batch = reorder(&batch, &schema, required_columns, run)?;
                if predicates.is_empty() {
                    return Ok(batch);
                }
                let filter = match &filter {
                    Some(filter) => filter,
                    None => filter.insert(
                        crate::oracle::exec::ScanPredicateFilter::compile(
                            &batch.schema(),
                            predicates,
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
            .run_batches(run, columns, predicates)?
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
