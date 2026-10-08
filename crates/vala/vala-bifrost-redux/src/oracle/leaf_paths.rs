//! The literal paths a query reads from each column holding a Variant,
//! handed to the Oracle readers so they decode only those paths' leaves.
//!
//! [`LeafPathPushdown`] runs last among the physical optimizer rules of
//! every Oracle session, and returns a plan with no Variant-holding leaf
//! column untouched. Otherwise it walks the plan from the root, works out how
//! each column reaching an Oracle leaf is used, and gives the leaf the
//! [`LeafPaths`] of every Variant-holding column used only through literal
//! Variant paths: a literal-key `variant_get`, reached through `get_field` on
//! a Struct or `array_element` with a literal index on a List, plus the same
//! paths below an `unnest` of a List column. The leaf reads each such column
//! through [`OracleVariantSql::trimmed`], which decodes only those paths'
//! leaves. Any other use of a column, including a Struct or List path that
//! reads no Variant, or an operator the walk does not see through, reads it
//! whole: `DataFusion`'s own Parquet projection prunes Struct leaves.
//!
//! A remote placeholder carries its paths to the follower in its codec
//! payload, where [`LeafPathPushdown::assign`] applies them to the
//! follower's own source.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use arrow::datatypes::{DataType, FieldRef, Schema, SchemaRef};
use datafusion::common::config::ConfigOptions;
use datafusion::common::tree_node::{TreeNode as _, TreeNodeRecursion};
use datafusion::datasource::physical_plan::{FileScanConfig, FileScanConfigBuilder};
use datafusion::datasource::source::DataSourceExec;
use datafusion::error::{DataFusionError, Result};
use datafusion::physical_expr::expressions::{Column, Literal};
use datafusion::physical_expr::projection::{ProjectionExpr, ProjectionExprs};
use datafusion::physical_expr::{PhysicalExpr, ScalarFunctionExpr};
use datafusion::physical_optimizer::PhysicalOptimizerRule;
use datafusion::physical_plan::ExecutionPlan;
use datafusion::physical_plan::coalesce_partitions::CoalescePartitionsExec;
use datafusion::physical_plan::coop::CooperativeExec;
use datafusion::physical_plan::filter::FilterExec;
use datafusion::physical_plan::limit::{GlobalLimitExec, LocalLimitExec};
use datafusion::physical_plan::projection::ProjectionExec;
use datafusion::physical_plan::repartition::RepartitionExec;
use datafusion::physical_plan::sorts::sort::SortExec;
use datafusion::physical_plan::sorts::sort_preserving_merge::SortPreservingMergeExec;
use datafusion::physical_plan::union::UnionExec;
use datafusion::physical_plan::unnest::UnnestExec;
use serde::{Deserialize, Serialize};
use wyrd_types::variant::is_variant;

use super::variant_sql::{OracleVariantSql, VARIANT_GET};
use crate::tables::holds_variant;

/// The literal paths read from each trimmed Variant-holding column of one
/// scan.
///
/// A column absent from the map is read whole. The map travels to followers
/// inside the remote placeholder's codec payload.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LeafPaths(BTreeMap<String, BTreeSet<Vec<String>>>);

impl LeafPaths {
    /// Returns whether every column is read whole.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Returns the paths a hot scan's Parquet projection trims each column to,
    /// wherever a trimmed read sits inside the projection's expressions, or
    /// `None` when `plan` is not a file scan. Tests assert it as the scan's
    /// physical field requirement.
    #[cfg(test)]
    pub(crate) fn of_scan(plan: &dyn ExecutionPlan) -> Option<Self> {
        let config = plan
            .downcast_ref::<DataSourceExec>()?
            .data_source()
            .downcast_ref::<FileScanConfig>()?;
        let mut paths = BTreeMap::new();
        for projection in config
            .file_source()
            .projection()
            .into_iter()
            .flat_map(ProjectionExprs::iter)
        {
            projection
                .expr
                .apply(|node| {
                    if let Some(read) = OracleVariantSql::trimmed_paths(node)
                        && let Some(column) = node
                            .children()
                            .first()
                            .and_then(|column| column.downcast_ref::<Column>())
                    {
                        paths.insert(column.name().to_owned(), read.clone());
                    }
                    Ok(TreeNodeRecursion::Continue)
                })
                .expect("an expression walk that never fails");
        }
        Some(Self(paths))
    }

    /// Returns the projection a reader evaluates over `schema`: each column
    /// as itself, except a trimmed column, read for its paths through
    /// [`OracleVariantSql::trimmed`].
    pub(crate) fn projection(&self, schema: &SchemaRef) -> ProjectionExprs {
        ProjectionExprs::new(schema.fields().iter().enumerate().map(|(index, field)| {
            let column: Arc<dyn PhysicalExpr> = Arc::new(Column::new(field.name(), index));
            let expr = match self.0.get(field.name()) {
                Some(paths) => OracleVariantSql::trimmed(column, field, paths.clone()),
                None => column,
            };
            ProjectionExpr::new(expr, field.name())
        }))
    }
}

/// How the operators above use one column.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Demand {
    /// Nothing reads the column's values.
    Unused,
    /// Only these literal paths read it.
    Paths(BTreeSet<Vec<String>>),
    /// Something reads the whole column.
    Whole,
}

impl Demand {
    /// Combines two uses of the same column: whole wins, paths accumulate.
    fn merge(&mut self, other: Self) {
        *self = match (std::mem::replace(self, Self::Unused), other) {
            (Self::Whole, _) | (_, Self::Whole) => Self::Whole,
            (Self::Unused, other) | (other, Self::Unused) => other,
            (Self::Paths(mut paths), Self::Paths(more)) => {
                paths.extend(more);
                Self::Paths(paths)
            }
        };
    }

    /// Merges `used` into the use of column `index` of `columns`; an index
    /// outside `columns` is ignored.
    fn merge_at(columns: &mut [Self], index: usize, used: Self) {
        if let Some(slot) = columns.get_mut(index) {
            slot.merge(used);
        }
    }

    /// Records every column use inside `expr`, evaluated over `schema`, into
    /// `columns`.
    ///
    /// A literal path ending in a `variant_get` over a column (see
    /// [`Self::path`]) uses that path. Any other column reference, a Struct or
    /// List path without a Variant read included, uses the whole column.
    fn collect(expr: &Arc<dyn PhysicalExpr>, schema: &Schema, columns: &mut [Self]) {
        if expr
            .downcast_ref::<ScalarFunctionExpr>()
            .is_some_and(|call| call.name() == VARIANT_GET)
            && let Some((column, path, _)) = Self::path(expr, schema)
            && !path.is_empty()
        {
            Self::merge_at(columns, column.index(), Self::Paths(BTreeSet::from([path])));
            return;
        }
        if let Some(column) = expr.downcast_ref::<Column>() {
            Self::merge_at(columns, column.index(), Self::Whole);
            return;
        }
        for child in expr.children() {
            Self::collect(child, schema, columns);
        }
    }

    /// Returns the column `expr` reads, the literal path it reads, and the
    /// field at the end of that path, when `expr` is a column or a literal
    /// path over one in `schema`.
    ///
    /// Each step must fit the type it applies to: `get_field` names a child
    /// of a Struct that is not a Variant, `array_element` with a literal
    /// index enters a List element and records the element field's name, and
    /// `variant_get` with literal string keys reads object keys of a Variant.
    fn path<'e>(
        expr: &'e Arc<dyn PhysicalExpr>,
        schema: &Schema,
    ) -> Option<(&'e Column, Vec<String>, FieldRef)> {
        if let Some(column) = expr.downcast_ref::<Column>() {
            return Some((
                column,
                Vec::new(),
                Arc::clone(schema.fields().get(column.index())?),
            ));
        }
        let call = expr.downcast_ref::<ScalarFunctionExpr>()?;
        let (base, steps) = call.args().split_first()?;
        let (column, mut path, mut field) = Self::path(base, schema)?;
        match call.name() {
            "get_field" if !is_variant(&field) => {
                for name in OracleVariantSql::literal_keys(steps)? {
                    let DataType::Struct(children) = field.data_type() else {
                        return None;
                    };
                    field = Arc::clone(children.find(&name)?.1);
                    path.push(name);
                }
            }
            "array_element" => {
                let [index] = steps else {
                    return None;
                };
                let (DataType::List(element), true) = (
                    field.data_type(),
                    index
                        .downcast_ref::<Literal>()?
                        .value()
                        .data_type()
                        .is_integer(),
                ) else {
                    return None;
                };
                path.push(element.name().clone());
                field = Arc::clone(element);
            }
            VARIANT_GET if is_variant(&field) => {
                path.extend(OracleVariantSql::literal_keys(steps)?);
            }
            _ => return None,
        }
        Some((column, path, field))
    }
}

/// The physical optimizer rule that hands [`LeafPaths`] to Oracle leaves.
///
/// It owns no state: the walk is a pure function of the plan.
#[derive(Debug)]
pub(crate) struct LeafPathPushdown;

impl PhysicalOptimizerRule for LeafPathPushdown {
    /// Returns `plan` untouched when no Oracle leaf in it has a
    /// Variant-holding column; otherwise walks it from its root, every root
    /// output read whole.
    ///
    /// # Errors
    ///
    /// Returns the `DataFusion` error raised when a rebuilt operator rejects
    /// its new children.
    fn optimize(
        &self,
        plan: Arc<dyn ExecutionPlan>,
        _config: &ConfigOptions,
    ) -> Result<Arc<dyn ExecutionPlan>> {
        let reads_variant = plan.exists(|node| {
            Ok(node.downcast_ref::<DataSourceExec>().is_some()
                && node
                    .schema()
                    .fields()
                    .iter()
                    .any(|field| holds_variant(field)))
        })?;
        if !reads_variant {
            return Ok(plan);
        }
        let whole = vec![Demand::Whole; plan.schema().fields().len()];
        Self::push(plan, whole)
    }

    /// The rule's name in optimizer diagnostics.
    fn name(&self) -> &'static str {
        "leaf_path_pushdown"
    }

    /// The walk rebuilds operators with their own schemas unchanged.
    fn schema_check(&self) -> bool {
        true
    }
}

impl LeafPathPushdown {
    /// Hands `plan` the use `demand` of each of its outputs and returns the
    /// rebuilt plan.
    ///
    /// Oracle leaves receive their paths. A projection maps each output's use
    /// onto the columns its expressions read. An operator that
    /// [keeps its input's columns](Self::keeps_columns) passes each column's
    /// use straight to its inputs, adding the uses of its own expressions
    /// (predicate, sort keys, hash keys). Every other operator reads its
    /// inputs whole.
    ///
    /// # Errors
    ///
    /// Returns the `DataFusion` error raised when a rebuilt operator rejects
    /// its new children.
    fn push(plan: Arc<dyn ExecutionPlan>, demand: Vec<Demand>) -> Result<Arc<dyn ExecutionPlan>> {
        if let Some(leaf) = Self::leaf(&plan, &demand)? {
            return Ok(leaf);
        }
        let inputs: Vec<Vec<Demand>> = if let Some(projection) =
            plan.downcast_ref::<ProjectionExec>()
        {
            let mut input = vec![Demand::Unused; projection.input().schema().fields().len()];
            for (expr, used) in projection.expr().iter().zip(demand) {
                match expr.expr.downcast_ref::<Column>() {
                    Some(column) => Demand::merge_at(&mut input, column.index(), used),
                    None => Demand::collect(&expr.expr, &projection.input().schema(), &mut input),
                }
            }
            vec![input]
        } else if let Some(input) = plan
            .downcast_ref::<UnnestExec>()
            .and_then(|unnest| Self::unnest_input(unnest, &demand))
        {
            vec![input]
        } else if Self::keeps_columns(&plan) {
            let projected = plan.downcast_ref::<FilterExec>().and_then(|filter| {
                Some((
                    filter.input().schema().fields().len(),
                    filter.projection().as_ref()?,
                ))
            });
            let mut input = match projected {
                Some((width, indices)) => {
                    let mut input = vec![Demand::Unused; width];
                    for (&index, used) in indices.iter().zip(demand) {
                        Demand::merge_at(&mut input, index, used);
                    }
                    input
                }
                None => demand,
            };
            if let Some(child) = plan.children().first() {
                let schema = child.schema();
                plan.apply_expressions(&mut |expr| {
                    Demand::collect(expr, &schema, &mut input);
                    Ok(TreeNodeRecursion::Continue)
                })?;
            }
            vec![input; plan.children().len()]
        } else {
            plan.children()
                .iter()
                .map(|child| vec![Demand::Whole; child.schema().fields().len()])
                .collect()
        };
        let children = plan
            .children()
            .into_iter()
            .zip(inputs)
            .map(|(child, input)| Self::push(Arc::clone(child), input))
            .collect::<Result<Vec<_>>>()?;
        if children.is_empty() {
            return Ok(plan);
        }
        datafusion::physical_plan::execution_plan::replace_children_if_necessary(plan, children)
    }

    /// Returns the use of each input column of `unnest` given the use
    /// `demand` of its outputs, or `None` when its outputs are not its input
    /// columns one for one.
    ///
    /// That holds when it unnests only List columns, each once and one level
    /// deep: every output is its input column, an unnested one replaced by
    /// its elements. A path on an element output is the same path below the
    /// List element on the input. Any other use of an unnested column reads
    /// it whole, since its lists decide how many rows come out.
    fn unnest_input(unnest: &UnnestExec, demand: &[Demand]) -> Option<Vec<Demand>> {
        let schema = unnest.input().schema();
        if !unnest.struct_column_indices().is_empty()
            || schema.fields().len() != demand.len()
            || unnest
                .list_column_indices()
                .iter()
                .any(|list| list.depth != 1)
        {
            return None;
        }
        let mut input = demand.to_vec();
        for list in unnest.list_column_indices() {
            let DataType::List(element) = schema.field(list.index_in_input_schema).data_type()
            else {
                return None;
            };
            let slot = input.get_mut(list.index_in_input_schema)?;
            *slot = match slot {
                Demand::Paths(paths) => Demand::Paths(
                    paths
                        .iter()
                        .map(|path| {
                            std::iter::once(element.name().clone())
                                .chain(path.iter().cloned())
                                .collect()
                        })
                        .collect(),
                ),
                _ => Demand::Whole,
            };
        }
        Some(input)
    }

    /// Returns whether every output column of `plan` is the same column of
    /// each of its inputs, so a column's use passes through unchanged.
    ///
    /// These operators only drop, reorder, split, or merge rows. A filter's
    /// optional index projection is the one column change, and the caller
    /// maps it.
    fn keeps_columns(plan: &Arc<dyn ExecutionPlan>) -> bool {
        plan.downcast_ref::<FilterExec>().is_some()
            || plan.downcast_ref::<SortExec>().is_some()
            || plan.downcast_ref::<SortPreservingMergeExec>().is_some()
            || plan.downcast_ref::<GlobalLimitExec>().is_some()
            || plan.downcast_ref::<LocalLimitExec>().is_some()
            || plan.downcast_ref::<RepartitionExec>().is_some()
            || plan.downcast_ref::<CoalescePartitionsExec>().is_some()
            || plan.downcast_ref::<CooperativeExec>().is_some()
            || plan.downcast_ref::<UnionExec>().is_some()
    }

    /// Returns `plan` with its paths when it is an Oracle leaf, else `None`.
    ///
    /// A hot scan with no column to trim is returned unchanged. A hot scan trims only outputs its projection reads as plain columns.
    /// An output that is an expression `DataFusion` moved into the scan is
    /// read whole: the Parquet reader already prunes the Struct leaves it
    /// names, and the file adapter reads shredded Variant paths in it
    /// directly or unshreds the column inside it, so wrapping the whole
    /// expression would only hide its column from both.
    ///
    /// A remote placeholder keeps the paths for its follower and applies them
    /// to the leader-readable leaf it holds.
    ///
    /// # Errors
    ///
    /// Returns the `DataFusion` error raised while applying the paths to a
    /// Parquet scan.
    fn leaf(
        plan: &Arc<dyn ExecutionPlan>,
        demand: &[Demand],
    ) -> Result<Option<Arc<dyn ExecutionPlan>>> {
        Ok(
            if let Some(hot) = plan.downcast_ref::<DataSourceExec>()
                && let Some(config) = hot.data_source().downcast_ref::<FileScanConfig>()
            {
                // An Oracle file scan is always a Parquet hot leaf, whose
                // source merges the trimmed projection into its own.
                let demand: Vec<Demand> = match config.file_source().projection() {
                    Some(projection) => projection
                        .iter()
                        .zip(demand)
                        .map(
                            |(output, used)| match output.expr.downcast_ref::<Column>() {
                                Some(_) => used.clone(),
                                None => Demand::Whole,
                            },
                        )
                        .collect(),
                    None => demand.to_vec(),
                };
                let paths = Self::paths(plan, &demand);
                // Nothing to trim: keep the scan as `DataFusion` built it.
                if paths.is_empty() {
                    return Ok(Some(Arc::clone(plan)));
                }
                let source = config
                    .file_source()
                    .try_pushdown_projection(&paths.projection(&plan.schema()))?
                    .ok_or_else(|| {
                        DataFusionError::Internal(
                            "the hot Parquet source refused a projection".to_owned(),
                        )
                    })?;
                Some(DataSourceExec::from_data_source(
                    FileScanConfigBuilder::from(config.clone())
                        .with_source(source)
                        .build(),
                ))
            } else {
                None
            },
        )
    }

    /// Returns the paths of each output of `leaf` that holds a Variant and is
    /// used only through literal paths.
    fn paths(leaf: &Arc<dyn ExecutionPlan>, demand: &[Demand]) -> LeafPaths {
        LeafPaths(
            leaf.schema()
                .fields()
                .iter()
                .zip(demand)
                .filter_map(|(field, used)| match used {
                    Demand::Paths(paths) if holds_variant(field) => {
                        Some((field.name().clone(), paths.clone()))
                    }
                    _ => None,
                })
                .collect(),
        )
    }
}

#[cfg(test)]
mod tests {
    use arrow::array::RecordBatch;
    use datafusion::datasource::MemTable;
    use datafusion::execution::SessionStateBuilder;
    use datafusion::prelude::SessionContext;
    use parquet::arrow::arrow_reader::{ArrowReaderOptions, ParquetRecordBatchReaderBuilder};

    use super::super::nested_pushdown::tests::{HotTable, logical_schema, write_file};
    use super::*;

    /// Plans `sql` with stock `DataFusion` over an in-memory copy of the
    /// unshredded fixture, and over the shredded fixture object read by the
    /// production hot scan trimmed by [`LeafPathPushdown`].
    ///
    /// Returns the paths the hot scan's projection was trimmed to, the rows
    /// the trimmed plan returns, and the rows stock `DataFusion` returns for
    /// the same query, both as [`json_rows`].
    ///
    /// # Panics
    ///
    /// Panics when the query does not plan or execute, or the plan does not
    /// hold exactly one scan.
    async fn trimmed_and_stock(sql: &str) -> (LeafPaths, String, String) {
        let schema = logical_schema();
        let rows = ParquetRecordBatchReaderBuilder::try_new_with_options(
            write_file(false),
            ArrowReaderOptions::new().with_schema(Arc::clone(&schema)),
        )
        .expect("valid footer")
        .build()
        .expect("whole read")
        .collect::<Result<Vec<_>, _>>()
        .expect("whole rows");
        let context = SessionContext::new_with_state(
            OracleVariantSql::shared()
                .install(SessionStateBuilder::new().with_default_features())
                .build(),
        );
        context
            .register_table(
                "t",
                Arc::new(MemTable::try_new(Arc::clone(&schema), vec![rows]).expect("table")),
            )
            .expect("register");
        let expected = context
            .sql(sql)
            .await
            .expect(sql)
            .collect()
            .await
            .expect(sql);

        let hot = HotTable::new(&write_file(true), schema);
        let trimmed = hot.plan(sql).await;
        let mut paths = Vec::new();
        trimmed
            .apply(|node| {
                if let Some(scan) = LeafPaths::of_scan(node.as_ref()) {
                    paths.push(scan);
                }
                Ok(TreeNodeRecursion::Continue)
            })
            .expect("walk");
        let [paths] = <[LeafPaths; 1]>::try_from(paths).expect("one scan");
        let actual = hot.run(trimmed).await;
        (paths, json_rows(&actual), json_rows(&expected))
    }

    /// Renders `batches` as JSON lines through the shared Wyrd JSON
    /// rendering, so a Variant compares by value rather than by encoding.
    ///
    /// # Panics
    ///
    /// Panics when a batch does not render.
    fn json_rows(batches: &[RecordBatch]) -> String {
        let mut writer = arrow::json::WriterBuilder::new()
            .with_explicit_nulls(true)
            .with_encoder_factory(Arc::new(wyrd_queue::variant::WyrdJsonEncoderFactory))
            .build::<_, arrow::json::writer::LineDelimited>(Vec::new());
        for batch in batches {
            writer.write(batch).expect("render");
        }
        writer.finish().expect("render");
        String::from_utf8(writer.into_inner()).expect("JSON text")
    }

    /// The [`LeafPaths`] of each named column.
    fn paths(columns: &[(&str, &[&[&str]])]) -> LeafPaths {
        LeafPaths(
            columns
                .iter()
                .map(|(column, paths)| {
                    (
                        (*column).to_owned(),
                        paths
                            .iter()
                            .map(|keys| keys.iter().map(|key| (*key).to_owned()).collect())
                            .collect(),
                    )
                })
                .collect(),
        )
    }

    /// Literal Variant paths that stay above the scan, in a filter `DataFusion`
    /// keeps above it or on `unnest` elements, trim the scan to their
    /// paths, and every trimmed plan returns exactly the rows stock
    /// `DataFusion` returns.
    ///
    /// # Panics
    ///
    /// Panics when a scan receives other paths or any row differs.
    #[tokio::test]
    async fn literal_key_reads_trim_the_scan_and_match_stock_datafusion() {
        let cases: [(&str, LeafPaths); 2] = [
            (
                "SELECT id FROM t WHERE events[1]['attributes'] ->> 'x' = '1' ORDER BY id",
                paths(&[("events", &[&["element", "attributes", "x"]])]),
            ),
            (
                "SELECT id, u['attributes'] ->> 'k' AS k \
                 FROM (SELECT id, unnest(events) AS u FROM t) ORDER BY id, k",
                paths(&[("events", &[&["element", "attributes", "k"]])]),
            ),
        ];
        for (sql, expected) in cases {
            let (paths, actual, stock) = trimmed_and_stock(sql).await;
            assert_eq!(paths, expected, "{sql}");
            assert_eq!(actual, stock, "{sql}");
        }
    }

    /// The rule leaves a column untrimmed, and the rows equal stock
    /// `DataFusion`'s, when a path is an expression `DataFusion` moved into
    /// the scan (the reader prunes Struct leaves and the file adapter reads
    /// shredded Variant paths there, including a common subexpression such
    /// as `events[1]` read twice), or when the column is used another way: a
    /// Struct or List path that reads no Variant, which `DataFusion` prunes, a
    /// bare Variant, Struct, or List use, a non-literal key or List index, a
    /// Variant array index, an `unnest` whose elements are used whole or not
    /// at all, or a path above an operator the walk does not see through. A
    /// window is such an operator: `DataFusion` does not move a path below
    /// it.
    ///
    /// # Panics
    ///
    /// Panics when such a scan is trimmed or any row differs.
    #[tokio::test]
    async fn other_uses_leave_the_column_untrimmed() {
        for sql in [
            "SELECT events[1] FROM t ORDER BY id",
            "SELECT events[1]['name'], events[1]['attributes'] ->> 'k' FROM t ORDER BY id",
            "SELECT events[1]['attributes'] ->> 'k' FROM t ORDER BY id",
            "SELECT events[2]['name'], events[1]['attributes'] FROM t ORDER BY id",
            "SELECT s['b'] FROM t ORDER BY id",
            "SELECT id FROM t WHERE s['a'] > 1 ORDER BY id",
            "SELECT s['b'], v ->> 'a' FROM t ORDER BY id",
            "SELECT v ->> 'a' FROM t ORDER BY id",
            "SELECT id FROM t WHERE v ->> 'b' = 'x' ORDER BY id",
            "SELECT v ->> 'a', v ->> 'c' FROM t WHERE (v ->> 'b') IS NOT NULL ORDER BY id",
            "SELECT id FROM t ORDER BY v ->> 'a' NULLS LAST, id",
            "SELECT v -> 'a' ->> 'x' FROM t ORDER BY id LIMIT 3",
            "SELECT id, v ->> 'c' FROM t ORDER BY id LIMIT 2",
            "SELECT v ->> 'b' AS b, count(*) FROM t GROUP BY v ->> 'b' ORDER BY b",
            "SELECT v FROM t ORDER BY id",
            "SELECT v ->> 'a' FROM t WHERE v IS NOT NULL ORDER BY id",
            "SELECT v ->> 'a', v FROM t ORDER BY id",
            "SELECT v ->> (CASE WHEN id = 1 THEN 'a' END) FROM t ORDER BY id",
            "SELECT v -> 'a' ->> 0 FROM t ORDER BY id",
            "SELECT id FROM t ORDER BY id",
            "SELECT s FROM t ORDER BY id",
            "SELECT s['b'] FROM t WHERE s IS NOT NULL ORDER BY id",
            "SELECT id, v ->> 'a', row_number() OVER (ORDER BY v ->> 'b', id) FROM t ORDER BY id",
            "SELECT events FROM t ORDER BY id",
            "SELECT events[id]['name'] FROM t ORDER BY id",
            "SELECT events[1]['attributes'] ->> 'k', events FROM t ORDER BY id",
            "SELECT id, u FROM (SELECT id, unnest(events) AS u FROM t) ORDER BY id, u['name']",
            "SELECT id FROM (SELECT id, unnest(events) AS u FROM t) ORDER BY id",
            "SELECT id, u['name'] AS name \
             FROM (SELECT id, unnest(events) AS u FROM t) ORDER BY id, name",
        ] {
            let (paths, actual, stock) = trimmed_and_stock(sql).await;
            assert_eq!(paths, LeafPaths::default(), "{sql}");
            assert_eq!(actual, stock, "{sql}");
        }
    }
}
