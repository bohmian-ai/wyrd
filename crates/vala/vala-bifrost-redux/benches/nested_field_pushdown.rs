//! `mise run bench:bifrost:nested-field-pushdown`: nested-field reads through
//! the production Oracle hot read, for a fixed Struct and the same object as an
//! unshredded and a shredded Variant, beside plain `DataFusion` reading the
//! same Struct file as the reference.
//!
//! The workload is the pinned `DataFusion` fork's `parquet_struct_projection`
//! benchmark (spec revision 24): 512 rows written as two 256-row batches into
//! 256-row row groups, with 16 KiB string siblings that vary per row. Before
//! timing, every case asserts its exact result and logical expression
//! (`get_field` or `variant_get`); the Oracle forms also assert their leaf
//! paths, and the Struct and shredded forms assert bytes read proving no
//! unselected large sibling was read. Unshredded Variant must read the whole
//! value, so it has no byte bound. It prints only Criterion medians; there are
//! no thresholds.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::fs::File;
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use arrow::array::{ArrayRef, Int32Array, RecordBatch, StringArray, StructArray};
use arrow::datatypes::{DataType, Field, Schema, SchemaRef};
use arrow::util::display::array_value_to_string;
use criterion::Criterion;
use datafusion::prelude::{ParquetReadOptions, SessionContext};
use parquet::arrow::ArrowWriter;
use parquet::file::metadata::ParquetMetaDataReader;
use parquet_variant_compute::{cast_to_variant, shred_variant};
use serde_json::{Value, json};
use vala_bifrost_redux::bench_support::{OracleHotTables, bytes_scanned, hot_leaf_paths};

/// Rows per written batch and per row group.
const ROWS_PER_BATCH: usize = 256;
/// Batches written per file.
const BATCHES: usize = 2;
/// Bytes in every large string value.
const STRING_BYTES: usize = 16 * 1024;
/// `SUM(small_int)` over ids `0..512`.
const SMALL_INT_SUM: &str = "130816";
/// Every large string sibling's name; the bytes check keys on these.
const LARGE_STRINGS: [&str; 6] = [
    "large_string",
    "str_a",
    "str_b",
    "str_c",
    "str_d",
    "extra_string",
];

/// One logical object shape, as in the fork benchmark.
#[derive(Clone, Copy)]
enum Shape {
    /// `s{large_string, small_int}`.
    Narrow,
    /// `s{str_a, str_b, str_c, str_d, small_int}`.
    Wide,
    /// `s{inner{large_string, small_int}, extra_string}`.
    Nested,
}

/// One timed query and what it must return and read.
struct Case {
    /// Criterion function name.
    name: &'static str,
    /// The query over the Struct form.
    struct_sql: &'static str,
    /// The same query over the Variant form.
    variant_sql: &'static str,
    /// The leaf paths both forms must trim `s` to.
    leaf_paths: Value,
    /// How many large string siblings the query selects.
    large_reads: u64,
    /// The result rows, each column rendered and comma-joined, sorted.
    rows: Vec<String>,
}

impl Shape {
    /// Every shape, in report order.
    const ALL: [Self; 3] = [Self::Wide, Self::Nested, Self::Narrow];

    /// The Criterion group and table name.
    fn name(self) -> &'static str {
        match self {
            Self::Narrow => "narrow",
            Self::Wide => "wide",
            Self::Nested => "nested",
        }
    }

    /// The table schema without field ids: `id Int32` and the object `s`.
    fn plain_schema(self) -> Schema {
        let string = |name: &str| Field::new(name, DataType::Utf8, false);
        let small = Field::new("small_int", DataType::Int32, false);
        let object = |fields: Vec<Field>| DataType::Struct(fields.into());
        let s = match self {
            Self::Narrow => object(vec![string("large_string"), small]),
            Self::Wide => object(vec![
                string("str_a"),
                string("str_b"),
                string("str_c"),
                string("str_d"),
                small,
            ]),
            Self::Nested => object(vec![
                Field::new("inner", object(vec![string("large_string"), small]), false),
                string("extra_string"),
            ]),
        };
        Schema::new(vec![
            Field::new("id", DataType::Int32, false),
            Field::new("s", s, false),
        ])
    }

    /// The cases the spec reports for this shape.
    fn cases(self) -> Vec<Case> {
        let ids = || (0..ROWS).map(|row| row.to_string()).collect::<Vec<_>>();
        let small = json!({"s": [["small_int"]]});
        match self {
            Self::Wide => vec![
                Case {
                    name: "select_small_field",
                    struct_sql: "SELECT s['small_int'] FROM wide",
                    variant_sql: "SELECT CAST(s ->> 'small_int' AS INT) FROM wide_variant",
                    leaf_paths: small.clone(),
                    large_reads: 0,
                    rows: ids(),
                },
                Case {
                    name: "sum_small_field",
                    struct_sql: "SELECT SUM(s['small_int']) FROM wide",
                    variant_sql: "SELECT SUM(CAST(s ->> 'small_int' AS INT)) FROM wide_variant",
                    leaf_paths: small,
                    large_reads: 0,
                    rows: vec![SMALL_INT_SUM.to_owned()],
                },
                Case {
                    name: "select_one_string_field",
                    struct_sql: "SELECT s['str_a'] FROM wide",
                    variant_sql: "SELECT s ->> 'str_a' FROM wide_variant",
                    leaf_paths: json!({"s": [["str_a"]]}),
                    large_reads: 1,
                    rows: (0..ROWS).map(|row| text("str_a", row)).collect(),
                },
            ],
            Self::Nested => {
                let inner = json!({"s": [["inner", "small_int"]]});
                vec![
                    Case {
                        name: "select_extra_string",
                        struct_sql: "SELECT s['extra_string'] FROM nested",
                        variant_sql: "SELECT s ->> 'extra_string' FROM nested_variant",
                        leaf_paths: json!({"s": [["extra_string"]]}),
                        large_reads: 1,
                        rows: (0..ROWS).map(|row| text("extra_string", row)).collect(),
                    },
                    Case {
                        name: "select_inner_small_field",
                        struct_sql: "SELECT s['inner']['small_int'] FROM nested",
                        variant_sql: "SELECT CAST(s -> 'inner' ->> 'small_int' AS INT) FROM nested_variant",
                        leaf_paths: inner.clone(),
                        large_reads: 0,
                        rows: ids(),
                    },
                    Case {
                        name: "sum_inner_small_field",
                        struct_sql: "SELECT SUM(s['inner']['small_int']) FROM nested",
                        variant_sql: "SELECT SUM(CAST(s -> 'inner' ->> 'small_int' AS INT)) FROM nested_variant",
                        leaf_paths: inner,
                        large_reads: 0,
                        rows: vec![SMALL_INT_SUM.to_owned()],
                    },
                ]
            }
            Self::Narrow => vec![
                Case {
                    name: "select_small_field",
                    struct_sql: "SELECT s['small_int'] FROM narrow",
                    variant_sql: "SELECT CAST(s ->> 'small_int' AS INT) FROM narrow_variant",
                    leaf_paths: small.clone(),
                    large_reads: 0,
                    rows: ids(),
                },
                Case {
                    name: "select_id_and_small_field",
                    struct_sql: "SELECT id, s['small_int'] FROM narrow",
                    variant_sql: "SELECT id, CAST(s ->> 'small_int' AS INT) FROM narrow_variant",
                    leaf_paths: small,
                    large_reads: 0,
                    rows: (0..ROWS).map(|row| format!("{row},{row}")).collect(),
                },
            ],
        }
    }
}

/// Rows in every file.
const ROWS: usize = ROWS_PER_BATCH * BATCHES;

/// The value of string leaf `name` at `row`: `STRING_BYTES` of xorshift hex,
/// seeded by both, so siblings differ and nothing collapses to a dictionary.
fn text(name: &str, row: usize) -> String {
    let mut state = name
        .bytes()
        .fold(row as u64 ^ 0x9E37_79B9_7F4A_7C15, |hash, byte| {
            hash.rotate_left(5) ^ u64::from(byte)
        })
        | 1;
    let mut value = String::with_capacity(STRING_BYTES);
    while value.len() < STRING_BYTES {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        write!(value, "{state:016x}").expect("writing to a String cannot fail");
    }
    value
}

/// The column `field` holds over `rows`: `Int32` leaves hold the row number,
/// string leaves [`text`], and Structs their children.
fn column(field: &Field, rows: Range<usize>) -> ArrayRef {
    match field.data_type() {
        DataType::Struct(fields) => Arc::new(StructArray::new(
            fields.clone(),
            fields
                .iter()
                .map(|child| column(child, rows.clone()))
                .collect(),
            None,
        )),
        DataType::Int32 => Arc::new(Int32Array::from_iter_values(
            rows.map(|row| i32::try_from(row).expect("512 rows fit i32")),
        )),
        _ => Arc::new(StringArray::from_iter_values(
            rows.map(|row| text(field.name(), row)),
        )),
    }
}

/// Writes `shape` as a Struct file, a fully shredded Variant file and an
/// unshredded Variant file under `dir`, and returns `(logical schema, file)`
/// for each, in that order.
///
/// Field ids come from Iceberg's own assignment. The Variant is arrow's
/// `cast_to_variant` of the Struct; the shredded file shreds it onto the
/// Struct's own type, the unshredded file writes it as cast.
fn write_shape(tables: &OracleHotTables, dir: &Path, shape: Shape) -> [(SchemaRef, PathBuf); 3] {
    let plain = shape.plain_schema();
    let iceberg =
        iceberg::arrow::arrow_schema_to_schema_auto_assign_ids(&plain).expect("Iceberg schema");
    let logical = Arc::new(iceberg::arrow::schema_to_arrow_schema(&iceberg).expect("Arrow schema"));
    let object_type = plain.field(1).data_type();
    let variant = wyrd_types::variant::variant_field("s", false);
    let mut metadata = variant.metadata().clone();
    metadata.extend(logical.field(1).metadata().clone());
    let variant_logical = Arc::new(Schema::new(vec![
        logical.field(0).clone(),
        variant.with_metadata(metadata),
    ]));
    let batches: Vec<RecordBatch> = (0..BATCHES)
        .map(|batch| {
            let rows = batch * ROWS_PER_BATCH..(batch + 1) * ROWS_PER_BATCH;
            let columns = logical
                .fields()
                .iter()
                .map(|field| column(field, rows.clone()));
            RecordBatch::try_new(Arc::clone(&logical), columns.collect()).expect("Struct batch")
        })
        .collect();
    let variants = |shred: bool| -> Vec<RecordBatch> {
        batches
            .iter()
            .map(|batch| {
                let cast = cast_to_variant(batch.column(1)).expect("Struct casts to Variant");
                let variant = if shred {
                    shred_variant(&cast, object_type).expect("Variant shreds on the Struct type")
                } else {
                    cast
                };
                let field = Field::new("s", variant.data_type().clone(), false)
                    .with_metadata(variant_logical.field(1).metadata().clone());
                let schema = Schema::new(vec![logical.field(0).clone(), field]);
                RecordBatch::try_new(
                    Arc::new(schema),
                    vec![Arc::clone(batch.column(0)), variant.into()],
                )
                .expect("Variant batch")
            })
            .collect()
    };
    let write = |suffix: &str, schema: SchemaRef, batches: &[RecordBatch]| {
        let path = dir.join(format!("{}_{suffix}.parquet", shape.name()));
        let file = File::create(&path).expect("create fixture");
        let mut writer = ArrowWriter::try_new(
            file,
            batches[0].schema(),
            Some(tables.writer_properties(ROWS_PER_BATCH)),
        )
        .expect("writer");
        for batch in batches {
            writer.write(batch).expect("write");
        }
        writer.close().expect("close");
        (schema, path)
    };
    let (shredded, unshredded) = (variants(true), variants(false));
    [
        write("struct", Arc::clone(&logical), &batches),
        write("variant", Arc::clone(&variant_logical), &shredded),
        write("unshredded", Arc::clone(&variant_logical), &unshredded),
    ]
}

/// Compressed bytes of the smallest large string sibling in `path`, summed
/// over row groups; Variant residual leaves count with their sibling.
fn smallest_large_leaf(path: &Path) -> u64 {
    let file = File::open(path).expect("open fixture");
    let footer = ParquetMetaDataReader::new()
        .parse_and_finish(&file)
        .expect("fixture footer");
    let mut sizes = BTreeMap::<&str, u64>::new();
    for group in footer.row_groups() {
        for chunk in group.columns() {
            let parts = chunk.column_path().parts();
            if let Some(name) = LARGE_STRINGS
                .iter()
                .find(|name| parts.iter().any(|part| part == *name))
            {
                *sizes.entry(name).or_default() +=
                    u64::try_from(chunk.compressed_size()).expect("size");
            }
        }
    }
    sizes
        .into_values()
        .min()
        .expect("the fixture holds a large string")
}

/// Runs `sql` once and asserts everything [`Case`] promises before timing.
///
/// Every form must return the case's rows through `function`. `oracle` forms
/// also report the case's leaf paths; `large` is the byte bound for forms
/// that must skip unselected large siblings, and `None` for forms that read
/// the whole object.
async fn check(
    context: &SessionContext,
    sql: &str,
    function: &str,
    case: &Case,
    oracle: bool,
    large: Option<u64>,
) {
    let frame = context.sql(sql).await.expect("plans");
    let logical = frame.logical_plan().display_indent().to_string();
    assert!(
        logical.contains(function),
        "{sql}: {function} missing from\n{logical}"
    );
    let plan = frame.create_physical_plan().await.expect("physical plan");
    let batches = datafusion::physical_plan::collect(Arc::clone(&plan), context.task_ctx())
        .await
        .expect("executes");
    let mut rows = Vec::new();
    for batch in &batches {
        for row in 0..batch.num_rows() {
            let values = batch
                .columns()
                .iter()
                .map(|column| array_value_to_string(column, row).expect("renders"));
            rows.push(values.collect::<Vec<_>>().join(","));
        }
    }
    rows.sort();
    let mut expected = case.rows.clone();
    expected.sort();
    assert!(rows == expected, "{sql}: wrong result");
    if !oracle {
        return;
    }
    let paths = hot_leaf_paths(&plan).expect("leaf paths serialize");
    assert_eq!(paths, vec![case.leaf_paths.clone()], "{sql}: leaf paths");
    if let Some(large) = large {
        let read = bytes_scanned(plan.as_ref()).expect("the scan reports bytes");
        assert!(
            read < (case.large_reads + 1) * large,
            "{sql}: read {read} bytes, a large sibling is {large}"
        );
    }
}

/// Builds every fixture, asserts every case, times them, and prints medians.
fn main() {
    let runtime = tokio::runtime::Runtime::new().expect("Tokio runtime");
    let dir = tempfile::tempdir().expect("fixture directory");
    let tables = OracleHotTables::new(dir.path());
    let out = Path::new(env!("CARGO_TARGET_TMPDIR")).join("nested-field-pushdown");
    let mut criterion = Criterion::default()
        .output_directory(&out)
        .configure_from_args();
    let mut medians = Vec::new();
    for shape in Shape::ALL {
        let [
            (struct_schema, struct_file),
            (variant_schema, variant_file),
            (unshredded_schema, unshredded_file),
        ] = write_shape(&tables, dir.path(), shape);
        let variant_table = format!("{}_variant", shape.name());
        let unshredded_table = format!("{}_unshredded", shape.name());
        let context = runtime
            .block_on(tables.session(&[
                (shape.name(), struct_schema, &struct_file),
                (&variant_table, variant_schema, &variant_file),
                (&unshredded_table, unshredded_schema, &unshredded_file),
            ]))
            .expect("session");
        let plain = SessionContext::new();
        runtime
            .block_on(plain.register_parquet(
                shape.name(),
                struct_file.to_str().expect("UTF-8 fixture path"),
                ParquetReadOptions::default(),
            ))
            .expect("plain DataFusion table");
        let large = smallest_large_leaf(&struct_file).min(smallest_large_leaf(&variant_file));
        let mut group = criterion.benchmark_group(shape.name());
        group
            .sample_size(10)
            .warm_up_time(Duration::from_secs(1))
            .measurement_time(Duration::from_secs(2));
        for case in shape.cases() {
            let unshredded_sql = case.variant_sql.replace(&variant_table, &unshredded_table);
            for (form, context, sql, function, oracle, bound) in [
                (
                    "datafusion",
                    &plain,
                    case.struct_sql,
                    "get_field",
                    false,
                    None,
                ),
                (
                    "struct",
                    &context,
                    case.struct_sql,
                    "get_field",
                    true,
                    Some(large),
                ),
                (
                    "variant_unshredded",
                    &context,
                    unshredded_sql.as_str(),
                    "variant_get",
                    true,
                    None,
                ),
                (
                    "variant_shredded",
                    &context,
                    case.variant_sql,
                    "variant_get",
                    true,
                    Some(large),
                ),
            ] {
                runtime.block_on(check(context, sql, function, &case, oracle, bound));
                let id = format!("{}_{form}", case.name);
                group.bench_function(&id, |bencher| {
                    bencher.to_async(&runtime).iter(|| async {
                        context
                            .sql(sql)
                            .await
                            .expect("plans")
                            .collect()
                            .await
                            .expect("executes")
                    });
                });
                medians.push((shape.name(), id));
            }
        }
        group.finish();
    }
    criterion.final_summary();
    print_medians(&out, &medians);
}

/// Prints the Criterion median of every timed `(group, id)` as a Markdown table,
/// skipping ids a filtered run did not time.
///
/// # Panics
/// Panics when an estimates file Criterion wrote is not valid JSON or has no
/// median.
fn print_medians(out: &Path, medians: &[(&str, String)]) {
    println!("\n| Shape/case | Median |\n|---|---:|");
    for (shape, id) in medians {
        let estimates = out.join(shape).join(id).join("new/estimates.json");
        let Ok(text) = std::fs::read_to_string(&estimates) else {
            continue;
        };
        let estimates: Value = serde_json::from_str(&text).expect("Criterion estimates");
        let nanos = estimates["median"]["point_estimate"]
            .as_f64()
            .expect("median");
        println!("| `{shape}/{id}` | {:.0} µs |", nanos / 1_000.0);
    }
}
