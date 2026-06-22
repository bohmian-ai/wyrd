use std::sync::Arc;

use arrow::array::{
    FixedSizeBinaryBuilder, Int64Array, StringArray, TimestampMicrosecondArray,
};
use arrow::datatypes::{DataType, Field, Schema, SchemaRef};
use arrow::record_batch::RecordBatch;
use vala_bifrost::types::TableScope;
use wyrd_spec::ids::DataTenantId;
use wyrd_spec::vala::system_columns::{DATA_TENANT_ID, WYRD_BATCH_ID, WYRD_INGESTED_AT};

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

/// Build a full Bifrost-schema RecordBatch (user columns + system columns) for `n` rows.
///
/// `event_time_us` sets `wyrd_event_time` for all rows; use distinct values across
/// days to stress partition selectivity benches.
pub fn make_bench_batch(
    n: usize,
    event_time_us: i64,
    tenant: Option<DataTenantId>,
    scope: TableScope,
) -> RecordBatch {
    let batch_id = *uuid::Uuid::now_v7().as_bytes();
    let full_schema = vala_bifrost::schema::bifrost_schema(
        vec![
            Field::new("id", DataType::Int64, false),
            Field::new("payload", DataType::Utf8, false),
        ],
        scope,
    );

    let ids: Int64Array = (0..n as i64).collect();
    let payloads: StringArray = (0..n).map(|i| Some(format!("p{i}"))).collect();
    let event_times = TimestampMicrosecondArray::from(vec![event_time_us; n])
        .with_timezone("UTC".to_string());
    let ingested = TimestampMicrosecondArray::from(vec![event_time_us; n])
        .with_timezone("UTC".to_string());

    let mut batch_id_builder = FixedSizeBinaryBuilder::with_capacity(n, 16);
    for _ in 0..n {
        batch_id_builder.append_value(batch_id).unwrap();
    }
    let batch_id_col = Arc::new(batch_id_builder.finish());

    if scope == TableScope::TenantOwned {
        RecordBatch::try_new(
            full_schema,
            vec![
                Arc::new(ids),
                Arc::new(payloads),
                Arc::new(event_times),
                Arc::new(ingested),
                batch_id_col,
            ],
        )
        .unwrap()
    } else {
        let tenant_str = tenant.expect("tenant required for SystemShared").to_string();
        let tenants: StringArray = std::iter::repeat(tenant_str.as_str()).take(n).collect();
        RecordBatch::try_new(
            full_schema,
            vec![
                Arc::new(ids),
                Arc::new(payloads),
                Arc::new(event_times),
                Arc::new(ingested),
                batch_id_col,
                Arc::new(tenants),
            ],
        )
        .unwrap()
    }
}

/// Build a wide RecordBatch for projection benches (`col_count` Int64 columns).
pub fn make_wide_batch(n: usize, col_count: usize, event_time_us: i64) -> RecordBatch {
    let batch_id = *uuid::Uuid::now_v7().as_bytes();

    let user_fields: Vec<Field> = (0..col_count)
        .map(|i| Field::new(format!("col_{i}"), DataType::Int64, false))
        .collect();
    let full_schema = vala_bifrost::schema::bifrost_schema(user_fields, TableScope::TenantOwned);

    let mut cols: Vec<Arc<dyn arrow::array::Array>> = (0..col_count)
        .map(|i| {
            let arr: Int64Array = (0..n as i64).map(|r| r + i as i64).collect();
            Arc::new(arr) as _
        })
        .collect();

    let event_times = TimestampMicrosecondArray::from(vec![event_time_us; n])
        .with_timezone("UTC".to_string());
    let ingested = TimestampMicrosecondArray::from(vec![event_time_us; n])
        .with_timezone("UTC".to_string());

    let mut batch_id_builder = FixedSizeBinaryBuilder::with_capacity(n, 16);
    for _ in 0..n {
        batch_id_builder.append_value(batch_id).unwrap();
    }

    cols.push(Arc::new(event_times));
    cols.push(Arc::new(ingested));
    cols.push(Arc::new(batch_id_builder.finish()));

    RecordBatch::try_new(full_schema, cols).unwrap()
}

/// Microseconds since Unix epoch for day `offset` (0 = 1970-01-01).
pub fn day_us(offset: i64) -> i64 {
    offset * 86_400 * 1_000_000
}
