//! The Scribe follower's memtable leaf: an in-memory source that drops every
//! batch a query's predicate rules out before any of its rows is compared.
//!
//! `DataFusion`'s `MemorySourceConfig` accepts no filter, so a point lookup
//! over the memtable evaluates its predicate on every row. Scribe records each
//! append batch's exact column bounds when the batch arrives; this source
//! hands them to `DataFusion`'s `PruningPredicate`, exactly as a Parquet scan
//! prunes row groups by their footer statistics.

use std::fmt;
use std::sync::Arc;

use arrow::datatypes::SchemaRef;
use arrow::record_batch::RecordBatch;
use datafusion::common::Statistics;
use datafusion::common::pruning::PrunableStatistics;
use datafusion::common::tree_node::TreeNodeRecursion;
use datafusion::config::ConfigOptions;
use datafusion::datasource::memory::MemorySourceConfig;
use datafusion::datasource::source::DataSource;
use datafusion::error::Result as DataFusionResult;
use datafusion::execution::TaskContext;
use datafusion::physical_expr::projection::ProjectionExprs;
use datafusion::physical_expr::utils::conjunction;
use datafusion::physical_expr::{EquivalenceProperties, PhysicalExpr};
use datafusion::physical_optimizer::pruning::PruningPredicateBuilder;
use datafusion::physical_plan::filter_pushdown::{FilterPushdownPropagation, PushedDown};
use datafusion::physical_plan::{DisplayFormatType, Partitioning, SendableRecordBatchStream};

/// An in-memory source over memtable batches that prunes them by their bounds.
///
/// Reads exactly as the wrapped `MemorySourceConfig` does. When
/// `DataFusion`'s `FilterPushdown` offers it the query's predicate, it keeps
/// only the batches the predicate cannot exclude and reports the predicate as
/// not pushed down, so the `FilterExec` above still evaluates it on every
/// surviving row and the answer stays exact.
#[derive(Debug, Clone)]
pub(super) struct MemtableSource {
    /// The batches, already dealt to partitions, that this source reads.
    inner: MemorySourceConfig,
    /// Exact bounds of each batch in `inner`, partition by partition, column
    /// for column with the source schema.
    statistics: Vec<Vec<Arc<Statistics>>>,
}

impl MemtableSource {
    /// Builds the source over `partitions` of batches and their bounds.
    ///
    /// # Errors
    ///
    /// Returns the `DataFusion` error raised when a batch does not match
    /// `schema`.
    pub(super) fn try_new(
        partitions: Vec<Vec<(RecordBatch, Arc<Statistics>)>>,
        schema: SchemaRef,
    ) -> DataFusionResult<Self> {
        let (batches, statistics): (Vec<Vec<_>>, Vec<Vec<_>>) = partitions
            .into_iter()
            .map(|partition| partition.into_iter().unzip())
            .unzip();
        Ok(Self {
            inner: MemorySourceConfig::try_new(&batches, schema, None)?,
            statistics,
        })
    }

    /// Returns this source without the batches `predicate` excludes, or
    /// `None` when it excludes none.
    ///
    /// A predicate that does not plan as a pruning predicate, or a partition
    /// whose bounds cannot be evaluated, keeps every batch it covers.
    ///
    /// # Errors
    ///
    /// Returns the `DataFusion` error raised when the kept batches cannot be
    /// rebuilt into a source.
    fn pruned(&self, predicate: Arc<dyn PhysicalExpr>) -> DataFusionResult<Option<Self>> {
        let schema = self.inner.original_schema();
        let Ok(pruning) = PruningPredicateBuilder::new()
            .with_file_schema(Arc::clone(&schema))
            .try_build(predicate)
        else {
            return Ok(None);
        };
        let mut excluded = false;
        let mut partitions = Vec::with_capacity(self.statistics.len());
        for (batches, statistics) in self.inner.partitions().iter().zip(&self.statistics) {
            let keep = pruning
                .prune(&PrunableStatistics::new(
                    statistics.clone(),
                    Arc::clone(&schema),
                ))
                .unwrap_or_else(|_| vec![true; batches.len()]);
            excluded |= keep.contains(&false);
            partitions.push(
                batches
                    .iter()
                    .zip(statistics)
                    .zip(keep)
                    .filter(|(_, keep)| *keep)
                    .map(|((batch, bounds), _)| (batch.clone(), Arc::clone(bounds)))
                    .collect(),
            );
        }
        if !excluded {
            return Ok(None);
        }
        Self::try_new(partitions, schema).map(Some)
    }
}

impl DataSource for MemtableSource {
    /// Streams one partition's kept batches.
    ///
    /// # Errors
    ///
    /// Returns the wrapped source's error for an out-of-range partition.
    fn open(
        &self,
        partition: usize,
        context: Arc<TaskContext>,
    ) -> DataFusionResult<SendableRecordBatchStream> {
        self.inner.open(partition, context)
    }

    /// Displays as the wrapped in-memory source.
    fn fmt_as(&self, t: DisplayFormatType, f: &mut fmt::Formatter) -> fmt::Result {
        self.inner.fmt_as(t, f)
    }

    /// Reports the wrapped source's partitioning.
    fn output_partitioning(&self) -> Partitioning {
        self.inner.output_partitioning()
    }

    /// Reports the wrapped source's equivalences.
    fn eq_properties(&self) -> EquivalenceProperties {
        self.inner.eq_properties()
    }

    /// Reports the wrapped source's statistics.
    ///
    /// # Errors
    ///
    /// Returns the wrapped source's error for an out-of-range partition.
    fn partition_statistics(&self, partition: Option<usize>) -> DataFusionResult<Arc<Statistics>> {
        self.inner.partition_statistics(partition)
    }

    /// Applies a limit through the wrapped source; the limited source no
    /// longer prunes.
    fn with_fetch(&self, limit: Option<usize>) -> Option<Arc<dyn DataSource>> {
        self.inner.with_fetch(limit)
    }

    /// Reports the wrapped source's limit.
    fn fetch(&self) -> Option<usize> {
        self.inner.fetch()
    }

    /// Absorbs a projection through the wrapped source; the projected source
    /// no longer prunes.
    ///
    /// # Errors
    ///
    /// Returns the wrapped source's projection error.
    fn try_swapping_with_projection(
        &self,
        projection: &ProjectionExprs,
    ) -> DataFusionResult<Option<Arc<dyn DataSource>>> {
        self.inner.try_swapping_with_projection(projection)
    }

    /// Prunes the batches `filters` exclude and leaves every filter to the
    /// operator above.
    ///
    /// # Errors
    ///
    /// Returns the `DataFusion` error raised when the kept batches cannot be
    /// rebuilt into a source.
    fn try_pushdown_filters(
        &self,
        filters: Vec<Arc<dyn PhysicalExpr>>,
        _config: &ConfigOptions,
    ) -> DataFusionResult<FilterPushdownPropagation<Arc<dyn DataSource>>> {
        let propagation = FilterPushdownPropagation::with_parent_pushdown_result(vec![
                PushedDown::No;
                filters.len()
            ]);
        Ok(match self.pruned(conjunction(filters))? {
            Some(pruned) => propagation.with_updated_node(Arc::new(pruned) as Arc<dyn DataSource>),
            None => propagation,
        })
    }

    /// Visits the wrapped source's expressions.
    ///
    /// # Errors
    ///
    /// Returns the visitor's error.
    fn apply_expressions(
        &self,
        f: &mut dyn FnMut(&Arc<dyn PhysicalExpr>) -> DataFusionResult<TreeNodeRecursion>,
    ) -> DataFusionResult<TreeNodeRecursion> {
        self.inner.apply_expressions(f)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use arrow::array::{Int64Array, RecordBatch};
    use arrow::datatypes::{DataType, Field, Schema};
    use datafusion::common::ScalarValue;
    use datafusion::common::stats::Precision;
    use datafusion::common::{ColumnStatistics, Statistics};
    use datafusion::config::ConfigOptions;
    use datafusion::datasource::source::DataSourceExec;
    use datafusion::logical_expr::Operator;
    use datafusion::physical_expr::expressions::{BinaryExpr, col, lit};
    use datafusion::physical_optimizer::PhysicalOptimizerRule as _;
    use datafusion::physical_optimizer::filter_pushdown::FilterPushdown;
    use datafusion::physical_plan::filter::FilterExec;
    use datafusion::physical_plan::{ExecutionPlan, collect};

    use super::MemtableSource;

    /// One batch of `values` with the exact bounds Scribe records for it.
    fn bounded(values: &[i64]) -> (RecordBatch, Arc<Statistics>) {
        let schema = Arc::new(Schema::new(vec![Field::new("id", DataType::Int64, false)]));
        let batch = RecordBatch::try_new(schema, vec![Arc::new(Int64Array::from(values.to_vec()))])
            .expect("fixture batch");
        let statistics = Statistics {
            num_rows: Precision::Exact(values.len()),
            total_byte_size: Precision::Absent,
            column_statistics: vec![ColumnStatistics {
                null_count: Precision::Exact(0),
                min_value: Precision::Exact(ScalarValue::Int64(values.iter().min().copied())),
                max_value: Precision::Exact(ScalarValue::Int64(values.iter().max().copied())),
                ..ColumnStatistics::new_unknown()
            }],
        };
        (batch, Arc::new(statistics))
    }

    /// A point lookup pushed into the memtable leaf keeps only the batch whose
    /// bounds hold the key, and the filter above still answers exactly.
    ///
    /// # Panics
    ///
    /// Panics when a batch the key cannot be in survives pushdown or the
    /// answer differs from the one matching row.
    #[tokio::test]
    async fn point_lookup_prunes_batches_outside_their_bounds() {
        let schema = Arc::new(Schema::new(vec![Field::new("id", DataType::Int64, false)]));
        let source = MemtableSource::try_new(
            vec![
                vec![bounded(&[0, 1, 2]), bounded(&[10, 11, 12])],
                vec![bounded(&[20, 21, 22])],
            ],
            Arc::clone(&schema),
        )
        .expect("memtable source");
        let predicate = Arc::new(BinaryExpr::new(
            col("id", &schema).expect("id column"),
            Operator::Eq,
            lit(11_i64),
        ));
        let plan: Arc<dyn ExecutionPlan> = Arc::new(
            FilterExec::try_new(predicate, Arc::new(DataSourceExec::new(Arc::new(source))))
                .expect("filter over the memtable"),
        );

        let plan = FilterPushdown::new()
            .optimize(plan, &ConfigOptions::default())
            .expect("filter pushdown");
        let kept = plan.children()[0]
            .downcast_ref::<DataSourceExec>()
            .and_then(|scan| scan.data_source().downcast_ref::<MemtableSource>())
            .map(|source| {
                source
                    .inner
                    .partitions()
                    .iter()
                    .map(Vec::len)
                    .collect::<Vec<_>>()
            });
        let rows = collect(
            plan,
            Arc::new(datafusion::execution::TaskContext::default()),
        )
        .await
        .expect("execute the pruned lookup");

        assert_eq!(kept, Some(vec![1, 0]));
        assert_eq!(rows.iter().map(RecordBatch::num_rows).sum::<usize>(), 1);
    }
}
