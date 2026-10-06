//! `vala.traces.spans` — the sole canonical durable table for the OTLP trace
//! signal.
//!
//! [`SPAN_FIELDS`] is the table's ledger: one immutable declaration per logical
//! field carrying its name, physical type, nullability, and sensitivity. Every
//! other representation is derived from it — the Arrow schema, the canonical
//! physical fingerprint, the sensitive-column list, the OTLP projection in
//! [`super::projection`], and the canonical Arrow validator. Nothing outside
//! this module restates a span column's order or meaning. Field ids belong to
//! the registered Iceberg table, not to this ledger.

use arrow::datatypes::Field;

use crate::tables::fields::{CanonicalField as F, CanonicalType as T, canonical_arrow_fields};
use crate::tables::{
    CorrelationPolicy, DomainTable, PayloadClass, hourly_layout, sort_asc, sort_desc,
};
use wyrd_spec::vala::api::PhysicalLayoutWire;
use wyrd_spec::vala::managed_columns::WYRD_EVENT_TIME;

/// Ordered fields of one nested span event.
///
/// Everything an event carries is caller content, so the whole subtree is
/// sensitive payload and is skipped by a metadata-only projection.
pub static SPAN_EVENT_FIELDS: [F; 4] = [
    F::sensitive("time_unix_nano", T::Int64, false),
    F::sensitive("name", T::Utf8, false),
    F::sensitive("attributes", T::Binary, false),
    F::sensitive("dropped_attributes_count", T::Int64, false),
];

/// Element declaration of the ordered span-event collection.
pub static SPAN_EVENT_ELEMENT: F = F::sensitive("event", T::Struct(&SPAN_EVENT_FIELDS), false);

/// Ordered fields of one nested span link.
pub static SPAN_LINK_FIELDS: [F; 6] = [
    F::sensitive("trace_id", T::FixedSizeBinary(16), false),
    F::sensitive("span_id", T::FixedSizeBinary(8), false),
    F::sensitive("trace_state", T::Utf8, false),
    F::sensitive("flags", T::Int64, false),
    F::sensitive("attributes", T::Binary, false),
    F::sensitive("dropped_attributes_count", T::Int64, false),
];

/// Element declaration of the ordered span-link collection.
pub static SPAN_LINK_ELEMENT: F = F::sensitive("link", T::Struct(&SPAN_LINK_FIELDS), false);

/// Element declaration of the ordered resource entity-reference collection.
///
/// An entity reference is an opaque repeated protocol value with no query
/// predicate, so it is stored as its canonical pinned `EntityRef` encoding
/// rather than exploded into columns.
pub static RESOURCE_ENTITY_REF_ELEMENT: F = F::sensitive("entity_ref", T::Binary, false);

/// The canonical `vala.traces.spans` ledger.
///
/// `?` in the specification ledger is `nullable = true` here; every other field is
/// non-null. Presence booleans (`status_present`, `resource_present`,
/// `scope_present`) preserve the difference between an absent message and a
/// present empty one, and their subordinate scalars carry canonical empty
/// values when the presence bit is false.
pub static SPAN_FIELDS: &[F] = &[
    F::meta("trace_id", T::FixedSizeBinary(16), false),
    F::meta("span_id", T::FixedSizeBinary(8), false),
    F::meta("parent_span_id", T::FixedSizeBinary(8), true),
    F::meta("trace_state", T::Utf8, false),
    F::meta("flags", T::Int64, false),
    F::meta("name", T::Utf8, false),
    F::meta("kind", T::Int32, false),
    F::meta("start_time_unix_nano", T::Int64, false),
    F::meta("end_time_unix_nano", T::Int64, false),
    F::meta("duration_nano", T::Int64, false),
    F::meta("status_present", T::Bool, false),
    F::meta("status_code", T::Int32, true),
    F::payload("status_message", T::Utf8, true),
    F::sensitive("attributes", T::Binary, false),
    F::meta("dropped_attributes_count", T::Int64, false),
    F::sensitive("events", T::List(&SPAN_EVENT_ELEMENT), false),
    F::meta("dropped_events_count", T::Int64, false),
    F::sensitive("links", T::List(&SPAN_LINK_ELEMENT), false),
    F::meta("dropped_links_count", T::Int64, false),
    F::meta("resource_present", T::Bool, false),
    F::sensitive("resource_attributes", T::Binary, false),
    F::meta("resource_dropped_attributes_count", T::Int64, false),
    F::meta("resource_schema_url", T::Utf8, false),
    F::sensitive(
        "resource_entity_refs",
        T::List(&RESOURCE_ENTITY_REF_ELEMENT),
        false,
    ),
    F::meta("scope_present", T::Bool, false),
    F::meta("scope_name", T::Utf8, false),
    F::meta("scope_version", T::Utf8, false),
    F::sensitive("scope_attributes", T::Binary, false),
    F::meta("scope_dropped_attributes_count", T::Int64, false),
    F::meta("scope_schema_url", T::Utf8, false),
    F::meta("service_name", T::Utf8, true),
    F::meta("gen_ai_operation_name", T::Utf8, true),
    F::meta("gen_ai_provider_name", T::Utf8, true),
    F::meta("gen_ai_request_model", T::Utf8, true),
    F::meta("gen_ai_conversation_id", T::Utf8, true),
    F::meta("gen_ai_usage_input_tokens", T::Int64, true),
    F::meta("gen_ai_usage_output_tokens", T::Int64, true),
];

/// The canonical durable table for the OTLP trace signal.
pub struct SpansTable;

impl DomainTable for SpansTable {
    const NAMESPACE: &'static str = "traces";
    const NAME: &'static str = "spans";
    const CORRELATION_POLICY: CorrelationPolicy = CorrelationPolicy::Observation;
    const PAYLOAD_CLASS: PayloadClass = PayloadClass::Sensitive;
    const SENSITIVE_PAYLOAD_COLUMNS: &'static [&'static str] = &[
        "attributes",
        "events",
        "links",
        "resource_attributes",
        "resource_entity_refs",
        "scope_attributes",
    ];

    const CANONICAL_VALIDATOR: Option<crate::tables::CanonicalBatchValidator> =
        Some(validate_canonical_user_batch_for_table);

    fn canonical_fields() -> Option<&'static [crate::tables::fields::CanonicalField]> {
        Some(SPAN_FIELDS)
    }

    fn arrow_fields() -> Vec<Field> {
        canonical_arrow_fields(SPAN_FIELDS)
    }

    fn physical_layout() -> PhysicalLayoutWire {
        hourly_layout(
            vec![sort_desc(WYRD_EVENT_TIME), sort_asc("trace_id")],
            &["trace_id", "span_id", "service_name"],
        )
    }
}

/// Validate one supplied user batch against this table's canonical ledger.
///
/// The registry stores this as the table's
/// [`crate::tables::CanonicalBatchValidator`] so every canonical value rule is
/// dispatched from the definition rather than from a caller that knows the
/// table's name.
///
/// # Errors
///
/// Returns the shared canonical reason when the supplied schema drifts from
/// the declared ledger or a canonical payload value is not canonically
/// encoded.
fn validate_canonical_user_batch_for_table(
    batch: &arrow::array::RecordBatch,
) -> Result<arrow::array::RecordBatch, String> {
    crate::tables::signal::validate_canonical_user_batch(SPAN_FIELDS, batch)
}
