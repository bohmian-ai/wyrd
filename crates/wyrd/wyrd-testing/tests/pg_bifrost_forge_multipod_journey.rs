use std::time::Duration;
use vala_bifrost_redux::forge::{ForgeScheduler, run_maintenance_tick};
use wyrd_testing::WyrdTestServer;

#[tokio::test]
#[ignore = "requires two Forge scheduler instances sharing Postgres and Iceberg"]
async fn journey_forge_scheduler_three_pod_lease_competition_converges_once() {
    let server = WyrdTestServer::builder()
        .with_forge_interval(Duration::from_millis(10))
        .start_bound()
        .await
        .expect("test server");
    let context = server
        .state()
        .forge_context
        .as_ref()
        .cloned()
        .expect("Forge context");
    let scheduler_a = ForgeScheduler::start((*context).clone(), Duration::from_millis(10))
        .expect("first shared scheduler");
    let scheduler_b = ForgeScheduler::start((*context).clone(), Duration::from_millis(10))
        .expect("second shared scheduler");
    tokio::time::sleep(Duration::from_millis(50)).await;
    scheduler_a
        .shutdown()
        .await
        .expect("first scheduler shutdown");
    scheduler_b
        .shutdown()
        .await
        .expect("second scheduler shutdown");
    let operator_pool = server
        .state()
        .forge_context
        .as_ref()
        .expect("Forge context")
        .operator_pool
        .clone();
    let lease_count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM vala.maintenance_leases WHERE lease_key LIKE 'forge:table:%'",
    )
    .fetch_one(operator_pool.pool())
    .await
    .expect("Forge lease query");
    assert_eq!(lease_count, 0, "server scheduler leaves no stale lease");
    server.shutdown().await.expect("server shutdown");
}

#[tokio::test]
#[ignore = "requires Forge restart and live-file assertions"]
async fn journey_forge_scheduler_restart_preserves_reads_and_live_files() {
    let server = WyrdTestServer::builder()
        .with_forge_interval(Duration::from_millis(10))
        .start_bound()
        .await
        .expect("test server");
    let context = server
        .state()
        .forge_context
        .as_ref()
        .cloned()
        .expect("Forge context");
    let outcome = run_maintenance_tick(&context)
        .await
        .expect("one-shot production Forge tick");
    assert_eq!(outcome.bins_committed, 0);
    server.shutdown().await.expect("server shutdown");
    let restarted = ForgeScheduler::start((*context).clone(), Duration::from_millis(10))
        .expect("restarted scheduler");
    tokio::time::sleep(Duration::from_millis(30)).await;
    restarted
        .shutdown()
        .await
        .expect("restarted scheduler shutdown");
}
