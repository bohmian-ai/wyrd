//! `vala.gateway.calls` — one terminal analytical row per captured gateway call.
//!
//! The user fields are the `GatewayCallPayloadV1` record in declaration order,
//! except that its two canonical payload JSON texts are stored as the Variant
//! columns `request_payload` and `response_payload`; the Observation policy appends the canonical
//! managed envelope. Only the gateway capture principal may write the table,
//! which Gate enforces, and the two payload columns require the additional
//! payload-read permission, which Oracle enforces.

use std::sync::Arc;

use arrow::datatypes::{DataType, Field, Fields};

use wyrd_queue::variant::variant_field;

use crate::tables::fields::{boolean, int32, int64, ts_us_utc, utf8};
use crate::tables::{
    CorrelationPolicy, DomainTable, PayloadClass, hourly_layout, sort_asc, sort_desc,
};
use wyrd_spec::vala::api::PhysicalLayoutWire;
use wyrd_spec::vala::managed_columns::WYRD_EVENT_TIME;

/// Redacted request payload Variant column, readable only with payload authority.
pub const REQUEST_PAYLOAD: &str = "request_payload";

/// Redacted response payload Variant column, readable only with payload authority.
pub const RESPONSE_PAYLOAD: &str = "response_payload";

/// Optional resolved `ModelRef` Struct column; present exactly when routing resolved a model.
pub const RESOLVED_MODEL: &str = "resolved_model";

/// Built-in definition of `vala.gateway.calls`.
pub struct CallsTable;

impl CallsTable {
    /// Arrow type of one normalized usage list.
    ///
    /// Each element is one `GatewayUsageAmount`; the quantity keeps its exact
    /// canonical decimal string rather than a lossy float.
    #[must_use]
    pub fn usage_type() -> DataType {
        DataType::List(Arc::new(Field::new(
            "item",
            DataType::Struct(Fields::from(vec![
                utf8("dimension", false),
                utf8("unit", false),
                utf8("quantity", false),
            ])),
            false,
        )))
    }

    /// Arrow type of the sorted pricing-version set.
    #[must_use]
    pub fn pricing_versions_type() -> DataType {
        DataType::List(Arc::new(Field::new("item", DataType::Utf8, false)))
    }

    /// Arrow type of the captured binary payload object references.
    ///
    /// Each element is one `GatewayPayloadObjectRefV1` in declaration order,
    /// which is also the list's sort key. Only the digest, media type, and
    /// byte length are projected: the bytes live in object storage and their
    /// derived storage key is never persisted here.
    #[must_use]
    pub fn payload_object_refs_type() -> DataType {
        DataType::List(Arc::new(Field::new(
            "item",
            DataType::Struct(Fields::from(vec![
                utf8("digest", false),
                utf8("content_type", false),
                int64("size_bytes", false),
            ])),
            false,
        )))
    }
}

impl DomainTable for CallsTable {
    const NAMESPACE: &'static str = "gateway";
    const NAME: &'static str = "calls";
    const CORRELATION_POLICY: CorrelationPolicy = CorrelationPolicy::Observation;
    const PAYLOAD_CLASS: PayloadClass = PayloadClass::Sensitive;
    /// Both Variant payload columns require payload-read authority.
    const SENSITIVE_PAYLOAD_COLUMNS: &'static [&'static str] = &[REQUEST_PAYLOAD, RESPONSE_PAYLOAD];
    /// A resolved model carries both its provider and model, or neither.
    const WHOLE_STRUCTS: &'static [&'static str] = &[RESOLVED_MODEL];

    /// The `GatewayCallPayloadV1` fields in contract order.
    ///
    /// Identifiers are canonical UUID strings so they join the envelope's
    /// `principal_id`; enums are their snake-case wire names; unsigned integers
    /// widen to the next signed Iceberg type; decimals stay exact strings.
    fn arrow_fields() -> Vec<Field> {
        vec![
            int32("schema_version", false),
            utf8("call_id", false),
            utf8("caller_principal_id", false),
            utf8("operation", false),
            utf8("ingress_dialect", false),
            // A `ModelRef`: its provider and provider-native model.
            Field::new(
                "requested_model",
                DataType::Struct(Fields::from(vec![
                    utf8("provider", false),
                    utf8("model", false),
                ])),
                false,
            ),
            // Optional, so both children are nullable: a required Parquet leaf
            // under a null Struct reads back padded values that a field query
            // would expose. The table validator still refuses a partial model.
            Field::new(
                RESOLVED_MODEL,
                DataType::Struct(Fields::from(vec![
                    utf8("provider", true),
                    utf8("model", true),
                ])),
                true,
            ),
            utf8("resolved_deployment", true),
            boolean("streaming", false),
            ts_us_utc("started_at", false),
            ts_us_utc("terminal_at", false),
            utf8("outcome", false),
            utf8("error_code", true),
            int64("attempt_count", false),
            Field::new("usage", Self::usage_type(), true),
            utf8("cost", true),
            utf8("currency", true),
            Field::new("pricing_versions", Self::pricing_versions_type(), false),
            int64("capture_policy_version", false),
            variant_field(REQUEST_PAYLOAD, true),
            variant_field(RESPONSE_PAYLOAD, true),
            Field::new(
                "payload_object_refs",
                Self::payload_object_refs_type(),
                false,
            ),
        ]
    }

    /// Partitions hourly on event time and clusters by call identity.
    fn physical_layout() -> PhysicalLayoutWire {
        hourly_layout(
            vec![sort_desc(WYRD_EVENT_TIME), sort_asc("call_id")],
            &["call_id", "caller_principal_id"],
        )
    }
}

#[cfg(test)]
mod tests {
    use arrow::datatypes::DataType;

    use super::{CallsTable, RESOLVED_MODEL};
    use crate::tables::{DomainTable, builtin_table};
    use wyrd_spec::vala::{
        CARD_UID, PRINCIPAL_ID, RUN_ID, WYRD_EVENT_TIME, WYRD_INGESTED_AT, WYRD_REQUEST_ID,
    };

    /// Pins the exact contract columns, their nullability, the two model
    /// Structs' child nullability, and the envelope.
    ///
    /// # Panics
    ///
    /// Panics when the built-in stops resolving or any column drifts in name,
    /// order, nullability, or envelope placement.
    #[test]
    fn calls_table_is_the_payload_contract_plus_the_managed_envelope() {
        let definition = builtin_table("gateway", "calls").expect("vala.gateway.calls resolves");
        let schema = (definition.schema)();
        let columns = schema
            .fields()
            .iter()
            .map(|field| (field.name().as_str(), field.is_nullable()))
            .collect::<Vec<_>>();
        assert_eq!(
            columns,
            vec![
                ("schema_version", false),
                ("call_id", false),
                ("caller_principal_id", false),
                ("operation", false),
                ("ingress_dialect", false),
                ("requested_model", false),
                ("resolved_model", true),
                ("resolved_deployment", true),
                ("streaming", false),
                ("started_at", false),
                ("terminal_at", false),
                ("outcome", false),
                ("error_code", true),
                ("attempt_count", false),
                ("usage", true),
                ("cost", true),
                ("currency", true),
                ("pricing_versions", false),
                ("capture_policy_version", false),
                ("request_payload", true),
                ("response_payload", true),
                ("payload_object_refs", false),
                (RUN_ID, true),
                (CARD_UID, true),
                (PRINCIPAL_ID, false),
                (WYRD_REQUEST_ID, false),
                (WYRD_EVENT_TIME, false),
                (WYRD_INGESTED_AT, false),
            ]
        );
        assert_eq!(
            definition.sensitive_payload_columns,
            ["request_payload", "response_payload"]
        );
        assert!(matches!(
            schema.field_with_name("usage").expect("usage").data_type(),
            DataType::List(_)
        ));
        // A required model has required children; only the optional resolved
        // model's children are nullable.
        for (model, nullable) in [("requested_model", false), (RESOLVED_MODEL, true)] {
            let DataType::Struct(children) =
                schema.field_with_name(model).expect(model).data_type()
            else {
                panic!("{model} is a Struct");
            };
            let layout = children
                .iter()
                .map(|child| (child.name().as_str(), child.is_nullable()))
                .collect::<Vec<_>>();
            assert_eq!(
                layout,
                [("provider", nullable), ("model", nullable)],
                "{model}"
            );
        }
        assert!(CallsTable::canonical_fields().is_none());
    }
}
