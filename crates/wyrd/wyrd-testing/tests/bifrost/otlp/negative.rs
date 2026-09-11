//! Negative OTLP journeys: exact partial success and whole-request refusal.
//!
//! Every case here drives the same public transports the fidelity journeys use
//! and then reads through the public canonical SQL contract, so a rejection is
//! only believed once the accepted siblings are queryable and the rejected ones
//! are provably absent.

use arrow::array::{Int32Array, StringArray};
use arrow::record_batch::RecordBatch;
use wyrd_tonic::otlp::logs::v1::{ResourceLogs, ScopeLogs};
use wyrd_tonic::otlp::metrics::v1::{ResourceMetrics, ScopeMetrics};
use wyrd_tonic::otlp::trace::v1::{ResourceSpans, ScopeSpans};

use super::support::{self, RESOURCE_SCHEMA_URL, SCOPE_SCHEMA_URL, SpanIdentity, column};

/// Scope the mixed-validity and all-invalid trace exports are recorded under.
///
/// Each signal gets its own scope so one query can select exactly the rows a
/// case wrote without depending on the shared fidelity fixtures' instants.
const NEGATIVE_TRACE_SCOPE: &str = "wyrd.tests.otlp.negative.trace";
/// Scope the negative log exports are recorded under.
const NEGATIVE_LOG_SCOPE: &str = "wyrd.tests.otlp.negative.log";
/// Scope the negative metric exports are recorded under.
const NEGATIVE_METRIC_SCOPE: &str = "wyrd.tests.otlp.negative.metric";

/// Trace identity every negative span shares.
const NEGATIVE_TRACE_ID: [u8; 16] = [
    0x91, 0x92, 0x93, 0x94, 0x95, 0x96, 0x97, 0x98, 0x99, 0x9a, 0x9b, 0x9c, 0x9d, 0x9e, 0x9f, 0xa0,
];

/// Per-position `trace_state` values the negative spans are distinguished by.
///
/// `trace_state` is an ordinary canonical meta column the caller controls, so
/// it discriminates rows without colliding with the field a case invalidates.
const SPAN_MARKERS: [&str; 3] = ["negative=0", "negative=1", "negative=2"];

/// Per-position `event_name` values the negative log records are known by.
const LOG_MARKERS: [&str; 3] = ["negative.log.0", "negative.log.1", "negative.log.2"];

/// The stable reason an empty span name is rejected with.
const SPAN_REJECTION: &str = "span name is empty or exceeds the accepted length";
/// The stable reason an over-long log severity text is rejected with.
const LOG_REJECTION: &str = "log severity_text exceeds the accepted length";
/// The stable reason an empty metric name is rejected with.
const METRIC_REJECTION: &str = "metric name is empty or exceeds the accepted length";

/// A severity text past the projection's accepted length.
fn over_long_severity_text() -> String {
    "W".repeat(128)
}

/// Builds one negative trace export whose spans are invalid at `invalid`.
///
/// Every span is otherwise the maximal fixture span, so the only reason a
/// sibling can be rejected is the name this builder deliberately empties.
fn negative_resource_spans(anchor: i64, invalid: &[usize]) -> Vec<ResourceSpans> {
    let spans = SPAN_MARKERS
        .iter()
        .enumerate()
        .map(|(index, marker)| {
            let mut span = support::maximal_span(
                anchor,
                SpanIdentity {
                    trace_id: NEGATIVE_TRACE_ID,
                    span_id: [
                        0xb0,
                        0xb1,
                        0xb2,
                        0xb3,
                        0xb4,
                        0xb5,
                        0xb6,
                        u8::try_from(index).expect("the fixture position fits one byte"),
                    ],
                },
            );
            span.trace_state = (*marker).to_owned();
            if invalid.contains(&index) {
                span.name = String::new();
            }
            span
        })
        .collect();
    vec![ResourceSpans {
        resource: Some(support::resource()),
        scope_spans: vec![ScopeSpans {
            scope: Some(support::signal_scope(NEGATIVE_TRACE_SCOPE)),
            spans,
            schema_url: SCOPE_SCHEMA_URL.to_owned(),
        }],
        schema_url: RESOURCE_SCHEMA_URL.to_owned(),
    }]
}

/// Builds one negative log export whose records are invalid at `invalid`.
fn negative_resource_logs(anchor: i64, invalid: &[usize]) -> Vec<ResourceLogs> {
    let log_records = LOG_MARKERS
        .iter()
        .enumerate()
        .map(|(index, marker)| {
            let mut record = support::maximal_log_record(anchor);
            record.event_name = (*marker).to_owned();
            if invalid.contains(&index) {
                record.severity_text = over_long_severity_text();
            }
            record
        })
        .collect();
    vec![ResourceLogs {
        resource: Some(support::resource()),
        scope_logs: vec![ScopeLogs {
            scope: Some(support::signal_scope(NEGATIVE_LOG_SCOPE)),
            log_records,
            schema_url: SCOPE_SCHEMA_URL.to_owned(),
        }],
        schema_url: RESOURCE_SCHEMA_URL.to_owned(),
    }]
}

/// Builds one negative metric export whose metrics are invalid at `invalid`.
///
/// The first three maximal metrics each carry exactly one data point, so a
/// rejected metric is also exactly one rejected point.
fn negative_resource_metrics(anchor: i64, invalid: &[usize]) -> Vec<ResourceMetrics> {
    let mut metrics = support::maximal_metrics(anchor);
    metrics.truncate(3);
    for index in invalid {
        metrics[*index].name = String::new();
    }
    vec![ResourceMetrics {
        resource: Some(support::resource()),
        scope_metrics: vec![ScopeMetrics {
            scope: Some(support::signal_scope(NEGATIVE_METRIC_SCOPE)),
            metrics,
            schema_url: SCOPE_SCHEMA_URL.to_owned(),
        }],
        schema_url: RESOURCE_SCHEMA_URL.to_owned(),
    }]
}

/// A valid run correlation the attribution journey stamps its second export with.
const ATTRIBUTION_RUN: &str = "01890f28-7c4a-7cc3-98e7-4f4a3c2d1b11";

/// Builds the wholly valid negative spans carrying `wyrd.run_id` correlation.
///
/// The spans are otherwise identical to `negative_resource_spans(anchor, &[])`,
/// so the only thing that can distinguish their stored rows from that export's
/// is the accepted run correlation the Gate must bind into batch identity.
fn correlated_resource_spans(anchor: i64) -> Vec<ResourceSpans> {
    let mut resource_spans = negative_resource_spans(anchor, &[]);
    for span in &mut resource_spans[0].scope_spans[0].spans {
        span.attributes
            .push(support::string_attribute("wyrd.run_id", ATTRIBUTION_RUN));
    }
    resource_spans
}

/// Reads the ordered `(discriminator, wyrd_row_ordinal)` pairs of one query.
///
/// The caller orders the query by `wyrd_row_ordinal`, so the returned order is
/// the stored order and both the accepted subset's relative order and its
/// ordinal contiguity are readable from one projection.
///
/// # Panics
///
/// Panics when either column is missing or is not the canonical type.
fn ordered_rows(batches: &[RecordBatch], discriminator: &str) -> Vec<(String, i32)> {
    let mut rows = Vec::new();
    for batch in batches {
        let markers = column::<StringArray>(batch, discriminator);
        let ordinals = column::<Int32Array>(batch, "wyrd_row_ordinal");
        for index in 0..batch.num_rows() {
            rows.push((markers.value(index).to_owned(), ordinals.value(index)));
        }
    }
    rows
}

/// Tests that need Postgres, a bound server, and the publication boundary.
mod pg_tests {
    use std::collections::{BTreeMap, BTreeSet};

    use arrow::array::{Array as _, StringArray};
    use arrow::record_batch::RecordBatch;
    use reqwest::StatusCode;
    use wyrd_runtime::Permission;
    use wyrd_tonic::otlp::logs_service::{ExportLogsServiceRequest, ExportLogsServiceResponse};
    use wyrd_tonic::otlp::metrics_service::{
        ExportMetricsServiceRequest, ExportMetricsServiceResponse,
    };
    use wyrd_tonic::otlp::trace_service::ExportTraceServiceRequest;
    use wyrd_tonic::prost::Message as _;

    use super::super::support::{self, LOGS_TABLE, METRICS_TABLE, OtlpJourney, SPANS_TABLE};
    use super::super::trace_export::export_traces_over_grpc;
    use super::super::trace_export::export_traces_over_grpc_as;
    use super::super::trace_export_http::{HttpEncoding, post_otlp, post_otlp_raw};
    use super::{
        ATTRIBUTION_RUN, LOG_MARKERS, LOG_REJECTION, METRIC_REJECTION, NEGATIVE_LOG_SCOPE,
        NEGATIVE_METRIC_SCOPE, NEGATIVE_TRACE_SCOPE, SPAN_MARKERS, SPAN_REJECTION,
        correlated_resource_spans, negative_resource_logs, negative_resource_metrics,
        negative_resource_spans, ordered_rows,
    };

    /// A mixed request commits its complete siblings and reports exactly one.
    ///
    /// Each signal is exported on a different transport — traces over gRPC,
    /// logs over OTLP/HTTP protobuf-JSON, metrics over OTLP/HTTP protobuf — so
    /// the partial-success contract is proven on every encoding the collector
    /// exposes rather than on one. For each signal the case asserts the exact
    /// rejected count and the stable first reason, then reads the table back
    /// and proves that only the complete siblings are stored, in their request
    /// order, with `wyrd_row_ordinal` contiguous from zero over the accepted
    /// subset alone.
    ///
    /// The request is then replayed byte-identically, which is what an
    /// at-least-once OTLP exporter does after a lost acknowledgement. The
    /// replay must report the same rejected count and the same reason, and it
    /// must not duplicate anything: the Gate derives one `wyrd_batch_id` from
    /// the accepted canonical rows, so Scribe's durable fence recognizes the
    /// repeat and every accepted sibling stays queryable exactly once while
    /// every rejected sibling stays absent.
    ///
    /// # Panics
    ///
    /// Panics when a transport is refused, when a partial success reports a
    /// different count or reason, or when the stored rows are not exactly the
    /// accepted siblings in order.
    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "requires the Postgres-backed Bifrost journey lane"]
    async fn mixed_otlp_requests_commit_only_complete_siblings_and_exact_partial_success() {
        let journey = OtlpJourney::start().await;
        let anchor = support::anchor_nanos();

        for attempt in 0..2 {
            let trace_partial =
                export_traces_over_grpc(&journey, negative_resource_spans(anchor, &[1]))
                    .await
                    .expect("a mixed trace export reports partial success");
            assert_eq!(
                trace_partial.rejected_spans, 1,
                "attempt {attempt}: exactly the one incomplete span is rejected"
            );
            assert_eq!(
                trace_partial.error_message, SPAN_REJECTION,
                "attempt {attempt}: the rejection reason is the projection's stable reason"
            );

            let request = ExportLogsServiceRequest {
                resource_logs: negative_resource_logs(anchor, &[1]),
            };
            let body = post_otlp(
                &journey,
                "/v1/logs",
                HttpEncoding::Json,
                HttpEncoding::Json.encode(&request),
            )
            .await;
            let response: ExportLogsServiceResponse = HttpEncoding::Json.decode(&body);
            let log_partial = response
                .partial_success
                .expect("a mixed log export reports partial success");
            assert_eq!(
                log_partial.rejected_log_records, 1,
                "attempt {attempt}: exactly the one incomplete log record is rejected"
            );
            assert_eq!(
                log_partial.error_message, LOG_REJECTION,
                "attempt {attempt}: the rejection reason is the projection's stable reason"
            );

            let request = ExportMetricsServiceRequest {
                resource_metrics: negative_resource_metrics(anchor, &[1]),
            };
            let body = post_otlp(
                &journey,
                "/v1/metrics",
                HttpEncoding::Protobuf,
                HttpEncoding::Protobuf.encode(&request),
            )
            .await;
            let response: ExportMetricsServiceResponse = HttpEncoding::Protobuf.decode(&body);
            let metric_partial = response
                .partial_success
                .expect("a mixed metric export reports partial success");
            assert_eq!(
                metric_partial.rejected_data_points, 1,
                "attempt {attempt}: exactly the one incomplete metric's point is rejected"
            );
            assert_eq!(
                metric_partial.error_message, METRIC_REJECTION,
                "attempt {attempt}: the rejection reason is the projection's stable reason"
            );
        }

        journey.publish().await;

        let spans = journey
            .query(&format!(
                "SELECT trace_state, wyrd_row_ordinal FROM {SPANS_TABLE} \
                 WHERE scope_name = '{NEGATIVE_TRACE_SCOPE}' ORDER BY wyrd_row_ordinal"
            ))
            .await;
        assert_accepted_subset(
            &ordered_rows(&spans, "trace_state"),
            &[SPAN_MARKERS[0], SPAN_MARKERS[2]],
            SPAN_MARKERS[1],
        );

        let logs = journey
            .query(&format!(
                "SELECT event_name, wyrd_row_ordinal FROM {LOGS_TABLE} \
                 WHERE scope_name = '{NEGATIVE_LOG_SCOPE}' ORDER BY wyrd_row_ordinal"
            ))
            .await;
        assert_accepted_subset(
            &ordered_rows(&logs, "event_name"),
            &[LOG_MARKERS[0], LOG_MARKERS[2]],
            LOG_MARKERS[1],
        );

        let metrics = journey
            .query(&format!(
                "SELECT metric_name, wyrd_row_ordinal FROM {METRICS_TABLE} \
                 WHERE scope_name = '{NEGATIVE_METRIC_SCOPE}' ORDER BY wyrd_row_ordinal"
            ))
            .await;
        assert_accepted_subset(
            &ordered_rows(&metrics, "metric_name"),
            &[support::GAUGE_INT_METRIC, support::SUM_INT_METRIC],
            "",
        );

        journey.shutdown().await;
    }

    /// One publisher's replay is exactly once; two publishers are two row sets.
    ///
    /// Retry suppression derives `wyrd_batch_id` from the accepted rows rather
    /// than from transport metadata, so it must be scoped to the authenticated
    /// publisher and the accepted correlation attribution it carries. This
    /// case exports the same wholly valid spans three ways: twice as the
    /// journey's own principal (the at-least-once replay), once as a second
    /// authorized principal in the same tenant, and once as the first
    /// principal again under a different accepted `wyrd.run_id`. The replay
    /// must converge on one fence and store one row set, while each of the
    /// other two must reach its own fence and store its own rows with its own
    /// `principal_id` and `run_id` — otherwise an acknowledgement would have
    /// claimed a row that was never stored under its own attribution.
    ///
    /// # Panics
    ///
    /// Panics when an export is refused, when the replay duplicates or drops a
    /// row, or when a distinct principal or run correlation does not produce
    /// its own complete, correctly attributed row set.
    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "requires the Postgres-backed Bifrost journey lane"]
    async fn identical_exports_from_two_principals_each_keep_their_own_attribution() {
        let journey = OtlpJourney::start().await;
        let anchor = support::anchor_nanos();

        for attempt in 0..2 {
            let partial = export_traces_over_grpc(&journey, negative_resource_spans(anchor, &[]))
                .await
                .and_then(|partial| (partial.rejected_spans != 0).then_some(partial));
            assert!(
                partial.is_none(),
                "attempt {attempt}: a wholly valid export rejects no span"
            );
        }

        let second = journey
            .token_with_permissions(
                "otlp_attribution_writer",
                &[Permission::bifrost_record_write()],
            )
            .await;
        export_traces_over_grpc_as(&journey, &second, negative_resource_spans(anchor, &[])).await;
        export_traces_over_grpc(&journey, correlated_resource_spans(anchor)).await;

        journey.publish().await;

        let rows = journey
            .query(&format!(
                "SELECT trace_state, principal_id, run_id FROM {SPANS_TABLE} \
                 WHERE scope_name = '{NEGATIVE_TRACE_SCOPE}'"
            ))
            .await;
        let mut attributed: BTreeMap<(String, String), Vec<String>> = BTreeMap::new();
        for batch in &rows {
            let markers = support::column::<StringArray>(batch, "trace_state");
            let principals = support::column::<StringArray>(batch, "principal_id");
            let runs = support::column::<StringArray>(batch, "run_id");
            for index in 0..batch.num_rows() {
                let run = if runs.is_null(index) {
                    String::new()
                } else {
                    runs.value(index).to_owned()
                };
                attributed
                    .entry((principals.value(index).to_owned(), run))
                    .or_default()
                    .push(markers.value(index).to_owned());
            }
        }

        assert_eq!(
            attributed.len(),
            3,
            "the replay collapses onto one fence while the second principal and the \
             differently correlated export each keep their own: {attributed:?}"
        );
        let principals: BTreeSet<&String> = attributed.keys().map(|(id, _)| id).collect();
        assert_eq!(
            principals.len(),
            2,
            "two authorized publishers keep two distinct principal attributions"
        );
        let runs: BTreeSet<&String> = attributed.keys().map(|(_, run)| run).collect();
        assert!(
            runs.contains(&String::new()) && runs.contains(&ATTRIBUTION_RUN.to_owned()),
            "the uncorrelated and the run-correlated exports are stored apart: {runs:?}"
        );
        for (attribution, mut markers) in attributed {
            markers.sort();
            assert_eq!(
                markers,
                SPAN_MARKERS.map(str::to_owned).to_vec(),
                "{attribution:?} stores every accepted sibling exactly once"
            );
        }

        journey.shutdown().await;
    }

    /// Asserts the stored rows are exactly `accepted`, once each, in order.
    ///
    /// The replayed export carries the same accepted canonical rows, so the
    /// Gate derives the same batch identity and the durable fence suppresses
    /// the repeat: each accepted sibling appears exactly once under the one
    /// ordinal its position in the request earns. A rejected sibling must
    /// never appear and must never consume an ordinal, which is what makes the
    /// run contiguous from zero over the accepted subset alone.
    ///
    /// # Panics
    ///
    /// Panics when a rejected marker is present, when an accepted sibling is
    /// duplicated, or when the stored markers and ordinals are not exactly the
    /// accepted subset's.
    fn assert_accepted_subset(rows: &[(String, i32)], accepted: &[&str], rejected: &str) {
        assert!(
            !rows.iter().any(|(marker, _)| marker == rejected),
            "the rejected sibling `{rejected}` is never stored"
        );
        let expected: Vec<(String, i32)> = accepted
            .iter()
            .enumerate()
            .map(|(index, marker)| {
                let ordinal = i32::try_from(index).expect("the fixture position fits i32");
                ((*marker).to_owned(), ordinal)
            })
            .collect();
        assert_eq!(
            rows, expected,
            "a replayed export stores its accepted siblings exactly once, in request \
             order, with `wyrd_row_ordinal` contiguous from zero over that subset alone"
        );
    }

    /// All-invalid and request-wide failures leave nothing queryable.
    ///
    /// The first half proves the all-invalid nonempty case on gRPC and on
    /// OTLP/HTTP protobuf-JSON: the signal-specific OTLP response reports the
    /// exact rejected count and the stable reason, and no row is created. The
    /// second half proves that a malformed transport body, a missing
    /// credential, a principal without the ingest permission, and an injected
    /// durable WAL refusal each fail the whole request through the public error
    /// contract, carry no `partial_success`, and commit nothing.
    ///
    /// # Panics
    ///
    /// Panics when a refusal reports a different public error, when any
    /// refusal is accepted, or when any row becomes queryable.
    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "requires the Postgres-backed Bifrost journey lane"]
    async fn all_invalid_and_request_wide_failures_leave_no_queryable_rows() {
        let journey = OtlpJourney::start().await;
        let anchor = support::anchor_nanos();
        let all = [0, 1, 2];

        let trace_partial =
            export_traces_over_grpc(&journey, negative_resource_spans(anchor, &all))
                .await
                .expect("an all-invalid trace export still answers with the OTLP response");
        assert_eq!(
            trace_partial.rejected_spans, 3,
            "an all-invalid export reports the exact rejected count"
        );
        assert_eq!(trace_partial.error_message, SPAN_REJECTION);

        let request = ExportLogsServiceRequest {
            resource_logs: negative_resource_logs(anchor, &all),
        };
        let body = post_otlp(
            &journey,
            "/v1/logs",
            HttpEncoding::Json,
            HttpEncoding::Json.encode(&request),
        )
        .await;
        let response: ExportLogsServiceResponse = HttpEncoding::Json.decode(&body);
        let log_partial = response
            .partial_success
            .expect("an all-invalid log export still answers with the OTLP response");
        assert_eq!(log_partial.rejected_log_records, 3);
        assert_eq!(log_partial.error_message, LOG_REJECTION);

        let valid = ExportTraceServiceRequest {
            resource_spans: negative_resource_spans(anchor, &[]),
        };
        let valid_protobuf = valid.encode_to_vec();

        assert_refused(
            &journey,
            "/v1/traces",
            HttpEncoding::Protobuf,
            b"not an OTLP protobuf message at all".to_vec(),
            Some(journey.token().to_owned()),
            StatusCode::BAD_REQUEST,
            "WYRD_VALA_400_OTLP_REQUEST_MALFORMED",
        )
        .await;

        assert_refused(
            &journey,
            "/v1/traces",
            HttpEncoding::Json,
            HttpEncoding::Json.encode(&valid),
            None,
            StatusCode::UNAUTHORIZED,
            "",
        )
        .await;

        let reader = journey
            .token_with_permissions("otlp_negative_reader", &[Permission::bifrost_query_read()])
            .await;
        assert_refused(
            &journey,
            "/v1/traces",
            HttpEncoding::Protobuf,
            valid_protobuf.clone(),
            Some(reader),
            StatusCode::FORBIDDEN,
            "WYRD_PERMISSION_403_DENIED_RBAC",
        )
        .await;

        journey
            .server()
            .trip_bifrost_wal_disk_full_for_test()
            .expect("the journey injects a durable WAL refusal");
        assert_refused(
            &journey,
            "/v1/traces",
            HttpEncoding::Protobuf,
            valid_protobuf,
            Some(journey.token().to_owned()),
            StatusCode::INSUFFICIENT_STORAGE,
            "WYRD_VALA_507_WAL_DISK_FULL",
        )
        .await;
        journey
            .server()
            .clear_bifrost_wal_disk_full_injection_for_test()
            .expect("the injected WAL refusal is released");

        journey.publish().await;
        for (table, scope) in [
            (SPANS_TABLE, NEGATIVE_TRACE_SCOPE),
            (LOGS_TABLE, NEGATIVE_LOG_SCOPE),
        ] {
            assert_no_rows(&journey, table, scope).await;
        }

        journey.shutdown().await;
    }

    /// Asserts `table` carries no row written under `scope`.
    ///
    /// A refused request may leave its table unmaterialized entirely, which is
    /// the same proof as an empty result: no accepted record ever existed. The
    /// two are therefore both accepted here, and any other refusal is not.
    ///
    /// # Panics
    ///
    /// Panics when a row is queryable or the query fails for any reason other
    /// than the table never having been created.
    async fn assert_no_rows(journey: &OtlpJourney, table: &str, scope: &str) {
        let sql = format!("SELECT wyrd_row_ordinal FROM {table} WHERE scope_name = '{scope}'");
        match journey.try_query(&sql).await {
            Ok(batches) => {
                let stored: usize = batches.iter().map(RecordBatch::num_rows).sum();
                assert_eq!(stored, 0, "{table} carries no row from a refused request");
            }
            Err(code) => assert_eq!(
                code, "WYRD_VALA_404_BIFROST_TABLE_NOT_FOUND",
                "{table} was never materialized, or was refused for another reason"
            ),
        }
    }

    /// Posts one export expected to be refused whole and asserts its identity.
    ///
    /// A refused export must never carry an OTLP body, so the response is
    /// checked to be the problem document the public error contract defines
    /// rather than a `partial_success` the caller could mistake for progress.
    /// `code` is empty when the refusal is owned by the transport's own
    /// authentication shell rather than by the Gate error catalog.
    ///
    /// # Panics
    ///
    /// Panics when the collector accepts the export or answers with a
    /// different status or public error code.
    async fn assert_refused(
        journey: &OtlpJourney,
        path: &str,
        encoding: HttpEncoding,
        body: Vec<u8>,
        token: Option<String>,
        expected_status: StatusCode,
        expected_code: &str,
    ) {
        let (status, response) =
            post_otlp_raw(journey, path, encoding, body, token.as_deref()).await;
        let text = String::from_utf8_lossy(&response);
        assert_eq!(
            status, expected_status,
            "the collector refuses the whole request: {text}"
        );
        assert!(
            !text.contains("partialSuccess") && !text.contains("partial_success"),
            "a whole-request refusal carries no partial success: {text}"
        );
        if !expected_code.is_empty() {
            let problem: serde_json::Value = serde_json::from_slice(&response)
                .expect("a public refusal answers with the problem document");
            assert_eq!(
                problem.get("code").and_then(serde_json::Value::as_str),
                Some(expected_code),
                "the refusal keeps its public error code: {text}"
            );
        }
    }
}
