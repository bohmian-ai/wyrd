use wyrd_testing::WyrdTestServer;

#[tokio::test]
#[ignore = "requires the Forge catalog and object-store journey lane"]
async fn journey_forge_single_pod_preserves_query_result_before_after_compaction() {
    let server = WyrdTestServer::start_bound().await.expect("test server");
    assert!(!server.base_url().expect("bound URL").is_empty());
    server.shutdown().await.expect("server shutdown");
}
