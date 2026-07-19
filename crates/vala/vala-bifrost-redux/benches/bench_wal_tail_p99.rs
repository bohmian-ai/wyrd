//! Bounded live-tail latency benchmark.

use std::sync::Arc;
use std::time::{Duration, Instant};

use arrow::array::{Int64Array, RecordBatch, StringArray, TimestampMicrosecondArray};
use arrow::datatypes::{DataType, Field, Schema, TimeUnit};
use chrono::NaiveDate;
use vala_bifrost_redux::catalog::{TableRef, TenantTableBinding};
use vala_bifrost_redux::namespaces::BifrostNamespace;
use vala_bifrost_redux::scribe::memtable::Memtable;
use vala_bifrost_redux::scribe::seal_key::{EventDay, SealKey};
use vala_bifrost_redux::scribe::stream_identity::{NodeId, StreamIdentity, WriterEpoch};
use vala_bifrost_redux::scribe::tail_rpc::{
    FetchLiveTailRequest, FetchLiveTailService, LiveTailShard,
};
use vala_bifrost_redux::scribe::wal::{ScribeAppendMeta, WalLsn};
use wyrd_spec::DataTenantId;
use wyrd_spec::vala::api::{AuditDecision, AuditEvent, AuditResult, AuthMethod};

fn event() -> AuditEvent {
    AuditEvent {
        request_id: wyrd_spec::request_id::RequestId::now_v7(),
        trace_id: None,
        operation: "bench.tail".to_owned(),
        resource: "vala.bifrost.events".to_owned(),
        card_ref: None,
        principal_id: wyrd_spec::auth::PrincipalId::new(uuid::Uuid::now_v7()),
        principal_kind: wyrd_spec::auth::PrincipalKindTag::Service,
        auth_method: AuthMethod::Jwt,
        permission: "bench".to_owned(),
        decision: AuditDecision::Allow,
        result: AuditResult::Success,
        payload_summary: "bench".to_owned(),
        detail: None,
    }
}

fn main() {
    const APPENDS: u64 = 64;
    const ITERATIONS: usize = 100;
    const P99_SLO: Duration = Duration::from_millis(500);

    let tenant = DataTenantId::SYSTEM_OWNER;
    let table = TableRef::new(BifrostNamespace::Bifrost, "tail_bench");
    let binding = TenantTableBinding::resolve((tenant, table.clone())).expect("binding");
    let shard = LiveTailShard {
        tenant_table: binding,
        table: table.clone(),
        tenant,
    };
    let stream = StreamIdentity::new(NodeId::new(uuid::Uuid::now_v7()), WriterEpoch::new(1));
    let memtable = Arc::new(Memtable::new());
    let schema = Arc::new(Schema::new(vec![
        Field::new("data_tenant_id", DataType::Utf8, false),
        Field::new(
            "wyrd_event_time",
            DataType::Timestamp(TimeUnit::Microsecond, None),
            false,
        ),
        Field::new("value", DataType::Int64, false),
    ]));
    for lsn in 1..=APPENDS {
        let key = SealKey::new(
            tenant,
            table.clone(),
            EventDay::new(NaiveDate::from_ymd_opt(2026, 7, 14).expect("date")),
        );
        let batch = RecordBatch::try_new(
            schema.clone(),
            vec![
                Arc::new(StringArray::from(vec![tenant.to_string()])),
                Arc::new(TimestampMicrosecondArray::from(vec![1_000_000i64])),
                Arc::new(Int64Array::from(vec![i64::try_from(lsn).expect("lsn")])),
            ],
        )
        .expect("batch");
        memtable
            .insert(
                &key,
                event(),
                ScribeAppendMeta {
                    batch_id: [u8::try_from(lsn).expect("batch id"); 16],
                    rows_accepted: 1,
                    wal_lsn_min: WalLsn::new(lsn),
                    wal_lsn_max: WalLsn::new(lsn),
                    seal_key: key.as_path_components(),
                },
                batch,
            )
            .expect("insert");
    }

    let runtime = tokio::runtime::Runtime::new().expect("runtime");
    let service = FetchLiveTailService::new(stream, memtable);
    let mut samples = Vec::with_capacity(ITERATIONS);
    for _ in 0..ITERATIONS {
        let start = Instant::now();
        runtime
            .block_on(service.fetch_live_tail(FetchLiveTailRequest {
                shard: shard.clone(),
                target_stream: stream,
                after_lsn: WalLsn::new(0),
            }))
            .expect("tail");
        samples.push(start.elapsed());
    }
    samples.sort_unstable();
    let p99 = samples[(samples.len() * 99 / 100).min(samples.len() - 1)];
    println!("bench_wal_tail_p99={p99:?}");
    assert!(p99 <= P99_SLO, "tail p99 {p99:?} exceeded {P99_SLO:?}");
}
