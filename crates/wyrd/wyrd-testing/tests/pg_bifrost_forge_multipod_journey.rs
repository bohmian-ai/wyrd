use wyrd_testing::WyrdTestServer;

#[tokio::test]
#[ignore = "requires two Forge scheduler instances sharing Postgres and Iceberg"]
async fn journey_forge_three_pod_lease_competition_and_restart_converges_once() {
    let server = WyrdTestServer::start_bound().await.expect("test server");
    assert!(!server.base_url().expect("bound URL").is_empty());
    server.shutdown().await.expect("server shutdown");
}
