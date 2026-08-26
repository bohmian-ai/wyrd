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

    use crate::support::{
        assert_grpc_otlp_limit, assert_http_otlp_limit, assert_otlp_owner_settled,
        capture_otlp_material_snapshot, export_and_flush, public_otlp_limits,
    };
    use wyrd_testing::{Bootstrap, WyrdTestServer};
    use wyrd_tonic::otlp::common::v1::{AnyValue, ArrayValue, KeyValue, KeyValueList, any_value};
    use wyrd_tonic::otlp::resource::v1::Resource as OtlpResource;
    use wyrd_tonic::otlp::trace::v1::{
        ResourceSpans, ScopeSpans, Span as OtlpSpan, Status as OtlpStatus, span, status::StatusCode,
    };
    use wyrd_tonic::otlp::trace_service::trace_service_client::TraceServiceClient;
    use wyrd_tonic::otlp::trace_service::{ExportTraceServiceRequest, ExportTraceServiceResponse};
    use wyrd_tonic::prost::Message;
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

    /// Build one alternating OTLP value tree at an exact recursive depth.
    fn alternating_value(depth: usize) -> AnyValue {
        if depth == 1 {
            return AnyValue {
                value: Some(any_value::Value::IntValue(1)),
            };
        }
        let nested = alternating_value(depth - 1);
        let value = if depth.is_multiple_of(2) {
            any_value::Value::ArrayValue(ArrayValue {
                values: vec![nested],
            })
        } else {
            any_value::Value::KvlistValue(KeyValueList {
                values: vec![KeyValue {
                    key: "nested".to_owned(),
                    value: Some(nested),
                }],
            })
        };
        AnyValue { value: Some(value) }
    }

    /// Build a trace request with exact signal-record and recursive-value counts.
    fn bounded_export_request(
        record_count: usize,
        value_depth: usize,
        marker: u8,
    ) -> ExportTraceServiceRequest {
        let spans = (0..record_count)
            .map(|index| OtlpSpan {
                trace_id: vec![marker; 16],
                span_id: vec![u8::try_from(index + 1).expect("small fixture index"); 8],
                parent_span_id: Vec::new(),
                trace_state: String::new(),
                flags: 0,
                name: format!("bounded-trace-{marker}-{index}"),
                kind: span::SpanKind::Internal as i32,
                start_time_unix_nano: 1_700_000_000_000_000_000,
                end_time_unix_nano: 1_700_000_000_001_000_000,
                attributes: vec![KeyValue {
                    key: "nested".to_owned(),
                    value: Some(alternating_value(value_depth)),
                }],
                dropped_attributes_count: 0,
                events: Vec::new(),
                dropped_events_count: 0,
                links: Vec::new(),
                dropped_links_count: 0,
                status: None,
            })
            .collect();
        ExportTraceServiceRequest {
            resource_spans: vec![ResourceSpans {
                resource: Some(OtlpResource {
                    attributes: vec![kv(
                        "service.name",
                        any_value::Value::StringValue(format!("bounded-trace-{marker}")),
                    )],
                    dropped_attributes_count: 0,
                    entity_refs: Vec::new(),
                }),
                scope_spans: vec![ScopeSpans {
                    scope: None,
                    spans,
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

    /// Public trace transports accept exact caps, reject cap plus one, and settle owners.
    #[tokio::test]
    async fn trace_http_and_grpc_enforce_bounded_decode_projection() {
        let limits = public_otlp_limits(1, 2);
        let srv = WyrdTestServer::builder()
            .with_scribe_ingest_limits_for_test(limits)
            .start_bound()
            .await
            .expect("bounded trace server");
        let jwt = match srv
            .bootstrap_user("bounded-trace-writer", &["admin"])
            .await
            .expect("bootstrap bounded trace writer")
        {
            Bootstrap::User { jwt, .. } => jwt,
            other => panic!("expected user bootstrap, got {other:?}"),
        };
        let grpc = srv.grpc_url().expect("gRPC URL");
        let mut traces = TraceServiceClient::new(connect(&grpc).await);

        let accepted_grpc = export_and_flush(
            &srv,
            traces.export(with_token(
                Request::new(bounded_export_request(1, 2, 0x31)),
                &jwt,
            )),
        )
        .await
        .into_inner();
        assert!(
            accepted_grpc
                .partial_success
                .as_ref()
                .is_none_or(|partial| { partial.rejected_spans == 0 }),
            "bounded trace must materialize one accepted span: {accepted_grpc:?}"
        );
        assert_otlp_owner_settled(&srv);

        for (records, depth, marker) in [(2, 1, 0x32), (1, 3, 0x33)] {
            let error = traces
                .export(with_token(
                    Request::new(bounded_export_request(records, depth, marker)),
                    &jwt,
                ))
                .await
                .expect_err("trace cap plus one must fail");
            assert_grpc_otlp_limit(&error);
            assert_otlp_owner_settled(&srv);
        }

        let client = reqwest::Client::new();
        let base = srv.base_url().expect("HTTP URL");
        let accepted = export_and_flush(
            &srv,
            client
                .post(format!("{base}/v1/traces"))
                .header("x-wyrd-access-token", format!("Bearer {jwt}"))
                .header("content-type", "application/x-protobuf")
                .body(bounded_export_request(1, 2, 0x34).encode_to_vec())
                .send(),
        )
        .await;
        assert_eq!(accepted.status(), 200);
        let accepted_http = ExportTraceServiceResponse::decode(
            accepted
                .bytes()
                .await
                .expect("bounded trace response bytes"),
        )
        .expect("bounded trace response protobuf");
        assert!(
            accepted_http
                .partial_success
                .as_ref()
                .is_none_or(|partial| partial.rejected_spans == 0),
            "bounded HTTP trace must materialize one accepted span: {accepted_http:?}"
        );
        assert_otlp_owner_settled(&srv);

        for (records, depth, marker) in [(2, 1, 0x35), (1, 3, 0x36)] {
            let response = client
                .post(format!("{base}/v1/traces"))
                .header("x-wyrd-access-token", format!("Bearer {jwt}"))
                .header("content-type", "application/x-protobuf")
                .body(bounded_export_request(records, depth, marker).encode_to_vec())
                .send()
                .await
                .expect("bounded trace request");
            assert_http_otlp_limit(response).await;
            assert_otlp_owner_settled(&srv);
        }

        srv.shutdown().await.expect("shutdown");
    }

    /// A representative trace cap refusal leaves no WAL or durable mutation.
    #[tokio::test]
    async fn trace_refusal_preserves_material_and_durable_baseline() {
        let srv = WyrdTestServer::builder()
            .with_scribe_ingest_limits_for_test(public_otlp_limits(1, 2))
            .start_bound()
            .await
            .expect("bounded trace server");
        let jwt = match srv
            .bootstrap_user("trace-refusal-baseline", &["admin"])
            .await
            .expect("bootstrap bounded trace writer")
        {
            Bootstrap::User { jwt, .. } => jwt,
            other => panic!("expected user bootstrap, got {other:?}"),
        };
        let grpc = srv.grpc_url().expect("gRPC URL");
        let channel = connect(&grpc).await;
        let mut traces = TraceServiceClient::new(channel.clone());

        export_and_flush(
            &srv,
            traces.export(with_token(
                Request::new(bounded_export_request(1, 2, 0x71)),
                &jwt,
            )),
        )
        .await;
        assert_otlp_owner_settled(&srv);
        let baseline = capture_otlp_material_snapshot(&srv, 0).await;

        const REFUSED_MARKER: u8 = 0x72;
        let error = traces
            .export(with_token(
                Request::new(bounded_export_request(2, 1, REFUSED_MARKER)),
                &jwt,
            ))
            .await
            .expect_err("trace cap plus one must fail");
        assert_grpc_otlp_limit(&error);
        srv.flush_bifrost()
            .await
            .expect("refused trace flush barrier");
        assert_otlp_owner_settled(&srv);
        let after_refusal = capture_otlp_material_snapshot(&srv, 0).await;

        let mut query = ValaQueryServiceClient::new(channel);
        let response = query
            .get_trace(with_token(
                Request::new(GetTraceRequest {
                    window: None,
                    trace_id: hex16(&[REFUSED_MARKER; 16]),
                }),
                &jwt,
            ))
            .await
            .expect("public trace marker query succeeds")
            .into_inner();
        let marker_rows = response.trace.map_or(0, |trace| trace.spans.len());
        assert_eq!(
            after_refusal.with_public_marker_rows(marker_rows),
            baseline,
            "refused trace must not change owners, durable state, or public rows"
        );
        srv.shutdown().await.expect("shutdown");
    }
}
