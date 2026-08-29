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
use crate::scribe::tail_rpc::{HotBatch, HotBatchSource};

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
    /// Returns rows from every staged source the caller leased.
    ///
    /// Sources arrive oldest first and are read in that order, so a truncated
    /// read returns the oldest rows rather than an arbitrary subset.
    ///
    /// Nothing is filtered here. A staged source exists only while the
    /// hot-source registry names its generation's runs as the one authority for
    /// those rows; the moment a hot object serves them the registry holds
    /// `Published` instead and the generation is not leased at all. Deciding
    /// again from WAL positions would be a second, weaker answer to a question
    /// the registry has already answered exactly: WAL records are numbered from
    /// one node-global counter while members are sealed per tenant, table,
    /// partition, and shard, so one member's bounds routinely enclose positions
    /// another member owns, and containment is not ownership.
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
                origin: HotBatchSource::StagedMember {
                    member: source.member,
                    wal: source.wal,
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
    use crate::catalog::{TableRef, TimeGranularity, TimePartition};
    use crate::namespaces::BifrostNamespace;
    use crate::scribe::assembly::StagedMemberId;
    use crate::scribe::hot_source::GenerationOrdinal;
    use crate::scribe::seal_key::SealKey;
    use crate::scribe::wal::WalLsn;
    use wyrd_spec::ids::DataTenantId;

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

    /// Builds one staged source over `runs` covering the given WAL range.
    ///
    /// # Panics
    ///
    /// Panics when the fixed partition literal is not a valid day boundary.
    fn source(generation: u64, runs: Vec<PathBuf>, wal: (u64, u64)) -> StagedSource {
        StagedSource {
            key: SealKey::new(
                DataTenantId::new_v7(),
                TableRef::new(BifrostNamespace::Bifrost, "events"),
                TimePartition::new(
                    TimeGranularity::Day,
                    chrono::DateTime::from_timestamp(1_772_150_400, 0)
                        .expect("a fixed representable instant"),
                )
                .expect("a fixed day boundary"),
            ),
            generation: GenerationOrdinal::new(0, generation),
            member: StagedMemberId::new(0, generation),
            runs,
            bytes: 4_096,
            wal: (WalLsn::new(wal.0), WalLsn::new(wal.1)),
        }
    }

    /// Builds an unbounded read requesting `columns`.
    fn unbounded(columns: &[String]) -> StagedTailRead<'_> {
        StagedTailRead {
            required_columns: columns,
            limits: ReadableBatchLimits {
                max_batches: usize::MAX,
                max_retained_bytes: usize::MAX,
            },
        }
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
        let batches = StagedTailReader::default()
            .read(&[source(1, vec![run], (1, 9))], &unbounded(&columns))
            .expect("the staged run reads");

        assert_eq!(batches.len(), 1);
        let rows = &batches[0].rows;
        assert_eq!(rows.num_columns(), 2);
        assert_eq!(rows.schema().field(0).name(), "label");
        assert_eq!(rows.schema().field(1).name(), "ordinal");
        assert_eq!(
            batches[0].origin,
            HotBatchSource::StagedMember {
                member: StagedMemberId::new(0, 1),
                wal: (WalLsn::new(1), WalLsn::new(9)),
            }
        );
    }

    /// A read stops at the caller's batch ceiling instead of draining a member.
    ///
    /// The ceiling is what keeps one large staged member from materializing
    /// unboundedly into a query, and the oldest source is read first so the
    /// truncated answer is a prefix rather than an arbitrary subset.
    ///
    /// # Panics
    ///
    /// Panics when the reader returns more batches than the ceiling allows or
    /// serves them out of source order.
    #[test]
    fn a_staged_read_stops_at_the_caller_batch_ceiling_oldest_first() {
        let root = tempfile::tempdir().expect("staged root");
        let first = write_run(root.path(), "run-1.parquet", 2);
        let second = write_run(root.path(), "run-2.parquet", 2);
        let columns = Vec::new();
        let read = StagedTailRead {
            required_columns: &columns,
            limits: ReadableBatchLimits {
                max_batches: 1,
                max_retained_bytes: usize::MAX,
            },
        };
        let batches = StagedTailReader::default()
            .read(
                &[
                    source(1, vec![first], (1, 9)),
                    source(2, vec![second], (10, 19)),
                ],
                &read,
            )
            .expect("the staged runs read");

        assert_eq!(batches.len(), 1);
        assert_eq!(
            batches[0].origin,
            HotBatchSource::StagedMember {
                member: StagedMemberId::new(0, 1),
                wal: (WalLsn::new(1), WalLsn::new(9)),
            },
            "the oldest source must be the one the truncated read returns"
        );
    }

    /// A member whose WAL range another member's bounds enclose is still read.
    ///
    /// WAL records are numbered from one node-global counter while members are
    /// sealed per tenant, table, partition, and shard, so a member covering
    /// records 1 through 30 routinely encloses a different member's 12 through
    /// 20 without owning a single one of its rows. The reader serves what it was
    /// leased; publication is decided by the hot-source registry, which never
    /// leases a generation a hot object already serves.
    ///
    /// # Panics
    ///
    /// Panics when an enclosed member's rows are dropped, which is how a
    /// numeric-containment inference shows up: acknowledged rows vanish from a
    /// strict read while the enclosing object never carried them.
    #[test]
    fn an_enclosed_member_is_read_because_containment_is_not_ownership() {
        let root = tempfile::tempdir().expect("staged root");
        let enclosing = write_run(root.path(), "enclosing.parquet", 2);
        let enclosed = write_run(root.path(), "enclosed.parquet", 2);
        let columns = Vec::new();
        let batches = StagedTailReader::default()
            .read(
                &[
                    source(1, vec![enclosing], (1, 30)),
                    source(2, vec![enclosed], (12, 20)),
                ],
                &unbounded(&columns),
            )
            .expect("the staged runs read");

        assert_eq!(
            batches.iter().map(|batch| batch.origin).collect::<Vec<_>>(),
            vec![
                HotBatchSource::StagedMember {
                    member: StagedMemberId::new(0, 1),
                    wal: (WalLsn::new(1), WalLsn::new(30)),
                },
                HotBatchSource::StagedMember {
                    member: StagedMemberId::new(0, 2),
                    wal: (WalLsn::new(12), WalLsn::new(20)),
                },
            ],
            "an enclosed member owns its own rows and must be served"
        );
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
        let error = StagedTailReader::default()
            .read(&[source(1, vec![run], (1, 9))], &unbounded(&columns))
            .expect_err("a column the run does not carry is refused");

        let detail = error.to_string();
        assert!(detail.contains("absent"), "{detail}");
        assert!(detail.contains("run-1.parquet"), "{detail}");
    }
}
