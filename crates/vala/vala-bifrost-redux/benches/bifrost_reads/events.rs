//! Sections 2 and 3: native `DataFusion` vs Wyrd over one stored `events`
//! table, first as a hot file, then as a published Iceberg snapshot.
//!
//! `events` has 4096 rows in four 1024-row row groups, one per second from
//! `2026-01-01T00:00:00Z`, and carries Bifrost's two managed time columns as
//! every Bifrost table does:
//!
//! | column | type | values |
//! |---|---|---|
//! | `id` | `Int64` | `0..4096` |
//! | `service` | `Utf8` | 8 distinct |
//! | `body` | `Utf8` | 4 KiB, different per row |
//! | `latency_ms` | `Float64` | `(id % 997) / 2`, exact in `f64` |
//! | `attrs` | `Struct{code Int32, region Utf8}` | 50 codes, 3 regions |
//! | `tags` | `List<Utf8>` | 0 to 3 colours |
//! | `wyrd_event_time` | `Timestamp(µs, UTC)` | one per second |
//! | `wyrd_ingested_at` | `Timestamp(µs, UTC)` | one second after the event |
//!
//! Both sections run the same [`CASES`] against the same file.

use std::fs::File;
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use arrow::array::{
    ArrayRef, Float64Array, Int32Array, Int64Array, ListBuilder, RecordBatch, StringArray,
    StringBuilder, StructArray, TimestampMicrosecondArray,
};
use arrow::datatypes::{DataType, Field, Schema, SchemaRef, TimeUnit};
use datafusion::prelude::ParquetReadOptions;
use parquet::arrow::ArrowWriter;
use vala_bifrost_redux::bench_support::OracleBenchTables;
use wyrd_spec::vala::{WYRD_EVENT_TIME, WYRD_INGESTED_AT};

use crate::report::{Report, Side};

/// Rows in the table.
const ROWS: usize = 4096;
/// Rows per written batch and per row group.
const ROWS_PER_GROUP: usize = 1024;
/// Bytes in every `body`.
const BODY_BYTES: usize = 4 * 1024;
/// `2026-01-01T00:00:00Z`, the first row's `wyrd_event_time`, in microseconds.
const START_MICROS: i64 = 1_767_225_600_000_000;
/// The `service` values, cycled by row.
const SERVICES: [&str; 8] = [
    "api", "auth", "billing", "cart", "search", "ship", "store", "user",
];
/// The `attrs.region` values, cycled by row.
const REGIONS: [&str; 3] = ["us", "eu", "ap"];
/// The `tags` values, cycled by row.
const COLOURS: [&str; 3] = ["red", "green", "blue"];

/// Every timed query, `(case, sql)`; both sides run the SQL as written.
const CASES: [(&str, &str); 8] = [
    (
        "point_lookup",
        "SELECT id, service, latency_ms FROM events WHERE id = 1234",
    ),
    ("select_narrow", "SELECT id, latency_ms FROM events"),
    ("select_wide", "SELECT * FROM events"),
    ("count_star", "SELECT COUNT(*) FROM events"),
    (
        "group_by",
        "SELECT service, COUNT(*), AVG(latency_ms) FROM events GROUP BY service",
    ),
    (
        "time_range",
        "SELECT id, latency_ms FROM events \
         WHERE wyrd_event_time >= '2026-01-01T00:10:00Z' \
         AND wyrd_event_time < '2026-01-01T00:15:00Z'",
    ),
    (
        "struct_field",
        "SELECT id, attrs['code'] FROM events WHERE attrs['region'] = 'eu'",
    ),
    (
        "list_predicate",
        "SELECT id FROM events WHERE array_has(tags, 'blue')",
    ),
];

/// The stored `events` table: its logical schema and its one Parquet file.
pub struct Events {
    /// The logical schema, carrying Iceberg's field ids.
    schema: SchemaRef,
    /// The file, under the fixture root.
    path: PathBuf,
}

/// Writes `events` under `dir` with the Bifrost writer recipe.
///
/// # Panics
///
/// Panics when the schema has no Iceberg form or the file cannot be written.
pub fn write(tables: &OracleBenchTables, dir: &Path) -> Events {
    let time = DataType::Timestamp(TimeUnit::Microsecond, Some("+00:00".into()));
    let plain = Schema::new(vec![
        Field::new("id", DataType::Int64, false),
        Field::new("service", DataType::Utf8, false),
        Field::new("body", DataType::Utf8, false),
        Field::new("latency_ms", DataType::Float64, false),
        Field::new(
            "attrs",
            DataType::Struct(
                vec![
                    Field::new("code", DataType::Int32, false),
                    Field::new("region", DataType::Utf8, false),
                ]
                .into(),
            ),
            false,
        ),
        Field::new_list("tags", Field::new("element", DataType::Utf8, false), false),
        Field::new(WYRD_EVENT_TIME, time.clone(), false),
        Field::new(WYRD_INGESTED_AT, time, false),
    ]);
    let iceberg =
        iceberg::arrow::arrow_schema_to_schema_auto_assign_ids(&plain).expect("Iceberg schema");
    let schema = Arc::new(iceberg::arrow::schema_to_arrow_schema(&iceberg).expect("Arrow schema"));
    let path = dir.join("events.parquet");
    let mut writer = ArrowWriter::try_new(
        File::create(&path).expect("create fixture"),
        Arc::clone(&schema),
        Some(tables.writer_properties(ROWS_PER_GROUP)),
    )
    .expect("writer");
    for start in (0..ROWS).step_by(ROWS_PER_GROUP) {
        let rows = start..start + ROWS_PER_GROUP;
        let columns = schema
            .fields()
            .iter()
            .map(|field| column(field, rows.clone()));
        let batch = RecordBatch::try_new(Arc::clone(&schema), columns.collect()).expect("batch");
        writer.write(&batch).expect("write");
    }
    writer.close().expect("close");
    Events { schema, path }
}

/// The column `field` holds over `rows`.
///
/// # Panics
///
/// Panics on a field `events` does not have.
fn column(field: &Field, rows: Range<usize>) -> ArrayRef {
    let strings = |values: &[&str], rows: Range<usize>| -> ArrayRef {
        Arc::new(StringArray::from_iter_values(
            rows.map(|row| values[row % values.len()]),
        ))
    };
    let number = |row: usize| i32::try_from(row).expect("4096 rows fit i32");
    let micros = |second: usize| START_MICROS + i64::from(number(second)) * 1_000_000;
    match (field.name().as_str(), field.data_type()) {
        ("id", _) => Arc::new(Int64Array::from_iter_values(
            rows.map(|row| i64::from(number(row))),
        )),
        (WYRD_EVENT_TIME, data_type) => Arc::new(
            TimestampMicrosecondArray::from_iter_values(rows.map(micros))
                .with_data_type(data_type.clone()),
        ),
        (WYRD_INGESTED_AT, data_type) => Arc::new(
            TimestampMicrosecondArray::from_iter_values(rows.map(|row| micros(row + 1)))
                .with_data_type(data_type.clone()),
        ),
        ("service", _) => strings(&SERVICES, rows),
        ("body", _) => Arc::new(StringArray::from_iter_values(
            rows.map(|row| crate::text("body", row, BODY_BYTES)),
        )),
        ("latency_ms", _) => Arc::new(Float64Array::from_iter_values(
            rows.map(|row| f64::from(number(row % 997)) * 0.5),
        )),
        ("attrs", DataType::Struct(fields)) => Arc::new(StructArray::new(
            fields.clone(),
            vec![
                Arc::new(Int32Array::from_iter_values(
                    rows.clone().map(|row| number(row % 50)),
                )),
                strings(&REGIONS, rows),
            ],
            None,
        )),
        ("tags", DataType::List(element)) => {
            let mut tags = ListBuilder::new(StringBuilder::new()).with_field(Arc::clone(element));
            for row in rows {
                for k in 0..row % 4 {
                    tags.values()
                        .append_value(COLOURS[(row + k) % COLOURS.len()]);
                }
                tags.append(true);
            }
            Arc::new(tags.finish())
        }
        (name, _) => panic!("events has no column {name}"),
    }
}

impl Events {
    /// Section 2: every case against the file as a hot file, natively through
    /// `register_parquet` and through the Oracle hot read.
    ///
    /// # Panics
    ///
    /// Panics when either table cannot be registered or a case fails.
    pub async fn hot(&self, tables: &OracleBenchTables) -> Report {
        let native = OracleBenchTables::native_session();
        native
            .register_parquet(
                "events",
                self.path.to_str().expect("UTF-8 fixture path"),
                ParquetReadOptions::default(),
            )
            .await
            .expect("native table");
        let wyrd = tables
            .session(&[("events", Arc::clone(&self.schema), &self.path)])
            .await
            .expect("Wyrd session");
        Self::time(
            Report::new("Hot file: native DataFusion vs Wyrd", "datafusion", "wyrd"),
            &native,
            &wyrd,
        )
        .await
    }

    /// Section 3: every case against the file committed as an Iceberg
    /// snapshot, natively through `IcebergStaticTableProvider` and through the
    /// Oracle published read.
    ///
    /// # Panics
    ///
    /// Panics when the snapshot cannot be committed, either table cannot be
    /// registered, or a case fails.
    pub async fn published(&self, tables: &OracleBenchTables) -> Report {
        let table = tables
            .publish("events", &self.schema, &self.path)
            .await
            .expect("publish");
        // Native reads the same snapshot through Iceberg's own local `FileIO`.
        let native_table = iceberg::table::Table::builder()
            .metadata(table.metadata_ref())
            .metadata_location(table.metadata_location().expect("committed table"))
            .identifier(table.identifier().clone())
            .file_io(iceberg::io::FileIO::new_with_fs())
            .runtime(iceberg::Runtime::current())
            .build()
            .expect("native table");
        let native = OracleBenchTables::native_session();
        native
            .register_table(
                "events",
                Arc::new(
                    iceberg_datafusion::IcebergStaticTableProvider::try_new_from_table(
                        native_table,
                    )
                    .await
                    .expect("native provider"),
                ),
            )
            .expect("native table");
        let wyrd = tables
            .published_session(&[("events", &table)])
            .await
            .expect("Wyrd session");
        Self::time(
            Report::new(
                "Published Iceberg snapshot: native DataFusion vs Wyrd",
                "datafusion",
                "wyrd",
            ),
            &native,
            &wyrd,
        )
        .await
    }

    /// Times every case with `native` on the left and `wyrd` on the right.
    async fn time(
        mut report: Report,
        native: &datafusion::prelude::SessionContext,
        wyrd: &datafusion::prelude::SessionContext,
    ) -> Report {
        for (case, sql) in CASES {
            report
                .time(
                    case,
                    Side {
                        context: native,
                        sql,
                    },
                    Side { context: wyrd, sql },
                )
                .await;
        }
        report
    }
}
