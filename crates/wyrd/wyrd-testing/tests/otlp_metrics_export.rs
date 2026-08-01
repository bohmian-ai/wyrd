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

    use crate::otlp_support::export_and_flush;
    use wyrd_testing::{Bootstrap, WyrdTestServer};
    use wyrd_tonic::otlp::common::v1::{AnyValue, KeyValue, any_value};
    use wyrd_tonic::otlp::metrics::v1::{
        Metric, NumberDataPoint, ResourceMetrics, ScopeMetrics, Sum, metric, number_data_point,
    };
    use wyrd_tonic::otlp::metrics_service::ExportMetricsServiceRequest;
    use wyrd_tonic::otlp::metrics_service::metrics_service_client::MetricsServiceClient;
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
}
