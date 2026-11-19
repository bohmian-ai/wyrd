//! The Scribe follower's memtable leaf: a source over captured memory
//! batches that drops every batch a query's predicate rules out before any of
//! its rows is compared, and evaluates each kept batch only when its stream
//! is asked for the next one.
//!
//! `DataFusion`'s `MemorySourceConfig` holds batches already evaluated and
//! accepts no filter, so a point lookup over the memtable evaluates its
//! predicate on every row and every batch is filtered, projected, and
//! reconstructed before the query reads one. Scribe records each append
//! batch's exact column bounds when the batch arrives; this source prunes by
//! them through [`BatchPruning`], the same pruning the Scribe listing sizes
//! the query with, as a Parquet scan prunes row groups by their footer
//! statistics, and pulls each batch it keeps from the Scribe capture on
//! demand.
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
use datafusion::datasource::source::DataSource;
use datafusion::error::{DataFusionError, Result as DataFusionResult};
use datafusion::execution::TaskContext;
use datafusion::physical_expr::projection::ProjectionExprs;
use datafusion::physical_expr::utils::conjunction;
use datafusion::physical_expr::{DynamicFilterTracking, EquivalenceProperties, PhysicalExpr};
use datafusion::physical_expr_common::physical_expr::is_volatile;
use datafusion::physical_plan::filter_pushdown::{FilterPushdownPropagation, PushedDown};
use datafusion::physical_plan::stream::RecordBatchStreamAdapter;
use datafusion::physical_plan::{DisplayFormatType, Partitioning, SendableRecordBatchStream};

use crate::scan_sizing::{BatchPruning, ScanScope, ScanSizer};

/// The rows of one memtable batch: a captured batch evaluated when pulled.
#[derive(Debug, Clone)]
pub(super) enum MemtableRows {
    /// A Scribe capture's batch, filtered, projected, and reconstructed only
    /// when the source's stream asks for it.
    Captured(crate::scribe::memtable::LiveBatch),
    /// Rows already evaluated, as a unit fixture supplies them.
    #[cfg(test)]
    Evaluated(RecordBatch),
}

impl MemtableRows {
    /// Evaluates the rows in `schema`, the source's columns, or `None` when
    /// the capture's own filter keeps none of them.
    ///
    /// # Errors
    ///
    /// Returns the Scribe evaluation error, or the `DataFusion` error raised
    /// when the evaluated rows do not project to `schema`.
    fn evaluate(&self, schema: &SchemaRef) -> DataFusionResult<Option<RecordBatch>> {
        let rows = match self {
            Self::Captured(batch) => batch
                .evaluate()
                .map_err(|error| DataFusionError::External(Box::new(error)))?,
            #[cfg(test)]
            Self::Evaluated(batch) => Some(batch.clone()),
        };
        rows.map(|rows| super::exec::project_batch(&rows, Arc::clone(schema)))
            .transpose()
    }
}

/// A source over memtable batches that prunes them by their bounds and
/// evaluates each kept batch only when its stream pulls it.
///
/// It reads every batch, in order, partition by partition, until
/// `DataFusion`'s `FilterPushdown` offers it the query's predicate. It then
/// keeps only the batches the predicate cannot exclude, takes over every
/// static filter, and evaluates them itself on the kept batches the bounds
/// cannot prove fully matching. Dynamic and volatile filters stay with the
/// operator above, which still evaluates them on every surviving row. A
/// limit or column projection pushed into it applies to each evaluated batch,
/// so a stream that reaches its limit, or is dropped, evaluates no later
/// batch.
///
/// The batches share the Scribe capture's Arrow buffers, so the source, every
/// clone or pushed-down rewrite of it, and every stream it opens hold the
/// capture's accounting owners: retirement of the batches' generation cannot
/// return their charge to the Scribe root while any of them still reads.
#[derive(Debug, Clone)]
pub(super) struct MemtableSource {
    /// The batches, dealt to partitions, that this source reads.
    partitions: Vec<Vec<MemtableRows>>,
    /// The batches' columns, before any pushed-down projection.
    schema: SchemaRef,
    /// Indices into `schema` of the columns a pushed-down projection keeps,
    /// or `None` for every column.
    projection: Option<Vec<usize>>,
    /// `schema` after `projection`: the columns this source emits.
    projected_schema: SchemaRef,
    /// Most rows each partition emits, or `None` for every row.
    fetch: Option<usize>,
    /// Bounds of each batch in `partitions`, partition by partition, column
    /// for column with `schema`.
    statistics: Vec<Vec<Arc<Statistics>>>,
    /// `statistics` merged once per partition, then across partitions, so
    /// the planner's repeated statistics requests read Scribe's recorded
    /// bounds instead of re-walking the batches.
    merged: Merged,
    /// The sizing that deals batches over partitions by their recorded column bytes.
    partitioning: crate::scan_sizing::OraclePartitioning,
    /// Accounting owners of the batches' Arrow buffers: the captured
    /// generations' charges and the query's reservation of what the read
    /// allocated for it.
    charges: Arc<crate::scribe::memtable::LiveReadOwners>,
    /// The static filters this source took over, as one conjunction, or
    /// `None` while every filter stays with the operator above.
    filter: Option<Arc<dyn PhysicalExpr>>,
    /// Per batch of `partitions`, whether its bounds prove `filter` holds on
    /// every row, so the batch streams unfiltered.
    matched: Vec<Vec<bool>>,
}

impl MemtableSource {
    /// Builds the source over `batches` and their bounds, dealt round-robin
    /// over the partitions their bytes earn under `partitioning`, at least
    /// one and at most `cap`. No batch is evaluated here.
    ///
    /// Each batch evaluates to `schema`'s columns, so they are sized by
    /// [`ScanSizer`] with every column covered. `charges` are the accounting
    /// owners of the batches' buffers, held for the source's life. The
    /// batches' bounds are merged into [`Merged`] here, once. The source
    /// holds no filter until filter pushdown offers it one.
    ///
    /// # Errors
    ///
    /// Returns the `DataFusion` error raised when bounds do not merge because
    /// their column counts differ.
    pub(super) fn try_new(
        batches: Vec<(MemtableRows, Arc<Statistics>)>,
        charges: Arc<crate::scribe::memtable::LiveReadOwners>,
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
        let (partitions, statistics): (Vec<Vec<_>>, Vec<Vec<_>>) = dealt.into_iter().unzip();
        let merged = Merged::try_new(&statistics, &schema)?;
        Ok(Self {
            partitions,
            projected_schema: Arc::clone(&schema),
            schema,
            projection: None,
            fetch: None,
            matched: statistics
                .iter()
                .map(|bounds| vec![false; bounds.len()])
                .collect(),
            statistics,
            merged,
            partitioning,
            charges,
            filter: None,
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
    /// of its rows. No batch is evaluated.
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
        if self.fetch.is_some() || self.projection.is_some() {
            return Ok(None);
        }
        let mut excluded = false;
        let mut kept = Vec::new();
        let pruning = BatchPruning::try_new(&self.schema, predicate);
        for (batches, statistics) in self.partitions.iter().zip(&self.statistics) {
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
            Arc::clone(&self.schema),
        )?;
        let filter = conjunction(self.filter.iter().cloned().chain(absorbed));
        source.matched = BatchPruning::fully_matching(&source.schema, &filter, &source.statistics);
        source.filter = Some(filter);
        Ok(Some(source))
    }
}

/// Returns a pull of `pull`'s batches coalesced into batches of
/// `batch_size` rows, the buffer held under `reservation`.
///
/// A batch of at least half `batch_size` rows passes through uncopied, as
/// `DataFusion`'s own coalescing does. The rows still buffered when `pull`
/// ends are emitted last. An error from `pull`, the coalescer, or the
/// reservation is yielded once, after which the pull ends.
fn coalesced(
    mut pull: impl FnMut() -> Option<DataFusionResult<RecordBatch>>,
    schema: SchemaRef,
    batch_size: usize,
    reservation: datafusion::execution::memory_pool::MemoryReservation,
) -> impl FnMut() -> Option<DataFusionResult<RecordBatch>> {
    let mut coalescer = arrow::compute::BatchCoalescer::new(schema, batch_size)
        .with_biggest_coalesce_batch_size(Some(batch_size / 2));
    let mut ended = false;
    move || {
        loop {
            if let Some(rows) = coalescer.next_completed_batch() {
                reservation.resize(coalescer.size());
                return Some(Ok(rows));
            }
            if ended {
                return None;
            }
            let pushed = match pull() {
                Some(Ok(rows)) => coalescer
                    .push_batch(rows)
                    .map_err(Into::into)
                    .and_then(|()| reservation.try_resize(coalescer.size())),
                Some(Err(error)) => Err(error),
                None => {
                    ended = true;
                    coalescer.finish_buffered_batch().map_err(Into::into)
                }
            };
            if let Err(error) = pushed {
                ended = true;
                coalescer = arrow::compute::BatchCoalescer::new(coalescer.schema(), batch_size);
                reservation.free();
                return Some(Err(error));
            }
        }
    }
}

/// A memtable source's recorded bounds, merged.
#[derive(Debug, Clone)]
struct Merged {
    /// Each partition's batch bounds merged, in partition order.
    partitions: Vec<Arc<Statistics>>,
    /// Every partition's merged bounds merged again.
    total: Arc<Statistics>,
}

impl Merged {
    /// Merges `statistics`, one list of batch bounds per partition, with
    /// `DataFusion`'s [`Statistics::try_merge_iter`]; an empty partition
    /// reports unknown statistics over `schema`.
    ///
    /// The planner answers `MIN` and `MAX` from exact bounds, so a column
    /// whose recorded bound is not of its `schema` type, as when the follower
    /// cast the column to the query's closure, reports no bound at all.
    ///
    /// # Errors
    ///
    /// Returns the merge error when two bounds differ in column count.
    fn try_new(statistics: &[Vec<Arc<Statistics>>], schema: &SchemaRef) -> DataFusionResult<Self> {
        let partitions = statistics
            .iter()
            .map(|bounds| {
                let mut merged =
                    Statistics::try_merge_iter(bounds.iter().map(AsRef::as_ref), schema)?;
                for (column, field) in merged.column_statistics.iter_mut().zip(schema.fields()) {
                    for bound in [&mut column.min_value, &mut column.max_value] {
                        if bound
                            .get_value()
                            .is_some_and(|value| value.data_type() != *field.data_type())
                        {
                            *bound = datafusion::common::stats::Precision::Absent;
                        }
                    }
                }
                Ok(Arc::new(merged))
            })
            .collect::<DataFusionResult<Vec<_>>>()?;
        let total = Arc::new(Statistics::try_merge_iter(
            partitions.iter().map(AsRef::as_ref),
            schema,
        )?);
        Ok(Self { partitions, total })
    }
}

impl DataSource for MemtableSource {
    /// Streams one partition's kept batches in order, evaluating each only
    /// when the stream needs more rows: the batch is evaluated from the
    /// capture, filtered by the filters this source took over unless its
    /// bounds prove it fully matching, dropped when left empty, projected,
    /// and cut at the partition's limit, after which no later batch is
    /// evaluated.
    ///
    /// Appends are small, so the surviving rows are coalesced into batches of
    /// the session's batch size before they leave the source; a batch already
    /// half that size passes through uncopied, and the rows buffered when the
    /// partition or its limit ends are emitted as they are. The buffer is
    /// reserved from the task's memory pool. A batch that fails ends the
    /// stream with that error, dropping the buffered rows. The stream holds
    /// the source's accounting owners until it drops, since a stream can
    /// outlive its plan.
    ///
    /// # Errors
    ///
    /// Returns an internal `DataFusion` error for an out-of-range partition;
    /// the stream yields an evaluation, filter, projection, coalescing, or
    /// memory refusal error in place of the batch it was building.
    fn open(
        &self,
        partition: usize,
        context: Arc<TaskContext>,
    ) -> DataFusionResult<SendableRecordBatchStream> {
        let batches = self.partitions.get(partition).cloned().ok_or_else(|| {
            DataFusionError::Internal(format!("memtable partition {partition} is out of range"))
        })?;
        let matched = self.matched.get(partition).cloned().unwrap_or_default();
        let mut batches = batches
            .into_iter()
            .zip(matched.into_iter().chain(std::iter::repeat(false)));
        let schema = Arc::clone(&self.schema);
        let filter = self.filter.clone();
        let projection = self.projection.clone();
        let mut remaining = self.fetch;
        let charges = Arc::clone(&self.charges);
        let pull = move || -> Option<DataFusionResult<RecordBatch>> {
            let _charges = &charges;
            loop {
                if remaining == Some(0) {
                    return None;
                }
                let (rows, matched) = batches.next()?;
                let pulled = rows.evaluate(&schema).and_then(|rows| {
                    rows.map(|rows| match &filter {
                        Some(filter) if !matched => {
                            datafusion::physical_plan::filter::batch_filter(&rows, filter)
                        }
                        _ => Ok(rows),
                    })
                    .transpose()
                });
                let projected = pulled.and_then(|rows| {
                    rows.filter(|rows| rows.num_rows() > 0)
                        .map(|rows| match &projection {
                            Some(projection) => rows.project(projection).map_err(Into::into),
                            None => Ok(rows),
                        })
                        .transpose()
                });
                let mut rows = match projected {
                    Ok(Some(rows)) => rows,
                    Ok(None) => continue,
                    Err(error) => {
                        // A failure is terminal: no later batch is evaluated.
                        remaining = Some(0);
                        return Some(Err(error));
                    }
                };
                if let Some(remaining) = remaining.as_mut() {
                    rows = rows.slice(0, rows.num_rows().min(*remaining));
                    *remaining -= rows.num_rows();
                }
                return Some(Ok(rows));
            }
        };
        let next = coalesced(
            pull,
            Arc::clone(&self.projected_schema),
            context.session_config().batch_size(),
            datafusion::execution::memory_pool::MemoryConsumer::new(format!(
                "MemtableSource[{partition}]"
            ))
            .register(context.memory_pool()),
        );
        Ok(Box::pin(datafusion::physical_plan::coop::cooperative(
            RecordBatchStreamAdapter::new(
                Arc::clone(&self.projected_schema),
                futures_util::stream::iter(std::iter::from_fn(next)),
            ),
        )))
    }

    /// Displays as an in-memory source, with any limit and filter it holds.
    fn fmt_as(&self, t: DisplayFormatType, f: &mut fmt::Formatter) -> fmt::Result {
        let sizes: Vec<_> = self.partitions.iter().map(Vec::len).collect();
        match t {
            DisplayFormatType::Default | DisplayFormatType::Verbose => {
                write!(f, "partitions={}, partition_sizes={sizes:?}", sizes.len())?;
                if let Some(fetch) = self.fetch {
                    write!(f, ", fetch={fetch}")?;
                }
            }
            DisplayFormatType::TreeRender => {
                writeln!(f, "format=memory")?;
                write!(f, "batches={}", sizes.iter().sum::<usize>())?;
            }
        }
        match &self.filter {
            Some(filter) => write!(f, ", predicate={filter}"),
            None => Ok(()),
        }
    }

    /// Reports one partition per dealt batch list.
    fn output_partitioning(&self) -> Partitioning {
        Partitioning::UnknownPartitioning(self.partitions.len())
    }

    /// Reports no ordering or equivalence over the emitted columns.
    fn eq_properties(&self) -> EquivalenceProperties {
        EquivalenceProperties::new(Arc::clone(&self.projected_schema))
    }

    /// Reports the merged recorded bounds of one partition, or of all of
    /// them, projected like the emitted columns and cut by the limit; inexact
    /// once this source filters, because they count rows before the filter.
    /// An out-of-range partition reports unknown statistics.
    ///
    /// # Errors
    ///
    /// Returns the `DataFusion` error raised when the limit cannot apply to
    /// the bounds.
    fn partition_statistics(&self, partition: Option<usize>) -> DataFusionResult<Arc<Statistics>> {
        let recorded = match partition {
            None => Some(Arc::clone(&self.merged.total)),
            Some(partition) => self.merged.partitions.get(partition).cloned(),
        };
        let Some(recorded) = recorded else {
            return Ok(Arc::new(Statistics::new_unknown(&self.projected_schema)));
        };
        if self.projection.is_none() && self.fetch.is_none() && self.filter.is_none() {
            return Ok(recorded);
        }
        let mut statistics = Arc::unwrap_or_clone(recorded).project(self.projection.as_ref());
        if self.filter.is_some() {
            statistics = statistics.to_inexact();
        }
        let partitions = if partition.is_some() {
            1
        } else {
            self.partitions.len()
        };
        Ok(Arc::new(statistics.with_fetch(self.fetch, 0, partitions)?))
    }

    /// Applies a limit to every partition while this source holds no filter;
    /// the limited source no longer prunes. A filtering source refuses, so
    /// the limit stays above the filter it takes over.
    fn with_fetch(&self, limit: Option<usize>) -> Option<Arc<dyn DataSource>> {
        if self.filter.is_some() {
            return None;
        }
        Some(Arc::new(Self {
            fetch: limit,
            ..self.clone()
        }))
    }

    /// Reports the limit this source applies to each partition.
    fn fetch(&self) -> Option<usize> {
        self.fetch
    }

    /// Absorbs a projection of bare, unaliased columns while this source
    /// holds no filter; the projected source no longer prunes. A filtering
    /// source, or any other projection, is refused.
    ///
    /// # Errors
    ///
    /// Returns the `DataFusion` error raised when the projected schema
    /// cannot be built.
    fn try_swapping_with_projection(
        &self,
        projection: &ProjectionExprs,
    ) -> DataFusionResult<Option<Arc<dyn DataSource>>> {
        use datafusion::physical_plan::projection::{
            all_alias_free_columns, new_projections_for_columns,
        };
        let exprs: Vec<_> = projection.iter().cloned().collect();
        if self.filter.is_some() || !all_alias_free_columns(&exprs) {
            return Ok(None);
        }
        let every: Vec<usize> = (0..self.schema.fields().len()).collect();
        let columns =
            new_projections_for_columns(&exprs, self.projection.as_deref().unwrap_or(&every));
        let projected_schema = datafusion::common::project_schema(&self.schema, Some(&columns))?;
        Ok(Some(Arc::new(Self {
            projection: Some(columns),
            projected_schema,
            ..self.clone()
        })))
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

    /// Visits the filter this source took over, if any.
    ///
    /// # Errors
    ///
    /// Returns the visitor's error.
    fn apply_expressions(
        &self,
        f: &mut dyn FnMut(&Arc<dyn PhysicalExpr>) -> DataFusionResult<TreeNodeRecursion>,
    ) -> DataFusionResult<TreeNodeRecursion> {
        match &self.filter {
            Some(filter) => f(filter),
            None => Ok(TreeNodeRecursion::Continue),
        }
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
    use datafusion::datasource::source::{DataSource as _, DataSourceExec};
    use datafusion::logical_expr::Operator;
    use datafusion::physical_expr::expressions::{BinaryExpr, col, lit};
    use datafusion::physical_optimizer::PhysicalOptimizerRule as _;
    use datafusion::physical_optimizer::filter_pushdown::FilterPushdown;
    use datafusion::physical_plan::filter::FilterExec;
    use datafusion::physical_plan::{ExecutionPlan, collect};

    use super::{MemtableRows, MemtableSource};

    /// Builds the source over already evaluated `batches`, as
    /// [`MemtableSource::try_new`] does over captured ones.
    fn evaluated_source(
        batches: Vec<(RecordBatch, Arc<Statistics>)>,
        charges: Arc<crate::scribe::memtable::LiveReadOwners>,
        partitioning: crate::scan_sizing::OraclePartitioning,
        cap: usize,
        schema: arrow::datatypes::SchemaRef,
    ) -> datafusion::error::Result<MemtableSource> {
        MemtableSource::try_new(
            batches
                .into_iter()
                .map(|(batch, bounds)| (MemtableRows::Evaluated(batch), bounds))
                .collect(),
            charges,
            partitioning,
            cap,
            schema,
        )
    }

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
        let source = evaluated_source(
            vec![
                bounded(&[0, 1, 2]),
                bounded(&[10, 11, 12]),
                bounded(&[20, 21, 22]),
            ],
            Arc::default(),
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
        let kept = source.partitions.iter().map(Vec::len).collect::<Vec<_>>();
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
        let source = evaluated_source(
            vec![
                bounded(&[0, 1, 2]),
                bounded(&[10, 11, 12]),
                (with_null, Arc::new(null_bounds)),
            ],
            Arc::default(),
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

    /// Partial bounds keep their exact batches useful without claiming more
    /// than they prove: beside an unknown and an inexact batch, the exact
    /// batch below `id >= 15` is excluded from the scan and from sizing, only
    /// the exact batch above it is proven fully matching, and the rows equal
    /// those of an unpruned filter over every batch.
    ///
    /// # Panics
    ///
    /// Panics when the source cannot be built, a batch is kept, sized, or
    /// classified wrongly, or the answer differs from the reference rows.
    #[tokio::test]
    async fn partial_statistics_preserve_conservative_filtering() {
        let schema = Arc::new(Schema::new(vec![Field::new("id", DataType::Int64, false)]));
        let recorded = |values: &[i64]| {
            let batch = RecordBatch::try_new(
                Arc::clone(&schema),
                vec![Arc::new(Int64Array::from(values.to_vec()))],
            )
            .expect("fixture batch");
            let bounds = crate::scribe::memtable::batch_statistics(&batch);
            (batch, bounds)
        };
        let below = recorded(&[0, 1, 2]);
        let (unknown_batch, mut unknown) = recorded(&[3, 16]);
        for column in &mut Arc::make_mut(&mut unknown).column_statistics {
            column.min_value = Precision::Absent;
            column.max_value = Precision::Absent;
        }
        let (inexact_batch, inexact) = recorded(&[14, 15]);
        let inexact = Arc::new(inexact.as_ref().clone().to_inexact());
        let above = recorded(&[20, 21, 22]);
        let batches = vec![
            below.clone(),
            (unknown_batch, unknown),
            (inexact_batch, inexact),
            above.clone(),
        ];
        let statistics = batches
            .iter()
            .map(|(_, bounds)| Arc::clone(bounds))
            .collect::<Vec<_>>();
        let predicate = Arc::new(BinaryExpr::new(
            col("id", &schema).expect("id column"),
            Operator::GtEq,
            lit(15_i64),
        ));
        let reference = datafusion::physical_plan::filter::batch_filter(
            &arrow::compute::concat_batches(&schema, batches.iter().map(|(batch, _)| batch))
                .expect("reference batch"),
            &(Arc::clone(&predicate) as Arc<dyn datafusion::physical_expr::PhysicalExpr>),
        )
        .expect("reference rows");
        let ids = |rows: &[RecordBatch]| {
            let mut ids = rows
                .iter()
                .flat_map(|batch| {
                    batch
                        .column(0)
                        .as_any()
                        .downcast_ref::<Int64Array>()
                        .expect("ids")
                        .values()
                        .to_vec()
                })
                .collect::<Vec<_>>();
            ids.sort_unstable();
            ids
        };
        let scope = crate::scan_sizing::ScanScope {
            columns: vec!["id".to_owned()],
            filters: vec![datafusion::prelude::col("id").gt_eq(datafusion::prelude::lit(15_i64))],
        };
        let sized = crate::scan_sizing::ScanSizer::new(&scope, &schema).batch_bytes(&statistics);
        let bytes = |bounds: &Statistics| {
            *bounds.column_statistics[0]
                .byte_size
                .get_value()
                .expect("recorded byte size") as u64
        };
        let source = evaluated_source(
            batches,
            Arc::default(),
            crate::scan_sizing::OraclePartitioning::new(
                1,
                crate::scan_sizing::ORACLE_PARTITION_SCAN_BYTES,
            ),
            1,
            Arc::clone(&schema),
        )
        .expect("memtable source");
        let plan: Arc<dyn ExecutionPlan> = Arc::new(
            FilterExec::try_new(
                Arc::clone(&predicate) as _,
                Arc::new(DataSourceExec::new(Arc::new(source))),
            )
            .expect("filter over the memtable"),
        );

        let (plan, source) = pushed_down(plan);
        let rows = collect(
            plan,
            Arc::new(datafusion::execution::TaskContext::default()),
        )
        .await
        .expect("execute the pruned scan");

        assert_eq!(
            source.partitions.iter().map(Vec::len).collect::<Vec<_>>(),
            vec![3],
            "only the exact batch below the range is excluded"
        );
        assert_eq!(source.matched, vec![vec![false, false, true]]);
        assert_eq!(
            sized,
            statistics[1..]
                .iter()
                .map(|bounds| bytes(bounds))
                .sum::<u64>(),
            "sizing counts the unknown and inexact batches and not the excluded one"
        );
        assert_ne!(bytes(&below.1), 0);
        assert_eq!(
            crate::scan_sizing::BatchPruning::try_new(&schema, Arc::clone(&predicate) as _)
                .expect("the range is a pruning predicate")
                .keep(&[], 0),
            Vec::<bool>::new(),
            "no batches keep nothing"
        );
        assert_eq!(ids(&rows), ids(&[reference]));
        assert_eq!(ids(&rows), vec![15, 16, 20, 21, 22]);
    }

    /// The source reports Scribe's recorded bounds merged per partition and
    /// in total, not statistics recomputed from the batches, a bound whose
    /// type is not the source schema's column type is not reported, and a
    /// limited source reports the wrapped source's statistics instead.
    ///
    /// # Panics
    ///
    /// Panics when the source cannot be built or a merged statistic differs.
    #[test]
    fn statistics_merge_recorded_bounds() {
        let partitioning = crate::scan_sizing::OraclePartitioning::new(
            2,
            crate::scan_sizing::ORACLE_PARTITION_SCAN_BYTES,
        );
        let schema = Arc::new(Schema::new(vec![Field::new("id", DataType::Int64, false)]));
        let (first_batch, mut recorded) = bounded(&[4, 9]);
        // A recorded bound wider than the rows: only Scribe's bound reports it.
        Arc::make_mut(&mut recorded).column_statistics[0].min_value =
            Precision::Exact(ScalarValue::Int64(Some(-7)));
        let source = evaluated_source(
            vec![(first_batch, recorded), bounded(&[-3, 2])],
            Arc::default(),
            partitioning,
            1,
            Arc::clone(&schema),
        )
        .expect("memtable source");
        let total = source.partition_statistics(None).expect("total");
        let first = source.partition_statistics(Some(0)).expect("partition");

        assert_eq!(total.num_rows, Precision::Exact(4));
        assert_eq!(
            total.column_statistics[0].min_value,
            Precision::Exact(ScalarValue::Int64(Some(-7)))
        );
        assert_eq!(
            total.column_statistics[0].max_value,
            Precision::Exact(ScalarValue::Int64(Some(9)))
        );
        assert_eq!(first.num_rows, total.num_rows);
        let limited = source
            .with_fetch(Some(1))
            .expect("limited source")
            .partition_statistics(None)
            .expect("limited statistics");
        assert_ne!(
            limited.column_statistics[0].min_value,
            Precision::Exact(ScalarValue::Int64(Some(-7))),
            "a limited source does not report the recorded bounds"
        );

        let cast = Arc::new(Schema::new(vec![Field::new("id", DataType::Int32, false)]));
        let (batch, bounds) = bounded(&[1, 2]);
        let batch = super::super::exec::project_batch(&batch, Arc::clone(&cast)).expect("cast");
        let source = evaluated_source(vec![(batch, bounds)], Arc::default(), partitioning, 1, cast)
            .expect("cast source");
        let total = source.partition_statistics(None).expect("total");

        assert_eq!(total.column_statistics[0].min_value, Precision::Absent);
        assert_eq!(total.column_statistics[0].max_value, Precision::Absent);
    }

    /// Small kept batches leave the source coalesced: three one- and
    /// two-row batches stream as one batch of all their rows, in order, and
    /// a limit cuts the coalesced rows without evaluating past it.
    ///
    /// # Panics
    ///
    /// Panics when the batches stream uncoalesced, out of order, or past the
    /// limit.
    #[tokio::test]
    async fn small_batches_stream_coalesced_up_to_the_limit() {
        use datafusion::physical_plan::common::collect;

        let schema = Arc::new(Schema::new(vec![Field::new("id", DataType::Int64, false)]));
        let source = |fetch| {
            let mut source = evaluated_source(
                vec![bounded(&[1, 2]), bounded(&[3]), bounded(&[4, 5])],
                Arc::default(),
                crate::scan_sizing::OraclePartitioning::new(
                    1,
                    crate::scan_sizing::ORACLE_PARTITION_SCAN_BYTES,
                ),
                1,
                Arc::clone(&schema),
            )
            .expect("memtable source");
            source.fetch = fetch;
            source
        };
        let read = |source: MemtableSource| async move {
            collect(
                source
                    .open(0, Arc::new(datafusion::execution::TaskContext::default()))
                    .expect("the partition opens"),
            )
            .await
            .expect("the partition reads")
        };
        let ids = |batch: &RecordBatch| {
            batch
                .column(0)
                .as_any()
                .downcast_ref::<Int64Array>()
                .expect("ids")
                .values()
                .to_vec()
        };

        let all = read(source(None)).await;
        assert_eq!(all.len(), 1, "the small batches coalesce into one");
        assert_eq!(ids(&all[0]), vec![1, 2, 3, 4, 5]);
        let limited = read(source(Some(3))).await;
        assert_eq!(limited.len(), 1);
        assert_eq!(ids(&limited[0]), vec![1, 2, 3], "the limit cuts the rows");
    }

    /// A batch that fails to evaluate ends its partition's stream: the
    /// batches before it stream, the failure is yielded once, no later batch
    /// is evaluated, and the capture's accounting owners stay held while the
    /// stream reads and are released once it and its source drop. The
    /// session's batch size is the first batch's two rows, so that batch
    /// streams before the failing one is evaluated.
    ///
    /// # Panics
    ///
    /// Panics when a batch streams after the failure, the failure is not
    /// yielded, or the owners are not held and released as described.
    #[tokio::test]
    async fn a_failed_batch_ends_its_stream_and_releases_its_owners() {
        use futures_util::StreamExt as _;

        let schema = Arc::new(Schema::new(vec![Field::new("id", DataType::Int64, false)]));
        let unreadable = RecordBatch::try_new(
            Arc::new(Schema::new(vec![Field::new(
                "other",
                DataType::Int64,
                false,
            )])),
            vec![Arc::new(Int64Array::from(vec![5_i64]))],
        )
        .expect("unreadable batch");
        let charges = Arc::new(crate::scribe::memtable::LiveReadOwners::default());
        let source = evaluated_source(
            vec![
                bounded(&[1, 2]),
                (unreadable, Arc::new(Statistics::new_unknown(&schema))),
                bounded(&[3, 4]),
            ],
            Arc::clone(&charges),
            crate::scan_sizing::OraclePartitioning::new(
                1,
                crate::scan_sizing::ORACLE_PARTITION_SCAN_BYTES,
            ),
            1,
            Arc::clone(&schema),
        )
        .expect("memtable source");
        let two_rows = datafusion::execution::TaskContext::default()
            .with_session_config(datafusion::prelude::SessionConfig::new().with_batch_size(2));
        let mut stream = source
            .open(0, Arc::new(two_rows))
            .expect("the partition opens");

        let first = stream
            .next()
            .await
            .expect("a first batch")
            .expect("it reads");
        assert_eq!(first.num_rows(), 2);
        assert_eq!(
            Arc::strong_count(&charges),
            3,
            "the open stream holds the owners"
        );
        assert!(stream.next().await.expect("the failure").is_err());
        assert!(
            stream.next().await.is_none(),
            "no batch follows the failure"
        );
        drop(stream);
        drop(source);
        assert_eq!(Arc::strong_count(&charges), 1, "the owners are released");
    }
}
