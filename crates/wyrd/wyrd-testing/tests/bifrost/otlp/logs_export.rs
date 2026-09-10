//! The OTLP log journey: body, context and payload authorization.

use wyrd_tonic::otlp::logs::v1::ResourceLogs;
use wyrd_tonic::otlp::logs_service::logs_service_client::LogsServiceClient;
use wyrd_tonic::otlp::logs_service::{ExportLogsServiceRequest, ExportLogsServiceResponse};
use wyrd_tonic::tonic::Request;
use wyrd_tonic::tonic::transport::Channel;

use super::support::OtlpJourney;
use super::trace_export_http::{HttpEncoding, post_otlp};

/// Sends one OTLP log export through the bound gRPC collector route.
///
/// # Panics
///
/// Panics when the transport cannot be dialed or the export is refused.
pub(super) async fn export_logs_over_grpc(journey: &OtlpJourney, resource_logs: Vec<ResourceLogs>) {
    let channel = Channel::from_shared(journey.grpc_url())
        .expect("the bound gRPC URL is a valid endpoint")
        .connect()
        .await
        .expect("the OTLP exporter dials the bound collector");
    let mut request = Request::new(ExportLogsServiceRequest { resource_logs });
    request.metadata_mut().insert(
        "x-wyrd-access-token",
        format!("Bearer {}", journey.token())
            .parse()
            .expect("the minted bearer is valid ASCII metadata"),
    );
    let partial = LogsServiceClient::new(channel)
        .export(request)
        .await
        .expect("the collector accepts the log export")
        .into_inner()
        .partial_success;
    assert!(
        partial.is_none_or(|partial| partial.rejected_log_records == 0),
        "a wholly valid export reports no rejected log record"
    );
}

/// Posts one log export in the requested HTTP encoding.
///
/// # Panics
///
/// Panics when the collector refuses the export.
pub(super) async fn export_logs_over_http(
    journey: &OtlpJourney,
    encoding: HttpEncoding,
    resource_logs: Vec<ResourceLogs>,
) {
    let request = ExportLogsServiceRequest { resource_logs };
    let body = post_otlp(journey, "/v1/logs", encoding, encoding.encode(&request)).await;
    let response: ExportLogsServiceResponse = encoding.decode(&body);
    if let Some(partial) = response.partial_success {
        assert_eq!(
            partial.rejected_log_records,
            0,
            "a wholly valid {} export reports no rejected log record: {}",
            encoding.content_type(),
            partial.error_message
        );
    }
}

/// Tests that need Postgres, a bound server, and the publication boundary.
mod pg_tests {
    use arrow::array::{
        BooleanArray, FixedSizeBinaryArray, Int32Array, Int64Array, LargeBinaryArray, StringArray,
    };
    use wyrd_runtime::Permission;
    use wyrd_tonic::prost::Message;

    use super::super::support::{
        self, GRPC_SPAN, LOG_DROPPED_ATTRIBUTES, LOG_EVENT_NAME, LOG_FLAGS,
        LOG_OBSERVED_OFFSET_NANOS, LOG_SCOPE_NAME, LOG_SEVERITY_NUMBER, LOG_SEVERITY_TEXT,
        LOGS_TABLE, OtlpJourney, RESOURCE_DROPPED_ATTRIBUTES, RESOURCE_SCHEMA_URL,
        SCOPE_DROPPED_ATTRIBUTES, SCOPE_SCHEMA_URL, SCOPE_VERSION, column,
    };
    use super::super::trace_export_http::HttpEncoding;
    use super::{export_logs_over_grpc, export_logs_over_http};

    /// Every canonical log column that carries caller content.
    ///
    /// These are exactly `vala.logs.records`'s declared sensitive payload
    /// columns; a caller without `bifrost_log_payload:read` must not be able
    /// to read any of them, individually or through `SELECT *`.
    const PAYLOAD_COLUMNS: [&str; 5] = [
        "body",
        "attributes",
        "resource_attributes",
        "resource_entity_refs",
        "scope_attributes",
    ];

    /// The metadata columns the same unauthorized caller must still read.
    const PERMITTED_COLUMNS: &str = "time_unix_nano, severity_number, severity_text, event_name";

    /// A log record round-trips its body and context, and its payload is gated.
    ///
    /// One maximal log record — a present body, an `OTel` event name, a real
    /// trace/span correlation, every attribute shape, and a full resource and
    /// scope envelope — is exported over all three transports and read back
    /// through canonical SQL. Storing the record is only half the contract:
    /// its body and attributes are declared sensitive payload, so the same
    /// published rows are then read by a second authenticated principal that
    /// holds `bifrost_query:read` and deliberately does not hold
    /// `bifrost_log_payload:read`. That caller must still read the permitted
    /// metadata and must be refused every protected column, including through
    /// `SELECT *` — refused before projection rather than served a redacted
    /// or empty value that a client could mistake for "nothing was recorded".
    ///
    /// # Panics
    ///
    /// Panics when an export is refused, when a stored column differs from
    /// what was sent, when the unauthorized caller receives protected content,
    /// or when that caller is refused the metadata it is entitled to.
    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "requires the Postgres-backed Bifrost journey lane"]
    async fn otlp_log_routes_round_trip_body_context_and_redaction() {
        let journey = OtlpJourney::start().await;
        let anchor = support::anchor_nanos();

        export_logs_over_grpc(&journey, support::maximal_resource_logs(anchor)).await;
        for (offset, encoding) in [(1, HttpEncoding::Protobuf), (2, HttpEncoding::Json)] {
            export_logs_over_http(
                &journey,
                encoding,
                support::maximal_resource_logs(anchor + offset),
            )
            .await;
        }
        journey.publish().await;

        for (offset, transport) in [(0, "gRPC"), (1, "HTTP protobuf"), (2, "HTTP JSON")] {
            let time = anchor + offset;
            let row = journey
                .query_one_row(&format!(
                    "SELECT * FROM {LOGS_TABLE} WHERE time_unix_nano = {time}"
                ))
                .await;
            assert_eq!(
                column::<Int64Array>(&row, "observed_time_unix_nano").value(0),
                time + LOG_OBSERVED_OFFSET_NANOS,
                "{transport} keeps the observed instant distinct from the record instant"
            );
            assert_eq!(
                column::<Int32Array>(&row, "severity_number").value(0),
                LOG_SEVERITY_NUMBER
            );
            assert_eq!(
                column::<StringArray>(&row, "severity_text").value(0),
                LOG_SEVERITY_TEXT
            );
            assert_eq!(
                column::<StringArray>(&row, "event_name").value(0),
                LOG_EVENT_NAME,
                "{transport} stores the OTel event name rather than folding it away"
            );
            assert_eq!(
                column::<LargeBinaryArray>(&row, "body").value(0),
                support::log_body().encode_to_vec(),
                "{transport} keeps the body as its canonical AnyValue encoding"
            );
            assert_eq!(
                column::<FixedSizeBinaryArray>(&row, "trace_id").value(0),
                GRPC_SPAN.trace_id,
                "{transport} keeps the record's trace correlation"
            );
            assert_eq!(
                column::<FixedSizeBinaryArray>(&row, "span_id").value(0),
                GRPC_SPAN.span_id
            );
            assert_eq!(column::<Int64Array>(&row, "flags").value(0), LOG_FLAGS);
            assert_eq!(
                column::<LargeBinaryArray>(&row, "attributes").value(0),
                support::canonical_attribute_bytes(&support::log_attributes())
            );
            assert_eq!(
                column::<Int64Array>(&row, "dropped_attributes_count").value(0),
                LOG_DROPPED_ATTRIBUTES
            );
            assert!(column::<BooleanArray>(&row, "resource_present").value(0));
            assert_eq!(
                column::<LargeBinaryArray>(&row, "resource_attributes").value(0),
                support::canonical_attribute_bytes(&support::resource_attributes())
            );
            assert_eq!(
                column::<Int64Array>(&row, "resource_dropped_attributes_count").value(0),
                RESOURCE_DROPPED_ATTRIBUTES
            );
            assert_eq!(
                column::<StringArray>(&row, "resource_schema_url").value(0),
                RESOURCE_SCHEMA_URL
            );
            assert!(column::<BooleanArray>(&row, "scope_present").value(0));
            assert_eq!(
                column::<StringArray>(&row, "scope_name").value(0),
                LOG_SCOPE_NAME
            );
            assert_eq!(
                column::<StringArray>(&row, "scope_version").value(0),
                SCOPE_VERSION
            );
            assert_eq!(
                column::<LargeBinaryArray>(&row, "scope_attributes").value(0),
                support::canonical_attribute_bytes(&support::scope_attributes())
            );
            assert_eq!(
                column::<Int64Array>(&row, "scope_dropped_attributes_count").value(0),
                SCOPE_DROPPED_ATTRIBUTES
            );
            assert_eq!(
                column::<StringArray>(&row, "scope_schema_url").value(0),
                SCOPE_SCHEMA_URL
            );
        }

        let metadata_only = journey
            .client_with_permissions("otlp_log_metadata", &[Permission::bifrost_query_read()])
            .await;
        let permitted = journey
            .query_as(
                &metadata_only,
                &format!(
                    "SELECT {PERMITTED_COLUMNS} FROM {LOGS_TABLE} WHERE time_unix_nano = {anchor}"
                ),
            )
            .await;
        assert_eq!(
            permitted
                .iter()
                .map(arrow::array::RecordBatch::num_rows)
                .sum::<usize>(),
            1,
            "a caller without payload permission still reads permitted metadata"
        );

        for column_name in PAYLOAD_COLUMNS {
            let refusal = journey
                .query_error(
                    &metadata_only,
                    &format!("SELECT {column_name} FROM {LOGS_TABLE}"),
                )
                .await;
            assert_eq!(
                refusal, "WYRD_VALA_403_PAYLOAD_FORBIDDEN",
                "projecting `{column_name}` without payload permission is refused: {refusal:?}"
            );
        }
        let star = journey
            .query_error(&metadata_only, &format!("SELECT * FROM {LOGS_TABLE}"))
            .await;
        assert_eq!(
            star, "WYRD_VALA_403_PAYLOAD_FORBIDDEN",
            "`SELECT *` cannot smuggle protected columns past the payload gate"
        );

        journey.shutdown().await;
    }
}
