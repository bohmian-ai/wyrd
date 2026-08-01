//! OTLP source-span durability journey through the Gate-owned Scribe path.
//!
//! Scribe intentionally publishes Parquet and `vala.file_list` without an
//! Iceberg snapshot; derivation/Oracle consumption is a later task. This
//! journey therefore verifies the durable source boundary and replay row count.

mod pg_tests {
    use std::time::{Duration, Instant};

    use wyrd_testing::{Bootstrap, WyrdTestServer};
    use wyrd_tonic::otlp::common::v1::{AnyValue, KeyValue, any_value};
    use wyrd_tonic::otlp::resource::v1::Resource as OtlpResource;
    use wyrd_tonic::otlp::trace::v1::{
        ResourceSpans, ScopeSpans, Span as OtlpSpan, Status as OtlpStatus, span, status::StatusCode,
    };
    use wyrd_tonic::otlp::trace_service::ExportTraceServiceRequest;
    use wyrd_tonic::tonic::Request;
    use wyrd_tonic::tonic::transport::Channel;

    const TRACE_ID: [u8; 16] = [
        0x21, 0x22, 0x23, 0x24, 0x25, 0x26, 0x27, 0x28, 0x29, 0x2a, 0x2b, 0x2c, 0x2d, 0x2e, 0x2f,
        0x30,
    ];
    const SPAN_ID: [u8; 8] = [0xd1, 0xd2, 0xd3, 0xd4, 0xd5, 0xd6, 0xd7, 0xd8];

    // A different span ID for the replay check (same trace, different span = new
    // source batch; we reuse the same export_request shape for the model/provider
    // query).
    const SPAN_ID_2: [u8; 8] = [0xd1, 0xd2, 0xd3, 0xd4, 0xd5, 0xd6, 0xd7, 0xd9];

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
        export_request_with_span_id(SPAN_ID.to_vec())
    }

    fn export_request_with_span_id(span_id: Vec<u8>) -> ExportTraceServiceRequest {
        ExportTraceServiceRequest {
            resource_spans: vec![ResourceSpans {
                resource: Some(OtlpResource {
                    attributes: vec![kv("service.name", "genai-journey")],
                    dropped_attributes_count: 0,
                    entity_refs: Vec::new(),
                }),
                scope_spans: vec![ScopeSpans {
                    scope: None,
                    spans: vec![OtlpSpan {
                        trace_id: TRACE_ID.to_vec(),
                        span_id,
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
    async fn otlp_source_spans_are_durable_after_explicit_flush() {
        let srv = WyrdTestServer::start_bound().await.expect("bound server");
        let jwt = match srv
            .bootstrap_user("otlp-source-writer", &["admin"])
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
        srv.flush_bifrost().await.expect("source span flushes");
        let mut conn = srv
            .tenant_conn_for(srv.data_tenant_id())
            .await
            .expect("tenant connection");
        let first_rows: i64 = sqlx::query_scalar(
            "SELECT coalesce(sum(row_count), 0)::bigint
               FROM vala.file_list
              WHERE namespace = 'vala.traces' AND table_name = 'spans'",
        )
        .fetch_one(&mut **conn.transaction())
        .await
        .expect("source file-list query");
        conn.commit().await.expect("source transaction commits");
        assert_eq!(first_rows, 1);

        otlp.export(with_token(
            Request::new(export_request_with_span_id(SPAN_ID_2.to_vec())),
            &jwt,
        ))
        .await
        .expect("second OTLP export succeeds");
        srv.flush_bifrost()
            .await
            .expect("replayed source span flushes");
        let mut conn = srv
            .tenant_conn_for(srv.data_tenant_id())
            .await
            .expect("second tenant connection");
        let total_rows: i64 = sqlx::query_scalar(
            "SELECT coalesce(sum(row_count), 0)::bigint
               FROM vala.file_list
              WHERE namespace = 'vala.traces' AND table_name = 'spans'",
        )
        .fetch_one(&mut **conn.transaction())
        .await
        .expect("second source file-list query");
        conn.commit()
            .await
            .expect("second source transaction commits");
        assert_eq!(total_rows, 2);
        srv.shutdown().await.expect("shutdown");
    }
}
