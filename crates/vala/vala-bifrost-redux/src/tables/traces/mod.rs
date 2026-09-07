//! Canonical trace-signal table definitions and their table-owned projection.

mod events;
mod links;
pub mod projection;
pub mod spans;

pub use events::EventsTable;
pub use links::LinksTable;
pub use projection::{canonical_span_schema, project_resource_spans};
pub use spans::SpansTable;

#[cfg(test)]
mod tests {
    use arrow::array::{
        Array, BinaryArray, BooleanArray, FixedSizeBinaryArray, Int32Array, Int64Array, ListArray,
        StringArray, StructArray,
    };
    use arrow::record_batch::RecordBatch;
    use std::sync::Arc;
    use wyrd_tonic::otlp::common::v1::any_value::Value;
    use wyrd_tonic::otlp::common::v1::{
        AnyValue, EntityRef, InstrumentationScope, KeyValue, KeyValueList,
    };
    use wyrd_tonic::otlp::resource::v1::Resource;
    use wyrd_tonic::otlp::trace::v1::{ResourceSpans, ScopeSpans, Span, Status, span};

    use super::spans::SPAN_FIELDS;
    use super::{canonical_span_schema, project_resource_spans};
    use crate::tables::fields::{PARQUET_FIELD_ID, WYRD_SENSITIVE};
    use crate::tables::signal::{encode_attributes, validate_canonical_user_batch};

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
        ]
    }

    /// Build the maximal supported resource-span fixture.
    fn maximal_resource_spans(attributes: Vec<KeyValue>) -> Vec<ResourceSpans> {
        vec![ResourceSpans {
            resource: Some(Resource {
                attributes: vec![
                    attribute("service.name", Value::StringValue("checkout".to_owned())),
                    attribute("host.id", Value::IntValue(7)),
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
    /// Panics when any canonical value, order, presence distinction, stable id,
    /// sensitivity marker, or name-bound validation result differs.
    #[test]
    fn maximal_span_projection_is_lossless_and_identity_mapped() {
        let attributes = maximal_span_attributes();
        let request = maximal_resource_spans(attributes.clone());
        let (batch, outcome) = project_resource_spans(&request).expect("maximal span projects");

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
        assert_eq!(
            typed::<BinaryArray>(&batch, "attributes").value(0),
            encode_attributes(&attributes).as_slice()
        );
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
        for declared in SPAN_FIELDS {
            let field = batch
                .schema()
                .field_with_name(declared.name)
                .expect("every declared field is present")
                .clone();
            assert_eq!(
                field.metadata().get(PARQUET_FIELD_ID),
                Some(&declared.id.to_string()),
                "{} carries its stable id",
                declared.name
            );
            assert_eq!(
                field.metadata().get(WYRD_SENSITIVE),
                Some(&declared.class.is_sensitive().to_string()),
                "{} carries its sensitivity",
                declared.name
            );
        }

        let permuted = permute(&batch);
        let revalidated =
            validate_canonical_user_batch(SPAN_FIELDS, &permuted).expect("names bind, not indexes");
        assert_eq!(revalidated.schema(), canonical_span_schema());
        assert_eq!(revalidated, batch);
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
        assert_eq!(events.len(), 2);
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
            .and_then(|column| column.as_any().downcast_ref::<BinaryArray>())
            .expect("event attribute column");
        assert_eq!(
            event_attributes.value(0),
            encode_attributes(&[attribute("index", Value::IntValue(0))]).as_slice()
        );
        assert!(
            event_attributes.value(1).is_empty(),
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
    /// Panics when a presence bit, context scalar, entity-reference count, or
    /// promoted `GenAI` value differs.
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
            .downcast_ref::<BinaryArray>()
            .expect("entity refs are binary");
        assert_eq!(entity_refs.len(), 2);
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
        let (batch, outcome) = project_resource_spans(&request).expect("projection completes");

        assert_eq!(outcome.accepted_spans, 0);
        assert_eq!(outcome.rejected_spans, 1);
        assert_eq!(batch.num_rows(), 0);
        assert_eq!(
            outcome.rejection_message.as_deref(),
            Some("a promoted gen_ai attribute has the wrong type")
        );
    }
}
