//! Canonical log-signal table definition and its table-owned projection.

pub mod projection;
pub mod records;

pub use projection::{canonical_log_schema, project_resource_logs};
pub use records::RecordsTable;

#[cfg(test)]
mod tests {
    use arrow::array::{
        Array, BinaryArray, BooleanArray, FixedSizeBinaryArray, Int32Array, Int64Array, ListArray,
        StringArray,
    };
    use arrow::record_batch::RecordBatch;
    use std::sync::Arc;
    use wyrd_tonic::otlp::common::v1::any_value::Value;
    use wyrd_tonic::otlp::common::v1::{
        AnyValue, ArrayValue, EntityRef, InstrumentationScope, KeyValue, KeyValueList,
    };
    use wyrd_tonic::otlp::logs::v1::{LogRecord, ResourceLogs, ScopeLogs};
    use wyrd_tonic::otlp::resource::v1::Resource;

    use super::records::LOG_FIELDS;
    use super::{canonical_log_schema, project_resource_logs};
    use crate::tables::fields::{PARQUET_FIELD_ID, WYRD_SENSITIVE};
    use crate::tables::signal::{
        encode_any_value, encode_attributes, validate_canonical_user_batch,
    };

    /// Build one attribute entry with the supplied protocol value.
    fn attribute(key: &str, value: Value) -> KeyValue {
        KeyValue {
            key: key.to_owned(),
            value: Some(AnyValue { value: Some(value) }),
        }
    }

    /// Every `AnyValue` form the log body must retain bit-for-bit.
    ///
    /// The list deliberately covers each protocol variant, including the two
    /// recursive ones, so an accepted body proves the canonical encoding is not
    /// narrowed to a string or a display form.
    fn body_forms() -> Vec<Value> {
        vec![
            Value::StringValue("body text".to_owned()),
            Value::BoolValue(true),
            Value::IntValue(-9_007_199_254_740_993),
            Value::DoubleValue(f64::from_bits(0x7ff8_0000_0000_0001)),
            Value::BytesValue(vec![0x00, 0xff, 0x7f]),
            Value::ArrayValue(ArrayValue {
                values: vec![
                    AnyValue {
                        value: Some(Value::IntValue(1)),
                    },
                    AnyValue { value: None },
                ],
            }),
            Value::KvlistValue(KeyValueList {
                values: vec![attribute("inner", Value::DoubleValue(1.5))],
            }),
        ]
    }

    /// Build the maximal supported resource-log fixture.
    ///
    /// The first scope carries one correlated, fully populated record per body
    /// form; the trailing scope carries an uncorrelated record with an absent
    /// body, an absent resource, and an absent scope so presence and
    /// nullability distinctions are exercised in the same batch.
    fn maximal_resource_logs() -> Vec<ResourceLogs> {
        let correlated: Vec<LogRecord> = body_forms()
            .into_iter()
            .enumerate()
            .map(|(index, value)| LogRecord {
                time_unix_nano: 1_700_000_000_123_456_789,
                observed_time_unix_nano: 1_700_000_000_987_654_321,
                severity_number: 9,
                severity_text: "INFO".to_owned(),
                event_name: format!("wyrd.event.{index}"),
                body: Some(AnyValue { value: Some(value) }),
                attributes: vec![attribute("payload.bytes", Value::BytesValue(vec![0xab]))],
                dropped_attributes_count: 3,
                flags: 0xdead_beef,
                trace_id: vec![0x11; 16],
                span_id: vec![0x22; 8],
            })
            .collect();

        vec![
            ResourceLogs {
                resource: Some(Resource {
                    attributes: vec![attribute(
                        "service.name",
                        Value::StringValue("wyrd".to_owned()),
                    )],
                    dropped_attributes_count: 5,
                    entity_refs: vec![EntityRef {
                        schema_url: "https://wyrd.test/entity".to_owned(),
                        r#type: "service".to_owned(),
                        id_keys: vec!["service.name".to_owned()],
                        description_keys: vec!["service.version".to_owned()],
                    }],
                }),
                scope_logs: vec![ScopeLogs {
                    scope: Some(InstrumentationScope {
                        name: "wyrd.logs".to_owned(),
                        version: "1.2.3".to_owned(),
                        attributes: vec![attribute("scope.kind", Value::BoolValue(false))],
                        dropped_attributes_count: 7,
                    }),
                    log_records: correlated,
                    schema_url: "https://wyrd.test/scope".to_owned(),
                }],
                schema_url: "https://wyrd.test/resource".to_owned(),
            },
            ResourceLogs {
                resource: None,
                scope_logs: vec![ScopeLogs {
                    scope: None,
                    log_records: vec![LogRecord::default()],
                    schema_url: String::new(),
                }],
                schema_url: String::new(),
            },
        ]
    }

    /// Borrow one named column, panicking when it is absent.
    fn column<'a>(batch: &'a RecordBatch, name: &str) -> &'a dyn Array {
        batch
            .column_by_name(name)
            .unwrap_or_else(|| panic!("canonical log batch has column {name}"))
            .as_ref()
    }

    /// Downcast one named column, panicking when its Arrow type differs.
    fn typed<'a, T: 'static>(batch: &'a RecordBatch, name: &str) -> &'a T {
        column(batch, name)
            .as_any()
            .downcast_ref::<T>()
            .unwrap_or_else(|| panic!("column {name} has the declared Arrow type"))
    }

    /// Logs retain their body, event identity, and context exactly.
    ///
    /// # Panics
    ///
    /// Panics when any canonical body byte, event name, raw flag word,
    /// nanosecond timestamp, optional correlation id, presence distinction,
    /// stable field id, or sensitivity marker differs, or when a malformed
    /// canonical payload or a drifted schema identity is accepted.
    #[test]
    fn maximal_log_projection_preserves_body_context_and_presence() {
        let request = maximal_resource_logs();
        let (batch, outcome) = project_resource_logs(&request).expect("maximal logs project");

        let forms = body_forms();
        let correlated = forms.len();
        assert_eq!(
            outcome.accepted_records,
            i64::try_from(correlated).expect("row count fits i64") + 1
        );
        assert_eq!(outcome.rejected_records, 0);
        assert!(outcome.rejection_message.is_none());
        assert_eq!(batch.num_rows(), correlated + 1);
        assert_eq!(batch.num_columns(), LOG_FIELDS.len());

        let bodies = typed::<BinaryArray>(&batch, "body");
        for (row, value) in forms.into_iter().enumerate() {
            let expected = encode_any_value(&AnyValue { value: Some(value) });
            assert_eq!(bodies.value(row), expected.as_slice(), "body row {row}");
        }
        assert!(
            bodies.is_null(correlated),
            "an absent body projects to null"
        );

        let names = typed::<StringArray>(&batch, "event_name");
        assert_eq!(names.value(0), "wyrd.event.0");
        assert!(names.is_null(correlated), "an empty event name is absent");

        assert_record_scalars(&batch, correlated);
        assert_context_presence(&batch, correlated);
        assert_schema_identity(&batch);
        assert_non_canonical_inputs_are_rejected(&batch);
    }

    /// Assert every scalar log column retains its exact protocol value.
    ///
    /// # Panics
    ///
    /// Panics when a raw flag word, nanosecond timestamp, severity, drop
    /// count, correlation identifier, or attribute payload differs.
    fn assert_record_scalars(batch: &RecordBatch, correlated: usize) {
        let flags = typed::<Int64Array>(batch, "flags");
        assert_eq!(flags.value(0), 0xdead_beef, "raw flag word is not decoded");
        let time = typed::<Int64Array>(batch, "time_unix_nano");
        assert_eq!(time.value(0), 1_700_000_000_123_456_789);
        let observed = typed::<Int64Array>(batch, "observed_time_unix_nano");
        assert_eq!(observed.value(0), 1_700_000_000_987_654_321);
        assert_eq!(typed::<Int32Array>(batch, "severity_number").value(0), 9);
        assert_eq!(
            typed::<StringArray>(batch, "severity_text").value(0),
            "INFO"
        );
        assert_eq!(
            typed::<Int64Array>(batch, "dropped_attributes_count").value(0),
            3
        );

        let trace_ids = typed::<FixedSizeBinaryArray>(batch, "trace_id");
        assert_eq!(trace_ids.value(0), [0x11; 16]);
        assert!(
            trace_ids.is_null(correlated),
            "uncorrelated trace id is null"
        );
        let span_ids = typed::<FixedSizeBinaryArray>(batch, "span_id");
        assert_eq!(span_ids.value(0), [0x22; 8]);
        assert!(span_ids.is_null(correlated), "uncorrelated span id is null");

        assert_eq!(
            typed::<BinaryArray>(batch, "attributes").value(0),
            encode_attributes(&[attribute("payload.bytes", Value::BytesValue(vec![0xab]))])
                .as_slice()
        );
    }

    /// Assert resource and scope presence and context survive projection.
    ///
    /// # Panics
    ///
    /// Panics when a presence bit, context scalar, or entity-reference list
    /// length differs between the correlated and uncorrelated rows.
    fn assert_context_presence(batch: &RecordBatch, correlated: usize) {
        let resource_present = typed::<BooleanArray>(batch, "resource_present");
        assert!(resource_present.value(0));
        assert!(!resource_present.value(correlated));
        let scope_present = typed::<BooleanArray>(batch, "scope_present");
        assert!(scope_present.value(0));
        assert!(!scope_present.value(correlated));
        assert_eq!(
            typed::<StringArray>(batch, "resource_schema_url").value(0),
            "https://wyrd.test/resource"
        );
        assert_eq!(
            typed::<Int64Array>(batch, "resource_dropped_attributes_count").value(0),
            5
        );
        assert_eq!(
            typed::<StringArray>(batch, "scope_name").value(0),
            "wyrd.logs"
        );
        assert_eq!(
            typed::<StringArray>(batch, "scope_version").value(0),
            "1.2.3"
        );
        assert_eq!(
            typed::<Int64Array>(batch, "scope_dropped_attributes_count").value(0),
            7
        );
        assert_eq!(
            typed::<StringArray>(batch, "scope_schema_url").value(0),
            "https://wyrd.test/scope"
        );

        let refs = typed::<ListArray>(batch, "resource_entity_refs");
        assert_eq!(refs.value_length(0), 1);
        assert_eq!(refs.value_length(correlated), 0);
    }

    /// Assert the projected schema carries the ledger's declared identity.
    ///
    /// # Panics
    ///
    /// Panics when a declared field is missing, carries a different stable id
    /// or sensitivity marker, or fails canonical validation.
    fn assert_schema_identity(batch: &RecordBatch) {
        for declared in LOG_FIELDS {
            let field = batch
                .schema()
                .field_with_name(declared.name)
                .expect("every declared log field is present")
                .clone();
            assert_eq!(
                field.metadata().get(PARQUET_FIELD_ID),
                Some(&declared.id.to_string()),
                "field {} carries its stable id",
                declared.name
            );
            assert_eq!(
                field.metadata().get(WYRD_SENSITIVE),
                Some(&declared.class.is_sensitive().to_string()),
                "field {} carries its sensitivity",
                declared.name
            );
        }

        validate_canonical_user_batch(LOG_FIELDS, batch)
            .expect("the projected batch validates against its own ledger");
    }

    /// Assert malformed payloads and drifted identity are rejected outright.
    ///
    /// # Panics
    ///
    /// Panics when non-canonical attribute bytes or a drifted stable field id
    /// validate successfully.
    fn assert_non_canonical_inputs_are_rejected(batch: &RecordBatch) {
        let malformed = RecordBatch::try_new(
            canonical_log_schema(),
            batch
                .schema()
                .fields()
                .iter()
                .zip(batch.columns())
                .map(|(field, column)| {
                    if field.name() == "attributes" {
                        Arc::new(BinaryArray::from_iter_values(std::iter::repeat_n(
                            [0xffu8].as_slice(),
                            batch.num_rows(),
                        ))) as Arc<dyn Array>
                    } else {
                        Arc::clone(column)
                    }
                })
                .collect(),
        )
        .expect("the malformed batch still assembles");
        assert!(
            validate_canonical_user_batch(LOG_FIELDS, &malformed).is_err(),
            "non-canonical attribute bytes are rejected without a row"
        );

        let drifted_field =
            arrow::datatypes::Field::new("body", arrow::datatypes::DataType::Binary, true)
                .with_metadata(std::collections::HashMap::from([
                    (PARQUET_FIELD_ID.to_owned(), "999".to_owned()),
                    (WYRD_SENSITIVE.to_owned(), "true".to_owned()),
                ]));
        let drifted_fields: Vec<_> = batch
            .schema()
            .fields()
            .iter()
            .map(|field| {
                if field.name() == "body" {
                    Arc::new(drifted_field.clone())
                } else {
                    Arc::clone(field)
                }
            })
            .collect();
        let drifted = RecordBatch::try_new(
            Arc::new(arrow::datatypes::Schema::new(drifted_fields)),
            batch.columns().to_vec(),
        )
        .expect("the drifted batch still assembles");
        assert!(
            validate_canonical_user_batch(LOG_FIELDS, &drifted).is_err(),
            "a drifted stable field id is rejected without a row"
        );
    }
}
