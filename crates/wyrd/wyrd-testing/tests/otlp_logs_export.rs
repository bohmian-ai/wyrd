//! OTLP logs export journey: an authenticated `ExportLogsServiceRequest` is
//! decoded, written through the live group-commit coordinator on `logs.records`,
//! and read back via `ValaQueryService::QueryLogs`.
//!
//! Covers both transports: OTLP/gRPC (`LogsService::Export`) and OTLP/HTTP
//! (`POST /v1/logs`, `application/x-protobuf`). Each writes a distinct trace id so
//! the two tests never collide on read-back when they share a database.
//!
//! Both tests boot a `start_bound` server (PgFixture), so the fast family lane
//! skips them via `--skip pg_tests`; `mise run test:e2e` (Postgres up) runs the
//! whole crate.

mod pg_tests {
    use std::time::{Duration, Instant};

    use wyrd_testing::{Bootstrap, WyrdTestServer};
    use wyrd_tonic::otlp::common::v1::{AnyValue, KeyValue, any_value};
    use wyrd_tonic::otlp::logs::v1::{
        LogRecord as OtlpLog, ResourceLogs, ScopeLogs, SeverityNumber,
    };
    use wyrd_tonic::otlp::logs_service::ExportLogsServiceRequest;
    use wyrd_tonic::otlp::logs_service::logs_service_client::LogsServiceClient;
    use wyrd_tonic::otlp::resource::v1::Resource as OtlpResource;
    use wyrd_tonic::prost::Message;
    use wyrd_tonic::tonic::Request;
    use wyrd_tonic::tonic::transport::Channel;
    use wyrd_tonic::wyrd::v1::vala_query_service_client::ValaQueryServiceClient;
    use wyrd_tonic::wyrd::v1::{QueryLogsRequest, QueryWindow};

    const TRACE_ID_GRPC: [u8; 16] = [
        0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, 0x99, 0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff,
        0x0a,
    ];
    const SPAN_ID_GRPC: [u8; 8] = [0xd1, 0xd2, 0xd3, 0xd4, 0xd5, 0xd6, 0xd7, 0xd8];
    const TRACE_ID_HTTP: [u8; 16] = [
        0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, 0x99, 0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff,
        0x0b,
    ];
    const SPAN_ID_HTTP: [u8; 8] = [0xe1, 0xe2, 0xe3, 0xe4, 0xe5, 0xe6, 0xe7, 0xe8];

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

    fn export_request(trace_id: [u8; 16], span_id: [u8; 8]) -> ExportLogsServiceRequest {
        let log = OtlpLog {
            time_unix_nano: 1_700_000_000_000_000_000,
            observed_time_unix_nano: 1_700_000_000_050_000_000,
            severity_number: SeverityNumber::Error as i32,
            severity_text: "ERROR".to_owned(),
            body: Some(AnyValue {
                value: Some(any_value::Value::StringValue("boom".to_owned())),
            }),
            attributes: vec![kv(
                "log.source",
                any_value::Value::StringValue("app".to_owned()),
            )],
            dropped_attributes_count: 0,
            flags: 1,
            trace_id: trace_id.to_vec(),
            span_id: span_id.to_vec(),
        };
        ExportLogsServiceRequest {
            resource_logs: vec![ResourceLogs {
                resource: Some(OtlpResource {
                    attributes: vec![kv(
                        "service.name",
                        any_value::Value::StringValue("checkout".to_owned()),
                    )],
                    dropped_attributes_count: 0,
                }),
                scope_logs: vec![ScopeLogs {
                    scope: None,
                    log_records: vec![log],
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

    async fn read_back_log(channel: Channel, jwt: &str, trace_id: [u8; 16], span_id: [u8; 8]) {
        let mut query = ValaQueryServiceClient::new(channel);
        let rows = query
            .query_logs(with_token(
                Request::new(QueryLogsRequest {
                    window: Some(QueryWindow {
                        since: String::new(),
                        until: String::new(),
                        limit: 100,
                        page_token: String::new(),
                    }),
                    severity_number_min: 0,
                    trace_id: hex16(&trace_id),
                    event_name: String::new(),
                }),
                jwt,
            ))
            .await
            .expect("query_logs succeeds")
            .into_inner()
            .rows;

        assert_eq!(rows.len(), 1, "exactly one log written for this trace");
        let row = &rows[0];
        assert_eq!(row.severity_number, SeverityNumber::Error as i32);
        assert_eq!(row.severity_text, "ERROR");
        assert_eq!(row.trace_id, hex16(&trace_id));
        assert_eq!(row.span_id, hex16(&span_id));
    }

    #[tokio::test]
    async fn otlp_logs_export_grpc_write_read() {
        let srv = WyrdTestServer::start_bound().await.expect("bound server");
        let jwt = bootstrap_writer(&srv, "otlp-logs-grpc").await;
        let grpc = srv.grpc_url().expect("grpc url");
        let channel = connect(&grpc).await;

        let mut otlp = LogsServiceClient::new(channel.clone());
        let response = otlp
            .export(with_token(
                Request::new(export_request(TRACE_ID_GRPC, SPAN_ID_GRPC)),
                &jwt,
            ))
            .await
            .expect("export succeeds")
            .into_inner();
        assert!(
            response.partial_success.is_none()
                || response
                    .partial_success
                    .as_ref()
                    .is_some_and(|p| p.rejected_log_records == 0),
            "a well-formed record must not be rejected: {response:?}"
        );

        read_back_log(channel, &jwt, TRACE_ID_GRPC, SPAN_ID_GRPC).await;

        srv.shutdown().await.expect("shutdown");
    }

    #[tokio::test]
    async fn otlp_logs_export_http_write_read() {
        let srv = WyrdTestServer::start_bound().await.expect("bound server");
        let jwt = bootstrap_writer(&srv, "otlp-logs-http").await;
        let base_url = srv.base_url().expect("http base url").to_owned();

        let body = export_request(TRACE_ID_HTTP, SPAN_ID_HTTP).encode_to_vec();
        let response = reqwest::Client::new()
            .post(format!("{base_url}/v1/logs"))
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
        read_back_log(channel, &jwt, TRACE_ID_HTTP, SPAN_ID_HTTP).await;

        srv.shutdown().await.expect("shutdown");
    }
}
