//! WAL failure, replay, and durable-ack closure coverage.

use std::sync::Arc;

use crate::catalog::TableRef;
use crate::contracts::{
    FrameAdmission, IngressPayload, Scribe, ScribeError, ScribeIngressFrame,
    projected_source_schema_fingerprint,
};
use crate::namespaces::BifrostNamespace;
use crate::scribe::admission::EventTimeWindow;
use crate::scribe::audit_envelope::encode_audit_event;
use crate::scribe::memory::MemoryCategory;
use crate::scribe::replay::replay_wal_directory;
use crate::scribe::seal_key::SealKey;
use crate::scribe::stream_identity::NodeId;
use crate::scribe::wal::{SegmentHeader, WalConfig, WalWriter};
use arrow::array::{Int64Array, TimestampMicrosecondArray};
use arrow::datatypes::{DataType, Field, Schema, TimeUnit};
use arrow::record_batch::RecordBatch;
use chrono::Datelike as _;
use opendal::services::Memory;
use tempfile::TempDir;
use uuid::Uuid;
use wyrd_runtime::{PermissionSet, Principal, PrincipalKind};
use wyrd_spec::auth::{PrincipalId, PrincipalKindTag};
use wyrd_spec::ids::DataTenantId;
use wyrd_spec::request_id::RequestId;
use wyrd_spec::vala::api::{AuditDecision, AuditEvent, AuditResult, AuthMethod};

/// Build one deterministic WAL closeout scope.
fn key(tenant: DataTenantId, table: &str) -> SealKey {
    SealKey::new(
        tenant,
        TableRef::new(BifrostNamespace::Bifrost, table),
        fixture_event_partition(),
    )
}

/// The one admitted event-time instant every closeout fixture shares.
///
/// Derived from the production admission window rather than pinned, so the
/// batch this module builds stays inside `[receipt - past, receipt + future]`
/// at any wall-clock time. A pinned literal here ages out of the window and
/// turns every appending test in this file into a permanent failure.
fn fixture_event_time_micros() -> i64 {
    EventTimeWindow::default().admitted_event_time_micros(1)
}

/// The exact daily partition covering [`fixture_event_time_micros`].
///
/// Both come from the same instant, so a `SealKey` built here cannot drift
/// onto a different partition than the row it seals. Built through the shared
/// [`crate::partition_fixtures::day_partition`] constructor so this module
/// never derives its own partition boundary.
///
/// # Panics
/// Panics when the derived instant is not representable as a UTC date.
fn fixture_event_partition() -> crate::catalog::layout::TimePartition {
    let date = chrono::DateTime::from_timestamp_micros(fixture_event_time_micros())
        .expect("derived event time must be representable")
        .date_naive();
    crate::partition_fixtures::day_partition(date.year(), date.month(), date.day())
}

/// Encode the fixed closeout audit payload.
///
/// # Panics
/// Panics when the static audit event cannot be encoded.
fn audit() -> Vec<u8> {
    encode_audit_event(&AuditEvent {
        request_id: RequestId::now_v7(),
        trace_id: None,
        operation: "scribe.wal".to_owned(),
        resource: "vala.bifrost.scribe_wal".to_owned(),
        card_ref: None,
        principal_id: PrincipalId::new(Uuid::now_v7()),
        principal_kind: PrincipalKindTag::User,
        auth_method: AuthMethod::Jwt,
        permission: "bifrost:write".to_owned(),
        decision: AuditDecision::Allow,
        result: AuditResult::Success,
        payload_summary: "1 rows".to_owned(),
        detail: None,
    })
    .expect("audit")
}

/// Build the server-created audit event that Gate owns for a production frame.
///
/// Closeout fixtures pass through `Scribe::ingest_frame`, which never mints its
/// own audit record, so each fixture supplies the allow/success event a real
/// authenticated write would carry.
fn frame_audit_event(principal: &Principal, table: &str, request_id: &RequestId) -> AuditEvent {
    AuditEvent {
        request_id: request_id.clone(),
        trace_id: None,
        operation: "bifrost.append".to_owned(),
        resource: format!("vala.bifrost.{table}"),
        card_ref: principal.card_ref().cloned(),
        principal_id: principal.id,
        principal_kind: principal.kind.tag(),
        auth_method: AuthMethod::Jwt,
        permission: "bifrost:append".to_owned(),
        decision: AuditDecision::Allow,
        result: AuditResult::Success,
        payload_summary: "1 rows".to_owned(),
        detail: None,
    }
}

/// Build the fixed one-row closeout batch.
///
/// # Panics
/// Panics when static date or Arrow fixture construction fails.
fn batch() -> RecordBatch {
    batch_with_value(7)
}

/// Build the fixed one-row batch with a caller-selected payload value.
fn batch_with_value(value: i64) -> RecordBatch {
    RecordBatch::try_new(
        Arc::new(Schema::new(vec![
            Field::new(
                "wyrd_event_time",
                DataType::Timestamp(TimeUnit::Microsecond, Some("UTC".into())),
                false,
            ),
            Field::new("value", DataType::Int64, false),
        ])),
        vec![
            Arc::new(
                TimestampMicrosecondArray::from(vec![fixture_event_time_micros()])
                    .with_timezone("UTC"),
            ),
            Arc::new(Int64Array::from(vec![value])),
        ],
    )
    .expect("batch")
}

/// Build the tenant principal used by durable append cases.
fn principal(tenant: DataTenantId) -> Principal {
    Principal {
        id: PrincipalId::new(Uuid::now_v7()),
        kind: PrincipalKind::User,
        tenant_id: tenant,
        roles: Vec::new(),
        effective_permissions: PermissionSet::new(),
    }
}

/// Construct a Scribe and WAL sharing one explicit owner root.
///
/// # Panics
/// Panics when the local WAL or memory object store cannot be built.
fn scribe(wal_root: &TempDir, node: NodeId) -> (Arc<WalWriter>, crate::scribe::ScribeImpl) {
    let wal = Arc::new(
        WalWriter::new(wal_root.path(), *node.as_bytes(), 1, WalConfig::default())
            .expect("WAL writer"),
    );
    let operator = Arc::new(
        opendal::Operator::new(Memory::default())
            .expect("memory object store")
            .finish(),
    );
    let scribe = crate::scribe::ScribeImpl::new_for_embedded_with_deps(
        operator,
        Arc::clone(&wal),
        &node.to_string(),
        1,
    );
    (wal, scribe)
}

/// Ingest one fixed batch through Scribe's durable production seam.
///
/// # Errors
/// Returns the exact Scribe ingress error from the durable owner.
async fn ingest(
    scribe: &crate::scribe::ScribeImpl,
    tenant: DataTenantId,
    table: &str,
    batch_id: Uuid,
) -> Result<FrameAdmission, ScribeError> {
    ingest_as(scribe, principal(tenant), table, batch_id).await
}

/// Ingest one fixed batch as a retained authenticated principal.
///
/// # Errors
/// Returns the exact Scribe ingress error from the durable owner.
async fn ingest_as(
    scribe: &crate::scribe::ScribeImpl,
    principal: Principal,
    table: &str,
    batch_id: Uuid,
) -> Result<FrameAdmission, ScribeError> {
    let rows = batch();
    let request_id = RequestId::now_v7();
    Scribe::ingest_frame(
        scribe,
        ScribeIngressFrame {
            authenticated_tenant: principal.tenant_id,
            audit_event: frame_audit_event(&principal, table, &request_id),
            principal,
            table: TableRef::new(BifrostNamespace::Bifrost, table),
            expected_schema_fingerprint: Some(projected_source_schema_fingerprint(
                rows.schema().as_ref(),
            )),
            request_id,
            batch_id,
            measured_wire_bytes: 0,
            payload: IngressPayload::Canonical(crate::contracts::CanonicalIngress::unreserved(vec![rows])),
        },
    )
    .await
}

/// Ingest a payload variant through the production durable seam.
///
/// # Errors
/// Returns the exact Scribe ingress error from the durable owner.
async fn ingest_value(
    scribe: &crate::scribe::ScribeImpl,
    tenant: DataTenantId,
    table: &str,
    batch_id: Uuid,
    value: i64,
) -> Result<FrameAdmission, ScribeError> {
    let rows = batch_with_value(value);
    let principal = principal(tenant);
    let request_id = RequestId::now_v7();
    Scribe::ingest_frame(
        scribe,
        ScribeIngressFrame {
            authenticated_tenant: tenant,
            audit_event: frame_audit_event(&principal, table, &request_id),
            principal,
            table: TableRef::new(BifrostNamespace::Bifrost, table),
            expected_schema_fingerprint: Some(projected_source_schema_fingerprint(
                rows.schema().as_ref(),
            )),
            request_id,
            batch_id,
            measured_wire_bytes: 0,
            payload: IngressPayload::Canonical(crate::contracts::CanonicalIngress::unreserved(vec![rows])),
        },
    )
    .await
}

/// Ingest one fixed batch while reporting an explicit measured wire size.
///
/// # Errors
/// Returns the exact Scribe ingress error from the durable owner.
async fn ingest_measured(
    scribe: &crate::scribe::ScribeImpl,
    tenant: DataTenantId,
    table: &str,
    batch_id: Uuid,
    measured_wire_bytes: usize,
) -> Result<FrameAdmission, ScribeError> {
    let rows = batch();
    let principal = principal(tenant);
    let request_id = RequestId::now_v7();
    Scribe::ingest_frame(
        scribe,
        ScribeIngressFrame {
            authenticated_tenant: tenant,
            audit_event: frame_audit_event(&principal, table, &request_id),
            principal,
            table: TableRef::new(BifrostNamespace::Bifrost, table),
            expected_schema_fingerprint: Some(projected_source_schema_fingerprint(
                rows.schema().as_ref(),
            )),
            request_id,
            batch_id,
            measured_wire_bytes,
            payload: IngressPayload::Canonical(crate::contracts::CanonicalIngress::unreserved(vec![rows])),
        },
    )
    .await
}

/// Assert one exact Scribe rejection counter and no identity-bearing variant.
fn assert_rejection(recorder: &wyrd_bench::BenchmarkRecorder, reason: &str, count: u64) {
    let snapshot = recorder.snapshot();
    assert_eq!(
        snapshot.counters.get(&format!(
            "bifrost_scribe_rejections_total{{reason=\"{reason}\"}}"
        )),
        Some(&count)
    );
    assert_eq!(
        snapshot
            .counters
            .iter()
            .filter(|(key, _)| key.starts_with("bifrost_scribe_rejections_total{"))
            .map(|(_, value)| value)
            .sum::<u64>(),
        count,
        "one public rejection must have one terminal reason"
    );
    assert!(
        snapshot
            .gauges
            .get("bifrost_scribe_ingress_active")
            .is_some_and(|value| value.abs() <= f64::EPSILON),
        "the public ingress owner must drain after rejection"
    );
    assert!(!snapshot.counters.keys().any(|key| {
        key.starts_with("bifrost_scribe_rejections_total")
            && ["tenant", "table", "path", "request", "node", "error", "sql"]
                .iter()
                .any(|forbidden| key.contains(forbidden))
    }));
}

/// Assert every admitted ingress root has one terminal release and no live child.
fn assert_ingress_owners_settled(scribe: &crate::scribe::ScribeImpl) {
    let lifecycle = scribe
        .inspection_snapshot()
        .expect("settled ingress inspection")
        .ingress_lifecycle;
    assert_eq!(lifecycle.active_attempts, 0);
    assert_eq!(lifecycle.active_reservations, 0);
    assert_eq!(lifecycle.active_reserved_bytes, 0);
    assert_eq!(lifecycle.active_materializations, 0);
    assert_eq!(lifecycle.active_materialized_bytes, 0);
    assert_eq!(lifecycle.active_shard_transfers, 0);
    assert_eq!(lifecycle.active_shard_transferred_bytes, 0);
    assert_eq!(lifecycle.reservations, lifecycle.releases);
    assert_eq!(lifecycle.reserved_bytes, lifecycle.released_bytes);
    assert_eq!(lifecycle.shard_transfers, lifecycle.reservations);
    assert_eq!(lifecycle.shard_transferred_bytes, lifecycle.reserved_bytes);
    assert!(lifecycle.transfers <= lifecycle.materializations);
    assert!(lifecycle.transferred_bytes <= lifecycle.materialized_bytes);
}

/// Pinning ingress at its sublimit trips exactly one D84 ceiling-labelled reason.
///
/// The scenario pins the whole ingress sublimit
/// ([`crate::resources::ScribeResources::ingress_limit_bytes`]) with a
/// held [`MemoryCategory::Raw`] reservation, so the append's post-seal retry
/// cannot fit under the ingress ceiling. Per D84 that rejection is labelled by
/// the closed ceiling that tripped — here
/// [`crate::scribe::memory::ScribeRejectionCeiling::IngressSublimit`], recorded as
/// `reason="ingress_sublimit"` rather than the superseded collapsed
/// `reason="memory"`. The assertion still proves exactly one terminal rejection
/// reason, no identity-bearing labels, and a drained ingress gauge.
#[tokio::test(flavor = "current_thread")]
async fn scribe_memory_rejection_emits_exact_owner_reason() {
    let recorder = wyrd_bench::BenchmarkRecorder::default();
    let _recorder = metrics::set_default_local_recorder(&recorder);
    let wal_root = tempfile::tempdir().expect("WAL directory");
    let (_wal, scribe) = scribe(&wal_root, NodeId::new(Uuid::now_v7()));
    let held = scribe
        .memory
        .try_reserve_ingress(MemoryCategory::Raw, scribe.memory.ingress_limit_bytes())
        .expect("reserve Scribe ingress breaker capacity");

    let error = ingest(&scribe, DataTenantId::new_v7(), "memory", Uuid::now_v7())
        .await
        .expect_err("memory breaker rejects append");
    assert!(matches!(error, ScribeError::IngestBusy { .. }));
    assert_rejection(&recorder, "ingress_sublimit", 1);
    drop(held);
    scribe
        .shutdown(std::time::Instant::now() + std::time::Duration::from_secs(1))
        .await;
}

/// The production shard mailbox branch emits exactly one bounded queue rejection.
#[tokio::test(flavor = "current_thread")]
async fn scribe_queue_rejection_emits_exact_owner_reason() {
    let recorder = wyrd_bench::BenchmarkRecorder::default();
    let _recorder = metrics::set_default_local_recorder(&recorder);
    let wal_root = tempfile::tempdir().expect("WAL directory");
    let (_wal, scribe) = scribe(&wal_root, NodeId::new(Uuid::now_v7()));
    scribe.shards.close();

    ingest(&scribe, DataTenantId::new_v7(), "queue", Uuid::now_v7())
        .await
        .expect_err("closed shard mailbox rejects append");
    assert_rejection(&recorder, "queue", 1);
    scribe
        .shutdown(std::time::Instant::now() + std::time::Duration::from_secs(1))
        .await;
}

/// The production lifecycle gate emits exactly one bounded closed rejection.
#[tokio::test(flavor = "current_thread")]
async fn scribe_closed_rejection_emits_exact_owner_reason() {
    let recorder = wyrd_bench::BenchmarkRecorder::default();
    let _recorder = metrics::set_default_local_recorder(&recorder);
    let wal_root = tempfile::tempdir().expect("WAL directory");
    let (_wal, scribe) = scribe(&wal_root, NodeId::new(Uuid::now_v7()));
    scribe
        .closed
        .store(true, std::sync::atomic::Ordering::Release);

    let error = ingest(&scribe, DataTenantId::new_v7(), "closed", Uuid::now_v7())
        .await
        .expect_err("closed lifecycle rejects append");
    assert!(matches!(error, ScribeError::IngressClosed));
    assert_rejection(&recorder, "closed", 1);
    scribe
        .shutdown(std::time::Instant::now() + std::time::Duration::from_secs(1))
        .await;
}

/// The production request-size branch emits exactly one bounded invalid rejection.
#[tokio::test(flavor = "current_thread")]
async fn scribe_invalid_rejection_emits_exact_owner_reason() {
    let recorder = wyrd_bench::BenchmarkRecorder::default();
    let _recorder = metrics::set_default_local_recorder(&recorder);
    let wal_root = tempfile::tempdir().expect("WAL directory");
    let (_wal, scribe) = scribe(&wal_root, NodeId::new(Uuid::now_v7()));

    let error = ingest_measured(
        &scribe,
        DataTenantId::new_v7(),
        "invalid",
        Uuid::now_v7(),
        crate::gate::limits::BIFROST_TRANSPORT_MESSAGE_LIMIT_BYTES + 1,
    )
    .await
    .expect_err("oversized request rejects append");
    assert!(matches!(error, ScribeError::PayloadTooLarge { .. }));
    assert_rejection(&recorder, "invalid", 1);
    scribe
        .shutdown(std::time::Instant::now() + std::time::Duration::from_secs(1))
        .await;
}

/// Concurrent and sequential shutdown callers share one completed drain.
#[tokio::test]
/// Repeated shutdown shares one completion and releases every retained owner.
async fn duplicate_shutdown_is_idempotent_and_closes_owners() {
    let wal_root = tempfile::tempdir().expect("WAL directory");
    let node = NodeId::new(Uuid::now_v7());
    let (_wal, scribe) = scribe(&wal_root, node);
    let scribe = Arc::new(scribe);
    ingest(
        &scribe,
        DataTenantId::new(Uuid::now_v7()).expect("tenant ID"),
        "shutdown_owner",
        Uuid::now_v7(),
    )
    .await
    .expect("durable append opens one WAL stream");
    assert_eq!(
        scribe
            .inspection_snapshot()
            .expect("pre-shutdown inspection")
            .open_wal_stream_count,
        1
    );
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    let first = {
        let scribe = Arc::clone(&scribe);
        tokio::spawn(async move { scribe.shutdown(deadline).await })
    };
    let second = {
        let scribe = Arc::clone(&scribe);
        tokio::spawn(async move { scribe.shutdown(deadline).await })
    };
    first.await.expect("first shutdown owner");
    second.await.expect("second shutdown waiter");
    scribe
        .shutdown(std::time::Instant::now() + std::time::Duration::from_secs(1))
        .await;
    let snapshot = scribe.inspection_snapshot().expect("shutdown inspection");
    assert_eq!(snapshot.queued_items, 0);
    assert_eq!(snapshot.open_wal_stream_count, 0);
    assert_ingress_owners_settled(&scribe);
}

/// Cancelling the graceful owner cannot strand Scribe in its draining state.
///
/// # Panics
///
/// Panics if the fixture cannot start, draining is not observed, cancellation
/// fails to run the synchronous finalizer, or later shutdown paths exceed their
/// bounded deadlines or retain a worker owner.
#[tokio::test]
async fn cancelled_shutdown_owner_is_recovered_by_abort_finalizer() {
    let wal_root = tempfile::tempdir().expect("WAL directory");
    let node = NodeId::new(Uuid::now_v7());
    let (_wal, scribe) = scribe(&wal_root, node);
    let scribe = Arc::new(scribe);
    let stalled = scribe.install_shutdown_stall_for_test().await;
    let first = {
        let scribe = Arc::clone(&scribe);
        tokio::spawn(async move {
            scribe
                .shutdown(std::time::Instant::now() + std::time::Duration::from_secs(30))
                .await;
        })
    };

    tokio::time::timeout(
        std::time::Duration::from_secs(1),
        scribe.wait_shutdown_draining_for_test(),
    )
    .await
    .expect("shutdown reaches draining");
    first.abort();
    first.await.expect_err("first shutdown future is cancelled");

    tokio::time::timeout(
        std::time::Duration::from_secs(1),
        scribe.shutdown(std::time::Instant::now() + std::time::Duration::from_secs(1)),
    )
    .await
    .expect("subsequent shutdown observes stopped state");
    scribe.abort_shutdown();
    assert!(stalled.is_finished());
    assert_eq!(
        scribe
            .inspection_snapshot()
            .expect("shutdown inspection")
            .open_wal_stream_count,
        0
    );
    assert_ingress_owners_settled(&scribe);
}

#[test]
/// A future WAL header version fails with its typed compatibility error.
fn wal_v2_header_fails_with_typed_unsupported_version() {
    let mut header = SegmentHeader::new([3_u8; 16], 1, 0, 0);
    header.version = 2;
    let mut encoded = header.encode();
    let crc = crc32c::crc32c(&encoded[..60]);
    encoded[60..64].copy_from_slice(&crc.to_le_bytes());

    assert!(matches!(
        SegmentHeader::decode(&encoded),
        Err(ScribeError::UnsupportedWalVersion { version: 2 })
    ));
}

#[tokio::test]
/// A sync failure produces neither acknowledgement nor memtable visibility.
async fn sync_failure_has_no_ack_or_memtable_visibility() {
    let wal_root = tempfile::tempdir().expect("WAL directory");
    let node = NodeId::new(Uuid::now_v7());
    let (wal, scribe) = scribe(&wal_root, node);
    let tenant = DataTenantId::new_v7();
    wal.trip_sync_failure_for_test();

    let error = ingest(&scribe, tenant, "sync_failure", Uuid::now_v7())
        .await
        .expect_err("sync failure must not acknowledge");
    // Assert the variant, not the wording. A non-disk-full fsync failure is
    // modelled as `Internal` by `WalSegment::sync_data`, and the injection
    // returns that same variant, so this holds for the real failure too;
    // `WalDiskFull` stays distinct because only it marks the disk hard-failed.
    assert!(
        matches!(error, ScribeError::Internal { .. }),
        "sync failure must surface as a typed Internal error; got {error:?}"
    );
    let stats = scribe.memtable_stats().expect("memtable stats");
    assert_eq!(stats.writable_rows + stats.immutable_rows, 0);
    assert!(
        wal.bytes_on_disk() > 0,
        "record was written before sync failed"
    );
    assert_ingress_owners_settled(&scribe);
    scribe
        .shutdown(std::time::Instant::now() + std::time::Duration::from_secs(1))
        .await;
}

#[tokio::test]
/// A post-fsync response failure reuses the stable batch exactly once.
async fn failure_after_fsync_before_ack_reuses_stable_batch_once() {
    let wal_root = tempfile::tempdir().expect("WAL directory");
    let node = NodeId::new(Uuid::now_v7());
    let (wal, scribe) = scribe(&wal_root, node);
    let tenant = DataTenantId::new_v7();
    let batch_id = Uuid::now_v7();
    let retry_principal = principal(tenant);
    wal.trip_post_sync_failure_for_test();

    let first = ingest_as(
        &scribe,
        retry_principal.clone(),
        "post_sync_failure",
        batch_id,
    );
    tokio::pin!(first);
    tokio::select! {
        result = &mut first => panic!("post-COMMIT owner acknowledged before visibility: {result:?}"),
        () = tokio::time::sleep(std::time::Duration::from_millis(20)) => {}
    }
    assert_eq!(scribe.memtable_stats().expect("stats").writable_rows, 0);
    let retained = scribe
        .inspection_snapshot()
        .expect("retained post-COMMIT owner")
        .ingress_lifecycle;
    assert_eq!(retained.active_attempts, 1);
    assert!(retained.active_reserved_bytes > 0);

    let retry = ingest_as(&scribe, retry_principal, "post_sync_failure", batch_id);
    let (first_result, retry_result) = tokio::join!(first, retry);
    first_result.expect("original ACK follows visible insertion");
    retry_result.expect("stable batch retry");
    assert_eq!(scribe.memtable_stats().expect("stats").writable_rows, 1);
    assert_ingress_owners_settled(&scribe);
    scribe
        .shutdown(std::time::Instant::now() + std::time::Duration::from_secs(1))
        .await;

    let replayed = replay_wal_directory(wal_root.path()).expect("replay");
    let state = replayed
        .values()
        .find(|state| state.seal_key.table.name == "post_sync_failure")
        .expect("replayed stable batch");
    assert_eq!(
        state.data_records.len(),
        1,
        "retry must not append a duplicate WAL record"
    );
    assert_eq!(state.append_metas.len(), 1);
    let replayed_identity = &state.append_metas[0].append_slice_id;
    assert_eq!(replayed_identity.batch_id, batch_id);
    assert_eq!(replayed_identity.seal_key.tenant, tenant);
    assert_eq!(replayed_identity.seal_key.table.name, "post_sync_failure");
    assert_eq!(replayed_identity.slice_index, 0);
}

#[tokio::test]
/// A reused batch ID with different payload identity fails before WAL mutation.
async fn contradictory_batch_retry_is_rejected_before_second_wal_append() {
    let wal_root = tempfile::tempdir().expect("WAL directory");
    let node = NodeId::new(Uuid::now_v7());
    let (wal, scribe) = scribe(&wal_root, node);
    let tenant = DataTenantId::new_v7();
    let batch_id = Uuid::now_v7();

    ingest_value(&scribe, tenant, "contradictory_retry", batch_id, 7)
        .await
        .expect("first append");
    let wal_bytes_before = wal.bytes_on_disk();
    let error = ingest_value(&scribe, tenant, "contradictory_retry", batch_id, 8)
        .await
        .expect_err("contradictory retry must not acknowledge");
    assert!(error.to_string().contains("contradictory payload identity"));
    assert_eq!(wal.bytes_on_disk(), wal_bytes_before);
    assert_eq!(scribe.memtable_stats().expect("stats").writable_rows, 1);
    assert_ingress_owners_settled(&scribe);

    scribe
        .shutdown(std::time::Instant::now() + std::time::Duration::from_secs(1))
        .await;
    let replayed = replay_wal_directory(wal_root.path()).expect("replay");
    let state = replayed
        .values()
        .find(|state| state.seal_key.table.name == "contradictory_retry")
        .expect("first append remains durable");
    assert_eq!(state.data_records.len(), 1);
    assert_eq!(state.append_metas.len(), 1);
    assert_eq!(state.commits.len(), 1);
}

#[test]
/// Multi-segment replay preserves order while deduplicating stable batches.
fn multi_segment_replay_preserves_order_and_deduplicates() {
    let wal_root = tempfile::tempdir().expect("WAL directory");
    let node = NodeId::new(Uuid::now_v7());
    let writer = WalWriter::new(
        wal_root.path(),
        *node.as_bytes(),
        1,
        WalConfig::new(256).expect("small segments"),
    )
    .expect("WAL writer");
    let tenant = DataTenantId::new_v7();
    let seal_key = key(tenant, "multi_segment_replay");
    let stable_audit = audit();
    for index in 0_u8..20 {
        let stable = index % 10;
        writer
            .append_and_commit_for_replay_test(&seal_key, [stable; 16], &stable_audit, &[stable])
            .expect("append");
    }

    let replayed = replay_wal_directory(wal_root.path()).expect("replay");
    let state = &replayed[&seal_key.as_path_components()];
    assert_eq!(state.data_records.len(), 10);
    assert!(
        state
            .append_metas
            .windows(2)
            .all(|window| window[0].wal_lsn < window[1].wal_lsn)
    );
}

#[test]
/// Root-issued Scribe and WAL hard limits reject before any WAL mutation.
fn scribe_root_and_wal_hard_limits_reject_before_append() {
    let resources =
        crate::scribe::embedded_scribe_resources(&crate::scribe::AdmissionConfig::default());
    let limit = resources.limit_bytes();
    let scribe = resources
        .try_reserve_maintenance(MemoryCategory::Raw, limit)
        .expect("Scribe limit reservation");
    assert!(
        resources
            .try_reserve_maintenance(MemoryCategory::Raw, 1)
            .is_err()
    );
    drop(scribe);

    let wal_root = tempfile::tempdir().expect("WAL directory");
    let writer =
        WalWriter::new(wal_root.path(), [8_u8; 16], 1, WalConfig::default()).expect("WAL writer");
    writer.trip_disk_full_for_test();
    let error = writer
        .append_and_fsync_for_test(
            &key(DataTenantId::new_v7(), "hard_limit"),
            [1_u8; 16],
            b"audit",
            b"data",
        )
        .expect_err("WAL hard limit");
    assert!(matches!(error, ScribeError::WalDiskFull));
    assert_eq!(writer.bytes_on_disk(), 0);
}
