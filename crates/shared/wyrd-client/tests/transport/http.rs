use secrecy::SecretString;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use wyrd_client::auth::AuthMiddleware;
use wyrd_client::config::ClientConfig;
use wyrd_client::error::WyrdClientError;
use wyrd_client::transport::HttpTransport;
use wyrd_client::transport::config::{HTTP_DEFAULT_BASE_URL, HTTP_DEFAULT_TIMEOUT_MS, HttpConfig};
use wyrd_client::transport::credential::ResolvedCredential;
use wyrd_spec::request_id::RequestId;
use wyrd_spec::security::{SecretRef, TlsConfig};

#[test]
fn http_default_values() {
    let h = HttpConfig::default();
    assert_eq!(h.base_url, HTTP_DEFAULT_BASE_URL);
    assert_eq!(h.timeout_ms, HTTP_DEFAULT_TIMEOUT_MS);
    assert!(h.tls.is_none());
    assert!(!h.compression);
}

#[test]
fn http_default_validates_clean() {
    assert!(HttpConfig::default().validate().is_ok());
}

#[test]
fn http_default_round_trips() {
    let h = HttpConfig::default();
    let s = serde_json::to_string(&h).unwrap();
    let back: HttpConfig = serde_json::from_str(&s).unwrap();
    assert_eq!(h, back);
}

#[test]
fn http_deserialize_uses_serde_defaults_for_missing_fields() {
    // `serde(default = "...")` fills `base_url`/`timeout_ms`; `compression`
    // defaults via `#[serde(default)]`. Unknown fields are still rejected by
    // `deny_unknown_fields`.
    let h: HttpConfig = serde_json::from_str("{}").unwrap();
    assert_eq!(h, HttpConfig::default());
}

#[test]
fn http_config_round_trips() {
    let h = HttpConfig {
        base_url: "https://wyrd-ingest.example.com".to_string(),
        timeout_ms: 10_000,
        tls: None,
        compression: true,
    };
    let s = serde_json::to_string(&h).unwrap();
    let back: HttpConfig = serde_json::from_str(&s).unwrap();
    assert_eq!(h, back);
}

#[test]
fn http_compression_false_round_trips() {
    let h = HttpConfig {
        base_url: "https://wyrd.example.com".to_string(),
        timeout_ms: 30_000,
        tls: None,
        compression: false,
    };
    let s = serde_json::to_string(&h).unwrap();
    let back: HttpConfig = serde_json::from_str(&s).unwrap();
    assert_eq!(h, back);
}

#[test]
fn http_validate_rejects_empty_base_url() {
    let h = HttpConfig {
        base_url: String::new(),
        timeout_ms: 5_000,
        tls: None,
        compression: false,
    };
    let err = h.validate().unwrap_err();
    assert_config_error(err, "http_config.base_url", "must not be empty");
}

#[test]
fn http_validate_accepts_non_empty_base_url() {
    let h = HttpConfig {
        base_url: "https://example.com".to_string(),
        timeout_ms: 5_000,
        tls: None,
        compression: false,
    };
    assert!(h.validate().is_ok());
}

#[test]
fn http_validate_rejects_remote_cleartext() {
    let h = HttpConfig {
        base_url: "http://wyrd.example.com".to_string(),
        timeout_ms: 5_000,
        tls: None,
        compression: false,
    };
    let err = h.validate().unwrap_err();
    assert_config_error(
        err,
        "http_config.base_url",
        "remote cleartext HTTP is not allowed; use https:// or a loopback host",
    );
}

#[test]
fn http_validate_rejects_zero_timeout() {
    let h = HttpConfig {
        base_url: "https://example.com".to_string(),
        timeout_ms: 0,
        tls: None,
        compression: false,
    };
    let err = h.validate().unwrap_err();
    assert_config_error(err, "http_config.timeout_ms", "must be at least 1");
}

#[test]
fn http_with_full_tls_round_trips() {
    let h = HttpConfig {
        base_url: "https://wyrd.example.com".to_string(),
        timeout_ms: 5_000,
        tls: Some(TlsConfig {
            ca_cert: Some(SecretRef::File {
                path: "/etc/ssl/ca.pem".to_string(),
            }),
            client_cert: Some(SecretRef::Vault {
                key: "wyrd/tls/cert".to_string(),
            }),
            client_key: Some(SecretRef::Vault {
                key: "wyrd/tls/key".to_string(),
            }),
            server_name_override: Some("ingest.internal".to_string()),
            insecure_skip_verify: false,
        }),
        compression: true,
    };
    let s = serde_json::to_string(&h).unwrap();
    let back: HttpConfig = serde_json::from_str(&s).unwrap();
    assert_eq!(h, back);
}

#[test]
fn http_rejects_unknown_fields() {
    let r: Result<HttpConfig, _> = serde_json::from_str(
        r#"{"base_url":"https://example.com","timeout_ms":5000,"compression":false,"bad":1}"#,
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

/// Explicit query identity crosses the streaming request and is verified on response.
#[tokio::test]
async fn running_query_request_id_and_controls_round_trip() {
    let request_id = RequestId::now_v7();
    let expected = request_id.to_string();
    let mismatch = RequestId::now_v7().to_string();
    for (echo, accepted) in [
        (Some(expected.clone()), true),
        (None, false),
        (Some(mismatch), false),
    ] {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let address = listener.local_addr().expect("address");
        let server_expected = expected.clone();
        let server_echo = echo.clone();
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.expect("accept");
            let mut bytes = vec![0_u8; 4096];
            let read = socket.read(&mut bytes).await.expect("read");
            let request = String::from_utf8_lossy(&bytes[..read]);
            assert!(request.contains(&format!("wyrd-request-id: {server_expected}")));
            let echo_header = server_echo
                .map(|value| format!("wyrd-request-id: {value}\r\n"))
                .unwrap_or_default();
            let response = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: application/vnd.wyrd.bifrost-query-stream\r\n{echo_header}content-length: 0\r\nconnection: close\r\n\r\n"
            );
            socket.write_all(response.as_bytes()).await.expect("write");
        });
        let config = ClientConfig {
            http: HttpConfig {
                base_url: format!("http://{address}"),
                ..HttpConfig::default()
            },
            ..ClientConfig::default()
        };
        let auth = AuthMiddleware::new(
            &config,
            ResolvedCredential::BearerToken(SecretString::from("token".to_owned())),
        )
        .expect("auth");
        let transport = HttpTransport::new(&config.http, auth).expect("transport");
        let result = transport
            .request_json_stream_with_id(
                reqwest::Method::POST,
                "/v1/query",
                &serde_json::json!({"sql": "SELECT 1"}),
                &request_id,
            )
            .await;
        if accepted {
            let response = result.expect("exact echoed request ID accepted");
            assert_eq!(
                response
                    .headers()
                    .get("wyrd-request-id")
                    .and_then(|value| value.to_str().ok()),
                Some(expected.as_str())
            );
        } else {
            let error = result.expect_err("missing or mismatched request ID rejected");
            assert_eq!(error.status(), 502);
            assert_eq!(
                error.as_problem_json()["details"]["reason"],
                "request_id_mismatch"
            );
        }
        server.await.expect("server joins");
    }
}

// ── HttpTransport behavioral tests ────────────────────────────────────────────

mod transport_behavior {
    use std::collections::VecDeque;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use futures_util::StreamExt;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;
    use tokio::sync::Mutex;

    use wyrd_client::WyrdClient;
    use wyrd_client::auth::AuthMiddleware;
    use wyrd_client::config::ClientConfig;
    use wyrd_client::transport::HttpTransport;
    use wyrd_client::transport::config::HttpConfig;
    use wyrd_client::transport::credential::ResolvedCredential;

    // ── mock server helpers ───────────────────────────────────────────────────

    struct MockServer {
        pub base_url: String,
        pub hits: Arc<AtomicUsize>,
        pub captured: Arc<Mutex<Vec<String>>>,
        _handle: tokio::task::JoinHandle<()>,
    }

    /// Scripted local HTTP response with optional delayed body delivery.
    struct MockResponse {
        /// Numeric HTTP response status.
        status: u16,
        /// Complete response body written after any configured delay.
        body: String,
        /// Additional response headers appended to the fixture defaults.
        extra_headers: Vec<(String, String)>,
        /// Optional delay between response headers and body bytes.
        body_delay: Option<std::time::Duration>,
    }

    impl MockResponse {
        /// Builds a successful JSON fixture response.
        fn ok(body: &str) -> Self {
            Self {
                status: 200,
                body: body.to_owned(),
                extra_headers: vec![],
                body_delay: None,
            }
        }

        /// Builds a fixture response with an explicit status.
        fn status(status: u16, body: &str) -> Self {
            Self {
                status,
                body: body.to_owned(),
                extra_headers: vec![],
                body_delay: None,
            }
        }

        /// Appends one response header.
        fn with_header(mut self, name: &str, value: &str) -> Self {
            self.extra_headers.push((name.to_owned(), value.to_owned()));
            self
        }

        /// Delays body bytes after response headers have been written.
        fn with_body_delay(mut self, delay: std::time::Duration) -> Self {
            self.body_delay = Some(delay);
            self
        }
    }

    async fn spawn_mock(responses: Vec<MockResponse>) -> MockServer {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let addr = listener.local_addr().expect("addr");
        let hits = Arc::new(AtomicUsize::new(0));
        let captured: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
        let responses = Arc::new(Mutex::new(responses.into_iter().collect::<VecDeque<_>>()));

        let hits_clone = hits.clone();
        let captured_clone = captured.clone();

        let handle = tokio::spawn(async move {
            loop {
                let Ok((mut stream, _)) = listener.accept().await else {
                    break;
                };
                let hits_inner = hits_clone.clone();
                let captured_inner = captured_clone.clone();
                let responses_inner = responses.clone();

                tokio::spawn(async move {
                    hits_inner.fetch_add(1, Ordering::SeqCst);
                    let mut buf = vec![0u8; 8192];
                    let n = stream.read(&mut buf).await.unwrap_or(0);
                    let raw = String::from_utf8_lossy(&buf[..n]).to_string();
                    captured_inner.lock().await.push(raw);

                    let MockResponse {
                        status,
                        body,
                        extra_headers,
                        body_delay,
                    } = responses_inner
                        .lock()
                        .await
                        .pop_front()
                        .unwrap_or_else(|| MockResponse::ok("{}"));

                    let has_content_type = extra_headers
                        .iter()
                        .any(|(name, _)| name.eq_ignore_ascii_case("content-type"));
                    let content_type = if has_content_type {
                        String::new()
                    } else {
                        "content-type: application/json\r\n".to_owned()
                    };
                    let mut response = format!(
                        "HTTP/1.1 {status} Status\r\n{content_type}content-length: {}\r\nconnection: close\r\n",
                        body.len()
                    );
                    for (name, value) in &extra_headers {
                        response.push_str(&format!("{name}: {value}\r\n"));
                    }
                    response.push_str("\r\n");
                    let _ = stream.write_all(response.as_bytes()).await;
                    if let Some(delay) = body_delay {
                        tokio::time::sleep(delay).await;
                    }
                    let _ = stream.write_all(body.as_bytes()).await;
                });
            }
        });

        MockServer {
            base_url: format!("http://{addr}"),
            hits,
            captured,
            _handle: handle,
        }
    }

    fn make_transport(base_url: String) -> HttpTransport {
        let credential = ResolvedCredential::BearerToken("test-bearer".to_owned().into());
        let config = ClientConfig::default();
        let auth = AuthMiddleware::new(&config, credential).expect("auth builds");
        let http_config = HttpConfig {
            base_url,
            ..HttpConfig::default()
        };
        HttpTransport::new(&http_config, auth).expect("transport builds")
    }

    /// Build a transport whose auth exchanges an `ApiKey` against `base_url`,
    /// so a `401` re-exchange actually rotates the cached token (unlike the
    /// no-op `BearerToken` path).
    fn make_api_key_transport(base_url: String) -> HttpTransport {
        let credential = ResolvedCredential::ApiKey("api-key-value".to_owned().into());
        let mut config = ClientConfig::default();
        config.http.base_url = base_url.clone();
        let auth = AuthMiddleware::new(&config, credential).expect("auth builds");
        let http_config = HttpConfig {
            base_url,
            ..HttpConfig::default()
        };
        HttpTransport::new(&http_config, auth).expect("transport builds")
    }

    /// Build an API-key [`WyrdClient`] over a mock server, so a public handle
    /// exercises the same renewal the transport owns.
    fn make_api_key_client(base_url: String) -> WyrdClient {
        let credential = ResolvedCredential::ApiKey("api-key-value".to_owned().into());
        let mut config = ClientConfig::default();
        config.http.base_url = base_url.clone();
        let auth = AuthMiddleware::new(&config, credential).expect("auth builds");
        let http_config = HttpConfig {
            base_url,
            ..HttpConfig::default()
        };
        let transport =
            HttpTransport::new(&http_config, Arc::clone(&auth)).expect("transport builds");
        wyrd_client::WyrdClient::from_parts(auth, transport, config.grpc)
    }

    /// Revoking a credential renews once after an authentication refusal, like
    /// every other control call.
    ///
    /// This operation used to be the one control request that sent exactly once
    /// and never refreshed, so a durable API key whose cached bearer the server
    /// had stopped accepting could revoke nothing while its neighbours renewed
    /// and succeeded. The 204 the route answers with carries no body, which is
    /// what had made the raw path look like the natural fit.
    #[tokio::test]
    async fn revoke_credential_re_exchanges_once_and_replays() {
        let server = spawn_mock(vec![
            MockResponse::ok(&token_response("tok-A")),
            MockResponse::status(
                401,
                r#"{"code":"WYRD_AUTH_401_INVALID_TOKEN","detail":"unauthorized","details":{}}"#,
            ),
            MockResponse::ok(&token_response("tok-B")),
            MockResponse::status(204, ""),
        ])
        .await;
        let principals = wyrd_client::principals::Principals::with_client(make_api_key_client(
            server.base_url.clone(),
        ));
        let principal = wyrd_spec::auth::PrincipalId::new(
            "01890f28-7c4a-7cc3-98e7-4f4a3c2d1b00"
                .parse()
                .expect("principal id parses"),
        );

        principals
            .revoke_credential(&principal, "cred-1")
            .await
            .expect("the replay after renewal retires the credential");

        assert_eq!(
            server.hits.load(Ordering::SeqCst),
            4,
            "expected: exchange, 401, re-exchange, replay"
        );
        let captured = server.captured.lock().await;
        assert_eq!(
            extract_header(&captured[3], "x-wyrd-access-token").as_deref(),
            Some("Bearer tok-B"),
            "the replay carries the re-exchanged token"
        );
        assert!(
            captured[3].starts_with("DELETE /v1/principals/"),
            "the replay repeats the same revocation: {}",
            captured[3].lines().next().unwrap_or_default()
        );
    }

    /// A second authentication refusal is terminal: renewal is replayed once,
    /// never in a loop.
    #[tokio::test]
    async fn revoke_credential_stops_after_a_second_refusal() {
        let unauthorized =
            r#"{"code":"WYRD_AUTH_401_INVALID_TOKEN","detail":"unauthorized","details":{}}"#;
        let server = spawn_mock(vec![
            MockResponse::ok(&token_response("tok-A")),
            MockResponse::status(401, unauthorized),
            MockResponse::ok(&token_response("tok-B")),
            MockResponse::status(401, unauthorized),
        ])
        .await;
        let principals = wyrd_client::principals::Principals::with_client(make_api_key_client(
            server.base_url.clone(),
        ));
        let principal = wyrd_spec::auth::PrincipalId::new(
            "01890f28-7c4a-7cc3-98e7-4f4a3c2d1b00"
                .parse()
                .expect("principal id parses"),
        );

        principals
            .revoke_credential(&principal, "cred-1")
            .await
            .expect_err("a refused replay is reported, not retried again");

        assert_eq!(
            server.hits.load(Ordering::SeqCst),
            4,
            "expected: exchange, 401, re-exchange, refused replay — and no more"
        );
    }

    fn token_response(access: &str) -> String {
        serde_json::json!({
            "access_token": access,
            "refresh_token": "drop",
            "token_type": "Bearer",
            "expires_at": "2099-01-01T00:00:00Z",
        })
        .to_string()
    }

    fn extract_header(raw: &str, name: &str) -> Option<String> {
        let lower = name.to_lowercase();
        for line in raw.lines() {
            if let Some(pos) = line.find(':')
                && line[..pos].trim().to_lowercase() == lower
            {
                return Some(line[pos + 1..].trim().to_owned());
            }
        }
        None
    }

    // ── tests ─────────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn request_json_round_trips_json() {
        let server = spawn_mock(vec![MockResponse::ok(r#"{"result":"ok"}"#)]).await;
        let t = make_transport(server.base_url.clone());
        let body = serde_json::json!({"key": "value"});

        let response: serde_json::Value = t
            .request_json(reqwest::Method::POST, "/v1/test", Some(&body))
            .await
            .expect("request ok");

        assert_eq!(response["result"], "ok");
        assert_eq!(server.hits.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn problem_json_error_maps_to_wyrd_error() {
        let problem = serde_json::json!({
            "code": "WYRD_SPEC_404_NOT_FOUND",
            "detail": "card not found",
            "details": {},
        })
        .to_string();
        let server = spawn_mock(vec![MockResponse::status(404, &problem)]).await;
        let t = make_transport(server.base_url.clone());

        let err = t
            .request_json::<serde_json::Value, serde_json::Value>(
                reqwest::Method::GET,
                "/v1/cards/missing",
                None,
            )
            .await
            .expect_err("404 must return error");

        assert_eq!(err.code(), "WYRD_SPEC_404_NOT_FOUND");
    }

    #[tokio::test]
    async fn request_arrow_reads_bytes_and_metadata_headers() {
        let arrow_bytes = b"\x00\x01\x02\x03arrow-ipc-payload";
        let server = spawn_mock(vec![
            MockResponse::ok(&String::from_utf8_lossy(arrow_bytes))
                .with_header("X-Wyrd-Schema-Fingerprint", "sha256:abc123")
                .with_header("X-Wyrd-Row-Count", "42")
                .with_header("content-type", "application/vnd.apache.arrow.stream"),
        ])
        .await;
        let t = make_transport(server.base_url.clone());

        let resp = t
            .request_arrow::<serde_json::Value>(reqwest::Method::GET, "/v1/query", None)
            .await
            .expect("arrow request ok");

        assert!(
            !resp.frames.is_empty(),
            "frames must not be empty for a 2xx Arrow response"
        );
        assert_eq!(
            resp.schema_fingerprint.as_deref(),
            Some("sha256:abc123"),
            "schema_fingerprint header must be forwarded"
        );
        assert_eq!(
            resp.row_count,
            Some(42),
            "row_count header must be parsed as u64"
        );
    }

    #[tokio::test]
    async fn request_arrow_missing_headers_returns_none() {
        let server = spawn_mock(vec![MockResponse::ok(r#"fake-arrow"#)]).await;
        let t = make_transport(server.base_url.clone());

        let resp = t
            .request_arrow::<serde_json::Value>(reqwest::Method::GET, "/v1/query", None)
            .await
            .expect("arrow request ok");

        assert!(resp.schema_fingerprint.is_none());
        assert!(resp.row_count.is_none());
    }

    #[tokio::test]
    async fn wyrd_request_id_is_present_on_every_request() {
        let server = spawn_mock(vec![MockResponse::ok("{}")]).await;
        let t = make_transport(server.base_url.clone());

        let _: serde_json::Value = t
            .request_json(reqwest::Method::GET, "/v1/ping", None::<&serde_json::Value>)
            .await
            .expect("request ok");

        let captured = server.captured.lock().await;
        let raw = captured.first().expect("one request captured");
        let id =
            extract_header(raw, "wyrd-request-id").expect("wyrd-request-id header must be present");

        let parsed = uuid::Uuid::parse_str(&id).expect("wyrd-request-id must be a valid UUID");
        assert_eq!(
            parsed.get_version_num(),
            7,
            "wyrd-request-id must be UUIDv7"
        );
    }

    #[tokio::test]
    async fn submit_idempotent_replays_key_across_retry() {
        let ok_body = r#"{"job_id":"123"}"#;
        let server = spawn_mock(vec![
            MockResponse::status(
                503,
                r#"{"code":"WYRD_SPEC_500_INTERNAL","detail":"overloaded","details":{}}"#,
            ),
            MockResponse::ok(ok_body),
        ])
        .await;
        let t = make_transport(server.base_url.clone());
        let payload = serde_json::json!({"op": "run"});

        let result: serde_json::Value = t
            .submit_idempotent(reqwest::Method::POST, "/v1/jobs", &payload)
            .await
            .expect("submit ok on second attempt");

        assert_eq!(result["job_id"], "123");
        assert_eq!(
            server.hits.load(Ordering::SeqCst),
            2,
            "must retry exactly once after 503"
        );

        let captured = server.captured.lock().await;
        let key_first = extract_header(&captured[0], "Idempotency-Key").expect("key on attempt 1");
        let key_second = extract_header(&captured[1], "Idempotency-Key").expect("key on attempt 2");

        assert_eq!(
            key_first, key_second,
            "Idempotency-Key must be identical on every retry attempt"
        );
        assert!(!key_first.is_empty(), "Idempotency-Key must be non-empty");
    }

    #[tokio::test]
    async fn submit_with_idempotency_key_replays_caller_key_across_retry() {
        let server = spawn_mock(vec![
            MockResponse::status(
                503,
                r#"{"code":"WYRD_SPEC_500_INTERNAL","detail":"overloaded","details":{}}"#,
            ),
            MockResponse::ok(r#"{"job_id":"456"}"#),
        ])
        .await;
        let transport = make_transport(server.base_url.clone());
        let payload = serde_json::json!({"op": "run"});

        let _: serde_json::Value = transport
            .submit_with_idempotency_key(
                reqwest::Method::POST,
                "/v1/jobs",
                &payload,
                "wyrd-engine-deterministic-key",
            )
            .await
            .expect("submit succeeds on retry");

        let captured = server.captured.lock().await;
        assert_eq!(
            extract_header(&captured[0], "Idempotency-Key").as_deref(),
            Some("wyrd-engine-deterministic-key")
        );
        assert_eq!(
            extract_header(&captured[1], "Idempotency-Key").as_deref(),
            Some("wyrd-engine-deterministic-key")
        );
    }

    #[tokio::test]
    async fn four_oh_one_re_exchanges_api_key_and_retries_with_fresh_token() {
        // With an ApiKey credential the reactive 401 path must re-exchange
        // and retry with the *new* token. Scripted exchange: the mock serves,
        // in order, token tok-A, a 401 on the protected route, token tok-B, then
        // 200. The retry must carry tok-B, and /auth/token must be hit twice.
        let server = spawn_mock(vec![
            MockResponse::ok(&token_response("tok-A")),
            MockResponse::status(
                401,
                r#"{"code":"WYRD_AUTH_401_INVALID_TOKEN","detail":"unauthorized","details":{}}"#,
            ),
            MockResponse::ok(&token_response("tok-B")),
            MockResponse::ok(r#"{"ok":true}"#),
        ])
        .await;
        let t = make_api_key_transport(server.base_url.clone());

        let result: serde_json::Value = t
            .request_json(
                reqwest::Method::GET,
                "/v1/protected",
                None::<&serde_json::Value>,
            )
            .await
            .expect("second attempt succeeds after re-exchange");

        assert_eq!(result["ok"], true);
        assert_eq!(
            server.hits.load(Ordering::SeqCst),
            4,
            "expected: exchange, 401, re-exchange, retry"
        );

        let captured = server.captured.lock().await;
        // captured[0] and [2] are the two /auth/token exchanges; [1] and [3] are
        // the protected-route attempts.
        let token_attempts = captured
            .iter()
            .filter(|raw| raw.contains("/auth/token"))
            .count();
        assert_eq!(token_attempts, 2, "/auth/token must be hit exactly twice");

        let first = extract_header(&captured[1], "x-wyrd-access-token")
            .expect("first protected attempt carries a token");
        let retried = extract_header(&captured[3], "x-wyrd-access-token")
            .expect("retried protected attempt carries a token");
        assert_eq!(
            first, "Bearer tok-A",
            "first attempt uses the original token"
        );
        assert_eq!(
            retried, "Bearer tok-B",
            "the retry must use the re-exchanged token, not the stale one"
        );
    }

    #[tokio::test]
    async fn same_origin_absolute_url_is_accepted_for_authenticated_requests() {
        // V-001 (same-origin allowed): LocalFs upload plans may hand the
        // client back a full absolute URL rooted at the configured Wyrd
        // origin. That must still work, and it must carry the
        // `x-wyrd-access-token` bearer.
        let server = spawn_mock(vec![MockResponse::ok("{}")]).await;
        let t = make_transport(server.base_url.clone());
        let absolute = format!("{}/v1/cards/upload/local/blob", server.base_url);

        let _: serde_json::Value = t
            .request_json(reqwest::Method::GET, &absolute, None::<&serde_json::Value>)
            .await
            .expect("same-origin absolute URL must be accepted");

        let captured = server.captured.lock().await;
        let raw = captured.first().expect("one request captured");
        assert!(
            extract_header(raw, "x-wyrd-access-token").is_some(),
            "same-origin absolute URL must still carry the Wyrd bearer"
        );
    }

    #[tokio::test]
    async fn cross_origin_absolute_url_is_rejected_for_authenticated_requests() {
        // V-001 (leak prevention): if an authenticated helper is handed an
        // absolute URL that does not match the configured Wyrd origin, the
        // request must fail *before* the client attaches any Wyrd headers or
        // opens a socket to the third-party host. This guards against a
        // server-minted plan that inadvertently points a LocalFs URL at an
        // attacker-controlled host.
        let server = spawn_mock(vec![MockResponse::ok("{}")]).await;
        let t = make_transport(server.base_url.clone());

        let attacker_url = "https://attacker.example.com/v1/cards/upload/local/blob";
        let err = t
            .request_json::<serde_json::Value, serde_json::Value>(
                reqwest::Method::GET,
                attacker_url,
                None,
            )
            .await
            .expect_err("cross-origin absolute URL must be rejected");
        assert_eq!(err.code(), "WYRD_SPEC_400_VALIDATION");
        assert_eq!(
            server.hits.load(Ordering::SeqCst),
            0,
            "no socket may be opened to a cross-origin authenticated target"
        );

        let stream_err = t
            .request_stream(
                reqwest::Method::PUT,
                attacker_url,
                reqwest::Body::from("payload"),
            )
            .await
            .expect_err("request_stream must reject cross-origin URLs");
        assert_eq!(stream_err.code(), "WYRD_SPEC_400_VALIDATION");

        let raw_err = t
            .request_raw(reqwest::Method::GET, attacker_url)
            .await
            .expect_err("request_raw must reject cross-origin URLs");
        assert_eq!(raw_err.code(), "WYRD_SPEC_400_VALIDATION");
    }

    #[tokio::test]
    async fn wyrd_access_token_header_is_injected_not_authorization() {
        // The Wyrd JWT must travel in `x-wyrd-access-token`
        // (the only header the server authenticates from), and the SDK must NOT
        // write the caller's reserved `Authorization` header.
        let server = spawn_mock(vec![MockResponse::ok("{}")]).await;
        let t = make_transport(server.base_url.clone());

        let _: serde_json::Value = t
            .request_json(reqwest::Method::GET, "/v1/ping", None::<&serde_json::Value>)
            .await
            .expect("request ok");

        let captured = server.captured.lock().await;
        let token = extract_header(&captured[0], "x-wyrd-access-token")
            .expect("x-wyrd-access-token header must be present");
        assert!(
            token.starts_with("Bearer "),
            "x-wyrd-access-token must be 'Bearer <token>', got {token:?}"
        );
        assert!(
            extract_header(&captured[0], "Authorization").is_none(),
            "the SDK must not write the caller's reserved Authorization header"
        );
    }

    /// Proves streaming JSON POSTs retain auth and negotiate the closed media type.
    #[tokio::test]
    async fn request_json_stream_authenticates_and_preserves_body_stream() {
        let server =
            spawn_mock(vec![MockResponse::ok("frame-bytes").with_header(
                "content-type",
                "application/vnd.wyrd.bifrost-query-stream",
            )])
            .await;
        let transport = make_transport(server.base_url);
        let response = transport
            .request_json_stream(
                reqwest::Method::POST,
                "/v1/query",
                &serde_json::json!({"sql": "SELECT 1"}),
            )
            .await
            .expect("stream response accepted");
        assert_eq!(
            response.bytes().await.expect("response bytes"),
            "frame-bytes"
        );
        let captured = server.captured.lock().await;
        assert_eq!(
            extract_header(&captured[0], "accept").as_deref(),
            Some("application/vnd.wyrd.bifrost-query-stream")
        );
        assert!(
            extract_header(&captured[0], "x-wyrd-access-token").is_some(),
            "streaming request carries the Wyrd bearer"
        );
    }

    /// Delay applied to every slow fixture leg; four times the short timeout.
    const SLOW_DELAY: std::time::Duration = std::time::Duration::from_millis(80);

    /// Builds a bearer transport whose `timeout_ms` is shorter than [`SLOW_DELAY`].
    ///
    /// # Panics
    /// Panics when the auth middleware or transport cannot be built.
    fn make_short_timeout_transport(base_url: String) -> HttpTransport {
        let credential = ResolvedCredential::BearerToken("test-bearer".to_owned().into());
        let auth = AuthMiddleware::new(&ClientConfig::default(), credential).expect("auth builds");
        HttpTransport::new(
            &HttpConfig {
                base_url,
                timeout_ms: 20,
                ..HttpConfig::default()
            },
            auth,
        )
        .expect("transport builds")
    }

    /// Proves retried JSON bodies keep the configured total deadline while a
    /// terminal query stream outlives it on the same transport.
    ///
    /// # Panics
    /// Panics when the delayed JSON body does not time out or the delayed query
    /// stream body does not arrive.
    #[tokio::test]
    async fn request_json_stream_outlives_ordinary_total_timeout() {
        let server = spawn_mock(vec![
            MockResponse::ok("{}").with_body_delay(SLOW_DELAY),
            MockResponse::ok("frame-bytes")
                .with_header("content-type", "application/vnd.wyrd.bifrost-query-stream")
                .with_body_delay(SLOW_DELAY),
        ])
        .await;
        let transport = make_short_timeout_transport(server.base_url);

        let ordinary = transport
            .request_json::<(), serde_json::Value>(reqwest::Method::GET, "/v1/ordinary", None)
            .await
            .expect_err("a delayed JSON body keeps its total deadline");
        assert!(
            ordinary.to_string().contains("reading response body"),
            "the JSON body read timed out: {ordinary}"
        );

        let streaming = transport
            .request_json_stream(
                reqwest::Method::POST,
                "/v1/query",
                &serde_json::json!({"sql": "SELECT 1"}),
            )
            .await
            .expect("stream response headers arrive");
        assert_eq!(
            streaming.bytes().await.expect("delayed stream body"),
            "frame-bytes"
        );
    }

    /// Proves an authenticated streaming GET body outlives `timeout_ms`.
    ///
    /// # Panics
    /// Panics when the delayed body fails or the same-origin request lacks the
    /// Wyrd bearer.
    #[tokio::test]
    async fn request_raw_slow_download_outlives_timeout() {
        let server = spawn_mock(vec![
            MockResponse::ok("object-bytes").with_body_delay(SLOW_DELAY),
        ])
        .await;
        let transport = make_short_timeout_transport(server.base_url);
        let response = transport
            .request_raw(reqwest::Method::GET, "/v1/storage/object")
            .await
            .expect("download headers arrive");
        assert_eq!(
            response.bytes().await.expect("slow download"),
            "object-bytes"
        );
        let captured = server.captured.lock().await;
        assert!(extract_header(&captured[0], "x-wyrd-access-token").is_some());
    }

    /// Proves a credential-free external streaming GET body outlives `timeout_ms`.
    ///
    /// # Panics
    /// Panics when the delayed body fails or the presigned request carries
    /// Wyrd credentials.
    #[tokio::test]
    async fn request_external_stream_slow_download_outlives_timeout() {
        let server = spawn_mock(vec![
            MockResponse::ok("object-bytes").with_body_delay(SLOW_DELAY),
        ])
        .await;
        let transport = make_short_timeout_transport(HTTP_TEST_ORIGIN.to_owned());
        let response = transport
            .request_external_stream(
                reqwest::Method::GET,
                &format!("{}/bucket/object", server.base_url),
                None,
                &[],
            )
            .await
            .expect("download headers arrive");
        assert_eq!(
            response.bytes().await.expect("slow download"),
            "object-bytes"
        );
        let captured = server.captured.lock().await;
        assert!(extract_header(&captured[0], "x-wyrd-access-token").is_none());
        assert!(extract_header(&captured[0], "wyrd-request-id").is_none());
    }

    /// Proves an authenticated one-shot streaming PUT outlives `timeout_ms`.
    ///
    /// # Panics
    /// Panics when the upload fails, the server receives different bytes, or
    /// the same-origin request lacks the Wyrd bearer.
    #[tokio::test]
    async fn request_stream_slow_upload_outlives_timeout() {
        let server = spawn_upload_mock().await;
        let transport = make_short_timeout_transport(server.base_url.clone());
        let response = transport
            .request_stream(reqwest::Method::PUT, "/v1/storage/object", slow_body())
            .await
            .expect("slow upload accepted");
        assert!(response.status().is_success());
        let (head, body) = server.received().await;
        assert_eq!(body, SLOW_UPLOAD.concat().into_bytes());
        assert!(extract_header(&head, "x-wyrd-access-token").is_some());
    }

    /// Proves a credential-free external streaming PUT outlives `timeout_ms`.
    ///
    /// # Panics
    /// Panics when the upload fails, the server receives different bytes, or
    /// the presigned request carries Wyrd credentials.
    #[tokio::test]
    async fn request_external_stream_slow_upload_outlives_timeout() {
        let server = spawn_upload_mock().await;
        let transport = make_short_timeout_transport(HTTP_TEST_ORIGIN.to_owned());
        let response = transport
            .request_external_stream(
                reqwest::Method::PUT,
                &format!("{}/bucket/object", server.base_url),
                Some(slow_body()),
                &[("content-type", "application/octet-stream")],
            )
            .await
            .expect("slow upload accepted");
        assert!(response.status().is_success());
        let (head, body) = server.received().await;
        assert_eq!(body, SLOW_UPLOAD.concat().into_bytes());
        assert!(extract_header(&head, "x-wyrd-access-token").is_none());
        assert!(extract_header(&head, "wyrd-request-id").is_none());
    }

    /// Origin configured for external-transfer tests; never contacted.
    const HTTP_TEST_ORIGIN: &str = "http://127.0.0.1:9";

    /// Upload chunks delivered with [`SLOW_DELAY`] before each one.
    const SLOW_UPLOAD: [&str; 3] = ["alpha-", "beta-", "gamma"];

    /// Builds a streaming request body that yields [`SLOW_UPLOAD`] slowly, so
    /// the whole transfer lasts several times the transport timeout.
    fn slow_body() -> reqwest::Body {
        let chunks = futures_util::stream::iter(SLOW_UPLOAD).then(|chunk| async move {
            tokio::time::sleep(SLOW_DELAY).await;
            Ok::<_, std::io::Error>(bytes::Bytes::from_static(chunk.as_bytes()))
        });
        reqwest::Body::wrap_stream(chunks)
    }

    /// Single-request upload fixture that consumes the full request body before
    /// answering `200`.
    struct UploadMock {
        /// Base URL of the listening fixture.
        base_url: String,
        /// Request head and decoded body, available once fully consumed.
        received: tokio::sync::oneshot::Receiver<(String, Vec<u8>)>,
    }

    impl UploadMock {
        /// Waits for the fixture to finish consuming the request.
        ///
        /// # Panics
        /// Panics when the fixture task ended without recording a request.
        async fn received(self) -> (String, Vec<u8>) {
            self.received
                .await
                .expect("upload fixture consumed the request")
        }
    }

    /// Spawns an [`UploadMock`] that decodes a chunked or sized request body
    /// completely and only then writes its response.
    ///
    /// # Panics
    /// Panics when binding fails; the spawned task panics on malformed requests.
    async fn spawn_upload_mock() -> UploadMock {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let base_url = format!("http://{}", listener.local_addr().expect("addr"));
        let (sender, received) = tokio::sync::oneshot::channel();
        tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.expect("accept upload");
            let mut raw = Vec::new();
            let mut buf = [0u8; 4096];
            let head_end = loop {
                if let Some(end) = raw.windows(4).position(|window| window == b"\r\n\r\n") {
                    break end + 4;
                }
                let read = stream.read(&mut buf).await.expect("read head");
                assert!(read > 0, "request head ended early");
                raw.extend_from_slice(&buf[..read]);
            };
            let head = String::from_utf8_lossy(&raw[..head_end]).to_string();
            let mut rest = raw.split_off(head_end);
            let body = if let Some(length) = extract_header(&head, "content-length") {
                let length: usize = length.parse().expect("content-length");
                while rest.len() < length {
                    let read = stream.read(&mut buf).await.expect("read body");
                    assert!(read > 0, "sized body ended early");
                    rest.extend_from_slice(&buf[..read]);
                }
                rest
            } else {
                while !rest.ends_with(b"0\r\n\r\n") {
                    let read = stream.read(&mut buf).await.expect("read chunk");
                    assert!(read > 0, "chunked body ended early");
                    rest.extend_from_slice(&buf[..read]);
                }
                let mut decoded = Vec::new();
                let mut cursor = rest.as_slice();
                loop {
                    let line = cursor
                        .windows(2)
                        .position(|window| window == b"\r\n")
                        .expect("chunk size line");
                    let size = usize::from_str_radix(
                        std::str::from_utf8(&cursor[..line]).expect("chunk size"),
                        16,
                    )
                    .expect("hex chunk size");
                    if size == 0 {
                        break decoded;
                    }
                    decoded.extend_from_slice(&cursor[line + 2..line + 2 + size]);
                    cursor = &cursor[line + 2 + size + 2..];
                }
            };
            let _ = stream
                .write_all(b"HTTP/1.1 200 OK\r\ncontent-length: 0\r\nconnection: close\r\n\r\n")
                .await;
            let _ = sender.send((head, body));
        });
        UploadMock { base_url, received }
    }

    /// Proves a successful response with the wrong media type is rejected.
    #[tokio::test]
    async fn request_json_stream_rejects_unapproved_media_type() {
        let server = spawn_mock(vec![MockResponse::ok("not-a-query-stream")]).await;
        let transport = make_transport(server.base_url);
        let error = transport
            .request_json_stream(
                reqwest::Method::POST,
                "/v1/query",
                &serde_json::json!({"sql": "SELECT 1"}),
            )
            .await
            .expect_err("JSON success must not masquerade as a query stream");
        assert_eq!(error.code(), "WYRD_SPEC_502_UPSTREAM_FAILURE");
    }
}
