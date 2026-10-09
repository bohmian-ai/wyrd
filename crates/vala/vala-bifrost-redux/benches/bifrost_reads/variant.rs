//! Section 3 (`-- variant`): Variant reads, unshredded vs shredded, through
//! plain `DataFusion` and both Wyrd read paths over the same files.
//!
//! The shapes, row counts, 16 KiB strings and SQL cases are `DataFusion`'s
//! `parquet_variant_query` benchmark's, word for word: narrow, wide and nested
//! objects, 2 batches of 256 rows, one 256-row row group per batch. Each shape
//! is written twice through Bifrost's own Variant write path (the Bifrost
//! writer recipe, the Scribe sampler under `BIFROST_VARIANT_SHREDDING`, and the
//! layout's `shred`): once unshredded, once shredded onto the objects' own
//! type. Both tables declare the canonical, unshredded Variant, as a table does
//! when its files may hold either layout.
//!
//! Every `{shape}/{layout}/{case}` runs on three engines over the same file:
//! plain `DataFusion` (a `ListingTable` under the canonical schema, as the
//! `DataFusion` benchmark registers it), the Wyrd hot read, and the Wyrd
//! published read. Before any timing, every case's rows (sorted, Variant as
//! JSON) must be equal across both layouts and all three engines; a mismatch
//! aborts naming the case. Then each case is timed end to end and planning
//! alone: [`WARM_UP`] of untimed runs, then [`SAMPLES`] timed runs per engine,
//! the engines alternating every run. The section prints both tables and every
//! gate the plan sets with its ratio and verdict.

use std::fs::File;
use std::ops::Range;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use arrow::array::{ArrayRef, Int32Array, RecordBatch, StringArray, StructArray};
use arrow::datatypes::{DataType, Field, Fields, Schema, SchemaRef};
use datafusion::datasource::file_format::parquet::ParquetFormat;
use datafusion::datasource::listing::{
    ListingOptions, ListingTable, ListingTableConfig, ListingTableUrl,
};
use datafusion::prelude::SessionContext;
use iceberg::writer::file_writer::variant_shredding::{VariantLayout, VariantSampler};
use parquet::arrow::ArrowWriter;
use parquet_variant_compute::cast_to_variant;
use vala_bifrost_redux::bench_support::OracleBenchTables;
use vala_bifrost_redux::parquet::BIFROST_VARIANT_SHREDDING;

use crate::report::{self, Latency, Side, micros};

/// Batches written per file.
const NUM_BATCHES: usize = 2;
/// Rows per written batch and per row group.
const WRITE_RECORD_BATCH_SIZE: usize = 256;
/// Bytes in every string leaf.
const LARGE_STRING_LEN: usize = 16 * 1024;
/// Untimed running time per case before its samples, as Criterion's warm-up.
const WARM_UP: Duration = Duration::from_secs(3);
/// Timed runs per engine per case.
const SAMPLES: usize = 30;
/// The two file layouts, `(layout, shredded)`, in report order.
const LAYOUTS: [(&str, bool); 2] = [("unshredded", false), ("shredded", true)];
/// The engines' column labels; index 0 is the plain `DataFusion` baseline.
const ENGINES: [&str; 3] = ["datafusion", "wyrd hot", "wyrd published"];
/// Cases that read the whole Variant, held to the looser layout gate.
const WHOLE_VARIANT: [&str; 2] = ["select_variant", "select_all"];

/// `(shape, the objects' type, [(case, SQL)])`.
type Shape = (&'static str, Fields, [(&'static str, &'static str); 6]);

/// A non-null `Utf8` object key.
fn utf8(name: &str) -> Field {
    Field::new(name, DataType::Utf8, false)
}

/// The non-null `Int32` key `small_int`.
fn small_int() -> Field {
    Field::new("small_int", DataType::Int32, false)
}

/// The `DataFusion` benchmark's three shapes and their six cases each.
fn shapes() -> [Shape; 3] {
    let inner = Fields::from(vec![utf8("large_string"), small_int()]);
    [
        (
            "narrow",
            inner.clone(),
            [
                ("select_variant", "SELECT s FROM t"),
                (
                    "select_small_field",
                    "SELECT CAST(s ->> 'small_int' AS INT) FROM t",
                ),
                ("select_large_field", "SELECT s ->> 'large_string' FROM t"),
                ("select_all", "SELECT * FROM t"),
                (
                    "select_id_and_small_field",
                    "SELECT id, CAST(s ->> 'small_int' AS INT) FROM t",
                ),
                (
                    "sum_small_field",
                    "SELECT SUM(CAST(s ->> 'small_int' AS INT)) FROM t",
                ),
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
            [
                ("select_variant", "SELECT s FROM t"),
                (
                    "select_small_field",
                    "SELECT CAST(s ->> 'small_int' AS INT) FROM t",
                ),
                ("select_one_string_field", "SELECT s ->> 'str_a' FROM t"),
                (
                    "select_two_string_fields",
                    "SELECT s ->> 'str_a', s ->> 'str_b' FROM t",
                ),
                ("select_all", "SELECT * FROM t"),
                (
                    "sum_small_field",
                    "SELECT SUM(CAST(s ->> 'small_int' AS INT)) FROM t",
                ),
            ],
        ),
        (
            "nested",
            Fields::from(vec![
                Field::new("inner", DataType::Struct(inner), false),
                utf8("extra_string"),
            ]),
            [
                ("select_variant", "SELECT s FROM t"),
                ("select_inner_object", "SELECT s -> 'inner' FROM t"),
                (
                    "select_inner_small_field",
                    "SELECT CAST(s -> 'inner' ->> 'small_int' AS INT) FROM t",
                ),
                ("select_extra_string", "SELECT s ->> 'extra_string' FROM t"),
                ("select_all", "SELECT * FROM t"),
                (
                    "sum_inner_small_field",
                    "SELECT SUM(CAST(s -> 'inner' ->> 'small_int' AS INT)) FROM t",
                ),
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
/// path and returns its three engines.
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
/// an engine cannot be registered.
async fn engines(
    tables: &OracleBenchTables,
    root: &Path,
    (shape, fields): (&str, &Fields),
    (layout, shredded): (&str, bool),
) -> [SessionContext; 3] {
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

    let native = OracleBenchTables::native_session();
    let options =
        ListingOptions::new(Arc::new(ParquetFormat::default())).with_file_extension(".parquet");
    let config = ListingTableConfig::new(
        ListingTableUrl::parse(dir.to_str().expect("UTF-8 fixture path")).expect("table URL"),
    )
    .with_listing_options(options)
    .with_schema(Arc::clone(&schema));
    // The session's statistics cache, as `register_parquet` attaches it.
    let listing = ListingTable::try_new(config)
        .expect("native table")
        .with_cache(
            native
                .runtime_env()
                .cache_manager
                .get_file_statistic_cache(),
        );
    native
        .register_table("t", Arc::new(listing))
        .expect("native table");
    let hot = tables
        .session(&[("t", Arc::clone(&schema), &path)])
        .await
        .expect("Wyrd hot session");
    let table = tables.publish("t", &schema, &path).await.expect("publish");
    let published = tables
        .published_session(&[("t", &table)])
        .await
        .expect("Wyrd published session");
    [native, hot, published]
}

/// What one timed run measures.
#[derive(Clone, Copy)]
enum Measure {
    /// Plan, execute, and collect every batch.
    Run,
    /// Plan to a physical plan only.
    Plan,
}

impl Measure {
    /// Runs `sql` in `context` once and returns how long this measure took.
    ///
    /// # Panics
    ///
    /// Panics when the query fails to plan or execute.
    async fn once(self, context: &SessionContext, sql: &str) -> Duration {
        match self {
            Self::Run => report::run(&Side { context, sql }).await,
            Self::Plan => {
                let start = Instant::now();
                context
                    .sql(sql)
                    .await
                    .expect("plans")
                    .create_physical_plan()
                    .await
                    .expect("plans physically");
                start.elapsed()
            }
        }
    }

    /// Warms every engine up for [`WARM_UP`], then takes [`SAMPLES`] timed
    /// runs of each, the engines alternating every run so machine drift lands
    /// on all of them equally.
    ///
    /// # Panics
    ///
    /// Panics when the query fails to plan or execute.
    async fn sample(self, engines: &[SessionContext; 3], sql: &str) -> [Latency; 3] {
        let start = Instant::now();
        while start.elapsed() < WARM_UP {
            for context in engines {
                self.once(context, sql).await;
            }
        }
        let mut samples: [Vec<Duration>; 3] = Default::default();
        for _ in 0..SAMPLES {
            for (context, runs) in engines.iter().zip(&mut samples) {
                runs.push(self.once(context, sql).await);
            }
        }
        samples.map(Latency::of)
    }
}

/// One timed `{shape}/{layout}/{case}`: each engine's end-to-end and
/// planning latency, in [`ENGINES`] order.
struct Timed {
    /// The shape.
    shape: &'static str,
    /// The file layout.
    layout: &'static str,
    /// The case.
    case: &'static str,
    /// End-to-end latency per engine.
    run: [Latency; 3],
    /// Planning latency per engine.
    plan: [Latency; 3],
}

/// The section's results, in run order.
pub struct VariantReport {
    /// Every timed case.
    timed: Vec<Timed>,
}

/// Writes every shape in both layouts under `root`, proves every case
/// equivalent everywhere, then times every case.
///
/// # Panics
///
/// Panics when a fixture cannot be written or registered, a query fails, or a
/// case's rows differ between layouts or engines; the message names the case.
pub async fn run(tables: &OracleBenchTables, root: &Path) -> VariantReport {
    let mut written = Vec::new();
    for (shape, fields, cases) in shapes() {
        let mut layouts = Vec::new();
        for layout in LAYOUTS {
            layouts.push((
                layout.0,
                engines(tables, root, (shape, &fields), layout).await,
            ));
        }
        written.push((shape, cases, layouts));
    }
    for (shape, cases, layouts) in &written {
        for (case, sql) in cases {
            let expected = report::rows(&layouts[0].1[0], sql).await;
            for (layout, engines) in layouts {
                for (engine, context) in ENGINES.iter().zip(engines) {
                    assert!(
                        report::rows(context, sql).await == expected,
                        "{shape}/{layout}/{case}: {engine} returns different rows from \
                         datafusion over the unshredded file for `{sql}`"
                    );
                }
            }
        }
    }
    let mut timed = Vec::new();
    for (shape, cases, layouts) in &written {
        for (layout, engines) in layouts {
            for (case, sql) in cases {
                timed.push(Timed {
                    shape,
                    layout,
                    case,
                    run: Measure::Run.sample(engines, sql).await,
                    plan: Measure::Plan.sample(engines, sql).await,
                });
                eprintln!("timed {shape}/{layout}/{case}");
            }
        }
    }
    VariantReport { timed }
}

impl VariantReport {
    /// Prints the end-to-end table, the planning table, and every gate.
    pub fn print(&self) {
        Self::table(
            "Variant reads: plain DataFusion vs Wyrd hot and published",
            |t| &t.run,
            &self.timed,
        );
        Self::table("Variant planning only", |t| &t.plan, &self.timed);
        let gates = self.gates();
        let missed = gates.iter().filter(|gate| gate.ends_with("MISS |")).count();
        println!("\n## Variant gates ({missed} of {} missed)\n", gates.len());
        println!("| gate | case | engine | ratio | limit | verdict |");
        println!("|---|---|---|---:|---|---|");
        for gate in &gates {
            println!("{gate}");
        }
    }

    /// Prints one titled table of `latency` per engine for every timed case.
    fn table(title: &str, latency: impl Fn(&Timed) -> &[Latency; 3], timed: &[Timed]) {
        println!("\n## {title}\n");
        print!("| case |");
        for engine in ENGINES {
            print!(" {engine} p50 | {engine} p95 |");
        }
        println!("\n|---|{}", "---:|---:|".repeat(ENGINES.len()));
        for t in timed {
            print!("| `{}/{}/{}` |", t.shape, t.layout, t.case);
            for l in latency(t) {
                print!(" {} | {} |", micros(l.p50), micros(l.p95));
            }
            println!();
        }
    }

    /// Every gate with its ratio and `PASS` or `MISS`, each as one Markdown
    /// table row.
    ///
    /// Shredded over unshredded p50: at most 0.8x for key reads and 2.0x for
    /// whole-Variant reads, per engine; planning within ±10%, per engine. Wyrd
    /// over plain `DataFusion` p50: at most 1.10x, per layout and Wyrd path.
    fn gates(&self) -> Vec<String> {
        let ratio = |a: &Latency, b: &Latency| a.p50.as_secs_f64() / b.p50.as_secs_f64();
        let verdict = |pass: bool| if pass { "PASS" } else { "MISS" };
        let mut gates = Vec::new();
        for shredded in self.timed.iter().filter(|t| t.layout == "shredded") {
            let id = format!("{}/{}", shredded.shape, shredded.case);
            let unshredded = self
                .timed
                .iter()
                .find(|t| {
                    t.layout == "unshredded" && t.shape == shredded.shape && t.case == shredded.case
                })
                .expect("every case is timed in both layouts");
            let limit = if WHOLE_VARIANT.contains(&shredded.case) {
                2.0
            } else {
                0.8
            };
            for (engine, index) in ENGINES.iter().zip(0..) {
                let run = ratio(&shredded.run[index], &unshredded.run[index]);
                gates.push(format!(
                    "| shredded / unshredded | `{id}` | {engine} | {run:.2} | <= {limit} | {} |",
                    verdict(run <= limit)
                ));
                let plan = ratio(&shredded.plan[index], &unshredded.plan[index]);
                gates.push(format!(
                    "| planning shredded / unshredded | `{id}` | {engine} | {plan:.2} | 0.9 to 1.1 | {} |",
                    verdict((0.9..=1.1).contains(&plan))
                ));
            }
        }
        for t in &self.timed {
            for (engine, index) in ENGINES.iter().zip(0..).skip(1) {
                let wyrd = ratio(&t.run[index], &t.run[0]);
                gates.push(format!(
                    "| Wyrd / DataFusion | `{}/{}/{}` | {engine} | {wyrd:.2} | <= 1.10 | {} |",
                    t.shape,
                    t.layout,
                    t.case,
                    verdict(wyrd <= 1.10)
                ));
            }
        }
        gates
    }
}
