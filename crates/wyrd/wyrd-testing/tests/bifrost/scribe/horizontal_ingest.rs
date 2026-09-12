//! Horizontal production journeys for multi-pod Scribe ingestion.
//!
//! Both cases here answer a question a single-pod case cannot: does a real SaaS
//! topology — three independently bound servers over one Postgres, one object
//! store and one catalog — accept work on every pod at once, and does adding
//! tenants and tables to that topology change the pod-local machine that serves
//! them? Every write is a public authenticated gRPC append to one endpoint,
//! every read is the public strict fused query route, and every claim about a
//! pod is read from that pod's own production Scribe inspection rather than
//! inferred from the client side.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use arrow::array::{FixedSizeBinaryArray, Int32Array, Int64Array};
use arrow::datatypes::{DataType, Field};
use vala_bifrost_redux::catalog::{CreateTableRequest, TableRef};
use vala_bifrost_redux::namespaces::BifrostNamespace;
use vala_bifrost_redux::scribe::routing::SCRIBE_SHARD_COUNT;
use wyrd_client::WyrdClient;
use wyrd_spec::DataTenantId;
use wyrd_testing::WyrdTestServer;
use wyrd_testing::bifrost::cluster::{BifrostTopology, WyrdTestCluster};

use super::support::{encode_ipc, unique_table};

/// Number of independently bound production pods both journeys drive.
const PODS: usize = 3;

/// WAL fsync delay applied to every pod in the concurrency journey.
///
/// The delay is a deterministic production control, not a sleep: it widens the
/// window in which an accepted append is still owned by a Scribe stage so the
/// journey can observe all three pods holding work at the same instant. No
/// assertion depends on its magnitude, only on the fact that concurrent work is
/// simultaneously visible.
const WAL_SYNC_DELAY: Duration = Duration::from_millis(250);

/// Longest the journey waits for a production state it requires to appear.
///
/// Every wait in this module polls observed production state and fails with
/// what it actually saw. The deadline turns a stuck pod into a diagnosable
/// failure; elapsed time is never itself evidence.
const OBSERVATION_DEADLINE: Duration = Duration::from_secs(30);

/// Batches submitted in the concurrency journey, four to each of three pods.
const CONCURRENT_BATCHES: usize = 12;

/// Rows carried by every batch in both journeys.
///
/// Small on purpose: both journeys prove pod participation, convergence and
/// topology stability, none of which are saturation properties.
const ROWS_PER_BATCH: usize = 4;

/// Tenants in the dynamic-identity journey.
const TENANTS: usize = 4;

/// Registered tables per tenant in the dynamic-identity journey.
///
/// Four tenants times five tables is twenty logical tables, which exceeds the
/// fixed sixteen lanes and is therefore the direct proof that a table identity
/// does not own a lane.
const TABLES_PER_TENANT: usize = 5;

/// One row as a public reader can identify it after publication.
///
/// The triple is the durable identity Scribe stamps on every accepted row plus
/// the caller's own payload, so comparing complete sorted sets of these rejects
/// a missing row, a duplicated row, a corrupted payload and a row that belongs
/// to some other batch — none of which a row count can distinguish.
type RowIdentity = (uuid::Uuid, i32, i64);

/// One batch as a journey submits it: the pod endpoint, its idempotency key,
/// and the caller's payload values in row order.
type SubmittedBatch = (usize, uuid::Uuid, Vec<i64>);

/// Every batch submitted for one `(tenant ordinal, table ordinal)` identity.
///
/// Keyed rather than flat because the dynamic-identity journey reads each
/// identity back on its own and must compare it against exactly the batches all
/// three pods accepted for that identity alone.
type SubmissionsByIdentity = BTreeMap<(usize, usize), Vec<SubmittedBatch>>;

/// Three independently bound pods accept concurrent batches that then converge.
///
/// The journey submits twelve distinct batches for one tenant and one table,
/// four to each pod's public ingest endpoint, released together from one
/// barrier. While they are in flight it requires every pod to report its own
/// in-flight ingress work simultaneously, which sequential writes to three
/// endpoints cannot produce. It then requires each pod's durable
/// acknowledgement total to have advanced, flushes every pod, and compares the
/// complete `(batch id, row ordinal, value)` set a public read returns against
/// the exact set that was submitted.
///
/// # Panics
///
/// Panics when the cluster cannot start, when any append is refused, when the
/// three pods are never simultaneously active inside [`OBSERVATION_DEADLINE`],
/// when a pod acknowledged nothing, when the read-back is not exactly the
/// submitted set, or when terminal shutdown ownership is not empty.
#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
#[ignore = "requires Postgres and object storage"]
async fn multi_pod_concurrent_batches_are_owned_and_visible() {
    let cluster =
        WyrdTestCluster::start_with_wal_sync_delay(PODS, BifrostTopology::ThreePod, WAL_SYNC_DELAY)
            .await
            .expect("the three-pod production cluster starts");
    let tenant = cluster.data_tenant_id();
    let name = unique_table("horizontal_concurrent");
    let table = register_table(&cluster, tenant, &name).await;
    let clients = endpoint_clients(&cluster, tenant).await;

    let baseline = acknowledged_by_pod(&cluster);
    assert_eq!(
        in_flight_by_pod(&cluster),
        vec![0; PODS],
        "no pod may hold ingress work before the barrier releases; without this \
         the simultaneous-activity observation below could be satisfied by a \
         stale total rather than by live concurrent work"
    );

    let submitted: Vec<SubmittedBatch> = (0..CONCURRENT_BATCHES)
        .map(|ordinal| {
            let values = (0..ROWS_PER_BATCH)
                .map(|row| (ordinal * ROWS_PER_BATCH + row) as i64)
                .collect::<Vec<_>>();
            (ordinal % PODS, uuid::Uuid::now_v7(), values)
        })
        .collect();

    let barrier = Arc::new(tokio::sync::Barrier::new(CONCURRENT_BATCHES + 1));
    let mut appends = tokio::task::JoinSet::new();
    for (pod, batch_id, values) in submitted.clone() {
        let client = Arc::clone(&clients[pod]);
        let table = table.clone();
        let barrier = Arc::clone(&barrier);
        appends.spawn(async move {
            barrier.wait().await;
            append_values(&client, &table, batch_id, &values).await;
        });
    }
    barrier.wait().await;

    await_all_pods_active(&cluster).await;

    while let Some(joined) = appends.join_next().await {
        joined.expect("every public append task completes");
    }

    let acknowledged = acknowledged_by_pod(&cluster);
    for pod in 0..PODS {
        assert!(
            acknowledged[pod] > baseline[pod],
            "pod {pod} must durably acknowledge the batches sent to its own \
             endpoint, but its acknowledgement total stayed at {}",
            acknowledged[pod]
        );
    }

    flush_every_pod(&cluster).await;

    let mut expected = expected_rows(&submitted);
    expected.sort_unstable();
    let observed = read_rows(&clients[0], &table).await;
    assert_eq!(
        observed, expected,
        "a public read must return exactly the rows the three pods accepted, \
         with no missing, duplicated, corrupted or foreign row"
    );

    assert_clean_shutdown(cluster).await;
}

/// One sealed batch delivered to every pod at once is visible exactly once.
///
/// The canonical Gate-to-Scribe path is what retained audit publication relies
/// on for safety across replicas: a batch identity derived from content alone
/// means several pods can present the *same* sealed batch, and only Scribe's
/// durable commit fence decides how many times its rows exist. A sibling
/// journey already proves three pods accept *distinct* batches concurrently,
/// which says nothing about collision — a fence that admitted every attempt
/// would pass it and triple the rows here.
///
/// So this submits one immutable batch — one batch id over one byte-identical
/// Arrow payload — to all three pod endpoints simultaneously from one barrier,
/// and requires the public strict fused read to return each
/// `(batch id, row ordinal)` exactly once. Every attempt must still be
/// acknowledged: a suppressed duplicate is an idempotent success, not a refusal
/// the caller has to interpret.
///
/// # Panics
///
/// Panics when the cluster cannot start, when any pod refuses the sealed batch,
/// when the read-back is not exactly one copy of it, or when terminal shutdown
/// ownership is not empty.
#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
#[ignore = "requires Postgres and object storage"]
async fn one_sealed_batch_submitted_to_every_pod_is_visible_once() {
    let cluster =
        WyrdTestCluster::start_with_wal_sync_delay(PODS, BifrostTopology::ThreePod, WAL_SYNC_DELAY)
            .await
            .expect("the three-pod production cluster starts");
    let tenant = cluster.data_tenant_id();
    let name = unique_table("horizontal_sealed");
    let table = register_table(&cluster, tenant, &name).await;
    let clients = endpoint_clients(&cluster, tenant).await;

    let batch_id = uuid::Uuid::now_v7();
    let values: Vec<i64> = (0..ROWS_PER_BATCH as i64).collect();

    let barrier = Arc::new(tokio::sync::Barrier::new(PODS));
    let mut appends = tokio::task::JoinSet::new();
    for pod in 0..PODS {
        let client = Arc::clone(&clients[pod]);
        let table = table.clone();
        let values = values.clone();
        let barrier = Arc::clone(&barrier);
        appends.spawn(async move {
            barrier.wait().await;
            append_values(&client, &table, batch_id, &values).await;
        });
    }
    while let Some(joined) = appends.join_next().await {
        joined.expect("every pod's attempt at the sealed batch is acknowledged");
    }

    flush_every_pod(&cluster).await;

    let mut expected = expected_rows(&[(0, batch_id, values)]);
    expected.sort_unstable();
    let observed = read_rows(&clients[0], &table).await;
    assert_eq!(
        observed, expected,
        "three pods presented one sealed batch identity, so the durable commit \
         fence must leave exactly one copy of its rows visible"
    );

    assert_clean_shutdown(cluster).await;
}

/// Twenty tenant/table identities ingest on every pod without growing topology.
///
/// The journey registers five tables for each of four tenants, then submits one
/// batch for every `(tenant, table, pod)` combination — sixty in all — released
/// together from one barrier. Twenty logical tables exceed the sixteen fixed
/// lanes, so if identity mapped to topology the pods would have to grow. Each
/// pod's shard tasks, shard channels and open WAL streams are read from its own
/// production inspection before and after the run and must be unchanged. Every
/// tenant then reads each of its own tables through its own authenticated
/// client and must see exactly the rows all three pods wrote for it, and none
/// belonging to another tenant.
///
/// # Panics
///
/// Panics when the cluster cannot start, when any append is refused, when a pod
/// grew a lane, channel or WAL owner, when a tenant's read is not exactly its
/// own submitted rows, or when terminal shutdown ownership is not empty.
#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
#[ignore = "requires Postgres and object storage"]
async fn dynamic_tenant_tables_ingest_without_topology_growth() {
    let cluster = WyrdTestCluster::start_spec(
        wyrd_testing::bifrost::cluster::BifrostClusterSpec::three_mixed(),
    )
    .await
    .expect("the three-pod production cluster starts");

    // Every tenant registers the same five table names. Distinct names would
    // make cross-tenant leakage unobservable: a read scoped to a name only one
    // tenant owns cannot return another tenant's rows no matter how isolation
    // behaves. Sharing the names puts four tenants' rows behind one logical
    // identity, so the exact per-tenant read below is a real isolation check.
    let shared_names: Vec<String> = (0..TABLES_PER_TENANT)
        .map(|table_ordinal| unique_table(&format!("horizontal_dynamic_{table_ordinal}")))
        .collect();
    let mut tenants = Vec::with_capacity(TENANTS);
    for ordinal in 0..TENANTS {
        let tenant = if ordinal == 0 {
            cluster.data_tenant_id()
        } else {
            cluster
                .add_tenant(&unique_table("scribe-horizontal"))
                .await
                .expect("the cluster provisions an additional tenant")
        };
        let clients = endpoint_clients(&cluster, tenant).await;
        let mut tables = Vec::with_capacity(TABLES_PER_TENANT);
        for name in &shared_names {
            tables.push(register_table(&cluster, tenant, name).await);
        }
        tenants.push((clients, tables));
    }

    let before = topology_by_pod(&cluster);
    assert_fixed_topology(&before, "before ingestion");

    let mut submitted: SubmissionsByIdentity = BTreeMap::new();
    for (tenant_ordinal, (_, tables)) in tenants.iter().enumerate() {
        for table_ordinal in 0..tables.len() {
            for pod in 0..PODS {
                let base = ((tenant_ordinal * TABLES_PER_TENANT + table_ordinal) * PODS + pod)
                    * ROWS_PER_BATCH;
                let values = (0..ROWS_PER_BATCH)
                    .map(|row| (base + row) as i64)
                    .collect::<Vec<_>>();
                submitted
                    .entry((tenant_ordinal, table_ordinal))
                    .or_default()
                    .push((pod, uuid::Uuid::now_v7(), values));
            }
        }
    }

    let total = TENANTS * TABLES_PER_TENANT * PODS;
    let barrier = Arc::new(tokio::sync::Barrier::new(total));
    let mut appends = tokio::task::JoinSet::new();
    for ((tenant_ordinal, table_ordinal), batches) in &submitted {
        let (clients, tables) = &tenants[*tenant_ordinal];
        for (pod, batch_id, values) in batches {
            let client = Arc::clone(&clients[*pod]);
            let table = tables[*table_ordinal].clone();
            let batch_id = *batch_id;
            let values = values.clone();
            let barrier = Arc::clone(&barrier);
            appends.spawn(async move {
                barrier.wait().await;
                append_values(&client, &table, batch_id, &values).await;
            });
        }
    }
    while let Some(joined) = appends.join_next().await {
        joined.expect("every public append task completes");
    }

    let after = topology_by_pod(&cluster);
    assert_fixed_topology(&after, "after ingestion");

    flush_every_pod(&cluster).await;

    for ((tenant_ordinal, table_ordinal), batches) in &submitted {
        let (clients, tables) = &tenants[*tenant_ordinal];
        let table = &tables[*table_ordinal];
        let mut expected = expected_rows(batches);
        expected.sort_unstable();
        let observed = read_rows(&clients[0], table).await;
        assert_eq!(
            observed, expected,
            "tenant {tenant_ordinal} table {table_ordinal} must read back exactly \
             the rows its own three pods accepted for it"
        );
    }

    assert_clean_shutdown(cluster).await;
}

/// Registers one single-column table for a tenant through the shared catalog.
///
/// Registration goes through the first pod because the catalog is one durable
/// owner shared by the whole cluster; a table does not belong to the pod that
/// declared it.
async fn register_table(cluster: &WyrdTestCluster, tenant: DataTenantId, name: &str) -> String {
    pod(cluster, 0)
        .create_bifrost_table_for_test(CreateTableRequest {
            table: TableRef::new(BifrostNamespace::Datasets, name),
            user_fields: vec![Field::new("value", DataType::Int64, false)],
            tenant,
            physical_layout: None,
            audit: None,
        })
        .await
        .expect("the shared catalog registers the table");
    format!("{}.{name}", BifrostNamespace::Datasets.as_str())
}

/// Returns one pod's server by index.
///
/// # Panics
///
/// Panics when the cluster does not own a pod at `index`.
fn pod(cluster: &WyrdTestCluster, index: usize) -> &WyrdTestServer {
    cluster
        .server(index)
        .unwrap_or_else(|| panic!("the cluster owns pod {index}"))
}

/// Builds one authenticated client per pod endpoint for one tenant.
///
/// The tenant's service principal is minted once and reused across the three
/// endpoints, which is what a real horizontally scaled caller does: one
/// identity, several interchangeable serving addresses. One client per endpoint
/// is retained for the whole journey rather than built per batch.
async fn endpoint_clients(cluster: &WyrdTestCluster, tenant: DataTenantId) -> Vec<Arc<WyrdClient>> {
    let bootstrap = pod(cluster, 0)
        .bootstrap_service_in_tenant(
            tenant,
            &unique_table("scribe_horizontal_client"),
            &["admin"],
        )
        .await
        .expect("tenant service bootstrap");
    let api_key = bootstrap.api_key().expect("service API key").clone();
    (0..PODS)
        .map(|index| {
            let server = pod(cluster, index);
            Arc::new(
                WyrdClient::with_config(wyrd_client::config::ClientConfig {
                    grpc: wyrd_client::transport::GrpcConfig {
                        endpoint: server.grpc_url().expect("bound gRPC URL"),
                        connect_retries: 0,
                        ..wyrd_client::transport::GrpcConfig::default()
                    },
                    http: wyrd_client::transport::HttpConfig {
                        base_url: server.base_url().expect("bound HTTP URL").to_owned(),
                        ..wyrd_client::transport::HttpConfig::default()
                    },
                    credential: Some(api_key.clone()),
                    ..wyrd_client::config::ClientConfig::default()
                })
                .expect("tenant SDK client"),
            )
        })
        .collect()
}

/// Sends one batch through one pod's public gRPC ingest and requires acceptance.
///
/// Neither journey exercises refusal, so any error here is a real failure of
/// horizontal ingestion rather than backpressure a caller should retry.
///
/// # Panics
///
/// Panics when the transport cannot connect or the append is not acknowledged.
async fn append_values(client: &WyrdClient, table: &str, batch_id: uuid::Uuid, values: &[i64]) {
    let schema = Arc::new(arrow::datatypes::Schema::new(vec![Field::new(
        "value",
        DataType::Int64,
        false,
    )]));
    let batch = arrow::record_batch::RecordBatch::try_new(
        schema,
        vec![Arc::new(Int64Array::from(values.to_vec()))],
    )
    .expect("value batch");
    wyrd_testing::bifrost::write::RawIngest::connect(client)
        .await
        .expect("public ingest transport connects")
        .insert(table, batch_id, encode_ipc(&batch))
        .await
        .unwrap_or_else(|error| panic!("append to `{table}` must be acknowledged: {error:?}"));
}

/// Expands submitted batches into the row identities a reader must observe.
fn expected_rows(batches: &[SubmittedBatch]) -> Vec<RowIdentity> {
    batches
        .iter()
        .flat_map(|(_, batch_id, values)| {
            values
                .iter()
                .enumerate()
                .map(move |(ordinal, value)| (*batch_id, ordinal as i32, *value))
        })
        .collect()
}

/// Reads one table's complete durable identities through the public query route.
///
/// Strict fused visibility is what makes this an authority read: the rows must
/// come from whichever source currently owns them. The batch id and row ordinal
/// are the identity Scribe itself stamped, so the returned set is comparable to
/// the submitted set without the test inventing an identity of its own.
///
/// # Panics
///
/// Panics when the query cannot start or stream, or when a managed identity
/// column is absent or of an unexpected Arrow type.
async fn read_rows(client: &WyrdClient, table: &str) -> Vec<RowIdentity> {
    let sql = format!("SELECT wyrd_batch_id, wyrd_row_ordinal, value FROM {table}");
    let mut stream = vala_sdk::query::QueryClient::new(client)
        .query(&wyrd_spec::vala::api::BifrostQueryRequest {
            sql: sql.clone(),
            visibility: wyrd_spec::vala::api::VisibilityMode::Fused,
            freshness: wyrd_spec::vala::api::FreshnessPolicy::Strict,
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
        let batch_ids = batch
            .column_by_name(wyrd_spec::vala::managed_columns::WYRD_BATCH_ID)
            .expect("the result carries the managed batch id")
            .as_any()
            .downcast_ref::<FixedSizeBinaryArray>()
            .expect("the managed batch id is a 16-byte key");
        let ordinals = batch
            .column_by_name(wyrd_spec::vala::managed_columns::WYRD_ROW_ORDINAL)
            .expect("the result carries the managed row ordinal")
            .as_any()
            .downcast_ref::<Int32Array>()
            .expect("the managed row ordinal is Int32");
        let values = batch
            .column_by_name("value")
            .expect("the result carries the caller's value")
            .as_any()
            .downcast_ref::<Int64Array>()
            .expect("value is Int64");
        for row in 0..batch.num_rows() {
            let key: [u8; 16] = batch_ids
                .value(row)
                .try_into()
                .expect("the managed batch id is exactly sixteen bytes");
            rows.push((
                uuid::Uuid::from_bytes(key),
                ordinals.value(row),
                values.value(row),
            ));
        }
    }
    rows.sort_unstable();
    rows
}

/// Returns each pod's currently owned ingress work in pod order.
///
/// Accepted-but-unsettled attempts plus queued shard commands are live gauges,
/// so an all-zero reading is a genuine idle pod rather than an absence of
/// history.
fn in_flight_by_pod(cluster: &WyrdTestCluster) -> Vec<usize> {
    (0..PODS)
        .map(|index| {
            let snapshot = pod(cluster, index)
                .scribe_inspection_snapshot()
                .expect("pod Scribe ownership is inspectable");
            snapshot.ingress_lifecycle.active_attempts + snapshot.queued_items
        })
        .collect()
}

/// Returns each pod's durable acknowledgement total in pod order.
fn acknowledged_by_pod(cluster: &WyrdTestCluster) -> Vec<u64> {
    (0..PODS)
        .map(|index| {
            pod(cluster, index)
                .scribe_inspection_snapshot()
                .expect("pod Scribe ownership is inspectable")
                .ingress_lifecycle
                .succeeded
        })
        .collect()
}

/// Returns each pod's fixed-topology counts in pod order.
///
/// The triple is `(shard tasks, shard channels, open WAL streams)`: the runtime,
/// mailbox and WAL-owner facts that must not follow tenant or table cardinality.
fn topology_by_pod(cluster: &WyrdTestCluster) -> Vec<(usize, usize, usize)> {
    (0..PODS)
        .map(|index| {
            let snapshot = pod(cluster, index)
                .scribe_inspection_snapshot()
                .expect("pod Scribe ownership is inspectable");
            (
                snapshot.shard_task_count,
                snapshot.shard_channel_count,
                snapshot.open_wal_stream_count,
            )
        })
        .collect()
}

/// Requires every pod to retain exactly the fixed lane topology.
///
/// # Panics
///
/// Panics when any pod reports other than [`SCRIBE_SHARD_COUNT`] shard tasks or
/// channels, or more open WAL streams than it has lanes.
fn assert_fixed_topology(observed: &[(usize, usize, usize)], phase: &str) {
    for (index, (tasks, channels, wal)) in observed.iter().enumerate() {
        assert_eq!(
            *tasks, SCRIBE_SHARD_COUNT,
            "pod {index} must own exactly {SCRIBE_SHARD_COUNT} shard tasks {phase}"
        );
        assert_eq!(
            *channels, SCRIBE_SHARD_COUNT,
            "pod {index} must own exactly {SCRIBE_SHARD_COUNT} shard channels {phase}"
        );
        assert!(
            *wal <= SCRIBE_SHARD_COUNT,
            "pod {index} must not open more than {SCRIBE_SHARD_COUNT} WAL streams \
             {phase}, but it holds {wal}"
        );
    }
}

/// Waits until every pod simultaneously owns in-flight ingress work.
///
/// This is the concurrency proof: three pods writing sequentially can never
/// produce a single observation in which all three hold accepted work at once.
/// The check reads each pod's own production ingress lifecycle and shard queue,
/// so it observes the servers rather than the client tasks.
///
/// # Panics
///
/// Panics when the pods are never simultaneously active inside
/// [`OBSERVATION_DEADLINE`], reporting the last per-pod state it saw.
async fn await_all_pods_active(cluster: &WyrdTestCluster) {
    let deadline = tokio::time::Instant::now() + OBSERVATION_DEADLINE;
    loop {
        let active = in_flight_by_pod(cluster);
        if active.iter().all(|owned| *owned > 0) {
            return;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "the three pods never held work at the same time inside \
             {OBSERVATION_DEADLINE:?}; last observed per-pod in-flight work was \
             {active:?}"
        );
        tokio::time::sleep(Duration::from_millis(2)).await;
    }
}

/// Publishes every pod's staged rows through the harness flush operation.
async fn flush_every_pod(cluster: &WyrdTestCluster) {
    for index in 0..PODS {
        pod(cluster, index)
            .flush_bifrost()
            .await
            .unwrap_or_else(|error| panic!("pod {index} publishes its staged rows: {error:?}"));
    }
}

/// Shuts the cluster down and requires terminal ownership to be empty.
///
/// # Panics
///
/// Panics when a listener, Scribe queue, in-flight append, WAL stream or
/// supervised task survives the bounded shutdown.
async fn assert_clean_shutdown(cluster: WyrdTestCluster) {
    let inspection = cluster
        .shutdown_and_inspect()
        .await
        .expect("the cluster shuts down");
    assert!(
        inspection.servers_stopped && inspection.listeners_stopped,
        "every pod and every bound listener must stop: {inspection:?}"
    );
    assert_eq!(inspection.scribe_queued, 0, "no Scribe command may survive");
    assert_eq!(
        inspection.scribe_inflight, 0,
        "no accepted append may survive shutdown"
    );
    assert_eq!(
        inspection.scribe_wal_streams, 0,
        "no WAL stream owner may survive shutdown"
    );
    assert_eq!(
        inspection.supervised_tasks, 0,
        "no supervised task may survive the owner it belongs to"
    );
}
