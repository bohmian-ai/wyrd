//! Benchmark: ingest→ack latency (append → fsync → ack).
//!
//! Measures end-to-end latency from `Scribe::append` call to `AppendAck` return.
//! Not CI-gated; compile-only verification.

use std::sync::Arc;
use std::time::Instant;

use arrow::array::{RecordBatch, TimestampMicrosecondArray, UInt64Array};
use arrow::datatypes::{DataType, Field, Schema, TimeUnit};
use vala_bifrost_redux::contracts::{Scribe, ScribeAppend};
use vala_bifrost_redux::scribe::ScribeImpl;
use vala_bifrost_redux::scribe::wal::WalWriter;
use wyrd_runtime::{Principal, PrincipalKind, permission::PermissionSet};
use wyrd_spec::DataTenantId;
use wyrd_spec::auth::PrincipalId;

fn stub_scribe() -> ScribeImpl {
    let operator = Arc::new(
        opendal::Operator::new(opendal::services::Memory::default())
            .expect("memory backend")
            .finish(),
    );
    let temp = tempfile::tempdir().expect("temp WAL dir");
    let node_id_bytes = *uuid::Uuid::nil().as_bytes();
    let wal = Arc::new(
        WalWriter::new(
            temp.path(),
            node_id_bytes,
            1,
            DataTenantId::SYSTEM_OWNER,
            None,
        )
        .expect("WAL writer"),
    );
    // Leak the tempdir so it outlives the bench binary; benches never clean up.
    std::mem::forget(temp);
    ScribeImpl::new_with_deps(operator, wal, uuid::Uuid::nil().to_string(), 1)
}

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
        .map(|i| base_time + (i64::try_from(i).expect("bounded row index") * 1000))
        .collect();
    let values: Vec<u64> = (0..row_count)
        .map(|i| u64::try_from(i).expect("bounded row index"))
        .collect();

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

    let scribe = stub_scribe();
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

    println!("Append latency: {latency:?}");
}
