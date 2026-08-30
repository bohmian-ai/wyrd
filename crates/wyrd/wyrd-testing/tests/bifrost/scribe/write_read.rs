//! The Scribe write/flush/read user journey.

use std::sync::Arc;

use arrow::array::Int64Array;
use arrow::datatypes::{DataType, Field, Schema};
use arrow::record_batch::RecordBatch;
use vala_bifrost_redux::namespaces::BifrostNamespace;
use vala_sdk::query::ValaSdkError;
use wyrd_client::config::ClientConfig;
use wyrd_client::transport::{GrpcConfig, HttpConfig};
use wyrd_spec::vala::api::{BifrostQueryRequest, QueryTerminalErrorCode, VisibilityMode};
use wyrd_testing::bifrost::{
    BifrostClusterSpec, ScribeCacheMode, ScribeCheckpointNameV1, ScribeProductionEvidenceV1,
    ScribeProductionWorkloadV1, ScribePublishedHotFileV1, ScribeStorageDrainObservationV1,
    WyrdTestCluster,
};

use super::support::{register_table, start_scribe_server, unique_table};

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

    let cluster = WyrdTestCluster::start_spec(
        BifrostClusterSpec::one_mixed().with_metadata_cache_mode(ScribeCacheMode::Disabled),
    )
    .await
    .expect("the one-pod mixed cluster starts with the cache disabled");
    assert_empty_table_reads_cleanly(cluster.server(0).expect("the mixed pod is running")).await;

    let uncached_run = cluster
        .run_scribe_production_workload(&workload, ScribeCacheMode::Disabled)
        .await
        .expect("the production workload runs on public routes with the cache disabled");
    uncached_run
        .evidence
        .assert_matches(&workload, ScribeCacheMode::Disabled)
        .expect("the uncached run satisfies every normative field of the record");

    let cached_cluster = WyrdTestCluster::start_spec(
        BifrostClusterSpec::one_mixed().with_metadata_cache_mode(ScribeCacheMode::Enabled),
    )
    .await
    .expect("a fresh one-pod mixed cluster starts with the cache enabled");
    let cached_run = cached_cluster
        .run_scribe_production_workload(&cached, ScribeCacheMode::Enabled)
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
        uncached_storage.cache_bypasses > 0 && uncached_storage.cache_bypasses_disabled > 0,
        "a disabled composition must positively record a bypass with reason disabled, observed {uncached_storage:?}"
    );
    assert_eq!(
        (
            uncached_storage.cache_hits,
            uncached_storage.cache_misses,
            uncached_storage.cache_joins
        ),
        (0, 0, 0),
        "a disabled composition must record no hit, miss, or join, observed {uncached_storage:?}"
    );
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
/// # Panics
/// Panics when the owner is not closed, its starts and terminals disagree, any
/// live count is nonzero, or any settlement was unmatched.
fn assert_settled(storage: &ScribeStorageDrainObservationV1, label: &str) {
    assert_eq!(storage.lifecycle, "closed", "{label}: {storage:?}");
    assert_eq!(
        storage.load_starts, storage.load_terminals,
        "{label}: every started metadata load must publish a terminal"
    );
    assert_eq!(
        storage.request_starts, storage.request_terminals,
        "{label}: every admitted governed request must publish a terminal"
    );
    assert_eq!(
        (
            storage.resident_entries,
            storage.resident_bytes,
            storage.inflight_loads,
            storage.waiters,
            storage.active_requests,
            storage.anomalies,
        ),
        (0, 0, 0, 0, 0, 0),
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
    let client = tenant_client(server, tenant).await;
    let mut empty = vala_sdk::query::QueryClient::new(&client)
        .query(&BifrostQueryRequest {
            sql: format!("SELECT value FROM {table}"),
            visibility: VisibilityMode::Fused,
            freshness: wyrd_spec::vala::api::FreshnessPolicy::Strict,
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

/// An undialable ready Scribe peer fails a strict fused read with its typed 503.
///
/// This drives the public SDK against an active, unflushed generation so Oracle
/// must use the private Scribe RPC. The test then replaces only the durable
/// membership address with a concrete closed loopback endpoint and refreshes
/// the production registry snapshot. No successful terminal may be returned.
///
/// # Panics
///
/// Panics when setup fails, membership advertises `:0`, the private endpoint is
/// not initially reachable, or the public SDK flattens the terminal failure.
#[tokio::test]
#[ignore = "requires Postgres and object storage"]
async fn scribe_undialable_private_peer_returns_typed_visibility_failure() {
    let server = start_scribe_server().await;
    let tenant = server.data_tenant_id();
    let table = register_table(
        &server,
        tenant,
        BifrostNamespace::Datasets,
        &unique_table("undialable_peer"),
    )
    .await;
    let client = tenant_client(&server, tenant).await;
    append_active_row(&client, &table).await;

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
    let reachable = scribe
        .address
        .strip_prefix("http://")
        .expect("test peer uses plaintext loopback");
    tokio::net::TcpStream::connect(reachable)
        .await
        .expect("advertised private Scribe endpoint is reachable before readiness");

    let closed = std::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
        .expect("reserve closed endpoint")
        .local_addr()
        .expect("closed endpoint address");
    let pool = server
        .pg_fixture()
        .superuser_pool()
        .await
        .expect("system membership pool");
    sqlx::query(
        "UPDATE vala.cluster_nodes SET advertise_addr=$1, heartbeat_at=now(), ready=true \
         WHERE data_tenant_id=$2 AND role='scribe'",
    )
    .bind(format!("http://{closed}"))
    .bind(uuid::Uuid::from(wyrd_spec::DataTenantId::SYSTEM_OWNER))
    .execute(&pool)
    .await
    .expect("inject undialable private membership");
    cluster
        .refresh_snapshot()
        .await
        .expect("Oracle observes the undialable membership");

    let started = vala_sdk::query::QueryClient::new(&client)
        .query(&BifrostQueryRequest {
            sql: format!("SELECT value FROM {table}"),
            visibility: VisibilityMode::Fused,
            freshness: wyrd_spec::vala::api::FreshnessPolicy::Strict,
            deadline_ms: Some(5_000),
        })
        .await;
    let error = match started {
        Err(error) => error,
        Ok(mut stream) => match stream.next_batch().await {
            Err(error) => error,
            Ok(Some(_)) => panic!("undialable strict read emitted a successful batch"),
            Ok(None) => panic!("undialable strict read emitted a successful terminal"),
        },
    };
    match &error {
        ValaSdkError::FailedTerminal { terminal } => {
            assert_eq!(
                terminal.outcome,
                wyrd_spec::vala::api::QueryTerminalOutcome::Failed
            );
            assert_eq!(
                terminal.error.as_ref().expect("failed terminal error").code,
                QueryTerminalErrorCode::QueryVisibilityUnavailable
            );
        }
        other => panic!("expected typed failed terminal, got {other:?}"),
    }
    assert_eq!(
        error_code(&error),
        "WYRD_VALA_503_QUERY_VISIBILITY_UNAVAILABLE"
    );

    server.shutdown().await.expect("the server drains cleanly");
}

/// Builds one public SDK client scoped to the supplied workload tenant.
async fn tenant_client(
    server: &wyrd_testing::WyrdTestServer,
    tenant: wyrd_spec::DataTenantId,
) -> wyrd_client::WyrdClient {
    let bootstrap = server
        .bootstrap_service_in_tenant(tenant, &unique_table("peer_client"), &["admin"])
        .await
        .expect("tenant service bootstrap");
    let api_key = bootstrap.api_key().expect("service API key");
    wyrd_client::WyrdClient::with_config(ClientConfig {
        grpc: GrpcConfig {
            endpoint: server.grpc_url().expect("bound gRPC URL"),
            connect_retries: 0,
            ..GrpcConfig::default()
        },
        http: HttpConfig {
            base_url: server.base_url().expect("bound HTTP URL").to_owned(),
            ..HttpConfig::default()
        },
        api_key: Some(api_key.clone()),
        ..ClientConfig::default()
    })
    .expect("tenant SDK client")
}

/// Appends one row without flushing so strict fused visibility requires Scribe.
async fn append_active_row(client: &wyrd_client::WyrdClient, table: &str) {
    let schema = Arc::new(Schema::new(vec![Field::new(
        "value",
        DataType::Int64,
        false,
    )]));
    let batch = RecordBatch::try_new(
        Arc::clone(&schema),
        vec![Arc::new(Int64Array::from(vec![7_i64]))],
    )
    .expect("active row batch");
    let mut ipc = Vec::new();
    let mut writer =
        arrow::ipc::writer::StreamWriter::try_new(&mut ipc, schema.as_ref()).expect("IPC writer");
    writer.write(&batch).expect("IPC batch");
    writer.finish().expect("IPC terminal");
    vala_sdk::grpc::BifrostGrpcTransport::connect(client)
        .await
        .expect("public ingest transport")
        .insert_batch(table, uuid::Uuid::now_v7().into_bytes(), ipc)
        .await
        .expect("active row append");
}

/// Returns the public stable code without consuming the typed SDK error.
fn error_code(error: &ValaSdkError) -> &'static str {
    error.code()
}
