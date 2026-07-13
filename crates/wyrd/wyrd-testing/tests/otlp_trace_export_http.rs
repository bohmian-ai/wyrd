//! OTLP/HTTP trace export journey: an authenticated `ExportTraceServiceRequest`
//! POSTed to `/v1/traces` — in both `application/x-protobuf` and
//! `application/json` (OTLP protobuf-JSON) encodings — is decoded through Task
//! B's shared decode→write core, committed to `traces.spans`, and read back via
//! `ValaQueryService::GetTrace`. Proves the HTTP transport is at parity with the
//! gRPC collector: both encodings land an identical span.
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
    use wyrd_tonic::prost::Message;
    use wyrd_tonic::tonic::Request;
    use wyrd_tonic::tonic::transport::Channel;
    use wyrd_tonic::wyrd::v1::GetTraceRequest;
    use wyrd_tonic::wyrd::v1::vala_query_service_client::ValaQueryServiceClient;

    // Distinct trace/span ids per encoding so the two tests never collide on
    // read-back even when they share a database.
    const TRACE_ID_PROTOBUF: [u8; 16] = [
        0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, 0x99, 0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff,
        0x02,
    ];
    const SPAN_ID_PROTOBUF: [u8; 8] = [0xb1, 0xb2, 0xb3, 0xb4, 0xb5, 0xb6, 0xb7, 0xb8];
    const TRACE_ID_JSON: [u8; 16] = [
        0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, 0x99, 0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff,
        0x03,
    ];
    const SPAN_ID_JSON: [u8; 8] = [0xc1, 0xc2, 0xc3, 0xc4, 0xc5, 0xc6, 0xc7, 0xc8];

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

    fn export_request(trace_id: [u8; 16], span_id: [u8; 8]) -> ExportTraceServiceRequest {
        let span = OtlpSpan {
            trace_id: trace_id.to_vec(),
            span_id: span_id.to_vec(),
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

    async fn bootstrap_writer(srv: &WyrdTestServer) -> String {
        // admin carries the wildcard permission, which covers bifrost_record:write.
        match srv
            .bootstrap_user("otlp-http-writer", &["admin"])
            .await
            .expect("bootstrap user")
        {
            Bootstrap::User { jwt, .. } => jwt,
            other => panic!("expected user bootstrap, got {other:?}"),
        }
    }

    async fn read_back_span(channel: Channel, jwt: &str, trace_id: [u8; 16], span_id: [u8; 8]) {
        let mut query = ValaQueryServiceClient::new(channel);
        let waterfall = query
            .get_trace(with_token(
                Request::new(GetTraceRequest {
                    window: None,
                    trace_id: hex16(&trace_id),
                }),
                jwt,
            ))
            .await
            .expect("get_trace succeeds")
            .into_inner()
            .trace
            .expect("trace present");

        assert_eq!(waterfall.trace_id, hex16(&trace_id));
        assert_eq!(waterfall.spans.len(), 1, "exactly one span written");
        let row = &waterfall.spans[0];
        assert_eq!(row.span_id, hex16(&span_id));
        assert_eq!(row.name, "GET /checkout");
        assert_eq!(row.kind, "SERVER");
        assert_eq!(row.status, "OK");
    }

    #[tokio::test]
    async fn otlp_trace_export_http_protobuf_parity() {
        let srv = WyrdTestServer::start_bound().await.expect("bound server");
        let jwt = bootstrap_writer(&srv).await;
        let base_url = srv.base_url().expect("http base url").to_owned();

        // Export one span over OTLP/HTTP using application/x-protobuf.
        let body = export_request(TRACE_ID_PROTOBUF, SPAN_ID_PROTOBUF).encode_to_vec();
        let response = reqwest::Client::new()
            .post(format!("{base_url}/v1/traces"))
            .header("x-wyrd-access-token", format!("Bearer {jwt}"))
            .header("content-type", "application/x-protobuf")
            .body(body)
            .send()
            .await
            .expect("request sent");
        assert_eq!(response.status(), 200, "protobuf export must be accepted");
        assert_eq!(
            response
                .headers()
                .get("content-type")
                .and_then(|value| value.to_str().ok()),
            Some("application/x-protobuf"),
            "protobuf request must get a protobuf response"
        );

        let grpc = srv.grpc_url().expect("grpc url");
        let channel = connect(&grpc).await;
        read_back_span(channel, &jwt, TRACE_ID_PROTOBUF, SPAN_ID_PROTOBUF).await;

        srv.shutdown().await.expect("shutdown");
    }

    #[tokio::test]
    async fn otlp_trace_export_http_json_parity() {
        let srv = WyrdTestServer::start_bound().await.expect("bound server");
        let jwt = bootstrap_writer(&srv).await;
        let base_url = srv.base_url().expect("http base url").to_owned();

        // Export the same span over OTLP/HTTP using application/json
        // (OTLP protobuf-JSON), and require identical read-back.
        let request = export_request(TRACE_ID_JSON, SPAN_ID_JSON);
        let response = reqwest::Client::new()
            .post(format!("{base_url}/v1/traces"))
            .header("x-wyrd-access-token", format!("Bearer {jwt}"))
            .json(&request)
            .send()
            .await
            .expect("request sent");
        assert_eq!(response.status(), 200, "json export must be accepted");
        assert_eq!(
            response
                .headers()
                .get("content-type")
                .and_then(|value| value.to_str().ok())
                .map(|value| value
                    .split(';')
                    .next()
                    .unwrap_or_default()
                    .trim()
                    .to_owned()),
            Some("application/json".to_owned()),
            "json request must get a json response"
        );

        let grpc = srv.grpc_url().expect("grpc url");
        let channel = connect(&grpc).await;
        read_back_span(channel, &jwt, TRACE_ID_JSON, SPAN_ID_JSON).await;

        srv.shutdown().await.expect("shutdown");
    }
}
