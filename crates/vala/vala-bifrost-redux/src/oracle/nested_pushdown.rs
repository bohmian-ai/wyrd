//! The per-file Parquet read plan, for tests.
//!
//! Every Oracle file is read by `DataFusion`'s own `ParquetSource`, which plans
//! each file through `DataFusion`'s shared [`PerFileParquetReadPlanner`] and its
//! default expression adapter. [`FileReadPlan`] calls that same planner on one
//! file so tests can assert which leaves, row groups, and pages a Struct field
//! (`get_field`) or Variant path (`variant_get`) selects.

use std::sync::Arc;

use arrow::datatypes::SchemaRef;
use datafusion::datasource::physical_plan::parquet::{
    ParquetAccessPlan, ParquetFileMetrics, PerFileParquetReadInput, PerFileParquetReadPlan,
    PerFileParquetReadPlanner, RowGroupAccessPlanFilter,
};
use datafusion::error::Result as DataFusionResult;
use datafusion::physical_expr::PhysicalExpr;
use datafusion::physical_expr::projection::ProjectionExprs;
use datafusion::physical_expr_adapter::DefaultPhysicalExprAdapterFactory;
use datafusion::physical_plan::metrics::ExecutionPlanMetricsSet;
use parquet::arrow::arrow_reader::{ArrowReaderMetadata, RowSelection};
use parquet::file::metadata::ParquetMetaData;

/// One Parquet file's read plan from `DataFusion`'s shared per-file planner.
///
/// Built once the footer is proved, it holds exactly what `ParquetSource`
/// derives for the same file — the projection mask over the closure's leaves,
/// the decoder row filter, and the row-group and page pruning predicates — so
/// a Struct field or Variant path prunes and projects through the same
/// machinery on every reader. Wyrd owns no statistics pruning of its own.
pub(super) struct FileReadPlan {
    /// The facade's owned plan for this file.
    pub(super) plan: PerFileParquetReadPlan,
    /// The file's own Arrow schema the predicates were adapted to.
    physical_schema: SchemaRef,
    /// The file's cached footer.
    metadata: Arc<ParquetMetaData>,
    /// `DataFusion` per-file counters the row filter and pruning record into.
    file_metrics: ParquetFileMetrics,
}

impl FileReadPlan {
    /// Plans evaluating `projection` over `logical_schema` from the file
    /// `reader` describes, filtered by `filter`. Performs no IO.
    ///
    /// # Errors
    ///
    /// Returns the `DataFusion` error raised when a projection or filter
    /// expression cannot be adapted to or evaluated against the file schema.
    pub(super) fn new(
        reader: &ArrowReaderMetadata,
        logical_schema: &SchemaRef,
        projection: ProjectionExprs,
        filter: Option<Arc<dyn PhysicalExpr>>,
        max_in_list_size: usize,
    ) -> DataFusionResult<Self> {
        let physical_schema = Arc::clone(reader.schema());
        let metadata = Arc::clone(reader.metadata());
        let file_metrics = ParquetFileMetrics::new(0, "hot", &ExecutionPlanMetricsSet::new());
        let plan = PerFileParquetReadPlanner::plan(PerFileParquetReadInput {
            projection,
            filter,
            logical_file_schema: Arc::clone(logical_schema),
            physical_file_schema: Arc::clone(&physical_schema),
            metadata: Arc::clone(&metadata),
            expr_adapter_factory: Arc::new(DefaultPhysicalExprAdapterFactory),
            max_in_list_size,
            file_metrics: file_metrics.clone(),
        })?;
        Ok(Self {
            plan,
            physical_schema,
            metadata,
            file_metrics,
        })
    }

    /// Returns an access plan scanning exactly `row_groups` of this file.
    fn access(&self, row_groups: &[usize]) -> ParquetAccessPlan {
        let mut access = ParquetAccessPlan::new_none(self.metadata.num_row_groups());
        for &row_group in row_groups {
            access.scan(row_group);
        }
        access
    }

    /// Keeps the `candidates` whose footer statistics may satisfy the filter,
    /// in order.
    ///
    /// Without a row-group predicate every candidate is retained; absent or
    /// unusable statistics always keep a group.
    pub(super) fn select_row_groups(&self, candidates: Vec<usize>) -> Vec<usize> {
        let Some(predicate) = self.plan.row_group_predicate.as_deref() else {
            return candidates;
        };
        let mut filter = RowGroupAccessPlanFilter::new(self.access(&candidates));
        filter.prune_by_statistics_with_metadata(
            &self.physical_schema,
            &self.metadata,
            predicate,
            &self.file_metrics,
        );
        filter.row_group_indexes().collect()
    }

    /// Narrows `row_groups` to the pages whose page index may satisfy the
    /// filter.
    ///
    /// Returns the row groups still read, the row selection over exactly those
    /// groups when any page inside them was skipped, and the number of rows of
    /// `row_groups` no longer read.
    ///
    /// # Errors
    ///
    /// Returns the `DataFusion` error raised when the pruned access plan
    /// cannot be turned into a row selection.
    pub(super) fn select_pages(
        &self,
        row_groups: Vec<usize>,
    ) -> DataFusionResult<(Vec<usize>, Option<RowSelection>, usize)> {
        let Some(pages) = self.plan.page_predicate.as_deref() else {
            return Ok((row_groups, None, 0));
        };
        let rows = |groups: &[usize]| {
            groups
                .iter()
                .map(|&group| {
                    usize::try_from(self.metadata.row_group(group).num_rows()).unwrap_or(0)
                })
                .sum::<usize>()
        };
        let access = pages.prune_plan_with_page_index(
            self.access(&row_groups),
            &self.physical_schema,
            self.metadata.file_metadata().schema_descr(),
            &self.metadata,
            &self.file_metrics,
        );
        let kept_groups = access.row_group_indexes();
        let selection = access.into_overall_row_selection(self.metadata.row_groups())?;
        let kept_rows = selection
            .as_ref()
            .map_or_else(|| rows(&kept_groups), RowSelection::row_count);
        let skipped = rows(&row_groups).saturating_sub(kept_rows);
        Ok((kept_groups, selection, skipped))
    }
}

#[cfg(test)]
pub(super) mod tests {
    use arrow::array::{
        Array, ArrayRef, AsArray, Int64Array, ListArray, RecordBatch, StringArray, StructArray,
    };
    use arrow::buffer::OffsetBuffer;
    use arrow::datatypes::{DataType, Field, Int64Type, Schema};
    use bytes::Bytes;
    use datafusion::common::DFSchema;
    use datafusion::config::ConfigOptions;
    use datafusion::logical_expr::{Expr, col, lit};
    use datafusion::physical_expr::projection::ProjectionExpr;
    use datafusion::physical_plan::ExecutionPlan;
    use parquet::arrow::ArrowWriter;
    use parquet::arrow::arrow_reader::ArrowReaderOptions;
    use parquet_variant_compute::{ShreddedSchemaBuilder, json_to_variant, shred_variant};
    use serde_json::{Value, json};
    use wyrd_queue::variant::variant_cell_to_json;
    use wyrd_types::variant::variant_field;

    use super::super::exec::tests::{FIXTURE_TENANT, warehouse_objects};
    use super::super::exec::{HotFileSource, HotObjects, hot_parquet_scan};
    use super::*;

    /// The JSON documents the fixture's Variant column holds, one per row.
    ///
    /// Row 1 adds a residual-only key, row 2 stores `a` with a type its
    /// shredded `Int64` cannot hold, and row 3 is SQL null.
    const DOCUMENTS: [Option<&str>; 4] = [
        Some(r#"{"a":1,"b":"x"}"#),
        Some(r#"{"a":2,"c":true}"#),
        Some(r#"{"a":"str"}"#),
        None,
    ];

    /// The `attributes` of every fixture event, in row order: row 0 holds
    /// two events, row 1 an event with a residual-only key and one with null
    /// attributes; row 2's list is null and row 3's empty.
    const EVENT_ATTRIBUTES: [Option<&str>; 4] = [
        Some(r#"{"k":"v1","x":1}"#),
        Some(r#"{"k":"v2"}"#),
        Some(r#"{"x":2,"r":"only residual"}"#),
        None,
    ];

    /// The table schema every fixture file is read against: an id, a
    /// nullable Struct `s{a, b}`, a canonical unshredded Variant `v`, and
    /// span-shaped `events` whose elements hold a Variant `attributes`.
    pub(in crate::oracle) fn logical_schema() -> SchemaRef {
        Arc::new(Schema::new(vec![
            Field::new("id", DataType::Int64, false),
            Field::new(
                "s",
                DataType::Struct(
                    vec![
                        Field::new("a", DataType::Int64, false),
                        Field::new("b", DataType::Utf8, true),
                    ]
                    .into(),
                ),
                true,
            ),
            variant_field("v", true),
            Field::new(
                "events",
                events_type(variant_field("attributes", true)),
                true,
            ),
        ]))
    }

    /// The span-event shape: `List<Struct{name, attributes}>` whose element
    /// is named `element`, as Iceberg names it, with `attributes` stored as
    /// the given field.
    fn events_type(attributes: Field) -> DataType {
        DataType::List(Arc::new(Field::new(
            "element",
            DataType::Struct(vec![Field::new("name", DataType::Utf8, true), attributes].into()),
            true,
        )))
    }

    /// The Variant of the JSON documents `json`, shredded on the object
    /// keys and types of `shredding` when given, else unshredded.
    ///
    /// # Panics
    ///
    /// Panics when a document is not JSON or Arrow refuses the layout.
    fn variant_of(json: &[Option<&str>], shredding: Option<[(&str, DataType); 2]>) -> ArrayRef {
        let text: ArrayRef = Arc::new(StringArray::from(json.to_vec()));
        let variant = json_to_variant(&text).expect("JSON documents encode");
        let Some(shredding) = shredding else {
            return variant.into();
        };
        let layout = shredding
            .iter()
            .try_fold(ShreddedSchemaBuilder::new(), |layout, (key, data_type)| {
                layout.with_path(*key, data_type)
            })
            .expect("layout")
            .build();
        shred_variant(&variant, &layout).expect("shred").into()
    }

    /// Writes one file holding [`DOCUMENTS`] with `v` shredded on `a: Int64`
    /// and `b: Utf8`, and [`EVENT_ATTRIBUTES`] with `attributes` shredded on
    /// `k: Utf8` and `x: Int64`, when `shredded`, else stored unshredded. Row 2's struct
    /// is null over a placeholder child. The footer proves the fixture tenant,
    /// so a hot leaf can read the file too, and fields carry the Iceberg field
    /// ids `id` 1, `s` 2, `s.a` 3, `s.b` 4, `v` 5, `events` 6, its element
    /// 7, `name` 8, and `attributes` 9, and the Variants the Variant
    /// extension, as published files do.
    ///
    /// # Panics
    ///
    /// Panics when the fixture cannot be shredded or encoded.
    pub(in crate::oracle) fn write_file(shredded: bool) -> Bytes {
        let variant = variant_of(
            &DOCUMENTS,
            shredded.then_some([("a", DataType::Int64), ("b", DataType::Utf8)]),
        );
        let attributes = variant_of(
            &EVENT_ATTRIBUTES,
            shredded.then_some([("k", DataType::Utf8), ("x", DataType::Int64)]),
        );
        let id = |field: Field, id: i32| {
            let mut metadata = field.metadata().clone();
            metadata.insert(
                parquet::arrow::PARQUET_FIELD_ID_META_KEY.to_owned(),
                id.to_string(),
            );
            field.with_metadata(metadata)
        };
        let structs = StructArray::new(
            vec![
                id(Field::new("a", DataType::Int64, false), 3),
                id(Field::new("b", DataType::Utf8, true), 4),
            ]
            .into(),
            vec![
                Arc::new(Int64Array::from(vec![1, 2, 0, 4])) as ArrayRef,
                Arc::new(StringArray::from(vec![
                    Some("x"),
                    Some("y"),
                    None,
                    Some("z"),
                ])),
            ],
            Some(vec![true, true, false, true].into()),
        );
        let element = StructArray::new(
            vec![
                id(Field::new("name", DataType::Utf8, true), 8),
                id(
                    Field::new("attributes", attributes.data_type().clone(), true)
                        .with_metadata(variant_field("attributes", true).metadata().clone()),
                    9,
                ),
            ]
            .into(),
            vec![
                Arc::new(StringArray::from(vec![
                    Some("a"),
                    Some("b"),
                    Some("c"),
                    None,
                ])) as ArrayRef,
                attributes,
            ],
            None,
        );
        let events = ListArray::new(
            Arc::new(id(
                Field::new("element", element.data_type().clone(), true),
                7,
            )),
            OffsetBuffer::from_lengths([2, 2, 0, 0]),
            Arc::new(element),
            Some(vec![true, true, false, true].into()),
        );
        let schema = Arc::new(Schema::new(vec![
            id(Field::new("id", DataType::Int64, false), 1),
            id(Field::new("s", structs.data_type().clone(), true), 2),
            id(
                Field::new("v", variant.data_type().clone(), true)
                    .with_metadata(variant_field("v", true).metadata().clone()),
                5,
            ),
            id(Field::new("events", events.data_type().clone(), true), 6),
        ]));
        let batch = RecordBatch::try_new(
            Arc::clone(&schema),
            vec![
                Arc::new(Int64Array::from(vec![1, 2, 3, 4])),
                Arc::new(structs),
                variant,
                Arc::new(events),
            ],
        )
        .expect("fixture batch");
        let mut sink = Vec::new();
        let mut writer = ArrowWriter::try_new(
            &mut sink,
            schema,
            Some(super::super::exec::tests::fixture_writer_properties(&[])),
        )
        .expect("writer");
        writer.write(&batch).expect("write");
        writer.close().expect("close");
        Bytes::from(sink)
    }

    /// Plans the logical `expr` over [`logical_schema`].
    ///
    /// # Panics
    ///
    /// Panics when the expression does not plan.
    fn physical(expr: &Expr) -> Arc<dyn PhysicalExpr> {
        let df_schema = DFSchema::try_from(logical_schema()).expect("df schema");
        datafusion::physical_expr::create_physical_expr(
            expr,
            &df_schema,
            &datafusion::execution::context::ExecutionProps::new(),
            &datafusion::logical_expr::physical_planning_context::PhysicalPlanningContext::default(
            ),
        )
        .expect("physical expression")
    }

    /// Plans `exprs` as aliased outputs over `published`, filtered by `filter`.
    ///
    /// # Panics
    ///
    /// Panics when the footer does not decode or an expression does not plan.
    fn plan(published: &Bytes, exprs: &[(&str, Expr)], filter: Option<&Expr>) -> FileReadPlan {
        let schema = logical_schema();
        let projection = ProjectionExprs::new(
            exprs
                .iter()
                .map(|(name, expr)| ProjectionExpr::new(physical(expr), *name)),
        );
        let reader =
            ArrowReaderMetadata::load(published, ArrowReaderOptions::new()).expect("valid footer");
        FileReadPlan::new(&reader, &schema, projection, filter.map(physical), 20)
            .expect("per-file plan")
    }

    /// Builds `v ->> key`, the text of a top-level key of the fixture Variant.
    fn text_at(key: &str) -> Expr {
        datafusion::functions_variant::VARIANT_GET_TEXT.call(vec![col("v"), lit(key)])
    }

    /// Returns the dotted paths of the Parquet leaves `plan` decodes.
    pub(in crate::oracle) fn decoded_leaves(published: &Bytes, plan: &FileReadPlan) -> Vec<String> {
        let reader =
            ArrowReaderMetadata::load(published, ArrowReaderOptions::new()).expect("valid footer");
        let descriptor = reader.metadata().file_metadata().schema_descr();
        (0..descriptor.num_columns())
            .filter(|&leaf| plan.plan.projection_mask.leaf_included(leaf))
            .map(|leaf| descriptor.column(leaf).path().string())
            .collect()
    }

    /// Renders every row of the Variant `column` as JSON.
    fn json_rows(column: &dyn Array) -> Vec<Value> {
        (0..column.len())
            .map(|row| variant_cell_to_json(column, row).expect("renders"))
            .collect()
    }

    /// Writes `groups` of JSON documents as one row group each, with `v`
    /// shredded on `a: Int64` and `o.x: Int64` and ids numbered from 1
    /// across groups.
    ///
    /// # Panics
    ///
    /// Panics when the fixture cannot be shredded or encoded.
    fn write_groups(groups: &[&[&str]]) -> Bytes {
        write_groups_with_a(groups, Some(&DataType::Int64))
    }

    /// [`write_groups`] with `a` shredded as `a_type` instead of `Int64`, or
    /// every document stored unshredded when `a_type` is `None`.
    ///
    /// # Panics
    ///
    /// Panics when the fixture cannot be shredded or encoded.
    fn write_groups_with_a(groups: &[&[&str]], a_type: Option<&DataType>) -> Bytes {
        let layout = a_type.map(|a_type| {
            ShreddedSchemaBuilder::new()
                .with_path("a", a_type)
                .expect("layout")
                .with_path("o.x", &DataType::Int64)
                .expect("layout")
                .build()
        });
        let mut id = 0_i64;
        let batches = groups
            .iter()
            .map(|documents| {
                let text: ArrayRef = Arc::new(StringArray::from(documents.to_vec()));
                let variant = json_to_variant(&text).expect("encode");
                let variant: ArrayRef = match &layout {
                    Some(layout) => shred_variant(&variant, layout).expect("shred"),
                    None => variant,
                }
                .into();
                let ids = (id + 1
                    ..=id + i64::try_from(documents.len()).expect("group length fits i64"))
                    .collect::<Vec<_>>();
                id += i64::try_from(documents.len()).expect("group length fits i64");
                let schema = Arc::new(Schema::new(vec![
                    Field::new("id", DataType::Int64, false),
                    Field::new("v", variant.data_type().clone(), true)
                        .with_metadata(variant_field("v", true).metadata().clone()),
                ]));
                RecordBatch::try_new(schema, vec![Arc::new(Int64Array::from(ids)), variant])
                    .expect("group batch")
            })
            .collect::<Vec<_>>();
        let mut sink = Vec::new();
        let mut writer = ArrowWriter::try_new(
            &mut sink,
            batches[0].schema(),
            Some(super::super::exec::tests::fixture_writer_properties(&[])),
        )
        .expect("writer");
        for batch in &batches {
            writer.write(batch).expect("write");
            writer.flush().expect("row group");
        }
        writer.close().expect("close");
        Bytes::from(sink)
    }

    /// The table schema every [`write_groups`] file is read against: an id
    /// and a canonical unshredded Variant `v`.
    fn groups_schema() -> SchemaRef {
        Arc::new(Schema::new(vec![
            Field::new("id", DataType::Int64, false),
            variant_field("v", true),
        ]))
    }

    /// One fixture file served as table `t` through the production hot scan.
    ///
    /// Queries plan with stock `DataFusion` over an empty `t`; the scan is then
    /// swapped for [`hot_parquet_scan`] over the fixture object, as the Oracle
    /// provider's hot leaf is planned.
    pub(in crate::oracle) struct HotTable {
        /// Warehouse root holding the fixture object for the table's lifetime.
        root: tempfile::TempDir,
        /// The fixture object's exact size.
        size: usize,
        /// The table schema the file is read against.
        schema: SchemaRef,
        /// Default-featured session with `t` registered.
        context: datafusion::prelude::SessionContext,
    }

    impl HotTable {
        /// The warehouse key the fixture object is stored at.
        const OBJECT: &str = "t.parquet";

        /// Stores `file` under a fresh warehouse root and registers `t`
        /// with `schema`.
        ///
        /// # Panics
        /// Panics when the fixture cannot be stored or registered.
        pub(in crate::oracle) fn new(file: &Bytes, schema: SchemaRef) -> Self {
            let root = tempfile::tempdir().expect("hot table root");
            std::fs::write(root.path().join(Self::OBJECT), file).expect("hot table object");
            let context = datafusion::prelude::SessionContext::new();
            context
                .register_table(
                    "t",
                    Arc::new(
                        datafusion::datasource::MemTable::try_new(
                            Arc::clone(&schema),
                            vec![Vec::new()],
                        )
                        .expect("table"),
                    ),
                )
                .expect("register");
            Self {
                root,
                size: file.len(),
                schema,
                context,
            }
        }

        /// Builds the production hot scan over the whole table.
        ///
        /// # Panics
        /// Panics when the scan does not plan.
        pub(in crate::oracle) fn scan(&self, options: &ConfigOptions) -> Arc<dyn ExecutionPlan> {
            hot_parquet_scan(
                vec![HotFileSource {
                    location: Self::OBJECT.to_owned(),
                    size_bytes: self.size,
                    row_count: None,
                    event_time: crate::catalog::event_time::EventTimeStatistics::Unusable(
                        crate::catalog::event_time::EventTimeBoundsDefect::Missing,
                    ),
                }],
                HotObjects::Warehouse(warehouse_objects(self.root.path())),
                *FIXTURE_TENANT,
                &self.schema,
                &Arc::new(options.clone()),
            )
            .expect("hot scan plans")
        }

        /// Plans `sql` over `t` with its scan swapped for the hot scan, its
        /// filters pushed into that scan by `DataFusion`'s `FilterPushdown`
        /// and the projections above merged into it by `ProjectionPushdown`,
        /// the order an Oracle session runs them in.
        ///
        /// Each call plans afresh: executing a plan leaves state behind (a
        /// `TopK`'s dynamic filter), so a reused plan would not start from what
        /// the rules see at planning.
        ///
        /// # Panics
        /// Panics when the query does not plan.
        pub(in crate::oracle) async fn plan(&self, sql: &str) -> Arc<dyn ExecutionPlan> {
            self.plan_with(
                sql,
                &crate::resources::OracleSessionShape::for_test().config_options(),
            )
            .await
        }

        /// Plans `sql` as [`Self::plan`] does, under `options`.
        ///
        /// # Panics
        /// Panics when the query does not plan.
        async fn plan_with(&self, sql: &str, options: &ConfigOptions) -> Arc<dyn ExecutionPlan> {
            use datafusion::common::tree_node::{Transformed, TreeNode as _};
            use datafusion::physical_optimizer::PhysicalOptimizerRule as _;

            let swapped = self
                .context
                .sql(sql)
                .await
                .expect(sql)
                .create_physical_plan()
                .await
                .expect(sql)
                .transform_up(|node| {
                    if !node.children().is_empty() {
                        return Ok(Transformed::no(node));
                    }
                    let columns = node
                        .schema()
                        .fields()
                        .iter()
                        .map(|field| {
                            let index = self.schema.index_of(field.name()).expect("scan column");
                            ProjectionExpr::new(
                                Arc::new(datafusion::physical_expr::expressions::Column::new(
                                    field.name(),
                                    index,
                                )),
                                field.name(),
                            )
                        })
                        .collect::<Vec<_>>();
                    Ok(Transformed::yes(Arc::new(
                        datafusion::physical_plan::projection::ProjectionExec::try_new(
                            columns,
                            self.scan(options),
                        )?,
                    )))
                })
                .expect("swap scan")
                .data;
            let filtered = datafusion::physical_optimizer::filter_pushdown::FilterPushdown::new()
                .optimize(swapped, options)
                .expect("filter pushdown");
            datafusion::physical_optimizer::projection_pushdown::ProjectionPushdown::new()
                .optimize(filtered, options)
                .expect("projection pushdown")
        }

        /// Runs `plan` to completion on this table's session.
        ///
        /// # Panics
        /// Panics when execution fails.
        pub(in crate::oracle) async fn run(
            &self,
            plan: Arc<dyn ExecutionPlan>,
        ) -> Vec<RecordBatch> {
            datafusion::physical_plan::collect(plan, self.context.task_ctx())
                .await
                .expect("hot rows")
        }

        /// Plans and runs `sql`, returning its rows as one batch.
        ///
        /// # Panics
        /// Panics when the query does not plan or execute.
        pub(in crate::oracle) async fn sql(&self, sql: &str) -> RecordBatch {
            let plan = self.plan(sql).await;
            let schema = plan.schema();
            arrow::compute::concat_batches(&schema, &self.run(plan).await).expect("concat")
        }

        /// Runs `SELECT id FROM t WHERE {filter}` with the filter pushed into
        /// the hot scan and Parquet statistics pruning set to `pruning`,
        /// returning the `id` column in order.
        ///
        /// # Panics
        /// Panics when the query fails.
        async fn ids(&self, filter: &str, pruning: bool) -> Vec<i64> {
            let mut options = crate::resources::OracleSessionShape::for_test().config_options();
            options.execution.parquet.pruning = pruning;
            let plan = self
                .plan_with(
                    &format!("SELECT id FROM t WHERE {filter} ORDER BY id"),
                    &options,
                )
                .await;
            self.run(plan)
                .await
                .iter()
                .flat_map(|batch| {
                    batch
                        .column_by_name("id")
                        .expect("id")
                        .as_primitive::<Int64Type>()
                        .values()
                        .to_vec()
                })
                .collect()
        }
    }

    /// Returns `batch`'s first column as `Int64` values.
    fn int64s(batch: &RecordBatch) -> Vec<i64> {
        batch
            .column(0)
            .as_primitive::<Int64Type>()
            .values()
            .to_vec()
    }

    /// Returns `batch`'s columns as optional text, one vector per column.
    fn texts(batch: &RecordBatch) -> Vec<Vec<Option<String>>> {
        batch
            .columns()
            .iter()
            .map(|column| {
                column
                    .as_string::<i32>()
                    .iter()
                    .map(|cell| cell.map(ToOwned::to_owned))
                    .collect()
            })
            .collect()
    }

    /// Struct `get_field` and Variant `variant_get` stay distinct logical
    /// expressions yet plan through the one shared per-file plan: on a
    /// shredded file each decodes only its own leaves (`s.a`; `v.metadata` and
    /// `v.typed_value.a`), and the Variant leaf predicate filters while
    /// decoding. Through the hot scan they return exact values, including a
    /// null struct's field and an incompatibly typed residual, the pushed
    /// predicate keeps exactly its row, whole-Variant reads reconstruct every
    /// row exactly, and an unshredded file reads the Variant root whole.
    ///
    /// # Panics
    ///
    /// Panics when any plan decodes other leaves or any value differs.
    #[tokio::test]
    async fn both_readers_use_shared_per_file_plan() {
        let shredded = write_file(true);
        let variant_a = || text_at("a");
        let struct_a = || datafusion::functions::core::expr_fn::get_field(col("s"), "a");
        let leaves = [("s_a", struct_a()), ("v_a", variant_a())];

        let leaf_plan = plan(&shredded, &leaves, None);
        assert_eq!(
            decoded_leaves(&shredded, &leaf_plan),
            [
                "s.a",
                "v.metadata",
                "v.typed_value.a.value",
                "v.typed_value.a.typed_value"
            ]
        );
        assert!(leaf_plan.plan.fallback.is_none());
        let filtered = plan(
            &shredded,
            &[("id", col("id"))],
            Some(&variant_a().eq(lit("2"))),
        );
        assert!(
            filtered.plan.row_filter.is_some(),
            "the Variant leaf filters while decoding"
        );

        let table = HotTable::new(&shredded, logical_schema());
        let batch = table
            .sql("SELECT s['a'] AS s_a, v ->> 'a' AS v_a FROM t ORDER BY id")
            .await;
        assert_eq!(
            batch.column(0).as_primitive::<Int64Type>(),
            &Int64Array::from(vec![Some(1), Some(2), None, Some(4)])
        );
        assert_eq!(
            batch.column(1).as_string::<i32>(),
            &StringArray::from(vec![Some("1"), Some("2"), Some("str"), None])
        );
        assert_eq!(table.ids("v ->> 'a' = '2'", true).await, vec![2]);

        let expected = vec![
            json!({"a": 1, "b": "x"}),
            json!({"a": 2, "c": true}),
            json!({"a": "str"}),
            Value::Null,
        ];
        for published in [&shredded, &write_file(false)] {
            let whole = HotTable::new(published, logical_schema())
                .sql("SELECT v FROM t ORDER BY id")
                .await;
            assert_eq!(json_rows(whole.column(0).as_ref()), expected);
        }

        let unshredded = write_file(false);
        let root_plan = plan(&unshredded, &[("v_a", variant_a())], None);
        assert_eq!(
            decoded_leaves(&unshredded, &root_plan),
            ["v.metadata", "v.value"]
        );
    }

    /// Returns the leaves a projection of `v ->> key` for each of `keys`
    /// decodes from `published`, and each key's text read through the hot
    /// scan.
    ///
    /// # Panics
    ///
    /// Panics when the plan does not build, decode, or evaluate.
    async fn read_keys(
        published: &Bytes,
        keys: &[&str],
    ) -> (Vec<String>, Vec<Vec<Option<String>>>) {
        let exprs = keys
            .iter()
            .map(|key| (*key, text_at(key)))
            .collect::<Vec<_>>();
        let select = keys
            .iter()
            .map(|key| format!("v ->> '{key}'"))
            .collect::<Vec<_>>()
            .join(", ");
        let batch = HotTable::new(published, logical_schema())
            .sql(&format!("SELECT {select} FROM t ORDER BY id"))
            .await;
        (
            decoded_leaves(published, &plan(published, &exprs, None)),
            texts(&batch),
        )
    }

    /// A Variant read for only some keys decodes just their leaves (a
    /// shredded key's subtree, or the root residual for a key the file did
    /// not shred, plus `metadata`) and never a sibling shredded child, yet
    /// every key reads exactly what an all-residual file returns.
    ///
    /// # Panics
    ///
    /// Panics when a read decodes other leaves or any value differs.
    #[tokio::test]
    async fn variant_key_reads_decode_only_their_leaves() {
        let shredded = write_file(true);
        let unshredded = write_file(false);
        for (keys, leaves) in [
            (
                vec!["a"],
                vec![
                    "v.metadata",
                    "v.typed_value.a.value",
                    "v.typed_value.a.typed_value",
                ],
            ),
            (vec!["c"], vec!["v.metadata", "v.value"]),
            (
                vec!["a", "c"],
                vec![
                    "v.metadata",
                    "v.value",
                    "v.typed_value.a.value",
                    "v.typed_value.a.typed_value",
                ],
            ),
        ] {
            let (decoded, texts) = read_keys(&shredded, &keys).await;
            assert_eq!(decoded, leaves, "{keys:?}");
            let (whole, expected) = read_keys(&unshredded, &keys).await;
            assert_eq!(whole, ["v.metadata", "v.value"]);
            assert_eq!(texts, expected, "{keys:?}");
        }
    }

    /// A key the file did not shred reads as null even when every row's
    /// residual `value` is null: Arrow then reports the key missing, and the
    /// leaf must still be a Variant, so a predicate pushed into the hot scan
    /// keeps no row and the projection is all null instead of failing to
    /// decode.
    ///
    /// # Panics
    ///
    /// Panics when the read fails or the absent key reads non-null.
    #[tokio::test]
    async fn absent_variant_key_reads_null_when_every_residual_is_null() {
        let table = HotTable::new(
            &write_groups(&[&[r#"{"a":1}"#, r#"{"a":2}"#]]),
            groups_schema(),
        );
        assert!(table.ids("v ->> 'c' = 'x'", true).await.is_empty());
        assert_eq!(
            texts(&table.sql("SELECT v ->> 'c' FROM t ORDER BY id").await),
            vec![vec![None::<String>, None]]
        );
    }

    /// A nested key reads through whichever leaf holds it: the shredded
    /// `o.x`, the residual of `o` for the unshredded `o.y`, and null for `o.z`
    /// when no row keeps a residual under `o`, so Arrow reports it missing.
    /// The predicate is pushed into the hot scan and keeps exactly its rows.
    ///
    /// # Panics
    ///
    /// Panics when a read fails or a nested filter returns other than its
    /// exact rows.
    #[tokio::test]
    async fn nested_variant_keys_filter_through_shredded_and_residual_leaves() {
        let mixed = HotTable::new(
            &write_groups(&[&[
                r#"{"o":{"x":1,"y":"p"}}"#,
                r#"{"o":{"x":2}}"#,
                r#"{"o":{"x":3,"y":"q"}}"#,
            ]]),
            groups_schema(),
        );
        let typed_only = HotTable::new(
            &write_groups(&[&[r#"{"o":{"x":1}}"#, r#"{"o":{"x":2}}"#]]),
            groups_schema(),
        );
        for (table, key, text, ids) in [
            (&mixed, "x", "2", vec![2_i64]),
            (&mixed, "y", "q", vec![3]),
            (&typed_only, "z", "q", vec![]),
        ] {
            let filter = format!("v -> 'o' ->> '{key}' = '{text}'");
            assert_eq!(table.ids(&filter, true).await, ids, "{filter}");
        }
    }

    /// A key shredded in one file and residual in another reads identically:
    /// for top-level and nested keys, typed and type-mismatched values, and
    /// keys no file shredded, every filter and projection over the shredded
    /// file returns exactly what the all-residual file returns, and an absent
    /// key reads null and matches nothing instead of failing.
    ///
    /// # Panics
    ///
    /// Panics when a read fails or the shredded and residual reads differ.
    #[tokio::test]
    async fn shredded_and_residual_files_read_identically() {
        let documents: &[&str] = &[
            r#"{"a":1,"o":{"x":1,"y":"p"}}"#,
            r#"{"a":2,"o":{"x":2}}"#,
            r#"{"a":"2","o":{"x":"2","y":"q"}}"#,
            r#"{"b":true}"#,
        ];
        let shredded = HotTable::new(&write_groups(&[documents]), groups_schema());
        let residual = HotTable::new(&write_groups_with_a(&[documents], None), groups_schema());
        let projection = "SELECT v ->> 'a', v -> 'o' ->> 'x', v -> 'o' ->> 'y', \
                          v ->> 'zz', v -> 'o' ->> 'z' FROM t ORDER BY id";
        let projected = residual.sql(projection).await;
        assert_eq!(shredded.sql(projection).await, projected);
        assert_eq!(
            projected.column(3).as_string::<i32>(),
            &StringArray::from(vec![None::<&str>; 4])
        );
        for (filter, ids) in [
            ("CAST(v ->> 'a' AS BIGINT) = 2", vec![2_i64, 3]),
            ("v ->> 'a' = '2'", vec![2, 3]),
            ("CAST(v -> 'o' ->> 'x' AS BIGINT) = 2", vec![2, 3]),
            ("v -> 'o' ->> 'y' = 'q'", vec![3]),
            ("v ->> 'zz' = 'x'", vec![]),
            ("CAST(v -> 'o' ->> 'z' AS BIGINT) = 1", vec![]),
        ] {
            for table in [&shredded, &residual] {
                let batch = table
                    .sql(&format!("SELECT id FROM t WHERE {filter} ORDER BY id"))
                    .await;
                assert_eq!(int64s(&batch), ids, "{filter}");
            }
        }
    }

    /// Typed Variant statistics prune a row group only when its residual is
    /// all null and the comparison fits the shredded type: the group holding
    /// a residual `"10"` is always read and its row still matches, an
    /// all-typed group outside the literal is skipped whether `a` is shredded
    /// as `Int64` or narrowed to `Int16` or `Int8`, a `Utf8` comparison
    /// against an integer shredding prunes nothing, a `BIGINT` literal
    /// outside an `Int8` leaf's range prunes every all-typed group and matches
    /// nothing, and
    /// the hot scan returns identical rows with statistics pruning on and off.
    ///
    /// # Panics
    ///
    /// Panics when a group is pruned or kept against the rule above, or when
    /// pruned and unpruned reads differ.
    #[tokio::test]
    async fn typed_variant_statistics_prune_only_with_all_null_residuals() {
        for a_type in [DataType::Int64, DataType::Int16, DataType::Int8] {
            typed_statistics_prune_at(&a_type).await;
        }
    }

    /// Runs the typed-statistics cases of
    /// [`typed_variant_statistics_prune_only_with_all_null_residuals`] with
    /// `a` shredded as `a_type`.
    ///
    /// # Panics
    ///
    /// Panics when a group is pruned or kept against that test's rule, or
    /// when pruned and unpruned reads differ.
    async fn typed_statistics_prune_at(a_type: &DataType) {
        let published = write_groups_with_a(
            &[
                &[r#"{"a":1}"#, r#"{"a":2}"#],
                &[r#"{"a":10}"#, r#"{"a":"10"}"#],
                &[r#"{"a":20}"#, r#"{"a":21}"#],
            ],
            Some(a_type),
        );
        let table = HotTable::new(&published, groups_schema());
        let text = || text_at("a");
        let number = || {
            Expr::Cast(datafusion::logical_expr::expr::Cast::new(
                Box::new(text()),
                DataType::Int64,
            ))
        };
        for (filter, sql, retained, ids) in [
            (
                number().eq(lit(2_i64)),
                "CAST(v ->> 'a' AS BIGINT) = 2",
                vec![0, 1],
                vec![2],
            ),
            (
                number().eq(lit(10_i64)),
                "CAST(v ->> 'a' AS BIGINT) = 10",
                vec![1],
                vec![3, 4],
            ),
            (
                text().eq(lit("2")),
                "v ->> 'a' = '2'",
                vec![0, 1, 2],
                vec![2],
            ),
            (
                number().eq(lit(1_000_i64)),
                "CAST(v ->> 'a' AS BIGINT) = 1000",
                vec![1],
                vec![],
            ),
        ] {
            assert_eq!(
                plan(&published, &[("id", col("id"))], Some(&filter))
                    .select_row_groups(vec![0, 1, 2]),
                retained,
                "{filter} at {a_type}"
            );
            for pruning in [true, false] {
                assert_eq!(
                    table.ids(sql, pruning).await,
                    ids,
                    "{filter} at {a_type} pruning={pruning}"
                );
            }
        }
    }
}
