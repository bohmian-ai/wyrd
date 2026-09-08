//! Edge-contract coverage for the composed Wyrd HTTP router.
//!
//! Every case drives [`WyrdTestServer`], which boots the server through
//! `wyrd_server::boot::compose_bifrost` and `build_router`, so the router,
//! middleware stack, and default-deny auth layer under assertion are the
//! production ones. The suite locks the edge behaviour that no individual
//! handler owns: request-id propagation, the 401/413/503 problem+json arms,
//! `/healthz` and `/readyz` shapes, and the guarantee that an unauthenticated
//! caller cannot distinguish a real `/v1` route from a missing one.

use std::sync::Arc;

use axum::body::to_bytes;
use axum::http::{Request, StatusCode};
use wyrd_server::components::health::{ProbeOutcome, ProbeReason, ReadinessSnapshot};
use wyrd_testing::WyrdTestServer;

/// Build an upload-init request body that is well formed enough to pass the
/// auth layer and reach its handler.
fn upload_init_body() -> axum::body::Body {
    axum::body::Body::from(
        serde_json::json!({
            "card_uid": "018f0000-0000-7000-8000-000000000001",
            "relative_path": "model.bin",
            "expected_sha256": "abc",
            "expected_size_bytes": 1
        })
        .to_string(),
    )
}

/// Collect a response body and parse it as the Wyrd problem+json envelope.
///
/// # Panics
/// Panics when the body cannot be collected or is not valid JSON.
async fn problem_json(response: axum::response::Response) -> serde_json::Value {
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body collects");
    serde_json::from_slice(&body).expect("problem JSON")
}

/// `/healthz` is a liveness probe: it must answer before auth and must not
/// acquire a request id, so a probe cannot inflate request-id telemetry.
#[tokio::test]
async fn healthz_returns_ok_without_request_id() {
    let server = WyrdTestServer::start_in_process()
        .await
        .expect("test server starts");

    let response = server
        .oneshot(
            Request::builder()
                .uri("/healthz")
                .body(axum::body::Body::empty())
                .expect("request builds"),
        )
        .await
        .expect("router responds");

    assert_eq!(response.status(), StatusCode::OK);
    assert!(!response.headers().contains_key("wyrd-request-id"));
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body collects");
    assert_eq!(&body[..], b"ok");

    server.shutdown().await.expect("server shuts down");
}

/// An unauthenticated `/v1` call must be refused as problem+json carrying the
/// stable code and a request id an agent can quote back in a support request.
#[tokio::test]
async fn unauthenticated_v1_request_returns_problem_json() {
    let server = WyrdTestServer::start_in_process()
        .await
        .expect("test server starts");

    let response = server
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/cards/upload/init")
                .body(axum::body::Body::empty())
                .expect("request builds"),
        )
        .await
        .expect("router responds");

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(
        response
            .headers()
            .get(axum::http::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok()),
        Some("application/problem+json")
    );
    assert!(response.headers().contains_key("wyrd-request-id"));

    let problem = problem_json(response).await;
    assert_eq!(problem["code"], "WYRD_AUTH_401_UNAUTHENTICATED");
    assert_eq!(problem["status"], 401);

    server.shutdown().await.expect("server shuts down");
}

/// A real minted token must clear the principal extractor. The handler may
/// still fail downstream; the contract asserted here is only that the edge
/// stack does not reject an authenticated caller.
#[tokio::test]
async fn request_with_real_jwt_passes_extractor() {
    let server = WyrdTestServer::start_in_process()
        .await
        .expect("test server starts");
    let user = server
        .bootstrap_user("router-smoke", &["admin"])
        .await
        .expect("user bootstraps");

    let response = server
        .oneshot_authenticated(
            user.jwt()
                .expect("bootstrapped user carries a minted token"),
            Request::builder()
                .method("POST")
                .uri("/v1/cards/upload/init")
                .header("content-type", "application/json")
                .body(upload_init_body())
                .expect("request builds"),
        )
        .await
        .expect("router responds");

    assert_ne!(response.status(), StatusCode::UNAUTHORIZED);

    server.shutdown().await.expect("server shuts down");
}

/// A missing credential header is an authentication failure, not a malformed
/// request: it must map to the 401 arm rather than 400.
#[tokio::test]
async fn request_with_missing_header_returns_401_unauthenticated() {
    let server = WyrdTestServer::start_in_process()
        .await
        .expect("test server starts");

    let response = server
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/cards/upload/init")
                .body(axum::body::Body::empty())
                .expect("request builds"),
        )
        .await
        .expect("router responds");

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    let problem = problem_json(response).await;
    assert_eq!(problem["code"], "WYRD_AUTH_401_UNAUTHENTICATED");

    server.shutdown().await.expect("server shuts down");
}

/// Guards two invariants: `/auth/token` is not behind the default-deny layer
/// (otherwise no client could ever obtain a first token), and auth routes still
/// carry `wyrd-request-id` from `attach_request_id`.
#[tokio::test]
async fn auth_routes_are_reachable_without_credential_and_carry_request_id() {
    let server = WyrdTestServer::start_in_process()
        .await
        .expect("test server starts");

    let response = server
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/auth/token")
                .header("content-type", "application/json")
                .body(axum::body::Body::from("{}"))
                .expect("request builds"),
        )
        .await
        .expect("router responds");

    assert_ne!(
        response.status(),
        StatusCode::UNAUTHORIZED,
        "/auth/token must be reachable without a credential (public route)"
    );
    assert!(
        response.headers().contains_key("wyrd-request-id"),
        "auth routes sit in the protected router and must carry wyrd-request-id"
    );

    server.shutdown().await.expect("server shuts down");
}

/// When the composed state has no token verifier, every `/v1` request must
/// answer `503 WYRD_AUTH_503_VERIFY_UNAVAILABLE` — not 401 and not 500. This
/// locks the third arm of the 400/401/503 auth-error contract and the
/// `retry_after_seconds` hint agents use for backoff.
#[tokio::test]
async fn v1_request_without_verifier_configured_returns_503() {
    let issuer = WyrdTestServer::start_in_process()
        .await
        .expect("token-issuing server starts");
    let user = issuer
        .bootstrap_user("verify-unavailable", &["admin"])
        .await
        .expect("user bootstraps");
    let token = user
        .jwt()
        .expect("bootstrapped user carries a minted token")
        .to_owned();
    issuer.shutdown().await.expect("issuer shuts down");

    let server = WyrdTestServer::builder()
        .without_token_verifier_for_test()
        .start_in_process()
        .await
        .expect("verifier-less server starts");

    let response = server
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/cards/upload/init")
                .header("x-wyrd-access-token", format!("Bearer {token}"))
                .body(axum::body::Body::empty())
                .expect("request builds"),
        )
        .await
        .expect("router responds");

    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    let problem = problem_json(response).await;
    assert_eq!(problem["code"], "WYRD_AUTH_503_VERIFY_UNAVAILABLE");
    assert_eq!(problem["status"], 503);

    server.shutdown().await.expect("server shuts down");
}

/// `/healthz` must never answer as problem+json; Kubernetes treats the body as
/// opaque and a problem envelope here would signal a routing regression.
#[tokio::test]
async fn healthz_returns_ok_without_problem_json() {
    let server = WyrdTestServer::start_in_process()
        .await
        .expect("test server starts");

    let response = server
        .oneshot(
            Request::builder()
                .uri("/healthz")
                .body(axum::body::Body::empty())
                .expect("request builds"),
        )
        .await
        .expect("router responds");

    assert_eq!(response.status(), StatusCode::OK);
    let content_type = response
        .headers()
        .get(axum::http::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default();
    assert!(
        !content_type.contains("problem"),
        "/healthz must not return problem+json"
    );

    server.shutdown().await.expect("server shuts down");
}

/// On cold boot the readiness snapshot is all-warmup, so `/readyz` must refuse
/// with 503 problem+json rather than reporting a server that cannot yet serve.
#[tokio::test]
async fn readyz_returns_json_even_on_cold_boot() {
    let server = WyrdTestServer::start_in_process()
        .await
        .expect("test server starts");

    let response = server
        .oneshot(
            Request::builder()
                .uri("/readyz")
                .body(axum::body::Body::empty())
                .expect("request builds"),
        )
        .await
        .expect("router responds");

    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(
        response
            .headers()
            .get(axum::http::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok()),
        Some("application/problem+json")
    );
    let problem = problem_json(response).await;
    assert_eq!(problem["code"], "WYRD_SERVER_503_NOT_READY");
    assert_eq!(problem["status"], 503);

    server.shutdown().await.expect("server shuts down");
}

/// Publish an all-ok snapshot into the composed state's live readiness cell and
/// assert `/readyz` flips to 200. Guards the success path so a wrong status code
/// or serialization regression cannot silently block Kubernetes readiness.
#[tokio::test]
async fn readyz_returns_ok_when_all_probes_pass() {
    let server = WyrdTestServer::start_in_process()
        .await
        .expect("test server starts");

    let ok_probe = ProbeOutcome {
        ok: true,
        reason: ProbeReason::Ok,
        elapsed_ms: 1,
    };
    server.state().readiness.store(Arc::new(ReadinessSnapshot {
        postgres: ok_probe.clone(),
        storage: ok_probe.clone(),
        scribe: ok_probe.clone(),
        oracle: ok_probe.clone(),
        peer: ok_probe,
    }));

    let response = server
        .oneshot(
            Request::builder()
                .uri("/readyz")
                .body(axum::body::Body::empty())
                .expect("request builds"),
        )
        .await
        .expect("router responds");

    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body collects");
    let json: serde_json::Value = serde_json::from_slice(&body).expect("JSON");
    assert_eq!(json["status"], "ok");

    server.shutdown().await.expect("server shuts down");
}

/// A body past the configured ceiling must be refused as 413 problem+json by
/// the edge layer, before any handler or credential check consumes it.
#[tokio::test]
async fn oversized_body_returns_413_problem_json() {
    let server = WyrdTestServer::builder()
        .with_limits_for_test(wyrd_server::state::LimitsConfig {
            body_bytes: 10,
            timeout: std::time::Duration::from_secs(30),
            concurrency: 1024,
        })
        .start_in_process()
        .await
        .expect("test server starts");

    let response = server
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/cards/upload/init")
                .header("content-type", "application/json")
                .header("x-wyrd-access-token", "Bearer invalid.token.here")
                .body(axum::body::Body::from("a".repeat(100)))
                .expect("request builds"),
        )
        .await
        .expect("router responds");

    assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
    assert_eq!(
        response
            .headers()
            .get(axum::http::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok()),
        Some("application/problem+json")
    );
    let problem = problem_json(response).await;
    assert_eq!(problem["code"], "WYRD_SPEC_413_PAYLOAD_TOO_LARGE");

    server.shutdown().await.expect("server shuts down");
}

/// The default-deny layer wraps the `/v1` fallback, so an unknown `/v1` path
/// with no token is rejected with 401 before `v1_not_found` runs — not 404.
/// This is the "no route-existence oracle" guarantee: an unauthenticated caller
/// cannot distinguish a real route from a missing one.
#[tokio::test]
async fn unknown_v1_path_without_token_returns_401_via_layer() {
    let server = WyrdTestServer::start_in_process()
        .await
        .expect("test server starts");

    let response = server
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/v1/this/route/does/not/exist")
                .body(axum::body::Body::empty())
                .expect("request builds"),
        )
        .await
        .expect("router responds");

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    server.shutdown().await.expect("server shuts down");
}

/// Sweep one real route from each mounted group (storage, authz, admin). With
/// no token every one must be rejected by the layer with 401, proving auth is a
/// property of the whole `/v1` nest rather than of any single handler.
#[tokio::test]
async fn representative_v1_routes_without_token_all_return_401() {
    let server = WyrdTestServer::start_in_process()
        .await
        .expect("test server starts");

    let routes = [
        ("POST", "/v1/cards/upload/init"),
        ("POST", "/v1/cards/download/init"),
        ("POST", "/v1/authz/check"),
        (
            "POST",
            "/v1/principals/018f0000-0000-7000-8000-000000000001/revoke",
        ),
        ("GET", "/v1/admin/trusted-issuers"),
        ("GET", "/v1/admin/workload-bindings"),
    ];

    for (method, path) in routes {
        let response = server
            .oneshot(
                Request::builder()
                    .method(method)
                    .uri(path)
                    .body(axum::body::Body::empty())
                    .expect("request builds"),
            )
            .await
            .expect("router responds");

        assert_eq!(
            response.status(),
            StatusCode::UNAUTHORIZED,
            "{method} {path} must be rejected with 401 by the default-deny layer"
        );
    }

    server.shutdown().await.expect("server shuts down");
}

/// A minted valid token on a real `/v1` route must pass the default-deny layer.
/// The handler may still fail downstream; the layer itself must not answer 401.
#[tokio::test]
async fn valid_token_is_not_rejected_by_default_deny_layer() {
    let server = WyrdTestServer::start_in_process()
        .await
        .expect("test server starts");
    let user = server
        .bootstrap_user("default-deny", &["admin"])
        .await
        .expect("user bootstraps");

    let response = server
        .oneshot_authenticated(
            user.jwt()
                .expect("bootstrapped user carries a minted token"),
            Request::builder()
                .method("POST")
                .uri("/v1/cards/upload/init")
                .header("content-type", "application/json")
                .body(upload_init_body())
                .expect("request builds"),
        )
        .await
        .expect("router responds");

    assert_ne!(response.status(), StatusCode::UNAUTHORIZED);

    server.shutdown().await.expect("server shuts down");
}

/// A composed, API-serving target refuses the production profile while it still
/// carries the stub policy hook.
///
/// `AppState::production_validate` short-circuits for a target that serves no
/// public API, so the guard can only be observed against a state composed
/// through `wyrd_server::boot::compose_bifrost` — which is exactly what the
/// harness builds.
#[tokio::test]
async fn production_profile_refuses_a_serving_target_with_stub_defaults() {
    let server = WyrdTestServer::start_in_process()
        .await
        .expect("test server starts");
    let state = server
        .state()
        .clone()
        .with_deployment_profile(wyrd_server::config::DeploymentProfile::Production);

    let refusal = state
        .production_validate()
        .expect_err("a serving production target must refuse its stub defaults");

    assert!(
        matches!(
            refusal,
            wyrd_server::state::ProductionValidationError::StubPolicyHook
        ),
        "the stub policy hook is the first production guard to refuse; got {refusal:?}"
    );

    server.shutdown().await.expect("server shuts down");
}

// ─── canonical trace and GenAI reads ──────────────────────────────────────────

/// Trace id every fixture span in the canonical read case shares.
const CANONICAL_TRACE_ID: [u8; 16] = [
    0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, 0x99, 0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff, 0x01,
];

/// Fixed start timestamp of the generation span, in protocol nanoseconds.
const GENERATION_START_NANOS: u64 = 1_800_000_000_123_456_789;

/// Fixed end timestamp of the generation span, in protocol nanoseconds.
const GENERATION_END_NANOS: u64 = 1_800_000_000_987_654_321;

/// Name of the role granting bounded query read without any payload permission.
const METADATA_ONLY_ROLE: &str = "bifrost_metadata_reader";

/// Build one OTLP string attribute.
fn otlp_str(key: &str, value: &str) -> wyrd_tonic::otlp::common::v1::KeyValue {
    use wyrd_tonic::otlp::common::v1::{AnyValue, KeyValue, any_value};
    KeyValue {
        key: key.to_owned(),
        value: Some(AnyValue {
            value: Some(any_value::Value::StringValue(value.to_owned())),
        }),
    }
}

/// Build one OTLP signed-integer attribute.
fn otlp_int(key: &str, value: i64) -> wyrd_tonic::otlp::common::v1::KeyValue {
    use wyrd_tonic::otlp::common::v1::{AnyValue, KeyValue, any_value};
    KeyValue {
        key: key.to_owned(),
        value: Some(AnyValue {
            value: Some(any_value::Value::IntValue(value)),
        }),
    }
}

/// Build one structured GenAI message list attribute.
///
/// Each element is a keyvalue list carrying a `role` and a `content` string, so
/// the decoded public value is an array of objects rather than a stringified
/// blob.
fn otlp_messages(key: &str, turns: &[(&str, &str)]) -> wyrd_tonic::otlp::common::v1::KeyValue {
    use wyrd_tonic::otlp::common::v1::{AnyValue, ArrayValue, KeyValue, KeyValueList, any_value};
    KeyValue {
        key: key.to_owned(),
        value: Some(AnyValue {
            value: Some(any_value::Value::ArrayValue(ArrayValue {
                values: turns
                    .iter()
                    .map(|(role, content)| AnyValue {
                        value: Some(any_value::Value::KvlistValue(KeyValueList {
                            values: vec![otlp_str("role", role), otlp_str("content", content)],
                        })),
                    })
                    .collect(),
            })),
        }),
    }
}

/// Build the OTLP export payload the canonical read case ingests.
///
/// One `chat` generation span carries promoted GenAI metadata, structured
/// input/output messages, one event and one link. Three sibling spans use
/// operation names outside the approved generation set, so they belong to the
/// same trace but must never appear in a GenAI generation search.
fn canonical_trace_export() -> wyrd_tonic::otlp::trace_service::ExportTraceServiceRequest {
    use wyrd_tonic::otlp::resource::v1::Resource;
    use wyrd_tonic::otlp::trace::v1::{ResourceSpans, ScopeSpans, Span, Status, span, status};
    use wyrd_tonic::otlp::trace_service::ExportTraceServiceRequest;

    let generation = Span {
        trace_id: CANONICAL_TRACE_ID.to_vec(),
        span_id: 1_u64.to_be_bytes().to_vec(),
        name: "chat gpt-canonical".to_owned(),
        kind: span::SpanKind::Client.into(),
        start_time_unix_nano: GENERATION_START_NANOS,
        end_time_unix_nano: GENERATION_END_NANOS,
        status: Some(Status {
            code: status::StatusCode::Ok.into(),
            message: "generation completed".to_owned(),
        }),
        attributes: vec![
            otlp_str("gen_ai.operation.name", "chat"),
            otlp_str("gen_ai.provider.name", "openai"),
            otlp_str("gen_ai.request.model", "gpt-canonical"),
            otlp_str("gen_ai.conversation.id", "conversation-canonical"),
            otlp_int("gen_ai.usage.input_tokens", 41),
            otlp_int("gen_ai.usage.output_tokens", 17),
            otlp_messages("gen_ai.input.messages", &[("user", "what is wyrd?")]),
            otlp_messages("gen_ai.output.messages", &[("assistant", "an AI layer")]),
        ],
        events: vec![span::Event {
            time_unix_nano: GENERATION_START_NANOS.saturating_add(5),
            name: "first_token".to_owned(),
            attributes: vec![otlp_int("token.index", 0)],
            dropped_attributes_count: 0,
        }],
        links: vec![span::Link {
            trace_id: CANONICAL_TRACE_ID.to_vec(),
            span_id: 2_u64.to_be_bytes().to_vec(),
            attributes: vec![otlp_str("link.kind", "follows_from")],
            ..span::Link::default()
        }],
        ..Span::default()
    };

    let excluded = ["embeddings", "invoke_agent", "execute_tool"]
        .into_iter()
        .enumerate()
        .map(|(index, operation)| Span {
            trace_id: CANONICAL_TRACE_ID.to_vec(),
            span_id: (index as u64 + 2).to_be_bytes().to_vec(),
            name: operation.to_owned(),
            kind: span::SpanKind::Internal.into(),
            start_time_unix_nano: GENERATION_START_NANOS,
            end_time_unix_nano: GENERATION_END_NANOS,
            attributes: vec![
                otlp_str("gen_ai.operation.name", operation),
                otlp_str("gen_ai.request.model", "gpt-canonical"),
            ],
            ..Span::default()
        });

    ExportTraceServiceRequest {
        resource_spans: vec![ResourceSpans {
            resource: Some(Resource {
                attributes: vec![otlp_str("service.name", "canonical-read-case")],
                ..Resource::default()
            }),
            scope_spans: vec![ScopeSpans {
                spans: std::iter::once(generation).chain(excluded).collect(),
                ..ScopeSpans::default()
            }],
            ..ResourceSpans::default()
        }],
    }
}

/// Build one caller holding exactly the supplied permissions in `tenant`.
fn caller_with(
    tenant: wyrd_spec::DataTenantId,
    permissions: impl IntoIterator<Item = wyrd_runtime::Permission>,
) -> wyrd_server::components::auth::Caller {
    use wyrd_runtime::Principal;
    use wyrd_runtime::permission::PermissionSet;
    use wyrd_runtime::principal::{PrincipalId, PrincipalKind};
    use wyrd_spec::request_id::RequestId;

    wyrd_server::components::auth::Caller {
        data_tenant_id: tenant,
        principal: Principal::new(
            PrincipalId::new(uuid::Uuid::now_v7()),
            PrincipalKind::User,
            tenant,
            Vec::new(),
            PermissionSet::from_iter(permissions),
        ),
        request_id: RequestId::now_v7(),
        delegation_chain: Vec::new(),
    }
}

/// Collect the projected output column names of one built plan.
fn plan_columns(plan: &datafusion::logical_expr::LogicalPlan) -> Vec<String> {
    plan.schema()
        .fields()
        .iter()
        .map(|field| field.name().clone())
        .collect()
}

/// Read a JSON response body, requiring the supplied status.
async fn json_body(response: axum::response::Response, expected: StatusCode) -> serde_json::Value {
    let status = response.status();
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body collects");
    assert_eq!(
        status,
        expected,
        "unexpected status; body was {}",
        String::from_utf8_lossy(&body)
    );
    serde_json::from_slice(&body).expect("JSON body")
}

/// Canonical trace and GenAI reads must classify generations through promoted
/// columns and gate every payload column before any storage read.
///
/// The case ingests one real OTLP export through the production `/v1/traces`
/// route, then compares the plans and the served responses of a payload-bearing
/// caller against a caller holding only `bifrost_query:read`. The unauthorized
/// plan must not project `attributes`, `events`, `links`, or the resource/scope
/// payload at all — the gate is a projection decision taken before IO, not a
/// redaction applied after reading.
#[tokio::test]
async fn canonical_trace_and_genai_queries_filter_promotions_before_payload_projection() {
    use wyrd_runtime::{Permission, Resource};
    use wyrd_server::vala_query::service::{build_get_trace_plan, build_query_genai_plan};
    use wyrd_spec::vala::api::{GetTraceRequest, QueryGenAiRequest, QueryWindow};
    use wyrd_tonic::prost::Message;

    let server = WyrdTestServer::start_in_process()
        .await
        .expect("test server starts");
    let tenant = server.data_tenant_id();
    let admin = server
        .bootstrap_user("canonical-reads-admin", &["admin"])
        .await
        .expect("admin bootstraps");

    // A metadata-only reader is not a builtin role: query read without any
    // payload permission is exactly the identity this case has to contrast.
    let mut conn = server
        .tenant_conn_for(tenant)
        .await
        .expect("tenant connection");
    sqlx::query(
        "INSERT INTO wyrd.auth_roles (id, data_tenant_id, name, permissions, builtin) \
         VALUES ($1, $2, $3, $4, FALSE)",
    )
    .bind(uuid::Uuid::now_v7())
    .bind(tenant.as_uuid())
    .bind(METADATA_ONLY_ROLE)
    .bind(serde_json::json!([{ "resource": "bifrost_query", "action": "read" }]))
    .execute(&mut **conn.transaction())
    .await
    .expect("metadata-only role inserts");
    conn.commit().await.expect("role commits");

    let metadata_only = server
        .bootstrap_user("canonical-reads-metadata", &[METADATA_ONLY_ROLE])
        .await
        .expect("metadata-only user bootstraps");

    let export = canonical_trace_export().encode_to_vec();
    let ingest = server
        .oneshot_authenticated(
            admin.jwt().expect("admin carries a token"),
            Request::builder()
                .method("POST")
                .uri("/v1/traces")
                .header("content-type", "application/x-protobuf")
                .body(axum::body::Body::from(export))
                .expect("request builds"),
        )
        .await
        .expect("router responds");
    assert_eq!(ingest.status(), StatusCode::OK, "OTLP export is accepted");
    server.flush_bifrost().await.expect("spans become readable");

    let trace_hex = CANONICAL_TRACE_ID
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();

    // ── plan shape: the payload gate is a projection taken before IO ──
    let payload_caller = caller_with(
        tenant,
        [
            Permission::bifrost_query_read(),
            Permission {
                resource: Resource::BifrostTracePayload,
                action: wyrd_runtime::Action::Read,
            },
            Permission {
                resource: Resource::BifrostGenAiPayload,
                action: wyrd_runtime::Action::Read,
            },
        ],
    );
    let bounded_caller = caller_with(tenant, [Permission::bifrost_query_read()]);
    let trace_request = GetTraceRequest {
        trace_id: trace_hex.clone(),
        since: None,
        until: None,
    };

    let authorized_trace_plan =
        build_get_trace_plan(server.state(), &payload_caller, &trace_request)
            .await
            .expect("authorized trace plan builds");
    let bounded_trace_plan = build_get_trace_plan(server.state(), &bounded_caller, &trace_request)
        .await
        .expect("bounded trace plan builds");
    let authorized_columns = plan_columns(&authorized_trace_plan);
    let bounded_columns = plan_columns(&bounded_trace_plan);
    for gated in [
        "attributes",
        "events",
        "links",
        "resource_attributes",
        "scope_attributes",
    ] {
        assert!(
            authorized_columns.iter().any(|name| name == gated),
            "authorized trace plan must project {gated}"
        );
        assert!(
            !bounded_columns.iter().any(|name| name == gated),
            "bounded trace plan must never project {gated}"
        );
    }
    for unprojected in ["resource_entity_refs"] {
        assert!(
            !authorized_columns.iter().any(|name| name == unprojected),
            "no trace plan returns {unprojected}"
        );
    }
    assert!(
        bounded_columns.iter().any(|name| name == "span_id"),
        "bounded trace plan still returns span metadata"
    );

    let genai_request = QueryGenAiRequest {
        window: QueryWindow::default(),
        conversation_id: None,
        model: None,
        provider: None,
    };
    let authorized_genai_plan =
        build_query_genai_plan(server.state(), &payload_caller, &genai_request)
            .await
            .expect("authorized GenAI plan builds");
    let bounded_genai_plan =
        build_query_genai_plan(server.state(), &bounded_caller, &genai_request)
            .await
            .expect("bounded GenAI plan builds");
    assert_eq!(
        plan_columns(&bounded_genai_plan),
        vec![
            "start_time_unix_nano",
            "gen_ai_conversation_id",
            "gen_ai_request_model",
            "gen_ai_provider_name",
            "gen_ai_usage_input_tokens",
            "gen_ai_usage_output_tokens",
        ],
        "a bounded GenAI search projects promotions only"
    );
    assert_eq!(
        plan_columns(&authorized_genai_plan)
            .last()
            .map(String::as_str),
        Some("attributes"),
        "the canonical message payload is added only for an authorized caller"
    );
    let plan_text = format!("{}", authorized_genai_plan.display_indent());
    for operation in ["chat", "generate_content", "text_completion"] {
        assert!(
            plan_text.contains(operation),
            "generation membership must bind {operation} in the plan"
        );
    }
    for excluded in ["embeddings", "invoke_agent", "execute_tool"] {
        assert!(
            !plan_text.contains(excluded),
            "{excluded} is not an approved generation operation"
        );
    }
    assert!(
        plan_text.contains("gen_ai_operation_name"),
        "classification binds the promoted column, never the attributes payload"
    );

    // ── served responses ──
    let authorized_trace = json_body(
        server
            .oneshot_authenticated(
                admin.jwt().expect("admin carries a token"),
                Request::builder()
                    .method("GET")
                    .uri(format!("/v1/traces/{trace_hex}"))
                    .body(axum::body::Body::empty())
                    .expect("request builds"),
            )
            .await
            .expect("router responds"),
        StatusCode::OK,
    )
    .await;
    let spans = authorized_trace["trace"]["spans"]
        .as_array()
        .expect("spans array");
    assert_eq!(spans.len(), 4, "the complete trace cut returns every span");
    let generation = spans
        .iter()
        .find(|span| span["name"] == "chat gpt-canonical")
        .expect("generation span is served");
    assert_eq!(generation["start_time_unix_nano"], GENERATION_START_NANOS);
    assert_eq!(generation["end_time_unix_nano"], GENERATION_END_NANOS);
    assert_eq!(
        generation["duration_nano"],
        GENERATION_END_NANOS - GENERATION_START_NANOS
    );
    assert_eq!(generation["service_name"], "canonical-read-case");
    assert_eq!(
        generation["attributes"]["gen_ai.request.model"],
        "gpt-canonical"
    );
    assert_eq!(generation["events"][0]["name"], "first_token");
    assert_eq!(generation["events"][0]["attributes"]["token.index"], 0);
    assert_eq!(generation["links"][0]["linked_trace_id"], trace_hex);
    assert_eq!(
        generation["links"][0]["attributes"]["link.kind"],
        "follows_from"
    );

    let bounded_trace = json_body(
        server
            .oneshot_authenticated(
                metadata_only.jwt().expect("reader carries a token"),
                Request::builder()
                    .method("GET")
                    .uri(format!("/v1/traces/{trace_hex}"))
                    .body(axum::body::Body::empty())
                    .expect("request builds"),
            )
            .await
            .expect("router responds"),
        StatusCode::OK,
    )
    .await;
    let bounded_spans = bounded_trace["trace"]["spans"]
        .as_array()
        .expect("spans array");
    assert_eq!(bounded_spans.len(), 4, "metadata stays fully visible");
    for span in bounded_spans {
        assert!(span["name"].is_string(), "span metadata is served");
        for gated in ["attributes", "events", "links", "resource_attributes"] {
            assert!(
                span.get(gated).is_none(),
                "{gated} must be omitted without BifrostTracePayload:Read"
            );
        }
    }

    let authorized_genai = json_body(
        server
            .oneshot_authenticated(
                admin.jwt().expect("admin carries a token"),
                Request::builder()
                    .method("POST")
                    .uri("/v1/genai/query")
                    .header("content-type", "application/json")
                    .body(axum::body::Body::from("{}"))
                    .expect("request builds"),
            )
            .await
            .expect("router responds"),
        StatusCode::OK,
    )
    .await;
    let rows = authorized_genai["rows"].as_array().expect("rows array");
    assert_eq!(
        rows.len(),
        1,
        "only the approved generation operations are generations"
    );
    assert_eq!(rows[0]["model"], "gpt-canonical");
    assert_eq!(rows[0]["provider"], "openai");
    assert_eq!(rows[0]["conversation_id"], "conversation-canonical");
    assert_eq!(rows[0]["input_tokens"], 41);
    assert_eq!(rows[0]["output_tokens"], 17);
    assert_eq!(rows[0]["start_time_unix_nano"], GENERATION_START_NANOS);
    assert_eq!(rows[0]["input_messages"][0]["role"], "user");
    assert_eq!(rows[0]["input_messages"][0]["content"], "what is wyrd?");
    assert_eq!(rows[0]["output_messages"][0]["role"], "assistant");
    assert_eq!(rows[0]["output_messages"][0]["content"], "an AI layer");

    let bounded_genai = json_body(
        server
            .oneshot_authenticated(
                metadata_only.jwt().expect("reader carries a token"),
                Request::builder()
                    .method("POST")
                    .uri("/v1/genai/query")
                    .header("content-type", "application/json")
                    .body(axum::body::Body::from("{}"))
                    .expect("request builds"),
            )
            .await
            .expect("router responds"),
        StatusCode::OK,
    )
    .await;
    let bounded_rows = bounded_genai["rows"].as_array().expect("rows array");
    assert_eq!(bounded_rows.len(), 1, "promotions stay searchable");
    assert_eq!(bounded_rows[0]["model"], "gpt-canonical");
    for gated in ["input_messages", "output_messages"] {
        assert!(
            bounded_rows[0].get(gated).is_none(),
            "{gated} must be omitted without BifrostGenAiPayload:Read"
        );
    }

    server.shutdown().await.expect("server shuts down");
}
