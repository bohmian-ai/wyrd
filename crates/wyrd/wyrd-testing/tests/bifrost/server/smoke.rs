use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use arrow::datatypes::{DataType, Field};
use vala_bifrost_redux::catalog::{CreateTableRequest, TableRef};
use vala_bifrost_redux::namespaces::BifrostNamespace;
use vala_sdk::{CollectedQueryLimits, QueryClient};
use wyrd_client::WyrdClient;
use wyrd_client::config::ClientConfig;
use wyrd_client::transport::{GrpcConfig, HttpConfig};
use wyrd_spec::vala::api::{BifrostQueryRequest, FreshnessPolicy, VisibilityMode};
use wyrd_testing::{Bootstrap, WyrdTestServer};

fn e2e_enabled() -> bool {
    std::env::var("WYRD_AUTH_E2E").is_ok()
}

#[tokio::test]
async fn server_in_process_healthz_returns_ok() {
    if !e2e_enabled() {
        return;
    }
    let srv = WyrdTestServer::start_in_process()
        .await
        .expect("start_in_process");
    let req = axum::http::Request::builder()
        .uri("/healthz")
        .body(axum::body::Body::empty())
        .expect("request builds");
    let resp = srv.oneshot(req).await.expect("oneshot");
    assert_eq!(resp.status(), axum::http::StatusCode::OK);
    srv.shutdown().await.expect("shutdown");
}

#[tokio::test]
async fn server_bound_real_socket_serves_healthz() {
    if !e2e_enabled() {
        return;
    }
    let srv = WyrdTestServer::start_bound().await.expect("start_bound");
    let base_url = srv.base_url().expect("has base url").to_owned();
    let resp = reqwest::Client::new()
        .get(format!("{base_url}/healthz"))
        .send()
        .await
        .expect("request sent");
    assert_eq!(resp.status(), 200);
    srv.shutdown().await.expect("shutdown");
}

/// A bound server retains the exact volume roots used by Scribe and Oracle admission.
///
/// The first public native write forces Scribe to sample its WAL volume. The
/// following public query forces Oracle to admit work against its scratch
/// volume, proving both owners still reference live harness directories after
/// startup completes.
///
/// # Panics
///
/// Panics when server startup, table creation, authentication, native ingest,
/// Scribe publication, Oracle admission, or shutdown fails.
#[tokio::test]
async fn server_retains_bifrost_volume_roots_across_public_write_and_query() {
    if !e2e_enabled() {
        return;
    }
    let server = WyrdTestServer::start_bound()
        .await
        .expect("bound Bifrost server");
    let table_name = "retained_volume_roots";
    let table_fqn = format!("vala.bifrost.{table_name}");
    server
        .create_bifrost_table_for_test(CreateTableRequest {
            table: TableRef::new(BifrostNamespace::Bifrost, table_name),
            user_fields: vec![Field::new("value", DataType::Int64, false)],
            tenant: server.data_tenant_id(),
            physical_layout: None,
            audit: None,
        })
        .await
        .expect("retained-root table");
    server
        .seed_bifrost_rows(&table_fqn, &[7])
        .await
        .expect("public Scribe write samples retained WAL root");

    let bootstrap = server
        .bootstrap_service("retained-root-reader", &["admin"])
        .await
        .expect("query reader bootstrap");
    let api_key = match bootstrap {
        Bootstrap::Machine { api_key, .. } => api_key,
        Bootstrap::User { .. } => panic!("query reader bootstrap returned a user"),
    };
    let client = WyrdClient::with_config(ClientConfig {
        grpc: GrpcConfig {
            endpoint: server.grpc_url().expect("bound gRPC URL"),
            connect_retries: 0,
            ..GrpcConfig::default()
        },
        http: HttpConfig {
            base_url: server.base_url().expect("bound HTTP URL").to_owned(),
            ..HttpConfig::default()
        },
        api_key: Some(api_key),
        ..ClientConfig::default()
    })
    .expect("query client");
    let result = QueryClient::new(&client)
        .collect_bounded(
            &BifrostQueryRequest {
                sql: format!("SELECT value FROM {table_fqn}"),
                visibility: VisibilityMode::PublishedOnly,
                freshness: FreshnessPolicy::Strict,
                deadline_ms: None,
            },
            CollectedQueryLimits {
                max_rows: 4,
                max_encoded_bytes: 1024 * 1024,
            },
        )
        .await
        .expect("public Oracle query samples retained scratch root");
    assert_eq!(result.rows, 1);
    assert_eq!(result.terminal.row_count, 1);
    server.shutdown().await.expect("retained-root shutdown");
}

/// Tenants created after Oracle readiness write and query without role restart.
///
/// The journey provisions two tenants only after the bound server is serving,
/// then drives each through public gRPC ingest and public HTTP query. The same
/// logical table name resolves to tenant-qualified physical state, while the
/// durable admission configuration remains the four canonical null-tenant rows.
///
/// # Panics
///
/// Panics when late tenant provisioning, public write/flush/query, tenant
/// isolation, canonical policy inspection, or server shutdown fails.
#[tokio::test]
#[ignore = "PostgreSQL-backed public dynamic-tenant admission journey"]
async fn pg_post_boot_tenants_use_dynamic_oracle_admission() {
    let server = WyrdTestServer::start_bound()
        .await
        .expect("bound server reaches Oracle readiness");
    let tenant_a = server
        .seed_tenant("post-boot-oracle-a")
        .await
        .expect("first post-boot tenant");
    let tenant_b = server
        .seed_tenant("post-boot-oracle-b")
        .await
        .expect("second post-boot tenant");
    let table_name = "post_boot_dynamic_admission";
    let table_fqn = format!("vala.bifrost.{table_name}");
    for tenant in [tenant_a, tenant_b] {
        server
            .create_bifrost_table_for_test(CreateTableRequest {
                table: TableRef::new(BifrostNamespace::Bifrost, table_name),
                user_fields: vec![Field::new("value", DataType::Int64, false)],
                tenant,
                physical_layout: None,
                audit: None,
            })
            .await
            .expect("tenant-qualified table");
    }
    server
        .seed_bifrost_rows_for_tenant(tenant_a, &table_fqn, &[11])
        .await
        .expect("first tenant public write and flush");
    server
        .seed_bifrost_rows_for_tenant(tenant_b, &table_fqn, &[22, 23])
        .await
        .expect("second tenant public write and flush");

    for (tenant, expected_rows) in [(tenant_a, 1), (tenant_b, 2)] {
        let bootstrap = server
            .bootstrap_service_in_tenant(tenant, "post-boot-query-reader", &["admin"])
            .await
            .expect("post-boot query reader");
        let api_key = match bootstrap {
            Bootstrap::Machine { api_key, .. } => api_key,
            Bootstrap::User { .. } => panic!("query reader bootstrap returned a user"),
        };
        let client = WyrdClient::with_config(ClientConfig {
            grpc: GrpcConfig {
                endpoint: server.grpc_url().expect("bound gRPC URL"),
                connect_retries: 0,
                ..GrpcConfig::default()
            },
            http: HttpConfig {
                base_url: server.base_url().expect("bound HTTP URL").to_owned(),
                ..HttpConfig::default()
            },
            api_key: Some(api_key),
            ..ClientConfig::default()
        })
        .expect("tenant query client");
        let result = QueryClient::new(&client)
            .collect_bounded(
                &BifrostQueryRequest {
                    sql: format!("SELECT value FROM {table_fqn}"),
                    visibility: VisibilityMode::PublishedOnly,
                    freshness: FreshnessPolicy::Strict,
                    deadline_ms: None,
                },
                CollectedQueryLimits {
                    max_rows: 4,
                    max_encoded_bytes: 1024 * 1024,
                },
            )
            .await
            .expect("post-boot public Oracle query");
        assert_eq!(result.rows, expected_rows);
        assert_eq!(result.terminal.row_count, expected_rows as u64);
    }
    let tenant_policy_rows: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM vala.oracle_admission_policies WHERE data_tenant_id IS NOT NULL",
    )
    .fetch_one(server.pg_fixture().operator_pool().pool())
    .await
    .expect("tenant policy inspection");
    assert_eq!(tenant_policy_rows, 0);
    server.shutdown().await.expect("post-boot journey shutdown");
}

/// A readiness failure tears down the real bound task before another process starts.
///
/// # Panics
///
/// Panics when injected readiness rollback leaves a process-local pool or
/// listener task alive enough to prevent a fresh bound server from serving.
#[tokio::test]
async fn readiness_failure_rolls_back_before_fresh_bound_server_starts() {
    if !e2e_enabled() {
        return;
    }
    let reserve = || {
        let listener = std::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
            .expect("reserve test address");
        listener.local_addr().expect("reserved address")
    };
    let http = reserve();
    let grpc = reserve();
    let aborted = Arc::new(AtomicBool::new(false));
    let result = WyrdTestServer::builder()
        .with_bind_addrs_for_test(http, grpc)
        .with_readiness_failure_for_test()
        .with_stalled_drain_for_test(Arc::clone(&aborted))
        .start_bound()
        .await;
    let error = match result {
        Ok(server) => {
            server
                .shutdown()
                .await
                .expect("unexpectedly ready server shutdown");
            panic!("injected readiness failure must fail startup");
        }
        Err(error) => error,
    };
    assert!(
        matches!(error, wyrd_testing::WyrdTestServerError::Bind(message) if message.contains("injected readiness failure")),
        "readiness error must remain primary after rollback"
    );
    assert!(
        aborted.load(Ordering::SeqCst),
        "timed-out startup drain must abort the stalled serve task"
    );

    let fresh = WyrdTestServer::builder()
        .with_bind_addrs_for_test(http, grpc)
        .start_bound()
        .await
        .expect("fresh process after readiness rollback");
    let response = reqwest::Client::new()
        .get(format!(
            "{}/healthz",
            fresh.base_url().expect("fresh base url")
        ))
        .send()
        .await
        .expect("fresh process local pool and listener");
    assert_eq!(response.status(), 200);
    fresh.shutdown().await.expect("fresh server shutdown");
}

#[tokio::test]
async fn server_in_process_auth_token_round_trip() {
    if !e2e_enabled() {
        return;
    }
    let srv = WyrdTestServer::start_in_process()
        .await
        .expect("start_in_process");
    let bootstrap = srv
        .bootstrap_service("test-svc", &["writer"])
        .await
        .expect("bootstrap_service");
    let api_key = bootstrap.api_key().expect("machine has api key").clone();
    let jwt = srv
        .exchange_api_key(&api_key)
        .await
        .expect("exchange_api_key");
    assert!(!jwt.is_empty());
    srv.shutdown().await.expect("shutdown");
}

#[tokio::test]
async fn server_drop_on_runtime_is_cancel_only() {
    if !e2e_enabled() {
        return;
    }
    let srv = WyrdTestServer::start_bound().await.expect("start_bound");
    drop(srv);
    // Test must not deadlock. If it returns, the cancel-only path fired.
}
