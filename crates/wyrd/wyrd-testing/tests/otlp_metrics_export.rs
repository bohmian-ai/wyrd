//! OTLP metrics export journey: an authenticated `ExportMetricsServiceRequest`
//! is decoded, written through the live group-commit coordinator on
//! `metrics.points`, and read back via `ValaQueryService::QueryMetrics`.
//!
//! Covers both transports: OTLP/gRPC (`MetricsService::Export`) and OTLP/HTTP
//! (`POST /v1/metrics`, `application/x-protobuf`). Each writes a distinct metric
//! name so the two tests never collide on read-back when they share a database.
//!
//! Both tests boot a `start_bound` server (PgFixture), so the fast family lane
//! skips them via `--skip pg_tests`; `mise run test:e2e` (Postgres up) runs the
//! whole crate.

mod pg_tests {
    use std::time::{Duration, Instant};

    use crate::otlp_support::{
        assert_grpc_otlp_limit, assert_http_otlp_limit, assert_otlp_owner_settled,
        capture_otlp_material_snapshot, export_and_flush, public_otlp_limits,
    };
    use wyrd_testing::{Bootstrap, WyrdTestServer};
    use wyrd_tonic::otlp::common::v1::{AnyValue, ArrayValue, KeyValue, KeyValueList, any_value};
    use wyrd_tonic::otlp::metrics::v1::{
        Metric, NumberDataPoint, ResourceMetrics, ScopeMetrics, Sum, metric, number_data_point,
    };
    use wyrd_tonic::otlp::metrics_service::metrics_service_client::MetricsServiceClient;
    use wyrd_tonic::otlp::metrics_service::{
        ExportMetricsServiceRequest, ExportMetricsServiceResponse,
    };
    use wyrd_tonic::otlp::resource::v1::Resource as OtlpResource;
    use wyrd_tonic::prost::Message;
    use wyrd_tonic::tonic::Request;
    use wyrd_tonic::tonic::transport::Channel;
    use wyrd_tonic::wyrd::v1::vala_query_service_client::ValaQueryServiceClient;
    use wyrd_tonic::wyrd::v1::{QueryMetricsRequest, QueryWindow};

    // OTLP AggregationTemporality: Unspecified=0, Delta=1, Cumulative=2.
    const OTLP_TEMPORALITY_CUMULATIVE: i32 = 2;

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

    fn export_request(metric_name: &str) -> ExportMetricsServiceRequest {
        let point = NumberDataPoint {
            attributes: vec![kv(
                "http.method",
                any_value::Value::StringValue("GET".to_owned()),
            )],
            start_time_unix_nano: 1_700_000_000_000_000_000,
            time_unix_nano: 1_700_000_000_050_000_000,
            exemplars: vec![],
            flags: 0,
            value: Some(number_data_point::Value::AsDouble(42.0)),
        };
        let metric = Metric {
            name: metric_name.to_owned(),
            description: "count of requests".to_owned(),
            unit: "{request}".to_owned(),
            metadata: vec![],
            data: Some(metric::Data::Sum(Sum {
                data_points: vec![point],
                aggregation_temporality: OTLP_TEMPORALITY_CUMULATIVE,
                is_monotonic: true,
            })),
        };
        ExportMetricsServiceRequest {
            resource_metrics: vec![ResourceMetrics {
                resource: Some(OtlpResource {
                    attributes: vec![kv(
                        "service.name",
                        any_value::Value::StringValue("checkout".to_owned()),
                    )],
                    dropped_attributes_count: 0,
                    entity_refs: Vec::new(),
                }),
                scope_metrics: vec![ScopeMetrics {
                    scope: None,
                    metrics: vec![metric],
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

    /// Build a metrics request with exact point and recursive-value counts.
    fn bounded_export_request(
        record_count: usize,
        value_depth: usize,
        marker: &str,
    ) -> ExportMetricsServiceRequest {
        let data_points = (0..record_count)
            .map(|index| NumberDataPoint {
                attributes: vec![KeyValue {
                    key: "nested".to_owned(),
                    value: Some(alternating_value(value_depth)),
                }],
                start_time_unix_nano: 1_700_000_000_000_000_000,
                time_unix_nano: 1_700_000_000_001_000_000,
                exemplars: Vec::new(),
                flags: 0,
                value: Some(number_data_point::Value::AsInt(
                    i64::try_from(index).expect("small fixture index"),
                )),
            })
            .collect();
        ExportMetricsServiceRequest {
            resource_metrics: vec![ResourceMetrics {
                resource: Some(OtlpResource {
                    attributes: vec![kv(
                        "service.name",
                        any_value::Value::StringValue(format!("bounded-metrics-{marker}")),
                    )],
                    dropped_attributes_count: 0,
                    entity_refs: Vec::new(),
                }),
                scope_metrics: vec![ScopeMetrics {
                    scope: None,
                    metrics: vec![Metric {
                        name: format!("bounded.metric.{marker}"),
                        description: String::new(),
                        unit: String::new(),
                        metadata: Vec::new(),
                        data: Some(metric::Data::Sum(Sum {
                            data_points,
                            aggregation_temporality: OTLP_TEMPORALITY_CUMULATIVE,
                            is_monotonic: true,
                        })),
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

    async fn bootstrap_writer(srv: &WyrdTestServer, name: &str) -> String {
        // admin carries the wildcard permission, which covers bifrost_record:write.
        match srv
            .bootstrap_user(name, &["admin"])
            .await
            .expect("bootstrap user")
        {
            Bootstrap::User { jwt, .. } => jwt,
            other => panic!("expected user bootstrap, got {other:?}"),
        }
    }

    async fn read_back_metric(channel: Channel, jwt: &str, metric_name: &str) {
        let mut query = ValaQueryServiceClient::new(channel);
        let rows = query
            .query_metrics(with_token(
                Request::new(QueryMetricsRequest {
                    window: Some(QueryWindow {
                        since: String::new(),
                        until: String::new(),
                        limit: 100,
                        page_token: String::new(),
                    }),
                    metric_name: metric_name.to_owned(),
                    metric_type: "sum".to_owned(),
                }),
                jwt,
            ))
            .await
            .expect("query_metrics succeeds")
            .into_inner()
            .rows;

        let row = rows
            .iter()
            .find(|r| r.metric_name == metric_name)
            .unwrap_or_else(|| panic!("metric {metric_name} present, got {rows:?}"));
        assert_eq!(row.metric_type, "sum");
        assert_eq!(row.value, 42.0);
    }

    #[tokio::test]
    async fn otlp_metrics_export_grpc_write_read() {
        let srv = WyrdTestServer::start_bound().await.expect("bound server");
        let jwt = bootstrap_writer(&srv, "otlp-metrics-grpc").await;
        let grpc = srv.grpc_url().expect("grpc url");
        let channel = connect(&grpc).await;

        let mut otlp = MetricsServiceClient::new(channel.clone());
        let response = export_and_flush(
            &srv,
            otlp.export(with_token(
                Request::new(export_request("http.server.requests.grpc")),
                &jwt,
            )),
        )
        .await
        .into_inner();
        assert!(
            response.partial_success.is_none()
                || response
                    .partial_success
                    .as_ref()
                    .is_some_and(|p| p.rejected_data_points == 0),
            "a well-formed point must not be rejected: {response:?}"
        );

        read_back_metric(channel, &jwt, "http.server.requests.grpc").await;

        srv.shutdown().await.expect("shutdown");
    }

    #[tokio::test]
    async fn otlp_metrics_export_http_write_read() {
        let srv = WyrdTestServer::start_bound().await.expect("bound server");
        let jwt = bootstrap_writer(&srv, "otlp-metrics-http").await;
        let base_url = srv.base_url().expect("http base url").to_owned();

        let body = export_request("http.server.requests.http").encode_to_vec();
        let response = export_and_flush(
            &srv,
            reqwest::Client::new()
                .post(format!("{base_url}/v1/metrics"))
                .header("x-wyrd-access-token", format!("Bearer {jwt}"))
                .header("content-type", "application/x-protobuf")
                .body(body)
                .send(),
        )
        .await;
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
        read_back_metric(channel, &jwt, "http.server.requests.http").await;

        srv.shutdown().await.expect("shutdown");
    }

    /// OTLP/HTTP JSON metrics reach the same durable public read path as protobuf.
    #[tokio::test]
    async fn metrics_http_json_success_and_readback() {
        let srv = WyrdTestServer::start_bound().await.expect("bound server");
        let jwt = bootstrap_writer(&srv, "otlp-metrics-http-json").await;
        let base_url = srv.base_url().expect("http base url").to_owned();
        let metric_name = "http.server.requests.http_json";

        let response = export_and_flush(
            &srv,
            reqwest::Client::new()
                .post(format!("{base_url}/v1/metrics"))
                .header("x-wyrd-access-token", format!("Bearer {jwt}"))
                .json(&export_request(metric_name))
                .send(),
        )
        .await;
        assert_eq!(response.status(), 200, "JSON export must be accepted");
        assert_eq!(
            response
                .headers()
                .get("content-type")
                .and_then(|value| value.to_str().ok())
                .and_then(|value| value.split(';').next()),
            Some("application/json"),
            "JSON request must get a JSON response"
        );

        let grpc = srv.grpc_url().expect("gRPC URL");
        read_back_metric(connect(&grpc).await, &jwt, metric_name).await;
        srv.shutdown().await.expect("shutdown");
    }

    /// Public metrics transports accept exact caps, reject cap plus one, and settle owners.
    #[tokio::test]
    async fn metrics_http_and_grpc_enforce_bounded_decode_projection() {
        let srv = WyrdTestServer::builder()
            .with_scribe_ingest_limits_for_test(public_otlp_limits(1, 2))
            .start_bound()
            .await
            .expect("bounded metrics server");
        let jwt = bootstrap_writer(&srv, "bounded-metrics-writer").await;
        let grpc = srv.grpc_url().expect("gRPC URL");
        let mut metrics = MetricsServiceClient::new(connect(&grpc).await);

        let accepted_grpc = export_and_flush(
            &srv,
            metrics.export(with_token(
                Request::new(bounded_export_request(1, 2, "grpc-cap")),
                &jwt,
            )),
        )
        .await
        .into_inner();
        assert!(
            accepted_grpc
                .partial_success
                .as_ref()
                .is_none_or(|partial| partial.rejected_data_points == 0),
            "bounded gRPC metric must materialize one accepted point: {accepted_grpc:?}"
        );
        assert_otlp_owner_settled(&srv);
        for (records, depth, marker) in [(2, 1, "grpc-records"), (1, 3, "grpc-depth")] {
            let error = metrics
                .export(with_token(
                    Request::new(bounded_export_request(records, depth, marker)),
                    &jwt,
                ))
                .await
                .expect_err("metrics cap plus one must fail");
            assert_grpc_otlp_limit(&error);
            assert_otlp_owner_settled(&srv);
        }

        let client = reqwest::Client::new();
        let base = srv.base_url().expect("HTTP URL");
        let accepted = export_and_flush(
            &srv,
            client
                .post(format!("{base}/v1/metrics"))
                .header("x-wyrd-access-token", format!("Bearer {jwt}"))
                .header("content-type", "application/x-protobuf")
                .body(bounded_export_request(1, 2, "http-cap").encode_to_vec())
                .send(),
        )
        .await;
        assert_eq!(accepted.status(), 200);
        let accepted_http = ExportMetricsServiceResponse::decode(
            accepted
                .bytes()
                .await
                .expect("bounded metrics response bytes"),
        )
        .expect("bounded metrics response protobuf");
        assert!(
            accepted_http
                .partial_success
                .as_ref()
                .is_none_or(|partial| partial.rejected_data_points == 0),
            "bounded HTTP metric must materialize one accepted point: {accepted_http:?}"
        );
        assert_otlp_owner_settled(&srv);
        for (records, depth, marker) in [(2, 1, "http-records"), (1, 3, "http-depth")] {
            let response = client
                .post(format!("{base}/v1/metrics"))
                .header("x-wyrd-access-token", format!("Bearer {jwt}"))
                .header("content-type", "application/x-protobuf")
                .body(bounded_export_request(records, depth, marker).encode_to_vec())
                .send()
                .await
                .expect("bounded metrics request");
            assert_http_otlp_limit(response).await;
            assert_otlp_owner_settled(&srv);
        }

        srv.shutdown().await.expect("shutdown");
    }

    /// A representative metrics cap refusal leaves no WAL or durable mutation.
    #[tokio::test]
    async fn metrics_refusal_preserves_material_and_durable_baseline() {
        let srv = WyrdTestServer::builder()
            .with_scribe_ingest_limits_for_test(public_otlp_limits(1, 2))
            .start_bound()
            .await
            .expect("bounded metrics server");
        let jwt = bootstrap_writer(&srv, "metrics-refusal-baseline").await;
        let grpc = srv.grpc_url().expect("gRPC URL");
        let channel = connect(&grpc).await;
        let mut metrics = MetricsServiceClient::new(channel.clone());

        export_and_flush(
            &srv,
            metrics.export(with_token(
                Request::new(bounded_export_request(1, 2, "baseline")),
                &jwt,
            )),
        )
        .await;
        assert_otlp_owner_settled(&srv);
        let baseline = capture_otlp_material_snapshot(&srv, 0).await;

        let marker = "cap-plus-one";
        let error = metrics
            .export(with_token(
                Request::new(bounded_export_request(2, 1, marker)),
                &jwt,
            ))
            .await
            .expect_err("metrics cap plus one must fail");
        assert_grpc_otlp_limit(&error);
        srv.flush_bifrost()
            .await
            .expect("refused metrics flush barrier");
        assert_otlp_owner_settled(&srv);
        let after_refusal = capture_otlp_material_snapshot(&srv, 0).await;

        let mut query = ValaQueryServiceClient::new(channel);
        let rows = query
            .query_metrics(with_token(
                Request::new(QueryMetricsRequest {
                    window: Some(QueryWindow {
                        since: String::new(),
                        until: String::new(),
                        limit: 100,
                        page_token: String::new(),
                    }),
                    metric_name: format!("bounded.metric.{marker}"),
                    metric_type: "sum".to_owned(),
                }),
                &jwt,
            ))
            .await
            .expect("public metrics marker query succeeds")
            .into_inner()
            .rows;
        assert_eq!(
            after_refusal.with_public_marker_rows(rows.len()),
            baseline,
            "refused metric must not change owners, durable state, or public rows"
        );
        srv.shutdown().await.expect("shutdown");
    }
}
