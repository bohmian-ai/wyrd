//! Bounded external merge over the staged runs of one claim.
//!
//! A claim is a set of ready members that share an exact
//! [`ScribeAssemblyKey`](crate::scribe::assembly::ScribeAssemblyKey), and every
//! member's runs are already sorted under that key's `PhysicalLayout`. Assembly
//! therefore needs a merge, not a sort: [`StagedRunMerge`] reads one bounded
//! batch per run at a time and emits globally ordered batches, so the memory a
//! claim owns is proportional to the number of runs rather than to the object
//! it produces.
//!
//! Ordering is the layout's sort keys followed by `(wyrd_batch_id,
//! wyrd_row_ordinal)`. The trailing pair is what makes the merge total: rows
//! that tie on every layout key still have exactly one order, so re-running an
//! interrupted claim over the same members reproduces the same objects rather
//! than a permutation of them.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use arrow::array::RecordBatch;
use arrow::compute::interleave_record_batch;
use arrow::datatypes::SchemaRef;
use arrow::row::{Row, RowConverter, Rows, SortField};
use parquet::arrow::arrow_reader::{ParquetRecordBatchReader, ParquetRecordBatchReaderBuilder};

use crate::catalog::layout::PhysicalLayout;
use crate::contracts::ScribeError;
use crate::schema::SchemaFingerprint;

/// Stable tie-breakers appended after every layout sort key.
///
/// These are managed columns, so they are present in every physical schema the
/// staged writer produces; a run missing one is a corrupted member rather than
/// a supported shape, and the merge refuses it.
const STABLE_TIE_BREAKERS: [&str; 2] = ["wyrd_batch_id", "wyrd_row_ordinal"];

/// One staged run positioned on the row it currently offers to the merge.
///
/// The cursor owns the decoded batch and its comparable row encoding together
/// because both are replaced at the same moment: a refill invalidates every
/// index into the previous batch.
struct RunCursor {
    /// Reader producing bounded batches from one staged run.
    reader: ParquetRecordBatchReader,
    /// Batch the cursor is currently offering rows from.
    batch: RecordBatch,
    /// Comparable encoding of `batch`'s sort key, row for row.
    rows: Rows,
    /// Index of the next unmerged row within `batch`.
    index: usize,
    /// Whether the reader has produced its final batch.
    drained: bool,
}

impl RunCursor {
    /// Returns the comparable row this cursor currently offers.
    fn peek(&self) -> Row<'_> {
        self.rows.row(self.index)
    }

    /// Returns whether this cursor has no row left to offer.
    fn is_exhausted(&self) -> bool {
        self.drained && self.index >= self.batch.num_rows()
    }
}

/// Bounded k-way merge over one claim's staged runs.
///
/// The merge is an owner rather than a free function because it holds the
/// readers, their decoded batches, and the row encoder across calls: each
/// [`Self::next_batch`] resumes exactly where the previous one stopped.
pub struct StagedRunMerge {
    /// Physical schema every run and every emitted batch shares.
    schema: SchemaRef,
    /// Encoder turning sort-key columns into byte-comparable rows.
    converter: RowConverter,
    /// Sort-key column indices in key order, tie-breakers last.
    key_columns: Vec<usize>,
    /// One cursor per staged run, in claim order.
    cursors: Vec<RunCursor>,
    /// Maximum rows one emitted batch may carry.
    batch_rows: usize,
}

impl StagedRunMerge {
    /// Opens every run of one claim and positions it on its first row.
    ///
    /// The schema is taken from the caller rather than from the first run so a
    /// member encoded under a different physical schema is refused here, before
    /// its rows can reach an object that claims the key's fingerprint.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::Internal`] when a run cannot be opened or read,
    /// a run's schema is not the claim's schema, a sort key or stable
    /// tie-breaker is missing from that schema, the row encoder cannot be built
    /// for the key's types, or `batch_rows` is zero.
    pub fn open(
        runs: &[PathBuf],
        schema: SchemaRef,
        layout: &PhysicalLayout,
        batch_rows: usize,
    ) -> Result<Self, ScribeError> {
        if batch_rows == 0 {
            return Err(ScribeError::Internal {
                detail: "staged run merge requires a positive batch size".to_owned(),
            });
        }
        let mut key_columns =
            Vec::with_capacity(layout.sort_keys().len() + STABLE_TIE_BREAKERS.len());
        let mut fields = Vec::with_capacity(key_columns.capacity());
        for key in layout.sort_keys() {
            let index = column_index(&schema, key.column())?;
            key_columns.push(index);
            fields.push(SortField::new_with_options(
                schema.field(index).data_type().clone(),
                arrow::compute::SortOptions {
                    descending: key.is_descending(),
                    nulls_first: key.nulls_first(),
                },
            ));
        }
        for name in STABLE_TIE_BREAKERS {
            let index = column_index(&schema, name)?;
            key_columns.push(index);
            fields.push(SortField::new(schema.field(index).data_type().clone()));
        }
        let converter = RowConverter::new(fields).map_err(|error| ScribeError::Internal {
            detail: format!("build the staged merge row encoder: {error}"),
        })?;
        let mut merge = Self {
            schema,
            converter,
            key_columns,
            cursors: Vec::with_capacity(runs.len()),
            batch_rows,
        };
        for run in runs {
            if let Some(cursor) = merge.open_cursor(run)? {
                merge.cursors.push(cursor);
            }
        }
        Ok(merge)
    }

    /// Returns the next globally ordered batch, or `None` when every run is drained.
    ///
    /// A batch ends at `batch_rows`, or earlier when a run must decode its next
    /// batch: the emitted batch is built by interleaving the cursors' current
    /// batches, so a refill would invalidate the rows already chosen from the
    /// batch it replaces. Ending early is therefore a bounded-memory property,
    /// not a partial result — the caller simply receives more, smaller batches.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::Internal`] when a run cannot be decoded, its
    /// schema changes mid-run, its sort key cannot be encoded, or the merged
    /// rows cannot be interleaved into an output batch.
    pub fn next_batch(&mut self) -> Result<Option<RecordBatch>, ScribeError> {
        let mut picks: Vec<(usize, usize)> = Vec::with_capacity(self.batch_rows);
        while picks.len() < self.batch_rows {
            if let Some(cursor) = self.cursor_needing_refill() {
                if !picks.is_empty() {
                    break;
                }
                self.refill(cursor)?;
                continue;
            }
            let Some(cursor) = self.next_cursor() else {
                break;
            };
            let row = self.cursors[cursor].index;
            picks.push((cursor, row));
            self.cursors[cursor].index += 1;
        }
        if picks.is_empty() {
            return Ok(None);
        }
        let batches: Vec<&RecordBatch> = self.cursors.iter().map(|cursor| &cursor.batch).collect();
        interleave_record_batch(&batches, &picks)
            .map(Some)
            .map_err(|error| ScribeError::Internal {
                detail: format!("interleave the merged staged rows: {error}"),
            })
    }

    /// Returns a cursor whose current batch is spent but whose run is not.
    ///
    /// Refilling is separated from choosing the next row so it can never happen
    /// while chosen rows still point into the batch being replaced.
    fn cursor_needing_refill(&self) -> Option<usize> {
        self.cursors
            .iter()
            .position(|cursor| !cursor.drained && cursor.index >= cursor.batch.num_rows())
    }

    /// Returns the cursor offering the smallest remaining row.
    ///
    /// The scan is linear in the number of runs because a claim merges the
    /// members of one assembly key, which is bounded by the assembler's claim
    /// membership rather than by the table's size; a heap would add ordering
    /// state without removing that bound.
    fn next_cursor(&self) -> Option<usize> {
        let mut best: Option<usize> = None;
        for (index, cursor) in self.cursors.iter().enumerate() {
            if cursor.is_exhausted() || cursor.index >= cursor.batch.num_rows() {
                continue;
            }
            match best {
                Some(current) if self.cursors[current].peek() <= cursor.peek() => {}
                _ => best = Some(index),
            }
        }
        best
    }

    /// Decodes one cursor's next batch, marking the run drained when it ends.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::Internal`] when the run cannot be decoded, its
    /// schema changes mid-run, or its sort key cannot be encoded.
    fn refill(&mut self, cursor: usize) -> Result<(), ScribeError> {
        let Some(batch) = decode_next(&mut self.cursors[cursor].reader)? else {
            self.cursors[cursor].drained = true;
            return Ok(());
        };
        if !self.is_claim_schema(batch.schema_ref()) {
            return Err(ScribeError::Internal {
                detail: "staged run changed schema between batches".to_owned(),
            });
        }
        let rows = self.encode_keys(&batch)?;
        self.cursors[cursor].batch = batch;
        self.cursors[cursor].rows = rows;
        self.cursors[cursor].index = 0;
        Ok(())
    }

    /// Reports whether a decoded run carries the claim's physical schema.
    ///
    /// The comparison is the same decode-side identity ingress admitted the
    /// batch under, so a run cannot be refused here for a difference admission
    /// already deemed irrelevant. `SchemaFingerprint::from_arrow_schema_exact`
    /// commits field names, order, nullability, and each type's exact variant
    /// and parameters — everything that changes an Arrow buffer — and excludes
    /// field metadata, which changes no buffer and whose map iteration order is
    /// not stable.
    fn is_claim_schema(&self, schema: &SchemaRef) -> bool {
        SchemaFingerprint::from_arrow_schema_exact(schema)
            == SchemaFingerprint::from_arrow_schema_exact(&self.schema)
    }

    /// Opens one run and positions it on its first row.
    ///
    /// A run that decodes no rows is dropped rather than carried as an empty
    /// cursor: the staged writer never seals an empty run, so this only absorbs
    /// a caller passing a run list that outlived its rows.
    ///
    /// Schema identity is [`Self::is_claim_schema`], not whole-schema equality:
    /// metadata is publication evidence, not column identity, and a claim's
    /// schema may carry the writer-v2 key/value envelope a sealed run's footer
    /// records while a batch decoded back out of Parquet carries none.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::Internal`] when the run cannot be opened or
    /// decoded, it does not carry the claim's physical schema, or its sort key
    /// cannot be encoded.
    fn open_cursor(&mut self, run: &Path) -> Result<Option<RunCursor>, ScribeError> {
        let file = std::fs::File::open(run).map_err(|error| ScribeError::Internal {
            detail: format!("open the staged run for merge: {error}"),
        })?;
        let mut reader = ParquetRecordBatchReaderBuilder::try_new(file)
            .map_err(|error| ScribeError::Internal {
                detail: format!("build the staged run merge reader: {error}"),
            })?
            .with_batch_size(self.batch_rows)
            .build()
            .map_err(|error| ScribeError::Internal {
                detail: format!("start the staged run merge read: {error}"),
            })?;
        let Some(batch) = decode_next(&mut reader)? else {
            return Ok(None);
        };
        if !self.is_claim_schema(batch.schema_ref()) {
            return Err(ScribeError::Internal {
                detail: format!(
                    "staged run `{}` was encoded under a different physical schema: run [{}], claim [{}]",
                    run.display(),
                    field_names(batch.schema_ref()),
                    field_names(&self.schema)
                ),
            });
        }
        let rows = self.encode_keys(&batch)?;
        Ok(Some(RunCursor {
            reader,
            batch,
            rows,
            index: 0,
            drained: false,
        }))
    }

    /// Encodes one batch's sort key into byte-comparable rows.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::Internal`] when the key columns cannot be encoded
    /// under the converter's fields.
    fn encode_keys(&self, batch: &RecordBatch) -> Result<Rows, ScribeError> {
        let columns: Vec<_> = self
            .key_columns
            .iter()
            .map(|index| Arc::clone(batch.column(*index)))
            .collect();
        self.converter
            .convert_columns(&columns)
            .map_err(|error| ScribeError::Internal {
                detail: format!("encode the staged merge sort key: {error}"),
            })
    }
}

impl Iterator for StagedRunMerge {
    type Item = Result<RecordBatch, ScribeError>;

    /// Yields the next ordered batch so the merge can drive the claim writer.
    ///
    /// A merge failure ends the iteration by surfacing as the item: the writer
    /// must stop at the first unreadable run rather than seal an object that
    /// silently drops its rows.
    fn next(&mut self) -> Option<Self::Item> {
        self.next_batch().transpose()
    }
}

impl std::fmt::Debug for StagedRunMerge {
    /// Names the merge by its width rather than its decoded rows.
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("StagedRunMerge")
            .field("schema", &self.schema)
            .field("key_columns", &self.key_columns)
            .field("runs", &self.cursors.len())
            .field("batch_rows", &self.batch_rows)
            .finish_non_exhaustive()
    }
}

/// Resolves one sort-key column, naming the schema that lacks it.
///
/// # Errors
///
/// Returns [`ScribeError::Internal`] when the column is absent, which means the
/// claim's schema and its layout disagree.
fn column_index(schema: &SchemaRef, column: &str) -> Result<usize, ScribeError> {
    schema.index_of(column).map_err(|_| ScribeError::Internal {
        detail: format!("staged merge sort column `{column}` is not in the claim schema"),
    })
}

/// Decodes one run's next batch, skipping empty batches.
///
/// # Errors
///
/// Returns [`ScribeError::Internal`] when the reader fails to decode.
fn decode_next(reader: &mut ParquetRecordBatchReader) -> Result<Option<RecordBatch>, ScribeError> {
    loop {
        let Some(batch) = reader
            .next()
            .transpose()
            .map_err(|error| ScribeError::Internal {
                detail: format!("decode the staged run merge batch: {error}"),
            })?
        else {
            return Ok(None);
        };
        if batch.num_rows() > 0 {
            return Ok(Some(batch));
        }
    }
}

/// Renders every field property a schema comparison can differ on.
///
/// A merge that refuses a run needs to say how the two schemas differ, and the
/// full `Debug` of an Arrow schema is too large to read in a log line. Arrow
/// compares names, types, nullability, and field metadata, so all four appear
/// here: a message that printed only names and types would show two identical
/// lists for a run refused over a nullability or metadata difference.
fn field_names(schema: &arrow::datatypes::Schema) -> String {
    let fields = schema
        .fields()
        .iter()
        .map(|field| {
            let mut rendered = format!("{}:{}", field.name(), field.data_type());
            if field.is_nullable() {
                rendered.push_str(":null");
            }
            if !field.metadata().is_empty() {
                let mut keys = field.metadata().keys().cloned().collect::<Vec<_>>();
                keys.sort_unstable();
                rendered.push_str(":meta{");
                rendered.push_str(&keys.join(","));
                rendered.push('}');
            }
            rendered
        })
        .collect::<Vec<_>>()
        .join(", ");
    if schema.metadata().is_empty() {
        return fields;
    }
    let mut keys = schema.metadata().keys().cloned().collect::<Vec<_>>();
    keys.sort_unstable();
    format!("{fields} | schema meta {{{}}}", keys.join(","))
}

#[cfg(test)]
mod tests {
    use super::*;
    use arrow::array::{FixedSizeBinaryArray, Int32Array, TimestampMicrosecondArray};
    use arrow::datatypes::{DataType, Field, Schema, TimeUnit};

    /// Builds the physical schema every merge fixture run is written under.
    fn merge_schema() -> SchemaRef {
        Arc::new(Schema::new(vec![
            Field::new(
                "wyrd_event_time",
                DataType::Timestamp(TimeUnit::Microsecond, None),
                false,
            ),
            Field::new("wyrd_batch_id", DataType::FixedSizeBinary(16), false),
            Field::new("wyrd_row_ordinal", DataType::Int32, false),
        ]))
    }

    /// Resolves the hourly layout the fixtures are sorted and merged under.
    fn merge_layout(schema: &Schema) -> PhysicalLayout {
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

    /// Writes one already sorted run and returns its path.
    ///
    /// Rows are given as `(event_time, batch_id_byte, row_ordinal)` so a test
    /// can place an exact tie on `wyrd_event_time` and state which side of it
    /// the stable tie-breakers must order first.
    fn write_run(directory: &Path, name: &str, rows: &[(i64, u8, i32)]) -> PathBuf {
        let schema = merge_schema();
        let times = TimestampMicrosecondArray::from_iter_values(rows.iter().map(|row| row.0));
        let batch_ids = FixedSizeBinaryArray::try_from_iter(rows.iter().map(|row| [row.1; 16]))
            .expect("fixture batch identity");
        let ordinals = Int32Array::from_iter_values(rows.iter().map(|row| row.2));
        let batch = RecordBatch::try_new(
            Arc::clone(&schema),
            vec![Arc::new(times), Arc::new(batch_ids), Arc::new(ordinals)],
        )
        .expect("fixture run batch");
        let path = directory.join(name);
        let file = std::fs::File::create(&path).expect("fixture run file");
        let mut writer =
            parquet::arrow::ArrowWriter::try_new(file, schema, None).expect("fixture run writer");
        writer.write(&batch).expect("fixture run rows");
        writer.close().expect("fixture run footer");
        path
    }

    /// Drains a merge into the exact `(event_time, batch_id_byte, ordinal)` order it emitted.
    fn drain(merge: &mut StagedRunMerge) -> Vec<(i64, u8, i32)> {
        let mut merged = Vec::new();
        while let Some(batch) = merge.next_batch().expect("merged batch") {
            let times = batch
                .column(0)
                .as_any()
                .downcast_ref::<TimestampMicrosecondArray>()
                .expect("merged event time");
            let batch_ids = batch
                .column(1)
                .as_any()
                .downcast_ref::<FixedSizeBinaryArray>()
                .expect("merged batch identity");
            let ordinals = batch
                .column(2)
                .as_any()
                .downcast_ref::<Int32Array>()
                .expect("merged row ordinal");
            for row in 0..batch.num_rows() {
                merged.push((
                    times.value(row),
                    batch_ids.value(row)[0],
                    ordinals.value(row),
                ));
            }
        }
        merged
    }

    /// The merge is total and deterministic: rows leave in layout order, exact
    /// ties on every layout key are ordered by `(wyrd_batch_id,
    /// wyrd_row_ordinal)`, no row is dropped or duplicated, and re-running the
    /// same claim over the same runs reproduces the identical sequence.
    #[test]
    fn merged_runs_leave_in_one_total_deterministic_order() {
        let directory = tempfile::tempdir().expect("merge fixture root");
        let first = write_run(
            directory.path(),
            "run-0.parquet",
            &[(10, 1, 0), (20, 1, 1), (30, 1, 2), (40, 1, 3)],
        );
        let second = write_run(
            directory.path(),
            "run-1.parquet",
            &[(15, 2, 0), (20, 2, 1), (50, 2, 2)],
        );
        let schema = merge_schema();
        let layout = merge_layout(schema.as_ref());
        let runs = vec![second, first];

        let mut merge =
            StagedRunMerge::open(&runs, Arc::clone(&schema), &layout, 2).expect("merge opens");
        let merged = drain(&mut merge);
        assert_eq!(
            merged,
            vec![
                (10, 1, 0),
                (15, 2, 0),
                (20, 1, 1),
                (20, 2, 1),
                (30, 1, 2),
                (40, 1, 3),
                (50, 2, 2),
            ],
            "the tie at 20 is broken by batch identity, not by claim order"
        );

        let mut replay =
            StagedRunMerge::open(&runs, Arc::clone(&schema), &layout, 3).expect("merge reopens");
        assert_eq!(
            drain(&mut replay),
            merged,
            "a different bounded batch size changes only where batches end"
        );
    }

    /// A run encoded under a different physical schema is refused at open:
    /// merging it would put rows into an object whose footer claims the claim's
    /// schema fingerprint.
    #[test]
    fn a_run_under_another_schema_is_refused() {
        let directory = tempfile::tempdir().expect("merge fixture root");
        let run = write_run(directory.path(), "run-0.parquet", &[(10, 1, 0)]);
        let schema = merge_schema();
        let layout = merge_layout(schema.as_ref());
        let mut foreign_fields = merge_schema().fields().to_vec();
        foreign_fields.push(Arc::new(Field::new("later_column", DataType::Int32, true)));
        let foreign = Arc::new(Schema::new(foreign_fields));

        let refusal = StagedRunMerge::open(&[run], foreign, &layout, 8)
            .expect_err("a foreign schema is refused");
        assert!(
            refusal.to_string().contains("physical schema"),
            "refusal names the schema disagreement: {refusal}"
        );
    }
}
