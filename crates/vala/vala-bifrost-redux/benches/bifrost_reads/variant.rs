//! Section 3 (`-- variant`): proves Bifrost's Variant shredding works and
//! speeds up key reads through Wyrd.
//!
//! The same Variant objects are written twice through Bifrost's own Variant
//! write path (the Bifrost writer recipe, the Scribe sampler under
//! `BIFROST_VARIANT_SHREDDING`, and the layout's `shred`): once unshredded,
//! once shredded onto the objects' own type. Both tables declare the
//! canonical, unshredded Variant, as a table does when its files may hold
//! either layout. Three object shapes (narrow, wide, nested; 2 batches of 256
//! rows, 16 KiB strings) each run a key read, a wider extraction where the
//! shape has one, and the whole value.
//!
//! Each Wyrd read path (hot and published) is one [`Report`] whose left side
//! is the unshredded file and right side the shredded one, so every case
//! asserts both layouts return identical rows before it is timed. Engine-level
//! Variant performance is `DataFusion`'s `parquet_variant_query` benchmark's
//! job, not this section's.

use std::fs::File;
use std::ops::Range;
use std::path::Path;
use std::sync::Arc;

use arrow::array::{ArrayRef, Int32Array, RecordBatch, StringArray, StructArray};
use arrow::datatypes::{DataType, Field, Fields, Schema, SchemaRef};
use datafusion::prelude::SessionContext;
use iceberg::writer::file_writer::variant_shredding::{VariantLayout, VariantSampler};
use parquet::arrow::ArrowWriter;
use parquet_variant_compute::cast_to_variant;
use vala_bifrost_redux::bench_support::OracleBenchTables;
use vala_bifrost_redux::parquet::BIFROST_VARIANT_SHREDDING;

use crate::report::{Report, Side};

/// Batches written per file.
const NUM_BATCHES: usize = 2;
/// Rows per written batch and per row group.
const WRITE_RECORD_BATCH_SIZE: usize = 256;
/// Bytes in every string leaf.
const LARGE_STRING_LEN: usize = 16 * 1024;
/// The case that reads the whole Variant, held to the looser gate.
const WHOLE_VARIANT: &str = "select_variant";
/// Highest shredded / unshredded p50 for a key read or extraction.
const KEY_READ_LIMIT: f64 = 0.8;
/// Highest shredded / unshredded p50 for a whole-Variant read.
const WHOLE_VARIANT_LIMIT: f64 = 2.0;

/// `(shape, the objects' type, [(case, SQL)])`.
type Shape = (&'static str, Fields, Vec<(&'static str, &'static str)>);

/// A non-null `Utf8` object key.
fn utf8(name: &str) -> Field {
    Field::new(name, DataType::Utf8, false)
}

/// The non-null `Int32` key `small_int`.
fn small_int() -> Field {
    Field::new("small_int", DataType::Int32, false)
}

/// The three shapes and their cases: a key read, a wider extraction where the
/// shape has more than one key to extract, and the whole value.
fn shapes() -> [Shape; 3] {
    let inner = Fields::from(vec![utf8("large_string"), small_int()]);
    [
        (
            "narrow",
            inner.clone(),
            vec![
                (
                    "select_small_field",
                    "SELECT CAST(s ->> 'small_int' AS INT) FROM t",
                ),
                (WHOLE_VARIANT, "SELECT s FROM t"),
            ],
        ),
        (
            "wide",
            Fields::from(vec![
                utf8("str_a"),
                utf8("str_b"),
                utf8("str_c"),
                utf8("str_d"),
                small_int(),
            ]),
            vec![
                (
                    "select_small_field",
                    "SELECT CAST(s ->> 'small_int' AS INT) FROM t",
                ),
                (
                    "select_two_string_fields",
                    "SELECT s ->> 'str_a', s ->> 'str_b' FROM t",
                ),
                (WHOLE_VARIANT, "SELECT s FROM t"),
            ],
        ),
        (
            "nested",
            Fields::from(vec![
                Field::new("inner", DataType::Struct(inner), false),
                utf8("extra_string"),
            ]),
            vec![
                (
                    "select_inner_small_field",
                    "SELECT CAST(s -> 'inner' ->> 'small_int' AS INT) FROM t",
                ),
                ("select_inner_object", "SELECT s -> 'inner' FROM t"),
                (WHOLE_VARIANT, "SELECT s FROM t"),
            ],
        ),
    ]
}

/// `field` over `rows`: `Int32` leaves hold the row number, strings
/// [`crate::text`] of [`LARGE_STRING_LEN`] seeded by the leaf name.
///
/// # Panics
///
/// Panics when a row number does not fit `i32`.
fn column(field: &Field, rows: Range<usize>) -> ArrayRef {
    match field.data_type() {
        DataType::Struct(fields) => Arc::new(StructArray::new(
            fields.clone(),
            fields.iter().map(|f| column(f, rows.clone())).collect(),
            None,
        )),
        DataType::Int32 => Arc::new(Int32Array::from_iter_values(
            rows.map(|row| i32::try_from(row).expect("512 rows fit i32")),
        )),
        _ => Arc::new(StringArray::from_iter_values(
            rows.map(|row| crate::text(field.name(), row, LARGE_STRING_LEN)),
        )),
    }
}

/// Writes one shape's objects in one layout through Bifrost's Variant write
/// path and returns the Wyrd hot and published sessions over that file.
///
/// The file is `{root}/{shape}/{layout}/data.parquet`. The logical schema is
/// `id Int32, s Variant` with Iceberg field ids; the encoder opens on the
/// layout's physical schema and every batch is shredded by the layout, as
/// Scribe's rolling writer does. A shredded layout is the Scribe sampler's
/// over every row and must shred `s`.
///
/// # Panics
///
/// Panics when the file cannot be written, the sampler does not shred `s`, or
/// a session cannot be registered.
async fn sessions(
    tables: &OracleBenchTables,
    root: &Path,
    shape: &str,
    fields: &Fields,
    shredded: bool,
) -> [SessionContext; 2] {
    let layout = if shredded { "shredded" } else { "unshredded" };
    let plain = Schema::new(vec![
        Field::new("id", DataType::Int32, false),
        wyrd_types::variant::variant_field("s", false),
    ]);
    let iceberg =
        iceberg::arrow::arrow_schema_to_schema_auto_assign_ids(&plain).expect("Iceberg schema");
    let schema: SchemaRef =
        Arc::new(iceberg::arrow::schema_to_arrow_schema(&iceberg).expect("Arrow schema"));
    let objects = Field::new("s", DataType::Struct(fields.clone()), false);
    let batches: Vec<RecordBatch> = (0..NUM_BATCHES)
        .map(|batch| {
            let rows = batch * WRITE_RECORD_BATCH_SIZE..(batch + 1) * WRITE_RECORD_BATCH_SIZE;
            let variant: ArrayRef = cast_to_variant(&column(&objects, rows.clone()))
                .expect("objects cast to Variant")
                .into();
            let variant =
                arrow::compute::cast(&variant, &wyrd_types::variant::variant_storage_type())
                    .expect("canonical Variant storage");
            RecordBatch::try_new(
                Arc::clone(&schema),
                vec![column(schema.field(0), rows), variant],
            )
            .expect("batch")
        })
        .collect();
    let variant_layout = if shredded {
        let mut sampler = VariantSampler::new(
            &schema,
            BIFROST_VARIANT_SHREDDING,
            0,
            &[NUM_BATCHES * WRITE_RECORD_BATCH_SIZE],
        );
        for (index, batch) in batches.iter().enumerate() {
            let position = u64::try_from(index * WRITE_RECORD_BATCH_SIZE).expect("fits u64");
            sampler
                .offer(batch, &vec![0; batch.num_rows()], position)
                .expect("sample");
        }
        let inferred = sampler.layout();
        let chosen = inferred.shredding_type(&["s"]).cloned();
        eprintln!("{shape}/{layout}: `s` shredded as {chosen:?}");
        assert!(
            chosen.is_some(),
            "{shape}/{layout}: the sampler left `s` unshredded"
        );
        inferred
    } else {
        VariantLayout::default()
    };
    let dir = root.join(shape).join(layout);
    std::fs::create_dir_all(&dir).expect("fixture directory");
    let path = dir.join("data.parquet");
    let mut writer = ArrowWriter::try_new(
        File::create(&path).expect("create fixture"),
        variant_layout
            .physical_schema(&schema)
            .expect("physical schema"),
        Some(tables.writer_properties(WRITE_RECORD_BATCH_SIZE)),
    )
    .expect("writer");
    for batch in &batches {
        writer
            .write(&variant_layout.shred(batch).expect("shred"))
            .expect("write");
    }
    writer.close().expect("close");

    let hot = tables
        .session(&[("t", Arc::clone(&schema), &path)])
        .await
        .expect("Wyrd hot session");
    let table = tables.publish("t", &schema, &path).await.expect("publish");
    let published = tables
        .published_session(&[("t", &table)])
        .await
        .expect("Wyrd published session");
    [hot, published]
}

/// The section's results: one unshredded-vs-shredded report per Wyrd path.
pub struct VariantReport {
    /// Wyrd hot, then Wyrd published.
    paths: [Report; 2],
}

/// Writes every shape in both layouts under `root`, then, per Wyrd path,
/// proves each case returns the same rows from both layouts and times it.
///
/// # Panics
///
/// Panics when a fixture cannot be written or registered, a query fails, or a
/// case's rows differ between layouts; the message names the case.
pub async fn run(tables: &OracleBenchTables, root: &Path) -> VariantReport {
    let mut paths = [
        Report::new("Variant on Wyrd hot", "unshredded", "shredded"),
        Report::new("Variant on Wyrd published", "unshredded", "shredded"),
    ];
    for (shape, fields, cases) in shapes() {
        let unshredded = sessions(tables, root, shape, &fields, false).await;
        let shredded = sessions(tables, root, shape, &fields, true).await;
        for (report, (left, right)) in paths.iter_mut().zip(unshredded.iter().zip(&shredded)) {
            for &(case, sql) in &cases {
                report
                    .time(
                        &format!("{shape}/{case}"),
                        Side { context: left, sql },
                        Side {
                            context: right,
                            sql,
                        },
                    )
                    .await;
            }
        }
    }
    VariantReport { paths }
}

impl VariantReport {
    /// Prints each path's table, then every gate with its verdict.
    pub fn print(&self) {
        for report in &self.paths {
            report.print();
        }
        let gates = self.gates();
        let missed = gates.iter().filter(|gate| gate.ends_with("MISS |")).count();
        println!("\n## Variant gates ({missed} of {} missed)\n", gates.len());
        println!("| path | case | shredded / unshredded p50 | limit | verdict |");
        println!("|---|---|---:|---|---|");
        for gate in &gates {
            println!("{gate}");
        }
    }

    /// Every gate as one Markdown table row: shredded over unshredded p50 at
    /// most [`KEY_READ_LIMIT`] for key reads and extractions and
    /// [`WHOLE_VARIANT_LIMIT`] for whole-Variant reads, per Wyrd path.
    fn gates(&self) -> Vec<String> {
        let mut gates = Vec::new();
        for (path, report) in ["hot", "published"].iter().zip(&self.paths) {
            for (case, unshredded, shredded) in &report.rows {
                let ratio = shredded.p50.as_secs_f64() / unshredded.p50.as_secs_f64();
                let limit = if case.ends_with(WHOLE_VARIANT) {
                    WHOLE_VARIANT_LIMIT
                } else {
                    KEY_READ_LIMIT
                };
                let verdict = if ratio <= limit { "PASS" } else { "MISS" };
                gates.push(format!(
                    "| {path} | `{case}` | {ratio:.2} | <= {limit} | {verdict} |"
                ));
            }
        }
        gates
    }
}
