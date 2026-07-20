use std::sync::Arc;
use std::time::Duration;

use tokio::sync::RwLock;
use wyrd_testing::WyrdTestServer;
use wyrd_testing::load::{QueryResponse, SustainedLoadHarness, TenantWorkload};

#[tokio::test]
#[ignore = "requires the sustained Forge compaction lane"]
async fn forge_sustained_three_tenant_compaction_has_zero_loss_or_leak() {
    let server = WyrdTestServer::start_bound().await.expect("test server");
    let tenant = server.data_tenant_id();
    let rows = Arc::new(RwLock::new(Vec::new()));
    let rows_for_ingest = Arc::clone(&rows);
    let rows_for_query = Arc::clone(&rows);
    let client = reqwest::Client::new();
    let query_client = client.clone();
    let base_url = server.base_url().expect("bound URL").to_owned();
    let workload = TenantWorkload::simple(tenant, 1, 1, "SELECT row_id FROM forge_rows");
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
            let client = query_client.clone();
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
    let report = harness.run().await.expect("sustained Forge journey");
    assert_eq!(report.missing_rows, 0);
    harness.shutdown().await.expect("server shutdown");
}
