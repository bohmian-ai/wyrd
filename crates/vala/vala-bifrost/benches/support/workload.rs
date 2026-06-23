use std::sync::Arc;

use arrow::array::{Array, Int64Array, StringArray};
use arrow::datatypes::{DataType, Field, Schema, SchemaRef};
use arrow::record_batch::RecordBatch;

/// User schema for standard write/scan benches: one Int64 + one Utf8 column.
pub fn simple_schema() -> SchemaRef {
    Arc::new(Schema::new(vec![
        Field::new("id", DataType::Int64, false),
        Field::new("payload", DataType::Utf8, false),
    ]))
}

/// Wide schema for projection benches: `col_count` Int64 columns.
pub fn wide_schema(col_count: usize) -> SchemaRef {
    let fields: Vec<Field> = (0..col_count)
        .map(|i| Field::new(format!("col_{i}"), DataType::Int64, false))
        .collect();
    Arc::new(Schema::new(fields))
}

/// Build a user-fields-only RecordBatch (`id`, `payload`) for `n` rows.
///
/// Callers supply user fields only — the write path server-stamps every system
/// column (`wyrd_event_time`, `wyrd_ingested_at`, `wyrd_batch_id`, and
/// `data_tenant_id` on SystemShared) at flush. A batch carrying any of those is
/// rejected.
pub fn make_bench_batch(n: usize) -> RecordBatch {
    let ids: Int64Array = (0..n as i64).collect();
    let payloads: StringArray = (0..n).map(|i| Some(format!("p{i}"))).collect();
    RecordBatch::try_new(simple_schema(), vec![Arc::new(ids), Arc::new(payloads)]).unwrap()
}

/// Build a user-fields-only wide RecordBatch (`col_count` Int64 columns) for `n` rows.
pub fn make_wide_batch(n: usize, col_count: usize) -> RecordBatch {
    let cols: Vec<Arc<dyn Array>> = (0..col_count)
        .map(|i| {
            let arr: Int64Array = (0..n as i64).map(|r| r + i as i64).collect();
            Arc::new(arr) as _
        })
        .collect();
    RecordBatch::try_new(wide_schema(col_count), cols).unwrap()
}
