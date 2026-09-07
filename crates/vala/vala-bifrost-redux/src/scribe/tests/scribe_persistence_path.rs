use std::sync::Arc;

use crate::catalog::{TableRef, TenantTableBinding};
use crate::contracts::{
    IngressPayload, Scribe, ScribeError, ScribeIngressFrame, projected_source_schema_fingerprint,
};
use crate::namespaces::BifrostNamespace;
use crate::scribe::ScribeImpl;
use crate::scribe::admission::EventTimeWindow;
use crate::scribe::seal_key::SealKey;
use crate::scribe::stream_identity::{NodeId, StreamIdentity, WriterEpoch};
use crate::scribe::tail_rpc::FetchLiveTailRequest;
use crate::scribe::wal::{WalConfig, WalWriter};
use arrow::array::{ArrayRef, Int64Array, TimestampMicrosecondArray};
use arrow::datatypes::{DataType, Field, Schema, TimeUnit};
use arrow::record_batch::RecordBatch;
use tempfile::TempDir;
use uuid::Uuid;
use wyrd_runtime::{PermissionSet, Principal, PrincipalKind};
use wyrd_spec::auth::PrincipalId;
use wyrd_spec::ids::DataTenantId;
use wyrd_spec::request_id::RequestId;
use wyrd_spec::vala::api::{AuditDecision, AuditEvent, AuditResult, AuthMethod};

/// Build the server-created audit event Gate owns for one production frame.
///
/// These fixtures call `Scribe::ingest_frame` directly, which never mints an
/// audit record of its own, so each frame carries the allow/success event an
/// authenticated write would have produced at the transport boundary.
fn frame_audit_event(
    principal: &Principal,
    table: &TableRef,
    request_id: &RequestId,
) -> AuditEvent {
    AuditEvent {
        request_id: request_id.clone(),
        trace_id: None,
        operation: "bifrost.append".to_owned(),
        resource: table.fqn(),
        card_ref: principal.card_ref().cloned(),
        principal_id: principal.id,
        principal_kind: principal.kind.tag(),
        auth_method: AuthMethod::Jwt,
        permission: "bifrost:append".to_owned(),
        decision: AuditDecision::Allow,
        result: AuditResult::Success,
        payload_summary: "projected persistence fixture rows".to_owned(),
        detail: None,
    }
}

/// Returns an event partition the production admission window still admits,
/// `days_before_receipt` days below the current receipt instant.
///
/// These fixtures must not pin an absolute date. A pinned instant silently ages
/// out of `[receipt - past, receipt + future]` as wall-clock advances, so the
/// append it feeds starts failing with `EventTimeOutOfRange` on some later
/// date and the test stops covering the path it names. Deriving the day from
/// the window keeps every case correct at any wall-clock time.
///
/// The partition is returned at hourly granularity because these fixtures drive
/// Scribe without a catalog registration, and an unregistered table resolves to
/// the hourly omission default. A seal key bucketed any other way would land in
/// a partition no tail request in this file selects.
///
/// # Panics
/// Panics when the derived instant is not representable, or is not an exact
/// hour boundary after truncation.
fn fixture_event_day(days_before_receipt: i64) -> crate::catalog::layout::TimePartition {
    let instant = chrono::DateTime::from_timestamp_micros(
        EventTimeWindow::default().admitted_event_time_micros(days_before_receipt),
    )
    .expect("derived event time must be representable");
    crate::catalog::TimeGranularity::Hour
        .bucket(instant)
        .expect("a truncated instant is an exact hourly boundary")
}

/// Build the fixed one-row event-time batch for Scribe path tests.
///
/// # Panics
/// Panics when static time or Arrow fixture construction fails.
fn batch(partition: crate::catalog::layout::TimePartition) -> RecordBatch {
    let timestamp = partition.start_utc().timestamp_micros();
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
            Arc::new(TimestampMicrosecondArray::from(vec![timestamp]).with_timezone("UTC")),
            Arc::new(Int64Array::from(vec![42])),
        ],
    )
    .expect("scribe persistence batch")
}

/// Build the tenant principal used by public-shaped append cases.
fn principal(tenant: DataTenantId) -> Principal {
    Principal {
        id: PrincipalId::new(Uuid::now_v7()),
        kind: PrincipalKind::User,
        tenant_id: tenant,
        roles: Vec::new(),
        effective_permissions: PermissionSet::new(),
    }
}

/// Build a canonical nested batch shaped like the signal ledgers' output.
///
/// The columns carry `PARQUET:field_id` metadata and a `List<Struct<..>>`
/// repeated-record column, which is the deepest shape any canonical signal
/// projection emits. Only the recursive fixed IPC encoder can persist it, so
/// this fixture proves the managed WAL path accepts canonical nesting.
///
/// # Panics
/// Panics when the Arrow fixture cannot be constructed.
fn nested_batch(partition: crate::catalog::layout::TimePartition) -> RecordBatch {
    use arrow::array::{Float64Array, ListArray, StringArray, StructArray};
    use arrow::buffer::OffsetBuffer;
    use arrow::datatypes::Fields;
    use std::collections::HashMap;

    let with_id = |field: Field, id: i32| {
        field.with_metadata(HashMap::from([(
            "PARQUET:field_id".to_owned(),
            id.to_string(),
        )]))
    };
    let quantile_fields = Fields::from(vec![
        with_id(Field::new("quantile", DataType::Float64, false), 3),
        with_id(Field::new("value", DataType::Float64, true), 4),
    ]);
    let quantiles = Arc::new(StructArray::new(
        quantile_fields.clone(),
        vec![
            Arc::new(Float64Array::from(vec![0.5, 0.9])) as ArrayRef,
            Arc::new(Float64Array::from(vec![Some(1.0), None])),
        ],
        None,
    ));
    let element = Arc::new(with_id(
        Field::new("item", DataType::Struct(quantile_fields), false),
        2,
    ));
    let quantile_values = ListArray::new(
        Arc::clone(&element),
        OffsetBuffer::new(vec![0, 2].into()),
        quantiles,
        None,
    );
    let timestamp = partition.start_utc().timestamp_micros();
    RecordBatch::try_new(
        Arc::new(Schema::new(vec![
            Field::new(
                "wyrd_event_time",
                DataType::Timestamp(TimeUnit::Microsecond, Some("UTC".into())),
                false,
            ),
            with_id(Field::new("metric_name", DataType::Utf8, false), 1),
            Field::new(
                "quantile_values",
                DataType::List(Arc::clone(&element)),
                false,
            )
            .with_metadata(HashMap::from([(
                "PARQUET:field_id".to_owned(),
                "5".to_owned(),
            )])),
        ])),
        vec![
            Arc::new(TimestampMicrosecondArray::from(vec![timestamp]).with_timezone("UTC")),
            Arc::new(StringArray::from(vec!["latency"])),
            Arc::new(quantile_values),
        ],
    )
    .expect("canonical nested batch")
}

/// Canonical nested batches take the one managed WAL path (S2).
///
/// The nested, metadata-bearing shape an OTLP projection produces is admitted,
/// managed-stamped, and durably appended through exactly the path a flat
/// public canonical write takes, then read back losslessly from the tail.
#[tokio::test]
async fn canonical_nested_batches_share_one_managed_wal_path() {
    let tenant = DataTenantId::new_v7();
    let table = TableRef::new(BifrostNamespace::Bifrost, "scribe_nested_canonical");
    let day = fixture_event_day(1);
    let rows = nested_batch(day);
    let quantiles = Arc::clone(rows.column(2));
    let batch_id = Uuid::now_v7();
    let temp_dir = TempDir::new().expect("WAL temp dir");
    let operator = Arc::new(
        opendal::Operator::new(opendal::services::Memory::default())
            .expect("memory operator")
            .finish(),
    );
    let wal = Arc::new(
        WalWriter::new(
            temp_dir.path(),
            *Uuid::nil().as_bytes(),
            1,
            WalConfig::default(),
        )
        .expect("WAL writer"),
    );
    let scribe = ScribeImpl::new_for_embedded_with_deps(operator, wal, &Uuid::nil().to_string(), 1);

    let principal = principal(tenant);
    let request_id = RequestId::now_v7();
    let admission = Scribe::ingest_frame(
        &scribe,
        ScribeIngressFrame {
            authenticated_tenant: tenant,
            audit_event: frame_audit_event(&principal, &table, &request_id),
            principal,
            table: table.clone(),
            expected_schema_fingerprint: Some(projected_source_schema_fingerprint(
                rows.schema().as_ref(),
            )),
            request_id,
            batch_id,
            measured_wire_bytes: 0,
            payload: IngressPayload::Canonical(crate::contracts::CanonicalIngress::unreserved(
                vec![rows],
            )),
        },
    )
    .await
    .expect("nested canonical batches reach the managed WAL path");
    assert_eq!(admission.rows_accepted, 1);

    let binding = TenantTableBinding::resolve((tenant, table)).expect("binding");
    let hot = scribe
        .tail_service()
        .expect("tail service")
        .fetch_hot_batches(FetchLiveTailRequest {
            binding,
            target_stream: StreamIdentity::new(NodeId::new(Uuid::nil()), WriterEpoch::new(1)),
            start_partition: day,
            end_partition: day,
            required_columns: vec!["quantile_values".to_owned(), "wyrd_row_ordinal".to_owned()],
            predicates: Vec::new(),
            max_batches: 64,
            max_retained_bytes: 64 * 1024 * 1024,
        })
        .await
        .expect("hot snapshot");
    assert_eq!(hot.len(), 1);
    let projected = &hot[0].rows;
    let nested = projected
        .column_by_name("quantile_values")
        .expect("the nested canonical column survives the managed WAL path");
    assert_eq!(nested.as_ref(), quantiles.as_ref());
    assert!(
        projected.column_by_name("wyrd_row_ordinal").is_some(),
        "the managed envelope stamps nested batches like every other write"
    );

    scribe
        .shutdown(std::time::Instant::now() + std::time::Duration::from_secs(1))
        .await;
}

#[tokio::test]
/// Production shards expose exact projections and WAL bounds to tail readers.
async fn production_shard_snapshot_serves_exact_projection_and_lsn_range() {
    let tenant = DataTenantId::new_v7();
    let table = TableRef::new(BifrostNamespace::Bifrost, "scribe_tail");
    let day = fixture_event_day(1);
    let rows = batch(day);
    let batch_id = Uuid::now_v7();
    let temp_dir = TempDir::new().expect("WAL temp dir");
    let operator = Arc::new(
        opendal::Operator::new(opendal::services::Memory::default())
            .expect("memory operator")
            .finish(),
    );
    let wal = Arc::new(
        WalWriter::new(
            temp_dir.path(),
            *Uuid::nil().as_bytes(),
            1,
            WalConfig::default(),
        )
        .expect("WAL writer"),
    );
    let scribe = ScribeImpl::new_for_embedded_with_deps(operator, wal, &Uuid::nil().to_string(), 1);

    let principal = principal(tenant);
    let request_id = RequestId::now_v7();
    let admission = Scribe::ingest_frame(
        &scribe,
        ScribeIngressFrame {
            authenticated_tenant: tenant,
            audit_event: frame_audit_event(&principal, &table, &request_id),
            principal,
            table: table.clone(),
            expected_schema_fingerprint: Some(projected_source_schema_fingerprint(
                rows.schema().as_ref(),
            )),
            request_id,
            batch_id,
            measured_wire_bytes: 0,
            payload: IngressPayload::Canonical(crate::contracts::CanonicalIngress::unreserved(
                vec![rows],
            )),
        },
    )
    .await
    .expect("durable append");
    assert_eq!(admission.batch_id, batch_id);
    assert_eq!(admission.rows_accepted, 1);
    let inspection = scribe
        .inspection_snapshot()
        .expect("exact ownership inspection");
    assert_eq!(inspection.shard_task_count, 16);
    assert_eq!(inspection.shard_channel_count, 16);
    assert_eq!(
        inspection.memory_by_shard.iter().sum::<usize>(),
        inspection.total_accounted_memory
    );
    assert_eq!(
        inspection
            .memory_by_bucket
            .iter()
            .map(|bucket| bucket.writable_bytes + bucket.immutable_bytes)
            .sum::<usize>(),
        inspection.memory_by_category[4] + inspection.memory_by_category[5]
    );

    let binding = TenantTableBinding::resolve((tenant, table.clone())).expect("binding");
    let stream = StreamIdentity::new(NodeId::new(Uuid::nil()), WriterEpoch::new(1));
    let request = FetchLiveTailRequest {
        binding,
        target_stream: stream,
        start_partition: day,
        end_partition: day,
        required_columns: vec!["value".to_owned()],
        predicates: Vec::new(),
        max_batches: 64,
        max_retained_bytes: 64 * 1024 * 1024,
    };
    let hot = scribe
        .tail_service()
        .expect("tail service")
        .fetch_hot_batches(request.clone())
        .await
        .expect("hot snapshot");
    assert_eq!(hot.len(), 1);
    assert_eq!(
        hot[0].origin,
        crate::scribe::tail_rpc::HotBatchSource::Append {
            batch_id: *batch_id.as_bytes(),
        }
    );
    assert_eq!(hot[0].rows.schema().fields().len(), 1);
    assert_eq!(hot[0].rows.schema().field(0).name(), "value");

    scribe
        .shutdown(std::time::Instant::now() + std::time::Duration::from_secs(1))
        .await;
}

#[tokio::test]
/// Oracle hot snapshots retain Arrow identity and isolate event days.
async fn oracle_hot_snapshot_preserves_pointer_identity_and_day_isolation() {
    let tenant = DataTenantId::new_v7();
    let pointer_table = TableRef::new(BifrostNamespace::Bifrost, "scribe_pointer_identity");
    let day_table = TableRef::new(BifrostNamespace::Bifrost, "scribe_day_isolation");
    let day_one = fixture_event_day(2);
    let day_two = fixture_event_day(1);
    let source = batch(day_one);
    let source_value = source.column(1).clone();
    let temp_dir = TempDir::new().expect("WAL temp dir");
    let operator = Arc::new(
        opendal::Operator::new(opendal::services::Memory::default())
            .expect("memory operator")
            .finish(),
    );
    let wal = Arc::new(
        WalWriter::new(
            temp_dir.path(),
            *Uuid::nil().as_bytes(),
            1,
            WalConfig::default(),
        )
        .expect("WAL writer"),
    );
    let scribe = ScribeImpl::new_for_embedded_with_deps(operator, wal, &Uuid::nil().to_string(), 1);
    let pointer_principal = principal(tenant);
    let pointer_request_id = RequestId::now_v7();
    let pointer_admission = Scribe::ingest_frame(
        &scribe,
        ScribeIngressFrame {
            authenticated_tenant: tenant,
            audit_event: frame_audit_event(&pointer_principal, &pointer_table, &pointer_request_id),
            principal: pointer_principal,
            table: pointer_table.clone(),
            expected_schema_fingerprint: Some(projected_source_schema_fingerprint(
                source.schema().as_ref(),
            )),
            request_id: pointer_request_id,
            batch_id: Uuid::now_v7(),
            measured_wire_bytes: 0,
            payload: IngressPayload::Canonical(crate::contracts::CanonicalIngress::unreserved(
                vec![source.clone()],
            )),
        },
    )
    .await
    .expect("pointer identity append");
    assert_eq!(pointer_admission.rows_accepted, 1);
    let stream = StreamIdentity::new(NodeId::new(Uuid::nil()), WriterEpoch::new(1));
    assert_pointer_identity(
        &scribe,
        tenant,
        &pointer_table,
        day_one,
        &source_value,
        stream,
    )
    .await;

    let cross_day = cross_day_batch(source.schema(), day_one, day_two);
    let cross_day_principal = principal(tenant);
    let cross_day_request_id = RequestId::now_v7();
    let cross_day_admission = Scribe::ingest_frame(
        &scribe,
        ScribeIngressFrame {
            authenticated_tenant: tenant,
            audit_event: frame_audit_event(&cross_day_principal, &day_table, &cross_day_request_id),
            principal: cross_day_principal,
            table: day_table.clone(),
            expected_schema_fingerprint: Some(projected_source_schema_fingerprint(
                cross_day.schema().as_ref(),
            )),
            request_id: cross_day_request_id,
            batch_id: Uuid::now_v7(),
            measured_wire_bytes: 0,
            payload: IngressPayload::Canonical(crate::contracts::CanonicalIngress::unreserved(
                vec![cross_day],
            )),
        },
    )
    .await
    .expect("cross-day append");
    assert_eq!(cross_day_admission.rows_accepted, 2);
    assert_cross_day_materialization(&scribe, tenant, &day_table, day_one, day_two, stream).await;
    assert_other_tenant_isolated(&scribe, &pointer_table, day_one, stream).await;
    scribe
        .shutdown(std::time::Instant::now() + std::time::Duration::from_secs(1))
        .await;
}

/// Assert a returned hot batch shares the expected Arrow allocation.
async fn assert_pointer_identity(
    scribe: &ScribeImpl,
    tenant: DataTenantId,
    table: &TableRef,
    day: crate::catalog::layout::TimePartition,
    source_value: &ArrayRef,
    stream: StreamIdentity,
) {
    let hot = scribe
        .tail_service()
        .expect("tail service")
        .fetch_hot_batches(FetchLiveTailRequest {
            binding: TenantTableBinding::resolve((tenant, table.clone())).expect("pointer binding"),
            target_stream: stream,
            start_partition: day,
            end_partition: day,
            required_columns: vec!["value".to_owned()],
            predicates: Vec::new(),
            max_batches: 64,
            max_retained_bytes: 64 * 1024 * 1024,
        })
        .await
        .expect("pointer hot snapshot");
    assert_eq!(hot.len(), 1);
    assert!(Arc::ptr_eq(source_value, hot[0].rows.column(0)));
}

/// Build one batch spanning two event-day partitions.
///
/// # Panics
/// Panics when the static cross-day Arrow fixture cannot be built.
fn cross_day_batch(
    schema: Arc<Schema>,
    day_one: crate::catalog::layout::TimePartition,
    day_two: crate::catalog::layout::TimePartition,
) -> RecordBatch {
    RecordBatch::try_new(
        schema,
        vec![
            Arc::new(
                TimestampMicrosecondArray::from(vec![
                    day_one.start_utc().timestamp_micros(),
                    day_two.start_utc().timestamp_micros(),
                ])
                .with_timezone("UTC"),
            ),
            Arc::new(Int64Array::from(vec![101_i64, 202_i64])),
        ],
    )
    .expect("cross-day batch")
}

/// Assert cross-day materialization preserves row ownership and ordering.
async fn assert_cross_day_materialization(
    scribe: &ScribeImpl,
    tenant: DataTenantId,
    table: &TableRef,
    day_one: crate::catalog::layout::TimePartition,
    day_two: crate::catalog::layout::TimePartition,
    stream: StreamIdentity,
) {
    let read_day = |day: crate::catalog::layout::TimePartition| FetchLiveTailRequest {
        binding: TenantTableBinding::resolve((tenant, table.clone())).expect("day binding"),
        target_stream: stream,
        start_partition: day,
        end_partition: day,
        required_columns: vec!["value".to_owned()],
        predicates: Vec::new(),
        max_batches: 64,
        max_retained_bytes: 64 * 1024 * 1024,
    };
    let day_one_hot = scribe
        .tail_service()
        .expect("tail service")
        .fetch_hot_batches(read_day(day_one))
        .await
        .expect("day one snapshot");
    let day_two_hot = scribe
        .tail_service()
        .expect("tail service")
        .fetch_hot_batches(read_day(day_two))
        .await
        .expect("day two snapshot");
    assert_eq!(day_one_hot.len(), 1);
    assert_eq!(day_two_hot.len(), 1);
    assert_eq!(day_one_hot[0].rows.num_rows(), 1);
    assert_eq!(day_two_hot[0].rows.num_rows(), 1);
    assert_eq!(hot_value(&day_one_hot[0].rows), 101);
    assert_eq!(hot_value(&day_two_hot[0].rows), 202);
}

/// Read the fixture's single hot value.
///
/// # Panics
/// Panics when the fixture column is not the expected Int64 shape.
fn hot_value(rows: &RecordBatch) -> i64 {
    rows.column(0)
        .as_any()
        .downcast_ref::<Int64Array>()
        .expect("hot values")
        .value(0)
}

/// Assert a distinct tenant cannot observe the retained hot batch.
async fn assert_other_tenant_isolated(
    scribe: &ScribeImpl,
    table: &TableRef,
    day: crate::catalog::layout::TimePartition,
    stream: StreamIdentity,
) {
    let other = scribe
        .tail_service()
        .expect("tail service")
        .fetch_hot_batches(FetchLiveTailRequest {
            binding: TenantTableBinding::resolve((DataTenantId::new_v7(), table.clone()))
                .expect("other tenant binding"),
            target_stream: stream,
            start_partition: day,
            end_partition: day,
            required_columns: vec!["value".to_owned()],
            predicates: Vec::new(),
            max_batches: 64,
            max_retained_bytes: 64 * 1024 * 1024,
        })
        .await
        .expect("other tenant snapshot");
    assert!(other.is_empty(), "hot snapshots must be tenant isolated");
}

#[test]
/// A concrete WAL disk fault rejects before mutating the segment.
fn concrete_wal_disk_failure_rejects_before_file_mutation() {
    let temp_dir = TempDir::new().expect("WAL temp dir");
    let writer = WalWriter::new(
        temp_dir.path(),
        *Uuid::nil().as_bytes(),
        1,
        WalConfig::default(),
    )
    .expect("WAL writer");
    writer.trip_disk_full_for_test();
    let seal_key = SealKey::new(
        DataTenantId::new_v7(),
        TableRef::new(BifrostNamespace::Bifrost, "scribe_wal_failure"),
        fixture_event_day(1),
    );

    let error = writer
        .append_and_fsync_for_test(&seal_key, [7_u8; 16], b"audit", b"data")
        .expect_err("injected WAL disk failure");
    assert!(matches!(error, ScribeError::WalDiskFull));
    assert_eq!(writer.bytes_on_disk(), 0);
}

#[tokio::test]
/// A shard WAL failure reaches the caller's durable completion boundary.
async fn shard_wal_failure_reaches_the_durable_completion() {
    let tenant = DataTenantId::new_v7();
    let table = TableRef::new(BifrostNamespace::Bifrost, "scribe_wal_failure");
    let day = fixture_event_day(1);
    let temp_dir = TempDir::new().expect("WAL temp dir");
    let operator = Arc::new(
        opendal::Operator::new(opendal::services::Memory::default())
            .expect("memory operator")
            .finish(),
    );
    let wal = Arc::new(
        WalWriter::new(
            temp_dir.path(),
            *Uuid::nil().as_bytes(),
            1,
            WalConfig::default(),
        )
        .expect("WAL writer"),
    );
    wal.trip_disk_full_for_test();
    let scribe = ScribeImpl::new_for_embedded_with_deps(
        operator,
        Arc::clone(&wal),
        &Uuid::nil().to_string(),
        1,
    );
    let rows = batch(day);
    let principal = principal(tenant);
    let request_id = RequestId::now_v7();
    let error = Scribe::ingest_frame(
        &scribe,
        ScribeIngressFrame {
            authenticated_tenant: tenant,
            audit_event: frame_audit_event(&principal, &table, &request_id),
            principal,
            table,
            expected_schema_fingerprint: Some(projected_source_schema_fingerprint(
                rows.schema().as_ref(),
            )),
            request_id,
            batch_id: Uuid::now_v7(),
            measured_wire_bytes: 0,
            payload: IngressPayload::Canonical(crate::contracts::CanonicalIngress::unreserved(
                vec![rows],
            )),
        },
    )
    .await
    .expect_err("WAL failure must fail the durable completion");
    assert!(matches!(error, ScribeError::WalDiskFull));
    scribe
        .shutdown(std::time::Instant::now() + std::time::Duration::from_secs(1))
        .await;
}
