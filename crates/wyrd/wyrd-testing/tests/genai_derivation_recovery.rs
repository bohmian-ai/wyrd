//! Crash-gap/restart journey for the GenAI source boundary.
//!
//! Scribe intentionally publishes the source Parquet and `vala.file_list`
//! bookkeeping without writing an Iceberg snapshot. Forge and downstream
//! derivation own the later publication boundary, so this journey verifies
//! that repeated source recovery/flushes preserve exactly-once source rows.

mod pg_tests {
    use axum::body::Body;
    use axum::http::Request;
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

    async fn source_row_count(srv: &WyrdTestServer) -> i64 {
        let mut conn = srv
            .tenant_conn_for(srv.data_tenant_id())
            .await
            .expect("tenant connection");
        let rows: i64 = sqlx::query_scalar(
            "SELECT coalesce(sum(row_count), 0)::bigint
               FROM vala.file_list
              WHERE namespace = 'vala.traces' AND table_name = 'spans'",
        )
        .fetch_one(&mut **conn.transaction())
        .await
        .expect("source file-list query");
        conn.commit().await.expect("source transaction commits");
        rows
    }

    #[tokio::test]
    async fn genai_source_recovery_preserves_durable_rows() {
        let srv = WyrdTestServer::start_bound().await.expect("bound server");

        let jwt = match srv
            .bootstrap_user("genai-recovery-writer", &["admin"])
            .await
            .expect("bootstrap user")
        {
            Bootstrap::User { jwt, .. } => jwt,
            other => panic!("expected user bootstrap, got {other:?}"),
        };

        // Source-first: export while no downstream derivation worker is needed.
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

        srv.flush_bifrost().await.expect("first source flush");
        assert_eq!(source_row_count(&srv).await, 1);

        // A second flush is the restart/replay analogue for this boundary. It
        // must not duplicate the already-published source generation.
        srv.flush_bifrost().await.expect("replay source flush");
        assert_eq!(source_row_count(&srv).await, 1);

        srv.shutdown().await.expect("shutdown");
    }
}
