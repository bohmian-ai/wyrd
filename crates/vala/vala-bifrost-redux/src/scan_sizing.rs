//! Sizes an Oracle scan: how many partitions it runs with, from the bytes it
//! reads.
//!
//! One module owns every rule the sizing uses, so the Oracle leader, the
//! Scribe listing that reports live bytes, and the Scribe follower that
//! executes the live leaf cannot disagree:
//!
//! - [`OraclePartitioning`] turns the pod's CPU and scanned bytes into a
//!   partition count; the Oracle and Scribe handles reach it through their
//!   resources.
//! - [`ScanScope`] names the columns and filters one scan reads, and
//!   `ScanScope::covers` is the one rule for which columns count.
//! - `BatchPruning` rules out whole batches, or Parquet row groups, by their
//!   recorded bounds.
//! - `ScanSizer` is the one measure of the bytes a scan reads, on every
//!   tier: memtable batches, staged runs, hot files, and published files.

use std::sync::Arc;

use num_traits::ToPrimitive;

use crate::resources::BifrostResourceError;

/// Fewest execution partitions any admitted Oracle query receives.
///
/// A single partition removes intra-query parallelism entirely, so even a one-core pod
/// plans two.
pub const ORACLE_MIN_TARGET_PARTITIONS: usize = 2;

/// Computes the `DataFusion` target partitions for one Oracle session.
///
/// A fully local scan runs one
/// partition per core because it has no IO latency to hide, and each remote
/// byte raises that linearly toward four partitions per core so remote reads
/// overlap. The pinned file count plays no part here; a Parquet leaf splits
/// its files by byte range into these partitions, capped at the row groups
/// left after file pruning when their footers are known. Memory plays no
/// part: the query's limit bounds only what the partitions may hold.
///
/// # Errors
///
/// Returns [`BifrostResourceError::InvalidPlan`] for zero CPU or non-finite or
/// out-of-range locality, and an overflow error for checked `cpu * 4` overflow.
pub fn oracle_target_partitions(
    effective_cpu: usize,
    local_ratio: f64,
) -> Result<usize, BifrostResourceError> {
    if effective_cpu == 0 || !local_ratio.is_finite() || !(0.0..=1.0).contains(&local_ratio) {
        return Err(BifrostResourceError::InvalidPlan {
            detail: "Oracle partition inputs must have positive CPU and locality in [0, 1]"
                .to_owned(),
        });
    }
    let query_threads = effective_cpu
        .checked_mul(4)
        .ok_or_else(crate::resources::accounting_overflow)?;
    let remote_span = query_threads - effective_cpu;
    let remote_partitions = remote_span
        .to_f64()
        .map(|span| (span * (1.0 - local_ratio)).trunc())
        .and_then(|partitions| partitions.to_usize())
        .ok_or_else(crate::resources::accounting_overflow)?;
    let partitions = effective_cpu
        .checked_add(remote_partitions)
        .ok_or_else(crate::resources::accounting_overflow)?;
    Ok(partitions.max(ORACLE_MIN_TARGET_PARTITIONS))
}

/// Default scanned bytes that earn one Oracle execution partition.
///
/// The same order as `ClickHouse`'s 24 MiB minimum for a concurrent read, and
/// between `DataFusion`'s 10 MiB file-split minimum and Trino's 32–64 MiB
/// splits: below it, a partition's fixed per-query cost (task, repartition
/// buffers, metrics) outweighs the decode work it takes over. Deployments
/// override it through
/// [`BifrostResourcePolicy::oracle_partition_scan_bytes`](crate::resources::BifrostResourcePolicy::oracle_partition_scan_bytes).
pub const ORACLE_PARTITION_SCAN_BYTES: u64 = 24 * 1024 * 1024;

/// The one owner of how many partitions an Oracle query, or the live
/// fragment it dispatches, executes with.
///
/// Resolved once from the pod's [`ResourcePlan`](crate::resources::ResourcePlan) and shared by the Oracle
/// leader and the Scribe follower, so both size by the same CPU cap and the
/// same bytes per partition.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OraclePartitioning {
    /// The pod's effective CPU, which caps a query's partitions.
    effective_cpu: usize,
    /// Scanned bytes that earn one partition; always positive.
    scan_bytes_per_partition: u64,
}

impl OraclePartitioning {
    /// Builds the sizing for a pod of `effective_cpu` cores where every
    /// `scan_bytes_per_partition` scanned bytes earn one partition.
    ///
    /// [`ResourcePlan::oracle_partitioning`](crate::resources::ResourcePlan::oracle_partitioning) is the production constructor;
    /// focused tests that compose no resource plan call this directly.
    #[must_use]
    pub(crate) const fn new(effective_cpu: usize, scan_bytes_per_partition: u64) -> Self {
        Self {
            effective_cpu,
            scan_bytes_per_partition,
        }
    }

    /// Returns the most partitions a query plans with for input of
    /// `local_ratio` locality; see [`oracle_target_partitions`].
    ///
    /// # Errors
    ///
    /// Returns [`BifrostResourceError::InvalidPlan`] for zero CPU or locality
    /// outside `[0, 1]`, and an overflow error for an unrepresentable CPU
    /// count.
    pub fn target_partitions(&self, local_ratio: f64) -> Result<usize, BifrostResourceError> {
        oracle_target_partitions(self.effective_cpu, local_ratio)
    }

    /// Returns the partitions a scan of `scan_bytes` earns, at least one and
    /// at most `cap`.
    ///
    /// One partition per `scan_bytes_per_partition` started: a point lookup
    /// or small aggregate runs as one stream with no fan-out, and a scan large
    /// enough to keep every core busy keeps `cap`. One partition also leaves
    /// the distributed planner no exchange to cut, so a small scan stays
    /// local.
    #[must_use]
    pub fn scan_partitions(&self, scan_bytes: u64, cap: usize) -> usize {
        let chunks = scan_bytes.div_ceil(self.scan_bytes_per_partition);
        usize::try_from(chunks)
            .unwrap_or(usize::MAX)
            .clamp(1, cap.max(1))
    }
}

/// What one scan reads from a table: the columns it projects and the filter
/// conjuncts that reach it.
///
/// Oracle builds it from the optimized logical plan and sizes the scan's
/// published bytes from cached Parquet footers with it; the Scribe listing
/// receives it over the private RPC and reports the live bytes it reads. Both
/// prune by `pruning` and count columns by `covers`. It is a
/// hint for sizing a plan, never for answering it: a conjunct that cannot be
/// planned is dropped, which can only keep more bytes.
#[derive(Debug, Clone, Default)]
pub struct ScanScope {
    /// Columns the query reads; empty means every column.
    pub columns: Vec<String>,
    /// Filter conjuncts the query applies to this table.
    pub filters: Vec<datafusion::logical_expr::Expr>,
}

impl ScanScope {
    /// Encodes the scope for the private listing RPC.
    ///
    /// Each conjunct is encoded alone with `datafusion-proto`, so one that
    /// cannot be serialized drops out instead of discarding the rest.
    #[must_use]
    pub fn to_wire(&self) -> (Vec<String>, Vec<Vec<u8>>) {
        use datafusion_proto::bytes::Serializeable as _;
        let filters = self
            .filters
            .iter()
            .filter_map(|filter| filter.to_bytes().ok().map(|bytes| bytes.to_vec()))
            .collect();
        (self.columns.clone(), filters)
    }

    /// Decodes a scope received on the private listing RPC.
    ///
    /// Conjuncts decode against Oracle's default function registry; one that
    /// does not decode is dropped.
    #[must_use]
    pub fn from_wire(columns: Vec<String>, filters: &[Vec<u8>]) -> Self {
        let filters = filters
            .iter()
            .filter_map(|bytes| {
                crate::oracle::session::OracleSessionTemplate::shared()
                    .decode_expr(bytes)
                    .ok()
            })
            .collect();
        Self { columns, filters }
    }

    /// Plans this scope's filters as a batch pruning over `schema`, or `None`
    /// when no conjunct plans against it.
    ///
    /// Each conjunct is planned alone, so a conjunct naming a column `schema`
    /// lacks drops out, recorded as [`SizingFallback::DroppedConjunct`], and
    /// the rest still prune. A logical plan qualifies its
    /// columns with the table name and `schema` is unqualified, so each
    /// conjunct's columns are unqualified first.
    #[must_use]
    fn pruning(&self, schema: &arrow::datatypes::SchemaRef) -> Option<BatchPruning> {
        let df_schema = datafusion::common::DFSchema::try_from(Arc::clone(schema)).ok()?;
        let props = datafusion::execution::context::ExecutionProps::new();
        let planning =
            datafusion::logical_expr::physical_planning_context::PhysicalPlanningContext::default();
        let conjuncts: Vec<_> = self
            .filters
            .iter()
            .filter_map(|filter| {
                datafusion::physical_expr::create_physical_expr(
                    &datafusion::logical_expr::expr_rewriter::unnormalize_col(filter.clone()),
                    &df_schema,
                    &props,
                    &planning,
                )
                .inspect_err(|error| {
                    record_fallback(SizingFallback::DroppedConjunct, &error.to_string());
                })
                .ok()
            })
            .collect();
        if conjuncts.is_empty() {
            return None;
        }
        BatchPruning::try_new(
            schema,
            datafusion::physical_expr::utils::conjunction(conjuncts),
        )
    }

    /// Returns whether the query reads `column`; an empty scope reads every
    /// column.
    ///
    /// This is the one column rule for scan sizing: [`ScanSizer`] counts a
    /// Parquet column chunk whose top-level column is covered, and a memtable
    /// batch's covered columns.
    #[must_use]
    fn covers(&self, column: &str) -> bool {
        self.columns.is_empty() || self.columns.iter().any(|name| name == column)
    }
}

/// The one owner of how many bytes a scan reads, on every tier.
///
/// A scan reads units, each with recorded bounds: a memtable batch, or a
/// Parquet row group of a staged run, hot file, or published file. The sizer
/// plans the scope's filters once, drops every unit the bounds rule out, and
/// counts the covered columns of the rest in decoded bytes: a batch column's
/// Arrow data, recorded once when Scribe appends it, and a column chunk's
/// uncompressed size. Decoded, because a
/// partition's work is decoding, so one partition size means the same work
/// on every tier. Oracle sizes published and hot files with it, the Scribe
/// listing its memtable batches and staged runs, and the Scribe follower its
/// memtable leaf.
pub(crate) struct ScanSizer<'scope> {
    /// Columns and filters of the scan being sized.
    scope: &'scope ScanScope,
    /// Schema the filters were planned against.
    schema: arrow::datatypes::SchemaRef,
    /// The scope's filters planned once against the table schema.
    pruning: Option<BatchPruning>,
    /// Indexes of the sized schema's columns the scope covers, in the order
    /// a memtable batch's column statistics are recorded.
    covered: Vec<usize>,
    /// Throwaway metrics the row-group pruner requires; never reported.
    metrics: datafusion::datasource::physical_plan::parquet::ParquetFileMetrics,
}

impl<'scope> ScanSizer<'scope> {
    /// Plans `scope`'s filters against `schema`, the schema the sized batches
    /// or files were written under.
    #[must_use]
    pub(crate) fn new(scope: &'scope ScanScope, schema: &arrow::datatypes::SchemaRef) -> Self {
        let metrics = datafusion::physical_plan::metrics::ExecutionPlanMetricsSet::new();
        Self {
            scope,
            schema: Arc::clone(schema),
            pruning: scope.pruning(schema),
            covered: schema
                .fields()
                .iter()
                .enumerate()
                .filter(|(_, field)| scope.covers(field.name()))
                .map(|(index, _)| index)
                .collect(),
            metrics: datafusion::datasource::physical_plan::parquet::ParquetFileMetrics::new(
                0,
                "scan_sizing",
                &metrics,
            ),
        }
    }

    /// Returns the uncompressed bytes of the scope's columns in the row
    /// groups of `footer` the filters cannot rule out, and how many row
    /// groups those are.
    ///
    /// A footer whose schema does not convert to Arrow, or a scope with no
    /// plannable filter, keeps every row group. A footer written under another
    /// schema revision has the filters planned against its own columns, so a
    /// filter on a column it lacks drops out instead of misreading another. A column chunk counts when
    /// its top-level column is covered, so a Variant column counts every
    /// shredded child.
    #[must_use]
    pub(crate) fn footer_size(
        &self,
        footer: &parquet::file::metadata::ParquetMetaData,
    ) -> ScanSize {
        use datafusion::datasource::physical_plan::parquet::{
            ParquetAccessPlan, RowGroupAccessPlanFilter,
        };
        let mut access =
            RowGroupAccessPlanFilter::new(ParquetAccessPlan::new_all(footer.num_row_groups()));
        let file_schema = parquet::arrow::parquet_to_arrow_schema(
            footer.file_metadata().schema_descr(),
            footer.file_metadata().key_value_metadata(),
        );
        if let Ok(file_schema) = file_schema {
            let revised;
            let pruning = if file_schema
                .fields()
                .iter()
                .map(|field| field.name())
                .eq(self.schema.fields().iter().map(|field| field.name()))
            {
                self.pruning.as_ref()
            } else {
                revised = self.scope.pruning(&Arc::new(file_schema.clone()));
                revised.as_ref()
            };
            if let Some(pruning) = pruning {
                access.prune_by_statistics_with_metadata(
                    &file_schema,
                    footer,
                    pruning.predicate(),
                    &self.metrics,
                );
            }
        }
        let kept: Vec<usize> = access.row_group_indexes().collect();
        let bytes = kept
            .iter()
            .flat_map(|index| footer.row_group(*index).columns())
            .filter(|chunk| {
                chunk
                    .column_path()
                    .parts()
                    .first()
                    .is_some_and(|root| self.scope.covers(root))
            })
            .map(|chunk| u64::try_from(chunk.uncompressed_size()).unwrap_or(0))
            .fold(0_u64, u64::saturating_add);
        ScanSize {
            bytes,
            row_groups: u64::try_from(kept.len()).unwrap_or(u64::MAX),
        }
    }

    /// Returns, per unit of `units` many, whether its recorded `statistics`
    /// leave it possibly matching the scope's filters.
    ///
    /// A unit is a memtable batch or a whole file with its declared bounds;
    /// no plannable filter, or statistics not index-aligned with the units,
    /// keeps every unit.
    #[must_use]
    pub(crate) fn keep(
        &self,
        statistics: &[Arc<datafusion::common::Statistics>],
        units: usize,
    ) -> Vec<bool> {
        self.pruning.as_ref().map_or_else(
            || vec![true; units],
            |pruning| pruning.keep(statistics, units),
        )
    }

    /// Returns the size of one Parquet file: its footer's
    /// [`Self::footer_size`], or `on_disk` bytes and no known row group when
    /// its footer could not be read, recorded as
    /// [`SizingFallback::UnreadFooter`].
    ///
    /// The scan reports an unreadable file itself; sizing it by its whole
    /// size only keeps more bytes.
    #[must_use]
    pub(crate) fn file_size(
        &self,
        footer: Option<&parquet::file::metadata::ParquetMetaData>,
        on_disk: u64,
    ) -> ScanSize {
        footer.map_or_else(
            || {
                record_fallback(SizingFallback::UnreadFooter, "footer not cached");
                ScanSize {
                    bytes: on_disk,
                    row_groups: 0,
                }
            },
            |footer| self.footer_size(footer),
        )
    }

    /// Returns the decoded bytes of the scope's columns in the memtable
    /// batches whose recorded `statistics` the filters cannot rule out.
    ///
    /// `statistics` holds one entry per batch, as Scribe records them on
    /// append with each column's [`ColumnStatistics::byte_size`], so sizing
    /// sums stored numbers and never walks a batch. [`Self::keep`] decides
    /// which batches count. A covered column without a recorded size counts
    /// nothing and is recorded as [`SizingFallback::UnsizedColumn`].
    ///
    /// [`ColumnStatistics::byte_size`]: datafusion::common::ColumnStatistics::byte_size
    #[must_use]
    pub(crate) fn batch_bytes(&self, statistics: &[Arc<datafusion::common::Statistics>]) -> u64 {
        statistics
            .iter()
            .zip(self.keep(statistics, statistics.len()))
            .filter(|(_, keep)| *keep)
            .flat_map(|(batch, _)| {
                self.covered
                    .iter()
                    .map(|index| batch.column_statistics.get(*index))
            })
            .map(|column| {
                column
                    .and_then(|column| column.byte_size.get_value())
                    .map_or_else(
                        || {
                            record_fallback(SizingFallback::UnsizedColumn, "no recorded byte size");
                            0
                        },
                        |bytes| u64::try_from(*bytes).unwrap_or(u64::MAX),
                    )
            })
            .fold(0_u64, u64::saturating_add)
    }
}

/// What a scan of some Parquet files reads: decoded bytes and the row groups
/// they lie in.
///
/// The row groups are the unit the scan's own pruning reports, so Oracle
/// counts them beside the scan's to show where sizing kept more than the
/// scan read.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct ScanSize {
    /// Uncompressed bytes of the covered column chunks.
    pub(crate) bytes: u64,
    /// Row groups the filters could not rule out.
    pub(crate) row_groups: u64,
}

impl ScanSize {
    /// Adds `other` into this size, saturating.
    #[must_use]
    pub(crate) const fn plus(self, other: Self) -> Self {
        Self {
            bytes: self.bytes.saturating_add(other.bytes),
            row_groups: self.row_groups.saturating_add(other.row_groups),
        }
    }
}

/// A point where sizing could not apply its rule, so its count differs from
/// what the scan reads.
///
/// Each one is silent in the answer, which stays exact, so it is counted in
/// `bifrost_scan_sizing_fallback_total{reason}` and logged at DEBUG with its
/// cause.
#[derive(Debug, Clone, Copy)]
enum SizingFallback {
    /// A filter conjunct did not plan against the sized schema, so it prunes
    /// nothing.
    DroppedConjunct,
    /// A Parquet file's footer was not read, so the file counts its size on
    /// disk.
    UnreadFooter,
    /// A memtable batch column has no recorded byte size, so it counts
    /// nothing.
    UnsizedColumn,
}

impl SizingFallback {
    /// Returns the metric label of this fallback.
    const fn label(self) -> &'static str {
        match self {
            Self::DroppedConjunct => "dropped_conjunct",
            Self::UnreadFooter => "unread_footer",
            Self::UnsizedColumn => "unsized_column",
        }
    }
}

/// Counts and logs one sizing `fallback` and its `cause`.
fn record_fallback(fallback: SizingFallback, cause: &str) {
    metrics::counter!("bifrost_scan_sizing_fallback_total", "reason" => fallback.label())
        .increment(1);
    tracing::debug!(reason = fallback.label(), cause, "scan sizing fell back");
}

/// A query predicate planned against a schema to rule out whole batches by
/// their recorded column bounds.
///
/// The one owner of bounds pruning for live data: the Scribe listing sizes a
/// query from the batches it keeps, and the Scribe follower's memtable leaf
/// reads exactly those batches. Parquet row groups prune with the same
/// [`PruningPredicate`](datafusion::physical_optimizer::pruning::PruningPredicate)
/// through [`Self::predicate`].
#[derive(Debug)]
pub(crate) struct BatchPruning(datafusion::physical_optimizer::pruning::PruningPredicate);

impl BatchPruning {
    /// Plans `predicate` against `schema`, or `None` when it does not plan
    /// as a pruning predicate.
    #[must_use]
    pub(crate) fn try_new(
        schema: &arrow::datatypes::SchemaRef,
        predicate: Arc<dyn datafusion::physical_expr::PhysicalExpr>,
    ) -> Option<Self> {
        datafusion::physical_optimizer::pruning::PruningPredicateBuilder::new()
            .with_file_schema(Arc::clone(schema))
            .try_build(predicate)
            .ok()
            .map(Self)
    }

    /// Returns the planned predicate, for pruning Parquet row groups.
    #[must_use]
    pub(crate) const fn predicate(
        &self,
    ) -> &datafusion::physical_optimizer::pruning::PruningPredicate {
        &self.0
    }

    /// Returns, per batch of `batches` many, whether its `statistics` leave it
    /// possibly matching.
    ///
    /// No batch keeps nothing without evaluating the predicate. Bounds not
    /// index-aligned with the batches, as in a hand-built test generation, or
    /// bounds that cannot be evaluated keep every batch.
    #[must_use]
    pub(crate) fn keep(
        &self,
        statistics: &[Arc<datafusion::common::Statistics>],
        batches: usize,
    ) -> Vec<bool> {
        if batches == 0 {
            // DataFusion cannot build a statistics array from zero
            // containers, and nothing is left to keep.
            return Vec::new();
        }
        if statistics.len() != batches {
            return vec![true; batches];
        }
        self.0
            .prune(&datafusion::common::pruning::PrunableStatistics::new(
                statistics.to_vec(),
                Arc::clone(self.0.schema()),
            ))
            .unwrap_or_else(|_| vec![true; batches])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Partitions follow the CPU/locality tuples and ignore memory.
    #[test]
    fn oracle_partition_count_follows_cpu_and_locality() {
        assert_eq!(oracle_target_partitions(8, 0.0).expect("remote"), 32);
        assert_eq!(oracle_target_partitions(8, 0.5).expect("mixed"), 20);
        assert_eq!(oracle_target_partitions(8, 1.0).expect("local"), 8);
        assert!(oracle_target_partitions(usize::MAX, 0.0).is_err());
        assert!(oracle_target_partitions(0, 0.0).is_err());
        assert!(oracle_target_partitions(8, f64::NAN).is_err());
    }

    /// Every query keeps the minimum partition floor.
    #[test]
    fn partition_count_never_falls_below_the_floor() {
        assert_eq!(
            oracle_target_partitions(1, 1.0).expect("single-core local scan"),
            ORACLE_MIN_TARGET_PARTITIONS
        );
    }

    /// Memtable batches are sized by the column byte sizes Scribe records on
    /// append, summed over the covered columns of the batches the filters
    /// keep by their recorded bounds.
    ///
    /// # Panics
    ///
    /// Panics when a fixture batch cannot be built or a size differs from
    /// the Arrow data of the kept, covered columns.
    #[test]
    fn scan_sizer_counts_covered_columns_of_kept_batches() {
        use arrow::array::{Int64Array, StringArray};
        use arrow::datatypes::{DataType, Field, Schema};
        use arrow::record_batch::RecordBatch;
        use datafusion::logical_expr::{col, lit};
        let schema = Arc::new(Schema::new(vec![
            Field::new("id", DataType::Int64, false),
            Field::new("payload", DataType::Utf8, false),
        ]));
        let statistics = |start: i64| {
            crate::scribe::memtable::batch_statistics(
                &RecordBatch::try_new(
                    Arc::clone(&schema),
                    vec![
                        Arc::new(Int64Array::from_iter_values(start..start + 100)),
                        Arc::new(StringArray::from_iter_values(
                            (0..100).map(|_| "x".repeat(64)),
                        )),
                    ],
                )
                .expect("batch"),
            )
        };
        let statistics = [statistics(0), statistics(100)];
        // 100 ids of 8 bytes; 101 offsets of 4 bytes and 100 values of 64.
        let (ids, payloads) = (800, 101 * 4 + 6_400);

        let everything = ScanScope::default();
        assert_eq!(
            ScanSizer::new(&everything, &schema).batch_bytes(&statistics),
            2 * (ids + payloads)
        );
        let ids_over_150 = ScanScope {
            columns: vec!["id".to_owned()],
            filters: vec![col("id").gt(lit(150_i64))],
        };
        assert_eq!(
            ScanSizer::new(&ids_over_150, &schema).batch_bytes(&statistics),
            ids
        );
    }

    /// Writes two row groups of `(id, payload)`, ids 0..100 then 100..200,
    /// and returns the file's footer.
    fn two_row_group_footer() -> Arc<parquet::file::metadata::ParquetMetaData> {
        use arrow::array::{Int64Array, StringArray};
        use arrow::datatypes::{DataType, Field, Schema};
        use arrow::record_batch::RecordBatch;
        let schema = Arc::new(Schema::new(vec![
            Field::new("id", DataType::Int64, false),
            Field::new("payload", DataType::Utf8, false),
        ]));
        let properties = parquet::file::properties::WriterProperties::builder()
            .set_max_row_group_row_count(Some(100))
            .set_dictionary_enabled(false)
            .set_compression(parquet::basic::Compression::ZSTD(
                parquet::basic::ZstdLevel::default(),
            ))
            .build();
        let mut bytes = Vec::new();
        let mut writer =
            parquet::arrow::ArrowWriter::try_new(&mut bytes, Arc::clone(&schema), Some(properties))
                .expect("writer");
        let batch = RecordBatch::try_new(
            schema,
            vec![
                Arc::new(Int64Array::from_iter_values(0..200)),
                Arc::new(StringArray::from_iter_values(
                    (0..200).map(|_| "x".repeat(64)),
                )),
            ],
        )
        .expect("batch");
        writer.write(&batch).expect("write");
        writer.close().expect("close");
        Arc::new(
            parquet::file::metadata::ParquetMetaDataReader::new()
                .parse_and_finish(&bytes::Bytes::from(bytes))
                .expect("footer"),
        )
    }

    /// A Parquet file is sized by the uncompressed bytes of the covered
    /// columns in the row groups the filters keep, never by compressed bytes.
    #[test]
    fn scan_sizer_counts_uncompressed_covered_kept_chunks() {
        use datafusion::logical_expr::{col, lit};
        let footer = two_row_group_footer();
        let schema = Arc::new(
            parquet::arrow::parquet_to_arrow_schema(
                footer.file_metadata().schema_descr(),
                footer.file_metadata().key_value_metadata(),
            )
            .expect("schema"),
        );
        let chunk = |group: usize, column: usize| {
            u64::try_from(footer.row_group(group).column(column).uncompressed_size()).expect("size")
        };
        let everything = ScanScope::default();
        assert_eq!(
            ScanSizer::new(&everything, &schema)
                .footer_size(&footer)
                .bytes,
            chunk(0, 0) + chunk(0, 1) + chunk(1, 0) + chunk(1, 1)
        );
        let compressed: i64 = (0..2)
            .map(|group| footer.row_group(group).compressed_size())
            .sum();
        assert!(chunk(0, 1) + chunk(1, 1) > u64::try_from(compressed).expect("size"));

        let ids_over_150 = ScanScope {
            columns: vec!["id".to_owned()],
            filters: vec![col("id").gt(lit(150_i64))],
        };
        assert_eq!(
            ScanSizer::new(&ids_over_150, &schema)
                .footer_size(&footer)
                .bytes,
            chunk(1, 0)
        );

        let qualified = ScanScope {
            columns: vec!["id".to_owned()],
            filters: vec![
                datafusion::logical_expr::Expr::Column(datafusion::common::Column::new(
                    Some("vala.bifrost.events"),
                    "id",
                ))
                .gt(lit(150_i64)),
            ],
        };
        assert_eq!(
            ScanSizer::new(&qualified, &schema)
                .footer_size(&footer)
                .bytes,
            chunk(1, 0),
            "a plan's table-qualified filter prunes as the scan's own does"
        );
    }
}
