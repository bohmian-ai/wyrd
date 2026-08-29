//! Shared production-harness setup for the Scribe suite.
//!
//! Every test in this binary uses the S9 harness and public routes: a real
//! server, real Postgres, the real object store, the production Scribe service
//! and the production Oracle read path. Nothing here builds a parallel topology
//! or writes a durable row the production path did not produce.

use vala_bifrost_redux::catalog::{CreateTableRequest, TableRef};
use vala_bifrost_redux::namespaces::BifrostNamespace;
use vala_bifrost_redux::scribe::geometry::ScribeGeometry;
use wyrd_spec::DataTenantId;
use wyrd_testing::WyrdTestServer;

use arrow::datatypes::{DataType, Field};

/// Installs the production-shaped telemetry pipeline once for this binary.
///
/// Every case in this suite drives real shard, staging and publication owners
/// whose settlement decisions are only observable through their emitted spans
/// and events. Without a subscriber those tests run blind: a failure reports
/// the assertion and nothing about the lifecycle that produced it. The
/// subscriber is global and installs at most once per process, so the guard is
/// retained for the life of the binary and every server started here shares it.
fn shared_telemetry() -> Option<std::sync::Arc<wyrd_telemetry::TelemetryGuard>> {
    static TELEMETRY: std::sync::OnceLock<Option<std::sync::Arc<wyrd_telemetry::TelemetryGuard>>> =
        std::sync::OnceLock::new();
    TELEMETRY
        .get_or_init(|| {
            wyrd_telemetry::init_test_capture(wyrd_telemetry::TelemetryConfig::default())
                .ok()
                .map(|(guard, _capture)| std::sync::Arc::new(guard))
        })
        .clone()
}

/// Starts one bound production server with the default Scribe geometry.
///
/// Bound rather than in-process because every Scribe case here drives public
/// gRPC ingest and the public query route, which need real endpoints.
pub(super) async fn start_scribe_server() -> WyrdTestServer {
    let mut builder = WyrdTestServer::builder();
    if let Some(telemetry) = shared_telemetry() {
        builder = builder.with_telemetry_for_test(telemetry);
    }
    builder
        .start_bound()
        .await
        .expect("the Scribe production harness starts")
}

/// Starts one bound production server with an explicit Scribe geometry.
///
/// The geometry is the only production control a scaled case may move: it lets
/// a test reach a rotation, a target roll or a residue boundary without writing
/// production-sized data, while every other control stays exactly what
/// production uses.
pub(super) async fn start_scribe_server_with_geometry(geometry: ScribeGeometry) -> WyrdTestServer {
    let mut builder = WyrdTestServer::builder().with_scribe_geometry_for_test(geometry);
    if let Some(telemetry) = shared_telemetry() {
        builder = builder.with_telemetry_for_test(telemetry);
    }
    builder
        .start_bound()
        .await
        .expect("the Scribe production harness starts with the requested geometry")
}

/// Starts one bound production server with an explicit Scribe admission config.
///
/// The admission configuration is the only control that moves the pod's derived
/// ownership ceiling, which is what a fairness case needs: real contention
/// cannot be placed on a pod whose measured capacity completes more tables than
/// the case can ever occupy. Every other control stays exactly what production
/// uses, so the scheduler under test is the production scheduler.
pub(super) async fn start_scribe_server_with_admission(
    admission: vala_bifrost_redux::scribe::admission::AdmissionConfig,
) -> WyrdTestServer {
    let mut builder = WyrdTestServer::builder().with_scribe_admission_for_test(admission);
    if let Some(telemetry) = shared_telemetry() {
        builder = builder.with_telemetry_for_test(telemetry);
    }
    builder
        .start_bound()
        .await
        .expect("the Scribe production harness starts with the requested admission capacity")
}

/// Registers one single-column table for a tenant through the real catalog.
pub(super) async fn register_table(
    server: &WyrdTestServer,
    tenant: DataTenantId,
    namespace: BifrostNamespace,
    name: &str,
) -> String {
    server
        .create_bifrost_table_for_test(CreateTableRequest {
            table: TableRef::new(namespace, name),
            user_fields: vec![Field::new("value", DataType::Int64, false)],
            tenant,
            physical_layout: None,
            audit: None,
        })
        .await
        .expect("the catalog registers the table");
    format!("{}.{name}", namespace.as_str())
}

/// Builds a unique table name for one case.
pub(super) fn unique_table(prefix: &str) -> String {
    format!("{prefix}_{}", uuid::Uuid::now_v7().simple())
}

/// Builds one authenticated public SDK client bound to `tenant`.
///
/// Every Scribe case drives ingest and query through the same public routes a
/// real caller uses, so each tenant in a case needs its own service principal
/// and API key rather than a shared fixture credential.
pub(super) async fn tenant_client(
    server: &WyrdTestServer,
    tenant: DataTenantId,
) -> wyrd_client::WyrdClient {
    let bootstrap = server
        .bootstrap_service_in_tenant(tenant, &unique_table("scribe_client"), &["admin"])
        .await
        .expect("tenant service bootstrap");
    let api_key = bootstrap.api_key().expect("service API key").clone();
    wyrd_client::WyrdClient::with_config(wyrd_client::config::ClientConfig {
        grpc: wyrd_client::transport::GrpcConfig {
            endpoint: server.grpc_url().expect("bound gRPC URL"),
            connect_retries: 0,
            ..wyrd_client::transport::GrpcConfig::default()
        },
        http: wyrd_client::transport::HttpConfig {
            base_url: server.base_url().expect("bound HTTP URL").to_owned(),
            ..wyrd_client::transport::HttpConfig::default()
        },
        api_key: Some(api_key),
        ..wyrd_client::config::ClientConfig::default()
    })
    .expect("tenant SDK client")
}

/// Returns the single-column ingress schema `register_table` declares.
pub(super) fn value_schema() -> std::sync::Arc<arrow::datatypes::Schema> {
    std::sync::Arc::new(arrow::datatypes::Schema::new(vec![Field::new(
        "value",
        DataType::Int64,
        false,
    )]))
}

/// Returns the ingress schema that also carries the client's own event time.
///
/// `wyrd_event_time` is a managed column: a caller may supply it and Scribe
/// lifts the value verbatim into the managed slot, which is how a case selects
/// the physical partition its rows land in rather than accepting wall clock.
pub(super) fn event_time_schema() -> std::sync::Arc<arrow::datatypes::Schema> {
    std::sync::Arc::new(arrow::datatypes::Schema::new(vec![
        Field::new("value", DataType::Int64, false),
        Field::new(
            wyrd_spec::vala::managed_columns::WYRD_EVENT_TIME,
            DataType::Timestamp(arrow::datatypes::TimeUnit::Microsecond, Some("UTC".into())),
            false,
        ),
    ]))
}

/// Encodes one batch as the native Arrow IPC stream public ingest accepts.
pub(super) fn encode_ipc(batch: &arrow::record_batch::RecordBatch) -> Vec<u8> {
    let mut ipc = Vec::new();
    let mut writer = arrow::ipc::writer::StreamWriter::try_new(&mut ipc, batch.schema().as_ref())
        .expect("IPC writer");
    writer.write(batch).expect("IPC batch");
    writer.finish().expect("IPC terminal");
    ipc
}

/// Appends one fixed-identity batch of values through public gRPC ingest.
///
/// Returns the ingest error rather than panicking so pressure, fencing and
/// replay cases can assert on the exact typed refusal a real client receives.
/// Failing to reach the route at all is a harness fault, not a refusal, so
/// connect failures still panic.
///
/// # Errors
///
/// Returns the stable Wyrd error the public ingest route produced.
pub(super) async fn append_values(
    client: &wyrd_client::WyrdClient,
    table: &str,
    batch_id: uuid::Uuid,
    rows: &[i64],
) -> Result<(), wyrd_spec::error::WyrdError> {
    let schema = value_schema();
    let batch = arrow::record_batch::RecordBatch::try_new(
        std::sync::Arc::clone(&schema),
        vec![std::sync::Arc::new(arrow::array::Int64Array::from(
            rows.to_vec(),
        ))],
    )
    .expect("value batch");
    append_batch(client, table, batch_id, &batch).await
}

/// Appends one caller-built batch through public gRPC ingest.
///
/// Fencing cases need to send a batch whose schema is deliberately not the
/// registered one, which the typed helpers cannot express.
///
/// # Errors
///
/// Returns the stable Wyrd error the public ingest route produced.
pub(super) async fn append_batch(
    client: &wyrd_client::WyrdClient,
    table: &str,
    batch_id: uuid::Uuid,
    batch: &arrow::record_batch::RecordBatch,
) -> Result<(), wyrd_spec::error::WyrdError> {
    vala_sdk::grpc::BifrostGrpcTransport::connect(client)
        .await
        .expect("public ingest transport connects")
        .insert_batch(table, batch_id.into_bytes(), encode_ipc(batch))
        .await
        .map(|_| ())
}

/// Appends one batch whose rows all carry the caller's chosen event time.
///
/// # Errors
///
/// Returns the stable Wyrd error the public ingest route produced.
pub(super) async fn append_values_at(
    client: &wyrd_client::WyrdClient,
    table: &str,
    batch_id: uuid::Uuid,
    rows: &[i64],
    event_time: chrono::DateTime<chrono::Utc>,
) -> Result<(), wyrd_spec::error::WyrdError> {
    let schema = event_time_schema();
    let batch = arrow::record_batch::RecordBatch::try_new(
        std::sync::Arc::clone(&schema),
        vec![
            std::sync::Arc::new(arrow::array::Int64Array::from(rows.to_vec())),
            std::sync::Arc::new(
                arrow::array::TimestampMicrosecondArray::from(vec![
                    event_time.timestamp_micros();
                    rows.len()
                ])
                .with_timezone("UTC"),
            ),
        ],
    )
    .expect("event-time batch");
    append_batch(client, table, batch_id, &batch).await
}

/// Reads one table's values back through the public strict fused query route.
///
/// Strict freshness and fused visibility are what make the read an authority
/// check: the answer must come from whichever source currently owns the rows,
/// not from whatever happens to be cheapest.
pub(super) async fn read_values(client: &wyrd_client::WyrdClient, table: &str) -> Vec<i64> {
    read_sql(client, &format!("SELECT value FROM {table}")).await
}

/// Runs one strict fused public query and collects its `value` column.
pub(super) async fn read_sql(client: &wyrd_client::WyrdClient, sql: &str) -> Vec<i64> {
    let mut stream = vala_sdk::query::QueryClient::new(client)
        .query(&wyrd_spec::vala::api::BifrostQueryRequest {
            sql: sql.to_owned(),
            visibility: wyrd_spec::vala::api::VisibilityMode::Fused,
            freshness: wyrd_spec::vala::api::FreshnessPolicy::Strict,
            deadline_ms: Some(120_000),
        })
        .await
        .unwrap_or_else(|error| panic!("public query `{sql}` starts: {}", error.detail()));
    let mut values = Vec::new();
    while let Some(batch) = stream
        .next_batch()
        .await
        .unwrap_or_else(|error| panic!("public query `{sql}` streams: {}", error.detail()))
    {
        let column = batch
            .column_by_name("value")
            .expect("query result has a value column")
            .as_any()
            .downcast_ref::<arrow::array::Int64Array>()
            .expect("value column is Int64");
        values.extend(column.values().iter().copied());
    }
    values
}

/// Returns the sorted values a table currently reads back.
pub(super) async fn sorted_values(client: &wyrd_client::WyrdClient, table: &str) -> Vec<i64> {
    let mut values = read_values(client, table).await;
    values.sort_unstable();
    values
}

/// Counts every published hot object one tenant owns for one table.
pub(super) async fn published_object_count(
    server: &WyrdTestServer,
    tenant: DataTenantId,
    namespace: BifrostNamespace,
    table_name: &str,
) -> usize {
    server
        .published_hot_files_for_test(tenant, namespace.as_str(), table_name)
        .await
        .expect("published hot files are inspectable")
        .len()
}

/// Returns how many rows one tenant's published hot objects account for.
///
/// Row count rather than object count is what proves exactly-once publication:
/// the same rows may legitimately land in one object or several, but the sum
/// must always equal what the client acknowledged.
pub(super) async fn published_rows(
    server: &WyrdTestServer,
    tenant: DataTenantId,
    table_name: &str,
) -> u64 {
    server
        .published_hot_files_for_test(tenant, BifrostNamespace::Datasets.as_str(), table_name)
        .await
        .expect("published hot files are inspectable")
        .iter()
        .map(|file| file.row_count)
        .sum()
}

/// Waits until Scribe's bounded persistence queue has settled.
///
/// The freeze hands generations to the persistence runtime; the staged member
/// exists only once that queue has processed them. Polling the production
/// depth is what makes the following assertions observations of a settled pod
/// rather than of a race.
///
/// # Panics
///
/// Panics when the queue has not drained inside the case deadline.
pub(super) async fn await_persistence_drained(scribe: &vala_bifrost_redux::scribe::ScribeImpl) {
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(30);
    while scribe.persistence_queue_depth_for_test() > 0 {
        assert!(
            tokio::time::Instant::now() < deadline,
            "Scribe persistence queue did not drain after the freeze"
        );
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
}

/// The pod's stable public refusal for capacity it cannot lend right now.
///
/// Shared rather than restated per case: a contention case that matched a
/// different code would silently accept a fault as backpressure.
pub(super) const INGEST_BUSY: &str = "WYRD_VALA_429_INGEST_BUSY";

/// How long one contending append may keep retrying before it counts as stuck.
///
/// The deadline is a diagnostic bound, never an ordering device: no assertion
/// in any case depends on how long a turn took, only on whether every
/// participant eventually got one. A participant that reaches this bound has
/// not lost a race, it has been passed over indefinitely.
const ADMISSION_DEADLINE: std::time::Duration = std::time::Duration::from_secs(60);

/// Longest pause between two retries of a refused append.
///
/// A refusal is a complete public round trip, so retrying in a tight loop makes
/// request throughput an accidental input to a fairness measurement: the
/// participant that happens to be scheduled most often issues the most attempts
/// and records the most refusals. Backing off bounds that, and bounds the load
/// a waiting participant puts on the pod it is waiting for.
const ADMISSION_MAX_BACKOFF: std::time::Duration = std::time::Duration::from_millis(16);

/// Sends one append through public ingest until the pod admits it.
///
/// Capacity refusal is the one outcome this retries. Every other error is a
/// fault and fails the case immediately, so a case can never mistake a broken
/// route for a busy one. The returned count is how many times the pod refused
/// before admitting, which is the measure of whose turn the scheduler kept
/// giving away.
///
/// Retries back off geometrically to [`ADMISSION_MAX_BACKOFF`] rather than
/// spinning, and give up at [`ADMISSION_DEADLINE`] with the label and the
/// refusal count in the message.
///
/// # Panics
///
/// Panics when the append fails for any reason other than capacity pressure,
/// or when it is still refused at [`ADMISSION_DEADLINE`].
pub(super) async fn append_until_admitted(
    client: &wyrd_client::WyrdClient,
    table: &str,
    rows: &[i64],
    label: &str,
) -> usize {
    until_admitted(label, || {
        append_values(client, table, uuid::Uuid::now_v7(), rows)
    })
    .await
}

/// Retries one caller-supplied public ingest attempt until the pod admits it.
///
/// This is the bounded waiting rule every contending case shares, factored out
/// so a case that needs to record something at the moment of admission — which
/// table took the turn, which tenant, which ordinal — still waits the same way
/// rather than spinning on its own budget. `send` is called once per attempt
/// and must be a complete public round trip, so the count it returns is the
/// number of times the pod refused before admitting.
///
/// Retries back off geometrically to [`ADMISSION_MAX_BACKOFF`] rather than
/// spinning, and give up at [`ADMISSION_DEADLINE`] with the label and the
/// refusal count in the message. Nothing here measures elapsed time as
/// evidence: the deadline exists only to turn indefinite starvation into a
/// diagnosable failure.
///
/// # Panics
///
/// Panics when the attempt fails for any reason other than capacity pressure,
/// or when it is still refused at [`ADMISSION_DEADLINE`].
pub(super) async fn until_admitted<F, Fut>(label: &str, mut send: F) -> usize
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<(), wyrd_spec::error::WyrdError>>,
{
    let deadline = tokio::time::Instant::now() + ADMISSION_DEADLINE;
    let mut backoff = std::time::Duration::from_millis(1);
    let mut refusals = 0_usize;
    loop {
        match send().await {
            Ok(()) => return refusals,
            Err(error) if error.code() == INGEST_BUSY => {
                refusals += 1;
                assert!(
                    tokio::time::Instant::now() < deadline,
                    "{label} was refused {refusals} times and never admitted inside \
                     {ADMISSION_DEADLINE:?}; the pod is not rotating its capacity"
                );
                tokio::time::sleep(backoff).await;
                backoff = (backoff * 2).min(ADMISSION_MAX_BACKOFF);
            }
            Err(error) => panic!("{label} must be acknowledged: {error:?}"),
        }
    }
}

/// Builds one `vala.traces.spans` batch carrying `values` as `duration_ms`.
///
/// Every other column is a well-formed constant: the cases that send these rows
/// are about how Scribe schedules, stages and publishes the built-in, so the
/// payload only has to be a real built-in row that public ingest accepts, with
/// one column (`duration_ms`) a read-back can compare exactly.
pub(super) fn span_batch(values: &[i64]) -> arrow::record_batch::RecordBatch {
    use arrow::array::{
        FixedSizeBinaryBuilder, Int64Array, StringArray, TimestampMicrosecondArray,
    };
    let definition = vala_bifrost_redux::tables::builtin_table("traces", "spans")
        .expect("traces spans built-in");
    let schema = std::sync::Arc::new(arrow::datatypes::Schema::new((definition.arrow_fields)()));
    let rows = values.len();
    let mut trace_id = FixedSizeBinaryBuilder::with_capacity(rows, 16);
    let mut span_id = FixedSizeBinaryBuilder::with_capacity(rows, 8);
    let mut parent_span_id = FixedSizeBinaryBuilder::with_capacity(rows, 8);
    for value in values {
        let mut trace = [0_u8; 16];
        trace[..8].copy_from_slice(&value.to_be_bytes());
        trace_id.append_value(trace).expect("trace id width");
        span_id
            .append_value(value.to_be_bytes())
            .expect("span id width");
        parent_span_id.append_null();
    }
    let now = chrono::Utc::now().timestamp_micros();
    let text = |literal: &str| {
        std::sync::Arc::new(StringArray::from(vec![literal; rows])) as arrow::array::ArrayRef
    };
    let zeros =
        || std::sync::Arc::new(Int64Array::from(vec![0_i64; rows])) as arrow::array::ArrayRef;
    let stamps = || {
        std::sync::Arc::new(TimestampMicrosecondArray::from(vec![now; rows]).with_timezone("UTC"))
            as arrow::array::ArrayRef
    };
    arrow::record_batch::RecordBatch::try_new(
        schema,
        vec![
            std::sync::Arc::new(trace_id.finish()),
            std::sync::Arc::new(span_id.finish()),
            std::sync::Arc::new(parent_span_id.finish()),
            zeros(),
            text("scribe-journey"),
            text("scribe-journey"),
            text("SPAN_KIND_INTERNAL"),
            stamps(),
            stamps(),
            std::sync::Arc::new(Int64Array::from(values.to_vec())),
            text("STATUS_CODE_OK"),
            text("{}"),
            zeros(),
            zeros(),
            zeros(),
            text("scribe"),
            text("1"),
            text("wyrd-testing"),
        ],
    )
    .expect("span batch")
}

/// Returns the hour-partition boundary one event time belongs to.
///
/// The default physical layout partitions by hour, so this is the exact
/// `partition.start_utc` the promotion record must carry for rows stamped with
/// `at`.
pub(super) fn hour_start(at: chrono::DateTime<chrono::Utc>) -> chrono::DateTime<chrono::Utc> {
    use chrono::Timelike;
    at.with_minute(0)
        .and_then(|value| value.with_second(0))
        .and_then(|value| value.with_nanosecond(0))
        .expect("an hour boundary is a valid instant")
}
