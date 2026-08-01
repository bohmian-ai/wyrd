//! OTLP/gRPC trace export journey: an authenticated `ExportTraceServiceRequest`
//! is decoded, written through the live group-commit coordinator on
//! `traces.spans`, and read back via `ValaQueryService::GetTrace`.
//!
//! Also covers the negative auth flow: an export with no bearer is rejected with
//! gRPC `Unauthenticated` before any write.
//!
//! Both tests boot a `start_bound` server (PgFixture), so the fast family lane
//! skips them via `--skip pg_tests`; `mise run test:e2e` (Postgres up) runs the
//! whole crate.

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
    use wyrd_tonic::tonic::transport::Channel;
    use wyrd_tonic::tonic::{Code, Request};
    use wyrd_tonic::wyrd::v1::GetTraceRequest;
    use wyrd_tonic::wyrd::v1::vala_query_service_client::ValaQueryServiceClient;

    const TRACE_ID: [u8; 16] = [
        0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, 0x99, 0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff,
        0x01,
    ];
    const SPAN_ID: [u8; 8] = [0xa1, 0xa2, 0xa3, 0xa4, 0xa5, 0xa6, 0xa7, 0xa8];

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

    fn export_request() -> ExportTraceServiceRequest {
        let span = OtlpSpan {
            trace_id: TRACE_ID.to_vec(),
            span_id: SPAN_ID.to_vec(),
            parent_span_id: vec![],
            trace_state: String::new(),
            flags: 0,
            name: "GET /checkout".to_owned(),
            kind: span::SpanKind::Server as i32,
            start_time_unix_nano: 1_700_000_000_000_000_000,
            end_time_unix_nano: 1_700_000_000_050_000_000,
            attributes: vec![kv(
                "http.method",
                any_value::Value::StringValue("GET".to_owned()),
            )],
            dropped_attributes_count: 0,
            events: vec![],
            dropped_events_count: 0,
            links: vec![],
            dropped_links_count: 0,
            status: Some(OtlpStatus {
                message: String::new(),
                code: StatusCode::Ok as i32,
            }),
        };
        ExportTraceServiceRequest {
            resource_spans: vec![ResourceSpans {
                resource: Some(OtlpResource {
                    attributes: vec![kv(
                        "service.name",
                        any_value::Value::StringValue("checkout".to_owned()),
                    )],
                    dropped_attributes_count: 0,
                    entity_refs: Vec::new(),
                }),
                scope_spans: vec![ScopeSpans {
                    scope: None,
                    spans: vec![span],
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

    fn hex16(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{b:02x}")).collect()
    }

    /// Proves one bound server uses its single Gate/Scribe runtime for an
    /// authenticated OTLP write, explicit durable flush, query readback, and
    /// ordered shutdown.
    #[tokio::test]
    async fn bifrost_ingest_runtime_journey() {
        let srv = WyrdTestServer::start_bound().await.expect("bound server");
        // admin carries the wildcard permission, which covers bifrost_record:write.
        let jwt = match srv
            .bootstrap_user("otlp-writer", &["admin"])
            .await
            .expect("bootstrap user")
        {
            Bootstrap::User { jwt, .. } => jwt,
            other => panic!("expected user bootstrap, got {other:?}"),
        };

        let grpc = srv.grpc_url().expect("grpc url");
        let channel = connect(&grpc).await;

        // Export one span over OTLP/gRPC.
        let mut otlp = TraceServiceClient::new(channel.clone());
        let response = otlp
            .export(with_token(Request::new(export_request()), &jwt))
            .await
            .expect("export succeeds")
            .into_inner();
        assert!(
            response.partial_success.is_none()
                || response
                    .partial_success
                    .as_ref()
                    .is_some_and(|p| p.rejected_spans == 0),
            "a well-formed span must not be rejected: {response:?}"
        );

        // Force the same server-owned Scribe allocation behind Gate to seal its
        // accepted work before the query projection reads the durable result.
        srv.flush_bifrost().await.expect("Scribe flush succeeds");

        // Read it back through the live query path by trace id after durability.
        let mut query = ValaQueryServiceClient::new(channel);
        let waterfall = query
            .get_trace(with_token(
                Request::new(GetTraceRequest {
                    window: None,
                    trace_id: hex16(&TRACE_ID),
                }),
                &jwt,
            ))
            .await
            .expect("get_trace succeeds")
            .into_inner()
            .trace
            .expect("trace present");

        assert_eq!(waterfall.trace_id, hex16(&TRACE_ID));
        assert_eq!(waterfall.spans.len(), 1, "exactly one span written");
        let row = &waterfall.spans[0];
        assert_eq!(row.span_id, hex16(&SPAN_ID));
        assert_eq!(row.name, "GET /checkout");
        assert_eq!(row.kind, "SERVER");
        assert_eq!(row.status, "OK");

        srv.shutdown().await.expect("shutdown");
    }

    #[tokio::test]
    async fn otlp_export_unauthenticated_rejected() {
        let srv = WyrdTestServer::start_bound().await.expect("bound server");
        let grpc = srv.grpc_url().expect("grpc url");
        let mut otlp = TraceServiceClient::new(connect(&grpc).await);

        let status = otlp
            .export(Request::new(export_request()))
            .await
            .expect_err("unauthenticated export must be rejected");
        assert_eq!(
            status.code(),
            Code::Unauthenticated,
            "no token must yield Unauthenticated; got {status:?}"
        );

        srv.shutdown().await.expect("shutdown");
    }
}
