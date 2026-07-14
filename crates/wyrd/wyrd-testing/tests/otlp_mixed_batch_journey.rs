//! OTLP mixed valid/invalid batch journey.
//!
//! One export request carrying two spans — one valid, one structurally
//! invalid (15-byte trace_id) — must:
//!
//! * Succeed at the gRPC / HTTP level (no server error).
//! * Return `partial_success` with `rejected_spans == 1` and a non-empty
//!   `error_message` for the rejected span's rejection reason.
//! * Commit the valid span durably (queryable by `trace_id`).
//!
//! This exercises the collector's per-span rejection path in
//! `vala_ingest::collector::map_resource_spans`, which surfaces reject
//! reasons into the OTLP `partial_success` envelope rather than failing the
//! whole batch.

mod pg_tests {
    use std::time::{Duration, Instant};

    use wyrd_testing::{Bootstrap, WyrdTestServer};
    use wyrd_tonic::otlp::common::v1::{AnyValue, KeyValue, any_value};
    use wyrd_tonic::otlp::resource::v1::Resource as OtlpResource;
    use wyrd_tonic::otlp::trace::v1::{
        ResourceSpans, ScopeSpans, Span as OtlpSpan, Status as OtlpStatus, span, status::StatusCode,
    };
    use wyrd_tonic::otlp::trace_service::ExportTraceServiceRequest;
    use wyrd_tonic::otlp::trace_service::trace_service_client::TraceServiceClient;
    use wyrd_tonic::prost::Message;
    use wyrd_tonic::tonic::Request;
    use wyrd_tonic::tonic::transport::Channel;
    use wyrd_tonic::wyrd::v1::GetTraceRequest;
    use wyrd_tonic::wyrd::v1::vala_query_service_client::ValaQueryServiceClient;

    /// The valid span's trace id — 16 bytes, must round-trip through the store.
    const VALID_TRACE_ID: [u8; 16] = [
        0x33, 0x34, 0x35, 0x36, 0x37, 0x38, 0x39, 0x3a, 0x3b, 0x3c, 0x3d, 0x3e, 0x3f, 0x40, 0x41,
        0x42,
    ];
    const VALID_SPAN_ID: [u8; 8] = [0xa1, 0xa2, 0xa3, 0xa4, 0xa5, 0xa6, 0xa7, 0xa8];

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

    fn kv(key: &str, value: any_value::Value) -> KeyValue {
        KeyValue {
            key: key.to_owned(),
            value: Some(AnyValue { value: Some(value) }),
        }
    }

    fn hex16(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{b:02x}")).collect()
    }

    fn with_grpc_token<T>(mut request: Request<T>, jwt: &str) -> Request<T> {
        request.metadata_mut().insert(
            "x-wyrd-access-token",
            format!("Bearer {jwt}").parse().expect("metadata value"),
        );
        request
    }

    async fn bootstrap_admin(srv: &WyrdTestServer, name: &str) -> String {
        match srv
            .bootstrap_user(name, &["admin"])
            .await
            .expect("bootstrap admin")
        {
            Bootstrap::User { jwt, .. } => jwt,
            other => panic!("expected user bootstrap, got {other:?}"),
        }
    }

    /// Build a valid OTLP span with the given ids.
    fn valid_span(trace_id: [u8; 16], span_id: [u8; 8]) -> OtlpSpan {
        OtlpSpan {
            trace_id: trace_id.to_vec(),
            span_id: span_id.to_vec(),
            parent_span_id: vec![],
            trace_state: String::new(),
            flags: 0,
            name: "mixed-batch valid".to_owned(),
            kind: span::SpanKind::Server as i32,
            start_time_unix_nano: 1_700_000_000_000_000_000,
            end_time_unix_nano: 1_700_000_000_050_000_000,
            attributes: vec![],
            dropped_attributes_count: 0,
            events: vec![],
            dropped_events_count: 0,
            links: vec![],
            dropped_links_count: 0,
            status: Some(OtlpStatus {
                message: String::new(),
                code: StatusCode::Ok as i32,
            }),
        }
    }

    /// Build a structurally invalid OTLP span: a 15-byte `trace_id` (spec
    /// mandates 16). `map_span` rejects it before the writer sees it.
    fn invalid_span() -> OtlpSpan {
        OtlpSpan {
            trace_id: vec![0u8; 15],
            span_id: vec![0xf1, 0xf2, 0xf3, 0xf4, 0xf5, 0xf6, 0xf7, 0xf8],
            parent_span_id: vec![],
            trace_state: String::new(),
            flags: 0,
            name: "mixed-batch invalid".to_owned(),
            kind: span::SpanKind::Server as i32,
            start_time_unix_nano: 1_700_000_000_000_000_000,
            end_time_unix_nano: 1_700_000_000_050_000_000,
            attributes: vec![],
            dropped_attributes_count: 0,
            events: vec![],
            dropped_events_count: 0,
            links: vec![],
            dropped_links_count: 0,
            status: Some(OtlpStatus {
                message: String::new(),
                code: StatusCode::Ok as i32,
            }),
        }
    }

    /// Build an export request that carries exactly two spans in one
    /// ScopeSpans: one valid, one invalid.
    fn mixed_export_request() -> ExportTraceServiceRequest {
        ExportTraceServiceRequest {
            resource_spans: vec![ResourceSpans {
                resource: Some(OtlpResource {
                    attributes: vec![kv(
                        "service.name",
                        any_value::Value::StringValue("mixed-batch-test".to_owned()),
                    )],
                    dropped_attributes_count: 0,
                }),
                scope_spans: vec![ScopeSpans {
                    scope: None,
                    spans: vec![valid_span(VALID_TRACE_ID, VALID_SPAN_ID), invalid_span()],
                    schema_url: String::new(),
                }],
                schema_url: String::new(),
            }],
        }
    }

    /// gRPC: one valid + one invalid span → partial_success with the reject
    /// count set, and the valid span is durably queryable.
    #[tokio::test]
    async fn grpc_mixed_batch_returns_partial_success_and_writes_valid_span() {
        let srv = WyrdTestServer::start_bound().await.expect("bound server");
        let admin_jwt = bootstrap_admin(&srv, "mixed-batch-grpc").await;
        let grpc = srv.grpc_url().expect("grpc url");
        let channel = connect(&grpc).await;

        let mut otlp = TraceServiceClient::new(channel.clone());
        let reply = otlp
            .export(with_grpc_token(
                Request::new(mixed_export_request()),
                &admin_jwt,
            ))
            .await
            .expect("mixed batch must not fail the whole request")
            .into_inner();

        // partial_success carries the reject count for the invalid span.
        let ps = reply
            .partial_success
            .expect("mixed batch must set partial_success with reject count");
        assert_eq!(
            ps.rejected_spans, 1,
            "exactly one span must be rejected; got {ps:?}"
        );
        assert!(
            !ps.error_message.is_empty(),
            "rejected span must carry a non-empty error_message reason; got {ps:?}"
        );

        // The valid span must be durably queryable.
        let mut query = ValaQueryServiceClient::new(channel);
        let waterfall = query
            .get_trace(with_grpc_token(
                Request::new(GetTraceRequest {
                    window: None,
                    trace_id: hex16(&VALID_TRACE_ID),
                }),
                &admin_jwt,
            ))
            .await
            .expect("get_trace succeeds")
            .into_inner()
            .trace
            .expect("valid span must be present after mixed batch");

        assert_eq!(
            waterfall.trace_id,
            hex16(&VALID_TRACE_ID),
            "the valid span's trace must be readable back"
        );
        assert_eq!(
            waterfall.spans.len(),
            1,
            "exactly the one accepted span must be written; got {}",
            waterfall.spans.len()
        );

        srv.shutdown().await.expect("shutdown");
    }

    /// HTTP: same mixed batch through `/v1/traces` must also succeed with
    /// `partial_success` set. Uses `application/x-protobuf` so we cover the
    /// non-gRPC OTLP path.
    #[tokio::test]
    async fn http_mixed_batch_returns_partial_success_and_writes_valid_span() {
        let srv = WyrdTestServer::start_bound().await.expect("bound server");
        let admin_jwt = bootstrap_admin(&srv, "mixed-batch-http").await;
        let base_url = srv.base_url().expect("http base url").to_owned();
        let grpc = srv.grpc_url().expect("grpc url");

        let body = mixed_export_request().encode_to_vec();
        let response = reqwest::Client::new()
            .post(format!("{base_url}/v1/traces"))
            .header("x-wyrd-access-token", format!("Bearer {admin_jwt}"))
            .header("content-type", "application/x-protobuf")
            .body(body)
            .send()
            .await
            .expect("request sent");

        assert_eq!(
            response.status(),
            200,
            "mixed batch must return HTTP 200 with partial_success — not a 4xx"
        );

        let body_bytes = response.bytes().await.expect("response body bytes");
        let decoded =
            wyrd_tonic::otlp::trace_service::ExportTraceServiceResponse::decode(&*body_bytes)
                .expect("HTTP body decodes as OTLP ExportTraceServiceResponse");
        let ps = decoded
            .partial_success
            .expect("mixed batch HTTP response must set partial_success");
        assert_eq!(
            ps.rejected_spans, 1,
            "exactly one span must be rejected; got {ps:?}"
        );
        assert!(
            !ps.error_message.is_empty(),
            "rejected span must carry a non-empty error_message; got {ps:?}"
        );

        // The valid span must be durably queryable.
        let mut query = ValaQueryServiceClient::new(connect(&grpc).await);
        let waterfall = query
            .get_trace(with_grpc_token(
                Request::new(GetTraceRequest {
                    window: None,
                    trace_id: hex16(&VALID_TRACE_ID),
                }),
                &admin_jwt,
            ))
            .await
            .expect("get_trace succeeds")
            .into_inner()
            .trace
            .expect("valid span must be present after mixed batch (HTTP)");

        assert_eq!(waterfall.trace_id, hex16(&VALID_TRACE_ID));
        assert_eq!(waterfall.spans.len(), 1);

        srv.shutdown().await.expect("shutdown");
    }
}
