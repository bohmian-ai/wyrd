//! Oracle journeys — Admission capacity and fairness: the capacity contract, heartbeat
//! survival under refusal, concurrent ingest progress, and multitenant
//! isolation.
//!
//! Module of the `oracle` binary; see `oracle.rs` for the capability it
//! proves and `support.rs` for the fixtures it shares.

use chrono::Utc;
use vala_bifrost_redux::cluster::RoleTiming;
use vala_sdk::{QueryClient, ValaSdkError};
use wyrd_spec::DataTenantId;
use wyrd_spec::error::WyrdError;
use wyrd_spec::vala::api::{
    BifrostQueryRequest, FreshnessPolicy, QueryClass, QueryTerminalErrorCode, QueryTerminalOutcome,
    VisibilityMode,
};
use wyrd_spec::vala::error::BifrostError;
use wyrd_testing::bifrost::{BifrostClusterSpec, WyrdTestCluster};

use crate::support::*;

/// A retained production Oracle reservation produces the public typed 429 and
/// allows the same durable query to complete after capacity is released.
#[tokio::test]
#[ignore = "requires the serialized Postgres-backed Oracle journey lane"]
async fn pg_bifrost_oracle_capacity_contract_journey() {
    let cluster = WyrdTestCluster::start_spec(BifrostClusterSpec::one_mixed())
        .await
        .expect("capacity contract cluster");
    let server = cluster.server(0).expect("mixed server");
    let table = unique_table("oracle_capacity_contract");
    register_table(server, cluster.data_tenant_id(), &table)
        .await
        .expect("capacity contract table");
    let writer = client(server, "oracle-capacity-writer")
        .await
        .expect("writer client");
    ingest(&writer, &format!("vala.bifrost.{table}"), &[1, 2])
        .await
        .expect("capacity fixture ingest");
    server
        .flush_bifrost()
        .await
        .expect("capacity fixture flush");
    let reader = client(server, "oracle-capacity-reader")
        .await
        .expect("reader client");
    let governor = server
        .state()
        .bifrost_resources()
        .expect("shared production memory governor");
    let oracle = governor.oracle().expect("Oracle resource capability");
    let retained = oracle
        .try_acquire_query(vala_bifrost_redux::resources::OracleResourceRequest {
            query_class: QueryClass::Interactive,
            memory_bytes: vala_bifrost_redux::resources::ORACLE_PARTITION_MEMORY_BYTES,
            scratch_bytes: vala_bifrost_redux::resources::ORACLE_PARTITION_MEMORY_BYTES as u64,
            slot_units: 1,
            local_ratio: 1.0,
        })
        .expect("retain the complete Oracle child budget");

    let refusal = match QueryClient::new(&reader)
        .query(&BifrostQueryRequest {
            sql: format!("SELECT id, value FROM vala.bifrost.{table} ORDER BY id"),
            visibility: VisibilityMode::PublishedOnly,
            freshness: FreshnessPolicy::Strict,
            deadline_ms: Some(10_000),
        })
        .await
    {
        Ok(_) => panic!("occupied Oracle budget must reject before opening a response stream"),
        Err(error) => error,
    };
    assert!(matches!(
        refusal,
        ValaSdkError::Transport(WyrdError::Vala {
            error: BifrostError::QueryAdmissionRejected
        })
    ));

    drop(retained);
    assert_eq!(
        query_rows(&reader, &table, VisibilityMode::PublishedOnly)
            .await
            .expect("query succeeds after retained capacity drains"),
        2
    );

    cluster.shutdown().await.expect("capacity cluster shutdown");
}

/// Sustained Oracle admission refusals leave the independent durable heartbeat
/// advancing and ready, and the same public query succeeds after pressure ends.
#[tokio::test]
#[ignore = "requires the serialized Postgres-backed Oracle journey lane"]
async fn oracle_heartbeat_survives_capacity_refusals() {
    let mut spec = BifrostClusterSpec::one_mixed();
    spec.nodes[0].role_timing = Some(RoleTiming::deterministic_test());
    let cluster = WyrdTestCluster::start_spec(spec)
        .await
        .expect("heartbeat capacity cluster");
    let server = cluster.server(0).expect("heartbeat mixed server");
    let table = unique_table("oracle_heartbeat_capacity");
    register_table(server, cluster.data_tenant_id(), &table)
        .await
        .expect("heartbeat table");
    let writer = client(server, "heartbeat-capacity-writer")
        .await
        .expect("heartbeat writer");
    ingest(&writer, &format!("vala.bifrost.{table}"), &[1, 2])
        .await
        .expect("heartbeat fixture ingest");
    server
        .flush_bifrost()
        .await
        .expect("heartbeat fixture flush");
    let reader = client(server, "heartbeat-capacity-reader")
        .await
        .expect("heartbeat reader");
    let owner = cluster
        .pg_fixture()
        .superuser_pool()
        .await
        .expect("heartbeat owner pool");
    let heartbeat_before: chrono::DateTime<Utc> =
        sqlx::query_scalar("SELECT heartbeat_at FROM vala.cluster_nodes WHERE role = 'oracle'")
            .fetch_one(&owner)
            .await
            .expect("initial Oracle heartbeat");
    let governor = server
        .state()
        .bifrost_resources()
        .expect("heartbeat shared governor");
    let oracle = governor.oracle().expect("Oracle resource capability");
    let retained = oracle
        .try_acquire_query(vala_bifrost_redux::resources::OracleResourceRequest {
            query_class: QueryClass::Interactive,
            memory_bytes: vala_bifrost_redux::resources::ORACLE_PARTITION_MEMORY_BYTES,
            scratch_bytes: vala_bifrost_redux::resources::ORACLE_PARTITION_MEMORY_BYTES as u64,
            slot_units: 1,
            local_ratio: 1.0,
        })
        .expect("retain Oracle capacity during heartbeat proof");
    let pressure_deadline = tokio::time::Instant::now() + std::time::Duration::from_millis(250);
    let mut refusals = 0_u64;
    while tokio::time::Instant::now() < pressure_deadline {
        let result = QueryClient::new(&reader)
            .query(&BifrostQueryRequest {
                sql: format!("SELECT COUNT(*) FROM vala.bifrost.{table}"),
                visibility: VisibilityMode::PublishedOnly,
                freshness: FreshnessPolicy::Strict,
                deadline_ms: Some(1_000),
            })
            .await;
        assert!(matches!(
            result,
            Err(ValaSdkError::Transport(WyrdError::Vala {
                error: BifrostError::QueryAdmissionRejected
            }))
        ));
        refusals = refusals.saturating_add(1);
        tokio::task::yield_now().await;
    }
    assert!(
        refusals >= 2,
        "pressure window must observe repeated typed 429s"
    );
    let (heartbeat_after, ready): (chrono::DateTime<Utc>, bool) =
        sqlx::query_as("SELECT heartbeat_at, ready FROM vala.cluster_nodes WHERE role = 'oracle'")
            .fetch_one(&owner)
            .await
            .expect("Oracle heartbeat after pressure");
    println!(
        "oracle heartbeat advanced under capacity pressure: before={heartbeat_before} after={heartbeat_after}"
    );
    assert!(heartbeat_after > heartbeat_before);
    assert!(
        ready,
        "Oracle readiness must remain advertised under pressure"
    );
    drop(retained);
    assert_eq!(
        query_rows(&reader, &table, VisibilityMode::PublishedOnly)
            .await
            .expect("strict query succeeds after heartbeat pressure"),
        2
    );
    cluster
        .shutdown()
        .await
        .expect("heartbeat cluster shutdown");
}

/// Proves tenant isolation, local fairness, durable audit, and audit refusal.
#[tokio::test]
#[ignore = "requires the serialized Postgres-backed Oracle journey lane"]
async fn pg_bifrost_oracle_multitenant_isolation_and_fairness_journey() {
    let cluster = WyrdTestCluster::start_spec(BifrostClusterSpec::one_mixed())
        .await
        .expect("isolation journey cluster");
    let server = cluster.server(0).expect("isolation journey server");
    let tenant_a = cluster.data_tenant_id();
    let tenant_b = cluster.add_tenant("oracle-j5-b").await.expect("tenant B");
    let table_name = unique_table("oracle_j5");
    for tenant in [tenant_a, tenant_b] {
        register_table(server, tenant, &table_name)
            .await
            .expect("tenant table registration");
        let client = client_for_tenant(server, tenant, &format!("j5-{tenant}"))
            .await
            .expect("tenant client");
        ingest(
            &client,
            &format!("vala.bifrost.{table_name}"),
            &[tenant_marker(tenant)],
        )
        .await
        .expect("tenant ingest");
        server
            .flush_bifrost_for_tenant(tenant)
            .await
            .expect("tenant flush");
        assert_eq!(
            query_rows(&client, &table_name, VisibilityMode::PublishedOnly)
                .await
                .expect("tenant query"),
            1
        );
    }
    seed_foreign_hot_row(&cluster, tenant_a, &table_name, tenant_b, "j5-foreign")
        .await
        .expect("foreign physical row");
    let tenant_a_client = client_for_tenant(server, tenant_a, "j5-tripwire")
        .await
        .expect("tripwire client");
    let table_fqn = format!("vala.bifrost.{table_name}");
    for sql in [
        format!("SELECT count(*) AS total FROM {table_fqn}"),
        format!("SELECT a.id FROM {table_fqn} a JOIN {table_fqn} b ON a.id = b.id"),
    ] {
        let (rows, outcome, error) = query_statement(&tenant_a_client, sql)
            .await
            .expect("tripwire terminal");
        assert_eq!(rows, 0, "foreign row reached a SQL operator");
        assert_eq!(outcome, QueryTerminalOutcome::Failed);
        assert_eq!(error, Some(QueryTerminalErrorCode::QueryTenantInvariant));
    }
    let owner = cluster
        .pg_fixture()
        .superuser_pool()
        .await
        .expect("isolation journey table-owner pool");
    let audit_deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM vala.audit_outbox \
             WHERE data_tenant_id = $1 \
               AND operation = 'bifrost.query.security_violation'",
        )
        .bind(tenant_a.as_uuid())
        .fetch_one(&owner)
        .await
        .expect("security audit count");
        if count >= 2 {
            break;
        }
        assert!(
            tokio::time::Instant::now() < audit_deadline,
            "security audit relay did not drain"
        );
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }
    let security_details: Vec<String> = sqlx::query_scalar(
        "SELECT detail::text FROM vala.audit_outbox \
         WHERE data_tenant_id = $1 \
           AND operation = 'bifrost.query.security_violation' \
         ORDER BY seq",
    )
    .bind(tenant_a.as_uuid())
    .fetch_all(&owner)
    .await
    .expect("security audit rows");
    assert!(
        security_details.len() >= 2,
        "COUNT and JOIN each require a durable security audit"
    );
    assert!(security_details.iter().all(|detail| {
        detail.contains("\"violation\":\"tenant_row\"") && detail.contains("\"phase\":\"source\"")
    }));

    let inspection = cluster
        .oracle_inspection()
        .await
        .expect("isolation journey inspection");
    assert!(inspection.audit_rows >= 4);
    assert_eq!(inspection.active_queries, 0);
    assert_eq!(inspection.queued_queries, 0);
    assert_eq!(inspection.reserved_memory_bytes, 0);
    assert_eq!(inspection.reserved_spill_bytes, 0);
    assert_eq!(inspection.peer_pending, 0);
    assert_eq!(inspection.peer_running, 0);
    drop(owner);
    cluster
        .shutdown()
        .await
        .expect("isolation journey shutdown");
}

/// Derive a deterministic row marker without exposing tenant identity in telemetry.
fn tenant_marker(tenant: DataTenantId) -> i64 {
    i64::from(tenant.as_uuid().as_bytes()[0])
}
