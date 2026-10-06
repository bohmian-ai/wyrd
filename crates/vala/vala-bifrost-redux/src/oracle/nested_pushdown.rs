//! The per-file Parquet read plan every Oracle reader applies.
//!
//! Hot sealed objects and published Iceberg files both decode Parquet one file
//! at a time after footer discovery. [`FileReadPlan`] asks `DataFusion`'s
//! shared [`PerFileParquetReadPlanner`] for that file's projection mask,
//! decoder row filter, and row-group and page pruning predicates, through the
//! Oracle Variant file adapter, so a Struct field (`get_field`) and a Variant
//! path (`variant_get`) select the same physical leaves on every reader while
//! keeping their distinct logical semantics.

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

use super::exec::{OracleScanMetricsHandle, RowGroupSelection};
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
    /// filter, recording the rows skipped in `metrics`.
    ///
    /// Returns the row groups still read and, when any page inside them was
    /// skipped, the row selection over exactly those groups.
    ///
    /// # Errors
    ///
    /// Returns the `DataFusion` error raised when the pruned access plan
    /// cannot be turned into a row selection.
    pub(super) fn select_pages(
        &self,
        row_groups: Vec<usize>,
        metrics: &OracleScanMetricsHandle,
    ) -> DataFusionResult<(Vec<usize>, Option<RowSelection>)> {
        let Some(pages) = self.plan.page_predicate.as_deref() else {
            return Ok((row_groups, None));
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
        metrics.record_page_pruned_rows(rows(&row_groups).saturating_sub(kept_rows));
        Ok((kept_groups, selection))
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

#[cfg(test)]
mod tests {
    use arrow::array::{
        Array, ArrayRef, AsArray, Int64Array, RecordBatch, StringArray, StructArray,
    };
    use arrow::datatypes::{DataType, Field, Int64Type, Schema};
    use bytes::Bytes;
    use datafusion::common::DFSchema;
    use datafusion::logical_expr::{Expr, col, lit};
    use datafusion::physical_expr::projection::ProjectionExpr;
    use parquet::arrow::ArrowWriter;
    use parquet::arrow::arrow_reader::{ArrowReaderOptions, ParquetRecordBatchReaderBuilder};
    use parquet_variant_compute::{ShreddedSchemaBuilder, json_to_variant, shred_variant};
    use serde_json::{Value, json};
    use wyrd_queue::variant::{variant_cell_to_json, variant_field};

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

    /// The table schema every fixture file is read against: an id, a
    /// nullable Struct `s{a, b}`, and a canonical unshredded Variant `v`.
    fn logical_schema() -> SchemaRef {
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
        ]))
    }

    /// Writes one file holding [`DOCUMENTS`] with `v` shredded on `a: Int64`
    /// and `b: Utf8` when `shredded`, else stored unshredded. Row 2's struct
    /// is null over a placeholder child.
    ///
    /// # Panics
    ///
    /// Panics when the fixture cannot be shredded or encoded.
    fn write_file(shredded: bool) -> Bytes {
        let text: ArrayRef = Arc::new(StringArray::from(DOCUMENTS.to_vec()));
        let variant = json_to_variant(&text).expect("JSON documents encode");
        let variant = if shredded {
            let layout = ShreddedSchemaBuilder::new()
                .with_path("a", &DataType::Int64)
                .and_then(|layout| layout.with_path("b", &DataType::Utf8))
                .expect("layout")
                .build();
            shred_variant(&variant, &layout).expect("shred")
        } else {
            variant
        };
        let variant: ArrayRef = variant.into();
        let structs = StructArray::new(
            vec![
                Field::new("a", DataType::Int64, false),
                Field::new("b", DataType::Utf8, true),
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
        let schema = Arc::new(Schema::new(vec![
            Field::new("id", DataType::Int64, false),
            Field::new("s", structs.data_type().clone(), true),
            Field::new("v", variant.data_type().clone(), true),
        ]));
        let batch = RecordBatch::try_new(
            Arc::clone(&schema),
            vec![
                Arc::new(Int64Array::from(vec![1, 2, 3, 4])),
                Arc::new(structs),
                variant,
            ],
        )
        .expect("fixture batch");
        let mut sink = Vec::new();
        let mut writer = ArrowWriter::try_new(&mut sink, schema, None).expect("writer");
        writer.write(&batch).expect("write");
        writer.close().expect("close");
        Bytes::from(sink)
    }

    /// Plans `exprs` as aliased outputs over `published`, filtered by `filter`.
    ///
    /// # Panics
    ///
    /// Panics when the footer does not decode or an expression does not plan.
    fn plan(published: &Bytes, exprs: &[(&str, Expr)], filter: Option<&Expr>) -> FileReadPlan {
        let schema = logical_schema();
        let df_schema = DFSchema::try_from(Arc::clone(&schema)).expect("df schema");
        let physical = |expr: &Expr| {
            datafusion::physical_expr::create_physical_expr(
                expr,
                &df_schema,
                &datafusion::execution::context::ExecutionProps::new(),
                &datafusion::logical_expr::physical_planning_context::PhysicalPlanningContext::default(),
            )
            .expect("physical expression")
        };
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
    fn read(published: &Bytes, plan: FileReadPlan) -> RecordBatch {
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
    fn decoded_leaves(published: &Bytes, plan: &FileReadPlan) -> Vec<String> {
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

    /// Writes `groups` of JSON documents as one row group each, with `v`
    /// shredded on `a: Int64` and ids numbered from 1 across groups.
    ///
    /// # Panics
    ///
    /// Panics when the fixture cannot be shredded or encoded.
    fn write_groups(groups: &[&[&str]]) -> Bytes {
        let layout = ShreddedSchemaBuilder::new()
            .with_path("a", &DataType::Int64)
            .expect("layout")
            .build();
        let mut id = 0_i64;
        let batches = groups
            .iter()
            .map(|documents| {
                let text: ArrayRef = Arc::new(StringArray::from(documents.to_vec()));
                let variant: ArrayRef =
                    shred_variant(&json_to_variant(&text).expect("encode"), &layout)
                        .expect("shred")
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
    /// all null and the comparison's type is exactly the shredded type: the
    /// group holding a residual `"10"` is always read and its row still
    /// matches, an all-typed group outside the literal is skipped, a `Utf8`
    /// comparison against the `Int64` shredding prunes nothing, and pruned
    /// and unpruned reads return identical rows.
    ///
    /// # Panics
    ///
    /// Panics when a group is pruned or kept against the rule above, or when
    /// pruned and unpruned reads differ.
    #[test]
    fn typed_variant_statistics_prune_only_with_all_null_residuals() {
        let published = write_groups(&[
            &[r#"{"a":1}"#, r#"{"a":2}"#],
            &[r#"{"a":10}"#, r#"{"a":"10"}"#],
            &[r#"{"a":20}"#, r#"{"a":21}"#],
        ]);
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
        ] {
            let read_plan = || plan(&published, &[("id", col("id"))], Some(&filter));
            assert_eq!(
                read_plan().select_row_groups(vec![0, 1, 2]).retained,
                retained,
                "{filter}"
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
                assert_eq!(read, ids, "{filter}");
            }
        }
    }
}
