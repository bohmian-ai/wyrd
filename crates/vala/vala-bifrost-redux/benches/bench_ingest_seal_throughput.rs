//! Benchmark: ingest + seal throughput (PR#5).
//!
//! Measures end-to-end throughput for the append → memtable → freeze → Parquet
//! encode path. This benchmark compiles but is not gated (no CI requirement).
//!
//! Run with: `cargo bench --bench bench_ingest_seal_throughput`

use arrow::array::{RecordBatch, StringArray, TimestampMicrosecondArray};
use arrow::datatypes::{DataType, Field, Schema, TimeUnit};
use chrono::NaiveDate;
use std::sync::Arc;
use wyrd_spec::ids::DataTenantId;

use vala_bifrost_redux::scribe::memtable::Memtable;
use vala_bifrost_redux::scribe::parquet_writer::write_frozen_to_parquet;
use vala_bifrost_redux::scribe::seal_key::{EventDay, SealKey, TableRef};
use vala_bifrost_redux::scribe::wal::{ScribeAppendMeta, WalLsn};
use wyrd_spec::vala::api::AuditEvent;

fn build_test_batch(rows: usize) -> RecordBatch {
    let schema = Arc::new(Schema::new(vec![
        Field::new("data_tenant_id", DataType::Utf8, false),
        Field::new(
            "wyrd_event_time",
            DataType::Timestamp(TimeUnit::Microsecond, None),
            false,
        ),
    ]));

    // Generate tenant UUIDs once to reuse across rows
    let tenants: Vec<String> = (0..10)
        .map(|_| DataTenantId::new_v7().to_string())
        .collect();
    let tenant_ids: Vec<&str> = (0..rows).map(|i| tenants[i % 10].as_str()).collect();
    let timestamps: Vec<i64> = (0..rows).map(|i| i as i64 * 1_000_000).collect();

    RecordBatch::try_new(
        schema,
        vec![
            Arc::new(StringArray::from(tenant_ids)),
            Arc::new(TimestampMicrosecondArray::from(timestamps)),
        ],
    )
    .unwrap()
}

fn main() {
    // Benchmark configuration
    const BATCH_SIZE: usize = 50_000;
    const ITERATIONS: usize = 10;

    let seal_key = SealKey::new(
        DataTenantId::SYSTEM_OWNER,
        TableRef::new("vala.bifrost".to_string(), "bench".to_string()),
        EventDay::new(NaiveDate::from_ymd_opt(2026, 7, 14).unwrap()),
    );

    let memtable = Memtable::new();
    let batch = build_test_batch(BATCH_SIZE);

    let event = AuditEvent {
        request_id: wyrd_spec::request_id::RequestId::now_v7(),
        trace_id: None,
        operation: "bench".to_string(),
        resource: "bench".to_string(),
        card_ref: None,
        principal_id: wyrd_spec::auth::PrincipalId::new(uuid::Uuid::now_v7()),
        principal_kind: wyrd_spec::auth::PrincipalKindTag::Service,
        auth_method: wyrd_spec::vala::api::AuthMethod::Jwt,
        permission: "bench".to_string(),
        decision: wyrd_spec::vala::api::AuditDecision::Allow,
        result: wyrd_spec::vala::api::AuditResult::Success,
        payload_summary: "bench".to_string(),
        detail: None,
    };

    let meta = ScribeAppendMeta {
        batch_id: [0u8; 16],
        rows_accepted: BATCH_SIZE,
        wal_lsn_min: WalLsn::new(0),
        wal_lsn_max: WalLsn::new(0),
        seal_key: seal_key.as_path_components(),
    };

    println!("Benchmark: ingest + seal throughput");
    println!("  Batch size: {} rows", BATCH_SIZE);
    println!("  Iterations: {}", ITERATIONS);
    println!();

    // Warmup
    for _ in 0..3 {
        memtable
            .insert(&seal_key, event.clone(), meta.clone(), batch.clone())
            .unwrap();
        let frozen = memtable.freeze(&seal_key).unwrap();
        let _ = write_frozen_to_parquet(&frozen).unwrap();
    }

    // Actual benchmark
    let start = std::time::Instant::now();
    for _ in 0..ITERATIONS {
        memtable
            .insert(&seal_key, event.clone(), meta.clone(), batch.clone())
            .unwrap();
        let frozen = memtable.freeze(&seal_key).unwrap();
        let encoded = write_frozen_to_parquet(&frozen).unwrap();
        let _ = encoded.bytes.len(); // Prevent optimizer from removing encoding
    }
    let elapsed = start.elapsed();

    let total_rows = BATCH_SIZE * ITERATIONS;
    let rows_per_sec = (total_rows as f64) / elapsed.as_secs_f64();
    let avg_latency = elapsed / ITERATIONS as u32;

    println!("Results:");
    println!("  Total time: {:?}", elapsed);
    println!("  Avg latency per seal: {:?}", avg_latency);
    println!("  Throughput: {:.0} rows/sec", rows_per_sec);
    println!("  Total rows: {}", total_rows);
}
