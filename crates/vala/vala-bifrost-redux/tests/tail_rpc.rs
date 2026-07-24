use std::sync::Arc;

use arrow::array::{RecordBatch, StringArray, TimestampMicrosecondArray};
use arrow::datatypes::{DataType, Field, Schema, TimeUnit};
use chrono::NaiveDate;
use uuid::Uuid;
use vala_bifrost_redux::catalog::{TableRef, TenantTableBinding};
use vala_bifrost_redux::namespaces::BifrostNamespace;
use vala_bifrost_redux::scribe::memtable::Memtable;
use vala_bifrost_redux::scribe::seal_key::{EventDay, SealKey};
use vala_bifrost_redux::scribe::stream_identity::{NodeId, StreamIdentity, WriterEpoch};
use vala_bifrost_redux::scribe::tail_rpc::{
    FetchLiveTailRequest, FetchLiveTailService, TailFrame,
};
use vala_bifrost_redux::scribe::wal::{ScribeAppendMeta, WalLsn};
use wyrd_spec::DataTenantId;
use wyrd_spec::vala::api::{AuditDecision, AuditEvent, AuditResult, AuthMethod};

fn batch(tenant: DataTenantId, value: i64) -> RecordBatch {
    let schema = Arc::new(Schema::new(vec![
        Field::new("data_tenant_id", DataType::Utf8, false),
        Field::new(
            "wyrd_event_time",
            DataType::Timestamp(TimeUnit::Microsecond, None),
            false,
        ),
        Field::new("value", DataType::Int64, false),
    ]));
    RecordBatch::try_new(
        schema,
        vec![
            Arc::new(StringArray::from(vec![tenant.to_string()])),
            Arc::new(TimestampMicrosecondArray::from(vec![1_000_000i64])),
            Arc::new(arrow::array::Int64Array::from(vec![value])),
        ],
    )
    .expect("tail test batch")
}

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

fn setup() -> (
    Arc<Memtable>,
    StreamIdentity,
    DataTenantId,
    TableRef,
    TenantTableBinding,
) {
    let tenant = DataTenantId::new_v7();
    let table = TableRef::new(BifrostNamespace::Bifrost, "events");
    let binding = TenantTableBinding::resolve((tenant, table.clone())).expect("binding");
    let stream = StreamIdentity::new(NodeId::new(Uuid::now_v7()), WriterEpoch::new(7));
    (Arc::new(Memtable::new()), stream, tenant, table, binding)
}

fn request(
    binding: TenantTableBinding,
    stream: StreamIdentity,
    after_lsn: WalLsn,
) -> FetchLiveTailRequest {
    FetchLiveTailRequest {
        binding,
        target_stream: stream,
        start_day: EventDay::new(NaiveDate::from_ymd_opt(2026, 7, 14).expect("date")),
        end_day: EventDay::new(NaiveDate::from_ymd_opt(2026, 7, 14).expect("date")),
        after_lsn,
        required_columns: Vec::new(),
    }
}

fn append(memtable: &Memtable, tenant: DataTenantId, table: &TableRef, lsn: u64, value: i64) {
    let key = SealKey::new(
        tenant,
        table.clone(),
        EventDay::new(NaiveDate::from_ymd_opt(2026, 7, 14).expect("date")),
    );
    memtable
        .insert(
            &key,
            event(),
            ScribeAppendMeta {
                batch_id: [u8::try_from(lsn).expect("test lsn"); 16],
                rows_accepted: 1,
                wal_lsn_min: WalLsn::new(lsn),
                wal_lsn_max: WalLsn::new(lsn),
                seal_key: key.as_path_components(),
            },
            batch(tenant, value),
        )
        .expect("append");
}

fn append_without_tenant(memtable: &Memtable, tenant: DataTenantId, table: &TableRef) {
    let key = SealKey::new(
        tenant,
        table.clone(),
        EventDay::new(NaiveDate::from_ymd_opt(2026, 7, 14).expect("date")),
    );
    let schema = Arc::new(Schema::new(vec![Field::new(
        "value",
        arrow::datatypes::DataType::Int64,
        false,
    )]));
    let rows = RecordBatch::try_new(
        schema,
        vec![Arc::new(arrow::array::Int64Array::from(vec![1]))],
    )
    .expect("batch");
    memtable
        .insert(
            &key,
            event(),
            ScribeAppendMeta {
                batch_id: [9; 16],
                rows_accepted: 1,
                wal_lsn_min: WalLsn::new(1),
                wal_lsn_max: WalLsn::new(1),
                seal_key: key.as_path_components(),
            },
            rows,
        )
        .expect("append");
}

#[tokio::test]
async fn tail_filters_strictly_after_lsn_and_emits_one_terminal() {
    let (memtable, stream, tenant, table, shard) = setup();
    append(&memtable, tenant, &table, 1, 1);
    append(&memtable, tenant, &table, 2, 2);
    let service = FetchLiveTailService::new(stream, memtable);

    let frames = service
        .fetch_live_tail(request(shard, stream, WalLsn::new(1)))
        .await
        .expect("tail");

    assert!(matches!(frames.first(), Some(TailFrame::Batch(batch)) if batch.lsn == WalLsn::new(2)));
    assert!(matches!(frames.last(), Some(TailFrame::Complete)));
    assert_eq!(
        frames
            .iter()
            .filter(|frame| matches!(frame, TailFrame::Complete | TailFrame::Exhausted { .. }))
            .count(),
        1
    );
}

#[tokio::test]
async fn stream_mismatch_rejects_before_memtable_read() {
    let (memtable, stream, tenant, table, shard) = setup();
    append(&memtable, tenant, &table, 1, 1);
    let requested = StreamIdentity::new(NodeId::new(Uuid::now_v7()), WriterEpoch::new(8));
    let service = FetchLiveTailService::new(stream, memtable);

    let error = service
        .fetch_live_tail(request(shard, requested, WalLsn::new(0)))
        .await
        .expect_err("mismatch");
    assert!(matches!(
        error,
        vala_bifrost_redux::contracts::ScribeError::StreamMismatch { .. }
    ));
}

#[tokio::test]
async fn tail_is_scoped_to_exact_tenant_and_table() {
    let (memtable, stream, tenant, table, shard) = setup();
    append(&memtable, tenant, &table, 1, 1);
    append(&memtable, DataTenantId::new_v7(), &table, 2, 2);
    append(
        &memtable,
        tenant,
        &TableRef::new(BifrostNamespace::Bifrost, "other"),
        3,
        3,
    );
    let service = FetchLiveTailService::new(stream, memtable);
    let frames = service
        .fetch_live_tail(request(shard, stream, WalLsn::new(0)))
        .await
        .expect("tail");
    let lsns: Vec<_> = frames
        .iter()
        .filter_map(|frame| match frame {
            TailFrame::Batch(batch) => Some(batch.lsn),
            TailFrame::Complete | TailFrame::Exhausted { .. } => None,
        })
        .collect();
    assert_eq!(lsns, vec![WalLsn::new(1)]);
}

#[tokio::test]
async fn empty_tail_has_exactly_one_complete_frame() {
    let (memtable, stream, _tenant, _table, shard) = setup();
    let frames = FetchLiveTailService::new(stream, memtable)
        .fetch_live_tail(request(shard, stream, WalLsn::new(0)))
        .await
        .expect("empty tail");
    assert_eq!(frames, vec![TailFrame::Complete]);
}

#[tokio::test]
async fn missing_data_tenant_column_is_internal_error() {
    let (memtable, stream, tenant, table, shard) = setup();
    append_without_tenant(&memtable, tenant, &table);
    let error = FetchLiveTailService::new(stream, memtable)
        .fetch_live_tail(request(shard, stream, WalLsn::new(0)))
        .await
        .expect_err("missing tenant column");
    assert!(matches!(
        error,
        vala_bifrost_redux::contracts::ScribeError::Internal { detail }
            if detail.contains("data_tenant_id")
    ));
}
