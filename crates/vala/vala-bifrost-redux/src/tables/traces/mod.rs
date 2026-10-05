//! Canonical trace-signal table definitions and their table-owned projection.

pub mod projection;
pub mod spans;

pub use projection::{SpanOutputWidths, canonical_span_schema, project_resource_spans};
pub use spans::SpansTable;

#[cfg(test)]
mod tests {
    use arrow::array::{
        Array, BooleanArray, FixedSizeBinaryArray, Int32Array, Int64Array, ListArray, StringArray,
        StructArray,
    };
    use arrow::record_batch::RecordBatch;
    use serde_json::json;
    use std::sync::Arc;
    use wyrd_queue::variant::variant_cell_to_json;
    use wyrd_tonic::otlp::common::v1::any_value::Value;
    use wyrd_tonic::otlp::common::v1::{
        AnyValue, EntityRef, InstrumentationScope, KeyValue, KeyValueList,
    };
    use wyrd_tonic::otlp::resource::v1::Resource;
    use wyrd_tonic::otlp::trace::v1::{ResourceSpans, ScopeSpans, Span, Status, span};

    use super::spans::SPAN_FIELDS;
    use super::{canonical_span_schema, project_resource_spans};
    use crate::tables::fields::{PARQUET_FIELD_ID, WYRD_SENSITIVE};
    use crate::tables::signal::{validate_canonical_user_batch, without_correlation_columns};

    /// Build one attribute entry with the supplied protocol value.
    fn attribute(key: &str, value: Value) -> KeyValue {
        KeyValue {
            key: key.to_owned(),
            value: Some(AnyValue { value: Some(value) }),
        }
    }

    /// The span attributes exercised by the maximal fixture.
    ///
    /// Deliberately mixes a byte-bearing value, a nested key-value list, a
    /// signed integer, and the pinned `GenAI` promotion sources so the canonical
    /// payload proves it retains every `AnyValue` form.
    fn maximal_span_attributes() -> Vec<KeyValue> {
        vec![
            attribute("payload.bytes", Value::BytesValue(vec![0x00, 0xff, 0x7f])),
            attribute(
                "payload.nested",
                Value::KvlistValue(KeyValueList {
                    values: vec![attribute("inner", Value::DoubleValue(1.5))],
                }),
            ),
            attribute(
                "gen_ai.operation.name",
                Value::StringValue("chat".to_owned()),
            ),
            attribute(
                "gen_ai.provider.name",
                Value::StringValue("anthropic".to_owned()),
            ),
            attribute(
                "gen_ai.request.model",
                Value::StringValue("claude".to_owned()),
            ),
            attribute(
                "gen_ai.conversation.id",
                Value::StringValue("conversation-1".to_owned()),
            ),
            attribute("gen_ai.usage.input_tokens", Value::IntValue(1_024)),
            attribute("gen_ai.usage.output_tokens", Value::IntValue(-1)),
            attribute("http.request.method", Value::StringValue("POST".to_owned())),
            attribute("http.route", Value::StringValue("/chat".to_owned())),
            attribute("http.response.status_code", Value::IntValue(502)),
            attribute(
                "url.full",
                Value::StringValue("https://api/chat".to_owned()),
            ),
        ]
    }

    /// The span event whose `exception.*` attributes the span promotes.
    ///
    /// Its stack trace is deliberately not a string, so the promotion is null.
    fn exception_event() -> span::Event {
        span::Event {
            time_unix_nano: 1_700_000_000_000_000_300,
            name: "exception".to_owned(),
            attributes: vec![
                attribute(
                    "exception.type",
                    Value::StringValue("TimeoutError".to_owned()),
                ),
                attribute(
                    "exception.message",
                    Value::StringValue("upstream timed out".to_owned()),
                ),
                attribute("exception.stacktrace", Value::IntValue(3)),
            ],
            dropped_attributes_count: 0,
        }
    }

    /// Build the maximal supported resource-span fixture.
    fn maximal_resource_spans(attributes: Vec<KeyValue>) -> Vec<ResourceSpans> {
        vec![ResourceSpans {
            resource: Some(Resource {
                attributes: vec![
                    attribute("service.name", Value::StringValue("checkout".to_owned())),
                    attribute("host.id", Value::IntValue(7)),
                    attribute("service.version", Value::StringValue("1.4.0".to_owned())),
                    attribute(
                        "deployment.environment",
                        Value::StringValue("legacy".to_owned()),
                    ),
                    attribute(
                        "deployment.environment.name",
                        Value::StringValue("prod".to_owned()),
                    ),
                ],
                dropped_attributes_count: 3,
                entity_refs: vec![
                    EntityRef {
                        schema_url: "https://schemas/entity/1".to_owned(),
                        r#type: "service".to_owned(),
                        id_keys: vec!["service.name".to_owned()],
                        description_keys: vec!["host.id".to_owned()],
                    },
                    EntityRef {
                        schema_url: String::new(),
                        r#type: "host".to_owned(),
                        id_keys: vec!["host.id".to_owned()],
                        description_keys: Vec::new(),
                    },
                ],
            }),
            schema_url: "https://schemas/resource/1".to_owned(),
            scope_spans: vec![ScopeSpans {
                scope: Some(InstrumentationScope {
                    name: "wyrd.tracer".to_owned(),
                    version: "1.2.3".to_owned(),
                    attributes: vec![attribute("scope.kind", Value::BoolValue(true))],
                    dropped_attributes_count: 5,
                }),
                schema_url: "https://schemas/scope/1".to_owned(),
                spans: vec![Span {
                    trace_id: vec![1; 16],
                    span_id: vec![2; 8],
                    trace_state: "vendor=value".to_owned(),
                    parent_span_id: vec![3; 8],
                    flags: 0x0000_0101,
                    name: "POST /chat".to_owned(),
                    kind: span::SpanKind::Server as i32,
                    start_time_unix_nano: 1_700_000_000_000_000_000,
                    end_time_unix_nano: 1_700_000_000_000_000_500,
                    attributes,
                    dropped_attributes_count: 11,
                    events: vec![
                        span::Event {
                            time_unix_nano: 1_700_000_000_000_000_100,
                            name: "first".to_owned(),
                            attributes: vec![attribute("index", Value::IntValue(0))],
                            dropped_attributes_count: 1,
                        },
                        span::Event {
                            time_unix_nano: 1_700_000_000_000_000_200,
                            name: "second".to_owned(),
                            attributes: Vec::new(),
                            dropped_attributes_count: 0,
                        },
                        exception_event(),
                    ],
                    dropped_events_count: 13,
                    links: vec![
                        span::Link {
                            trace_id: vec![4; 16],
                            span_id: vec![5; 8],
                            trace_state: "linked=1".to_owned(),
                            attributes: vec![attribute("rel", Value::StringValue("a".to_owned()))],
                            dropped_attributes_count: 2,
                            flags: 0x0000_0201,
                        },
                        span::Link {
                            trace_id: vec![6; 16],
                            span_id: vec![7; 8],
                            trace_state: String::new(),
                            attributes: Vec::new(),
                            dropped_attributes_count: 0,
                            flags: 0,
                        },
                    ],
                    dropped_links_count: 17,
                    status: Some(Status {
                        message: "upstream refused".to_owned(),
                        code: 2,
                    }),
                }],
            }],
        }]
    }

    /// Read one named column, panicking with the column name when absent.
    fn column<'a>(batch: &'a RecordBatch, name: &str) -> &'a dyn Array {
        batch
            .column_by_name(name)
            .unwrap_or_else(|| panic!("canonical span batch has column {name}"))
            .as_ref()
    }

    /// Downcast one named column, panicking when its Arrow type differs.
    fn typed<'a, T: 'static>(batch: &'a RecordBatch, name: &str) -> &'a T {
        column(batch, name)
            .as_any()
            .downcast_ref::<T>()
            .unwrap_or_else(|| panic!("column {name} has the declared Arrow type"))
    }

    /// A maximal span projects into exactly one lossless, identity-mapped row.
    ///
    /// # Panics
    ///
    /// Panics when any canonical value, order, presence distinction, declared field id,
    /// sensitivity marker, or name-bound validation result differs.
    #[test]
    fn maximal_span_projection_is_lossless_and_identity_mapped() {
        let attributes = maximal_span_attributes();
        let request = maximal_resource_spans(attributes.clone());
        let (batch, outcome) =
            project_resource_spans(&request, None, usize::MAX).expect("maximal span projects");

        assert_eq!(outcome.accepted_spans, 1);
        assert_eq!(outcome.rejected_spans, 0);
        assert_eq!(outcome.rejection_message, None);
        assert_eq!(batch.num_rows(), 1);

        assert_eq!(
            typed::<FixedSizeBinaryArray>(&batch, "trace_id").value(0),
            &[1; 16]
        );
        assert_eq!(
            typed::<FixedSizeBinaryArray>(&batch, "span_id").value(0),
            &[2; 8]
        );
        assert_eq!(
            typed::<FixedSizeBinaryArray>(&batch, "parent_span_id").value(0),
            &[3; 8]
        );
        assert_eq!(
            typed::<StringArray>(&batch, "trace_state").value(0),
            "vendor=value"
        );
        assert_eq!(typed::<Int64Array>(&batch, "flags").value(0), 0x0000_0101);
        assert_eq!(typed::<StringArray>(&batch, "name").value(0), "POST /chat");
        assert_eq!(
            typed::<Int32Array>(&batch, "kind").value(0),
            span::SpanKind::Server as i32
        );
        assert_eq!(
            typed::<Int64Array>(&batch, "start_time_unix_nano").value(0),
            1_700_000_000_000_000_000
        );
        assert_eq!(
            typed::<Int64Array>(&batch, "end_time_unix_nano").value(0),
            1_700_000_000_000_000_500
        );
        assert_eq!(typed::<Int64Array>(&batch, "duration_nano").value(0), 500);
        assert!(typed::<BooleanArray>(&batch, "status_present").value(0));
        assert_eq!(typed::<Int32Array>(&batch, "status_code").value(0), 2);
        assert_eq!(
            typed::<StringArray>(&batch, "status_message").value(0),
            "upstream refused"
        );
        assert_span_attributes(&batch);
        assert_eq!(
            typed::<Int64Array>(&batch, "dropped_attributes_count").value(0),
            11
        );
        assert_eq!(
            typed::<Int64Array>(&batch, "dropped_events_count").value(0),
            13
        );
        assert_eq!(
            typed::<Int64Array>(&batch, "dropped_links_count").value(0),
            17
        );

        assert_span_children(&batch);
        assert_span_context_and_promotions(&batch);
        assert_semantic_promotions(&batch);
        for declared in SPAN_FIELDS {
            let field = batch
                .schema()
                .field_with_name(declared.name)
                .expect("every declared field is present")
                .clone();
            assert_eq!(
                field.metadata().get(PARQUET_FIELD_ID),
                None,
                "{} declares no id; the registered table assigns it",
                declared.name
            );
            assert_eq!(
                field.metadata().get(WYRD_SENSITIVE),
                Some(&declared.class.is_sensitive().to_string()),
                "{} carries its sensitivity",
                declared.name
            );
        }

        let card_refs = typed::<StringArray>(&batch, "card_ref");
        let run_ids = typed::<StringArray>(&batch, "run_id");
        assert!(
            card_refs.is_null(0) && run_ids.is_null(0),
            "a span with no correlation attributes projects null correlation"
        );

        let ledger = without_correlation_columns(&batch)
            .expect("the appended correlation columns split off cleanly");
        let permuted = permute(&ledger);
        let revalidated =
            validate_canonical_user_batch(SPAN_FIELDS, &permuted).expect("names bind, not indexes");
        assert_eq!(revalidated.schema(), canonical_span_schema());
        assert_eq!(revalidated, ledger);
    }

    /// Assert the span attributes decode to every typed source value.
    ///
    /// # Panics
    ///
    /// Panics when a value changes type, an integer is not exact, or bytes do
    /// not round-trip.
    fn assert_span_attributes(batch: &RecordBatch) {
        assert_eq!(
            variant_cell_to_json(column(batch, "attributes"), 0).expect("attributes decode"),
            json!({
                "payload.bytes": "AP9/",
                "payload.nested": {"inner": 1.5},
                "gen_ai.operation.name": "chat",
                "gen_ai.provider.name": "anthropic",
                "gen_ai.request.model": "claude",
                "gen_ai.conversation.id": "conversation-1",
                "gen_ai.usage.input_tokens": 1_024,
                "gen_ai.usage.output_tokens": -1,
                "http.request.method": "POST",
                "http.route": "/chat",
                "http.response.status_code": 502,
                "url.full": "https://api/chat",
            })
        );
    }

    /// Assert the nested event and link collections survive intact.
    ///
    /// # Panics
    ///
    /// Panics when a nested event or link value, order, or present-but-empty
    /// payload differs.
    fn assert_span_children(batch: &RecordBatch) {
        let events = typed::<ListArray>(batch, "events").value(0);
        let events = events
            .as_any()
            .downcast_ref::<StructArray>()
            .expect("events are structs");
        assert_eq!(events.len(), 3);
        let event_names = events
            .column_by_name("name")
            .and_then(|column| column.as_any().downcast_ref::<StringArray>())
            .expect("event name column");
        assert_eq!(event_names.value(0), "first");
        assert_eq!(event_names.value(1), "second");
        let event_times = events
            .column_by_name("time_unix_nano")
            .and_then(|column| column.as_any().downcast_ref::<Int64Array>())
            .expect("event time column");
        assert_eq!(event_times.value(0), 1_700_000_000_000_000_100);
        assert_eq!(event_times.value(1), 1_700_000_000_000_000_200);
        let event_attributes = events
            .column_by_name("attributes")
            .expect("event attribute column");
        assert_eq!(
            variant_cell_to_json(event_attributes.as_ref(), 0).expect("event attributes decode"),
            json!({"index": 0})
        );
        assert_eq!(
            variant_cell_to_json(event_attributes.as_ref(), 1).expect("event attributes decode"),
            json!({}),
            "an empty event attribute collection stays present and empty"
        );

        let links = typed::<ListArray>(batch, "links").value(0);
        let links = links
            .as_any()
            .downcast_ref::<StructArray>()
            .expect("links are structs");
        assert_eq!(links.len(), 2);
        let linked_traces = links
            .column_by_name("trace_id")
            .and_then(|column| column.as_any().downcast_ref::<FixedSizeBinaryArray>())
            .expect("link trace column");
        assert_eq!(linked_traces.value(0), &[4; 16]);
        assert_eq!(linked_traces.value(1), &[6; 16]);
        let link_flags = links
            .column_by_name("flags")
            .and_then(|column| column.as_any().downcast_ref::<Int64Array>())
            .expect("link flag column");
        assert_eq!(link_flags.value(0), 0x0000_0201);
    }

    /// Assert resource, scope, and pinned promotion columns are exact.
    ///
    /// # Panics
    ///
    /// Panics when a presence bit, context scalar, entity reference, or
    /// promoted value differs.
    fn assert_span_context_and_promotions(batch: &RecordBatch) {
        assert!(typed::<BooleanArray>(batch, "resource_present").value(0));
        assert_eq!(
            typed::<Int64Array>(batch, "resource_dropped_attributes_count").value(0),
            3
        );
        assert_eq!(
            typed::<StringArray>(batch, "resource_schema_url").value(0),
            "https://schemas/resource/1"
        );
        let entity_refs = typed::<ListArray>(batch, "resource_entity_refs").value(0);
        let entity_refs = entity_refs
            .as_any()
            .downcast_ref::<StructArray>()
            .expect("entity refs are structs");
        assert_eq!(entity_refs.len(), 2);
        let entity_types = entity_refs
            .column_by_name("type")
            .and_then(|column| column.as_any().downcast_ref::<StringArray>())
            .expect("entity type column");
        assert_eq!(entity_types.value(0), "service");
        assert_eq!(entity_types.value(1), "host");
        let description_keys = entity_refs
            .column_by_name("description_keys")
            .and_then(|column| column.as_any().downcast_ref::<ListArray>())
            .expect("entity description keys column");
        assert_eq!(description_keys.value(0).len(), 1);
        assert_eq!(
            description_keys.value(1).len(),
            0,
            "an empty key list stays present and empty"
        );
        assert_eq!(
            variant_cell_to_json(column(batch, "resource_attributes"), 0)
                .expect("resource attributes decode")["host.id"],
            json!(7)
        );
        assert!(typed::<BooleanArray>(batch, "scope_present").value(0));
        assert_eq!(
            typed::<StringArray>(batch, "scope_name").value(0),
            "wyrd.tracer"
        );
        assert_eq!(
            typed::<StringArray>(batch, "scope_version").value(0),
            "1.2.3"
        );
        assert_eq!(
            typed::<Int64Array>(batch, "scope_dropped_attributes_count").value(0),
            5
        );
        assert_eq!(
            typed::<StringArray>(batch, "scope_schema_url").value(0),
            "https://schemas/scope/1"
        );

        assert_eq!(
            typed::<StringArray>(batch, "service_name").value(0),
            "checkout"
        );
        assert_eq!(
            typed::<StringArray>(batch, "gen_ai_operation_name").value(0),
            "chat"
        );
        assert_eq!(
            typed::<StringArray>(batch, "gen_ai_provider_name").value(0),
            "anthropic"
        );
        assert_eq!(
            typed::<StringArray>(batch, "gen_ai_request_model").value(0),
            "claude"
        );
        assert_eq!(
            typed::<StringArray>(batch, "gen_ai_conversation_id").value(0),
            "conversation-1"
        );
        assert_eq!(
            typed::<Int64Array>(batch, "gen_ai_usage_input_tokens").value(0),
            1_024
        );
        assert_eq!(
            typed::<Int64Array>(batch, "gen_ai_usage_output_tokens").value(0),
            -1
        );
    }

    /// Assert the resource, HTTP, and exception semantic-convention columns.
    ///
    /// # Panics
    ///
    /// Panics when a promoted value differs or a wrongly typed source is not
    /// promoted to null.
    fn assert_semantic_promotions(batch: &RecordBatch) {
        for (name, expected) in [
            ("service_version", Some("1.4.0")),
            ("deployment_environment", Some("prod")),
            ("http_request_method", Some("POST")),
            ("http_route", Some("/chat")),
            ("url_full", Some("https://api/chat")),
            ("exception_type", Some("TimeoutError")),
            ("exception_message", Some("upstream timed out")),
            ("exception_stacktrace", None),
        ] {
            let values = typed::<StringArray>(batch, name);
            assert_eq!(
                values.is_valid(0).then(|| values.value(0)),
                expected,
                "{name} promotes its convention, or null for a wrongly typed source"
            );
        }
        assert_eq!(
            typed::<Int64Array>(batch, "http_response_status_code").value(0),
            502
        );
    }

    /// Reverse a batch's column order without changing any value.
    fn permute(batch: &RecordBatch) -> RecordBatch {
        let schema = batch.schema();
        let mut fields: Vec<_> = schema.fields().iter().map(Arc::clone).collect();
        let mut columns: Vec<_> = batch.columns().to_vec();
        fields.reverse();
        columns.reverse();
        RecordBatch::try_new(Arc::new(arrow::datatypes::Schema::new(fields)), columns)
            .expect("a reversed batch still assembles")
    }

    /// Build one request whose spans differ only in their attribute collections.
    ///
    /// Every other span field is copied from the maximal fixture, so a test can
    /// vary correlation attributes alone and still exercise a complete span.
    fn spans_with_attribute_sets(sets: Vec<Vec<KeyValue>>) -> Vec<ResourceSpans> {
        let mut request = maximal_resource_spans(Vec::new());
        let template = request[0].scope_spans[0].spans[0].clone();
        request[0].scope_spans[0].spans = sets
            .into_iter()
            .enumerate()
            .map(|(index, attributes)| Span {
                span_id: vec![u8::try_from(index + 1).expect("fixture index is small"); 8],
                attributes,
                ..template.clone()
            })
            .collect();
        request
    }

    /// Optional Card correlation is read from the final record attribute only,
    /// authorized against the principal's signed scope, rejects exactly its own
    /// span, and never disturbs the lossless payload.
    ///
    /// # Panics
    ///
    /// Panics when a missing value is not null, a final duplicate does not win,
    /// a wrongly typed, malformed, out-of-scope, or UID-less value rejects more
    /// than its own span, an original attribute entry changes, or the outcome
    /// counts and first reason are not exact.
    #[test]
    fn optional_card_correlation_is_atomic_and_lossless() {
        use crate::tables::signal::correlation_fixture;
        const CARD: &str = correlation_fixture::IN_SCOPE;
        const RUN: &str = "01890f28-7c4a-7cc3-98e7-4f4a3c2d1b11";

        let missing = maximal_span_attributes();
        let mut valid = maximal_span_attributes();
        valid.push(attribute(
            "wyrd.card_ref",
            Value::StringValue(CARD.to_owned()),
        ));
        valid.push(attribute("wyrd.run_id", Value::StringValue(RUN.to_owned())));
        let mut duplicate = maximal_span_attributes();
        duplicate.push(attribute(
            "wyrd.card_ref",
            Value::StringValue("prod/Service/superseded@9.9.9".to_owned()),
        ));
        duplicate.push(attribute(
            "wyrd.card_ref",
            Value::StringValue(CARD.to_owned()),
        ));
        let mut wrong_typed = maximal_span_attributes();
        wrong_typed.push(attribute("wyrd.run_id", Value::IntValue(7)));
        let mut malformed = maximal_span_attributes();
        malformed.push(attribute(
            "wyrd.card_ref",
            Value::StringValue("not-a-card-ref".to_owned()),
        ));
        let mut out_of_scope = maximal_span_attributes();
        out_of_scope.push(attribute(
            "wyrd.card_ref",
            Value::StringValue(correlation_fixture::OUT_OF_SCOPE.to_owned()),
        ));
        let mut without_uid = maximal_span_attributes();
        without_uid.push(attribute(
            "wyrd.card_ref",
            Value::StringValue(correlation_fixture::WITHOUT_UID.to_owned()),
        ));

        let request = spans_with_attribute_sets(vec![
            missing.clone(),
            valid.clone(),
            duplicate.clone(),
            wrong_typed,
            malformed,
            out_of_scope,
            without_uid,
        ]);
        let (batch, outcome) =
            project_resource_spans(&request, Some(&correlation_fixture::scope()), usize::MAX)
                .expect("projection completes");

        assert_eq!(outcome.accepted_spans, 3);
        assert_eq!(
            outcome.rejected_spans, 4,
            "an out-of-scope and a UID-less reference each reject only their own span"
        );
        assert_eq!(
            outcome.rejection_message.as_deref(),
            Some("wyrd.run_id is not a valid run correlation"),
            "the first rejection in traversal order is reported"
        );
        assert_eq!(batch.num_rows(), 3, "only the two defective spans are lost");

        let card_refs = typed::<StringArray>(&batch, "card_ref");
        assert!(
            card_refs.is_null(0),
            "a missing wyrd.card_ref projects null"
        );
        assert_eq!(card_refs.value(1), CARD);
        assert_eq!(
            card_refs.value(2),
            CARD,
            "the final duplicate is authoritative"
        );
        let run_ids = typed::<StringArray>(&batch, "run_id");
        assert!(run_ids.is_null(0), "a missing wyrd.run_id projects null");
        assert_eq!(run_ids.value(1), RUN);
        assert!(run_ids.is_null(2));

        correlation_fixture::assert_attribute_rows(&batch, &[&missing, &valid, &duplicate]);
    }

    /// A wrongly typed pinned `GenAI` attribute rejects its whole span.
    ///
    /// # Panics
    ///
    /// Panics when the span is accepted, when a partial row survives, or when
    /// the outcome does not report exactly one rejection.
    #[test]
    fn wrong_genai_promotion_type_rejects_the_complete_span() {
        let mut attributes = maximal_span_attributes();
        attributes.push(attribute(
            "gen_ai.usage.input_tokens",
            Value::StringValue("1024".to_owned()),
        ));
        let request = maximal_resource_spans(attributes);
        let (batch, outcome) =
            project_resource_spans(&request, None, usize::MAX).expect("projection completes");

        assert_eq!(outcome.accepted_spans, 0);
        assert_eq!(outcome.rejected_spans, 1);
        assert_eq!(batch.num_rows(), 0);
        assert_eq!(
            outcome.rejection_message.as_deref(),
            Some("a promoted gen_ai attribute has the wrong type")
        );
    }
}
