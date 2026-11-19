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
//! Every emitted batch carries the claim's selected physical Variant layout. A
//! run already staged in that layout is merged as decoded, so its shredded
//! columns reach the writer without reconstruction or reshredding; only a run
//! in another layout is converted, batch by batch, to complete logical values
//! and then to the selected layout.
//!
//! Ordering is the layout's sort keys, then claim run order, then position
//! within a run. That makes the merge total: rows that tie on every layout key
//! still have exactly one order, because
//! a claim's members, and therefore its runs, are in durable claim order and
//! each run is immutable. Re-running an interrupted claim over the same members
//! reproduces the same objects rather than a permutation of them.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use arrow::array::RecordBatch;
use arrow::compute::interleave_record_batch;
use arrow::datatypes::SchemaRef;
use arrow::row::{Row, RowConverter, Rows, SortField};
use iceberg::writer::file_writer::variant_shredding::VariantLayout;
use parquet::arrow::arrow_reader::{ParquetRecordBatchReader, ParquetRecordBatchReaderBuilder};
use parquet::basic::Type as PhysicalType;
use parquet::file::metadata::ParquetMetaData;

use wyrd_spec::DataTenantId;

use crate::catalog::layout::PhysicalLayout;
use crate::contracts::ScribeError;
use crate::parquet::footer::{footer_schema_fingerprint, verify_footer_tenant};
use crate::resources::{ScribeMemoryLease, ScribeResources};
use crate::schema::SchemaFingerprint;
use crate::scribe::claim_assembly::MERGE_BATCH_BYTES;
use crate::scribe::memory::MemoryCategory;

/// One staged run positioned on the row it currently offers to the merge.
///
/// The cursor owns the decoded batch and its comparable row encoding together
/// because both are replaced at the same moment: a refill invalidates every
/// index into the previous batch. Its lease charges both to the Scribe
/// persistence reservation for as long as they are held.
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
    /// Whether the run is staged in another layout and each batch is converted.
    converts: bool,
    /// Estimated decoded bytes of one of this run's rows, from its footer.
    row_bytes: usize,
    /// Bytes reserved before decoding one batch, including conversion overlap.
    workspace: usize,
    /// Persistence reservation for the batch and rows this cursor holds.
    lease: ScribeMemoryLease,
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
    /// Logical schema the claim's runs were sealed under.
    schema: SchemaRef,
    /// Variant layout the claim's sample selected.
    variant_layout: VariantLayout,
    /// Physical schema of `variant_layout` that every emitted batch carries.
    physical: SchemaRef,
    /// Rows decoded from runs in another layout and converted to `physical`.
    converted_rows: usize,
    /// Tenant whose footer a run must carry before its physical layout may differ.
    tenant: DataTenantId,
    /// Encoder turning sort-key columns into byte-comparable rows.
    converter: RowConverter,
    /// Sort-key column indices in key order, the stable tie-breaker last.
    key_columns: Vec<usize>,
    /// One cursor per staged run, in claim order.
    cursors: Vec<RunCursor>,
    /// Maximum rows one emitted or decoded batch may carry.
    batch_rows: usize,
    /// Scribe capability every cursor's workspace is reserved from.
    memory: ScribeResources,
}

impl StagedRunMerge {
    /// Opens every run of one claim and positions it on its first row.
    ///
    /// The schema is taken from the caller rather than from the first run so a
    /// member encoded under a different schema is refused here, before its rows
    /// can reach an object that claims the key's fingerprint. A run may hold
    /// shredded Variant columns — its generation's layout — and is then
    /// accepted only when its footer names the claim's logical schema
    /// fingerprint and `tenant`. Every cursor offers `variant_layout`'s
    /// physical schema: a run whose columns already have exactly those types,
    /// nested field names, nullability and metadata included, is offered as
    /// decoded; any other run is reconstructed to the logical schema and
    /// shredded with `variant_layout` as it is decoded.
    ///
    /// Each run decodes batches of at most `batch_rows` rows shaped to
    /// [`MERGE_BATCH_BYTES`] by its footer's row width. Before a batch is
    /// decoded its cursor reserves that estimate from `memory` — three times
    /// over for a converting run, whose decoded, logical and shredded batches
    /// overlap — and then settles the reservation to the batch and sort-key
    /// rows it holds.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::Internal`] when a run cannot be opened or read,
    /// a run's schema is not the claim's schema, a shredded run's footer names
    /// another logical schema or tenant, `variant_layout` cannot shred the
    /// schema, a sort key or stable tie-breaker is missing from that schema,
    /// the row encoder cannot be built for the key's types, or `batch_rows` is
    /// zero, and [`ScribeError::IngestBusy`] when the shared root cannot hold
    /// a run's workspace.
    pub fn open(
        runs: &[PathBuf],
        schema: SchemaRef,
        variant_layout: VariantLayout,
        tenant: DataTenantId,
        layout: &PhysicalLayout,
        batch_rows: usize,
        memory: &ScribeResources,
    ) -> Result<Self, ScribeError> {
        if batch_rows == 0 {
            return Err(ScribeError::Internal {
                detail: "staged run merge requires a positive batch size".to_owned(),
            });
        }
        let mut key_columns = Vec::with_capacity(layout.sort_keys().len());
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
        let converter = RowConverter::new(fields).map_err(|error| ScribeError::Internal {
            detail: format!("build the staged merge row encoder: {error}"),
        })?;
        let physical =
            variant_layout
                .physical_schema(&schema)
                .map_err(|error| ScribeError::Internal {
                    detail: format!("derive the claim's physical Variant schema: {error}"),
                })?;
        let mut merge = Self {
            schema,
            variant_layout,
            physical,
            converted_rows: 0,
            tenant,
            converter,
            key_columns,
            cursors: Vec::with_capacity(runs.len()),
            batch_rows,
            memory: memory.clone(),
        };
        for run in runs {
            if let Some(cursor) = merge.open_cursor(run)? {
                merge.cursors.push(cursor);
            }
        }
        Ok(merge)
    }

    /// Returns the logical schema the claim's objects are sealed under.
    #[must_use]
    pub fn logical_schema(&self) -> &SchemaRef {
        &self.schema
    }

    /// Returns how many decoded rows were converted from another Variant layout.
    ///
    /// Rows of runs already in the selected layout are never counted, so a
    /// claim whose runs all match converts none.
    #[must_use]
    pub const fn converted_rows(&self) -> usize {
        self.converted_rows
    }

    /// Returns the next globally ordered batch, or `None` when every run is drained.
    ///
    /// A batch ends at `batch_rows` or once its rows' estimated bytes reach
    /// [`MERGE_BATCH_BYTES`], or earlier when a run must decode its next
    /// batch: the emitted batch is built by interleaving the cursors' current
    /// batches, so a refill would invalidate the rows already chosen from the
    /// batch it replaces. Ending early is therefore a bounded-memory property,
    /// not a partial result — the caller simply receives more, smaller batches.
    /// The caller charges the emitted batch while it holds it.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::Internal`] when a run cannot be decoded, its
    /// schema changes mid-run, its sort key cannot be encoded, or the merged
    /// rows cannot be interleaved into an output batch, and
    /// [`ScribeError::IngestBusy`] when a refill's workspace cannot be reserved.
    pub fn next_batch(&mut self) -> Result<Option<RecordBatch>, ScribeError> {
        let mut picks: Vec<(usize, usize)> = Vec::with_capacity(self.batch_rows);
        let mut picked_bytes = 0_usize;
        while picks.len() < self.batch_rows && picked_bytes < MERGE_BATCH_BYTES {
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
            picked_bytes = picked_bytes.saturating_add(self.cursors[cursor].row_bytes);
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
    /// An exact tie keeps the earliest cursor in claim order, and a cursor
    /// offers its run's rows in position order, so rows equal on every key
    /// leave in claim run order, then run position. That is
    /// the last level of the merge's total order.
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
    /// The spent batch is released first, then the workspace is reserved,
    /// then the new batch is decoded and the reservation settled to what the
    /// cursor holds; a drained run holds nothing.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::Internal`] when the run cannot be decoded, its
    /// schema changes mid-run, or its sort key cannot be encoded, and
    /// [`ScribeError::IngestBusy`] when the workspace cannot be reserved.
    fn refill(&mut self, cursor: usize) -> Result<(), ScribeError> {
        let converts = self.cursors[cursor].converts;
        let workspace = self.cursors[cursor].workspace;
        self.cursors[cursor].batch = RecordBatch::new_empty(Arc::clone(&self.physical));
        self.cursors[cursor].rows = self.converter.empty_rows(0, 0);
        self.cursors[cursor].lease.resize_maintenance(workspace)?;
        let Some(batch) = decode_next(&mut self.cursors[cursor].reader)? else {
            self.cursors[cursor].drained = true;
            return self.cursors[cursor].lease.resize_maintenance(0);
        };
        let batch = self.physical_batch(batch, converts)?;
        let rows = self.encode_keys(&batch)?;
        self.cursors[cursor]
            .lease
            .resize_maintenance(held_bytes(&batch, &rows))?;
        self.cursors[cursor].batch = batch;
        self.cursors[cursor].rows = rows;
        self.cursors[cursor].index = 0;
        Ok(())
    }

    /// Returns a decoded batch in the claim's selected physical layout.
    ///
    /// A batch of a matching run keeps its columns and takes the physical
    /// schema, which refuses any column whose type differs. A batch of a
    /// converting run is reconstructed to the logical schema, which must be
    /// the claim's, and shredded with the selected layout.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::Internal`] when a matching batch's columns are
    /// not the physical types, a converted batch is not the claim's logical
    /// schema, or a column cannot be reconstructed or shredded.
    fn physical_batch(
        &mut self,
        batch: RecordBatch,
        converts: bool,
    ) -> Result<RecordBatch, ScribeError> {
        let changed = |detail: String| ScribeError::Internal {
            detail: format!("staged run batch does not hold the claim's schema: {detail}"),
        };
        if !converts {
            return RecordBatch::try_new(Arc::clone(&self.physical), batch.columns().to_vec())
                .map_err(|error| changed(error.to_string()));
        }
        let logical = crate::schema::logical_batch(&batch, &self.schema).map_err(|detail| {
            ScribeError::Internal {
                detail: format!("reconstruct a shredded staged run: {detail}"),
            }
        })?;
        if !self.is_claim_schema(logical.schema_ref()) {
            return Err(changed(field_names(logical.schema_ref())));
        }
        let shredded =
            self.variant_layout
                .shred(&logical)
                .map_err(|error| ScribeError::Internal {
                    detail: format!("shred a staged run to the claim's Variant layout: {error}"),
                })?;
        self.converted_rows = self.converted_rows.saturating_add(batch.num_rows());
        RecordBatch::try_new(Arc::clone(&self.physical), shredded.columns().to_vec())
            .map_err(|error| changed(error.to_string()))
    }

    /// Proves a run whose physical schema differs was sealed for this claim's logical schema and tenant.
    ///
    /// The footer fingerprint is the logical schema the writer sealed under,
    /// whatever layout it shredded with, so a matching fingerprint and tenant
    /// make the difference a layout rather than another table's rows.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::Internal`] naming the run when its footer lacks
    /// either field or names another logical schema or tenant.
    fn verify_layout_footer(
        &self,
        run: &Path,
        footer: &parquet::file::metadata::FileMetaData,
    ) -> Result<(), ScribeError> {
        let refuse = |detail: String| ScribeError::Internal {
            detail: format!(
                "staged run `{}` was encoded under a different physical schema: {detail}",
                run.display()
            ),
        };
        let expected = hex::encode(SchemaFingerprint::from_arrow_schema_exact(&self.schema).0);
        if footer_schema_fingerprint(footer).map_err(refuse)? != expected {
            return Err(refuse(format!(
                "its footer names another logical schema than the claim's [{}]",
                field_names(&self.schema)
            )));
        }
        verify_footer_tenant(footer, self.tenant).map_err(refuse)
    }

    /// Reports whether a decoded run carries the claim's logical schema.
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
    /// decoded, it does not carry the claim's logical schema, a run in another
    /// physical layout fails [`Self::verify_layout_footer`], or its sort key
    /// cannot be encoded.
    fn open_cursor(&mut self, run: &Path) -> Result<Option<RunCursor>, ScribeError> {
        let file = std::fs::File::open(run).map_err(|error| ScribeError::Internal {
            detail: format!("open the staged run for merge: {error}"),
        })?;
        let builder = ParquetRecordBatchReaderBuilder::try_new(file).map_err(|error| {
            ScribeError::Internal {
                detail: format!("build the staged run merge reader: {error}"),
            }
        })?;
        if !self.is_claim_schema(builder.schema()) {
            self.verify_layout_footer(run, builder.metadata().file_metadata())?;
        }
        let converts = !builder
            .schema()
            .fields()
            .iter()
            .map(|field| field.data_type())
            .eq(self.physical.fields().iter().map(|field| field.data_type()));
        let (batch_rows, row_bytes) = shaped_batch_rows(builder.metadata(), None, self.batch_rows);
        let workspace = batch_rows
            .saturating_mul(row_bytes)
            .saturating_mul(if converts { 3 } else { 1 });
        let mut lease = self
            .memory
            .try_reserve_maintenance(MemoryCategory::Persistence, workspace)?;
        let mut reader = builder
            .with_batch_size(batch_rows)
            .build()
            .map_err(|error| ScribeError::Internal {
                detail: format!("start the staged run merge read: {error}"),
            })?;
        let Some(batch) = decode_next(&mut reader)? else {
            return Ok(None);
        };
        let batch = self.physical_batch(batch, converts)?;
        let rows = self.encode_keys(&batch)?;
        lease.resize_maintenance(held_bytes(&batch, &rows))?;
        Ok(Some(RunCursor {
            reader,
            batch,
            rows,
            index: 0,
            drained: false,
            converts,
            row_bytes,
            workspace,
            lease,
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

/// Returns the rows one decoded batch of a run may carry and its estimated row width.
///
/// The width is the widest row group's decoded bytes per row, over the
/// projected `roots` when given, rounded up and never zero. A leaf's decoded
/// bytes are its byte-array payload as the footer's size statistics record
/// it before any encoding, plus its values' fixed width — an offset for byte
/// arrays — and never less than its encoded pages, since dictionary encoding
/// can shrink a repeated payload to almost nothing. Rows follow from
/// [`MERGE_BATCH_BYTES`] over that width, between one and `max_rows` or the
/// run's own row count when it is smaller, so the
/// shape is a pure function of the run's footer and a retry decodes the same
/// batches.
pub(crate) fn shaped_batch_rows(
    metadata: &ParquetMetaData,
    roots: Option<&[usize]>,
    max_rows: usize,
) -> (usize, usize) {
    let schema = metadata.file_metadata().schema_descr();
    let row_bytes = metadata
        .row_groups()
        .iter()
        .filter(|group| group.num_rows() > 0)
        .map(|group| {
            let bytes = group
                .columns()
                .iter()
                .enumerate()
                .filter(|(leaf, _)| {
                    roots.is_none_or(|roots| roots.contains(&schema.get_column_root_idx(*leaf)))
                })
                .map(|(_, column)| {
                    let width = match column.column_type() {
                        PhysicalType::BOOLEAN => 1,
                        PhysicalType::INT32 | PhysicalType::FLOAT | PhysicalType::BYTE_ARRAY => 4,
                        PhysicalType::INT64 | PhysicalType::DOUBLE => 8,
                        PhysicalType::INT96 => 12,
                        PhysicalType::FIXED_LEN_BYTE_ARRAY => {
                            u64::try_from(column.column_descr().type_length()).unwrap_or(0)
                        }
                    };
                    let values = u64::try_from(column.num_values()).unwrap_or(0);
                    let payload = column
                        .unencoded_byte_array_data_bytes()
                        .and_then(|bytes| u64::try_from(bytes).ok())
                        .unwrap_or(0);
                    let encoded = u64::try_from(column.uncompressed_size()).unwrap_or(0);
                    values
                        .saturating_mul(width)
                        .saturating_add(payload)
                        .max(encoded)
                })
                .fold(0_u64, u64::saturating_add);
            let rows = u64::try_from(group.num_rows()).unwrap_or(1);
            usize::try_from(bytes.div_ceil(rows)).unwrap_or(usize::MAX)
        })
        .max()
        .unwrap_or(0)
        .max(1);
    let run_rows = usize::try_from(metadata.file_metadata().num_rows()).unwrap_or(usize::MAX);
    let max_rows = max_rows.min(run_rows).max(1);
    (
        (MERGE_BATCH_BYTES / row_bytes).clamp(1, max_rows),
        row_bytes,
    )
}

/// Bytes one cursor holds for a decoded batch and its sort-key rows.
fn held_bytes(batch: &RecordBatch, rows: &Rows) -> usize {
    crate::scribe::memory::retained_arrow_bytes(batch).saturating_add(rows.size())
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
    use arrow::array::{Int32Array, TimestampMicrosecondArray};
    use arrow::datatypes::{DataType, Field, Schema, TimeUnit};

    /// Builds the physical schema every merge fixture run is written under.
    ///
    /// `value` is an ordinary payload column the fixtures use only to tell
    /// otherwise-identical rows apart in the merged output.
    fn merge_schema() -> SchemaRef {
        Arc::new(Schema::new(vec![
            Field::new(
                "event_time",
                DataType::Timestamp(TimeUnit::Microsecond, Some("+00:00".into())),
                false,
            ),
            Field::new("value", DataType::Int32, false),
        ]))
    }

    /// Writes one already sorted run and returns its path.
    ///
    /// Rows are given as `(event_time, value)` so a test can place an exact
    /// tie on `event_time` and name each row in the merged output by its
    /// `value`.
    ///
    /// # Panics
    ///
    /// Panics when the rows cannot form a batch under the merge schema or when the
    /// Parquet file cannot be created, written, or closed.
    fn write_run(directory: &Path, name: &str, rows: &[(i64, i32)]) -> PathBuf {
        let schema = merge_schema();
        let times = TimestampMicrosecondArray::from_iter_values(rows.iter().map(|row| row.0))
            .with_timezone("+00:00");
        let values = Int32Array::from_iter_values(rows.iter().map(|row| row.1));
        let batch =
            RecordBatch::try_new(Arc::clone(&schema), vec![Arc::new(times), Arc::new(values)])
                .expect("fixture run batch");
        let path = directory.join(name);
        let file = std::fs::File::create(&path).expect("fixture run file");
        let mut writer =
            parquet::arrow::ArrowWriter::try_new(file, schema, None).expect("fixture run writer");
        writer.write(&batch).expect("fixture run rows");
        writer.close().expect("fixture run footer");
        path
    }

    /// Drains a merge into the exact `(event_time, value)` order it emitted.
    ///
    /// # Panics
    ///
    /// Panics when the merge fails to produce a batch or when a merged batch's
    /// columns are not the expected timestamp and `Int32` arrays.
    fn drain(merge: &mut StagedRunMerge) -> Vec<(i64, i32)> {
        let mut merged = Vec::new();
        while let Some(batch) = merge.next_batch().expect("merged batch") {
            let times = batch
                .column(0)
                .as_any()
                .downcast_ref::<TimestampMicrosecondArray>()
                .expect("merged event time");
            let values = batch
                .column(1)
                .as_any()
                .downcast_ref::<Int32Array>()
                .expect("merged value");
            for row in 0..batch.num_rows() {
                merged.push((times.value(row), values.value(row)));
            }
        }
        merged
    }

    /// The merge is total and deterministic: rows leave in layout order, an
    /// exact layout tie keeps claim run order and then run position, no row is
    /// dropped or
    /// duplicated, and re-running the same claim over the same runs reproduces
    /// the identical sequence.
    ///
    /// # Panics
    ///
    /// Panics when a fixture run cannot be written, when the merge cannot open,
    /// when the merged order differs from layout order with run-then-position tie
    /// breaking, or when a different batch size changes the sequence.
    #[test]
    fn merged_runs_leave_in_one_total_deterministic_order() {
        let directory = tempfile::tempdir().expect("merge fixture root");
        let first = write_run(
            directory.path(),
            "run-0.parquet",
            &[(10, 0), (20, 1), (25, 10), (25, 11), (40, 3)],
        );
        let second = write_run(
            directory.path(),
            "run-1.parquet",
            &[(15, 100), (20, 101), (25, 20), (50, 102)],
        );
        let schema = merge_schema();
        let layout = crate::tables::hourly_test_layout(schema.as_ref());
        let runs = vec![second, first];

        let mut merge = StagedRunMerge::open(
            &runs,
            Arc::clone(&schema),
            VariantLayout::default(),
            DataTenantId::new_v7(),
            &layout,
            2,
            &ScribeResources::for_test(),
        )
        .expect("merge opens");
        let merged = drain(&mut merge);
        assert_eq!(
            merged,
            vec![
                (10, 0),
                (15, 100),
                (20, 101),
                (20, 1),
                (25, 20),
                (25, 10),
                (25, 11),
                (40, 3),
                (50, 102),
            ],
            "a layout tie keeps claim run order, then position within a run"
        );

        let mut replay = StagedRunMerge::open(
            &runs,
            Arc::clone(&schema),
            VariantLayout::default(),
            DataTenantId::new_v7(),
            &layout,
            3,
            &ScribeResources::for_test(),
        )
        .expect("merge reopens");
        assert_eq!(
            drain(&mut replay),
            merged,
            "a different bounded batch size changes only where batches end"
        );
    }

    /// A run encoded under a different physical schema is refused at open:
    /// merging it would put rows into an object whose footer claims the claim's
    /// schema fingerprint.
    ///
    /// # Panics
    ///
    /// Panics when the fixture run cannot be written, when the merge opens under
    /// the foreign schema, or when the refusal does not name the physical schema
    /// disagreement.
    #[test]
    fn a_run_under_another_schema_is_refused() {
        let directory = tempfile::tempdir().expect("merge fixture root");
        let run = write_run(directory.path(), "run-0.parquet", &[(10, 0)]);
        let schema = merge_schema();
        let layout = crate::tables::hourly_test_layout(schema.as_ref());
        let mut foreign_fields = merge_schema().fields().to_vec();
        foreign_fields.push(Arc::new(Field::new("later_column", DataType::Int32, true)));
        let foreign = Arc::new(Schema::new(foreign_fields));

        let refusal = StagedRunMerge::open(
            &[run],
            foreign,
            VariantLayout::default(),
            DataTenantId::new_v7(),
            &layout,
            8,
            &ScribeResources::for_test(),
        )
        .expect_err("a foreign schema is refused");
        assert!(
            refusal.to_string().contains("physical schema"),
            "refusal names the schema disagreement: {refusal}"
        );
    }
}
