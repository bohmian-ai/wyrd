//! The OTLP/HTTP trace journey: protobuf and protobuf-JSON agree with gRPC.

use wyrd_tonic::otlp::trace::v1::ResourceSpans;
use wyrd_tonic::otlp::trace_service::ExportTraceServiceRequest;
use wyrd_tonic::prost::Message;

use super::support::OtlpJourney;

/// Wire encoding one OTLP/HTTP export declares and its response echoes.
#[derive(Clone, Copy, Debug)]
pub(super) enum HttpEncoding {
    /// `application/x-protobuf`, the OTLP/HTTP default.
    Protobuf,
    /// `application/json`, the OTLP protobuf-JSON mapping.
    Json,
}

impl HttpEncoding {
    /// The `Content-Type` an exporter declares for this encoding.
    pub(super) const fn content_type(self) -> &'static str {
        match self {
            Self::Protobuf => "application/x-protobuf",
            Self::Json => "application/json",
        }
    }

    /// Encodes one pinned prost request in this wire encoding.
    ///
    /// Both encodings serialize the *same* constructed message, so a
    /// disagreement between the two stored rows can only come from the
    /// server's decode path and never from two fixtures that drifted apart.
    ///
    /// # Panics
    ///
    /// Panics when the pinned message cannot be serialized, which would be a
    /// defect in the generated OTLP types rather than in this journey.
    pub(super) fn encode<M: Message + serde::Serialize>(self, message: &M) -> Vec<u8> {
        match self {
            Self::Protobuf => message.encode_to_vec(),
            Self::Json => {
                serde_json::to_vec(message).expect("the pinned OTLP message serializes to JSON")
            }
        }
    }

    /// Decodes one collector response encoded the same way the export was.
    ///
    /// # Panics
    ///
    /// Panics when the collector's response does not decode in the encoding it
    /// was asked to answer in, which is a protocol defect rather than a
    /// fidelity outcome.
    pub(super) fn decode<M: Message + Default + serde::de::DeserializeOwned>(
        self,
        body: &[u8],
    ) -> M {
        match self {
            Self::Protobuf => {
                M::decode(body).expect("the collector answers in the declared encoding")
            }
            Self::Json => serde_json::from_slice(body)
                .expect("the collector answers in the declared encoding"),
        }
    }
}

/// Posts one encoded OTLP body to a signal's HTTP collector route.
///
/// Returns the response body so a case can assert the echoed encoding and any
/// `partial_success` the collector reported.
///
/// # Panics
///
/// Panics when the route cannot be reached or does not accept the export; a
/// refusal is a defect in a fidelity case rather than an outcome it asserts.
pub(super) async fn post_otlp(
    journey: &OtlpJourney,
    path: &str,
    encoding: HttpEncoding,
    body: Vec<u8>,
) -> Vec<u8> {
    let response = reqwest::Client::new()
        .post(format!("{}{path}", journey.base_url()))
        .header("x-wyrd-access-token", format!("Bearer {}", journey.token()))
        .header("content-type", encoding.content_type())
        .body(body)
        .send()
        .await
        .expect("the OTLP exporter reaches the bound HTTP collector");
    let status = response.status();
    let body = response
        .bytes()
        .await
        .expect("the collector response body is readable")
        .to_vec();
    assert!(
        status.is_success(),
        "the collector accepts the {} export ({status}): {}",
        encoding.content_type(),
        String::from_utf8_lossy(&body)
    );
    body
}

/// Posts one trace export in the requested HTTP encoding.
///
/// # Panics
///
/// Panics when the collector refuses the export.
pub(super) async fn export_traces_over_http(
    journey: &OtlpJourney,
    encoding: HttpEncoding,
    resource_spans: Vec<ResourceSpans>,
) -> Vec<u8> {
    let request = ExportTraceServiceRequest { resource_spans };
    post_otlp(journey, "/v1/traces", encoding, encoding.encode(&request)).await
}

/// Tests that need Postgres, a bound server, and the publication boundary.
mod pg_tests {
    use arrow::record_batch::RecordBatch;

    use super::super::support::{
        self, GRPC_SPAN, HTTP_JSON_SPAN, HTTP_PROTOBUF_SPAN, OtlpJourney, SPANS_TABLE,
        SpanIdentity, row_by_span_id,
    };
    use super::super::trace_export::{assert_maximal_span_row, export_traces_over_grpc};
    use super::{HttpEncoding, export_traces_over_http};

    /// Columns whose value is the transport's own identity rather than signal.
    ///
    /// Everything else the caller supplied must be byte-identical across the
    /// three transports; these two are exactly what the fixture varies so the
    /// rows stay distinguishable in one table. Server-managed columns are
    /// skipped by their `wyrd_` prefix because they record when this pod
    /// accepted each request, which is not a property of the encoding.
    const IDENTITY_COLUMNS: [&str; 2] = ["trace_id", "span_id"];

    /// Protobuf, protobuf-JSON and gRPC store one dataset identically.
    ///
    /// The same constructed `ExportTraceServiceRequest` is serialized as OTLP
    /// protobuf and as OTLP protobuf-JSON and posted to `/v1/traces`, and the
    /// same dataset is exported over gRPC. All three cross the real Scribe
    /// acknowledgment and publication boundaries. Each stored row is compared
    /// against the fixture constants directly, and then the three rows are
    /// compared against each other column by column — so an encoding that
    /// silently loses a field, coerces a numeric type, or reorders an
    /// attribute collection fails here even if every individual value still
    /// looks plausible.
    ///
    /// # Panics
    ///
    /// Panics when any transport is refused, when a stored row differs from
    /// what was sent, or when two transports disagree about any signal column.
    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "requires the Postgres-backed Bifrost journey lane"]
    async fn otlp_http_protobuf_and_json_match_grpc_trace_rows() {
        let journey = OtlpJourney::start().await;
        let anchor = support::anchor_nanos();

        let sent: [SpanIdentity; 3] = [GRPC_SPAN, HTTP_PROTOBUF_SPAN, HTTP_JSON_SPAN];
        export_traces_over_grpc(&journey, support::maximal_resource_spans(anchor, sent[0])).await;
        for (encoding, identity) in [
            (HttpEncoding::Protobuf, sent[1]),
            (HttpEncoding::Json, sent[2]),
        ] {
            export_traces_over_http(
                &journey,
                encoding,
                support::maximal_resource_spans(anchor, identity),
            )
            .await;
        }

        journey.publish().await;

        let stored = journey
            .query(&format!(
                "SELECT * FROM {SPANS_TABLE} WHERE start_time_unix_nano = {anchor}"
            ))
            .await;
        let rows: Vec<_> = sent
            .into_iter()
            .map(|identity| {
                let row = row_by_span_id(&stored, identity.span_id);
                assert_maximal_span_row(&row, anchor, identity);
                row
            })
            .collect();

        let (reference, others) = rows.split_first().expect("three transports were exported");
        for (index, other) in others.iter().enumerate() {
            assert_transports_agree(reference, other, index);
        }

        journey.shutdown().await;
    }

    /// Compares two transports' stored rows outside their varied identity.
    ///
    /// The start instant is the fixture's per-transport discriminator and the
    /// two identity columns are its per-transport identities; every other
    /// column carries signal and must be identical.
    ///
    /// # Panics
    ///
    /// Panics when the rows do not carry the same columns or any signal column
    /// differs between them.
    fn assert_transports_agree(reference: &RecordBatch, other: &RecordBatch, index: usize) {
        assert_eq!(
            reference.schema(),
            other.schema(),
            "every transport writes the same canonical schema"
        );
        for (position, field) in reference.schema().fields().iter().enumerate() {
            let name = field.name().as_str();
            if IDENTITY_COLUMNS.contains(&name) || name.starts_with("wyrd_") {
                continue;
            }
            assert_eq!(
                reference.column(position),
                other.column(position),
                "transport {index} stores a different `{name}`"
            );
        }
    }
}
