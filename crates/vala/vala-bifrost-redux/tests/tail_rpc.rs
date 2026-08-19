use std::sync::Arc;
use std::time::{Duration as StdDuration, Instant};

use arrow::array::{Int32Array, Int64Array, RecordBatch, StringArray, TimestampMicrosecondArray};
use arrow::datatypes::{DataType, Field, Schema, TimeUnit};
use chrono::{Duration, NaiveDate, Utc};
use uuid::Uuid;
use vala_bifrost_redux::catalog::{TableRef, TenantTableBinding};
use vala_bifrost_redux::namespaces::BifrostNamespace;
use vala_bifrost_redux::schema::fingerprint::SchemaFingerprint;
use vala_bifrost_redux::scribe::file_list_writer::FileListCommitKey;
use vala_bifrost_redux::scribe::memtable::Memtable;
use vala_bifrost_redux::scribe::seal_key::{EventDay, SealKey};
use vala_bifrost_redux::scribe::stream_identity::{NodeId, StreamIdentity, WriterEpoch};
use vala_bifrost_redux::scribe::tail_rpc::{
    FetchLiveTailService, LocalTailPage, ScribeTailReader, TailFenceConfig, TailReadError,
};
use vala_bifrost_redux::scribe::wal::{ScribeAppendMeta, WalLsn};
use wyrd_spec::DataTenantId;
use wyrd_spec::vala::api::{
    AcquireTailFenceRequest, AuditDecision, AuditEvent, AuditResult, AuthMethod,
    EventDay as WireEventDay, SchemaFingerprint as WireSchemaFingerprint, TailCursor,
    TailPageRequest, TenantTableBinding as WireBinding,
};

/// Returns the production-shaped root capability used by direct tail fixtures.
fn tail_resources() -> vala_bifrost_redux::resources::ScribeResources {
    vala_bifrost_redux::scribe::tail_resources_for_test()
}

fn batch(tenant: DataTenantId, value: i64) -> RecordBatch {
    let schema = Arc::new(Schema::new(vec![
        Field::new("data_tenant_id", DataType::Utf8, false),
        Field::new(
            "wyrd_event_time",
            DataType::Timestamp(TimeUnit::Microsecond, None),
            false,
        ),
        Field::new("wyrd_row_ordinal", DataType::Int32, false),
        Field::new("value", DataType::Int64, false),
    ]));
    RecordBatch::try_new(
        schema,
        vec![
            Arc::new(StringArray::from(vec![tenant.to_string()])),
            Arc::new(TimestampMicrosecondArray::from(vec![1_000_000i64])),
            Arc::new(Int32Array::from(vec![0])),
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

/// Builds one typed private-fence request matching the in-memory tail fixture.
fn fence_request(binding: &TenantTableBinding, stream: StreamIdentity) -> AcquireTailFenceRequest {
    let expected = SchemaFingerprint::from_arrow_schema(&Schema::new(vec![Field::new(
        "value",
        DataType::Int64,
        false,
    )]));
    AcquireTailFenceRequest {
        query_id: Uuid::nil(),
        binding: WireBinding {
            tenant_id: binding.tenant,
            namespace: "bifrost".to_owned(),
            table: binding.table_ref.name.clone(),
        },
        event_day: WireEventDay::new("2026-07-14").expect("fixture event day"),
        exclusive_sealed: TailCursor {
            writer_epoch: u64::try_from(stream.writer_epoch.as_i64()).expect("positive epoch"),
            wal_lsn: 0,
            batch_id: Uuid::nil(),
            row_ordinal: 0,
        },
        deadline: Utc::now() + Duration::seconds(5),
        schema_fingerprint: WireSchemaFingerprint::new(hex::encode(expected.0))
            .expect("fixture fingerprint"),
        tail_protocol_version: 1,
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
                schema_fingerprint: [0; 32],
                data_digest: [0; 32],
                data_len: 0,
                payload_digest: [0; 32],
                payload_len: 0,
                slice_index: 0,
                slice_count: 1,
                rows_accepted: 1,
                wal_lsn_min: WalLsn::new(lsn),
                wal_lsn_max: WalLsn::new(lsn),
                seal_key: key.as_path_components(),
            },
            batch(tenant, value),
        )
        .expect("append");
}

/// Extracts logical values from shallow one-row tail batches for lifecycle assertions.
fn page_values(page: &LocalTailPage) -> Vec<i64> {
    page.batches
        .iter()
        .flat_map(|batch| {
            batch
                .column_by_name("value")
                .expect("tail page retains the logical value column")
                .as_any()
                .downcast_ref::<Int64Array>()
                .expect("tail value column remains Int64")
                .values()
                .iter()
                .copied()
                .collect::<Vec<_>>()
        })
        .collect()
}

/// Returns row-precise pages that resume strictly after the last cursor.
#[tokio::test]
async fn page_continuation_is_row_precise() {
    let (memtable, stream, tenant, table, binding) = setup();
    append(&memtable, tenant, &table, 1, 1);
    append(&memtable, tenant, &table, 2, 2);
    let reader = ScribeTailReader::new(
        Arc::new(FetchLiveTailService::new(
            stream,
            memtable,
            tail_resources(),
        )),
        TailFenceConfig::default(),
    );
    let fence = reader
        .acquire_fence(fence_request(&binding, stream))
        .await
        .expect("acquires metadata only");
    let first = reader
        .read_page(&TailPageRequest {
            query_id: uuid::Uuid::nil(),
            fence_id: fence.fence_id,
            after: None,
            max_rows: 1,
            max_encoded_bytes: 1024 * 1024,
        })
        .expect("first row page");
    assert_eq!(first.batches.len(), 1);
    assert!(!first.complete);
    let second = reader
        .read_page(&TailPageRequest {
            query_id: uuid::Uuid::nil(),
            fence_id: fence.fence_id,
            after: first.next,
            max_rows: 1,
            max_encoded_bytes: 1024 * 1024,
        })
        .expect("second row page");
    assert_eq!(second.batches.len(), 1);
    assert!(second.complete);
}

/// Refuses a full registry without reserving capacity for the rejected fence.
#[tokio::test]
async fn capacity_rejection_leaves_no_fence() {
    let (memtable, stream, tenant, table, binding) = setup();
    append(&memtable, tenant, &table, 1, 1);
    let reader = ScribeTailReader::new(
        Arc::new(FetchLiveTailService::new(
            stream,
            memtable,
            tail_resources(),
        )),
        TailFenceConfig {
            max_fences: 1,
            ..TailFenceConfig::default()
        },
    );
    let first = reader
        .acquire_fence(fence_request(&binding, stream))
        .await
        .expect("first fence reserves capacity");
    let error = reader
        .acquire_fence(fence_request(&binding, stream))
        .await
        .expect_err("second fence exceeds the configured capacity");
    assert!(matches!(
        error,
        vala_bifrost_redux::scribe::tail_rpc::TailReadError::Capacity
    ));
    assert!(
        reader
            .release_fence(first.fence_id)
            .expect("release succeeds")
            .released
    );
    reader
        .acquire_fence(fence_request(&binding, stream))
        .await
        .expect("rejected acquisition left no partial capacity charge");
}

/// Reclaims expiry once and makes a racing idempotent release a harmless no-op.
#[tokio::test]
async fn release_and_expiry_race_reclaims_once() {
    let (memtable, stream, tenant, table, binding) = setup();
    append(&memtable, tenant, &table, 1, 1);
    let resources = tail_resources();
    let baseline = resources.snapshot().expect("baseline Scribe root snapshot");
    let reader = ScribeTailReader::new(
        Arc::new(FetchLiveTailService::new(
            stream,
            memtable,
            resources.clone(),
        )),
        TailFenceConfig {
            ttl: StdDuration::from_millis(1),
            ..TailFenceConfig::default()
        },
    );
    let fence = reader
        .acquire_fence(fence_request(&binding, stream))
        .await
        .expect("fence acquires");
    assert!(
        resources
            .snapshot()
            .expect("retained-fence root snapshot")
            .scribe_memory_used_bytes
            > baseline.scribe_memory_used_bytes
    );
    let expiry = reader.expire_due(Instant::now() + StdDuration::from_secs(1), 1);
    assert_eq!(expiry.released, 1);
    assert_eq!(
        resources
            .snapshot()
            .expect("expired-fence root snapshot")
            .scribe_memory_used_bytes,
        baseline.scribe_memory_used_bytes
    );
    assert!(
        !reader
            .release_fence(fence.fence_id)
            .expect("racing release is idempotent")
            .released
    );
}

/// Rejects one encoded row that cannot fit without returning a partial page.
#[tokio::test]
async fn single_oversize_row_fails_empty() {
    let (memtable, stream, tenant, table, binding) = setup();
    append(&memtable, tenant, &table, 1, 1);
    let reader = ScribeTailReader::new(
        Arc::new(FetchLiveTailService::new(
            stream,
            memtable,
            tail_resources(),
        )),
        TailFenceConfig {
            max_page_encoded_bytes: 1,
            ..TailFenceConfig::default()
        },
    );
    let fence = reader
        .acquire_fence(fence_request(&binding, stream))
        .await
        .expect("fence acquires before paging");
    let error = reader
        .read_page(&TailPageRequest {
            query_id: uuid::Uuid::nil(),
            fence_id: fence.fence_id,
            after: None,
            max_rows: 1,
            max_encoded_bytes: u32::MAX,
        })
        .expect_err("one row cannot fit the configured encoded-byte ceiling");
    assert!(matches!(error, TailReadError::OversizeRow));
    assert!(
        reader
            .release_fence(fence.fence_id)
            .expect("oversize failure leaves the fence releasable")
            .released
    );
}

/// Preserves one acquired interval while its source generation seals, rotates, and retires.
#[tokio::test]
async fn seal_and_rotation_preserve_fence() {
    let (memtable, stream, tenant, table, binding) = setup();
    append(&memtable, tenant, &table, 1, 1);
    append(&memtable, tenant, &table, 2, 2);
    let resources = tail_resources();
    let baseline = resources.snapshot().expect("baseline Scribe root snapshot");
    let reader = ScribeTailReader::new(
        Arc::new(FetchLiveTailService::new(
            stream,
            Arc::clone(&memtable),
            resources.clone(),
        )),
        TailFenceConfig::default(),
    );
    let fence = reader
        .acquire_fence(fence_request(&binding, stream))
        .await
        .expect("fence freezes the writable interval");
    let key = SealKey::new(
        tenant,
        table.clone(),
        EventDay::new(NaiveDate::from_ymd_opt(2026, 7, 14).expect("date")),
    );
    let frozen = memtable.freeze(&key).expect("active generation seals");
    let first = reader
        .read_page(&TailPageRequest {
            query_id: uuid::Uuid::nil(),
            fence_id: fence.fence_id,
            after: None,
            max_rows: 1,
            max_encoded_bytes: 1024 * 1024,
        })
        .expect("first retained row remains readable after seal");
    assert_eq!(page_values(&first), vec![1]);
    assert!(!first.complete);

    append(&memtable, tenant, &table, 3, 3);
    memtable
        .complete_post_commit(
            frozen.seal_id,
            FileListCommitKey {
                data_tenant_id: tenant,
                namespace: "vala.bifrost".to_owned(),
                table_name: table.name.clone(),
                node_id: stream.node_id.as_uuid(),
                writer_epoch: stream.writer_epoch.as_i64(),
                wal_lsn_min: 1,
                wal_lsn_max: 2,
            },
        )
        .expect("sealed generation commits");
    // Retirement is immediate: the committed generation retires on the first sweep.
    let retired = memtable
        .sweep_once()
        .expect("committed generation retires from the memtable");
    assert_eq!(retired.len(), 1);
    let retained = resources
        .snapshot()
        .expect("retained-fence root snapshot after source retirement");
    assert!(
        retained.scribe_memory_used_bytes > baseline.scribe_memory_used_bytes,
        "source retirement cannot make the fence-held Arrow handles ownerless"
    );

    let second = reader
        .read_page(&TailPageRequest {
            query_id: uuid::Uuid::nil(),
            fence_id: fence.fence_id,
            after: first.next,
            max_rows: 1,
            max_encoded_bytes: 1024 * 1024,
        })
        .expect("continuation retains the sealed generation after retirement");
    assert_eq!(page_values(&second), vec![2]);
    assert!(second.complete);
    assert_eq!(second.next, Some(fence.inclusive_live));
    assert!(
        reader
            .release_fence(fence.fence_id)
            .expect("release reclaims retained shallow handles")
            .released
    );
    assert_eq!(
        resources
            .snapshot()
            .expect("released-fence root snapshot")
            .scribe_memory_used_bytes,
        baseline.scribe_memory_used_bytes
    );
}
