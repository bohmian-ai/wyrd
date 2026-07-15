//! Benchmark: ingest→ack latency (append → fsync → ack).
//!
//! Measures end-to-end latency from Scribe::append call to AppendAck return.
//! Not CI-gated; compile-only verification.

use std::sync::Arc;
use std::time::Instant;

use arrow::array::{RecordBatch, TimestampMicrosecondArray, UInt64Array};
use arrow::datatypes::{DataType, Field, Schema, TimeUnit};
use vala_bifrost_redux::contracts::{Scribe, ScribeAppend};
use vala_bifrost_redux::scribe::ScribeImpl;
use wyrd_runtime::{Principal, PrincipalKind, permission::PermissionSet};
use wyrd_spec::DataTenantId;
use wyrd_spec::auth::PrincipalId;

fn make_batch(row_count: usize) -> RecordBatch {
    let schema = Arc::new(Schema::new(vec![
        Field::new(
            "wyrd_event_time",
            DataType::Timestamp(TimeUnit::Microsecond, None),
            false,
        ),
        Field::new("value", DataType::UInt64, false),
    ]));

    let base_time = chrono::Utc::now().timestamp_micros();
    let timestamps: Vec<i64> = (0..row_count)
        .map(|i| base_time + (i as i64 * 1000))
        .collect();
    let values: Vec<u64> = (0..row_count).map(|i| i as u64).collect();

    RecordBatch::try_new(
        schema.clone(),
        vec![
            Arc::new(TimestampMicrosecondArray::from(timestamps)),
            Arc::new(UInt64Array::from(values)),
        ],
    )
    .expect("batch")
}

fn encode_batch(batch: &RecordBatch) -> Vec<u8> {
    use arrow::ipc::writer::StreamWriter;
    let mut buf = Vec::new();
    {
        let mut writer = StreamWriter::try_new(&mut buf, &batch.schema()).expect("writer");
        writer.write(batch).expect("write");
        writer.finish().expect("finish");
    }
    buf
}

#[tokio::main]
async fn main() {
    // Compile-only benchmark. Real implementation requires:
    // - Criterion.rs integration
    // - Real WAL with fsync
    // - Statistical latency distribution (p50, p95, p99)

    let scribe = ScribeImpl::new();
    let tenant = DataTenantId::new_v7();
    let principal = Principal {
        id: PrincipalId::new(uuid::Uuid::now_v7()),
        kind: PrincipalKind::User,
        tenant_id: tenant,
        roles: vec![],
        effective_permissions: PermissionSet::new(),
    };

    let batch = make_batch(1000);
    let batch_data = encode_batch(&batch);

    let req = ScribeAppend {
        table_fqn: "vala.events".to_string(),
        schema_fingerprint: [0u8; 32],
        batch_data,
        principal,
    };

    let start = Instant::now();
    let _ack = scribe.append(req).await.expect("append");
    let latency = start.elapsed();

    println!("Append latency: {:?}", latency);
}
