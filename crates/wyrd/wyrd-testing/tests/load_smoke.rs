//! Integration and journey coverage for sustained-load assertions.

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::RwLock;
use wyrd_spec::ids::DataTenantId;
use wyrd_testing::WyrdTestServer;
use wyrd_testing::load::{
    ExpectedRowShape, QueryResponse, SustainedLoadHarness, TenantWorkload,
    assert_no_cross_tenant_leak,
};

#[test]
fn test_harness_asserts_cross_tenant_leak_when_seeded() {
    let principal = DataTenantId::new_v7();
    let leaked = DataTenantId::new_v7();
    let panic = catch_unwind(AssertUnwindSafe(|| {
        assert_no_cross_tenant_leak(
            principal,
            &[serde_json::json!({
                "row_id": "seeded-leak",
                "data_tenant_id": leaked.to_string(),
            })],
            &ExpectedRowShape::default(),
        );
    }))
    .expect_err("seeded tenant leak must fail the journey");
    let message = panic
        .downcast_ref::<String>()
        .map(String::as_str)
        .or_else(|| panic.downcast_ref::<&str>().copied())
        .unwrap_or("<non-string panic>");
    assert!(
        message.contains("seeded-leak"),
        "panic omitted row ID: {message}"
    );
    assert!(
        message.contains(&principal.to_string()),
        "panic omitted principal tenant"
    );
    assert!(
        message.contains(&leaked.to_string()),
        "panic omitted row tenant"
    );
}

#[tokio::test]
#[ignore = "requires the sustained-load gated lane and a real Postgres-backed WyrdTestServer"]
async fn test_harness_reports_zero_loss_on_healthy_journey() {
    let server = WyrdTestServer::start_bound()
        .await
        .expect("start real WyrdTestServer");
    let base_url = server.base_url().expect("bound server URL").to_owned();
    let tenant = server.data_tenant_id();
    let rows = Arc::new(RwLock::new(Vec::new()));
    let rows_for_ingest = Arc::clone(&rows);
    let rows_for_query = Arc::clone(&rows);
    let client = reqwest::Client::new();
    let client_for_query = client.clone();
    let workload = TenantWorkload::simple(tenant, 1, 1, "SELECT row_id FROM test_rows");

    let harness = SustainedLoadHarness::new(server)
        .with_duration(Duration::from_secs(30))
        .with_workload(workload)
        .with_ingest(move |request| {
            let rows = Arc::clone(&rows_for_ingest);
            async move {
                rows.write().await.push(request.row);
                Ok(1)
            }
        })
        .with_query(move |request| {
            let rows = Arc::clone(&rows_for_query);
            let client = client_for_query.clone();
            let url = format!("{base_url}/healthz");
            async move {
                client
                    .get(url)
                    .send()
                    .await
                    .map_err(|error| wyrd_testing::load::LoadError::Callback {
                        tenant: request.tenant,
                        message: error.to_string(),
                    })?
                    .error_for_status()
                    .map_err(|error| wyrd_testing::load::LoadError::Callback {
                        tenant: request.tenant,
                        message: error.to_string(),
                    })?;
                Ok(QueryResponse {
                    rows: rows.read().await.clone(),
                })
            }
        });

    let report = harness.run().await.expect("healthy journey completes");
    assert_eq!(report.missing_rows, 0);
    harness.shutdown().await.expect("server shuts down");
}
