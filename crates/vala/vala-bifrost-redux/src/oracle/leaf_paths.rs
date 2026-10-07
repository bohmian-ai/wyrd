//! The literal paths a query reads from each Struct, List, and Variant
//! column, handed to the Oracle readers so they decode only those paths'
//! leaves.
//!
//! [`LeafPathPushdown`] runs last among the physical optimizer rules of
//! every Oracle session. It walks the plan from the root, works out how each
//! column reaching an Oracle leaf is used, and gives the leaf the
//! [`LeafPaths`] of every column used only through literal paths: chains of
//! `get_field` on a Struct, `array_element` with a literal index on a List,
//! and literal-key `variant_get` on a Variant, in any order the types
//! allow, plus the same paths below an `unnest` of a List column. The leaf
//! reads each such column through [`OracleVariantSql::trimmed`], which
//! decodes only those paths' leaves. Any other use of a column, or an
//! operator the walk does not see through, reads it whole.
//!
//! A remote placeholder carries its paths to the follower in its codec
//! payload, where [`LeafPathPushdown::assign`] applies them to the
//! follower's own source.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use arrow::datatypes::{DataType, FieldRef, Schema, SchemaRef};
use datafusion::common::config::ConfigOptions;
use datafusion::common::tree_node::TreeNodeRecursion;
use datafusion::error::Result;
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
use parquet::arrow::ProjectionMask;
use parquet::schema::types::SchemaDescriptor;
use serde::{Deserialize, Serialize};
use wyrd_types::variant::is_variant;

use super::variant_sql::{OracleVariantSql, VARIANT_GET};

/// The literal paths read from each trimmed Struct or Variant column of one
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

    /// Restricts `planned` to the leaves of trimmed columns: every leaf of
    /// another root column stays read, and a trimmed column keeps only the
    /// leaves `planned` selects in it.
    ///
    /// A reader that chooses columns by its own rules (the Iceberg reader
    /// matches field ids) intersects this mask with its own, so the trimming
    /// can only drop leaves of the trimmed columns.
    pub(crate) fn leaf_mask(
        &self,
        planned: &ProjectionMask,
        descriptor: &SchemaDescriptor,
    ) -> ProjectionMask {
        ProjectionMask::leaves(
            descriptor,
            (0..descriptor.num_columns()).filter(|&leaf| {
                let column = descriptor.column(leaf);
                !column
                    .path()
                    .parts()
                    .first()
                    .is_some_and(|root| self.0.contains_key(root))
                    || planned.leaf_included(leaf)
            }),
        )
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
    /// A literal path over a column (see [`Self::path`]) uses that path. Any
    /// other column reference uses the whole column.
    fn collect(expr: &Arc<dyn PhysicalExpr>, schema: &Schema, columns: &mut [Self]) {
        if let Some((column, path, _)) = Self::path(expr, schema)
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
    /// Walks `plan` from its root, every root output read whole.
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
    /// Applies `paths` to a follower's resolved source `plan`, whose outputs
    /// are the columns the paths name; every other output is read whole.
    ///
    /// # Errors
    ///
    /// Returns the `DataFusion` error raised when a rebuilt operator rejects
    /// its new children.
    pub(crate) fn assign(
        plan: Arc<dyn ExecutionPlan>,
        paths: &LeafPaths,
    ) -> Result<Arc<dyn ExecutionPlan>> {
        if paths.is_empty() {
            return Ok(plan);
        }
        let demand = plan
            .schema()
            .fields()
            .iter()
            .map(|field| match paths.0.get(field.name()) {
                Some(paths) => Demand::Paths(paths.clone()),
                None => Demand::Whole,
            })
            .collect();
        Self::push(plan, demand)
    }

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
    /// A remote placeholder keeps the paths for its follower and applies them
    /// to the leader-readable leaf it holds.
    ///
    /// # Errors
    ///
    /// Returns the `DataFusion` error raised while applying the paths to a
    /// placeholder's local leaf.
    fn leaf(
        plan: &Arc<dyn ExecutionPlan>,
        demand: &[Demand],
    ) -> Result<Option<Arc<dyn ExecutionPlan>>> {
        let paths = || Self::paths(plan, demand);
        Ok(
            if let Some(hot) = plan.downcast_ref::<super::exec::HotParquetExec>() {
                Some(Arc::new(hot.clone().with_leaf_paths(paths())))
            } else if let Some(published) =
                plan.downcast_ref::<super::exec::OracleIcebergScanExec>()
            {
                Some(Arc::new(published.clone().with_leaf_paths(paths())))
            } else if let Some(placeholder) =
                plan.downcast_ref::<super::codec::RemoteSourcePlaceholderExec>()
            {
                Some(Arc::new(placeholder.clone().with_leaf_paths(paths())?))
            } else {
                None
            },
        )
    }

    /// Returns the paths of each Variant or Struct output of `leaf` used only
    /// through literal paths.
    fn paths(leaf: &Arc<dyn ExecutionPlan>, demand: &[Demand]) -> LeafPaths {
        LeafPaths(
            leaf.schema()
                .fields()
                .iter()
                .zip(demand)
                .filter_map(|(field, used)| match used {
                    Demand::Paths(paths) => Some((field.name().clone(), paths.clone())),
                    _ => None,
                })
                .collect(),
        )
    }
}

#[cfg(test)]
mod tests {
    use arrow::array::{Array, AsArray, RecordBatch};
    use datafusion::common::tree_node::{Transformed, TreeNode as _};
    use datafusion::datasource::MemTable;
    use datafusion::execution::SessionStateBuilder;
    use datafusion::physical_plan::collect;
    use datafusion::prelude::SessionContext;
    use parquet::arrow::arrow_reader::{ArrowReaderMetadata, ArrowReaderOptions};

    use super::super::exec::tests::{HotCausalFixture, hot_exec_for};
    use super::super::exec::{HotParquetExec, HotParquetPlan, OracleScanMetricsHandle};
    use super::super::nested_pushdown::tests::{decoded_leaves, logical_schema, read, write_file};
    use super::super::nested_pushdown::{FileReadPlan, PublishedFileReadPlanner};
    use super::*;
    use bytes::Bytes;

    /// Plans `sql` with stock `DataFusion` over an in-memory copy of the
    /// fixture, then swaps the scan for a hot leaf over the shredded fixture
    /// file and runs [`LeafPathPushdown`] on that plan.
    ///
    /// Returns the paths the hot leaf received, the rows the trimmed plan
    /// returns, and the rows stock `DataFusion` returns for the same query,
    /// both as [`json_rows`].
    ///
    /// # Panics
    ///
    /// Panics when the query does not plan or execute, or the plan does not
    /// hold exactly one scan.
    async fn trimmed_and_stock(sql: &str) -> (LeafPaths, String, String) {
        let schema = logical_schema();
        let unshredded = write_file(false);
        let footer = ArrowReaderMetadata::load(&unshredded, ArrowReaderOptions::new())
            .expect("valid footer");
        let rows = read(
            &unshredded,
            FileReadPlan::new(
                &footer,
                &schema,
                LeafPaths::default().projection(&schema),
                None,
                20,
            )
            .expect("whole read"),
        );
        let state = OracleVariantSql::shared()
            .install(SessionStateBuilder::new().with_default_features())
            .build();
        let context = SessionContext::new_with_state(state);
        context
            .register_table(
                "t",
                Arc::new(MemTable::try_new(rows.schema(), vec![vec![rows]]).expect("table")),
            )
            .expect("register");
        // Each run gets its own planned instance: executing a plan leaves
        // state behind (a TopK's dynamic filter), so a reused plan would not
        // start from what the rule sees at planning.
        let plan = || async {
            context
                .sql(sql)
                .await
                .expect(sql)
                .create_physical_plan()
                .await
                .expect(sql)
        };
        let expected = collect(plan().await, context.task_ctx()).await.expect(sql);

        let directory = tempfile::tempdir().expect("fixture directory");
        let path = directory.path().join("shredded.parquet");
        let bytes = write_file(true);
        std::fs::write(&path, &bytes).expect("fixture file");
        let size = bytes.len();
        let fixture = HotCausalFixture {
            _directory: directory,
            path,
            schema: Arc::clone(&schema),
            bytes,
        };
        let metrics = Arc::new(OracleScanMetricsHandle::default());
        let swapped = plan()
            .await
            .transform_up(|node| {
                if !node.children().is_empty() {
                    return Ok(Transformed::no(node));
                }
                let hot: Arc<dyn ExecutionPlan> = Arc::new(hot_exec_for(
                    &fixture,
                    HotParquetPlan::Follower {
                        memory_pool: crate::resources::bounded_memory_pool(1 << 30),
                    },
                    &metrics,
                    size,
                ));
                let columns = node
                    .schema()
                    .fields()
                    .iter()
                    .map(|field| {
                        let index = schema.index_of(field.name()).expect("scan column");
                        ProjectionExpr::new(
                            Arc::new(Column::new(field.name(), index)),
                            field.name(),
                        )
                    })
                    .collect::<Vec<_>>();
                Ok(Transformed::yes(Arc::new(ProjectionExec::try_new(
                    columns, hot,
                )?)))
            })
            .expect("swap scan")
            .data;
        let trimmed = LeafPathPushdown
            .optimize(swapped, &ConfigOptions::default())
            .expect("pushdown");
        let mut paths = Vec::new();
        trimmed
            .apply(|node| {
                if let Some(hot) = node.downcast_ref::<HotParquetExec>() {
                    paths.push(hot.leaf_paths.clone());
                }
                Ok(TreeNodeRecursion::Continue)
            })
            .expect("walk");
        let [paths] = <[LeafPaths; 1]>::try_from(paths).expect("one scan");
        let actual = collect(trimmed, context.task_ctx()).await.expect(sql);
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

    /// The paths of `v` in a [`LeafPaths`].
    fn v(keys: &[&[&str]]) -> LeafPaths {
        paths(&[("v", keys)])
    }

    /// Literal-key reads, Struct field reads, List element reads at a
    /// literal index, and the same paths on `unnest` elements, in outputs,
    /// filters, sort keys, nested chains, and under sorts, limits, and
    /// grouping trim the scan to their paths, and every trimmed plan returns exactly the rows stock
    /// `DataFusion` returns.
    ///
    /// # Panics
    ///
    /// Panics when a scan receives other paths or any row differs.
    #[tokio::test]
    async fn literal_key_reads_trim_the_scan_and_match_stock_datafusion() {
        let cases: [(&str, LeafPaths); 16] = [
            (
                "SELECT events[1] FROM t ORDER BY id",
                paths(&[("events", &[&["element"]])]),
            ),
            (
                "SELECT events[1]['attributes'] ->> 'k' FROM t ORDER BY id",
                paths(&[("events", &[&["element", "attributes", "k"]])]),
            ),
            (
                "SELECT events[2]['name'], events[1]['attributes'] FROM t ORDER BY id",
                paths(&[(
                    "events",
                    &[&["element", "name"], &["element", "attributes"]],
                )]),
            ),
            (
                "SELECT id FROM t WHERE events[1]['attributes'] ->> 'x' = '1' ORDER BY id",
                paths(&[("events", &[&["element", "attributes", "x"]])]),
            ),
            (
                "SELECT id, u['attributes'] ->> 'k' AS k \
                 FROM (SELECT id, unnest(events) AS u FROM t) ORDER BY id, k",
                paths(&[("events", &[&["element", "attributes", "k"]])]),
            ),
            (
                "SELECT id, u['name'] AS name \
                 FROM (SELECT id, unnest(events) AS u FROM t) ORDER BY id, name",
                paths(&[("events", &[&["element", "name"]])]),
            ),
            (
                "SELECT s['b'] FROM t ORDER BY id",
                paths(&[("s", &[&["b"]])]),
            ),
            (
                "SELECT id FROM t WHERE s['a'] > 1 ORDER BY id",
                paths(&[("s", &[&["a"]])]),
            ),
            (
                "SELECT s['b'], v ->> 'a' FROM t ORDER BY id",
                paths(&[("s", &[&["b"]]), ("v", &[&["a"]])]),
            ),
            ("SELECT v ->> 'a' FROM t ORDER BY id", v(&[&["a"]])),
            (
                "SELECT id FROM t WHERE v ->> 'b' = 'x' ORDER BY id",
                v(&[&["b"]]),
            ),
            (
                "SELECT v ->> 'a', v ->> 'c' FROM t WHERE (v ->> 'b') IS NOT NULL ORDER BY id",
                v(&[&["a"], &["b"], &["c"]]),
            ),
            (
                "SELECT id FROM t ORDER BY v ->> 'a' NULLS LAST, id",
                v(&[&["a"]]),
            ),
            (
                "SELECT v -> 'a' ->> 'x' FROM t ORDER BY id LIMIT 3",
                v(&[&["a", "x"]]),
            ),
            (
                "SELECT id, v ->> 'c' FROM t ORDER BY id LIMIT 2",
                v(&[&["c"]]),
            ),
            (
                "SELECT v ->> 'b' AS b, count(*) FROM t GROUP BY v ->> 'b' ORDER BY b",
                v(&[&["b"]]),
            ),
        ];
        for (sql, expected) in cases {
            let (paths, actual, stock) = trimmed_and_stock(sql).await;
            assert_eq!(paths, expected, "{sql}");
            assert_eq!(actual, stock, "{sql}");
        }
    }

    /// A bare Variant, Struct, or List use, a
    /// non-literal key or List index, a Variant array index, an `unnest`
    /// whose elements are used whole or not at all, or a path used above an
    /// operator the walk does not see through reads the column whole, alone or beside literal-key reads, and the rows equal stock
    /// `DataFusion`'s. A window is such an operator: `DataFusion` does not
    /// move a path below it.
    ///
    /// # Panics
    ///
    /// Panics when such a scan is trimmed or any row differs.
    #[tokio::test]
    async fn other_uses_read_the_column_whole() {
        for sql in [
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
        ] {
            let (paths, actual, stock) = trimmed_and_stock(sql).await;
            assert_eq!(paths, LeafPaths::default(), "{sql}");
            assert_eq!(actual, stock, "{sql}");
        }
    }

    /// Rows of `s['b']`, of `v ->> 'a'`, and of every event's
    /// `attributes ->> 'k'` from `batch`: what the three paths read. The
    /// rebuilt Struct's own nulls are not among them; no path reads them.
    ///
    /// # Panics
    ///
    /// Panics when `batch` lacks a column or a path does not evaluate.
    fn read_values(batch: &RecordBatch) -> (Vec<Option<String>>, String) {
        let structs = batch.column_by_name("s").expect("s").as_struct();
        let child = structs.column_by_name("b").expect("s.b").as_string::<i32>();
        let b = (0..structs.len())
            .map(|row| {
                (structs.is_valid(row) && child.is_valid(row)).then(|| child.value(row).to_owned())
            })
            .collect();
        let key = |variant: &dyn Array, row: usize, key: &str| {
            wyrd_queue::variant::variant_cell_to_json(variant, row)
                .expect("variant")
                .get(key)
                .cloned()
        };
        let v = batch.column_by_name("v").expect("v");
        let a = (0..v.len())
            .map(|row| key(v.as_ref(), row, "a"))
            .collect::<Vec<_>>();
        let events = batch
            .column_by_name("events")
            .expect("events")
            .as_list::<i32>();
        let k = (0..events.len())
            .map(|row| {
                events.is_valid(row).then(|| {
                    let elements = events.value(row);
                    let attributes = elements
                        .as_struct()
                        .column_by_name("attributes")
                        .expect("attributes");
                    (0..attributes.len())
                        .map(|element| key(attributes.as_ref(), element, "k"))
                        .collect::<Vec<_>>()
                })
            })
            .collect::<Vec<_>>();
        (b, format!("{a:?} {k:?}"))
    }

    /// The paths every trimmed-read test reads: one Struct field, one
    /// Variant key, and one key of the Variant inside every List element.
    fn trimmed_paths() -> LeafPaths {
        paths(&[
            ("s", &[&["b"]]),
            ("v", &[&["a"]]),
            ("events", &[&["element", "attributes", "k"]]),
        ])
    }

    /// The Parquet leaves [`trimmed_paths`] decode from a shredded fixture
    /// file: `id` whole, `s.b`, and for each Variant its `metadata` and the
    /// key's shredded subtree, not the event `name`, the other shredded key
    /// `x`, or either residual `value`.
    const TRIMMED_LEAVES: [&str; 8] = [
        "id",
        "s.b",
        "v.metadata",
        "v.typed_value.a.value",
        "v.typed_value.a.typed_value",
        "events.list.element.attributes.metadata",
        "events.list.element.attributes.typed_value.k.value",
        "events.list.element.attributes.typed_value.k.typed_value",
    ];

    /// A hot read of a Struct field, a Variant key, and a key of the Variant
    /// inside every List element decodes only those paths' leaves, and reads
    /// the same values as a whole read of an all-residual file.
    ///
    /// # Panics
    ///
    /// Panics when the read decodes other leaves or any value differs.
    #[test]
    fn trimmed_struct_reads_only_its_paths_leaves() {
        let schema = logical_schema();
        let plan = |file: &Bytes, paths: &LeafPaths| {
            let footer =
                ArrowReaderMetadata::load(file, ArrowReaderOptions::new()).expect("valid footer");
            FileReadPlan::new(&footer, &schema, paths.projection(&schema), None, 20)
                .expect("per-file plan")
        };
        let file = write_file(true);
        let trimmed = plan(&file, &trimmed_paths());
        assert_eq!(decoded_leaves(&file, &trimmed), TRIMMED_LEAVES);
        let residual = write_file(false);
        let whole = plan(&residual, &LeafPaths::default());
        assert_eq!(
            read_values(&read(&file, trimmed)),
            read_values(&read(&residual, whole))
        );
    }

    /// A published file read through the Iceberg reader with a Struct
    /// field, a Variant key, and a key of the Variant inside every List
    /// element decodes only their leaves, and the paths read the same values
    /// as a whole published read of an all-residual file.
    ///
    /// # Panics
    ///
    /// Panics when the reader fails, decodes other leaves, or any value
    /// differs.
    #[tokio::test]
    async fn published_reads_decode_only_struct_and_variant_paths() {
        use futures_util::TryStreamExt as _;
        use iceberg::arrow::ParquetFileReadPlanner as _;

        let directory = tempfile::tempdir().expect("fixture directory");
        let path = directory.path().join("published.parquet");
        let file = write_file(true);
        std::fs::write(&path, &file).expect("fixture file");
        let residual_path = directory.path().join("residual.parquet");
        let residual = write_file(false);
        std::fs::write(&residual_path, &residual).expect("fixture file");
        let iceberg_schema = Arc::new(
            iceberg::arrow::arrow_schema_to_schema(
                ArrowReaderMetadata::load(&residual, ArrowReaderOptions::new())
                    .expect("valid footer")
                    .schema(),
            )
            .expect("the fixture carries every field id"),
        );
        let read = |path: &std::path::Path, paths: LeafPaths| {
            let task = iceberg::scan::FileScanTask::builder()
                .with_file_size_in_bytes(std::fs::metadata(path).expect("size").len())
                .with_start(0)
                .with_length(0)
                .with_data_file_path(path.to_string_lossy().into_owned())
                .with_data_file_format(iceberg::spec::DataFileFormat::Parquet)
                .with_schema(Arc::clone(&iceberg_schema))
                .with_project_field_ids(vec![1, 2, 5, 6])
                .with_case_sensitive(false)
                .build();
            let planner = PublishedFileReadPlanner::new(logical_schema(), None, paths, 20);
            async move {
                let batches = iceberg::arrow::ArrowReaderBuilder::new(
                    iceberg::io::FileIO::new_with_fs(),
                    iceberg::Runtime::current(),
                )
                .with_parquet_file_read_planner(Arc::new(planner))
                .build()
                .read(Box::pin(futures_util::stream::iter(vec![Ok(task)])))
                .expect("published reader")
                .stream()
                .try_collect::<Vec<_>>()
                .await
                .expect("published rows");
                arrow::compute::concat_batches(&batches[0].schema(), &batches).expect("concat")
            }
        };
        let footer =
            ArrowReaderMetadata::load(&file, ArrowReaderOptions::new()).expect("valid footer");
        let narrowing = PublishedFileReadPlanner::new(logical_schema(), None, trimmed_paths(), 20)
            .plan(&footer, vec![0])
            .expect("narrowing");
        let mask = narrowing.projection.expect("trimmed mask");
        let descriptor = footer.metadata().file_metadata().schema_descr();
        assert_eq!(
            (0..descriptor.num_columns())
                .filter(|&leaf| mask.leaf_included(leaf))
                .map(|leaf| descriptor.column(leaf).path().string())
                .collect::<Vec<_>>(),
            TRIMMED_LEAVES
        );
        let whole = read_values(&read(&residual_path, LeafPaths::default()).await);
        assert_eq!(read_values(&read(&path, trimmed_paths()).await), whole);
    }
}
