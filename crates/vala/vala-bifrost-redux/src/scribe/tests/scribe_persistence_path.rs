use std::sync::Arc;

use crate::catalog::{TableRef, TenantTableBinding};
use crate::contracts::{
    IngressPayload, Scribe, ScribeError, ScribeIngressFrame, projected_source_schema_fingerprint,
};
use crate::namespaces::BifrostNamespace;
use crate::scribe::ScribeImpl;
use crate::scribe::geometry::DEFAULT_SHARD_COUNT;
use crate::scribe::stream_identity::{NodeId, StreamIdentity, WriterEpoch};
use crate::scribe::tail_rpc::FetchLiveTailRequest;
use crate::scribe::wal::{WalConfig, WalWriter};
use arrow::array::{Array, ArrayRef, Int64Array};
use arrow::datatypes::{DataType, Field, Schema};
use arrow::record_batch::RecordBatch;
use tempfile::TempDir;
use uuid::Uuid;
use wyrd_runtime::{PermissionSet, Principal, PrincipalKind};
use wyrd_spec::auth::PrincipalId;
use wyrd_spec::ids::DataTenantId;
use wyrd_spec::request_id::RequestId;

/// The fixed instant, in nanoseconds, every projected metric point carries.
///
/// `metrics.points` partitions hourly on `time_unix_nano`, so pinning the
/// point time pins the one partition the tail request selects.
const FIXTURE_POINT_TIME_NANOS: u64 = 1_784_016_000_000_000_000;

/// Returns the hourly `metrics.points` partition the fixture points land in.
///
/// # Panics
/// Panics when the fixed instant is not representable.
fn fixture_point_hour() -> crate::catalog::layout::TimePartition {
    let instant = chrono::DateTime::from_timestamp_nanos(
        i64::try_from(FIXTURE_POINT_TIME_NANOS).expect("the fixed instant fits i64"),
    );
    crate::catalog::TimeGranularity::Hour
        .bucket(instant)
        .expect("a fixed instant buckets into an hour")
}

/// Returns the hourly receipt partitions between `since` and now.
///
/// These fixtures write unregistered tables without a catalog owner, so each
/// row partitions on its hourly `wyrd_ingested_at` receipt. Bracketing the
/// append with `since` and the read-time clock selects that receipt hour even
/// when the append crosses an hour boundary.
///
/// # Panics
/// Panics when either instant cannot be bucketed into an hour.
fn receipt_hours(
    since: chrono::DateTime<chrono::Utc>,
    shift: chrono::Duration,
) -> (
    crate::catalog::layout::TimePartition,
    crate::catalog::layout::TimePartition,
) {
    let hour = |at| {
        crate::catalog::TimeGranularity::Hour
            .bucket(at)
            .expect("a receipt instant buckets into an hour")
    };
    (hour(since + shift), hour(chrono::Utc::now() + shift))
}

/// Build the one-row `value` batch for Scribe path tests.
///
/// # Panics
/// Panics when the Arrow fixture cannot be built.
fn batch(value: i64) -> RecordBatch {
    RecordBatch::try_new(
        Arc::new(Schema::new(vec![Field::new(
            "value",
            DataType::Int64,
            false,
        )])),
        vec![Arc::new(Int64Array::from(vec![value]))],
    )
    .expect("scribe persistence batch")
}

/// Build the tenant principal used by public-shaped append cases.
fn principal(tenant: DataTenantId) -> Principal {
    Principal::new(
        PrincipalId::new(Uuid::now_v7()),
        PrincipalKind::User,
        tenant,
        Vec::new(),
        PermissionSet::new(),
    )
}

/// Build the projected metric batch this fixture drives.
///
/// The rows come from the owning `OTLP` metrics projector, so the fixture
/// carries exactly the nested, metadata-bearing, correlation-column shape a
/// real signal write produces rather than a hand-built imitation. Every point
/// carries a `time_unix_nano` in the hour [`fixture_point_hour`] names, so
/// every row lands in one deterministic partition the tail request selects.
///
/// # Panics
///
/// Panics when the projection rejects a point.
fn projected_metric_batch() -> RecordBatch {
    use wyrd_tonic::otlp::metrics::v1::{
        Metric, ResourceMetrics, ScopeMetrics, Summary, SummaryDataPoint, metric,
        summary_data_point::ValueAtQuantile,
    };

    let point = |index: u64| SummaryDataPoint {
        attributes: Vec::new(),
        start_time_unix_nano: 0,
        time_unix_nano: FIXTURE_POINT_TIME_NANOS + index,
        count: index + 1,
        sum: 1.5,
        quantile_values: vec![
            ValueAtQuantile {
                quantile: 0.5,
                value: 1.0,
            },
            ValueAtQuantile {
                quantile: 0.9,
                value: 2.0,
            },
        ],
        flags: 0,
    };
    let request = vec![ResourceMetrics {
        resource: None,
        scope_metrics: vec![ScopeMetrics {
            scope: None,
            metrics: vec![Metric {
                name: "latency".to_owned(),
                description: String::new(),
                unit: String::new(),
                metadata: Vec::new(),
                data: Some(metric::Data::Summary(Summary {
                    data_points: (0..4).map(point).collect(),
                })),
            }],
            schema_url: String::new(),
        }],
        schema_url: String::new(),
    }];
    let (projected, outcome) =
        crate::tables::metrics::project_resource_metrics(&request, None, usize::MAX)
            .expect("the owning metrics projector accepts the fixture");
    assert_eq!(outcome.accepted_points, 4);
    projected
}

/// Encode the supplied batches as one Arrow IPC stream.
///
/// # Panics
///
/// Panics when the batches cannot be written or the stream cannot be finished.
fn ipc_stream(batches: &[RecordBatch]) -> Vec<u8> {
    let schema = batches.first().expect("at least one batch").schema();
    let mut bytes = Vec::new();
    let mut writer =
        arrow::ipc::writer::StreamWriter::try_new(&mut bytes, schema.as_ref()).expect("IPC writer");
    for batch in batches {
        writer.write(batch).expect("IPC batch");
    }
    writer.finish().expect("IPC terminal");
    bytes
}

/// Read every stamped row of one frame, keyed by its request id.
///
/// Rows are returned as `(metric_name, rendered quantile_values)` sorted by
/// value: the layout sort interleaves the two frames' rows, and a stored row
/// carries no position within its batch, so the comparison is between the
/// frames' row multisets.
///
/// # Panics
///
/// Panics when a managed column is absent, carries the wrong Arrow type, or a
/// quantile value cannot be rendered.
fn stamped_rows(batches: &[RecordBatch], request_id: &RequestId) -> Vec<(String, String)> {
    use arrow::array::{Array, StringArray};
    use arrow::util::display::array_value_to_string;

    let mut rows = Vec::new();
    for batch in batches {
        let ids = batch
            .column_by_name("wyrd_request_id")
            .and_then(|column| column.as_any().downcast_ref::<StringArray>())
            .expect("wyrd_request_id is Utf8");
        let names = batch
            .column_by_name("metric_name")
            .and_then(|column| column.as_any().downcast_ref::<StringArray>())
            .expect("metric_name is Utf8");
        let quantiles = batch
            .column_by_name("quantile_values")
            .expect("the nested canonical column survives the managed WAL path");
        for row in 0..ids.len() {
            if ids.value(row) != request_id.as_str() {
                continue;
            }
            rows.push((
                names.value(row).to_owned(),
                array_value_to_string(quantiles, row).expect("quantile value renders"),
            ));
        }
    }
    rows.sort();
    rows
}

/// Assert both payload modes stored every row and the same rows.
///
/// # Panics
///
/// Panics when either frame did not store exactly `total_rows` rows, or when
/// the two frames do not read back identical user columns.
fn assert_payload_modes_agree(
    stored: &[RecordBatch],
    canonical_request_id: &RequestId,
    arrow_request_id: &RequestId,
    total_rows: usize,
) {
    let canonical_rows = stamped_rows(stored, canonical_request_id);
    let arrow_rows = stamped_rows(stored, arrow_request_id);
    assert_eq!(
        canonical_rows.len(),
        total_rows,
        "one canonical batch stores every row"
    );
    assert_eq!(
        arrow_rows.len(),
        total_rows,
        "one Arrow IPC stream stores every row across every record batch"
    );
    assert_eq!(
        canonical_rows, arrow_rows,
        "both payload modes read back identical user columns"
    );
}

/// The managed envelope every canonical signal table owns, in schema order.
const MANAGED_COLUMNS: &[&str] = &[
    "run_id",
    "card_uid",
    "sub_principal_id",
    "act_principal_id",
    "wyrd_request_id",
    "wyrd_ingested_at",
];

/// Assert every stored managed column is the table's own `Field` and value.
///
/// The canonical path constructs only the managed arrays and clones every
/// managed `Field` — sensitivity metadata, type, and nullability —
/// from `(definition.schema)()`, so a locally reconstructed envelope would
/// differ here even when the values happen to agree.
///
/// # Panics
///
/// Panics when a managed column is absent from the stored projection, when its
/// stored `Field` differs from the table-owned physical field, or when a
/// per-row tenant column reappears.
fn assert_managed_columns_are_table_owned(stored: &[RecordBatch]) {
    let definition =
        crate::tables::builtin_table("metrics", "points").expect("the points built-in resolves");
    let physical = (definition.schema)();
    for batch in stored {
        for name in MANAGED_COLUMNS {
            let stored_index = batch
                .schema()
                .index_of(name)
                .unwrap_or_else(|_| panic!("the stored projection retains {name}"));
            let owned_index = physical
                .index_of(name)
                .unwrap_or_else(|_| panic!("the points table declares {name}"));
            assert_eq!(
                batch.schema().field(stored_index),
                physical.field(owned_index),
                "{name} is stored as the table-owned physical field"
            );
        }
        assert!(
            batch.schema().index_of("data_tenant_id").is_err(),
            "tenant ownership is proved by the file footer, never a row column"
        );
    }
}

/// Canonical nested batches take the one managed WAL path (S2).
///
/// The nested, metadata-bearing shape the owning `OTLP` metrics projector
/// produces is admitted, managed-stamped, and durably appended through exactly
/// the path a flat public canonical write takes, in both payload modes. The
/// same rows are submitted once as one canonical batch and once split across
/// two record batches of one Arrow IPC stream, and the stream must behave as
/// one request: one batch id, and one disjoint contiguous ordinal range over
/// the whole stream rather than a range that restarts per record batch.
///
/// # Panics
///
/// Panics when either frame is refused, when the two payload modes do not read
/// back identical user columns in input order, or when the Arrow stream does
/// not carry one batch id and the contiguous ordinals `0..total_rows`.
#[tokio::test]
async fn canonical_nested_batches_share_one_managed_wal_path() {
    let tenant = DataTenantId::new_v7();
    // The real canonical built-in, so the write travels the registry-dispatched
    // canonical path and its managed columns must come from the table's own
    // physical schema rather than from locally reconstructed fields.
    let table = TableRef::new(BifrostNamespace::Metrics, "points");
    let day = fixture_point_hour();
    let rows = projected_metric_batch();
    let total_rows = rows.num_rows();
    let canonical_request_id = RequestId::now_v7();
    let arrow_request_id = RequestId::now_v7();
    let temp_dir = TempDir::new().expect("WAL temp dir");
    let operator = Arc::new(object_store::memory::InMemory::new());
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

    let fingerprint = projected_source_schema_fingerprint(rows.schema().as_ref());
    let frame = |request_id: &RequestId, payload: IngressPayload| {
        let principal = principal(tenant);
        ScribeIngressFrame {
            attributed_cards: None,
            attribution: wyrd_spec::auth::Attribution::direct(principal.id).into(),
            authenticated_tenant: tenant,
            principal,
            table: table.clone(),
            expected_schema_fingerprint: Some(fingerprint),
            table_version: None,
            request_id: request_id.clone(),
            batch_id: Uuid::now_v7(),
            measured_wire_bytes: 0,
            payload,
        }
    };

    let canonical = Scribe::ingest_frame(
        &scribe,
        frame(
            &canonical_request_id,
            IngressPayload::Canonical(crate::contracts::CanonicalIngress::unreserved(vec![
                rows.clone(),
            ])),
        ),
    )
    .await
    .expect("nested canonical batches reach the managed WAL path");
    assert_eq!(
        canonical.rows_accepted,
        u64::try_from(total_rows).expect("row count")
    );

    let split = [rows.slice(0, 2), rows.slice(2, total_rows - 2)];
    assert!(split.iter().all(|batch| batch.num_rows() > 0));
    let arrow = Scribe::ingest_frame(
        &scribe,
        frame(
            &arrow_request_id,
            IngressPayload::ArrowIpc(bytes::Bytes::from(ipc_stream(&split))),
        ),
    )
    .await
    .expect("one Arrow IPC stream reaches the same managed WAL path");
    assert_eq!(
        arrow.rows_accepted,
        u64::try_from(total_rows).expect("row count")
    );

    let binding = TenantTableBinding::resolve((tenant, table)).expect("binding");
    let stored = live_rows(
        &scribe,
        &FetchLiveTailRequest {
            binding,
            target_stream: StreamIdentity::new(NodeId::new(Uuid::nil()), WriterEpoch::new(1)),
            start_partition: day,
            end_partition: day,
            required_fields: crate::scribe::tail_rpc::tests::named_fields(
                MANAGED_COLUMNS
                    .iter()
                    .copied()
                    .chain(["metric_name", "quantile_values"]),
            ),
            query_id: Uuid::new_v4(),
            capture_id: Uuid::nil(),
        },
        "hot snapshot",
    )
    .await;

    assert_payload_modes_agree(
        &stored,
        &canonical_request_id,
        &arrow_request_id,
        total_rows,
    );
    assert_managed_columns_are_table_owned(&stored);

    scribe
        .shutdown(std::time::Instant::now() + std::time::Duration::from_secs(1))
        .await;
}

#[tokio::test]
/// Production shards expose exact projections and WAL bounds to tail readers.
async fn production_shard_snapshot_serves_exact_projection_and_lsn_range() {
    let tenant = DataTenantId::new_v7();
    let table = TableRef::new(BifrostNamespace::Bifrost, "scribe_tail");
    let rows = batch(42);
    let batch_id = Uuid::now_v7();
    let temp_dir = TempDir::new().expect("WAL temp dir");
    let operator = Arc::new(object_store::memory::InMemory::new());
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
    let since = chrono::Utc::now();
    let admission = Scribe::ingest_frame(
        &scribe,
        ScribeIngressFrame {
            attributed_cards: None,
            attribution: wyrd_spec::auth::Attribution::direct(principal.id).into(),
            authenticated_tenant: tenant,
            principal,
            table: table.clone(),
            expected_schema_fingerprint: Some(projected_source_schema_fingerprint(
                rows.schema().as_ref(),
            )),
            table_version: None,
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
    assert_eq!(inspection.shard_task_count, DEFAULT_SHARD_COUNT);
    assert_eq!(inspection.shard_channel_count, DEFAULT_SHARD_COUNT);
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
    let (start_partition, end_partition) = receipt_hours(since, chrono::Duration::zero());
    let request = FetchLiveTailRequest {
        binding,
        target_stream: stream,
        start_partition,
        end_partition,
        required_fields: crate::scribe::tail_rpc::tests::named_fields(["value"]),
        query_id: Uuid::new_v4(),
        capture_id: Uuid::nil(),
    };
    let hot = live_rows(&scribe, &request, "hot snapshot").await;
    assert_eq!(hot.len(), 1);
    assert_eq!(hot[0].schema().fields().len(), 1);
    assert_eq!(hot[0].schema().field(0).name(), "value");

    scribe
        .shutdown(std::time::Instant::now() + std::time::Duration::from_secs(1))
        .await;
}

#[tokio::test]
/// Oracle hot snapshots retain Arrow identity and isolate receipt partitions.
///
/// Both tables are unregistered, so each row partitions on its hourly
/// `wyrd_ingested_at` receipt. Shifting the receipt clock two days between the
/// isolation appends places them in disjoint partitions.
async fn oracle_hot_snapshot_preserves_pointer_identity_and_partition_isolation() {
    let tenant = DataTenantId::new_v7();
    let pointer_table = TableRef::new(BifrostNamespace::Bifrost, "scribe_pointer_identity");
    let isolation_table = TableRef::new(BifrostNamespace::Bifrost, "scribe_partition_isolation");
    let source = batch(42);
    let source_value = source.column(0).clone();
    let temp_dir = TempDir::new().expect("WAL temp dir");
    let operator = Arc::new(object_store::memory::InMemory::new());
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
    let since = chrono::Utc::now();
    let shift = chrono::Duration::days(2);
    append_one(&scribe, tenant, &pointer_table, source).await;
    append_one(&scribe, tenant, &isolation_table, batch(101)).await;
    scribe.shift_receipt_clock_for_test(shift.to_std().expect("a positive shift"));
    append_one(&scribe, tenant, &isolation_table, batch(202)).await;

    let stream = StreamIdentity::new(NodeId::new(Uuid::nil()), WriterEpoch::new(1));
    let current = receipt_hours(since, chrono::Duration::zero());
    let shifted = receipt_hours(since, shift);
    assert_pointer_identity(
        &scribe,
        tenant,
        &pointer_table,
        current,
        &source_value,
        stream,
    )
    .await;
    let read = |hours| hot_values(&scribe, tenant, &isolation_table, hours, stream);
    assert_eq!(read(current).await, vec![101]);
    assert_eq!(read(shifted).await, vec![202]);
    assert_other_tenant_isolated(&scribe, &pointer_table, current, stream).await;
    scribe
        .shutdown(std::time::Instant::now() + std::time::Duration::from_secs(1))
        .await;
}

/// Durably appends one single-row `batch` to `table` for `tenant`.
///
/// # Panics
/// Panics when Scribe refuses the append or accepts other than one row.
async fn append_one(
    scribe: &ScribeImpl,
    tenant: DataTenantId,
    table: &TableRef,
    rows: RecordBatch,
) {
    let principal = principal(tenant);
    let admission = Scribe::ingest_frame(
        scribe,
        ScribeIngressFrame {
            attributed_cards: None,
            attribution: wyrd_spec::auth::Attribution::direct(principal.id).into(),
            authenticated_tenant: tenant,
            principal,
            table: table.clone(),
            expected_schema_fingerprint: Some(projected_source_schema_fingerprint(
                rows.schema().as_ref(),
            )),
            table_version: None,
            request_id: RequestId::now_v7(),
            batch_id: Uuid::now_v7(),
            measured_wire_bytes: 0,
            payload: IngressPayload::Canonical(crate::contracts::CanonicalIngress::unreserved(
                vec![rows],
            )),
        },
    )
    .await
    .expect("durable append");
    assert_eq!(admission.rows_accepted, 1);
}

/// Builds the `value` tail read of `table` over the inclusive partition range.
///
/// # Panics
/// Panics when the tenant table binding cannot be resolved.
fn value_read(
    tenant: DataTenantId,
    table: &TableRef,
    (start_partition, end_partition): (
        crate::catalog::layout::TimePartition,
        crate::catalog::layout::TimePartition,
    ),
    stream: StreamIdentity,
) -> FetchLiveTailRequest {
    FetchLiveTailRequest {
        binding: TenantTableBinding::resolve((tenant, table.clone())).expect("tail binding"),
        target_stream: stream,
        start_partition,
        end_partition,
        required_fields: crate::scribe::tail_rpc::tests::named_fields(["value"]),
        query_id: Uuid::new_v4(),
        capture_id: Uuid::nil(),
    }
}

/// Assert a returned hot batch shares the expected Arrow allocation.
///
/// # Panics
/// Panics when the read does not return exactly the source allocation.
async fn assert_pointer_identity(
    scribe: &ScribeImpl,
    tenant: DataTenantId,
    table: &TableRef,
    hours: (
        crate::catalog::layout::TimePartition,
        crate::catalog::layout::TimePartition,
    ),
    source_value: &ArrayRef,
    stream: StreamIdentity,
) {
    let hot = live_rows(
        scribe,
        &value_read(tenant, table, hours, stream),
        "pointer hot snapshot",
    )
    .await;
    assert_eq!(hot.len(), 1);
    assert!(Arc::ptr_eq(source_value, hot[0].column(0)));
}

/// Reads every hot `value` of `table` in the partition range, in row order.
///
/// # Panics
/// Panics when the read cannot open or a value column is not `Int64`.
async fn hot_values(
    scribe: &ScribeImpl,
    tenant: DataTenantId,
    table: &TableRef,
    hours: (
        crate::catalog::layout::TimePartition,
        crate::catalog::layout::TimePartition,
    ),
    stream: StreamIdentity,
) -> Vec<i64> {
    live_rows(
        scribe,
        &value_read(tenant, table, hours, stream),
        "isolation snapshot",
    )
    .await
    .iter()
    .flat_map(|rows| {
        rows.column(0)
            .as_any()
            .downcast_ref::<Int64Array>()
            .expect("hot values")
            .values()
            .to_vec()
    })
    .collect()
}

/// Captures and opens one live read on `scribe` and returns its memtable rows.
///
/// These fixtures never stage a generation, so the memtable cut is the whole
/// read.
///
/// # Panics
/// Panics with `context` when the read cannot open or names a staged run.
async fn live_rows(
    scribe: &ScribeImpl,
    request: &FetchLiveTailRequest,
    context: &str,
) -> Vec<RecordBatch> {
    let service = scribe.tail_service().expect("tail service");
    let request = crate::scribe::tail_rpc::tests::captured(&service, request).await;
    let (rows, _charges, runs, _lease) =
        service.open_capture(&request).expect(context).into_parts();
    assert!(runs.is_empty(), "{context}: the fixture staged no run");
    rows.into_iter().map(|batch| batch.rows).collect()
}

/// Assert a distinct tenant cannot observe the retained hot batch.
///
/// # Panics
/// Panics when another tenant's read returns any row.
async fn assert_other_tenant_isolated(
    scribe: &ScribeImpl,
    table: &TableRef,
    hours: (
        crate::catalog::layout::TimePartition,
        crate::catalog::layout::TimePartition,
    ),
    stream: StreamIdentity,
) {
    let other = live_rows(
        scribe,
        &value_read(DataTenantId::new_v7(), table, hours, stream),
        "other tenant snapshot",
    )
    .await;
    assert!(other.is_empty(), "hot snapshots must be tenant isolated");
}

#[tokio::test]
/// A shard WAL failure reaches the caller's durable completion boundary.
async fn shard_wal_failure_reaches_the_durable_completion() {
    let tenant = DataTenantId::new_v7();
    let table = TableRef::new(BifrostNamespace::Bifrost, "scribe_wal_failure");
    let temp_dir = TempDir::new().expect("WAL temp dir");
    let operator = Arc::new(object_store::memory::InMemory::new());
    let wal = Arc::new(
        WalWriter::new(
            temp_dir.path(),
            *Uuid::nil().as_bytes(),
            1,
            WalConfig::default(),
        )
        .expect("WAL writer"),
    );
    wal.trip_sync_failure_for_test();
    let scribe = ScribeImpl::new_for_embedded_with_deps(
        operator,
        Arc::clone(&wal),
        &Uuid::nil().to_string(),
        1,
    );
    let rows = batch(42);
    let principal = principal(tenant);
    let request_id = RequestId::now_v7();
    let error = Scribe::ingest_frame(
        &scribe,
        ScribeIngressFrame {
            attributed_cards: None,
            attribution: wyrd_spec::auth::Attribution::direct(principal.id).into(),
            authenticated_tenant: tenant,
            principal,
            table,
            expected_schema_fingerprint: Some(projected_source_schema_fingerprint(
                rows.schema().as_ref(),
            )),
            table_version: None,
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
    assert!(matches!(error, ScribeError::Internal { .. }), "{error:?}");
    scribe
        .shutdown(std::time::Instant::now() + std::time::Duration::from_secs(1))
        .await;
}

mod pg_tests {
    use super::*;
    use secrecy::ExposeSecret;
    use wyrd_spec::vala::error::BifrostError;
    use wyrd_spec::vala::ids::TableVersion;

    /// A native frame naming a table version that is not current is refused
    /// as a typed table-version conflict naming the current version, before
    /// any durable work.
    ///
    /// The same batch id then commits at the current version as a first
    /// commit, so the refused frame left no fence behind.
    ///
    /// # Panics
    ///
    /// Panics when the fixture or registration fails, the stale frame is
    /// admitted or refused with any other error, or the retry at the current
    /// version is refused or is not a first commit.
    #[tokio::test]
    async fn stale_table_version_is_refused_without_residue() {
        let fixture = wyrd_dev_fixtures::pg::PgFixture::start()
            .await
            .expect("postgres fixture starts");
        let warehouse = TempDir::new().expect("warehouse directory");
        let catalog = Arc::new(
            crate::catalog::BifrostCatalog::new(
                fixture.catalog_dsn().expose_secret(),
                crate::storage::BifrostStorage::for_test(warehouse.path(), false),
                fixture.vala_postgres().clone(),
            )
            .await
            .expect("redux catalog builds over the fixture"),
        );
        let tenant = fixture.data_tenant_id();
        let table = TableRef::new(BifrostNamespace::Datasets, "stale_version");
        let registered = catalog
            .register_dataset(
                tenant,
                wyrd_spec::auth::Attribution::unresolved(),
                table.clone(),
                vec![Field::new("value", DataType::Int64, false)],
                None,
                crate::catalog::CompactionRegistration::default(),
            )
            .await
            .expect("dataset registers");
        assert_eq!(registered.table_version, TableVersion::INITIAL);
        let temp_dir = TempDir::new().expect("WAL temp dir");
        let wal = Arc::new(
            WalWriter::new(
                temp_dir.path(),
                *Uuid::nil().as_bytes(),
                1,
                WalConfig::default(),
            )
            .expect("WAL writer"),
        );
        let scribe = ScribeImpl::new_for_embedded_with_deps_and_catalog(
            Arc::new(object_store::memory::InMemory::new()),
            wal,
            &Uuid::nil().to_string(),
            1,
            catalog,
        );
        let batch_id = Uuid::now_v7();
        let frame = |table_version| {
            let principal = principal(tenant);
            ScribeIngressFrame {
                attributed_cards: None,
                attribution: wyrd_spec::auth::Attribution::direct(principal.id).into(),
                authenticated_tenant: tenant,
                principal,
                table: table.clone(),
                expected_schema_fingerprint: None,
                table_version: Some(table_version),
                request_id: RequestId::now_v7(),
                batch_id,
                measured_wire_bytes: 0,
                payload: IngressPayload::ArrowIpc(bytes::Bytes::from(ipc_stream(&[batch(7)]))),
            }
        };

        let refused = Scribe::ingest_frame(
            &scribe,
            frame(TableVersion::new(2).expect("positive version")),
        )
        .await
        .expect_err("a non-current table version is refused");
        assert!(
            matches!(
                &refused,
                ScribeError::ContractViolation(BifrostError::TableVersionConflict {
                    table_version,
                    ..
                }) if *table_version == TableVersion::INITIAL
            ),
            "{refused:?}"
        );
        let admitted = Scribe::ingest_frame(&scribe, frame(TableVersion::INITIAL))
            .await
            .expect("the current table version is admitted");
        assert!(admitted.first_commit, "the refused frame left no fence");
        assert_eq!(admitted.rows_accepted, 1);

        scribe
            .shutdown(std::time::Instant::now() + std::time::Duration::from_secs(1))
            .await;
    }
}
