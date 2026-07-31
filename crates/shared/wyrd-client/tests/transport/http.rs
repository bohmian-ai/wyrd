use wyrd_client::error::WyrdClientError;
use wyrd_client::transport::config::{HTTP_DEFAULT_BASE_URL, HTTP_DEFAULT_TIMEOUT_MS, HttpConfig};
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

// ── HttpTransport behavioral tests ────────────────────────────────────────────

mod transport_behavior {
    use std::collections::VecDeque;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;
    use tokio::sync::Mutex;

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

    struct MockResponse {
        status: u16,
        body: String,
        extra_headers: Vec<(String, String)>,
    }

    impl MockResponse {
        fn ok(body: &str) -> Self {
            Self {
                status: 200,
                body: body.to_owned(),
                extra_headers: vec![],
            }
        }

        fn status(status: u16, body: &str) -> Self {
            Self {
                status,
                body: body.to_owned(),
                extra_headers: vec![],
            }
        }

        fn with_header(mut self, name: &str, value: &str) -> Self {
            self.extra_headers.push((name.to_owned(), value.to_owned()));
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
                    response.push_str(&body);

                    let _ = stream.write_all(response.as_bytes()).await;
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
