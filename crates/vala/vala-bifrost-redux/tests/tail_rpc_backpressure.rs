use std::sync::Arc;

use arrow::array::{Int64Array, RecordBatch, StringArray, TimestampMicrosecondArray};
use arrow::datatypes::{DataType, Field, Schema, TimeUnit};
use chrono::NaiveDate;
use uuid::Uuid;
use vala_bifrost_redux::catalog::{TableRef, TenantTableBinding};
use vala_bifrost_redux::namespaces::BifrostNamespace;
use vala_bifrost_redux::scribe::memtable::Memtable;
use vala_bifrost_redux::scribe::seal_key::{EventDay, SealKey};
use vala_bifrost_redux::scribe::stream_identity::{NodeId, StreamIdentity, WriterEpoch};
use vala_bifrost_redux::scribe::tail_rpc::{
    FetchLiveTailRequest, FetchLiveTailService, TailConfig, TailFrame,
};
use vala_bifrost_redux::scribe::wal::{ScribeAppendMeta, WalLsn};
use wyrd_spec::DataTenantId;
use wyrd_spec::vala::api::{AuditDecision, AuditEvent, AuditResult, AuthMethod};

fn event() -> AuditEvent {
    AuditEvent {
        request_id: wyrd_spec::request_id::RequestId::now_v7(),
        trace_id: None,
        operation: "tail.test".to_owned(),
        resource: "vala.bifrost.events".to_owned(),
        card_ref: None,
        principal_id: wyrd_spec::auth::PrincipalId::new(Uuid::now_v7()),
        principal_kind: wyrd_spec::auth::PrincipalKindTag::Service,
        auth_method: AuthMethod::Jwt,
        permission: "tail.test".to_owned(),
        decision: AuditDecision::Allow,
        result: AuditResult::Success,
        payload_summary: "tail".to_owned(),
        detail: None,
    }
}

fn append(memtable: &Memtable, tenant: DataTenantId, table: &TableRef, lsn: u64) {
    let key = SealKey::new(
        tenant,
        table.clone(),
        EventDay::new(NaiveDate::from_ymd_opt(2026, 7, 14).expect("date")),
    );
    let schema = Arc::new(Schema::new(vec![
        Field::new("data_tenant_id", DataType::Utf8, false),
        Field::new(
            "wyrd_event_time",
            DataType::Timestamp(TimeUnit::Microsecond, None),
            false,
        ),
        Field::new("value", DataType::Int64, false),
    ]));
    let batch = RecordBatch::try_new(
        schema,
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

#[tokio::test]
async fn backpressure_emits_whole_batches_and_resume_token() {
    let tenant = DataTenantId::new_v7();
    let table = TableRef::new(BifrostNamespace::Bifrost, "events");
    let binding = TenantTableBinding::resolve((tenant, table.clone())).expect("binding");
    let stream = StreamIdentity::new(NodeId::new(Uuid::now_v7()), WriterEpoch::new(1));
    let memtable = Arc::new(Memtable::new());
    append(&memtable, tenant, &table, 1);
    append(&memtable, tenant, &table, 2);
    append(&memtable, tenant, &table, 3);

    let service = FetchLiveTailService::with_config(
        stream,
        Arc::clone(&memtable),
        TailConfig { max_bytes: 1 },
    );
    let first = service
        .fetch_live_tail(FetchLiveTailRequest {
            binding: binding.clone(),
            target_stream: stream,
            start_day: EventDay::new(NaiveDate::from_ymd_opt(2026, 7, 14).expect("date")),
            end_day: EventDay::new(NaiveDate::from_ymd_opt(2026, 7, 14).expect("date")),
            after_lsn: WalLsn::new(0),
            required_columns: Vec::new(),
        })
        .await
        .expect("first page");
    assert!(matches!(first[0], TailFrame::Batch(ref batch) if batch.lsn == WalLsn::new(1)));
    assert!(
        matches!(first[1], TailFrame::Exhausted { resume_after_lsn } if resume_after_lsn == WalLsn::new(1))
    );
    assert_eq!(
        first
            .iter()
            .filter(|frame| matches!(frame, TailFrame::Complete | TailFrame::Exhausted { .. }))
            .count(),
        1
    );

    let second = service
        .fetch_live_tail(FetchLiveTailRequest {
            binding,
            target_stream: stream,
            start_day: EventDay::new(NaiveDate::from_ymd_opt(2026, 7, 14).expect("date")),
            end_day: EventDay::new(NaiveDate::from_ymd_opt(2026, 7, 14).expect("date")),
            after_lsn: WalLsn::new(1),
            required_columns: Vec::new(),
        })
        .await
        .expect("second page");
    assert!(matches!(second[0], TailFrame::Batch(ref batch) if batch.lsn == WalLsn::new(2)));
    assert!(
        matches!(second[1], TailFrame::Exhausted { resume_after_lsn } if resume_after_lsn == WalLsn::new(2))
    );
}
