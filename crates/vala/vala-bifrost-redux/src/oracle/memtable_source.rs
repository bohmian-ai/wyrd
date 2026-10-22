//! The Scribe follower's memtable leaf: an in-memory source that drops every
//! batch a query's predicate rules out before any of its rows is compared.
//!
//! `DataFusion`'s `MemorySourceConfig` accepts no filter, so a point lookup
//! over the memtable evaluates its predicate on every row. Scribe records each
//! append batch's exact column bounds when the batch arrives; this source
//! prunes by them through [`BatchPruning`], the same pruning the Scribe
//! listing sizes the query with, as a Parquet scan prunes row groups by their
//! footer statistics.

use std::fmt;
use std::sync::Arc;

use arrow::datatypes::SchemaRef;
use arrow::record_batch::RecordBatch;
use datafusion::common::Statistics;
use datafusion::common::tree_node::TreeNodeRecursion;
use datafusion::config::ConfigOptions;
use datafusion::datasource::memory::MemorySourceConfig;
use datafusion::datasource::source::DataSource;
use datafusion::error::Result as DataFusionResult;
use datafusion::execution::TaskContext;
use datafusion::physical_expr::projection::ProjectionExprs;
use datafusion::physical_expr::utils::conjunction;
use datafusion::physical_expr::{EquivalenceProperties, PhysicalExpr};
use datafusion::physical_plan::filter_pushdown::{FilterPushdownPropagation, PushedDown};
use datafusion::physical_plan::stream::RecordBatchStreamAdapter;
use datafusion::physical_plan::{DisplayFormatType, Partitioning, SendableRecordBatchStream};

use crate::scan_sizing::{BatchPruning, ScanScope, ScanSizer};

/// An in-memory source over memtable batches that prunes them by their bounds.
///
/// Reads exactly as the wrapped `MemorySourceConfig` does. When
/// `DataFusion`'s `FilterPushdown` offers it the query's predicate, it keeps
/// only the batches the predicate cannot exclude and reports the predicate as
/// not pushed down, so the `FilterExec` above still evaluates it on every
/// surviving row and the answer stays exact.
///
/// The batches share the Scribe capture's Arrow buffers, so the source, every
/// clone or pushed-down rewrite of it, and every stream it opens hold the
/// capture's accounting owners: retirement of the batches' generation cannot
/// return their charge to the Scribe root while any of them still reads.
#[derive(Debug, Clone)]
pub(super) struct MemtableSource {
    /// The batches, already dealt to partitions, that this source reads.
    inner: MemorySourceConfig,
    /// Exact bounds of each batch in `inner`, partition by partition, column
    /// for column with the source schema.
    statistics: Vec<Vec<Arc<Statistics>>>,
    /// The sizing that deals batches over partitions by their recorded column bytes.
    partitioning: crate::scan_sizing::OraclePartitioning,
    /// Accounting owners of the batches' Arrow buffers.
    charges: Arc<[Arc<crate::scribe::memory::RetainedArrowCharge>]>,
}

impl MemtableSource {
    /// Builds the source over `batches` and their bounds, dealt round-robin
    /// over the partitions their bytes earn under `partitioning`, at least
    /// one and at most `cap`.
    ///
    /// The batches are already projected to the query's columns, so they are
    /// sized by [`ScanSizer`] with every column covered. `charges` are the
    /// accounting owners of the batches' buffers, held for the source's life.
    ///
    /// # Errors
    ///
    /// Returns the `DataFusion` error raised when a batch does not match
    /// `schema`.
    pub(super) fn try_new(
        batches: Vec<(RecordBatch, Arc<Statistics>)>,
        charges: Arc<[Arc<crate::scribe::memory::RetainedArrowCharge>]>,
        partitioning: crate::scan_sizing::OraclePartitioning,
        cap: usize,
        schema: SchemaRef,
    ) -> DataFusionResult<Self> {
        let (batches, bounds): (Vec<_>, Vec<_>) = batches.into_iter().unzip();
        let bytes = ScanSizer::new(&ScanScope::default(), &schema).batch_bytes(&bounds);
        let partitions = partitioning.scan_partitions(bytes, cap);
        let mut dealt = vec![(Vec::new(), Vec::new()); partitions];
        for (index, (batch, bounds)) in batches.into_iter().zip(bounds).enumerate() {
            let (partition, statistics) = &mut dealt[index % partitions];
            partition.push(batch);
            statistics.push(bounds);
        }
        let (batches, statistics): (Vec<Vec<_>>, Vec<Vec<_>>) = dealt.into_iter().unzip();
        Ok(Self {
            inner: MemorySourceConfig::try_new(&batches, schema, None)?,
            statistics,
            partitioning,
            charges,
        })
    }

    /// Rewraps a fetch- or projection-rewritten copy of `inner`, keeping this
    /// source's accounting owners, or `None` when `rewritten` is not an
    /// in-memory source.
    ///
    /// The rewritten source no longer prunes (see [`Self::pruned`]).
    fn rewrapped(&self, rewritten: &Arc<dyn DataSource>) -> Option<Self> {
        let rewritten: &dyn std::any::Any = rewritten.as_ref();
        rewritten
            .downcast_ref::<MemorySourceConfig>()
            .map(|inner| Self {
                inner: inner.clone(),
                statistics: self.statistics.clone(),
                partitioning: self.partitioning,
                charges: Arc::clone(&self.charges),
            })
    }

    /// Returns this source without the batches `predicate` excludes, dealt
    /// over the partitions the kept bytes earn, or `None` when it excludes
    /// none.
    ///
    /// The kept batches are re-dealt round-robin over
    /// [`crate::scan_sizing::OraclePartitioning::scan_partitions`] of their
    /// recorded column bytes, never more than this source had, so a point lookup that keeps one
    /// batch runs as one partition rather than one per core. A predicate that
    /// does not plan as a pruning predicate, or a partition whose bounds
    /// cannot be evaluated, keeps every batch it covers. A source carrying a
    /// fetch or projection is never pruned, since rebuilding would drop it.
    ///
    /// # Errors
    ///
    /// Returns the `DataFusion` error raised when the kept batches cannot be
    /// rebuilt into a source.
    fn pruned(&self, predicate: Arc<dyn PhysicalExpr>) -> DataFusionResult<Option<Self>> {
        if self.inner.fetch().is_some() || self.inner.projection().is_some() {
            return Ok(None);
        }
        let schema = self.inner.original_schema();
        let Some(pruning) = BatchPruning::try_new(&schema, predicate) else {
            return Ok(None);
        };
        let mut excluded = false;
        let mut kept = Vec::new();
        for (batches, statistics) in self.inner.partitions().iter().zip(&self.statistics) {
            let keep = pruning.keep(statistics, batches.len());
            excluded |= keep.contains(&false);
            kept.extend(
                batches
                    .iter()
                    .zip(statistics)
                    .zip(keep)
                    .filter(|(_, keep)| *keep)
                    .map(|((batch, bounds), _)| (batch.clone(), Arc::clone(bounds))),
            );
        }
        if !excluded {
            return Ok(None);
        }
        Self::try_new(
            kept,
            Arc::clone(&self.charges),
            self.partitioning,
            self.statistics.len(),
            schema,
        )
        .map(Some)
    }
}

impl DataSource for MemtableSource {
    /// Streams one partition's kept batches, holding the source's accounting
    /// owners until the stream drops, since a stream can outlive its plan.
    ///
    /// # Errors
    ///
    /// Returns the wrapped source's error for an out-of-range partition.
    fn open(
        &self,
        partition: usize,
        context: Arc<TaskContext>,
    ) -> DataFusionResult<SendableRecordBatchStream> {
        let stream = self.inner.open(partition, context)?;
        let schema = stream.schema();
        let charges = Arc::clone(&self.charges);
        let held = futures_util::StreamExt::map(stream, move |batch| {
            let _charges = &charges;
            batch
        });
        Ok(Box::pin(RecordBatchStreamAdapter::new(schema, held)))
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

    /// Applies a limit through the wrapped source, keeping the accounting
    /// owners; the limited source no longer prunes.
    fn with_fetch(&self, limit: Option<usize>) -> Option<Arc<dyn DataSource>> {
        let limited = self.inner.with_fetch(limit)?;
        self.rewrapped(&limited)
            .map(|source| Arc::new(source) as Arc<dyn DataSource>)
    }

    /// Reports the wrapped source's limit.
    fn fetch(&self) -> Option<usize> {
        self.inner.fetch()
    }

    /// Absorbs a projection through the wrapped source, keeping the
    /// accounting owners; the projected source no longer prunes.
    ///
    /// # Errors
    ///
    /// Returns the wrapped source's projection error.
    fn try_swapping_with_projection(
        &self,
        projection: &ProjectionExprs,
    ) -> DataFusionResult<Option<Arc<dyn DataSource>>> {
        Ok(self
            .inner
            .try_swapping_with_projection(projection)?
            .and_then(|projected| self.rewrapped(&projected))
            .map(|source| Arc::new(source) as Arc<dyn DataSource>))
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
    /// bounds hold the key, re-deals it into the one partition its bytes earn,
    /// and the filter above still answers exactly.
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
                bounded(&[0, 1, 2]),
                bounded(&[10, 11, 12]),
                bounded(&[20, 21, 22]),
            ],
            Arc::from(Vec::new()),
            crate::scan_sizing::OraclePartitioning::new(
                2,
                crate::scan_sizing::ORACLE_PARTITION_SCAN_BYTES,
            ),
            2,
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

        assert_eq!(kept, Some(vec![1]));
        assert_eq!(rows.iter().map(RecordBatch::num_rows).sum::<usize>(), 1);
    }
}
