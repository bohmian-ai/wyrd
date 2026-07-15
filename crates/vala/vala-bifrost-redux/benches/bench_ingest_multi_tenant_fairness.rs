//! Benchmark: multi-tenant fairness under concurrent load.
//!
//! Measures fairness of ack latency distribution across N tenants under
//! concurrent append load. Verifies no tenant is starved.
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
    // - Concurrent append load from N tenants
    // - Per-tenant latency histograms
    // - Fairness metric (e.g., max(p95_latency) / min(p95_latency) < 2.0)

    let scribe = Arc::new(ScribeImpl::new());
    let tenants: Vec<DataTenantId> = (0..10).map(|_| DataTenantId::new_v7()).collect();

    let batch = make_batch(1000);
    let batch_data = encode_batch(&batch);

    for tenant in &tenants {
        let scribe = Arc::clone(&scribe);
        let tenant = *tenant;
        let batch_data = batch_data.clone();

        tokio::spawn(async move {
            let principal = Principal {
                id: PrincipalId::new(uuid::Uuid::now_v7()),
                kind: PrincipalKind::User,
                tenant_id: tenant,
                roles: vec![],
                effective_permissions: PermissionSet::new(),
            };

            let req = ScribeAppend {
                table_fqn: "vala.events".to_string(),
                schema_fingerprint: [0u8; 32],
                batch_data,
                principal,
            };

            let start = Instant::now();
            let _ack = scribe.append(req).await.expect("append");
            let latency = start.elapsed();

            println!("Tenant {:?} append latency: {:?}", tenant, latency);
        });
    }

    // Wait for all tenants to complete (stub)
    tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
}
