use wyrd_client::error::WyrdClientError;
use wyrd_client::transport::config::GrpcConfig;
use wyrd_spec::security::{SecretRef, TlsConfig};

#[test]
fn grpc_default_values() {
    let g = GrpcConfig::default();
    assert_eq!(g.endpoint, "http://localhost:50051");
    assert_eq!(g.timeout_ms, 30_000);
    assert_eq!(g.connect_retries, 3);
    assert!(g.tls.is_none());
}

#[test]
fn grpc_default_round_trips() {
    let g = GrpcConfig::default();
    let s = serde_json::to_string(&g).unwrap();
    let back: GrpcConfig = serde_json::from_str(&s).unwrap();
    assert_eq!(g, back);
}

#[test]
fn grpc_with_tls_round_trips() {
    let g = GrpcConfig {
        tls: Some(TlsConfig {
            ca_cert: Some(SecretRef::File {
                path: "/etc/ssl/ca.pem".to_string(),
            }),
            client_cert: None,
            client_key: None,
            server_name_override: None,
            insecure_skip_verify: false,
        }),
        ..GrpcConfig::default()
    };
    let s = serde_json::to_string(&g).unwrap();
    let back: GrpcConfig = serde_json::from_str(&s).unwrap();
    assert_eq!(g, back);
}

#[test]
fn grpc_validate_rejects_empty_endpoint() {
    let g = GrpcConfig {
        endpoint: String::new(),
        ..GrpcConfig::default()
    };
    let err = g.validate().unwrap_err();
    assert_config_error(err, "grpc_config.endpoint", "must not be empty");
}

#[test]
fn grpc_validate_accepts_non_empty_endpoint() {
    let g = GrpcConfig::default();
    assert!(g.validate().is_ok());
}

#[test]
fn grpc_validate_rejects_zero_timeout() {
    let g = GrpcConfig {
        timeout_ms: 0,
        ..GrpcConfig::default()
    };
    let err = g.validate().unwrap_err();
    assert_config_error(err, "grpc_config.timeout_ms", "must be at least 1");
}

#[test]
fn grpc_validate_accepts_zero_connect_retries() {
    let g = GrpcConfig {
        connect_retries: 0,
        ..GrpcConfig::default()
    };
    assert!(g.validate().is_ok());
}

#[test]
fn grpc_rejects_unknown_fields() {
    let r: Result<GrpcConfig, _> = serde_json::from_str(
        r#"{"endpoint":"http://localhost:50051","timeout_ms":5000,"connect_retries":1,"unknown_field":true}"#,
    );
    assert!(r.is_err());
}

fn assert_config_error(err: WyrdClientError, expected_field: &str, expected_reason: &str) {
    let WyrdClientError::Config { field, reason } = err else {
        panic!("expected WyrdClientError::Config");
    };
    assert_eq!(field, expected_field);
    assert_eq!(reason, expected_reason);
}

mod grpc_connection {
    use std::sync::Arc;

    use wyrd_client::auth::AuthMiddleware;
    use wyrd_client::config::ClientConfig;
    use wyrd_client::error::WyrdClientError;
    use wyrd_client::transport::GrpcConnection;
    use wyrd_client::transport::config::GrpcConfig;
    use wyrd_client::transport::credential::ResolvedCredential;

    fn make_auth() -> Arc<AuthMiddleware> {
        let config = ClientConfig::default();
        let credential =
            ResolvedCredential::BearerToken(secrecy::SecretString::from("test-token".to_owned()));
        AuthMiddleware::new(&config, credential).expect("reqwest client builds")
    }

    /// Spin up a minimal TCP listener that performs just enough of the HTTP/2
    /// server handshake for tonic's `Endpoint::connect()` to return `Ok`.
    ///
    /// Sends the server connection preface (an empty SETTINGS frame) after
    /// accepting the first TCP connection, then keeps the socket open for the
    /// duration of the test so tonic's background driver does not see an
    /// immediate EOF.
    async fn stub_h2_listener() -> std::net::SocketAddr {
        use tokio::io::AsyncWriteExt as _;
        use tokio::net::TcpListener;

        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let addr = listener.local_addr().expect("local_addr");
        tokio::spawn(async move {
            if let Ok((mut stream, _)) = listener.accept().await {
                // HTTP/2 server connection preface: empty SETTINGS frame.
                // Frame format: 3-byte length | 1-byte type (0x04) |
                //               1-byte flags | 4-byte stream-id (big-endian).
                let settings = [0x00u8, 0x00, 0x00, 0x04, 0x00, 0x00, 0x00, 0x00, 0x00];
                let _ = stream.write_all(&settings).await;
                // Hold the connection open so tonic's connection driver does
                // not get an immediate EOF before the test verifies the result.
                tokio::time::sleep(std::time::Duration::from_secs(5)).await;
            }
        });
        addr
    }

    #[tokio::test]
    async fn dial_failure_invalid_url_maps_to_transport_down() {
        let config = GrpcConfig {
            endpoint: "not a valid uri".to_owned(),
            connect_retries: 0,
            ..GrpcConfig::default()
        };
        let err = GrpcConnection::connect(&config, make_auth())
            .await
            .unwrap_err();
        let WyrdClientError::TransportDown { transport, .. } = err else {
            panic!("expected TransportDown, got {err:?}");
        };
        assert_eq!(transport, "grpc");
    }

    #[tokio::test]
    async fn connect_stub_yields_channel_and_auth() {
        let addr = stub_h2_listener().await;
        let config = GrpcConfig {
            endpoint: format!("http://{addr}"),
            timeout_ms: 2_000,
            connect_retries: 0,
            ..GrpcConfig::default()
        };
        let auth = make_auth();
        let auth_for_check = Arc::clone(&auth);

        let conn = GrpcConnection::connect(&config, auth)
            .await
            .expect("connect to stub succeeds");

        let _channel = conn.channel();
        assert!(
            Arc::ptr_eq(&auth_for_check, &conn.auth()),
            "auth() must return the same Arc"
        );
    }

    #[tokio::test]
    async fn max_message_bytes_reflects_config() {
        let addr = stub_h2_listener().await;
        let config = GrpcConfig {
            endpoint: format!("http://{addr}"),
            timeout_ms: 2_000,
            connect_retries: 0,
            max_message_bytes: 8 * 1024 * 1024,
            ..GrpcConfig::default()
        };
        let conn = GrpcConnection::connect(&config, make_auth())
            .await
            .expect("connect to stub succeeds");

        assert_eq!(conn.max_message_bytes(), 8 * 1024 * 1024);
    }

    #[tokio::test]
    async fn keepalive_disabled_when_interval_zero() {
        // GrpcConfig with keepalive_interval_ms = 0 must still connect without
        // error — keepalive is simply not configured on the Endpoint.
        let addr = stub_h2_listener().await;
        let config = GrpcConfig {
            endpoint: format!("http://{addr}"),
            timeout_ms: 2_000,
            connect_retries: 0,
            keepalive_interval_ms: 0,
            ..GrpcConfig::default()
        };
        let conn = GrpcConnection::connect(&config, make_auth())
            .await
            .expect("connect without keepalive succeeds");

        let _ = conn.channel();
    }
}
