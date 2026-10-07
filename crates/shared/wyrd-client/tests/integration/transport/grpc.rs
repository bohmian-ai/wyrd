use wyrd_client::transport::config::GrpcConfig;

#[test]
fn grpc_rejects_unknown_fields() {
    let r: Result<GrpcConfig, _> = serde_json::from_str(
        r#"{"endpoint":"http://localhost:50051","timeout_ms":5000,"connect_retries":1,"unknown_field":true}"#,
    );
    assert!(r.is_err());
}

mod grpc_connection {
    use std::sync::Arc;

    use wyrd_client::auth::AuthMiddleware;
    use wyrd_client::config::ClientConfig;
    use wyrd_client::transport::GrpcConnection;
    use wyrd_client::transport::config::GrpcConfig;
    use wyrd_client::transport::credential::ResolvedCredential;

    fn make_auth() -> Arc<AuthMiddleware> {
        let config = ClientConfig::default();
        let credential =
            ResolvedCredential::BearerToken(secrecy::SecretString::from("test-token".to_owned()));
        AuthMiddleware::new(&config, credential).expect("reqwest client builds")
    }

    /// Endpoint a re-executed copy of this test dials; set only on the child
    /// process [`https_endpoint_trusts_the_platform_roots`] spawns.
    const TLS_ENDPOINT_VAR: &str = "WYRD_TEST_TLS_ENDPOINT";

    /// An `https://` endpoint trusts the platform roots, which honor the
    /// standard `SSL_CERT_FILE`, so a server whose certificate chains to that
    /// file is accepted.
    ///
    /// The platform verifier reads `SSL_CERT_FILE` from the process
    /// environment, so the dial runs in a re-executed copy of this test that
    /// receives the roots file and endpoint through [`std::process::Command`];
    /// the test never changes its own environment.
    ///
    /// # Panics
    /// Panics when the server cannot start or the child's verified dial fails.
    #[tokio::test]
    async fn https_endpoint_trusts_the_platform_roots() {
        use tokio::net::TcpListener;
        use tokio_util::sync::CancellationToken;
        use wyrd_testing::bifrost::peer_ca::BifrostPeerCa;
        use wyrd_tonic::server::{
            GrpcRouterConfig, NoopInterceptor, build_grpc_router, serve_grpc_with_listener,
        };
        use wyrd_tonic::tonic::transport::Identity;
        use wyrd_tonic::tonic_health::server::health_reporter;

        if let Ok(endpoint) = std::env::var(TLS_ENDPOINT_VAR) {
            let config = GrpcConfig {
                endpoint,
                timeout_ms: 2_000,
                connect_retries: 2,
                ..GrpcConfig::default()
            };
            GrpcConnection::connect(&config, make_auth())
                .await
                .expect("TLS dial verified against SSL_CERT_FILE");
            return;
        }

        let ca = BifrostPeerCa::generate("localhost").expect("test CA");
        let leaf = ca.issue_leaf("edge").expect("server leaf");
        let roots = tempfile::NamedTempFile::new().expect("roots file");
        std::fs::write(roots.path(), ca.ca_certificate_pem()).expect("write roots");

        let (_, health) = health_reporter();
        let router = build_grpc_router(
            health,
            NoopInterceptor,
            GrpcRouterConfig {
                reflection_enabled: false,
                tls_identity: Some(Identity::from_pem(
                    leaf.certificate_pem(),
                    leaf.private_key_pem(),
                )),
            },
        )
        .expect("TLS router");
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let port = listener.local_addr().expect("local_addr").port();
        tokio::spawn(serve_grpc_with_listener(
            router,
            listener,
            CancellationToken::new(),
        ));

        let mut child = std::process::Command::new(std::env::current_exe().expect("test binary"));
        child
            .args([
                "--exact",
                "transport::grpc::grpc_connection::https_endpoint_trusts_the_platform_roots",
                "--nocapture",
            ])
            .env("SSL_CERT_FILE", roots.path())
            .env(TLS_ENDPOINT_VAR, format!("https://localhost:{port}"));
        // The child dials the server this runtime serves, so it waits on the
        // blocking pool instead of the runtime thread.
        let output = tokio::task::spawn_blocking(move || child.output())
            .await
            .expect("child wait joins")
            .expect("child test runs");
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            output.status.success() && stdout.contains("1 passed"),
            "child dial failed: {stdout}{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}
