//! The Scribe follower's memtable leaf: an in-memory source that drops every
//! batch a query's predicate rules out before any of its rows is compared.
//!
//! `DataFusion`'s `MemorySourceConfig` accepts no filter, so a point lookup
//! over the memtable evaluates its predicate on every row. Scribe records each
//! append batch's exact column bounds when the batch arrives; this source
//! prunes by them through [`BatchPruning`], the same pruning the Scribe
//! listing sizes the query with, as a Parquet scan prunes row groups by their
//! footer statistics.
//!
//! The source also takes over the query's static filters. A batch the bounds
//! prove matches on every row streams without evaluating them; only the
//! batches the bounds cannot decide are filtered row by row.

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
use datafusion::physical_expr::{DynamicFilterTracking, EquivalenceProperties, PhysicalExpr};
use datafusion::physical_expr_common::physical_expr::is_volatile;
use datafusion::physical_plan::filter_pushdown::{FilterPushdownPropagation, PushedDown};
use datafusion::physical_plan::stream::RecordBatchStreamAdapter;
use datafusion::physical_plan::{DisplayFormatType, Partitioning, SendableRecordBatchStream};

use crate::scan_sizing::{BatchPruning, ScanScope, ScanSizer};

/// An in-memory source over memtable batches that prunes them by their bounds.
///
/// Reads exactly as the wrapped `MemorySourceConfig` does until
/// `DataFusion`'s `FilterPushdown` offers it the query's predicate. It then
/// keeps only the batches the predicate cannot exclude, takes over every
/// static filter, and evaluates them itself on the kept batches the bounds
/// cannot prove fully matching. Dynamic and volatile filters stay with the
/// operator above, which still evaluates them on every surviving row.
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
    /// The static filters this source took over, as one conjunction, or
    /// `None` while every filter stays with the operator above.
    filter: Option<Arc<dyn PhysicalExpr>>,
    /// Per batch of `inner`, partition by partition, whether its bounds
    /// prove `filter` holds on every row, so the batch streams unfiltered.
    matched: Vec<Vec<bool>>,
}

impl MemtableSource {
    /// Builds the source over `batches` and their bounds, dealt round-robin
    /// over the partitions their bytes earn under `partitioning`, at least
    /// one and at most `cap`.
    ///
    /// The batches are already projected to the query's columns, so they are
    /// sized by [`ScanSizer`] with every column covered. `charges` are the
    /// accounting owners of the batches' buffers, held for the source's life.
    /// The source holds no filter until filter pushdown offers it one.
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
            matched: statistics
                .iter()
                .map(|bounds| vec![false; bounds.len()])
                .collect(),
            statistics,
            partitioning,
            charges,
            filter: None,
        })
    }

    /// Rewraps a fetch- or projection-rewritten copy of `inner`, keeping this
    /// source's accounting owners, or `None` when `rewritten` is not an
    /// in-memory source.
    ///
    /// Only a source holding no filter is rewritten, and the rewritten source
    /// no longer prunes (see [`Self::pruned`]).
    fn rewrapped(&self, rewritten: &Arc<dyn DataSource>) -> Option<Self> {
        let rewritten: &dyn std::any::Any = rewritten.as_ref();
        rewritten
            .downcast_ref::<MemorySourceConfig>()
            .map(|inner| Self {
                inner: inner.clone(),
                statistics: self.statistics.clone(),
                partitioning: self.partitioning,
                charges: Arc::clone(&self.charges),
                filter: None,
                matched: self.matched.clone(),
            })
    }

    /// Returns this source pruned by `predicate` and, when `absorbed` names
    /// static filters, taking them over on top of any it already holds; or
    /// `None` when that changes nothing.
    ///
    /// Batches `predicate` excludes are dropped and the kept ones re-dealt
    /// round-robin over
    /// [`crate::scan_sizing::OraclePartitioning::scan_partitions`] of their
    /// recorded column bytes, never more than this source had, so a point
    /// lookup that keeps one batch runs as one partition rather than one per
    /// core. A predicate that does not plan as a pruning predicate, or a
    /// partition whose bounds cannot be evaluated, keeps every batch it
    /// covers. A source carrying a fetch or projection is never pruned, since
    /// rebuilding would drop it. Each kept batch is then marked fully matching when
    /// [`BatchPruning::fully_matching`] proves the held filter true on all
    /// of its rows.
    ///
    /// # Errors
    ///
    /// Returns the `DataFusion` error raised when the kept batches cannot be
    /// rebuilt into a source.
    fn pruned(
        &self,
        predicate: Arc<dyn PhysicalExpr>,
        absorbed: Vec<Arc<dyn PhysicalExpr>>,
    ) -> DataFusionResult<Option<Self>> {
        if self.inner.fetch().is_some() || self.inner.projection().is_some() {
            return Ok(None);
        }
        let schema = self.inner.original_schema();
        let mut excluded = false;
        let mut kept = Vec::new();
        let pruning = BatchPruning::try_new(&schema, predicate);
        for (batches, statistics) in self.inner.partitions().iter().zip(&self.statistics) {
            let keep = pruning.as_ref().map_or_else(
                || vec![true; batches.len()],
                |pruning| pruning.keep(statistics, batches.len()),
            );
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
        if !excluded && absorbed.is_empty() {
            return Ok(None);
        }
        let mut source = Self::try_new(
            kept,
            Arc::clone(&self.charges),
            self.partitioning,
            self.statistics.len(),
            schema,
        )?;
        let filter = conjunction(self.filter.iter().cloned().chain(absorbed));
        source.matched = source
            .statistics
            .iter()
            .map(|bounds| {
                BatchPruning::fully_matching(&source.inner.original_schema(), &filter, bounds)
            })
            .collect();
        source.filter = Some(filter);
        Ok(Some(source))
    }

    /// Streams `partition`'s kept batches, filtering by `filter` each one its
    /// bounds do not prove fully matching and dropping batches left empty.
    ///
    /// # Errors
    ///
    /// Returns an internal `DataFusion` error for an out-of-range partition.
    fn filtered(
        &self,
        partition: usize,
        filter: &Arc<dyn PhysicalExpr>,
    ) -> DataFusionResult<SendableRecordBatchStream> {
        let batches = self.inner.partitions().get(partition).ok_or_else(|| {
            datafusion::error::DataFusionError::Internal(format!(
                "memtable partition {partition} is out of range"
            ))
        })?;
        let matched = self.matched.get(partition).cloned().unwrap_or_default();
        let filter = Arc::clone(filter);
        let batches = batches
            .clone()
            .into_iter()
            .zip(matched.into_iter().chain(std::iter::repeat(false)))
            .map(move |(batch, matched)| {
                if matched {
                    Ok(batch)
                } else {
                    datafusion::physical_plan::filter::batch_filter(&batch, &filter)
                }
            })
            .filter(|batch| batch.as_ref().map_or(true, |batch| batch.num_rows() > 0));
        Ok(Box::pin(datafusion::physical_plan::coop::cooperative(
            RecordBatchStreamAdapter::new(
                self.inner.original_schema(),
                futures_util::stream::iter(batches),
            ),
        )))
    }
}

impl DataSource for MemtableSource {
    /// Streams one partition's kept batches, filtering each one its bounds
    /// do not prove fully matching by the filters this source took over, and
    /// holding the source's accounting owners until the stream drops, since a
    /// stream can outlive its plan.
    ///
    /// # Errors
    ///
    /// Returns the wrapped source's error for an out-of-range partition.
    fn open(
        &self,
        partition: usize,
        context: Arc<TaskContext>,
    ) -> DataFusionResult<SendableRecordBatchStream> {
        let stream = match &self.filter {
            None => self.inner.open(partition, context)?,
            Some(filter) => self.filtered(partition, filter)?,
        };
        let schema = stream.schema();
        let charges = Arc::clone(&self.charges);
        let held = futures_util::StreamExt::map(stream, move |batch| {
            let _charges = &charges;
            batch
        });
        Ok(Box::pin(RecordBatchStreamAdapter::new(schema, held)))
    }

    /// Displays as the wrapped in-memory source, with any filter it took over.
    fn fmt_as(&self, t: DisplayFormatType, f: &mut fmt::Formatter) -> fmt::Result {
        self.inner.fmt_as(t, f)?;
        match &self.filter {
            Some(filter) => write!(f, ", predicate={filter}"),
            None => Ok(()),
        }
    }

    /// Reports the wrapped source's partitioning.
    fn output_partitioning(&self) -> Partitioning {
        self.inner.output_partitioning()
    }

    /// Reports the wrapped source's equivalences.
    fn eq_properties(&self) -> EquivalenceProperties {
        self.inner.eq_properties()
    }

    /// Reports the wrapped source's statistics, inexact once this source
    /// filters, because they count rows before the filter.
    ///
    /// # Errors
    ///
    /// Returns the wrapped source's error for an out-of-range partition.
    fn partition_statistics(&self, partition: Option<usize>) -> DataFusionResult<Arc<Statistics>> {
        let statistics = self.inner.partition_statistics(partition)?;
        Ok(if self.filter.is_some() {
            Arc::new(Arc::unwrap_or_clone(statistics).to_inexact())
        } else {
            statistics
        })
    }

    /// Applies a limit through the wrapped source, keeping the accounting
    /// owners, while this source holds no filter; the limited source no
    /// longer prunes. A filtering source refuses, because the wrapped one
    /// would drop the filter.
    fn with_fetch(&self, limit: Option<usize>) -> Option<Arc<dyn DataSource>> {
        if self.filter.is_some() {
            return None;
        }
        let limited = self.inner.with_fetch(limit)?;
        self.rewrapped(&limited)
            .map(|source| Arc::new(source) as Arc<dyn DataSource>)
    }

    /// Reports the wrapped source's limit.
    fn fetch(&self) -> Option<usize> {
        self.inner.fetch()
    }

    /// Absorbs a projection through the wrapped source, keeping the
    /// accounting owners, while this source holds no filter; the projected
    /// source no longer prunes. A filtering source refuses, because the
    /// wrapped one would drop the filter.
    ///
    /// # Errors
    ///
    /// Returns the wrapped source's projection error.
    fn try_swapping_with_projection(
        &self,
        projection: &ProjectionExprs,
    ) -> DataFusionResult<Option<Arc<dyn DataSource>>> {
        if self.filter.is_some() {
            return Ok(None);
        }
        Ok(self
            .inner
            .try_swapping_with_projection(projection)?
            .and_then(|projected| self.rewrapped(&projected))
            .map(|source| Arc::new(source) as Arc<dyn DataSource>))
    }

    /// Prunes the batches `filters` exclude and takes over every static
    /// filter; dynamic and volatile filters stay with the operator above.
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
        let pushed: Vec<PushedDown> = filters
            .iter()
            .map(|filter| {
                if is_volatile(filter)
                    || DynamicFilterTracking::classify(filter).contains_dynamic_filter()
                {
                    PushedDown::No
                } else {
                    PushedDown::Yes
                }
            })
            .collect();
        let absorbed = filters
            .iter()
            .zip(&pushed)
            .filter(|(_, pushed)| matches!(pushed, PushedDown::Yes))
            .map(|(filter, _)| Arc::clone(filter))
            .collect();
        let predicate = conjunction(self.filter.iter().cloned().chain(filters));
        Ok(match self.pruned(predicate, absorbed)? {
            Some(pruned) => FilterPushdownPropagation::with_parent_pushdown_result(pushed)
                .with_updated_node(Arc::new(pruned) as Arc<dyn DataSource>),
            None => FilterPushdownPropagation::with_parent_pushdown_result(vec![
                PushedDown::No;
                pushed.len()
            ]),
        })
    }

    /// Visits the wrapped source's expressions and any filter it took over.
    ///
    /// # Errors
    ///
    /// Returns the visitor's error.
    fn apply_expressions(
        &self,
        f: &mut dyn FnMut(&Arc<dyn PhysicalExpr>) -> DataFusionResult<TreeNodeRecursion>,
    ) -> DataFusionResult<TreeNodeRecursion> {
        if let Some(filter) = &self.filter
            && f(filter)? == TreeNodeRecursion::Stop
        {
            return Ok(TreeNodeRecursion::Stop);
        }
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

    /// Runs filter pushdown over `plan` and returns the rewritten plan with
    /// the memtable source it now reads, which must have replaced the filter.
    ///
    /// # Panics
    ///
    /// Panics when pushdown fails or a filter still sits above the source.
    fn pushed_down(plan: Arc<dyn ExecutionPlan>) -> (Arc<dyn ExecutionPlan>, MemtableSource) {
        let plan = FilterPushdown::new()
            .optimize(plan, &ConfigOptions::default())
            .expect("filter pushdown");
        let source = plan
            .downcast_ref::<DataSourceExec>()
            .and_then(|scan| scan.data_source().downcast_ref::<MemtableSource>())
            .cloned()
            .expect("the memtable source took over the filter");
        (plan, source)
    }

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

        let (plan, source) = pushed_down(plan);
        let kept = source
            .inner
            .partitions()
            .iter()
            .map(Vec::len)
            .collect::<Vec<_>>();
        let rows = collect(
            plan,
            Arc::new(datafusion::execution::TaskContext::default()),
        )
        .await
        .expect("execute the pruned lookup");

        assert_eq!(kept, vec![1]);
        assert_eq!(source.matched, vec![vec![false]]);
        assert_eq!(rows.iter().map(RecordBatch::num_rows).sum::<usize>(), 1);
    }

    /// A range filter the memtable takes over streams the batch its bounds
    /// prove fully matching unfiltered, filters the boundary batch row by
    /// row, evaluates every row of a batch holding nulls, and still answers
    /// exactly, with inexact statistics so no count is answered from them.
    ///
    /// # Panics
    ///
    /// Panics when a batch is classified wrongly, the answer differs from
    /// the matching rows, or the statistics stay exact.
    #[tokio::test]
    async fn range_filter_skips_evaluation_only_on_proven_batches() {
        let schema = Arc::new(Schema::new(vec![Field::new("id", DataType::Int64, true)]));
        let with_null = RecordBatch::try_new(
            Arc::clone(&schema),
            vec![Arc::new(Int64Array::from(vec![Some(30), None, Some(1)]))],
        )
        .expect("nullable batch");
        let null_bounds = Statistics {
            num_rows: Precision::Exact(3),
            total_byte_size: Precision::Absent,
            column_statistics: vec![ColumnStatistics {
                null_count: Precision::Exact(1),
                min_value: Precision::Exact(ScalarValue::Int64(Some(30))),
                max_value: Precision::Exact(ScalarValue::Int64(Some(30))),
                ..ColumnStatistics::new_unknown()
            }],
        };
        let source = MemtableSource::try_new(
            vec![
                bounded(&[0, 1, 2]),
                bounded(&[10, 11, 12]),
                (with_null, Arc::new(null_bounds)),
            ],
            Arc::from(Vec::new()),
            crate::scan_sizing::OraclePartitioning::new(
                1,
                crate::scan_sizing::ORACLE_PARTITION_SCAN_BYTES,
            ),
            1,
            Arc::clone(&schema),
        )
        .expect("memtable source");
        let predicate = Arc::new(BinaryExpr::new(
            col("id", &schema).expect("id column"),
            Operator::GtEq,
            lit(2_i64),
        ));
        let plan: Arc<dyn ExecutionPlan> = Arc::new(
            FilterExec::try_new(predicate, Arc::new(DataSourceExec::new(Arc::new(source))))
                .expect("filter over the memtable"),
        );

        let (plan, source) = pushed_down(plan);
        let exact = datafusion::datasource::source::DataSource::partition_statistics(&source, None)
            .expect("statistics")
            .num_rows
            .is_exact();
        let rows = collect(
            plan,
            Arc::new(datafusion::execution::TaskContext::default()),
        )
        .await
        .expect("execute the filtered scan");
        let ids: Vec<i64> = rows
            .iter()
            .flat_map(|batch| {
                batch
                    .column(0)
                    .as_any()
                    .downcast_ref::<Int64Array>()
                    .expect("ids")
                    .iter()
                    .flatten()
            })
            .collect();

        assert_eq!(source.matched, vec![vec![false, true, false]]);
        assert_eq!(ids, vec![2, 10, 11, 12, 30]);
        assert_eq!(exact, Some(false));
    }
}
