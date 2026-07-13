//! Crash-gap/restart journey for GenAI derivation.
//!
//! The source commit is made while no derivation worker is running, so its
//! `NOTIFY` is missed. A fresh worker then recovers the source through its
//! fallback tick, which runs in virtual time here so the journey stays fast.

mod pg_tests {
    use std::time::Duration;

    use axum::body::{Body, to_bytes};
    use axum::http::Request;
    use tokio_util::sync::CancellationToken;
    use wyrd_spec::vala::api::QueryGenAiRequest;
    use wyrd_testing::{Bootstrap, WyrdTestServer};
    use wyrd_tonic::otlp::common::v1::{AnyValue, KeyValue, any_value};
    use wyrd_tonic::otlp::resource::v1::Resource as OtlpResource;
    use wyrd_tonic::otlp::trace::v1::{
        ResourceSpans, ScopeSpans, Span as OtlpSpan, Status as OtlpStatus, span, status::StatusCode,
    };
    use wyrd_tonic::otlp::trace_service::ExportTraceServiceRequest;
    use wyrd_tonic::prost::Message;

    const TRACE_ID: [u8; 16] = [
        0x31, 0x32, 0x33, 0x34, 0x35, 0x36, 0x37, 0x38, 0x39, 0x3a, 0x3b, 0x3c, 0x3d, 0x3e, 0x3f,
        0x40,
    ];
    const SPAN_ID: [u8; 8] = [0xe1, 0xe2, 0xe3, 0xe4, 0xe5, 0xe6, 0xe7, 0xe8];

    fn kv(key: &str, value: &str) -> KeyValue {
        KeyValue {
            key: key.to_owned(),
            value: Some(AnyValue {
                value: Some(any_value::Value::StringValue(value.to_owned())),
            }),
        }
    }

    fn export_request() -> ExportTraceServiceRequest {
        ExportTraceServiceRequest {
            resource_spans: vec![ResourceSpans {
                resource: Some(OtlpResource {
                    attributes: vec![kv("service.name", "genai-recovery-journey")],
                    dropped_attributes_count: 0,
                }),
                scope_spans: vec![ScopeSpans {
                    scope: None,
                    spans: vec![OtlpSpan {
                        trace_id: TRACE_ID.to_vec(),
                        span_id: SPAN_ID.to_vec(),
                        parent_span_id: vec![],
                        trace_state: String::new(),
                        flags: 0,
                        name: "recovery chat".to_owned(),
                        kind: span::SpanKind::Internal as i32,
                        start_time_unix_nano: 1_700_000_000_000_000_000,
                        end_time_unix_nano: 1_700_000_000_050_000_000,
                        attributes: vec![
                            kv("gen_ai.provider.name", "openai"),
                            kv("gen_ai.operation.name", "chat"),
                            kv("gen_ai.request.model", "recovery-model"),
                        ],
                        dropped_attributes_count: 0,
                        events: vec![],
                        dropped_events_count: 0,
                        links: vec![],
                        dropped_links_count: 0,
                        status: Some(OtlpStatus {
                            message: String::new(),
                            code: StatusCode::Ok as i32,
                        }),
                    }],
                    schema_url: String::new(),
                }],
                schema_url: String::new(),
            }],
        }
    }

    #[tokio::test]
    async fn genai_derivation_recovers_missed_notify_after_restart() {
        let srv = WyrdTestServer::start_in_process()
            .await
            .expect("in-process server");

        let jwt = match srv
            .bootstrap_user("genai-recovery-writer", &["admin"])
            .await
            .expect("bootstrap user")
        {
            Bootstrap::User { jwt, .. } => jwt,
            other => panic!("expected user bootstrap, got {other:?}"),
        };

        // No worker is started by start_in_process. The export commits the
        // source span and publishes NOTIFY while no listener exists.
        let export = Request::builder()
            .method("POST")
            .uri("/v1/traces")
            .header("content-type", "application/x-protobuf")
            .body(Body::from(export_request().encode_to_vec()))
            .expect("export request");
        let response = srv
            .oneshot_authenticated(&jwt, export)
            .await
            .expect("OTLP export request");
        assert_eq!(response.status(), 200, "source commit must succeed");

        tokio::time::pause();
        let shutdown = CancellationToken::new();
        let worker = vala_bifrost::serving::derivations::spawn_genai_derivation_worker(
            srv.state().bifrost.clone(),
            srv.app_pool(),
            srv.state()
                .postgres
                .operator_pool()
                .expect("operator pool for derivation worker"),
            shutdown.clone(),
        );

        // Let the restarted worker finish table/listener setup, then elapse a
        // full fallback cadence. The missed NOTIFY cannot trigger this path.
        for _ in 0..=61 {
            tokio::time::advance(Duration::from_secs(1)).await;
            tokio::task::yield_now().await;
        }
        tokio::time::resume();

        let query = Request::builder()
            .method("POST")
            .uri("/v1/genai/query")
            .header("content-type", "application/json")
            .body(Body::from(
                serde_json::to_vec(&QueryGenAiRequest {
                    window: wyrd_spec::vala::api::QueryWindow {
                        since: None,
                        until: None,
                        limit: Some(100),
                        page_token: None,
                    },
                    conversation_id: None,
                    model: Some("recovery-model".to_owned()),
                    provider: Some("openai".to_owned()),
                })
                .expect("query request serializes"),
            ))
            .expect("query request");
        let response = srv
            .oneshot_authenticated(&jwt, query)
            .await
            .expect("genai query request");
        let status = response.status();
        let body = to_bytes(response.into_body(), 1024 * 1024)
            .await
            .expect("genai query body");
        assert_eq!(status, 200, "genai query must succeed: {body:?}");
        let value: serde_json::Value =
            serde_json::from_slice(&body).expect("genai query response JSON");
        assert!(
            value["rows"]
                .as_array()
                .is_some_and(|rows| rows.iter().any(|row| row["model"] == "recovery-model")),
            "fallback recovery must materialize the missed GenAI source commit: {value}"
        );

        shutdown.cancel();
        worker.await.expect("derivation worker shutdown");
        srv.shutdown().await.expect("shutdown");
    }
}
