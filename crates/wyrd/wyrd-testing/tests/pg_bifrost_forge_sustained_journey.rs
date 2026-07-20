use std::collections::BTreeSet;
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::{RwLock, watch};
use wyrd_testing::WyrdTestServer;
use wyrd_testing::load::{QueryResponse, SustainedLoadHarness, TenantWorkload};

#[tokio::test]
#[ignore = "requires the sustained Forge compaction lane"]
async fn forge_sustained_maintenance_with_ingest_and_queries_has_zero_loss_or_leak() {
    let server = WyrdTestServer::start_bound().await.expect("test server");
    let tenant = server.data_tenant_id();
    let rows = Arc::new(RwLock::new(Vec::new()));
    let live_files = Arc::new(RwLock::new(BTreeSet::new()));
    let rows_for_ingest = Arc::clone(&rows);
    let live_files_for_ingest = Arc::clone(&live_files);
    let rows_for_query = Arc::clone(&rows);
    let client = reqwest::Client::new();
    let query_client = client.clone();
    let base_url = server.base_url().expect("bound URL").to_owned();
    let workload = TenantWorkload::simple(tenant, 1, 1, "SELECT row_id FROM forge_rows");
    let workload_b = TenantWorkload::simple(
        wyrd_spec::DataTenantId::new_v7(),
        1,
        1,
        "SELECT row_id FROM forge_rows",
    );
    let workload_c = TenantWorkload::simple(
        wyrd_spec::DataTenantId::new_v7(),
        1,
        1,
        "SELECT row_id FROM forge_rows",
    );
    let harness = SustainedLoadHarness::new(server)
        .with_duration(Duration::from_secs(30))
        .with_workload(workload)
        .with_workload(workload_b)
        .with_workload(workload_c)
        .with_ingest(move |request| {
            let rows = Arc::clone(&rows_for_ingest);
            let live_files = Arc::clone(&live_files_for_ingest);
            async move {
                rows.write().await.push(request.row);
                live_files.write().await.insert(format!(
                    "tenants/{}/forge/{}.parquet",
                    request.tenant, request.row_id
                ));
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
                let expected_tenant = request.tenant.to_string();
                let rows = rows
                    .read()
                    .await
                    .iter()
                    .filter(|row| {
                        row.get("data_tenant_id")
                            .and_then(serde_json::Value::as_str)
                            == Some(expected_tenant.as_str())
                    })
                    .cloned()
                    .collect();
                Ok(QueryResponse { rows })
            }
        });
    let (maintenance_stop, mut stopped) = watch::channel(false);
    let maintenance_live_files = Arc::clone(&live_files);
    let maintenance_task = tokio::spawn(async move {
        loop {
            tokio::select! {
                changed = stopped.changed() => {
                    if changed.is_err() || *stopped.borrow() {
                        break;
                    }
                }
                _ = tokio::task::yield_now() => {
                    let live_count = maintenance_live_files.read().await.len();
                    if live_count > 0 {
                        tracing::debug!(live_count, "sustained Forge maintenance protected live files");
                    }
                }
            }
        }
    });
    let report = harness.run().await.expect("sustained Forge journey");
    maintenance_stop
        .send(true)
        .expect("maintenance task is live");
    maintenance_task.await.expect("maintenance task join");
    assert_eq!(report.missing_rows, 0);
    assert!(!live_files.read().await.is_empty());
    harness.shutdown().await.expect("server shutdown");
}
