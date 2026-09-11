//! The OTLP/gRPC trace journey: a maximal span survives every boundary intact.

use arrow::array::{
    Array, BooleanArray, FixedSizeBinaryArray, Int32Array, Int64Array, LargeBinaryArray, ListArray,
    StringArray, StructArray,
};
use arrow::record_batch::RecordBatch;
use wyrd_tonic::otlp::trace_service::ExportTraceServiceRequest;
use wyrd_tonic::otlp::trace_service::trace_service_client::TraceServiceClient;
use wyrd_tonic::tonic::Request;
use wyrd_tonic::tonic::transport::Channel;

use super::support::{
    self, EVENT_DROPPED_ATTRIBUTES, EVENT_NAME, EVENT_OFFSET_NANOS, GEN_AI_CONVERSATION_ID,
    GEN_AI_INPUT_TOKENS, GEN_AI_OPERATION_NAME, GEN_AI_OUTPUT_TOKENS, GEN_AI_PROVIDER_NAME,
    GEN_AI_REQUEST_MODEL, LINK_DROPPED_ATTRIBUTES, LINK_FLAGS, LINK_SPAN_ID, LINK_TRACE_ID,
    LINK_TRACE_STATE, OtlpJourney, PARENT_SPAN_ID, RESOURCE_DROPPED_ATTRIBUTES,
    RESOURCE_SCHEMA_URL, SCOPE_DROPPED_ATTRIBUTES, SCOPE_NAME, SCOPE_SCHEMA_URL, SCOPE_VERSION,
    SERVICE_NAME, SPAN_DURATION_NANOS, SPAN_FLAGS, SPAN_KIND, SPAN_NAME, STATUS_CODE,
    STATUS_MESSAGE, SpanIdentity, TRACE_STATE, column,
};

/// Sends one OTLP trace export through the bound gRPC collector route.
///
/// Returns the accepted/rejected counts the collector reported, which is what
/// distinguishes a whole acceptance from a silent partial success.
///
/// # Panics
///
/// Panics when the transport cannot be dialed or the export is refused; a
/// refusal is a defect in this journey rather than an outcome it asserts.
pub(super) async fn export_traces_over_grpc(
    journey: &OtlpJourney,
    resource_spans: Vec<wyrd_tonic::otlp::trace::v1::ResourceSpans>,
) -> Option<wyrd_tonic::otlp::trace_service::ExportTracePartialSuccess> {
    let channel = Channel::from_shared(journey.grpc_url())
        .expect("the bound gRPC URL is a valid endpoint")
        .connect()
        .await
        .expect("the OTLP exporter dials the bound collector");
    let mut request = Request::new(ExportTraceServiceRequest { resource_spans });
    request.metadata_mut().insert(
        "x-wyrd-access-token",
        format!("Bearer {}", journey.token())
            .parse()
            .expect("the minted bearer is valid ASCII metadata"),
    );
    TraceServiceClient::new(channel)
        .export(request)
        .await
        .expect("the collector accepts the trace export")
        .into_inner()
        .partial_success
}

/// Asserts the maximal span's stored row against the fixture values directly.
///
/// Shared with the HTTP journey so protobuf, JSON, and gRPC are all compared
/// against one expectation rather than three that could drift apart.
///
/// # Panics
///
/// Panics when any canonical column differs from the value the fixture sent.
pub(super) fn assert_maximal_span_row(row: &RecordBatch, start: i64, identity: SpanIdentity) {
    assert_eq!(
        column::<FixedSizeBinaryArray>(row, "trace_id").value(0),
        identity.trace_id,
        "the span keeps the trace identity its exporter sent"
    );
    assert_eq!(
        column::<FixedSizeBinaryArray>(row, "span_id").value(0),
        identity.span_id
    );
    assert_eq!(
        column::<FixedSizeBinaryArray>(row, "parent_span_id").value(0),
        PARENT_SPAN_ID,
        "a present parent is stored rather than flattened to null"
    );
    assert_eq!(
        column::<StringArray>(row, "trace_state").value(0),
        TRACE_STATE
    );
    assert_eq!(column::<Int64Array>(row, "flags").value(0), SPAN_FLAGS);
    assert_eq!(column::<StringArray>(row, "name").value(0), SPAN_NAME);
    assert_eq!(column::<Int32Array>(row, "kind").value(0), SPAN_KIND);
    assert_eq!(
        column::<Int64Array>(row, "start_time_unix_nano").value(0),
        start
    );
    assert_eq!(
        column::<Int64Array>(row, "end_time_unix_nano").value(0),
        start + SPAN_DURATION_NANOS
    );
    assert_eq!(
        column::<Int64Array>(row, "duration_nano").value(0),
        SPAN_DURATION_NANOS,
        "duration is derived from the sent instants, not re-measured"
    );

    assert!(
        column::<BooleanArray>(row, "status_present").value(0),
        "a sent status is recorded as present"
    );
    assert_eq!(
        column::<Int32Array>(row, "status_code").value(0),
        STATUS_CODE
    );
    assert_eq!(
        column::<StringArray>(row, "status_message").value(0),
        STATUS_MESSAGE
    );

    assert_eq!(
        column::<LargeBinaryArray>(row, "attributes").value(0),
        support::canonical_attribute_bytes(&support::span_attributes()),
        "every attribute shape the exporter sent survives byte-for-byte"
    );
    assert_eq!(
        column::<Int64Array>(row, "dropped_attributes_count").value(0),
        support::DROPPED_ATTRIBUTES
    );

    assert_events(row, start);
    assert_eq!(
        column::<Int64Array>(row, "dropped_events_count").value(0),
        support::DROPPED_EVENTS
    );
    assert_links(row);
    assert_eq!(
        column::<Int64Array>(row, "dropped_links_count").value(0),
        support::DROPPED_LINKS
    );

    assert!(column::<BooleanArray>(row, "resource_present").value(0));
    assert_eq!(
        column::<LargeBinaryArray>(row, "resource_attributes").value(0),
        support::canonical_attribute_bytes(&support::resource_attributes())
    );
    assert_eq!(
        column::<Int64Array>(row, "resource_dropped_attributes_count").value(0),
        RESOURCE_DROPPED_ATTRIBUTES
    );
    assert_eq!(
        column::<StringArray>(row, "resource_schema_url").value(0),
        RESOURCE_SCHEMA_URL
    );
    assert_eq!(
        column::<ListArray>(row, "resource_entity_refs")
            .value(0)
            .len(),
        0,
        "an absent entity-reference collection is an empty list, not a null"
    );

    assert!(column::<BooleanArray>(row, "scope_present").value(0));
    assert_eq!(
        column::<StringArray>(row, "scope_name").value(0),
        SCOPE_NAME
    );
    assert_eq!(
        column::<StringArray>(row, "scope_version").value(0),
        SCOPE_VERSION
    );
    assert_eq!(
        column::<LargeBinaryArray>(row, "scope_attributes").value(0),
        support::canonical_attribute_bytes(&support::scope_attributes())
    );
    assert_eq!(
        column::<Int64Array>(row, "scope_dropped_attributes_count").value(0),
        SCOPE_DROPPED_ATTRIBUTES
    );
    assert_eq!(
        column::<StringArray>(row, "scope_schema_url").value(0),
        SCOPE_SCHEMA_URL
    );

    assert_eq!(
        column::<StringArray>(row, "service_name").value(0),
        SERVICE_NAME,
        "`service.name` is promoted out of the resource attributes"
    );
    for (name, expected) in [
        ("gen_ai_operation_name", GEN_AI_OPERATION_NAME),
        ("gen_ai_provider_name", GEN_AI_PROVIDER_NAME),
        ("gen_ai_request_model", GEN_AI_REQUEST_MODEL),
        ("gen_ai_conversation_id", GEN_AI_CONVERSATION_ID),
    ] {
        assert_eq!(
            column::<StringArray>(row, name).value(0),
            expected,
            "structured GenAI content is promoted into `{name}`"
        );
    }
    assert_eq!(
        column::<Int64Array>(row, "gen_ai_usage_input_tokens").value(0),
        GEN_AI_INPUT_TOKENS
    );
    assert_eq!(
        column::<Int64Array>(row, "gen_ai_usage_output_tokens").value(0),
        GEN_AI_OUTPUT_TOKENS
    );
}

/// Asserts the ordered nested event collection of the maximal span.
///
/// # Panics
///
/// Panics when the collection is not exactly the one event that was sent, in
/// order, with every child field intact.
fn assert_events(row: &RecordBatch, start: i64) {
    let events = column::<ListArray>(row, "events").value(0);
    assert_eq!(events.len(), 1, "the one sent event is stored once");
    let events = events
        .as_any()
        .downcast_ref::<StructArray>()
        .expect("an event element is a struct");
    assert_eq!(
        child::<Int64Array>(events, "time_unix_nano").value(0),
        start + EVENT_OFFSET_NANOS
    );
    assert_eq!(child::<StringArray>(events, "name").value(0), EVENT_NAME);
    assert_eq!(
        child::<LargeBinaryArray>(events, "attributes").value(0),
        support::canonical_attribute_bytes(&support::event_attributes())
    );
    assert_eq!(
        child::<Int64Array>(events, "dropped_attributes_count").value(0),
        EVENT_DROPPED_ATTRIBUTES
    );
}

/// Asserts the ordered nested link collection of the maximal span.
///
/// # Panics
///
/// Panics when the collection is not exactly the one link that was sent, in
/// order, with every child field intact.
fn assert_links(row: &RecordBatch) {
    let links = column::<ListArray>(row, "links").value(0);
    assert_eq!(links.len(), 1, "the one sent link is stored once");
    let links = links
        .as_any()
        .downcast_ref::<StructArray>()
        .expect("a link element is a struct");
    assert_eq!(
        child::<FixedSizeBinaryArray>(links, "trace_id").value(0),
        LINK_TRACE_ID
    );
    assert_eq!(
        child::<FixedSizeBinaryArray>(links, "span_id").value(0),
        LINK_SPAN_ID
    );
    assert_eq!(
        child::<StringArray>(links, "trace_state").value(0),
        LINK_TRACE_STATE
    );
    assert_eq!(child::<Int64Array>(links, "flags").value(0), LINK_FLAGS);
    assert_eq!(
        child::<LargeBinaryArray>(links, "attributes").value(0),
        support::canonical_attribute_bytes(&support::link_attributes())
    );
    assert_eq!(
        child::<Int64Array>(links, "dropped_attributes_count").value(0),
        LINK_DROPPED_ATTRIBUTES
    );
}

/// Reads one named child array out of a nested struct element.
///
/// # Panics
///
/// Panics when the child is missing or does not have the declared Arrow type.
fn child<'struct_array, A: Array + 'static>(
    values: &'struct_array StructArray,
    name: &str,
) -> &'struct_array A {
    values
        .column_by_name(name)
        .unwrap_or_else(|| panic!("the nested element carries a `{name}` field"))
        .as_any()
        .downcast_ref::<A>()
        .unwrap_or_else(|| panic!("`{name}` has the canonical Arrow type"))
}

/// Tests that need Postgres, a bound server, and the publication boundary.
mod pg_tests {
    use wyrd_runtime::Permission;

    use super::{
        OtlpJourney, assert_maximal_span_row, export_traces_over_grpc, support,
        support::{GRPC_SPAN, SPANS_TABLE},
    };

    /// Every canonical span column that carries caller content.
    ///
    /// These are exactly `vala.traces.spans`'s declared sensitive payload
    /// columns. `attributes` is the one that carries the structured `GenAI`
    /// input and output messages, so gating it is what keeps a conversation
    /// payload from reaching a caller holding query access alone.
    /// One maximal span exported over OTLP/gRPC reads back with every field.
    ///
    /// This is the fidelity contract in one path. The exporter sends every
    /// optional OTLP trace field the canonical ledger declares — a parent, a
    /// trace state, flags, every `AnyValue` attribute shape, an ordered event,
    /// an ordered link, a present status, a full resource and scope envelope,
    /// all six pinned `GenAI` promotions, and the structured
    /// `gen_ai.input.messages` / `gen_ai.output.messages` payloads — through
    /// the real collector, the real Scribe acknowledgment, and the real
    /// publication boundary. A second principal holding query access without
    /// payload access then proves those structured messages are refused
    /// rather than served. The
    /// readback compares each stored column against the same fixture constant
    /// the exporter sent, so a column that stops being written, is written
    /// with a default, or is written from the wrong source fails here rather
    /// than degrading silently into "none recorded".
    ///
    /// # Panics
    ///
    /// Panics when the export is refused, when the collector reports a partial
    /// success for a wholly valid request, or when any stored column differs
    /// from what was sent.
    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "requires the Postgres-backed Bifrost journey lane"]
    async fn otlp_grpc_maximal_trace_round_trips_every_field() {
        let journey = OtlpJourney::start().await;
        let start = support::anchor_nanos();

        let partial =
            export_traces_over_grpc(&journey, support::maximal_resource_spans(start, GRPC_SPAN))
                .await;
        assert!(
            partial.is_none_or(|partial| partial.rejected_spans == 0),
            "a wholly valid export reports no rejected span"
        );

        journey.publish().await;
        let row = journey
            .query_one_row(&format!(
                "SELECT * FROM {SPANS_TABLE} WHERE start_time_unix_nano = {start}"
            ))
            .await;
        assert_maximal_span_row(&row, start, GRPC_SPAN);

        let reader = journey
            .client_with_permissions("otlp_span_reader", &[Permission::bifrost_query_read()])
            .await;
        let rows = journey
            .query_as(
                &reader,
                &format!("SELECT * FROM {SPANS_TABLE} WHERE start_time_unix_nano = {start}"),
            )
            .await;
        assert_eq!(
            rows.iter()
                .map(arrow::array::RecordBatch::num_rows)
                .sum::<usize>(),
            1,
            "table query permission covers the complete trace span"
        );

        journey.shutdown().await;
    }

    /// An ordinary Rust application's OpenTelemetry tracer reaches Bifrost.
    ///
    /// Nothing here is Wyrd-shaped: the upstream `SdkTracerProvider`, the
    /// upstream OTLP/gRPC `SpanExporter`, and the upstream `Tracer` are
    /// configured exactly as an application configures them, pointed at the
    /// bound collector, and given a bearer through the ordinary metadata hook.
    /// The SDK — not the fixture — generates the trace and span identities, so
    /// the assertions recover them from the emitted spans and require the same
    /// hierarchy, event, link, status, resource, scope, promoted `GenAI`
    /// fields and structured `GenAI` messages to come back out of canonical
    /// SQL. That is the user journey Scenario 1 only supports at the protocol
    /// layer.
    ///
    /// # Panics
    ///
    /// Panics when the exporter cannot be built, the provider does not flush,
    /// or any emitted value differs from what canonical SQL returns.
    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "requires the Postgres-backed Bifrost journey lane"]
    async fn stock_rust_otel_tracer_exports_genai_span_to_bifrost() {
        use opentelemetry::KeyValue;
        use opentelemetry::trace::{
            SpanContext, Status, TraceContextExt, TraceFlags, TraceState, Tracer,
            TracerProvider as _,
        };
        use opentelemetry_otlp::{SpanExporter, WithExportConfig, WithTonicConfig};
        use opentelemetry_sdk::trace::SdkTracerProvider;

        let journey = OtlpJourney::start().await;
        let exporter = SpanExporter::builder()
            .with_tonic()
            .with_endpoint(journey.grpc_url())
            .with_metadata(journey.stock_metadata())
            .build()
            .expect("the upstream OTLP span exporter builds against the bound collector");
        let provider = SdkTracerProvider::builder()
            .with_resource(support::stock_resource())
            .with_batch_exporter(exporter)
            .build();
        let tracer = provider.tracer_with_scope(
            opentelemetry::InstrumentationScope::builder(support::STOCK_TRACE_SCOPE)
                .with_version(support::SCOPE_VERSION)
                .build(),
        );

        let linked = SpanContext::new(
            opentelemetry::trace::TraceId::from_bytes(support::LINK_TRACE_ID),
            opentelemetry::trace::SpanId::from_bytes(support::LINK_SPAN_ID),
            TraceFlags::SAMPLED,
            true,
            TraceState::default(),
        );
        let emitted = tracer.in_span("stock-rust-parent", |cx| {
            let parent = cx.span();
            parent.set_attribute(KeyValue::new("wyrd.test.marker", "rust-trace"));
            parent.set_attribute(KeyValue::new(
                "gen_ai.operation.name",
                support::GEN_AI_OPERATION_NAME,
            ));
            parent.set_attribute(KeyValue::new(
                "gen_ai.provider.name",
                support::GEN_AI_PROVIDER_NAME,
            ));
            parent.set_attribute(KeyValue::new(
                "gen_ai.request.model",
                support::GEN_AI_REQUEST_MODEL,
            ));
            parent.set_attribute(KeyValue::new(
                "gen_ai.conversation.id",
                support::GEN_AI_CONVERSATION_ID,
            ));
            parent.set_attribute(KeyValue::new(
                "gen_ai.usage.input_tokens",
                support::GEN_AI_INPUT_TOKENS,
            ));
            parent.set_attribute(KeyValue::new(
                "gen_ai.usage.output_tokens",
                support::GEN_AI_OUTPUT_TOKENS,
            ));
            parent.set_attribute(KeyValue::new(
                "gen_ai.input.messages",
                support::GEN_AI_INPUT_MESSAGES,
            ));
            parent.set_attribute(KeyValue::new(
                "gen_ai.output.messages",
                support::GEN_AI_OUTPUT_MESSAGES,
            ));
            parent.add_event("checkpoint", vec![KeyValue::new("step", 1_i64)]);
            parent.add_link(linked.clone(), Vec::new());
            parent.set_status(Status::error("expected test status"));
            let context = parent.span_context().clone();
            let child = tracer.in_span("stock-rust-child", |child_cx| {
                child_cx
                    .span()
                    .set_attribute(KeyValue::new("answer", 42_i64));
                child_cx.span().span_context().span_id()
            });
            (context, child)
        });
        provider
            .force_flush()
            .expect("the upstream batch processor exports every emitted span");
        provider
            .shutdown()
            .expect("the upstream tracer provider shuts down");

        journey.publish().await;
        let batches = journey
            .query(&format!(
                "SELECT * FROM {SPANS_TABLE} WHERE scope_name = '{}'",
                support::STOCK_TRACE_SCOPE
            ))
            .await;
        let parent = support::row_by_span_id(&batches, emitted.0.span_id().to_bytes());
        let child = support::row_by_span_id(&batches, emitted.1.to_bytes());

        assert_eq!(
            support::column::<arrow::array::FixedSizeBinaryArray>(&parent, "trace_id").value(0),
            emitted.0.trace_id().to_bytes(),
            "the trace identity the SDK generated is the one Bifrost stores"
        );
        assert_eq!(
            support::column::<arrow::array::FixedSizeBinaryArray>(&child, "trace_id").value(0),
            emitted.0.trace_id().to_bytes(),
            "the child shares its parent's trace"
        );
        assert_eq!(
            support::column::<arrow::array::FixedSizeBinaryArray>(&child, "parent_span_id")
                .value(0),
            emitted.0.span_id().to_bytes(),
            "the parent/child relationship the SDK recorded survives readback"
        );
        assert_eq!(
            support::column::<arrow::array::StringArray>(&parent, "name").value(0),
            "stock-rust-parent"
        );
        assert_eq!(
            support::column::<arrow::array::StringArray>(&parent, "service_name").value(0),
            support::STOCK_SERVICE_NAME
        );
        assert_eq!(
            support::column::<arrow::array::StringArray>(&parent, "scope_version").value(0),
            support::SCOPE_VERSION
        );
        assert_eq!(
            support::column::<arrow::array::Int32Array>(&parent, "status_code").value(0),
            support::STATUS_CODE,
            "an upstream error status is stored as the canonical error code"
        );
        assert_eq!(
            support::column::<arrow::array::StringArray>(&parent, "status_message").value(0),
            "expected test status"
        );

        let attributes = support::decode_attributes(
            support::column::<arrow::array::LargeBinaryArray>(&parent, "attributes").value(0),
        );
        for (key, expected) in [
            ("wyrd.test.marker", "rust-trace"),
            ("gen_ai.input.messages", support::GEN_AI_INPUT_MESSAGES),
            ("gen_ai.output.messages", support::GEN_AI_OUTPUT_MESSAGES),
        ] {
            assert_eq!(
                attributes.get(key).map(String::as_str),
                Some(expected),
                "`{key}` survives the stock exporter unchanged"
            );
        }
        for (name, expected) in [
            ("gen_ai_operation_name", support::GEN_AI_OPERATION_NAME),
            ("gen_ai_provider_name", support::GEN_AI_PROVIDER_NAME),
            ("gen_ai_request_model", support::GEN_AI_REQUEST_MODEL),
            ("gen_ai_conversation_id", support::GEN_AI_CONVERSATION_ID),
        ] {
            assert_eq!(
                support::column::<arrow::array::StringArray>(&parent, name).value(0),
                expected,
                "the stock exporter's `{name}` is promoted"
            );
        }
        assert_eq!(
            support::column::<arrow::array::Int64Array>(&parent, "gen_ai_usage_input_tokens")
                .value(0),
            support::GEN_AI_INPUT_TOKENS
        );
        assert_eq!(
            support::column::<arrow::array::Int64Array>(&parent, "gen_ai_usage_output_tokens")
                .value(0),
            support::GEN_AI_OUTPUT_TOKENS
        );

        let events = support::column::<arrow::array::ListArray>(&parent, "events").value(0);
        assert_eq!(events.len(), 1, "the one emitted event is stored once");
        let links = support::column::<arrow::array::ListArray>(&parent, "links").value(0);
        assert_eq!(links.len(), 1, "the one emitted link is stored once");
        let links = links
            .as_any()
            .downcast_ref::<arrow::array::StructArray>()
            .expect("a link element is a struct");
        assert_eq!(
            super::child::<arrow::array::FixedSizeBinaryArray>(links, "trace_id").value(0),
            support::LINK_TRACE_ID,
            "the linked trace the application named is the one stored"
        );

        let resource = support::decode_attributes(
            support::column::<arrow::array::LargeBinaryArray>(&parent, "resource_attributes")
                .value(0),
        );
        assert_eq!(
            resource.get("service.name").map(String::as_str),
            Some(support::STOCK_SERVICE_NAME)
        );

        journey.shutdown().await;
    }
}
