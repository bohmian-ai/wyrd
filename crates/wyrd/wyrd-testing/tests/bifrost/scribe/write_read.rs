//! The Scribe write/flush/read user journey.

use std::sync::Arc;

use arrow::array::{Array, Int64Array};
use arrow::datatypes::{DataType, Field, Schema};
use arrow::record_batch::RecordBatch;
use url::Url;
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
    let reader = tenant_writer(server, tenant).await;
    let mut empty = vala_sdk::query::QueryClient::new(reader.client())
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
/// Panics when setup fails, the HTTPS endpoint is invalid or unreachable,
/// fault injection changes its host/scheme, or the public SDK loses the typed
/// visibility refusal before or after the response stream opens.
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
        .await
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

    let started = vala_sdk::query::QueryClient::new(writer.client())
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
    assert_eq!(error.code(), "WYRD_VALA_503_QUERY_VISIBILITY_UNAVAILABLE");
    assert_eq!(error.status(), 503);
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
        ValaSdkError::Transport(wyrd_spec::error::WyrdError::Vala {
            error: wyrd_spec::vala::error::BifrostError::QueryVisibilityUnavailable,
        }) => {}
        other => panic!("expected typed visibility refusal, got {other:?}"),
    }
    assert_eq!(
        error_code(&error),
        "WYRD_VALA_503_QUERY_VISIBILITY_UNAVAILABLE"
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

/// Appends one row without flushing so strict fused visibility requires Scribe.
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

/// Returns the public stable code without consuming the typed SDK error.
fn error_code(error: &ValaSdkError) -> &'static str {
    error.code()
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

    let pool = server
        .pg_fixture()
        .superuser_pool()
        .await
        .expect("registry pool");
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
    .bind(component.space.as_str())
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
    .bind(root.space.as_str())
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
    let pool = server
        .pg_fixture()
        .superuser_pool()
        .await
        .expect("registry pool");
    let uid: uuid::Uuid = sqlx::query_scalar(
        "SELECT card_uid FROM wyrd.cards WHERE kind = $1 AND space = $2 AND name = $3 \
         AND version = $4",
    )
    .bind(card.kind.wire_name())
    .bind(card.space.as_str())
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
/// Panics when the strict fused read fails or a managed column is missing or
/// carries the wrong Arrow type.
async fn read_correlation(
    client: &wyrd_client::WyrdClient,
    table: &str,
) -> Vec<(i64, Option<String>, String)> {
    let sql = format!("SELECT value, card_uid, principal_id FROM {table} ORDER BY value");
    let mut stream = vala_sdk::query::QueryClient::new(client)
        .query(&BifrostQueryRequest {
            sql: sql.clone(),
            visibility: VisibilityMode::Fused,
            freshness: wyrd_spec::vala::api::FreshnessPolicy::Strict,
            deadline_ms: Some(120_000),
        })
        .await
        .unwrap_or_else(|error| panic!("public query `{sql}` starts: {}", error.detail()));
    let mut rows = Vec::new();
    while let Some(batch) = stream
        .next_batch()
        .await
        .unwrap_or_else(|error| panic!("public query `{sql}` streams: {}", error.detail()))
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
