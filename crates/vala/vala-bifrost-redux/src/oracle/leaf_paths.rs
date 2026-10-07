//! The literal paths a query reads from each Struct and Variant column,
//! handed to the Oracle readers so they decode only those paths' leaves.
//!
//! [`LeafPathPushdown`] runs last among the physical optimizer rules of
//! every Oracle session. It walks the plan from the root, works out how each
//! column reaching an Oracle leaf is used, and gives the leaf the
//! [`LeafPaths`] of every column used only through literal paths:
//! literal-key `variant_get` calls on a Variant, `get_field` chains on a
//! Struct. The leaf reads a trimmed Variant through
//! `variant_unshred(v, paths...)`, which decodes `metadata` and those paths'
//! leaves, and a trimmed Struct through [`StructTrim`], which reads just
//! those fields. Any other use of a column, or an operator the walk does not
//! see through, reads it whole.
//!
//! A remote placeholder carries its paths to the follower in its codec
//! payload, where [`LeafPathPushdown::assign`] applies them to the
//! follower's own source.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use arrow::array::{Array, ArrayRef, StructArray, make_array, new_null_array};
use arrow::buffer::NullBuffer;
use arrow::datatypes::{DataType, Field, Fields, Schema, SchemaRef};
use datafusion::common::config::ConfigOptions;
use datafusion::common::tree_node::TreeNodeRecursion;
use datafusion::common::{ScalarValue, internal_datafusion_err};
use datafusion::error::Result;
use datafusion::logical_expr::{
    ColumnarValue, ScalarFunctionArgs, ScalarUDF, ScalarUDFImpl, Signature, Volatility,
};
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
use parquet::arrow::ProjectionMask;
use parquet::schema::types::SchemaDescriptor;
use serde::{Deserialize, Serialize};
use wyrd_types::variant::is_variant;

use super::variant_sql::OracleVariantSql;

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
    /// [`OracleVariantSql::trimmed`] when it is a Variant and through
    /// [`StructTrim`] when it is a Struct.
    pub(crate) fn projection(&self, schema: &SchemaRef) -> ProjectionExprs {
        ProjectionExprs::new(schema.fields().iter().enumerate().map(|(index, field)| {
            let column: Arc<dyn PhysicalExpr> = Arc::new(Column::new(field.name(), index));
            let expr = match self.0.get(field.name()) {
                Some(paths) if is_variant(field) => OracleVariantSql::shared()
                    .trimmed(column, &paths.iter().cloned().collect::<Vec<_>>()),
                Some(paths) => StructTrim::expr(schema, index, paths)
                    .expect("a Struct path read plans over its own schema"),
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

/// Internal name of [`StructTrim`].
const STRUCT_TRIM: &str = "struct_trim";

/// Rebuilds a Struct column at its full type from the `get_field` reads of
/// only its literal paths.
///
/// Its arguments are one `get_field(column, path...)` per path, so the
/// per-file plan decodes just those leaves, as for any field access, even
/// when the file's Struct type differs from the table's. Every other child
/// is a placeholder (null, or zero where the child is non-nullable), and a
/// row is null only where a non-nullable child read is null. Only
/// [`LeafPathPushdown`] places it, and only when every use of the column
/// is one of its paths, which read the same values from the rebuilt Struct.
/// Never registered as SQL.
#[derive(Debug, PartialEq, Eq, Hash)]
struct StructTrim {
    /// The Struct field being rebuilt.
    field: Arc<Field>,
    /// The field-name path each argument reads, in argument order.
    paths: Vec<Vec<String>>,
    /// One `get_field` argument per path.
    signature: Signature,
}

impl StructTrim {
    /// Returns the read of the Struct `column`, at index `index` of
    /// `schema`, through only `paths`.
    ///
    /// Each path is cut where it leaves nested Structs, so a path into a Map
    /// or List child reads that child whole.
    ///
    /// # Errors
    ///
    /// Returns the `DataFusion` error raised when a `get_field` read does
    /// not plan over `schema`.
    fn expr(
        schema: &Schema,
        index: usize,
        paths: &BTreeSet<Vec<String>>,
    ) -> Result<Arc<dyn PhysicalExpr>> {
        let field = Arc::clone(&schema.fields()[index]);
        let paths = paths
            .iter()
            .map(|path| {
                let mut level = field.data_type();
                path.iter()
                    .take_while(|name| {
                        let DataType::Struct(fields) = level else {
                            return false;
                        };
                        fields.find(name).is_some_and(|(_, child)| {
                            level = child.data_type();
                            true
                        })
                    })
                    .cloned()
                    .collect::<Vec<_>>()
            })
            .filter(|path| !path.is_empty())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        let column: Arc<dyn PhysicalExpr> = Arc::new(Column::new(field.name(), index));
        let args = paths
            .iter()
            .map(|path| {
                let mut args = vec![Arc::clone(&column)];
                args.extend(path.iter().map(|name| {
                    Arc::new(Literal::new(ScalarValue::from(name.as_str())))
                        as Arc<dyn PhysicalExpr>
                }));
                Ok(Arc::new(ScalarFunctionExpr::try_new(
                    datafusion::functions::core::get_field(),
                    args,
                    schema,
                    Arc::new(ConfigOptions::default()),
                )?) as Arc<dyn PhysicalExpr>)
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(Arc::new(ScalarFunctionExpr::new(
            STRUCT_TRIM,
            Arc::new(ScalarUDF::new_from_impl(Self {
                field: Arc::clone(&field),
                paths,
                signature: Signature::variadic_any(Volatility::Immutable),
            })),
            args,
            field,
            Arc::new(ConfigOptions::default()),
        )))
    }

    /// Builds a Struct of `fields` from `reads`, each the remaining path of
    /// one read and its values.
    ///
    /// # Errors
    ///
    /// Returns the Arrow error raised when a placeholder or the Struct is
    /// invalid.
    fn rebuild(fields: &Fields, reads: &[(&[String], &ArrayRef)], len: usize) -> Result<ArrayRef> {
        let mut nulls = None;
        let children = fields
            .iter()
            .map(|field| {
                let under = reads
                    .iter()
                    .filter_map(|(path, values)| {
                        let (name, tail) = path.split_first()?;
                        (name == field.name()).then_some((tail, *values))
                    })
                    .collect::<Vec<_>>();
                let child =
                    if let Some((_, values)) = under.iter().find(|(tail, _)| tail.is_empty()) {
                        Arc::clone(values)
                    } else if let (DataType::Struct(children), false) =
                        (field.data_type(), under.is_empty())
                    {
                        Self::rebuild(children, &under, len)?
                    } else {
                        return Self::placeholder(field, len);
                    };
                if !field.is_nullable() {
                    nulls = NullBuffer::union(nulls.as_ref(), child.logical_nulls().as_ref());
                }
                Ok(child)
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(Arc::new(StructArray::try_new(
            fields.clone(),
            children,
            nulls,
        )?))
    }

    /// A never-read stand-in for `field`: all null, or for a non-nullable
    /// field the same zeroed values with no null buffer.
    ///
    /// # Errors
    ///
    /// Returns the Arrow error raised when the zeroed data is invalid.
    fn placeholder(field: &Field, len: usize) -> Result<ArrayRef> {
        let nulls = new_null_array(field.data_type(), len);
        if field.is_nullable() {
            return Ok(nulls);
        }
        Ok(make_array(
            nulls.into_data().into_builder().nulls(None).build()?,
        ))
    }
}

impl ScalarUDFImpl for StructTrim {
    /// The internal name readers project.
    fn name(&self) -> &str {
        STRUCT_TRIM
    }

    /// One `get_field` argument per path.
    fn signature(&self) -> &Signature {
        &self.signature
    }

    /// The rebuilt Struct's full type.
    ///
    /// # Errors
    ///
    /// Never fails.
    fn return_type(&self, _arg_types: &[DataType]) -> Result<DataType> {
        Ok(self.field.data_type().clone())
    }

    /// Rebuilds every row at the full type from the path reads.
    ///
    /// # Errors
    ///
    /// Returns the error [`StructTrim::rebuild`] raises, or an internal error
    /// when the rebuilt column is not a Struct.
    fn invoke_with_args(&self, args: ScalarFunctionArgs) -> Result<ColumnarValue> {
        let DataType::Struct(fields) = self.field.data_type() else {
            return Err(internal_datafusion_err!("struct_trim rebuilds a Struct"));
        };
        let values = ColumnarValue::values_to_arrays(&args.args)?;
        let reads = self
            .paths
            .iter()
            .map(Vec::as_slice)
            .zip(&values)
            .collect::<Vec<_>>();
        Ok(ColumnarValue::Array(Self::rebuild(
            fields,
            &reads,
            args.number_rows,
        )?))
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

    /// Records every column use inside `expr`, evaluated over `schema`, into
    /// `columns`.
    ///
    /// A literal path over a column of its kind uses that path: a
    /// literal-key `variant_get` over a Variant column, or a literal
    /// `get_field` chain over a Struct column. Any other column reference
    /// uses the whole column.
    fn collect(expr: &Arc<dyn PhysicalExpr>, schema: &Schema, columns: &mut [Self]) {
        if let Some((column, keys)) = Self::path(expr, schema) {
            if let Some(demand) = columns.get_mut(column.index()) {
                demand.merge(Self::Paths(BTreeSet::from([keys])));
            }
            return;
        }
        if let Some(column) = expr.downcast_ref::<Column>() {
            if let Some(demand) = columns.get_mut(column.index()) {
                demand.merge(Self::Whole);
            }
            return;
        }
        for child in expr.children() {
            Self::collect(child, schema, columns);
        }
    }

    /// Returns the column and literal path of `expr` when it reads one path
    /// of a column of the matching kind in `schema`.
    fn path<'e>(
        expr: &'e Arc<dyn PhysicalExpr>,
        schema: &Schema,
    ) -> Option<(&'e Column, Vec<String>)> {
        let field = |column: &Column| schema.fields().get(column.index());
        if let Some((column, keys)) = OracleVariantSql::literal_key_path(expr) {
            return field(column)
                .is_some_and(|field| is_variant(field))
                .then_some((column, keys));
        }
        let (column, names) = Self::field_path(expr)?;
        field(column)
            .is_some_and(|field| {
                matches!(field.data_type(), DataType::Struct(_)) && !is_variant(field)
            })
            .then_some((column, names))
    }

    /// Returns the column and names of `expr` when it is a `get_field` chain
    /// over a column with only literal string names.
    fn field_path(expr: &Arc<dyn PhysicalExpr>) -> Option<(&Column, Vec<String>)> {
        let call = expr.downcast_ref::<ScalarFunctionExpr>()?;
        if call.name() != "get_field" {
            return None;
        }
        let (base, names) = call.args().split_first()?;
        let (column, mut path) = match base.downcast_ref::<Column>() {
            Some(column) => (column, Vec::new()),
            None => Self::field_path(base)?,
        };
        for name in names {
            path.push(
                name.downcast_ref::<Literal>()?
                    .value()
                    .try_as_str()
                    .flatten()?
                    .to_owned(),
            );
        }
        (!names.is_empty()).then_some((column, path))
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
                    Some(column) => {
                        if let Some(slot) = input.get_mut(column.index()) {
                            slot.merge(used);
                        }
                    }
                    None => Demand::collect(&expr.expr, &projection.input().schema(), &mut input),
                }
            }
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
                        if let Some(slot) = input.get_mut(index) {
                            slot.merge(used);
                        }
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
    use arrow::array::{AsArray, RecordBatch};
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

    /// Literal-key reads and Struct field reads in outputs, filters, sort
    /// keys, nested chains, and under sorts, limits, and grouping trim the
    /// scan to their
    /// paths, and every trimmed plan returns exactly the rows stock
    /// `DataFusion` returns.
    ///
    /// # Panics
    ///
    /// Panics when a scan receives other paths or any row differs.
    #[tokio::test]
    async fn literal_key_reads_trim_the_scan_and_match_stock_datafusion() {
        let cases: [(&str, LeafPaths); 10] = [
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

    /// A bare Variant or Struct use, a non-literal key, an array index, or a
    /// path used
    /// above an operator the walk does not see through reads the column
    /// whole, alone or beside literal-key reads, and the rows equal stock
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
        ] {
            let (paths, actual, stock) = trimmed_and_stock(sql).await;
            assert_eq!(paths, LeafPaths::default(), "{sql}");
            assert_eq!(actual, stock, "{sql}");
        }
    }

    /// Rows of `s['b']` and of `v ->> 'a'` from `batch`: what the two paths
    /// read. The rebuilt Struct's own nulls are not among them; no path
    /// reads them.
    ///
    /// # Panics
    ///
    /// Panics when `batch` lacks a column or a path does not evaluate.
    fn read_values(batch: &RecordBatch) -> (Vec<Option<String>>, String) {
        let s = batch.column_by_name("s").expect("s").as_struct();
        let b = s.column_by_name("b").expect("s.b").as_string::<i32>();
        let b = (0..s.len())
            .map(|row| (s.is_valid(row) && b.is_valid(row)).then(|| b.value(row).to_owned()))
            .collect();
        let v = batch.column_by_name("v").expect("v");
        let a = (0..v.len())
            .map(|row| {
                wyrd_queue::variant::variant_cell_to_json(v.as_ref(), row)
                    .expect("variant")
                    .get("a")
                    .cloned()
            })
            .collect::<Vec<_>>();
        (b, format!("{a:?}"))
    }

    /// A Struct read for one field on a hot file decodes only that field's
    /// leaf and reads the same field values as a whole read; a Variant path
    /// beside it decodes only its own leaves.
    ///
    /// # Panics
    ///
    /// Panics when the read decodes other leaves or any value differs.
    #[test]
    fn trimmed_struct_reads_only_its_paths_leaves() {
        let schema = logical_schema();
        let file = write_file(true);
        let footer =
            ArrowReaderMetadata::load(&file, ArrowReaderOptions::new()).expect("valid footer");
        let plan = |paths: &LeafPaths| {
            FileReadPlan::new(&footer, &schema, paths.projection(&schema), None, 20)
                .expect("per-file plan")
        };
        let trimmed_paths = paths(&[("s", &[&["b"]]), ("v", &[&["a"]])]);
        let trimmed = plan(&trimmed_paths);
        assert_eq!(
            decoded_leaves(&file, &trimmed),
            [
                "id",
                "s.b",
                "v.metadata",
                "v.typed_value.a.value",
                "v.typed_value.a.typed_value",
            ]
        );
        let whole = plan(&LeafPaths::default());
        assert_eq!(
            read_values(&read(&file, trimmed)),
            read_values(&read(&file, whole))
        );
    }

    /// A published file read through the Iceberg reader with a Struct field
    /// and a Variant path decodes only their leaves, and the paths read the
    /// same values as from a whole read.
    ///
    /// # Panics
    ///
    /// Panics when the reader fails, decodes other leaves, or any value
    /// differs.
    #[tokio::test]
    async fn published_reads_decode_only_struct_and_variant_paths() {
        use futures_util::TryStreamExt as _;
        use iceberg::arrow::ParquetFileReadPlanner as _;
        use iceberg::spec::{NestedField, PrimitiveType, StructType, Type, VariantType};

        let directory = tempfile::tempdir().expect("fixture directory");
        let path = directory.path().join("published.parquet");
        let file = write_file(true);
        std::fs::write(&path, &file).expect("fixture file");
        let iceberg_schema = Arc::new(
            iceberg::spec::Schema::builder()
                .with_fields(vec![
                    Arc::new(NestedField::required(
                        1,
                        "id",
                        Type::Primitive(PrimitiveType::Long),
                    )),
                    Arc::new(NestedField::optional(
                        2,
                        "s",
                        Type::Struct(StructType::new(vec![
                            Arc::new(NestedField::required(
                                3,
                                "a",
                                Type::Primitive(PrimitiveType::Long),
                            )),
                            Arc::new(NestedField::optional(
                                4,
                                "b",
                                Type::Primitive(PrimitiveType::String),
                            )),
                        ])),
                    )),
                    Arc::new(NestedField::optional(5, "v", Type::Variant(VariantType))),
                ])
                .build()
                .expect("task schema"),
        );
        let read = |paths: LeafPaths| {
            let task = iceberg::scan::FileScanTask::builder()
                .with_file_size_in_bytes(std::fs::metadata(&path).expect("size").len())
                .with_start(0)
                .with_length(0)
                .with_data_file_path(path.to_string_lossy().into_owned())
                .with_data_file_format(iceberg::spec::DataFileFormat::Parquet)
                .with_schema(Arc::clone(&iceberg_schema))
                .with_project_field_ids(vec![1, 2, 5])
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
        let trimmed_paths = paths(&[("s", &[&["b"]]), ("v", &[&["a"]])]);

        let footer =
            ArrowReaderMetadata::load(&file, ArrowReaderOptions::new()).expect("valid footer");
        let narrowing =
            PublishedFileReadPlanner::new(logical_schema(), None, trimmed_paths.clone(), 20)
                .plan(&footer, vec![0])
                .expect("narrowing");
        let mask = narrowing.projection.expect("trimmed mask");
        let descriptor = footer.metadata().file_metadata().schema_descr();
        assert_eq!(
            (0..descriptor.num_columns())
                .filter(|&leaf| mask.leaf_included(leaf))
                .map(|leaf| descriptor.column(leaf).path().string())
                .collect::<Vec<_>>(),
            [
                "id",
                "s.b",
                "v.metadata",
                "v.typed_value.a.value",
                "v.typed_value.a.typed_value",
            ]
        );
        let whole = read_values(&read(LeafPaths::default()).await);
        assert_eq!(read_values(&read(trimmed_paths).await), whole);
    }
}
