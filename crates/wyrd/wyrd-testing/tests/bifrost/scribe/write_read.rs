//! The Scribe write/flush/read user journey.

use std::sync::Arc;

use arrow::array::{Array, Int64Array};
use arrow::datatypes::{DataType, Field, Schema};
use arrow::record_batch::RecordBatch;
use url::Url;
use vala_bifrost_redux::namespaces::BifrostNamespace;
use wyrd_client::config::ClientConfig;
use wyrd_client::transport::{GrpcConfig, HttpConfig};
use wyrd_spec::vala::api::BifrostQueryRequest;
use wyrd_testing::bifrost::telemetry::BifrostMetricKind;
use wyrd_testing::bifrost::{
    BifrostClusterSpec, ScribeCacheMode, ScribeCheckpointNameV1, ScribeProductionEvidenceV1,
    ScribeProductionWorkloadV1, ScribePublishedHotFileV1, ScribeStorageDrainObservationV1,
    WyrdTestCluster, shared_process_telemetry_for_test,
};

use super::support::{
    append_values, published_rows, register_table, sorted_values, start_scribe_server,
    tenant_client, unique_table,
};

/// The complete client-to-server Scribe contract, end to end.
///
/// This is the whole path a real caller takes, in the order a real caller takes
/// it: public registration, public append with a durable acknowledgment, the
/// freeze and the publication that move those rows between authoritative
/// sources, a strict public read at each of them, a replayed identical batch
/// that must not duplicate a row, a pod restart that must recover the same rows
/// under the same identities, and a terminal drain that must leak no ownership.
///
/// The record drives all of it and the comparator judges all of it. Everything
/// this journey asserts is a normative field of the canonical record, so a
/// consumer that runs the same bytes is held to the same contract; a hand
/// written epilogue here would be a second, weaker contract that only this file
/// enforces.
///
/// The record is serialized once, hashed, and re-read from those exact bytes,
/// so a field that only exists in this process cannot become part of the
/// contract and the bytes Forge and the cache task later run are provably the
/// bytes this journey ran.
///
/// The pod runs inside a one-node cluster because the last two boundaries need
/// a pod that can be stopped and started again on its retained roots, and the
/// runner takes the cluster because the record's terminal operation destroys
/// it.
///
/// # Panics
///
/// Panics when the record does not survive its wire form, or when the run does
/// not satisfy every normative field of the record: its six lifecycle
/// boundaries in order, its acknowledged-row counts, its read-back digest, the
/// publication each boundary is defined by, the recovery evidence at the
/// restart boundary, and the drain evidence at the terminal one.
#[tokio::test]
#[ignore = "requires Postgres and object storage"]
async fn scribe_write_flush_read_user_journey() {
    let canonical = ScribeProductionWorkloadV1::canonical();
    let bytes = serde_json::to_vec(&canonical).expect("the canonical record serializes");
    let digest = wyrd_testing::bifrost::scribe_workload_digest(&bytes);
    let workload: ScribeProductionWorkloadV1 =
        serde_json::from_slice(&bytes).expect("the canonical record deserializes");
    assert_eq!(workload, canonical, "the handoff must be lossless");
    assert_eq!(
        wyrd_testing::bifrost::scribe_workload_digest(
            &serde_json::to_vec(&workload).expect("the round-tripped record re-serializes")
        ),
        digest,
        "the record's wire bytes must be stable across the round trip that consumers repeat"
    );

    let cached: ScribeProductionWorkloadV1 =
        serde_json::from_slice(&bytes).expect("the same bytes deserialize for the second run");

    // Cache decisions are production counters, so the disabled composition is
    // judged over its own window: the boot reads, then the whole uncached run,
    // closed before the cached cluster starts.
    let (_telemetry_guard, telemetry) =
        shared_process_telemetry_for_test().expect("process production telemetry");
    let boot_window = telemetry.checkpoint().expect("boot telemetry window");
    let uncached_window = telemetry
        .checkpoint()
        .expect("uncached run telemetry window");
    let cluster = WyrdTestCluster::start_spec(
        BifrostClusterSpec::one_mixed().with_metadata_cache_mode(ScribeCacheMode::Disabled),
    )
    .await
    .expect("the one-pod mixed cluster starts with the cache disabled");
    assert_empty_table_reads_cleanly(cluster.server(0).expect("the mixed pod is running")).await;
    assert_canonical_genai_span_is_tenant_isolated(
        cluster.server(0).expect("the mixed pod is running"),
    )
    .await;
    // The span read above opens a freshly flushed hot object on a pod whose
    // Forge has not yet promoted it, so it is the one read that must decide
    // hot metadata. The terminal owner is the restarted pod's, and whether its
    // reads still find hot objects races Forge promotion after boot.
    let booted = cache_effects(&telemetry.delta_since(&boot_window).expect("boot delta"));
    assert!(
        booted
            .iter()
            .any(|((effect, reason), _)| effect == "bypass" && reason == "disabled"),
        "a disabled composition must positively record a bypass with reason disabled, observed {booted:?}"
    );

    let uncached_run =
        Box::pin(cluster.run_scribe_production_workload(&workload, ScribeCacheMode::Disabled))
            .await
            .expect("the production workload runs on public routes with the cache disabled");
    uncached_run
        .evidence
        .assert_matches(&workload, ScribeCacheMode::Disabled)
        .expect("the uncached run satisfies every normative field of the record");
    let uncached_effects = cache_effects(
        &telemetry
            .delta_since(&uncached_window)
            .expect("uncached run delta"),
    );

    let cached_cluster = WyrdTestCluster::start_spec(
        BifrostClusterSpec::one_mixed().with_metadata_cache_mode(ScribeCacheMode::Enabled),
    )
    .await
    .expect("a fresh one-pod mixed cluster starts with the cache enabled");
    let cached_run =
        Box::pin(cached_cluster.run_scribe_production_workload(&cached, ScribeCacheMode::Enabled))
            .await
            .expect("the same record runs on public routes with the cache enabled");
    cached_run
        .evidence
        .assert_matches(&cached, ScribeCacheMode::Enabled)
        .expect("the cached run satisfies every normative field of the record");

    assert_parity(&uncached_run.evidence, &cached_run.evidence);

    let uncached_storage = terminal_storage(&uncached_run.evidence);
    let cached_storage = terminal_storage(&cached_run.evidence);
    assert_settled(&uncached_storage, "cache disabled");
    assert_settled(&cached_storage, "cache enabled");
    assert!(
        !uncached_effects.is_empty()
            && uncached_effects
                .iter()
                .all(|((effect, reason), _)| effect == "bypass" && reason == "disabled"),
        "a disabled composition records only bypasses with reason disabled — no hit, miss, \
         or join — observed {uncached_effects:?}"
    );
}

/// Returns the nonzero metadata-cache decisions one telemetry window recorded.
///
/// Keyed by `(effect, reason)`, the family's complete closed label set.
fn cache_effects(
    delta: &wyrd_testing::bifrost::telemetry::BifrostTelemetryDelta,
) -> Vec<((String, String), f64)> {
    delta
        .metrics
        .iter()
        .filter(|sample| {
            sample.family == "bifrost_storage_metadata_cache_effects_total"
                && sample.kind == BifrostMetricKind::Counter
                && sample.value > 0.0
        })
        .map(|sample| {
            let label = |key: &str| sample.labels.get(key).cloned().unwrap_or_default();
            ((label("effect"), label("reason")), sample.value)
        })
        .collect()
}

/// One complete GenAI span survives this pod's write/flush/read path, and is
/// visible only inside the tenant that wrote it.
///
/// The canonical span ledger is a server-owned built-in, so the pod provisions
/// it and the fixture then uses the only door a caller has: the table's own
/// published description, one public Arrow batch write, the pod's own Scribe
/// publication, and a public read. A second active tenant
/// provisions the same ledger and reads the same scope, which must return
/// nothing — the ledger is shared by name, never by content.
///
/// # Panics
///
/// Panics when provisioning, describe, the write, the publication, or either
/// read fails, or when the readback is not exactly the span that was written.
async fn assert_canonical_genai_span_is_tenant_isolated(server: &wyrd_testing::WyrdTestServer) {
    use wyrd_testing::bifrost::canonical_signals as fixture;

    /// The fixed event-time anchor every canonical fixture row is written at.
    const ANCHOR: i64 = 1_760_000_000_000_000_000;
    /// The canonical span ledger both tenants provision and read.
    const SPANS: &str = "vala.traces.spans";

    let tenant = server.data_tenant_id();
    server
        .ensure_builtin_table_for_test(tenant, "traces", "spans")
        .await
        .expect("the canonical span ledger is provisioned for the writing tenant");
    let writer = tenant_writer(server, tenant).await;
    let described = wyrd_client::bifrost::TableConfig::describe(writer.client(), SPANS)
        .await
        .expect("the canonical span ledger describes itself");
    let scope = unique_table("scribe_canonical");
    writer
        .write_batch(
            SPANS,
            &fixture::spans(described.user_schema(), &scope, ANCHOR),
        )
        .await
        .expect("the canonical span batch is accepted");
    server
        .flush_bifrost()
        .await
        .expect("this pod publishes what it accepted");

    let flat = query_rows(
        &writer,
        &format!(
            "SELECT name, CAST(gen_ai_usage_input_tokens AS BIGINT) AS input_tokens, \
             CAST(status_code AS BIGINT) AS status_code FROM {SPANS} \
             WHERE scope_name = '{scope}' AND service_name = 'wyrd.fixture.service' \
             AND gen_ai_request_model = '{model}' AND resource_present AND scope_present \
             ORDER BY start_time_unix_nano",
            model = fixture::MODEL
        ),
    )
    .await;
    assert_eq!(
        text_values(&flat, "name"),
        vec![
            "chat claude-opus-5".to_owned(),
            "execute_tool search".to_owned()
        ],
        "the parent GenAI span and the tool span it made both read back"
    );
    assert_eq!(
        int_values(&flat, "input_tokens"),
        vec![fixture::INPUT_TOKENS, 64],
        "the promoted GenAI token columns read back exactly"
    );
    assert_eq!(
        int_values(&flat, "status_code"),
        vec![1, 2],
        "the child span keeps the error status it was written with"
    );

    let nested = query_rows(
        &writer,
        &format!(
            "SELECT CAST(array_length(events) AS BIGINT) AS events, \
             CAST(array_length(links) AS BIGINT) AS links FROM {SPANS} \
             WHERE scope_name = '{scope}' AND parent_span_id IS NULL"
        ),
    )
    .await;
    assert_eq!(
        (int_values(&nested, "events"), int_values(&nested, "links")),
        (vec![1], vec![1]),
        "the parent span keeps the one event and one link it was written with"
    );

    let other = server
        .seed_tenant(&unique_table("canonical_isolation"))
        .await
        .expect("a second active tenant is seeded");
    server
        .ensure_builtin_table_for_test(other, "traces", "spans")
        .await
        .expect("the canonical span ledger is provisioned for the second tenant");
    let stranger = tenant_writer(server, other).await;
    assert!(
        query_rows(
            &stranger,
            &format!("SELECT name FROM {SPANS} WHERE scope_name = '{scope}'"),
        )
        .await
        .is_empty(),
        "a second tenant reading the same canonical ledger sees no row of the first"
    );
}

/// Drain one public read into its batches.
///
/// # Panics
///
/// Panics when the query cannot start or does not reach a successful terminal.
async fn query_rows(
    writer: &wyrd_testing::bifrost::write::BifrostWriter,
    sql: &str,
) -> Vec<RecordBatch> {
    let mut stream = wyrd_client::Bifrost::query_only(writer.client())
        .query(&BifrostQueryRequest {
            sql: sql.to_owned(),
            deadline_ms: Some(60_000),
        })
        .await
        .expect("the public query starts");
    let mut batches = Vec::new();
    while let Some(batch) = stream
        .next_batch()
        .await
        .expect("the public query reaches its successful terminal")
    {
        batches.push(batch);
    }
    batches
}

/// Collect one `Utf8` column's values across every batch, in order.
///
/// # Panics
///
/// Panics when the column is absent or is not `Utf8`.
fn text_values(batches: &[RecordBatch], name: &str) -> Vec<String> {
    let mut values = Vec::new();
    for batch in batches {
        let array = batch
            .column_by_name(name)
            .unwrap_or_else(|| panic!("column `{name}`"))
            .as_any()
            .downcast_ref::<arrow::array::StringArray>()
            .unwrap_or_else(|| panic!("column `{name}` is not Utf8"));
        values.extend((0..array.len()).map(|index| array.value(index).to_owned()));
    }
    values
}

/// Collect one `Int64` column's values across every batch, in order.
///
/// # Panics
///
/// Panics when the column is absent, is not `Int64`, or carries a null.
fn int_values(batches: &[RecordBatch], name: &str) -> Vec<i64> {
    let mut values = Vec::new();
    for batch in batches {
        let array = batch
            .column_by_name(name)
            .unwrap_or_else(|| panic!("column `{name}`"))
            .as_any()
            .downcast_ref::<Int64Array>()
            .unwrap_or_else(|| panic!("column `{name}` is not Int64"));
        for index in 0..array.len() {
            assert!(array.is_valid(index), "column `{name}` carries no null");
            values.push(array.value(index));
        }
    }
    values
}

/// Returns the terminal boundary's storage-owner observation.
///
/// # Panics
/// Panics when the terminal boundary carries no drain or no storage evidence.
fn terminal_storage(evidence: &ScribeProductionEvidenceV1) -> ScribeStorageDrainObservationV1 {
    evidence
        .checkpoint(ScribeCheckpointNameV1::TerminalDrain)
        .expect("the record reaches its terminal boundary")
        .drained
        .as_ref()
        .expect("the terminal boundary carries drain evidence")
        .storage
        .clone()
        .expect("a composed pod publishes its terminal storage-owner snapshot")
}

/// Asserts one run's storage owner closed and retained nothing.
///
/// Every value is read from the owners themselves at teardown, so a nonzero
/// count is work the owner still holds rather than a tally that drifted.
///
/// # Panics
/// Panics when the owner is not closed or any live count is nonzero.
fn assert_settled(storage: &ScribeStorageDrainObservationV1, label: &str) {
    assert_eq!(storage.lifecycle, "closed", "{label}: {storage:?}");
    assert_eq!(
        (
            storage.resident_entries,
            storage.resident_bytes,
            storage.inflight_loads,
            storage.waiters,
            storage.active_requests,
        ),
        (0, 0, 0, 0, 0),
        "{label}: a drained owner must retain nothing, observed {storage:?}"
    );
}

/// Requires two runs of the same bytes to have produced the same public result.
///
/// The comparison is deliberately over identity-free shape. Two runs happen on
/// two fresh clusters, so their tenant UUIDs, SQL row UUIDs, object keys, file
/// checksums, and encoded byte sizes are necessarily different and comparing
/// them would only assert that two temporary directories differ. What must not
/// differ is everything a caller can observe: the boundaries reached and their
/// order, the rows acknowledged at each, the digest of the rows read back, how
/// many objects each tenant published and the row and row-group shape of each,
/// the recovery a restart produced, and the drain each pod completed. The
/// intentional `cache_mode` field and the cache counters are the only other
/// exclusions, because those are precisely what the two compositions differ in.
///
/// # Panics
/// Panics on the first observable difference between the two runs.
fn assert_parity(uncached: &ScribeProductionEvidenceV1, cached: &ScribeProductionEvidenceV1) {
    assert_eq!(
        uncached.version, cached.version,
        "both runs execute the same contract version"
    );
    let names = |evidence: &ScribeProductionEvidenceV1| {
        evidence
            .checkpoints
            .iter()
            .map(|checkpoint| checkpoint.name)
            .collect::<Vec<_>>()
    };
    assert_eq!(
        names(uncached),
        names(cached),
        "both runs must reach the same boundaries in the same order"
    );
    for (left, right) in uncached.checkpoints.iter().zip(&cached.checkpoints) {
        assert_eq!(
            left.acknowledged_rows, right.acknowledged_rows,
            "boundary {:?} acknowledged different row counts",
            left.name
        );
        assert_eq!(
            left.observed_row_digest, right.observed_row_digest,
            "boundary {:?} read back different rows",
            left.name
        );
        assert_eq!(
            left.recovered, right.recovered,
            "boundary {:?} produced different recovery evidence",
            left.name
        );
        assert_eq!(
            publication_shape(left.published.values()),
            publication_shape(right.published.values()),
            "boundary {:?} published a different set of objects",
            left.name
        );
        match (left.drained.as_ref(), right.drained.as_ref()) {
            (None, None) => {}
            (Some(left_drain), Some(right_drain)) => {
                assert_eq!(
                    (
                        left_drain.servers_stopped,
                        left_drain.listeners_stopped,
                        left_drain.admitted,
                        left_drain.queued,
                        left_drain.wal_streams,
                        left_drain.supervised_tasks,
                    ),
                    (
                        right_drain.servers_stopped,
                        right_drain.listeners_stopped,
                        right_drain.admitted,
                        right_drain.queued,
                        right_drain.wal_streams,
                        right_drain.supervised_tasks,
                    ),
                    "boundary {:?} drained differently",
                    left.name
                );
            }
            _ => panic!(
                "boundary {:?} carries drain evidence in only one run",
                left.name
            ),
        }
    }
}

/// Projects one boundary's publication into its identity-free shape.
///
/// Tenant slugs, object keys, checksums, and byte sizes are all fixture- or
/// content-derived and cannot match across fresh clusters. What survives is the
/// number of objects each tenant published and, per object, the logical table
/// it belongs to, its Iceberg spec identities, its row count, its row-group
/// count, and its per-field value and null counts — the shape a reader sees.
fn publication_shape<'a>(
    published: impl Iterator<Item = &'a Vec<ScribePublishedHotFileV1>>,
) -> (Vec<usize>, Vec<PublishedFileShape>) {
    let mut counts = Vec::new();
    let mut shapes = Vec::new();
    for files in published {
        counts.push(files.len());
        for file in files {
            shapes.push(PublishedFileShape {
                namespace: file.namespace.clone(),
                table_name: file.table_name.clone(),
                partition_spec_id: file.partition_spec_id,
                sort_order_id: file.sort_order_id,
                record_count: file.data_file.record_count,
                row_groups: file.data_file.split_offsets.len(),
                value_counts: file.data_file.value_counts.clone(),
                null_value_counts: file.data_file.null_value_counts.clone(),
            });
        }
    }
    counts.sort_unstable();
    shapes.sort();
    (counts, shapes)
}

/// One published object's identity-free, cluster-independent shape.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct PublishedFileShape {
    /// Logical namespace the object belongs to.
    namespace: String,
    /// Logical table the object belongs to.
    table_name: String,
    /// Iceberg partition-spec identity the object was written under.
    partition_spec_id: i32,
    /// Iceberg sort-order identity the object was written under.
    sort_order_id: i32,
    /// Rows the object contains.
    record_count: u64,
    /// Row groups the object was sealed with.
    row_groups: usize,
    /// Values, including nulls, per Iceberg field id.
    value_counts: std::collections::BTreeMap<i32, u64>,
    /// Null values per Iceberg field id.
    null_value_counts: std::collections::BTreeMap<i32, u64>,
}

/// Asserts an unwritten table's strict read reaches a clean empty terminal.
async fn assert_empty_table_reads_cleanly(server: &wyrd_testing::WyrdTestServer) {
    let tenant = server.data_tenant_id();
    let table = register_table(
        server,
        tenant,
        BifrostNamespace::Datasets,
        &unique_table("empty_query"),
    )
    .await;
    let reader = tenant_writer(server, tenant).await;
    let mut empty = wyrd_client::Bifrost::query_only(reader.client())
        .query(&BifrostQueryRequest {
            sql: format!("SELECT value FROM {table}"),
            deadline_ms: Some(60_000),
        })
        .await
        .expect("empty public query starts");
    assert!(
        empty
            .next_batch()
            .await
            .expect("empty public query reaches its successful terminal")
            .is_none(),
        "empty logical results must not emit zero-row batch frames"
    );
}

/// An undialable ready Scribe peer degrades a public read to known live loss.
///
/// This drives the public SDK against an active, unflushed generation on a
/// peer-enabled node, so Oracle must use the private Scribe RPC. The test then replaces only the durable
/// membership address with a concrete closed loopback endpoint and refreshes
/// the production registry snapshot. The known Scribe is unreachable before
/// any row, so the query ends `Degraded` with `LiveTailUnavailable` and returns
/// none of the unreachable live rows; it is never reported as complete. The
/// production metric window records that terminal as one Degraded Gate
/// stream and one Degraded Oracle execution, with no Success sample.
///
/// # Panics
///
/// Panics when setup fails, the HTTPS endpoint is invalid or unreachable,
/// fault injection changes its host/scheme, or the public query fails, emits a
/// live row, or ends without the degraded terminal.
#[tokio::test]
#[ignore = "requires Postgres and object storage"]
async fn scribe_undialable_private_peer_degrades_live_coverage() {
    // Peer mode, because only a peer-enabled node advertises a dialable
    // private address and reaches even its own Scribe through that transport;
    // a process-local node calls its Scribe in process and has nothing to break.
    let (_telemetry_guard, telemetry) =
        shared_process_telemetry_for_test().expect("process production telemetry");
    let peer_root = tempfile::tempdir().expect("peer TLS root");
    let peer_tls = wyrd_testing::bifrost::peer_ca::BifrostPeerCa::generate(
        wyrd_server::config::PEER_SERVER_NAME,
    )
    .expect("peer CA")
    .materialize(peer_root.path(), "undialable")
    .expect("peer TLS material");
    let server = wyrd_testing::WyrdTestServer::builder()
        .with_peer_tls(peer_tls)
        .start_bound()
        .await
        .expect("the peer-enabled Scribe harness starts");
    let tenant = server.data_tenant_id();
    let table = register_table(
        &server,
        tenant,
        BifrostNamespace::Datasets,
        &unique_table("undialable_peer"),
    )
    .await;
    let writer = tenant_writer(&server, tenant).await;
    append_active_row(&writer, &table).await;

    let cluster = server
        .state()
        .oracle_cluster()
        .expect("mixed server owns the Oracle cluster registry");
    cluster
        .refresh_snapshot()
        .await
        .expect("membership snapshot refreshes");
    let snapshot = cluster.snapshot();
    let scribe = snapshot
        .live_scribes()
        .into_iter()
        .next()
        .expect("one ready Scribe membership");
    assert!(!scribe.address.ends_with(":0"));
    let mut endpoint = Url::parse(&scribe.address).expect("private peer advertises a URL");
    assert_eq!(endpoint.scheme(), "https");
    let host = endpoint
        .host_str()
        .expect("private peer URL has a host")
        .to_owned();
    let port = endpoint
        .port()
        .expect("private peer URL has an explicit port");
    tokio::net::TcpStream::connect((host.as_str(), port))
        .await
        .expect("advertised private Scribe endpoint is reachable before readiness");

    let closed = std::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
        .expect("reserve closed endpoint")
        .local_addr()
        .expect("closed endpoint address");
    endpoint
        .set_port(Some(closed.port()))
        .expect("private peer URL accepts a port");
    assert_eq!(endpoint.scheme(), "https");
    assert_eq!(endpoint.host_str(), Some(host.as_str()));
    assert!(
        tokio::net::TcpStream::connect((host.as_str(), closed.port()))
            .await
            .is_err()
    );
    let pool = server
        .pg_fixture()
        .superuser_pool()
        .expect("system membership pool");
    sqlx::query(
        "UPDATE vala.cluster_nodes SET advertise_addr=$1, heartbeat_at=now(), ready=true \
         WHERE data_tenant_id=$2 AND role='scribe'",
    )
    .bind(endpoint.as_str())
    .bind(uuid::Uuid::from(wyrd_spec::DataTenantId::SYSTEM_OWNER))
    .execute(&pool)
    .await
    .expect("inject undialable private membership");
    cluster
        .refresh_snapshot()
        .await
        .expect("Oracle observes the undialable membership");

    let window = telemetry.checkpoint().expect("query telemetry window");
    let mut stream = wyrd_client::Bifrost::query_only(writer.client())
        .query(&BifrostQueryRequest {
            sql: format!("SELECT value FROM {table}"),
            deadline_ms: Some(5_000),
        })
        .await
        .expect("a known live loss does not refuse the query");
    while let Some(batch) = stream
        .next_batch()
        .await
        .expect("a known live loss reaches a degraded terminal")
    {
        assert_eq!(
            batch.num_rows(),
            0,
            "an unreachable Scribe emitted a live row"
        );
    }
    let terminal = stream.terminal().expect("degraded terminal");
    assert_eq!(
        terminal.outcome,
        wyrd_spec::vala::api::QueryTerminalOutcome::Degraded
    );
    assert_eq!(
        terminal.warnings,
        vec![wyrd_spec::vala::api::QueryWarning::LiveTailUnavailable]
    );
    // Both owners record their terminal before the terminal frame is yielded,
    // so the window already holds them: the Degraded result is its own
    // outcome on the Gate stream and on the Oracle execution, never Success.
    let delta = telemetry
        .delta_since(&window)
        .expect("query telemetry delta");
    let outcomes = |family: &str, kind: BifrostMetricKind| -> Vec<(String, f64)> {
        delta
            .metrics
            .iter()
            .filter(|sample| sample.family == family && sample.kind == kind && sample.value != 0.0)
            .map(|sample| {
                (
                    sample.labels.get("outcome").cloned().unwrap_or_default(),
                    sample.value,
                )
            })
            .collect()
    };
    assert_eq!(
        outcomes(
            "bifrost_gate_query_streams_total",
            BifrostMetricKind::Counter
        ),
        vec![("degraded".to_owned(), 1.0)],
        "the Gate stream terminal is exactly one Degraded outcome"
    );
    assert_eq!(
        outcomes(
            "oracle_query_duration_seconds",
            BifrostMetricKind::HistogramCount
        ),
        vec![("degraded".to_owned(), 1.0)],
        "the Oracle execution is exactly one Degraded observation"
    );

    server.shutdown().await.expect("the server drains cleanly");
}

/// Builds one public SDK client scoped to the supplied workload tenant.
async fn tenant_writer(
    server: &wyrd_testing::WyrdTestServer,
    tenant: wyrd_spec::DataTenantId,
) -> wyrd_testing::bifrost::write::BifrostWriter {
    let bootstrap = server
        .bootstrap_service_in_tenant(tenant, &unique_table("peer_client"), &["admin"])
        .await
        .expect("tenant service bootstrap");
    let api_key = bootstrap.api_key().expect("service API key").clone();
    let card_ref = bootstrap
        .card_ref()
        .expect("service bootstrap has a Card scope")
        .clone();
    wyrd_testing::bifrost::write::BifrostWriter::connect(
        ClientConfig {
            grpc: GrpcConfig {
                endpoint: server.grpc_url().expect("bound gRPC URL"),
                connect_retries: 0,
                ..GrpcConfig::default()
            },
            http: HttpConfig {
                base_url: server.base_url().expect("bound HTTP URL").to_owned(),
                ..HttpConfig::default()
            },
            credential: Some(api_key),
            ..ClientConfig::default()
        },
        card_ref,
    )
    .await
    .expect("tenant SDK write door")
}

/// Appends one row without flushing so the read needs the live Scribe tier.
async fn append_active_row(writer: &wyrd_testing::bifrost::write::BifrostWriter, table: &str) {
    let schema = Arc::new(Schema::new(vec![Field::new(
        "value",
        DataType::Int64,
        false,
    )]));
    writer
        .write(table, &schema, [br#"{"value": 7}"#.to_vec()])
        .await
        .expect("active row append");
}

/// Optional and scoped Card correlation, end to end through the public routes.
///
/// A real writer supplies `card_ref` per row: some rows carry none, some carry
/// the principal's own root Card, and some carry a component Card the mint
/// signed into the same scope. The server stamps only the UID the mint signed
/// onto that exact identity, and it does so with no ingest Card resolver of any
/// kind — the pod runs the default production composition, so a stamped UID
/// that matches the registry row can only have come from the signed claim.
///
/// # Panics
///
/// Panics when an uncorrelated row is stamped, when a correlated row does not
/// carry its own registry UID, when the publisher is not the writing principal,
/// or when an out-of-scope Card is admitted.
#[tokio::test]
#[ignore = "requires Postgres and object storage"]
async fn scribe_optional_and_scoped_card_correlation_journey() {
    let server = start_scribe_server().await;
    let tenant = server.data_tenant_id();
    let table = register_table(
        &server,
        tenant,
        BifrostNamespace::Datasets,
        &unique_table("card_correlation"),
    )
    .await;

    let bootstrap = server
        .bootstrap_service_in_tenant(tenant, &unique_table("card_writer"), &["admin"])
        .await
        .expect("the writing service bootstraps");
    let root = bootstrap
        .card_ref()
        .expect("a service principal is bound to a root Card")
        .clone();
    let principal_id = bootstrap.id();
    let api_key = bootstrap.api_key().expect("service API key").clone();
    let secondary = declare_component_card(&server, &root).await;

    let client = wyrd_client::WyrdClient::with_config(ClientConfig {
        grpc: GrpcConfig {
            endpoint: server.grpc_url().expect("bound gRPC URL"),
            connect_retries: 0,
            ..GrpcConfig::default()
        },
        http: HttpConfig {
            base_url: server.base_url().expect("bound HTTP URL").to_owned(),
            ..HttpConfig::default()
        },
        credential: Some(api_key),
        ..ClientConfig::default()
    })
    .expect("the writer's SDK client");

    let identity = |card: &wyrd_spec::reference::CardRef| {
        wyrd_spec::reference::CardRef {
            uid: None,
            ..card.clone()
        }
        .to_string()
    };
    append_correlated(
        &client,
        &table,
        &[
            (1, None),
            (2, Some(identity(&root))),
            (3, Some(identity(&secondary))),
        ],
    )
    .await
    .expect("optional and scoped correlations are admitted");

    let stamped = read_correlation(&client, &table).await;
    let root_uid = registry_card_uid(&server, &root).await;
    let secondary_uid = registry_card_uid(&server, &secondary).await;
    assert_eq!(
        stamped,
        vec![
            (1, None, principal_id.to_string()),
            (2, Some(root_uid), principal_id.to_string()),
            (3, Some(secondary_uid), principal_id.to_string()),
        ],
        "each row keeps its own signed Card UID and the exact publishing principal"
    );

    let refusal = append_correlated(
        &client,
        &table,
        &[(4, Some("prod/Service/other@1.0.0".to_owned()))],
    )
    .await
    .expect_err("an out-of-scope Card is refused");
    let refused_code = match &refusal {
        wyrd_spec::error::WyrdError::UpstreamFailure { details, .. } => details
            .get("original_code")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .to_owned(),
        other => other.code().to_owned(),
    };
    assert_eq!(
        refused_code, "WYRD_VALA_403_BIFROST_CARD_SCOPE",
        "the refusal is the typed scope denial, got {refusal:?}"
    );
    assert_eq!(
        read_correlation(&client, &table)
            .await
            .iter()
            .map(|(value, _, _)| *value)
            .collect::<Vec<_>>(),
        vec![1, 2, 3],
        "the refused batch left no row behind"
    );

    server.shutdown().await.expect("the server drains cleanly");
}

/// Declare one component Card on the writer's root Service Card.
///
/// The mint walk signs every observation-target component of the root spec into
/// the token's Card scope, so this is what gives the journey a second, non-root
/// scope member to correlate against. Returns the component's `CardRef`.
///
/// # Panics
///
/// Panics when the component Card cannot be registered or the root spec cannot
/// be rewritten to declare it.
async fn declare_component_card(
    server: &wyrd_testing::WyrdTestServer,
    root: &wyrd_spec::reference::CardRef,
) -> wyrd_spec::reference::CardRef {
    use wyrd_spec::envelope::{CardKind, Spec};

    let component = wyrd_spec::reference::CardRef {
        uid: Some(
            wyrd_spec::ids::CardUid::new(uuid::Uuid::now_v7().to_string()).expect("component uid"),
        ),
        name: wyrd_spec::ids::CardName::new(unique_table("component")).expect("component name"),
        ..root.clone()
    };
    let component_spec = Spec::from_kind_and_value(&CardKind::Service, serde_json::json!({}))
        .expect("the component Service spec is valid");
    let root_spec = Spec::from_kind_and_value(
        &CardKind::Service,
        serde_json::json!({
            "components": [{
                "alias": "component",
                "ref": wyrd_spec::reference::CardRef { uid: None, ..component.clone() },
            }],
        }),
    )
    .expect("the root Service spec declares its component");

    let pool = server.pg_fixture().superuser_pool().expect("registry pool");
    let (component_hash, _) = component_spec
        .canonical_hash_with_bytes()
        .expect("component spec hashes");
    sqlx::query(
        "INSERT INTO wyrd.cards (card_uid, data_tenant_id, kind, space, name, version, spec, \
         spec_hash, artifact_hash, labels, annotations, status, created_by) \
         SELECT $1, root.data_tenant_id, $2, $3, $4, $5, $6, $7, NULL, '{}'::jsonb, '{}'::jsonb, \
         'active', root.created_by FROM wyrd.cards root WHERE root.card_uid = $8",
    )
    .bind(
        component
            .uid
            .as_ref()
            .expect("component uid is set")
            .as_uuid(),
    )
    .bind(CardKind::Service.wire_name())
    .bind(
        component
            .space
            .as_ref()
            .expect("fixture card ref carries a space")
            .as_str(),
    )
    .bind(component.name.as_str())
    .bind(component.version.as_str())
    .bind(serde_json::to_value(&component_spec).expect("component spec encodes"))
    .bind(component_hash.as_str())
    .bind(root.uid.as_ref().expect("root uid is set").as_uuid())
    .execute(&pool)
    .await
    .expect("the component Card registers");

    let (root_hash, _) = root_spec
        .canonical_hash_with_bytes()
        .expect("root spec hashes");
    // A Card's spec is immutable, so the root is re-registered under the same
    // identity carrying the component declaration the mint walk must follow.
    sqlx::query("DELETE FROM wyrd.cards WHERE card_uid = $1")
        .bind(root.uid.as_ref().expect("root uid is set").as_uuid())
        .execute(&pool)
        .await
        .expect("the authored root Card is retired");
    sqlx::query(
        "INSERT INTO wyrd.cards (card_uid, data_tenant_id, kind, space, name, version, spec, \
         spec_hash, artifact_hash, labels, annotations, status, created_by) \
         SELECT $1, component.data_tenant_id, $2, $3, $4, $5, $6, $7, NULL, '{}'::jsonb, \
         '{}'::jsonb, 'active', component.created_by FROM wyrd.cards component \
         WHERE component.card_uid = $8",
    )
    .bind(root.uid.as_ref().expect("root uid is set").as_uuid())
    .bind(CardKind::Service.wire_name())
    .bind(
        root.space
            .as_ref()
            .expect("fixture card ref carries a space")
            .as_str(),
    )
    .bind(root.name.as_str())
    .bind(root.version.as_str())
    .bind(serde_json::to_value(&root_spec).expect("root spec encodes"))
    .bind(root_hash.as_str())
    .bind(
        component
            .uid
            .as_ref()
            .expect("component uid is set")
            .as_uuid(),
    )
    .execute(&pool)
    .await
    .expect("the root Card declares its component");

    component
}

/// Read one Card's registry UID exactly as the mint walk resolved it.
///
/// # Panics
///
/// Panics when the Card is absent from the tenant registry.
async fn registry_card_uid(
    server: &wyrd_testing::WyrdTestServer,
    card: &wyrd_spec::reference::CardRef,
) -> String {
    let pool = server.pg_fixture().superuser_pool().expect("registry pool");
    let uid: uuid::Uuid = sqlx::query_scalar(
        "SELECT card_uid FROM wyrd.cards WHERE kind = $1 AND space = $2 AND name = $3 \
         AND version = $4",
    )
    .bind(card.kind.wire_name())
    .bind(
        card.space
            .as_ref()
            .expect("fixture card ref carries a space")
            .as_str(),
    )
    .bind(card.name.as_str())
    .bind(card.version.as_str())
    .fetch_one(&pool)
    .await
    .expect("the Card is registered");
    uid.to_string()
}

/// Append one batch whose rows carry the caller's own optional `card_ref`.
///
/// # Errors
///
/// Returns the stable Wyrd error the public ingest route produced.
async fn append_correlated(
    client: &wyrd_client::WyrdClient,
    table: &str,
    rows: &[(i64, Option<String>)],
) -> Result<(), wyrd_spec::error::WyrdError> {
    let schema = Arc::new(Schema::new(vec![
        Field::new("value", DataType::Int64, false),
        Field::new(
            wyrd_spec::vala::managed_columns::CARD_REF,
            DataType::Utf8,
            true,
        ),
    ]));
    let batch = RecordBatch::try_new(
        Arc::clone(&schema),
        vec![
            Arc::new(Int64Array::from(
                rows.iter().map(|(value, _)| *value).collect::<Vec<_>>(),
            )),
            Arc::new(arrow::array::StringArray::from(
                rows.iter()
                    .map(|(_, card)| card.clone())
                    .collect::<Vec<_>>(),
            )),
        ],
    )
    .expect("correlated batch");
    let mut ipc = Vec::new();
    let mut writer =
        arrow::ipc::writer::StreamWriter::try_new(&mut ipc, schema.as_ref()).expect("IPC writer");
    writer.write(&batch).expect("IPC batch");
    writer.finish().expect("IPC terminal");
    wyrd_testing::bifrost::write::RawIngest::connect(client)
        .await
        .expect("public ingest transport")
        .insert(table, uuid::Uuid::now_v7(), ipc)
        .await
}

/// Read every row's value, stamped Card UID, and publishing principal.
///
/// # Panics
///
/// Panics when the public read fails or a managed column is missing or
/// carries the wrong Arrow type.
async fn read_correlation(
    client: &wyrd_client::WyrdClient,
    table: &str,
) -> Vec<(i64, Option<String>, String)> {
    let sql = format!("SELECT value, card_uid, principal_id FROM {table} ORDER BY value");
    let mut stream = wyrd_client::Bifrost::query_only(client)
        .query(&BifrostQueryRequest {
            sql: sql.clone(),
            deadline_ms: Some(120_000),
        })
        .await
        .unwrap_or_else(|error| panic!("public query `{sql}` starts: {error}"));
    let mut rows = Vec::new();
    while let Some(batch) = stream
        .next_batch()
        .await
        .unwrap_or_else(|error| panic!("public query `{sql}` streams: {error}"))
    {
        let values = batch
            .column_by_name("value")
            .and_then(|column| column.as_any().downcast_ref::<Int64Array>())
            .expect("value is Int64");
        let uids = batch
            .column_by_name("card_uid")
            .and_then(|column| column.as_any().downcast_ref::<arrow::array::StringArray>())
            .expect("card_uid is Utf8");
        let principals = batch
            .column_by_name("principal_id")
            .and_then(|column| column.as_any().downcast_ref::<arrow::array::StringArray>())
            .expect("principal_id is Utf8");
        for row in 0..batch.num_rows() {
            rows.push((
                values.value(row),
                (!uids.is_null(row)).then(|| uids.value(row).to_owned()),
                principals.value(row).to_owned(),
            ));
        }
    }
    rows.sort_by_key(|(value, _, _)| *value);
    rows
}

/// Rows in a near-maximum legal request.
///
/// Scribe materializes about 200 bytes per `value` row once the managed columns
/// are added, and refuses a request whose material exceeds the 64-MiB default
/// ceiling; 300,000 rows stays just inside it.
const MAX_REQUEST_ROWS: i64 = 300_000;

/// An acknowledged near-maximum request stays readable through stage pressure,
/// publishes once on retry, retires its WAL, and survives restart.
///
/// The acknowledgement is the durable promise: WAL sync and memtable insertion
/// happen before it, and nothing that follows may retract it. The case fills
/// the pod's one shared root after the acknowledgement so the stage attempt
/// cannot charge its sorted candidate. That attempt must settle promptly as a
/// retained generation and leave the rows where they were — live, unpublished,
/// and WAL-backed — rather than park on a predicted workspace. Once the root
/// has room, the retried stage publishes the rows exactly once and retires the
/// WAL segments that backed the acknowledgement. The pod is then restarted on
/// the same data root, and a resend of the acknowledged batch must neither
/// duplicate the rows nor publish them again.
///
/// # Panics
///
/// Panics when the near-maximum request is refused, when a stage under a full
/// root publishes or loses rows, when the retry does not publish every row
/// once, when the acknowledged WAL is not retired, or when restart or a resent
/// batch changes the rows.
#[tokio::test]
#[ignore = "requires Postgres and object storage"]
async fn acknowledged_rows_survive_stage_pressure_and_restart() {
    let data_root = tempfile::tempdir().expect("durable Bifrost data root");
    let builder = || {
        wyrd_testing::WyrdTestServer::builder()
            .with_durable_bifrost_data_root(data_root.path().to_path_buf())
    };
    let server = builder()
        .start_bound()
        .await
        .expect("the pod starts on a durable data root");
    let tenant = server.data_tenant_id();
    let name = unique_table("stage_pressure_restart");
    let table = register_table(&server, tenant, BifrostNamespace::Datasets, &name).await;
    let client = tenant_client(&server, tenant).await;

    let batch_id = uuid::Uuid::now_v7();
    let rows: Vec<i64> = (0..MAX_REQUEST_ROWS).collect();
    append_values(&client, &table, batch_id, &rows)
        .await
        .expect("the near-maximum legal request is acknowledged");
    assert_eq!(sorted_values(&client, &table).await, rows);
    let wal_root = server
        .scribe_wal_root_for_test()
        .expect("the pod composes a WAL")
        .to_path_buf();
    let acknowledged_segments = wal_segments(&wal_root);
    let acknowledged_bytes: u64 = acknowledged_segments.iter().map(|(_, bytes)| bytes).sum();
    assert!(
        acknowledged_bytes >= (MAX_REQUEST_ROWS as u64) * 8,
        "the acknowledgement follows WAL sync of the whole request: {acknowledged_bytes} bytes"
    );

    let occupant = server
        .state()
        .bifrost_resources()
        .and_then(vala_bifrost_redux::resources::BifrostRoleResources::forge)
        .expect("the embedded pod hosts Forge")
        .occupy_root_for_test();
    tokio::time::timeout(std::time::Duration::from_mins(1), server.flush_bifrost())
        .await
        .expect("a stage under a full root settles promptly instead of waiting for bytes")
        .expect("a refused stage attempt retains its generation rather than failing the pod");
    assert_eq!(
        published_rows(&server, tenant, &name).await,
        0,
        "a failed stage attempt may not publish"
    );
    drop(occupant);
    assert_eq!(
        sorted_values(&client, &table).await,
        rows,
        "the live authority keeps every acknowledged row after a failed stage"
    );

    server
        .flush_bifrost()
        .await
        .expect("the retried stage publishes the retained rows");
    assert_eq!(
        published_rows(&server, tenant, &name).await,
        MAX_REQUEST_ROWS as u64,
        "publication accounts for every acknowledged row exactly once"
    );
    assert_eq!(sorted_values(&client, &table).await, rows);
    let retained: Vec<_> = acknowledged_segments
        .iter()
        .filter(|(path, _)| path.exists())
        .collect();
    assert!(
        retained.is_empty(),
        "published rows retire the WAL segments that backed them: {retained:?}"
    );

    let server = Box::pin(server.restart_bound(builder()))
        .await
        .expect("the pod restarts on its retained data root");
    let client = tenant_client(&server, tenant).await;
    assert_eq!(
        sorted_values(&client, &table).await,
        rows,
        "restart serves exactly the acknowledged rows"
    );
    let resent = append_values(&client, &table, batch_id, &rows).await;
    assert_eq!(
        sorted_values(&client, &table).await,
        rows,
        "a resent acknowledged batch may not duplicate its rows: {resent:?}"
    );
    server
        .flush_bifrost()
        .await
        .expect("the restarted pod drains its staged work");
    assert_eq!(
        published_rows(&server, tenant, &name).await,
        MAX_REQUEST_ROWS as u64,
        "a resent acknowledged batch is not published a second time"
    );

    server.shutdown().await.expect("the server drains cleanly");
}

/// Lists every WAL segment file under `root` with its size in bytes.
///
/// # Panics
///
/// Panics when the WAL tree cannot be walked.
fn wal_segments(root: &std::path::Path) -> Vec<(std::path::PathBuf, u64)> {
    let mut segments = Vec::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(directory) = pending.pop() {
        for entry in std::fs::read_dir(&directory).expect("the WAL tree is readable") {
            let entry = entry.expect("a WAL entry is readable");
            let metadata = entry.metadata().expect("WAL entry metadata");
            if metadata.is_dir() {
                pending.push(entry.path());
            } else if entry.path().extension().is_some_and(|ext| ext == "wal") {
                segments.push((entry.path(), metadata.len()));
            }
        }
    }
    segments
}
