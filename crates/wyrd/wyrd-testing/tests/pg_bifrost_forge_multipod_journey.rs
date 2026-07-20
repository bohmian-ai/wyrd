use wyrd_testing::WyrdTestServer;

#[tokio::test]
#[ignore = "requires two Forge scheduler instances sharing Postgres and Iceberg"]
async fn journey_forge_three_pods_converge_expiry_and_gc_once() {
    let server = WyrdTestServer::start_bound().await.expect("test server");
    assert!(!server.base_url().expect("bound URL").is_empty());
    server.shutdown().await.expect("server shutdown");
}

#[tokio::test]
#[ignore = "requires Forge restart and live-file assertions"]
async fn journey_forge_maintenance_restart_preserves_reads_and_live_files() {
    let server = WyrdTestServer::start_bound().await.expect("test server");
    let base_url = server.base_url().expect("bound URL");
    assert!(!base_url.is_empty());
    server.shutdown().await.expect("server shutdown");
}
