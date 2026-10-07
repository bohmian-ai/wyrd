//! The per-file Parquet read plan every Oracle reader applies.
//!
//! Hot sealed objects and published Iceberg files both decode Parquet one file
//! at a time after footer discovery. [`FileReadPlan`] asks `DataFusion`'s
//! shared [`PerFileParquetReadPlanner`] for that file's projection mask,
//! decoder row filter, and row-group and page pruning predicates, through the
//! Oracle Variant file adapter, so a Struct field (`get_field`) and a Variant
//! path (`variant_get`) select the same physical leaves on every reader while
//! keeping their distinct logical semantics. Hot objects apply the plan
//! directly; published files reach it through [`PublishedFileReadPlanner`],
//! which the Iceberg reader calls per file inside its own task semantics.

use std::sync::Arc;

use arrow::datatypes::SchemaRef;
use datafusion::datasource::physical_plan::parquet::{
    ParquetAccessPlan, ParquetFileMetrics, PerFileParquetReadInput, PerFileParquetReadPlan,
    PerFileParquetReadPlanner, RowGroupAccessPlanFilter,
};
use datafusion::error::Result as DataFusionResult;
use datafusion::physical_expr::PhysicalExpr;
use datafusion::physical_expr::projection::{ProjectionExprs, Projector};
use datafusion::physical_plan::metrics::ExecutionPlanMetricsSet;
use parquet::arrow::arrow_reader::{ArrowReaderBuilder, ArrowReaderMetadata, RowSelection};
use parquet::file::metadata::ParquetMetaData;

use super::exec::RowGroupSelection;
use super::leaf_paths::LeafPaths;
use super::variant_sql::OracleVariantSql;

/// One Parquet file's read plan from `DataFusion`'s shared per-file planner.
///
/// Built once the footer is proved, it holds exactly what `ParquetSource`
/// derives for the same file — the projection mask over the closure's leaves,
/// the decoder row filter, and the row-group and page pruning predicates — so
/// a Struct field or Variant path prunes and projects through the same
/// machinery on every reader. Wyrd keeps only its scan metrics and Bloom
/// probes; it owns no statistics pruning of its own.
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
            expr_adapter_factory: OracleVariantSql::shared().file_adapter(),
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

    /// Keeps the `candidates` whose footer statistics may satisfy the filter.
    ///
    /// Without a row-group predicate every candidate is retained; absent or
    /// unusable statistics always keep a group.
    pub(super) fn select_row_groups(&self, candidates: Vec<usize>) -> RowGroupSelection {
        let Some(predicate) = self.plan.row_group_predicate.as_deref() else {
            return RowGroupSelection {
                retained: candidates,
                pruned: 0,
            };
        };
        let mut filter = RowGroupAccessPlanFilter::new(self.access(&candidates));
        filter.prune_by_statistics_with_metadata(
            &self.physical_schema,
            &self.metadata,
            predicate,
            &self.file_metrics,
        );
        let retained = filter.row_group_indexes().collect::<Vec<_>>();
        RowGroupSelection {
            pruned: (candidates.len() - retained.len()) as u64,
            retained,
        }
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

    /// Applies this plan's decoder half to `builder`: the `row_groups` and
    /// optional page `selection` chosen by pruning, the projection mask, and
    /// the decoder row filter.
    ///
    /// Returns the configured builder and the projector that turns each
    /// decoded batch into the plan's output expressions.
    ///
    /// # Errors
    ///
    /// Returns the `DataFusion` error raised when the projection cannot be
    /// bound to the decoded schema.
    pub(super) fn apply<T>(
        self,
        builder: ArrowReaderBuilder<T>,
        row_groups: Vec<usize>,
        selection: Option<RowSelection>,
    ) -> DataFusionResult<(ArrowReaderBuilder<T>, Projector)> {
        let plan = self.plan;
        let projector = plan.projection.make_projector(&plan.projected_schema)?;
        let mut builder = builder
            .with_row_groups(row_groups)
            .with_projection(plan.projection_mask);
        if let Some(selection) = selection {
            builder = builder.with_row_selection(selection);
        }
        if let Some(row_filter) = plan.row_filter {
            builder = builder.with_row_filter(row_filter);
        }
        Ok((builder, projector))
    }
}

/// The published source adapter's entry into the shared read core.
///
/// The Iceberg reader opens each planned data file and applies its own byte
/// range and Iceberg-predicate pruning, then calls this planner with the
/// file's footer-derived metadata. The planner builds the same
/// [`FileReadPlan`] a hot object gets and hands back its row-group and page
/// pruning and its decoder row filter, so a Struct field or shredded Variant
/// path prunes and filters published files exactly as it does hot ones.
/// Iceberg keeps projection by field id, deletes, schema transformation,
/// partition constants, and row lineage, and counts this planner's pruning in
/// its own scan metrics.
pub(super) struct PublishedFileReadPlanner {
    /// The scan closure the filter was compiled against.
    schema: SchemaRef,
    /// Conjunction of the scan's closed predicates over `schema`, if any.
    filter: Option<Arc<dyn PhysicalExpr>>,
    /// Literal paths read from each trimmed Struct or Variant column.
    leaf_paths: LeafPaths,
    /// The session's bound on `IN`-list literals used for pruning.
    max_in_list_size: usize,
}

impl PublishedFileReadPlanner {
    /// Plans every published file of one scan against `filter` over `schema`,
    /// trimming the columns `leaf_paths` names.
    pub(super) fn new(
        schema: SchemaRef,
        filter: Option<Arc<dyn PhysicalExpr>>,
        leaf_paths: LeafPaths,
        max_in_list_size: usize,
    ) -> Self {
        Self {
            schema,
            filter,
            leaf_paths,
            max_in_list_size,
        }
    }

    /// Builds one file's [`FileReadPlan`], applies its statistics and page
    /// pruning to `row_groups`, and returns the leaf mask of its trimmed
    /// columns.
    ///
    /// # Errors
    ///
    /// Returns the `DataFusion` error raised when the filter cannot be adapted
    /// to the file schema or the pruned access plan cannot become a row
    /// selection.
    fn narrow(
        &self,
        metadata: &ArrowReaderMetadata,
        row_groups: Vec<usize>,
    ) -> DataFusionResult<iceberg::arrow::ParquetFileReadNarrowing> {
        let mut read_plan = FileReadPlan::new(
            metadata,
            &self.schema,
            self.leaf_paths.projection(&self.schema),
            self.filter.clone(),
            self.max_in_list_size,
        )?;
        let selection = read_plan.select_row_groups(row_groups);
        let (row_groups, row_selection, _) = read_plan.select_pages(selection.retained)?;
        let projection = (!self.leaf_paths.is_empty()).then(|| {
            self.leaf_paths
                .leaf_mask(&read_plan.plan.projection_mask, metadata.parquet_schema())
        });
        Ok(iceberg::arrow::ParquetFileReadNarrowing {
            row_groups,
            row_selection,
            row_filter: read_plan.plan.row_filter.take(),
            projection,
        })
    }
}

impl iceberg::arrow::ParquetFileReadPlanner for PublishedFileReadPlanner {
    /// Narrows one published data file through the shared read core.
    ///
    /// # Errors
    ///
    /// Returns an Iceberg error carrying the `DataFusion` cause when the file
    /// cannot be planned; the reader then fails the task.
    fn plan(
        &self,
        metadata: &ArrowReaderMetadata,
        row_groups: Vec<usize>,
    ) -> iceberg::Result<iceberg::arrow::ParquetFileReadNarrowing> {
        self.narrow(metadata, row_groups).map_err(|error| {
            iceberg::Error::new(
                iceberg::ErrorKind::Unexpected,
                "published per-file read plan failed",
            )
            .with_source(error)
        })
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
    use datafusion::logical_expr::{Expr, col, lit};
    use datafusion::physical_expr::projection::ProjectionExpr;
    use parquet::arrow::ArrowWriter;
    use parquet::arrow::arrow_reader::{ArrowReaderOptions, ParquetRecordBatchReaderBuilder};
    use parquet_variant_compute::{ShreddedSchemaBuilder, json_to_variant, shred_variant};
    use serde_json::{Value, json};
    use wyrd_queue::variant::variant_cell_to_json;
    use wyrd_types::variant::variant_field;

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

    /// Decodes `published` under `plan` and evaluates its projection.
    ///
    /// # Panics
    ///
    /// Panics when the file does not decode under the plan.
    pub(in crate::oracle) fn read(published: &Bytes, plan: FileReadPlan) -> RecordBatch {
        let builder = ParquetRecordBatchReaderBuilder::try_new(published.clone()).expect("reader");
        let groups = (0..builder.metadata().num_row_groups()).collect();
        let (builder, projector) = plan.apply(builder, groups, None).expect("applied plan");
        let batches = builder
            .build()
            .expect("decoder")
            .map(|batch| {
                projector
                    .project_batch(&batch.expect("batch"))
                    .expect("projected")
            })
            .collect::<Vec<_>>();
        arrow::compute::concat_batches(projector.output_schema(), &batches).expect("concat")
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

    /// Struct `get_field` and Variant `variant_get` stay distinct logical
    /// expressions yet plan through the one shared per-file plan: on a
    /// shredded file each decodes only its own leaves (`s.a`; `v.metadata` and
    /// `v.typed_value.a`), returns exact values including a null struct's
    /// field and an incompatibly typed residual, and pushes the Variant leaf
    /// predicate into the decoder. Whole-Variant reads reconstruct every row
    /// exactly, and an unshredded file reads the Variant root whole.
    ///
    /// # Panics
    ///
    /// Panics when any plan decodes other leaves or any value differs.
    #[test]
    fn both_readers_use_shared_per_file_plan() {
        let shredded = write_file(true);
        let variant_a = || OracleVariantSql::shared().text_at(col("v"), &["a".to_owned()]);
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
        let batch = read(&shredded, leaf_plan);
        assert_eq!(
            batch.column(0).as_primitive::<Int64Type>(),
            &Int64Array::from(vec![Some(1), Some(2), None, Some(4)])
        );
        assert_eq!(
            batch.column(1).as_string::<i32>(),
            &StringArray::from(vec![Some("1"), Some("2"), Some("str"), None])
        );

        let filtered = plan(
            &shredded,
            &[("id", col("id"))],
            Some(&variant_a().eq(lit("2"))),
        );
        assert!(
            filtered.plan.row_filter.is_some(),
            "the Variant leaf filters while decoding"
        );
        assert_eq!(
            read(&shredded, filtered)
                .column(0)
                .as_primitive::<Int64Type>(),
            &Int64Array::from(vec![2])
        );

        let expected = vec![
            json!({"a": 1, "b": "x"}),
            json!({"a": 2, "c": true}),
            json!({"a": "str"}),
            Value::Null,
        ];
        for published in [&shredded, &write_file(false)] {
            let whole = read(published, plan(published, &[("v", col("v"))], None));
            assert_eq!(json_rows(whole.column(0).as_ref()), expected);
        }

        let unshredded = write_file(false);
        let root_plan = plan(&unshredded, &[("v_a", variant_a())], None);
        assert_eq!(
            decoded_leaves(&unshredded, &root_plan),
            ["v.metadata", "v.value"]
        );
    }

    /// Reads `published` with `v` projected through
    /// [`OracleVariantSql::trimmed`] for `paths`, returning the decoded leaves
    /// and each path's `->>` text.
    ///
    /// # Panics
    ///
    /// Panics when the plan does not build, decode, or evaluate.
    fn read_trimmed(
        published: &Bytes,
        paths: &[Vec<String>],
    ) -> (Vec<String>, Vec<Vec<Option<String>>>) {
        let schema = logical_schema();
        let column = Arc::new(datafusion::physical_expr::expressions::Column::new("v", 2));
        let projection = ProjectionExprs::new([ProjectionExpr::new(
            OracleVariantSql::trimmed(
                column,
                &Arc::new(schema.field_with_name("v").expect("v").clone()),
                paths.iter().cloned().collect(),
            ),
            "v",
        )]);
        let reader =
            ArrowReaderMetadata::load(published, ArrowReaderOptions::new()).expect("valid footer");
        let read_plan =
            FileReadPlan::new(&reader, &schema, projection, None, 20).expect("per-file plan");
        let leaves = decoded_leaves(published, &read_plan);
        (leaves, path_texts(&read(published, read_plan), paths))
    }

    /// Evaluates `v ->> path` for each of `paths` over `batch`'s `v` column.
    ///
    /// # Panics
    ///
    /// Panics when an expression does not plan or evaluate.
    fn path_texts(batch: &RecordBatch, paths: &[Vec<String>]) -> Vec<Vec<Option<String>>> {
        let df_schema = DFSchema::try_from(batch.schema()).expect("df schema");
        paths
            .iter()
            .map(|keys| {
                let expr = datafusion::physical_expr::create_physical_expr(
                    &OracleVariantSql::shared().text_at(col("v"), keys),
                    &df_schema,
                    &datafusion::execution::context::ExecutionProps::new(),
                    &datafusion::logical_expr::physical_planning_context::PhysicalPlanningContext::default(),
                )
                .expect("physical path");
                let text = expr
                    .evaluate(batch)
                    .and_then(|value| value.into_array(batch.num_rows()))
                    .expect("evaluated path");
                text.as_string::<i32>()
                    .iter()
                    .map(|cell| cell.map(ToOwned::to_owned))
                    .collect()
            })
            .collect()
    }

    /// A Variant read for only some paths decodes just their leaves (a
    /// shredded key's subtree, or the root residual for a key the file did
    /// not shred, plus `metadata`) and never a sibling shredded child, yet
    /// every path reads exactly what an all-residual file returns.
    ///
    /// # Panics
    ///
    /// Panics when a read decodes other leaves or any value differs.
    #[test]
    fn trimmed_variant_reads_only_its_paths_leaves() {
        let shredded = write_file(true);
        let unshredded = write_file(false);
        let path = |key: &str| vec![key.to_owned()];
        for (paths, leaves) in [
            (
                vec![path("a")],
                vec![
                    "v.metadata",
                    "v.typed_value.a.value",
                    "v.typed_value.a.typed_value",
                ],
            ),
            (vec![path("c")], vec!["v.metadata", "v.value"]),
            (
                vec![path("a"), path("c")],
                vec![
                    "v.metadata",
                    "v.value",
                    "v.typed_value.a.value",
                    "v.typed_value.a.typed_value",
                ],
            ),
        ] {
            let (decoded, texts) = read_trimmed(&shredded, &paths);
            assert_eq!(decoded, leaves, "{paths:?}");
            let (whole, expected) = read_trimmed(&unshredded, &paths);
            assert_eq!(whole, ["v.metadata", "v.value"]);
            assert_eq!(texts, expected, "{paths:?}");
        }
    }

    /// A key the file did not shred reads as null even when every row's
    /// residual `value` is null: Arrow then reports the key missing, and the
    /// leaf must still be a Variant, so the filter keeps no row and the
    /// projection is all null instead of failing to decode.
    ///
    /// # Panics
    ///
    /// Panics when the plan fails to decode or the absent key reads non-null.
    #[test]
    fn absent_variant_key_reads_null_when_every_residual_is_null() {
        let published = write_groups(&[&[r#"{"a":1}"#, r#"{"a":2}"#]]);
        let absent = || OracleVariantSql::shared().text_at(col("v"), &["c".to_owned()]);

        let filtered = plan(
            &published,
            &[("id", col("id"))],
            Some(&absent().eq(lit("x"))),
        );
        assert_eq!(read(&published, filtered).num_rows(), 0);

        let projected = read(&published, plan(&published, &[("v_c", absent())], None));
        assert_eq!(
            projected.column(0).as_string::<i32>(),
            &StringArray::from(vec![None::<&str>, None])
        );
    }

    /// A nested key reads through whichever leaf holds it: the shredded
    /// `o.x`, the residual of `o` for the unshredded `o.y`, and null for `o.z`
    /// when no row keeps a residual under `o`, so Arrow reports it missing.
    ///
    /// # Panics
    ///
    /// Panics when a plan fails to decode or a nested filter returns other
    /// than its exact rows.
    #[test]
    fn nested_variant_keys_filter_through_shredded_and_residual_leaves() {
        let nested = |key: &str| {
            OracleVariantSql::shared().text_at(col("v"), &["o".to_owned(), key.to_owned()])
        };
        let mixed = write_groups(&[&[
            r#"{"o":{"x":1,"y":"p"}}"#,
            r#"{"o":{"x":2}}"#,
            r#"{"o":{"x":3,"y":"q"}}"#,
        ]]);
        let typed_only = write_groups(&[&[r#"{"o":{"x":1}}"#, r#"{"o":{"x":2}}"#]]);
        for (published, key, text, ids) in [
            (&mixed, "x", "2", vec![2_i64]),
            (&mixed, "y", "q", vec![3]),
            (&typed_only, "z", "q", vec![]),
        ] {
            let filter = nested(key).eq(lit(text));
            let batch = read(
                published,
                plan(published, &[("id", col("id"))], Some(&filter)),
            );
            assert_eq!(
                batch.column(0).as_primitive::<Int64Type>(),
                &Int64Array::from(ids),
                "{filter}"
            );
        }
    }

    /// A key shredded in one file and residual in another reads identically:
    /// for top-level and nested keys, typed and type-mismatched values, and
    /// keys no file shredded, every filter and projection over the shredded
    /// file returns exactly what the all-residual file returns, and an absent
    /// key reads null and matches nothing instead of failing. The shredded
    /// file filters while decoding; the all-residual file decodes every row,
    /// and the filter above the scan keeps the same ones.
    ///
    /// # Panics
    ///
    /// Panics when a plan fails or the shredded and residual reads differ.
    #[test]
    fn shredded_and_residual_files_read_identically() {
        let documents: &[&str] = &[
            r#"{"a":1,"o":{"x":1,"y":"p"}}"#,
            r#"{"a":2,"o":{"x":2}}"#,
            r#"{"a":"2","o":{"x":"2","y":"q"}}"#,
            r#"{"b":true}"#,
        ];
        let shredded = write_groups(&[documents]);
        let residual = write_groups_with_a(&[documents], None);
        let at = |keys: &[&str]| {
            OracleVariantSql::shared().text_at(
                col("v"),
                &keys.iter().map(|key| (*key).to_owned()).collect::<Vec<_>>(),
            )
        };
        let number = |keys: &[&str]| {
            Expr::Cast(datafusion::logical_expr::expr::Cast::new(
                Box::new(at(keys)),
                DataType::Int64,
            ))
        };
        let projection = [
            ("a", at(&["a"])),
            ("o_x", at(&["o", "x"])),
            ("o_y", at(&["o", "y"])),
            ("zz", at(&["zz"])),
            ("o_z", at(&["o", "z"])),
        ];
        let projected = read(&residual, plan(&residual, &projection, None));
        assert_eq!(
            read(&shredded, plan(&shredded, &projection, None)),
            projected
        );
        assert_eq!(
            projected.column(3).as_string::<i32>(),
            &StringArray::from(vec![None::<&str>; 4])
        );
        for (filter, ids) in [
            (number(&["a"]).eq(lit(2_i64)), vec![2_i64, 3]),
            (at(&["a"]).eq(lit("2")), vec![2, 3]),
            (number(&["o", "x"]).eq(lit(2_i64)), vec![2, 3]),
            (at(&["o", "y"]).eq(lit("q")), vec![3]),
            (at(&["zz"]).eq(lit("x")), vec![]),
            (number(&["o", "z"]).eq(lit(1_i64)), vec![]),
        ] {
            for published in [&shredded, &residual] {
                // The decoder filter is best effort, so the filter is also
                // evaluated over the decoded rows, as the `FilterExec` above
                // every Oracle scan does.
                let batch = read(
                    published,
                    plan(
                        published,
                        &[("id", col("id")), ("keep", filter.clone())],
                        Some(&filter),
                    ),
                );
                let kept = arrow::compute::filter(batch.column(0), batch.column(1).as_boolean())
                    .expect("filtered ids");
                assert_eq!(
                    kept.as_primitive::<Int64Type>(),
                    &Int64Array::from(ids.clone()),
                    "{filter}"
                );
            }
        }
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
                    Field::new("v", variant.data_type().clone(), true),
                ]));
                RecordBatch::try_new(schema, vec![Arc::new(Int64Array::from(ids)), variant])
                    .expect("group batch")
            })
            .collect::<Vec<_>>();
        let mut sink = Vec::new();
        let mut writer =
            ArrowWriter::try_new(&mut sink, batches[0].schema(), None).expect("writer");
        for batch in &batches {
            writer.write(batch).expect("write");
            writer.flush().expect("row group");
        }
        writer.close().expect("close");
        Bytes::from(sink)
    }

    /// Typed Variant statistics prune a row group only when its residual is
    /// all null and the comparison fits the shredded type: the group holding
    /// a residual `"10"` is always read and its row still matches, an
    /// all-typed group outside the literal is skipped whether `a` is shredded
    /// as `Int64` or narrowed to `Int16` or `Int8`, a `Utf8` comparison
    /// against an integer shredding prunes nothing, a `BIGINT` literal
    /// outside an `Int8` leaf's range prunes nothing and matches nothing, and
    /// pruned and unpruned reads return identical rows.
    ///
    /// # Panics
    ///
    /// Panics when a group is pruned or kept against the rule above, or when
    /// pruned and unpruned reads differ.
    #[test]
    fn typed_variant_statistics_prune_only_with_all_null_residuals() {
        for a_type in [DataType::Int64, DataType::Int16, DataType::Int8] {
            typed_statistics_prune_at(&a_type);
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
    fn typed_statistics_prune_at(a_type: &DataType) {
        let published = write_groups_with_a(
            &[
                &[r#"{"a":1}"#, r#"{"a":2}"#],
                &[r#"{"a":10}"#, r#"{"a":"10"}"#],
                &[r#"{"a":20}"#, r#"{"a":21}"#],
            ],
            Some(a_type),
        );
        let text = || OracleVariantSql::shared().text_at(col("v"), &["a".to_owned()]);
        let number = || {
            Expr::Cast(datafusion::logical_expr::expr::Cast::new(
                Box::new(text()),
                DataType::Int64,
            ))
        };
        for (filter, retained, ids) in [
            (number().eq(lit(2_i64)), vec![0, 1], vec![2]),
            (number().eq(lit(10_i64)), vec![1], vec![3, 4]),
            (text().eq(lit("2")), vec![0, 1, 2], vec![2]),
            (
                number().eq(lit(1_000_i64)),
                if a_type == &DataType::Int8 {
                    vec![0, 1, 2]
                } else {
                    vec![1]
                },
                vec![],
            ),
        ] {
            let read_plan = || plan(&published, &[("id", col("id"))], Some(&filter));
            assert_eq!(
                read_plan().select_row_groups(vec![0, 1, 2]).retained,
                retained,
                "{filter} at {a_type}"
            );
            for groups in [retained.clone(), vec![0, 1, 2]] {
                let builder =
                    ParquetRecordBatchReaderBuilder::try_new(published.clone()).expect("reader");
                let (builder, projector) = read_plan()
                    .apply(builder, groups, None)
                    .expect("applied plan");
                let read = builder
                    .build()
                    .expect("decoder")
                    .flat_map(|batch| {
                        projector
                            .project_batch(&batch.expect("batch"))
                            .expect("projected")
                            .column(0)
                            .as_primitive::<Int64Type>()
                            .values()
                            .to_vec()
                    })
                    .collect::<Vec<_>>();
                assert_eq!(read, ids, "{filter} at {a_type}");
            }
        }
    }

    /// A published file reaches the same per-file plan through the Iceberg
    /// reader: inside Iceberg's own task handling, the shredded leaf's
    /// statistics skip the all-typed group outside the literal, the decoder
    /// filter keeps only the matching row, and Iceberg counts the skipped
    /// group in its scan metrics.
    ///
    /// # Panics
    ///
    /// Panics when the reader fails, returns other rows, or reports other
    /// pruning.
    #[tokio::test]
    async fn published_files_prune_and_filter_through_the_shared_plan() {
        use futures_util::TryStreamExt as _;
        use iceberg::spec::{NestedField, PrimitiveType, Type};

        let directory = tempfile::tempdir().expect("fixture directory");
        let path = directory.path().join("published.parquet");
        std::fs::write(
            &path,
            write_groups(&[
                &[r#"{"a":1}"#, r#"{"a":2}"#],
                &[r#"{"a":10}"#, r#"{"a":"10"}"#],
                &[r#"{"a":20}"#, r#"{"a":21}"#],
            ]),
        )
        .expect("fixture file");
        let task = iceberg::scan::FileScanTask::builder()
            .with_file_size_in_bytes(std::fs::metadata(&path).expect("fixture size").len())
            .with_start(0)
            .with_length(0)
            .with_data_file_path(path.to_string_lossy().into_owned())
            .with_data_file_format(iceberg::spec::DataFileFormat::Parquet)
            .with_schema(Arc::new(
                iceberg::spec::Schema::builder()
                    .with_fields(vec![Arc::new(NestedField::required(
                        1,
                        "id",
                        Type::Primitive(PrimitiveType::Long),
                    ))])
                    .build()
                    .expect("task schema"),
            ))
            .with_project_field_ids(vec![1])
            .with_case_sensitive(false)
            .build();
        let number = Expr::Cast(datafusion::logical_expr::expr::Cast::new(
            Box::new(OracleVariantSql::shared().text_at(col("v"), &["a".to_owned()])),
            DataType::Int64,
        ));
        let planner = PublishedFileReadPlanner::new(
            logical_schema(),
            Some(physical(&number.eq(lit(2_i64)))),
            LeafPaths::default(),
            20,
        );
        let reader = iceberg::arrow::ArrowReaderBuilder::new(
            iceberg::io::FileIO::new_with_fs(),
            iceberg::Runtime::current(),
        )
        .with_parquet_file_read_planner(Arc::new(planner))
        .build();
        let result = reader
            .read(Box::pin(futures_util::stream::iter(vec![Ok(task)])))
            .expect("published reader");
        let metrics = result.metrics().clone();
        let ids = result
            .stream()
            .try_collect::<Vec<_>>()
            .await
            .expect("published rows")
            .iter()
            .flat_map(|batch| {
                batch
                    .column(0)
                    .as_primitive::<Int64Type>()
                    .values()
                    .to_vec()
            })
            .collect::<Vec<_>>();
        assert_eq!(ids, vec![2]);
        assert_eq!(metrics.row_groups_considered(), 3);
        assert_eq!(metrics.row_groups_pruned_by_statistics(), 1);
    }
}
