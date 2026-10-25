//! Negative OTLP journeys: exact partial success and whole-request refusal.
//!
//! Every case here drives the same public transports the fidelity journeys use
//! and then reads through the public canonical SQL contract, so a rejection is
//! only believed once the accepted siblings are queryable and the rejected ones
//! are provably absent.

use arrow::array::StringArray;
use arrow::record_batch::RecordBatch;
use wyrd_tonic::otlp::common::v1::KeyValue;
use wyrd_tonic::otlp::logs::v1::{ResourceLogs, ScopeLogs};
use wyrd_tonic::otlp::metrics::v1::{ResourceMetrics, ScopeMetrics, metric};
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

/// Scope the expanded-ceiling trace exports are recorded under.
const FAN_OUT_TRACE_SCOPE: &str = "wyrd.tests.otlp.negative.fan_out";

/// Bytes of the single resource attribute projection repeats onto every span.
const FAN_OUT_ATTRIBUTE_BYTES: usize = 64 * 1024;

/// Builds one trace export whose large resource attribute fans out per span.
///
/// The attribute is sent once on the wire but projected onto every span row,
/// so the canonical output grows with `spans` while the wire stays small.
fn fan_out_resource_spans(anchor: i64, spans: u16) -> Vec<ResourceSpans> {
    let mut resource = support::resource();
    resource.attributes.push(support::string_attribute(
        "wyrd.test.padding",
        &"p".repeat(FAN_OUT_ATTRIBUTE_BYTES),
    ));
    let spans = (0..spans)
        .map(|index| {
            let [high, low] = index.to_be_bytes();
            support::maximal_span(
                anchor,
                SpanIdentity {
                    trace_id: NEGATIVE_TRACE_ID,
                    span_id: [0xc0, 0, 0, 0, 0, 0, high, low],
                },
            )
        })
        .collect();
    vec![ResourceSpans {
        resource: Some(resource),
        scope_spans: vec![ScopeSpans {
            scope: Some(support::signal_scope(FAN_OUT_TRACE_SCOPE)),
            spans,
            schema_url: SCOPE_SCHEMA_URL.to_owned(),
        }],
        schema_url: RESOURCE_SCHEMA_URL.to_owned(),
    }]
}

/// A severity text past the projection's accepted length.
fn over_long_severity_text() -> String {
    "W".repeat(128)
}

/// Scope prefix of the partition-edit replay, one scope per span.
const GROUPING_TRACE_SCOPE: &str = "wyrd.tests.otlp.negative.grouping";

/// Builds one trace export of `scopes` spans in one hour, each under its own
/// `GROUPING_TRACE_SCOPE.<salt>.<index>` scope.
///
/// A different `salt` is a different export with its own content and
/// therefore its own derived batch id.
fn scoped_resource_spans(anchor: i64, scopes: u8, salt: u8) -> Vec<ResourceSpans> {
    let scope_spans = (0..scopes)
        .map(|index| ScopeSpans {
            scope: Some(support::signal_scope(&format!(
                "{GROUPING_TRACE_SCOPE}.{salt}.{index}"
            ))),
            spans: vec![support::maximal_span(
                anchor,
                SpanIdentity {
                    trace_id: NEGATIVE_TRACE_ID,
                    span_id: [0xd0, salt, 0, 0, 0, 0, 0, index],
                },
            )],
            schema_url: SCOPE_SCHEMA_URL.to_owned(),
        })
        .collect();
    vec![ResourceSpans {
        resource: Some(support::resource()),
        scope_spans,
        schema_url: RESOURCE_SCHEMA_URL.to_owned(),
    }]
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
            (*marker).clone_into(&mut span.trace_state);
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
            (*marker).clone_into(&mut record.event_name);
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

/// Appends `cards[i]` as the final `wyrd.card_uid` of the `i`th record.
///
/// A `None` position names no Card, so one export mixes attributable,
/// unattributable, and uncorrelated siblings on every signal alike.
fn attribute_cards<'record>(
    records: impl IntoIterator<Item = &'record mut Vec<KeyValue>>,
    cards: [Option<&str>; 3],
) {
    for (attributes, card) in records.into_iter().zip(cards) {
        if let Some(card) = card {
            attributes.push(support::string_attribute("wyrd.card_uid", card));
        }
    }
}

/// Builds the negative trace export with each span naming its own `card_uid`.
///
/// # Panics
///
/// Panics when the negative trace fixture has no first resource or no first
/// scope to attribute.
fn card_attributed_resource_spans(anchor: i64, cards: [Option<&str>; 3]) -> Vec<ResourceSpans> {
    let mut resource_spans = negative_resource_spans(anchor, &[]);
    attribute_cards(
        resource_spans[0].scope_spans[0]
            .spans
            .iter_mut()
            .map(|span| &mut span.attributes),
        cards,
    );
    resource_spans
}

/// Builds the negative log export with each record naming its own `card_uid`.
///
/// # Panics
///
/// Panics when the negative log fixture has no first resource or no first
/// scope to attribute.
fn card_attributed_resource_logs(anchor: i64, cards: [Option<&str>; 3]) -> Vec<ResourceLogs> {
    let mut resource_logs = negative_resource_logs(anchor, &[]);
    attribute_cards(
        resource_logs[0].scope_logs[0]
            .log_records
            .iter_mut()
            .map(|record| &mut record.attributes),
        cards,
    );
    resource_logs
}

/// Builds the negative metric export with each point naming its own `card_uid`.
///
/// # Panics
///
/// Panics when a negative metric is not the single-point gauge or sum the
/// fixture defines.
fn card_attributed_resource_metrics(anchor: i64, cards: [Option<&str>; 3]) -> Vec<ResourceMetrics> {
    let mut resource_metrics = negative_resource_metrics(anchor, &[]);
    attribute_cards(
        resource_metrics[0].scope_metrics[0]
            .metrics
            .iter_mut()
            .map(|metric| match &mut metric.data {
                Some(metric::Data::Gauge(gauge)) => &mut gauge.data_points[0].attributes,
                Some(metric::Data::Sum(sum)) => &mut sum.data_points[0].attributes,
                other => panic!("a negative metric is a single-point gauge or sum: {other:?}"),
            }),
        cards,
    );
    resource_metrics
}

/// Reads the sorted `discriminator` markers of one query.
///
/// Sorting makes the result the stored marker multiset, independent of which
/// file or tier answered, so duplicates and absences are both visible.
///
/// # Panics
///
/// Panics when the column is missing or is not the canonical type.
fn sorted_markers(batches: &[RecordBatch], discriminator: &str) -> Vec<String> {
    let mut markers = Vec::new();
    for batch in batches {
        let column = column::<StringArray>(batch, discriminator);
        markers.extend((0..batch.num_rows()).map(|index| column.value(index).to_owned()));
    }
    markers.sort_unstable();
    markers
}

/// Tests that need Postgres, a bound server, and the publication boundary.
mod pg_tests {
    use std::collections::{BTreeMap, BTreeSet};

    use arrow::array::{Array as _, StringArray};
    use arrow::record_batch::RecordBatch;
    use reqwest::StatusCode;
    use wyrd_runtime::Permission;
    use wyrd_semver::VersionBlock;
    use wyrd_spec::auth::TokenAudience;
    use wyrd_spec::envelope::CardKind;
    use wyrd_spec::ids::{CardName, CardUid};
    use wyrd_spec::reference::CardRef;
    use wyrd_tonic::otlp::logs_service::{ExportLogsServiceRequest, ExportLogsServiceResponse};
    use wyrd_tonic::otlp::metrics_service::{
        ExportMetricsServiceRequest, ExportMetricsServiceResponse,
    };
    use wyrd_tonic::otlp::trace_service::{ExportTraceServiceRequest, ExportTraceServiceResponse};
    use wyrd_tonic::prost::Message as _;

    use super::super::support::{self, LOGS_TABLE, METRICS_TABLE, OtlpJourney, SPANS_TABLE};
    use super::super::trace_export::export_traces_over_grpc;
    use super::super::trace_export::{export_traces_over_grpc_as, export_traces_over_grpc_with};
    use super::super::trace_export_http::{
        HttpEncoding, post_otlp, post_otlp_raw, post_otlp_with_headers,
    };
    use super::{
        ATTRIBUTION_RUN, FAN_OUT_TRACE_SCOPE, GROUPING_TRACE_SCOPE, LOG_MARKERS, LOG_REJECTION,
        METRIC_REJECTION, NEGATIVE_LOG_SCOPE, NEGATIVE_METRIC_SCOPE, NEGATIVE_TRACE_SCOPE,
        SPAN_MARKERS, SPAN_REJECTION, card_attributed_resource_logs,
        card_attributed_resource_metrics, card_attributed_resource_spans,
        correlated_resource_spans, fan_out_resource_spans, negative_resource_logs,
        negative_resource_metrics, negative_resource_spans, scoped_resource_spans, sorted_markers,
    };
    use wyrd_testing::Bootstrap;

    /// A mixed request commits its complete siblings and reports exactly one.
    ///
    /// Each signal is exported on a different transport — traces over gRPC,
    /// logs over OTLP/HTTP protobuf-JSON, metrics over OTLP/HTTP protobuf — so
    /// the partial-success contract is proven on every encoding the collector
    /// exposes rather than on one. For each signal the case asserts the exact
    /// rejected count and the stable first reason, then reads the table back
    /// and proves that exactly the complete siblings are stored, once each.
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
    /// accepted siblings.
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
                "SELECT trace_state FROM {SPANS_TABLE} WHERE scope_name = '{NEGATIVE_TRACE_SCOPE}'"
            ))
            .await;
        assert_accepted_subset(
            &sorted_markers(&spans, "trace_state"),
            &[SPAN_MARKERS[0], SPAN_MARKERS[2]],
            SPAN_MARKERS[1],
        );

        let logs = journey
            .query(&format!(
                "SELECT event_name FROM {LOGS_TABLE} WHERE scope_name = '{NEGATIVE_LOG_SCOPE}'"
            ))
            .await;
        assert_accepted_subset(
            &sorted_markers(&logs, "event_name"),
            &[LOG_MARKERS[0], LOG_MARKERS[2]],
            LOG_MARKERS[1],
        );

        let metrics = journey
            .query(&format!(
                "SELECT metric_name FROM {METRICS_TABLE} WHERE scope_name = '{NEGATIVE_METRIC_SCOPE}'"
            ))
            .await;
        assert_accepted_subset(
            &sorted_markers(&metrics, "metric_name"),
            &[support::GAUGE_INT_METRIC, support::SUM_INT_METRIC],
            "",
        );

        journey.shutdown().await;
    }

    /// A replay across a definition edit is recovered under its accepting version.
    ///
    /// The Gate states no table version for canonical OTLP rows, so an
    /// exporter replaying after a Bloom-only edit reaches Scribe under a newer
    /// current version than the one that accepted it. Warm, the pod's committed
    /// index names that version; after a restart only the durable fence does.
    /// Both replays must acknowledge without storing a second row set.
    ///
    /// # Panics
    ///
    /// Panics when an export or replay is refused, or the spans are not stored
    /// exactly once.
    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "requires the Postgres-backed Bifrost journey lane"]
    async fn otlp_replay_across_a_definition_edit_is_acknowledged_once() {
        let data_root = tempfile::tempdir().expect("durable Bifrost data root");
        let builder = || {
            wyrd_testing::WyrdTestServer::builder()
                .with_durable_bifrost_data_root(data_root.path().to_path_buf())
        };
        let journey = OtlpJourney::start_on(builder()).await;
        let spans = negative_resource_spans(support::anchor_nanos(), &[]);
        let accepted = |partial: Option<_>| assert!(partial.is_none(), "every span is accepted");
        accepted(export_traces_over_grpc(&journey, spans.clone()).await);
        let tenant = journey.server().data_tenant_id();
        journey
            .server()
            .commit_bloom_revision(
                tenant,
                &vala_bifrost_redux::catalog::TableRef::new(
                    vala_bifrost_redux::namespaces::BifrostNamespace::Traces,
                    "spans",
                ),
                &["scope_name", "trace_state"],
            )
            .await;
        accepted(export_traces_over_grpc(&journey, spans.clone()).await);
        let journey = journey.restart(builder()).await;
        accepted(export_traces_over_grpc(&journey, spans).await);
        journey.publish().await;

        let stored = journey
            .query(&format!(
                "SELECT trace_state FROM {SPANS_TABLE} WHERE scope_name = '{NEGATIVE_TRACE_SCOPE}'"
            ))
            .await;
        assert_accepted_subset(&sorted_markers(&stored, "trace_state"), &SPAN_MARKERS, "");
        journey.shutdown().await;
    }

    /// A replay the current partition layout would refuse is acknowledged
    /// under the layout that accepted it.
    ///
    /// Thirty-three spans in one hour, each under its own scope, are accepted
    /// and published while `spans` partitions hourly only. An identity
    /// partition on `scope_name` then makes current planning see thirty-three
    /// tuples, past the request-wide limit. The Gate states no version, so the
    /// replay must still acknowledge under the original layout, leaving the
    /// first acceptance's rows, receipt instant, and request exactly once. A
    /// changed export no fence accepted keeps the current layout's refusal.
    /// Built-in provisioning still pins the canonical partition spec at
    /// startup, so the cold replay is proven by the Scribe tier-2
    /// accepted-retry test rather than by a restart here.
    ///
    /// # Panics
    ///
    /// Panics when an export or replay is refused, the stored rows differ
    /// from the first acceptance, or the changed export is admitted.
    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "requires the Postgres-backed Bifrost journey lane"]
    async fn otlp_replay_across_a_partition_edit_is_acknowledged_once() {
        let journey = OtlpJourney::start().await;
        let anchor = support::anchor_nanos();
        let spans = scoped_resource_spans(anchor, 33, 0);
        let accepted = |partial: Option<_>| assert!(partial.is_none(), "every span is accepted");
        let stored = async |journey: &OtlpJourney| {
            journey.publish().await;
            let batches = journey
                .query(&format!(
                    "SELECT scope_name, wyrd_ingested_at, wyrd_request_id FROM {SPANS_TABLE} \
                     WHERE scope_name LIKE '{GROUPING_TRACE_SCOPE}.%'"
                ))
                .await;
            let mut rows = Vec::new();
            for batch in &batches {
                let text = |name: &str| {
                    let column = arrow::compute::cast(
                        batch.column_by_name(name).expect("the column is selected"),
                        &arrow::datatypes::DataType::Utf8,
                    )
                    .expect("the column renders as text");
                    column
                        .as_any()
                        .downcast_ref::<StringArray>()
                        .expect("text is Utf8")
                        .clone()
                };
                let (scopes, receipts, requests) = (
                    text("scope_name"),
                    text("wyrd_ingested_at"),
                    text("wyrd_request_id"),
                );
                for index in 0..batch.num_rows() {
                    rows.push([
                        scopes.value(index).to_owned(),
                        receipts.value(index).to_owned(),
                        requests.value(index).to_owned(),
                    ]);
                }
            }
            rows.sort();
            rows
        };

        accepted(export_traces_over_grpc(&journey, spans.clone()).await);
        let original = stored(&journey).await;
        assert_eq!(original.len(), 33, "the export is stored once");
        journey
            .server()
            .commit_identity_partition_revision(
                journey.server().data_tenant_id(),
                &vala_bifrost_redux::catalog::TableRef::new(
                    vala_bifrost_redux::namespaces::BifrostNamespace::Traces,
                    "spans",
                ),
                "scope_name",
            )
            .await;
        accepted(export_traces_over_grpc(&journey, spans).await);
        assert_eq!(
            stored(&journey).await,
            original,
            "the replay is the original"
        );

        let changed = ExportTraceServiceRequest {
            resource_spans: scoped_resource_spans(anchor, 33, 1),
        };
        assert_refused(
            &journey,
            "/v1/traces",
            HttpEncoding::Protobuf,
            changed.encode_to_vec(),
            Some(journey.token().to_owned()),
            StatusCode::BAD_REQUEST,
            "WYRD_VALA_400_OTLP_REQUEST_MALFORMED",
        )
        .await;
        assert_eq!(
            stored(&journey).await,
            original,
            "the refusal stores nothing"
        );
        journey.shutdown().await;
    }

    /// One publisher's replay is exactly once; two publishers are two row sets.
    ///
    /// Retry suppression derives `wyrd_batch_id` from the accepted rows rather
    /// than from transport metadata, so it must be scoped to the verified
    /// publisher (subject and current actor) and the accepted correlation
    /// attribution it carries. This case exports the same wholly valid spans
    /// as: the journey's own principal over gRPC twice and OTLP/HTTP once (the
    /// at-least-once replay, across transports); a second authorized principal
    /// in the same tenant; the first principal again under a different
    /// accepted `wyrd.run_id`; and two actors, each under its own Card as the
    /// workload root a delegated actor must select, delegated by the first
    /// principal and each replaying over both transports. Every replay must
    /// converge on one fence and store one row set, while every distinct
    /// publisher pair or run correlation must reach its own fence and store its
    /// own rows with its own `sub_principal_id`, `act_principal_id`, and
    /// `run_id` — otherwise an acknowledgement would have claimed a row that
    /// was never stored under its own attribution.
    ///
    /// # Panics
    ///
    /// Panics when an export or delegation is refused, when a replay
    /// duplicates or drops a row, or when a distinct publisher pair or run
    /// correlation does not produce its own complete, correctly attributed
    /// row set.
    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "requires the Postgres-backed Bifrost journey lane"]
    async fn identical_exports_from_two_principals_each_keep_their_own_attribution() {
        let journey = OtlpJourney::start().await;
        let anchor = support::anchor_nanos();
        let export_over_http = async |token: &str| {
            let request = ExportTraceServiceRequest {
                resource_spans: negative_resource_spans(anchor, &[]),
            };
            let (status, body) = post_otlp_raw(
                &journey,
                "/v1/traces",
                HttpEncoding::Protobuf,
                HttpEncoding::Protobuf.encode(&request),
                Some(token),
            )
            .await;
            assert!(
                status.is_success(),
                "the collector accepts the export ({status}): {}",
                String::from_utf8_lossy(&body)
            );
        };

        for attempt in 0..2 {
            let partial = export_traces_over_grpc(&journey, negative_resource_spans(anchor, &[]))
                .await
                .and_then(|partial| (partial.rejected_spans != 0).then_some(partial));
            assert!(
                partial.is_none(),
                "attempt {attempt}: a wholly valid export rejects no span"
            );
        }
        export_over_http(journey.token()).await;

        let server = journey.server();
        let mut actors = Vec::new();
        for name in ["otlp_attribution_writer", "otlp_attribution_actor"] {
            let api_key = journey
                .api_key_with_permissions(name, &[Permission::bifrost_record_write()])
                .await;
            let rootless = server
                .exchange_api_key(&api_key)
                .await
                .expect("the actor exchanges its API key for a bearer");
            let rooted = server
                .exchange_api_key_for_workload(&api_key, &registered_uid(server, name).await)
                .await
                .expect("the actor selects its own Card as the workload root");
            actors.push((rootless, rooted));
        }
        let second = &actors[0].0;
        export_traces_over_grpc_as(&journey, second, negative_resource_spans(anchor, &[])).await;
        export_traces_over_grpc(&journey, correlated_resource_spans(anchor)).await;

        for (_, actor) in &actors {
            let delegated = server
                .delegate(journey.token(), actor, TokenAudience::Wyrd)
                .await
                .expect("the journey principal delegates to the actor");
            export_traces_over_grpc_as(&journey, &delegated, negative_resource_spans(anchor, &[]))
                .await;
            export_over_http(&delegated).await;
        }

        journey.publish().await;

        let rows = journey
            .query(&format!(
                "SELECT trace_state, sub_principal_id, act_principal_id, run_id \
                 FROM {SPANS_TABLE} WHERE scope_name = '{NEGATIVE_TRACE_SCOPE}'"
            ))
            .await;
        let mut attributed: BTreeMap<(String, String, String), Vec<String>> = BTreeMap::new();
        for batch in &rows {
            let markers = support::column::<StringArray>(batch, "trace_state");
            let subjects = support::column::<StringArray>(batch, "sub_principal_id");
            let actors = support::column::<StringArray>(batch, "act_principal_id");
            let runs = support::column::<StringArray>(batch, "run_id");
            for index in 0..batch.num_rows() {
                let run = if runs.is_null(index) {
                    String::new()
                } else {
                    runs.value(index).to_owned()
                };
                attributed
                    .entry((
                        subjects.value(index).to_owned(),
                        actors.value(index).to_owned(),
                        run,
                    ))
                    .or_default()
                    .push(markers.value(index).to_owned());
            }
        }

        assert_eq!(
            attributed.len(),
            5,
            "each replay collapses onto one fence while the second principal, the \
             differently correlated export, and each delegated actor keep their own: \
             {attributed:?}"
        );
        let direct: BTreeSet<&String> = attributed
            .keys()
            .filter(|(subject, actor, _)| subject == actor)
            .map(|(subject, _, _)| subject)
            .collect();
        assert_eq!(
            direct.len(),
            2,
            "two authorized direct publishers keep two distinct subjects: {attributed:?}"
        );
        let (subject, _, _) = attributed
            .keys()
            .find(|(_, _, run)| run == ATTRIBUTION_RUN)
            .expect("the run-correlated export is stored");
        let delegated: BTreeSet<&String> = attributed
            .keys()
            .filter(|(sub, actor, _)| sub == subject && actor != subject)
            .map(|(_, actor, _)| actor)
            .collect();
        assert_eq!(
            delegated.len(),
            2,
            "two actors delegated by one subject keep two distinct actor stamps: {attributed:?}"
        );
        assert!(
            delegated.iter().any(|actor| direct.contains(actor)),
            "the second principal acting for the subject is stamped as the actor: {attributed:?}"
        );
        let runs: BTreeSet<&String> = attributed.keys().map(|(_, _, run)| run).collect();
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

    /// A delegated export is owned by the current actor's exact root.
    ///
    /// One actor Service account runs two registered releases of its Card and
    /// receives delegation from a Service subject rooted at its own, different
    /// Card and from the journey's rootless User. Each delegation exports the
    /// same spans over gRPC and OTLP/HTTP. Every delegation must store its own
    /// row set, exactly once, owned by the actor's selected release
    /// (`workload_card_uid` and its canonical Service name and version), never
    /// by the subject's root and never left unowned, while the account pair
    /// keeps naming the subject and the actor.
    ///
    /// # Panics
    ///
    /// Panics when a fixture cannot be provisioned, a delegation or export is
    /// refused, or the stored ownership, pair, or row counts differ.
    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "requires the Postgres-backed Bifrost journey lane"]
    async fn delegated_exports_are_owned_by_the_current_actors_root() {
        let journey = OtlpJourney::start().await;
        let anchor = support::anchor_nanos();
        let server = journey.server();
        let role = "otlp_owner_writer";
        server
            .seed_role(role, &[Permission::bifrost_record_write()])
            .await
            .expect("the writer role is seeded");
        let subject = server
            .bootstrap_service("otlp-owner-subject", &[role])
            .await
            .expect("the subject Service registers");
        let subject_token = server
            .exchange_rooted(&subject)
            .await
            .expect("the subject selects its own Card as root");
        let releases = ["1.0.0", "2.0.0"].map(|version| CardRef {
            kind: CardKind::Service,
            name: CardName::new("otlp-owner-actor").expect("a valid Card name"),
            version: VersionBlock::parse(version).expect("a valid version"),
            uid: Some(CardUid::new(uuid::Uuid::now_v7().to_string()).expect("a UUIDv7 Card UID")),
        });
        for release in &releases {
            server
                .seed_card_principal(release, &[role])
                .await
                .expect("the actor release registers");
        }
        let actor = server
            .credential_registered_service(&releases[0], &[])
            .await
            .expect("the actor family account is credentialed");
        let actor_key = actor.api_key().expect("the actor carries an API key");
        let mut rooted = Vec::new();
        for release in &releases {
            let root = release.uid.clone().expect("a seeded release has a UID");
            let token = server
                .exchange_api_key_for_workload(actor_key, &root)
                .await
                .expect("one actor account selects either release");
            rooted.push((root, release.version.to_string(), token));
        }

        let subject_id = subject.id().to_string();
        let mut expected = BTreeSet::new();
        for (delegator, (root, version, actor_token)) in [
            (subject_token.as_str(), &rooted[0]),
            (subject_token.as_str(), &rooted[1]),
            (journey.token(), &rooted[0]),
        ] {
            let delegated = server
                .delegate(delegator, actor_token, TokenAudience::Wyrd)
                .await
                .expect("the subject delegates to the actor release");
            for _ in 0..2 {
                export_traces_over_grpc_as(
                    &journey,
                    &delegated,
                    negative_resource_spans(anchor, &[]),
                )
                .await;
            }
            let (status, body) = post_otlp_raw(
                &journey,
                "/v1/traces",
                HttpEncoding::Protobuf,
                HttpEncoding::Protobuf.encode(&ExportTraceServiceRequest {
                    resource_spans: negative_resource_spans(anchor, &[]),
                }),
                Some(&delegated),
            )
            .await;
            assert!(
                status.is_success(),
                "the delegated HTTP export is accepted ({status}): {}",
                String::from_utf8_lossy(&body)
            );
            expected.insert((
                delegator == subject_token,
                root.to_string(),
                version.clone(),
            ));
        }

        journey.publish().await;
        let rows = journey
            .query(&format!(
                "SELECT trace_state, sub_principal_id, act_principal_id, workload_card_uid, \
                 service_name, service_version \
                 FROM {SPANS_TABLE} WHERE scope_name = '{NEGATIVE_TRACE_SCOPE}'"
            ))
            .await;
        let mut owned: BTreeMap<(bool, String, String), Vec<String>> = BTreeMap::new();
        for batch in &rows {
            let text = |name: &str, index: usize| {
                let column = support::column::<StringArray>(batch, name);
                (!column.is_null(index)).then(|| column.value(index).to_owned())
            };
            for index in 0..batch.num_rows() {
                assert_eq!(
                    text("act_principal_id", index),
                    Some(actor.id().to_string()),
                    "the actor account is the stamped actor"
                );
                assert_eq!(
                    text("service_name", index).as_deref(),
                    Some("otlp-owner-actor"),
                    "the actor's release, not the resource or the subject, names the Service"
                );
                let workload = text("workload_card_uid", index).expect("a delegated row is owned");
                owned
                    .entry((
                        text("sub_principal_id", index) == Some(subject_id.clone()),
                        workload,
                        text("service_version", index).expect("a release version"),
                    ))
                    .or_default()
                    .push(text("trace_state", index).expect("a marker"));
            }
        }
        assert_eq!(
            owned.keys().cloned().collect::<BTreeSet<_>>(),
            expected,
            "each delegation is owned by the actor release it selected"
        );
        for (ownership, mut markers) in owned {
            markers.sort();
            assert_eq!(
                markers,
                SPAN_MARKERS.map(str::to_owned).to_vec(),
                "{ownership:?} stores every span exactly once"
            );
        }

        journey.shutdown().await;
    }

    /// An unbound tenant administrator attributes spans to registered Cards.
    ///
    /// The administrator's signed claims name no Card scope, so the Gate
    /// resolves each span's `wyrd.card_uid` against the tenant registry. A span
    /// naming a registered Service is stored with that Card's registry UID, a
    /// span naming no Card is stored uncorrelated, and a span naming an
    /// unregistered Card is rejected alone with the stable scope reason.
    ///
    /// # Panics
    ///
    /// Panics when the export is refused, when the partial success differs,
    /// or when the stored rows or their Card UIDs differ from the expectation.
    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "requires the Postgres-backed Bifrost journey lane"]
    async fn unbound_writer_attributes_spans_to_registered_cards_only() {
        let journey = OtlpJourney::start().await;
        let anchor = support::anchor_nanos();
        let server = journey.server();
        let observed = server
            .bootstrap_service("otlp-observed", &["viewer"])
            .await
            .expect("the observed Service registers")
            .card_ref()
            .expect("a bootstrapped Service is bound to its Card")
            .clone();
        let uid = registered_uid(server, observed.name.as_str())
            .await
            .as_str()
            .to_owned();
        let never_registered = uuid::Uuid::now_v7().to_string();
        let admin = server
            .exchange_api_key(&server.tenant_admin_key().await.expect("tenant admin key"))
            .await
            .expect("the administrator's key exchanges for a token");

        let partial = export_traces_over_grpc_as(
            &journey,
            &admin,
            card_attributed_resource_spans(anchor, [Some(&uid), Some(&never_registered), None]),
        )
        .await
        .expect("an unregistered Card rejects its span");
        assert_eq!(
            partial.rejected_spans, 1,
            "only the unregistered Card's span"
        );
        assert_eq!(
            partial.error_message,
            "wyrd.card_uid is outside the principal's Card scope"
        );

        journey.publish().await;
        let rows = journey
            .query(&format!(
                "SELECT trace_state, subject_card_uid FROM {SPANS_TABLE} \
                 WHERE scope_name = '{NEGATIVE_TRACE_SCOPE}' ORDER BY trace_state"
            ))
            .await;
        let mut stored = Vec::new();
        for batch in &rows {
            let markers = support::column::<StringArray>(batch, "trace_state");
            let cards = support::column::<StringArray>(batch, "subject_card_uid");
            for index in 0..batch.num_rows() {
                stored.push((
                    markers.value(index).to_owned(),
                    (!cards.is_null(index)).then(|| cards.value(index).to_owned()),
                ));
            }
        }
        assert_eq!(
            stored,
            vec![
                (SPAN_MARKERS[0].to_owned(), Some(uid)),
                (SPAN_MARKERS[2].to_owned(), None),
            ]
        );

        journey.shutdown().await;
    }

    /// The Card-scope rejection reason a rooted caller's foreign Card gets.
    const CARD_SCOPE_REJECTION: &str = "wyrd.card_uid is outside the principal's Card scope";

    /// An optional workload root narrows ingest without replacing authentication.
    ///
    /// Every export goes to traces, logs, and metrics over OTLP/HTTP, and the
    /// selector cases also go to traces over OTLP/gRPC. A rootless User and a
    /// rootless unrestricted Service each store generic telemetry under no
    /// Card. A caller without the write grant is refused even with a usable
    /// root. A rootless family Service stores generic telemetry, but naming
    /// a Card refuses the whole request until it selects a root. With
    /// `x-wyrd-workload-uid` its bearer resolves the family's root, so a
    /// record naming that root keeps it and a record naming a foreign Card is
    /// rejected alone. An unknown root and a foreign family's root are refused
    /// with one public error. A bearer signed for the root accepts the equal
    /// selector and refuses a different one, and a direct API key with the
    /// selector resolves the same root. The rooted exports all name one
    /// publisher and one accepted row set, so the fence stores them once.
    ///
    /// # Panics
    ///
    /// Panics when a fixture cannot be provisioned, when an export is accepted
    /// or refused other than expected, or when the stored rows, their
    /// publishers, or their Card UIDs differ from the expectation.
    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "requires the Postgres-backed Bifrost journey lane"]
    async fn optional_workload_context_preserves_authentication() {
        let journey = OtlpJourney::start().await;
        let anchor = support::anchor_nanos();
        let server = journey.server();
        server
            .seed_role(
                "otlp_workload_writer",
                &[Permission::bifrost_record_write()],
            )
            .await
            .expect("the writer role is seeded");
        let observed = server
            .bootstrap_service("otlp-workload-observed", &["viewer"])
            .await
            .expect("the observed Service registers");
        let observed = observed
            .card_ref()
            .expect("a bootstrapped Service is bound to its Card");
        server
            .bootstrap_service("otlp-workload-foreign", &["viewer"])
            .await
            .expect("the foreign Service registers");
        let root = registered_uid(server, observed.name.as_str()).await;
        let foreign = registered_uid(server, "otlp-workload-foreign").await;
        let unknown = wyrd_spec::ids::CardUid::from_uuid(uuid::Uuid::now_v7())
            .expect("a fresh UUIDv7 is a Card UID");
        let family = server
            .credential_registered_service(observed, &["otlp_workload_writer"])
            .await
            .expect("the observed family's runtime account is credentialed");
        let family_key = family
            .api_key()
            .expect("the family account carries an API key");
        let family_bearer = bearer(
            &server
                .exchange_api_key(family_key)
                .await
                .expect("the family account exchanges without a root"),
        );
        let rooted_bearer = bearer(
            &server
                .exchange_api_key_for_workload(family_key, &root)
                .await
                .expect("the family account exchanges for its root"),
        );
        let unrestricted = server
            .bootstrap_service("otlp-workload-unrestricted", &["otlp_workload_writer"])
            .await
            .expect("the unrestricted Service bootstraps");
        let unrestricted_bearer = bearer(
            &server
                .exchange_api_key(
                    unrestricted
                        .api_key()
                        .expect("the unrestricted Service carries an API key"),
                )
                .await
                .expect("the unrestricted Service exchanges without a root"),
        );
        let user = server
            .bootstrap_user("otlp-workload-user", &["admin"])
            .await
            .expect("the rootless User bootstraps");
        let Bootstrap::User { jwt, .. } = &user else {
            panic!("bootstrap_user returns a User");
        };
        let user_bearer = bearer(jwt);
        let reader_bearer = bearer(
            &journey
                .token_with_permissions("otlp_workload_reader", &[Permission::bifrost_query_read()])
                .await,
        );
        let api_key = secrecy::ExposeSecret::expose_secret(family_key);
        let (root, foreign, unknown) = (root.as_str(), foreign.as_str(), unknown.as_str());
        let generic = [None; 3];
        let rooted = [Some(root), Some(foreign), Some(root)];
        let token = "x-wyrd-access-token";
        let selector = wyrd_spec::auth::WORKLOAD_UID_HEADER;

        for headers in [
            [(token, user_bearer.as_str())],
            [(token, unrestricted_bearer.as_str())],
        ] {
            export_everywhere(&journey, anchor, generic, &headers, Ok((0, ""))).await;
        }
        export_everywhere(
            &journey,
            anchor,
            rooted,
            &[(token, reader_bearer.as_str()), (selector, root)],
            Err((StatusCode::FORBIDDEN, "WYRD_PERMISSION_403_DENIED_RBAC")),
        )
        .await;
        export_everywhere(
            &journey,
            anchor,
            generic,
            &[(token, family_bearer.as_str())],
            Ok((0, "")),
        )
        .await;
        export_everywhere(
            &journey,
            anchor,
            [Some(root), None, None],
            &[(token, family_bearer.as_str())],
            Err((
                StatusCode::FORBIDDEN,
                "WYRD_AUTH_403_WORKLOAD_CONTEXT_REQUIRED",
            )),
        )
        .await;
        for refused in [unknown, foreign] {
            export_everywhere(
                &journey,
                anchor,
                rooted,
                &[(token, family_bearer.as_str()), (selector, refused)],
                Err((
                    StatusCode::UNAUTHORIZED,
                    "WYRD_AUTH_401_WORKLOAD_UID_REFUSED",
                )),
            )
            .await;
        }
        export_everywhere(
            &journey,
            anchor,
            rooted,
            &[(token, rooted_bearer.as_str()), (selector, foreign)],
            Err((
                StatusCode::BAD_REQUEST,
                "WYRD_AUTH_400_SIGNED_WORKLOAD_MISMATCH",
            )),
        )
        .await;
        for headers in [
            [(token, family_bearer.as_str()), (selector, root)],
            [(token, rooted_bearer.as_str()), (selector, root)],
            [("x-wyrd-api-key", api_key), (selector, root)],
        ] {
            export_everywhere(
                &journey,
                anchor,
                rooted,
                &headers,
                Ok((1, CARD_SCOPE_REJECTION)),
            )
            .await;
        }

        journey.publish().await;
        for signal in Signal::ALL {
            let [first, second, last] = signal.markers().map(str::to_owned);
            let mut generic: Vec<(String, Option<String>)> = [&first, &second, &last]
                .map(|marker| (marker.clone(), None))
                .to_vec();
            generic.sort_unstable();
            let mut family_rows = generic.clone();
            family_rows.extend([first, last].map(|marker| (marker, Some(root.to_owned()))));
            family_rows.sort_unstable();
            assert_eq!(
                stored_attribution(&journey, signal).await,
                BTreeMap::from([
                    (user.id().to_string(), generic.clone()),
                    (unrestricted.id().to_string(), generic),
                    (family.id().to_string(), family_rows),
                ]),
                "{signal:?} stores each accepted record once under its publisher and Card"
            );
        }

        journey.shutdown().await;
    }

    /// The accepted request context, not a record's labels, fills the shared
    /// dimensions of OTLP rows.
    ///
    /// An export under a validated root stores that Service's canonical name
    /// and version over the resource's `service.name`, and an accepted request
    /// environment over the resource's. Without a request environment the
    /// typed resource environment survives, also under a validated root, and
    /// a rootless export keeps its observed Service and no version. An empty
    /// environment header refuses the whole request over HTTP and gRPC, and a
    /// repeated one over HTTP.
    ///
    /// # Panics
    ///
    /// Panics when a fixture cannot be provisioned, an export is accepted or
    /// refused other than expected, or a signal stores other dimensions.
    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "requires the Postgres-backed Bifrost journey lane"]
    async fn request_context_fills_shared_dimensions() {
        let journey = OtlpJourney::start().await;
        let anchor = support::anchor_nanos();
        let server = journey.server();
        server
            .seed_role("otlp_context_writer", &[Permission::bifrost_record_write()])
            .await
            .expect("the writer role is seeded");
        let observed = server
            .bootstrap_service("otlp-context-observed", &["viewer"])
            .await
            .expect("the observed Service registers");
        let observed = observed
            .card_ref()
            .expect("a bootstrapped Service is bound to its Card");
        let root = registered_uid(server, observed.name.as_str()).await;
        let family = server
            .credential_registered_service(observed, &["otlp_context_writer"])
            .await
            .expect("the observed family's runtime account is credentialed");
        let family_key = family
            .api_key()
            .expect("the family account carries an API key");
        let rootless = bearer(
            &server
                .exchange_api_key(family_key)
                .await
                .expect("the family account exchanges without a root"),
        );
        let api_key = secrecy::ExposeSecret::expose_secret(family_key);
        let (key, selector, environment) = (
            "x-wyrd-api-key",
            wyrd_spec::auth::WORKLOAD_UID_HEADER,
            wyrd_spec::auth::DEPLOYMENT_ENVIRONMENT_HEADER,
        );
        let root = root.as_str();
        let generic = [None; 3];
        let refused = Err((StatusCode::BAD_REQUEST, "WYRD_SPEC_400_VALIDATION"));

        export_everywhere(
            &journey,
            anchor,
            generic,
            &[(key, api_key), (selector, root), (environment, "")],
            refused,
        )
        .await;
        for signal in Signal::ALL {
            let (status, body) = post_otlp_with_headers(
                &journey,
                signal.path(),
                HttpEncoding::Protobuf,
                signal.body(anchor, generic),
                &[
                    (key, api_key),
                    (selector, root),
                    (environment, "staging"),
                    (environment, "prod"),
                ],
            )
            .await;
            assert_refusal(
                status,
                &body,
                StatusCode::BAD_REQUEST,
                "WYRD_SPEC_400_VALIDATION",
            );
        }
        for headers in [
            &[(key, api_key), (selector, root), (environment, "staging")][..],
            &[(key, api_key), (selector, root)],
            &[("x-wyrd-access-token", rootless.as_str())],
        ] {
            export_everywhere(&journey, anchor, generic, headers, Ok((0, ""))).await;
        }

        journey.publish().await;
        let (name, version) = (
            Some(observed.name.to_string()),
            Some(observed.version.to_string()),
        );
        let resource = Some("journey".to_owned());
        for signal in Signal::ALL {
            let (table, scope, _) = signal.ledger();
            let rows = journey
                .query(&format!(
                    "SELECT service_name, service_version, deployment_environment, \
                     workload_card_uid FROM {table} WHERE scope_name = '{scope}'"
                ))
                .await;
            let mut stored = BTreeSet::new();
            for batch in &rows {
                let text = |name: &str, index: usize| {
                    let column = support::column::<StringArray>(batch, name);
                    (!column.is_null(index)).then(|| column.value(index).to_owned())
                };
                for index in 0..batch.num_rows() {
                    stored.insert([
                        text("service_name", index),
                        text("service_version", index),
                        text("deployment_environment", index),
                        text("workload_card_uid", index),
                    ]);
                }
            }
            assert_eq!(
                stored,
                BTreeSet::from([
                    [
                        name.clone(),
                        version.clone(),
                        Some("staging".to_owned()),
                        Some(root.to_owned()),
                    ],
                    [
                        name.clone(),
                        version.clone(),
                        resource.clone(),
                        Some(root.to_owned()),
                    ],
                    [
                        Some(support::SERVICE_NAME.to_owned()),
                        None,
                        resource.clone(),
                        None,
                    ],
                ]),
                "{signal:?} stores the accepted context over its labels"
            );
        }

        journey.shutdown().await;
    }

    /// Formats `token` as the bearer value an exporter sends.
    fn bearer(token: &str) -> String {
        format!("Bearer {token}")
    }

    /// Reads the registry UID of the registered Service Card named `name`.
    ///
    /// # Panics
    ///
    /// Panics when the registry cannot be read or `card` is not registered.
    async fn registered_uid(
        server: &wyrd_testing::WyrdTestServer,
        name: &str,
    ) -> wyrd_spec::ids::CardUid {
        let uid: uuid::Uuid = sqlx::query_scalar(
            "SELECT card_uid FROM wyrd.cards WHERE kind = 'Service' AND name = $1",
        )
        .bind(name)
        .fetch_one(&server.pg_fixture().superuser_pool().expect("registry pool"))
        .await
        .expect("the Card is registered");
        wyrd_spec::ids::CardUid::from_uuid(uid).expect("a registry UID is a Card UID")
    }

    /// One OTLP signal a workload-context case exports and reads back.
    #[derive(Clone, Copy, Debug)]
    enum Signal {
        /// `bifrost.traces.spans`, the only signal also exported over gRPC here.
        Traces,
        /// `bifrost.logs.records`.
        Logs,
        /// `bifrost.metrics.points`.
        Metrics,
    }

    impl Signal {
        /// Every signal, in the order a case exports them.
        const ALL: [Self; 3] = [Self::Traces, Self::Logs, Self::Metrics];

        /// The OTLP/HTTP route the signal is posted to.
        const fn path(self) -> &'static str {
            match self {
                Self::Traces => "/v1/traces",
                Self::Logs => "/v1/logs",
                Self::Metrics => "/v1/metrics",
            }
        }

        /// The table, scope, and marker column the signal's rows are read by.
        const fn ledger(self) -> (&'static str, &'static str, &'static str) {
            match self {
                Self::Traces => (SPANS_TABLE, NEGATIVE_TRACE_SCOPE, "trace_state"),
                Self::Logs => (LOGS_TABLE, NEGATIVE_LOG_SCOPE, "event_name"),
                Self::Metrics => (METRICS_TABLE, NEGATIVE_METRIC_SCOPE, "metric_name"),
            }
        }

        /// The marker each export position is stored under.
        const fn markers(self) -> [&'static str; 3] {
            match self {
                Self::Traces => SPAN_MARKERS,
                Self::Logs => LOG_MARKERS,
                Self::Metrics => [
                    support::GAUGE_INT_METRIC,
                    support::GAUGE_DOUBLE_METRIC,
                    support::SUM_INT_METRIC,
                ],
            }
        }

        /// The protobuf export whose `i`th record names `cards[i]`.
        ///
        /// # Panics
        ///
        /// Panics when the signal's card-attributed fixture builder panics: a
        /// fixture without a first resource or scope, or a metric that is not
        /// the single-point gauge or sum the fixture defines.
        fn body(self, anchor: i64, cards: [Option<&str>; 3]) -> Vec<u8> {
            match self {
                Self::Traces => ExportTraceServiceRequest {
                    resource_spans: card_attributed_resource_spans(anchor, cards),
                }
                .encode_to_vec(),
                Self::Logs => ExportLogsServiceRequest {
                    resource_logs: card_attributed_resource_logs(anchor, cards),
                }
                .encode_to_vec(),
                Self::Metrics => ExportMetricsServiceRequest {
                    resource_metrics: card_attributed_resource_metrics(anchor, cards),
                }
                .encode_to_vec(),
            }
        }

        /// The rejected count and first reason of one accepted protobuf answer.
        ///
        /// # Panics
        ///
        /// Panics when `body` does not decode as the signal's protobuf export
        /// response.
        fn rejected(self, body: &[u8]) -> (i64, String) {
            let encoding = HttpEncoding::Protobuf;
            match self {
                Self::Traces => encoding
                    .decode::<ExportTraceServiceResponse>(body)
                    .partial_success
                    .map(|partial| (partial.rejected_spans, partial.error_message)),
                Self::Logs => encoding
                    .decode::<ExportLogsServiceResponse>(body)
                    .partial_success
                    .map(|partial| (partial.rejected_log_records, partial.error_message)),
                Self::Metrics => encoding
                    .decode::<ExportMetricsServiceResponse>(body)
                    .partial_success
                    .map(|partial| (partial.rejected_data_points, partial.error_message)),
            }
            .unwrap_or_default()
        }
    }

    /// Exports `cards` on every signal under `headers` and asserts `expected`.
    ///
    /// `Ok((rejected, reason))` is an accepted export reporting exactly that
    /// partial success; `Err((status, code))` is a whole-request refusal with
    /// that public error. Every signal goes over OTLP/HTTP protobuf, and a
    /// selector case also sends traces over OTLP/gRPC so the selector is
    /// proven on both transports.
    ///
    /// # Panics
    ///
    /// Panics when any transport answers other than `expected`.
    async fn export_everywhere(
        journey: &OtlpJourney,
        anchor: i64,
        cards: [Option<&str>; 3],
        headers: &[(&'static str, &str)],
        expected: Result<(i64, &str), (StatusCode, &str)>,
    ) {
        for signal in Signal::ALL {
            let (status, body) = post_otlp_with_headers(
                journey,
                signal.path(),
                HttpEncoding::Protobuf,
                signal.body(anchor, cards),
                headers,
            )
            .await;
            match expected {
                Ok((rejected, reason)) => {
                    assert!(
                        status.is_success(),
                        "{signal:?} over HTTP is accepted ({status}): {}",
                        String::from_utf8_lossy(&body)
                    );
                    assert_eq!(
                        signal.rejected(&body),
                        (rejected, reason.to_owned()),
                        "{signal:?} over HTTP reports the exact partial success"
                    );
                }
                Err((expected_status, code)) => {
                    assert_refusal(status, &body, expected_status, code);
                }
            }
        }
        if !headers
            .iter()
            .any(|(name, _)| *name == wyrd_spec::auth::WORKLOAD_UID_HEADER)
        {
            return;
        }
        let answer = export_traces_over_grpc_with(
            journey,
            headers,
            card_attributed_resource_spans(anchor, cards),
        )
        .await;
        match (expected, answer) {
            (Ok((rejected, reason)), Ok(partial)) => {
                let partial = partial.unwrap_or_default();
                assert_eq!(
                    (partial.rejected_spans, partial.error_message.as_str()),
                    (rejected, reason),
                    "traces over gRPC report the exact partial success"
                );
            }
            (Err((_, code)), Err(status)) => {
                let problem: serde_json::Value = serde_json::from_slice(
                    status
                        .metadata()
                        .get_bin(wyrd_tonic::error::WYRD_ERROR_HEADER)
                        .expect("a public gRPC refusal carries its problem document")
                        .to_bytes()
                        .expect("the problem document is valid base64")
                        .as_ref(),
                )
                .expect("the problem document is JSON");
                assert_eq!(
                    problem.get("code").and_then(serde_json::Value::as_str),
                    Some(code),
                    "the gRPC refusal keeps its public error code: {status:?}"
                );
            }
            (expected, answer) => {
                panic!("traces over gRPC answered {answer:?}, expected {expected:?}")
            }
        }
    }

    /// Reads `signal`'s stored rows as sorted (marker, Card UID) per publisher.
    ///
    /// # Panics
    ///
    /// Panics when the query fails or a column is not the canonical type.
    async fn stored_attribution(
        journey: &OtlpJourney,
        signal: Signal,
    ) -> BTreeMap<String, Vec<(String, Option<String>)>> {
        let (table, scope, marker) = signal.ledger();
        let rows = journey
            .query(&format!(
                "SELECT {marker}, sub_principal_id, subject_card_uid FROM {table} \
                 WHERE scope_name = '{scope}'"
            ))
            .await;
        let mut stored: BTreeMap<String, Vec<(String, Option<String>)>> = BTreeMap::new();
        for batch in &rows {
            let markers = support::column::<StringArray>(batch, marker);
            let publishers = support::column::<StringArray>(batch, "sub_principal_id");
            let cards = support::column::<StringArray>(batch, "subject_card_uid");
            for index in 0..batch.num_rows() {
                stored
                    .entry(publishers.value(index).to_owned())
                    .or_default()
                    .push((
                        markers.value(index).to_owned(),
                        (!cards.is_null(index)).then(|| cards.value(index).to_owned()),
                    ));
            }
        }
        for rows in stored.values_mut() {
            rows.sort_unstable();
        }
        stored
    }

    /// Asserts the sorted stored markers are exactly `accepted`, once each.
    ///
    /// The replayed export carries the same accepted canonical rows, so the
    /// Gate derives the same batch identity and the durable fence suppresses
    /// the repeat: each accepted sibling appears exactly once. A rejected
    /// sibling must never appear.
    ///
    /// # Panics
    ///
    /// Panics when a rejected marker is present, when an accepted sibling is
    /// duplicated or missing, or when any other marker is stored.
    fn assert_accepted_subset(markers: &[String], accepted: &[&str], rejected: &str) {
        assert!(
            !markers.iter().any(|marker| marker == rejected),
            "the rejected sibling `{rejected}` is never stored"
        );
        let mut expected: Vec<String> =
            accepted.iter().map(|marker| (*marker).to_owned()).collect();
        expected.sort_unstable();
        assert_eq!(
            markers, expected,
            "a replayed export stores its accepted siblings exactly once"
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

    /// A wire-small export whose projection expands past four times the wire
    /// ceiling is refused whole before WAL, while a smaller export of the same
    /// shape is accepted.
    ///
    /// The large resource attribute is sent once and repeated onto every span
    /// row, which is exactly the expansion the wire ceiling cannot see. The
    /// refused export must answer with the typed payload-too-large problem and
    /// leave no row behind: the scope holds only the accepted export's spans.
    ///
    /// # Panics
    ///
    /// Panics when the fitting export is refused, the expanding export exceeds
    /// the wire ceiling or is accepted, or any of its rows become queryable.
    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "requires the Postgres-backed Bifrost journey lane"]
    async fn projected_fan_out_above_expanded_ceiling_is_refused_before_ack() {
        let journey = OtlpJourney::start().await;
        let anchor = support::anchor_nanos();
        let fits = ExportTraceServiceRequest {
            resource_spans: fan_out_resource_spans(anchor, 8),
        };
        post_otlp(
            &journey,
            "/v1/traces",
            HttpEncoding::Protobuf,
            fits.encode_to_vec(),
        )
        .await;
        let expands = ExportTraceServiceRequest {
            resource_spans: fan_out_resource_spans(anchor, 2048),
        }
        .encode_to_vec();
        assert!(
            expands.len() < vala_bifrost_redux::gate::limits::BIFROST_INGEST_REQUEST_LIMIT_BYTES,
            "the expanding export is within the wire ceiling: {}",
            expands.len()
        );
        assert_refused(
            &journey,
            "/v1/traces",
            HttpEncoding::Protobuf,
            expands,
            Some(journey.token().to_owned()),
            StatusCode::PAYLOAD_TOO_LARGE,
            "WYRD_VALA_413_PAYLOAD_TOO_LARGE",
        )
        .await;

        journey.publish().await;
        let stored: usize = journey
            .try_query(&format!(
                "SELECT wyrd_request_id FROM {SPANS_TABLE} WHERE scope_name = '{FAN_OUT_TRACE_SCOPE}'"
            ))
            .await
            .expect("the accepted export is queryable")
            .iter()
            .map(RecordBatch::num_rows)
            .sum();
        assert_eq!(stored, 8, "only the accepted export's spans are stored");

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
        let sql = format!("SELECT wyrd_request_id FROM {table} WHERE scope_name = '{scope}'");
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
        assert_refusal(status, &response, expected_status, expected_code);
    }

    /// Asserts one collector answer is the whole-request refusal it names.
    ///
    /// `code` is empty when the refusal is owned by the transport's own
    /// authentication shell rather than by the public error catalog.
    ///
    /// # Panics
    ///
    /// Panics when the answer has a different status or public error code, or
    /// carries a partial success.
    fn assert_refusal(
        status: StatusCode,
        response: &[u8],
        expected_status: StatusCode,
        expected_code: &str,
    ) {
        let text = String::from_utf8_lossy(response);
        assert_eq!(
            status, expected_status,
            "the collector refuses the whole request: {text}"
        );
        assert!(
            !text.contains("partialSuccess") && !text.contains("partial_success"),
            "a whole-request refusal carries no partial success: {text}"
        );
        if !expected_code.is_empty() {
            let problem: serde_json::Value = serde_json::from_slice(response)
                .expect("a public refusal answers with the problem document");
            assert_eq!(
                problem.get("code").and_then(serde_json::Value::as_str),
                Some(expected_code),
                "the refusal keeps its public error code: {text}"
            );
        }
    }
}
