//! GenAI derivation journey: a committed OTLP span wakes the derivation worker
//! through PostgreSQL `NOTIFY`, before the 60-second recovery tick is due.

mod pg_tests {
    use std::time::{Duration, Instant};

    use tokio_util::sync::CancellationToken;
    use wyrd_testing::{Bootstrap, WyrdTestServer};
    use wyrd_tonic::otlp::common::v1::{AnyValue, KeyValue, any_value};
    use wyrd_tonic::otlp::resource::v1::Resource as OtlpResource;
    use wyrd_tonic::otlp::trace::v1::{
        ResourceSpans, ScopeSpans, Span as OtlpSpan, Status as OtlpStatus, span, status::StatusCode,
    };
    use wyrd_tonic::otlp::trace_service::ExportTraceServiceRequest;
    use wyrd_tonic::tonic::Request;
    use wyrd_tonic::tonic::transport::Channel;
    use wyrd_tonic::wyrd::v1::vala_query_service_client::ValaQueryServiceClient;
    use wyrd_tonic::wyrd::v1::{QueryGenAiRequest, QueryWindow};

    const TRACE_ID: [u8; 16] = [
        0x21, 0x22, 0x23, 0x24, 0x25, 0x26, 0x27, 0x28, 0x29, 0x2a, 0x2b, 0x2c, 0x2d, 0x2e, 0x2f,
        0x30,
    ];
    const SPAN_ID: [u8; 8] = [0xd1, 0xd2, 0xd3, 0xd4, 0xd5, 0xd6, 0xd7, 0xd8];

    async fn connect(url: &str) -> Channel {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            match Channel::from_shared(url.to_owned())
                .expect("endpoint parses")
                .connect()
                .await
            {
                Ok(channel) => return channel,
                Err(error) => {
                    assert!(
                        Instant::now() < deadline,
                        "gRPC channel never connected within 5s: {error}"
                    );
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
            }
        }
    }

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
                    attributes: vec![kv("service.name", "genai-journey")],
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
                        name: "chat completion".to_owned(),
                        kind: span::SpanKind::Internal as i32,
                        start_time_unix_nano: 1_700_000_000_000_000_000,
                        end_time_unix_nano: 1_700_000_000_050_000_000,
                        attributes: vec![
                            kv("gen_ai.provider.name", "openai"),
                            kv("gen_ai.operation.name", "chat"),
                            kv("gen_ai.request.model", "journey-model"),
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

    fn with_token<T>(mut request: Request<T>, jwt: &str) -> Request<T> {
        request.metadata_mut().insert(
            "x-wyrd-access-token",
            format!("Bearer {jwt}").parse().expect("metadata value"),
        );
        request
    }

    #[tokio::test]
    async fn genai_derivation_wakes_on_notify_before_fallback() {
        let srv = WyrdTestServer::start_bound().await.expect("bound server");
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

        let jwt = match srv
            .bootstrap_user("genai-notify-writer", &["admin"])
            .await
            .expect("bootstrap user")
        {
            Bootstrap::User { jwt, .. } => jwt,
            other => panic!("expected user bootstrap, got {other:?}"),
        };
        let channel = connect(&srv.grpc_url().expect("grpc url")).await;
        let mut otlp =
            wyrd_tonic::otlp::trace_service::trace_service_client::TraceServiceClient::new(
                channel.clone(),
            );
        otlp.export(with_token(Request::new(export_request()), &jwt))
            .await
            .expect("OTLP export succeeds");

        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            let mut query = ValaQueryServiceClient::new(channel.clone());
            let rows = query
                .query_gen_ai(with_token(
                    Request::new(QueryGenAiRequest {
                        window: Some(QueryWindow {
                            since: String::new(),
                            until: String::new(),
                            limit: 100,
                            page_token: String::new(),
                        }),
                        conversation_id: String::new(),
                        model: "journey-model".to_owned(),
                        provider: "openai".to_owned(),
                    }),
                    &jwt,
                ))
                .await
                .expect("genai query succeeds")
                .into_inner()
                .rows;
            if rows.iter().any(|row| row.model == "journey-model") {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "genai derivation did not wake from NOTIFY before fallback: {rows:?}"
            );
            tokio::time::sleep(Duration::from_millis(100)).await;
        }

        shutdown.cancel();
        worker.await.expect("derivation worker shutdown");
        srv.shutdown().await.expect("shutdown");
    }
}
