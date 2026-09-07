//! `vala.traces.spans` — the sole canonical durable table for the OTLP trace
//! signal.
//!
//! [`SPAN_FIELDS`] is the table's ledger: one immutable declaration per logical
//! field carrying its table-local stable id, name, physical type, nullability,
//! and sensitivity. Every other representation is derived from it — the Arrow
//! schema and its `PARQUET:field_id` metadata, the Iceberg field ids, the
//! canonical physical fingerprint, the sensitive-column list, the OTLP
//! projection in [`super::projection`], and the canonical Arrow validator.
//! Nothing outside this module restates a span column's order or meaning.

use arrow::datatypes::Field;

use crate::tables::fields::{CanonicalField as F, CanonicalType as T, canonical_arrow_fields};
use crate::tables::{
    CorrelationPolicy, DomainTable, EntityBoundsMapping, PayloadClass, hourly_layout, sort_asc,
    sort_desc,
};
use wyrd_spec::vala::api::PhysicalLayoutWire;
use wyrd_spec::vala::managed_columns::WYRD_EVENT_TIME;

/// Ordered fields of one nested span event.
///
/// Everything an event carries is caller content, so the whole subtree is
/// sensitive payload and is skipped by a metadata-only projection.
pub static SPAN_EVENT_FIELDS: [F; 4] = [
    F::sensitive(18, "time_unix_nano", T::Int64, false),
    F::sensitive(19, "name", T::Utf8, false),
    F::sensitive(20, "attributes", T::Binary, false),
    F::sensitive(21, "dropped_attributes_count", T::Int64, false),
];

/// Element declaration of the ordered span-event collection.
pub static SPAN_EVENT_ELEMENT: F = F::sensitive(17, "event", T::Struct(&SPAN_EVENT_FIELDS), false);

/// Ordered fields of one nested span link.
pub static SPAN_LINK_FIELDS: [F; 6] = [
    F::sensitive(25, "trace_id", T::FixedSizeBinary(16), false),
    F::sensitive(26, "span_id", T::FixedSizeBinary(8), false),
    F::sensitive(27, "trace_state", T::Utf8, false),
    F::sensitive(28, "flags", T::Int64, false),
    F::sensitive(29, "attributes", T::Binary, false),
    F::sensitive(30, "dropped_attributes_count", T::Int64, false),
];

/// Element declaration of the ordered span-link collection.
pub static SPAN_LINK_ELEMENT: F = F::sensitive(24, "link", T::Struct(&SPAN_LINK_FIELDS), false);

/// Element declaration of the ordered resource entity-reference collection.
///
/// An entity reference is an opaque repeated protocol value with no query
/// predicate, so it is stored as its canonical pinned `EntityRef` encoding
/// rather than exploded into columns.
pub static RESOURCE_ENTITY_REF_ELEMENT: F = F::sensitive(37, "entity_ref", T::Binary, false);

/// The canonical `vala.traces.spans` ledger.
///
/// Ids are table-local, immutable, and never renumbered or reused. `?` in the
/// specification ledger is `nullable = true` here; every other field is
/// non-null. Presence booleans (`status_present`, `resource_present`,
/// `scope_present`) preserve the difference between an absent message and a
/// present empty one, and their subordinate scalars carry canonical empty
/// values when the presence bit is false.
pub static SPAN_FIELDS: &[F] = &[
    F::meta(1, "trace_id", T::FixedSizeBinary(16), false),
    F::meta(2, "span_id", T::FixedSizeBinary(8), false),
    F::meta(3, "parent_span_id", T::FixedSizeBinary(8), true),
    F::meta(4, "trace_state", T::Utf8, false),
    F::meta(5, "flags", T::Int64, false),
    F::meta(6, "name", T::Utf8, false),
    F::meta(7, "kind", T::Int32, false),
    F::meta(8, "start_time_unix_nano", T::Int64, false),
    F::meta(9, "end_time_unix_nano", T::Int64, false),
    F::meta(10, "duration_nano", T::Int64, false),
    F::meta(11, "status_present", T::Bool, false),
    F::meta(12, "status_code", T::Int32, true),
    F::payload(13, "status_message", T::Utf8, true),
    F::sensitive(14, "attributes", T::Binary, false),
    F::meta(15, "dropped_attributes_count", T::Int64, false),
    F::sensitive(16, "events", T::List(&SPAN_EVENT_ELEMENT), false),
    F::meta(22, "dropped_events_count", T::Int64, false),
    F::sensitive(23, "links", T::List(&SPAN_LINK_ELEMENT), false),
    F::meta(31, "dropped_links_count", T::Int64, false),
    F::meta(32, "resource_present", T::Bool, false),
    F::sensitive(33, "resource_attributes", T::Binary, false),
    F::meta(34, "resource_dropped_attributes_count", T::Int64, false),
    F::meta(35, "resource_schema_url", T::Utf8, false),
    F::sensitive(
        36,
        "resource_entity_refs",
        T::List(&RESOURCE_ENTITY_REF_ELEMENT),
        false,
    ),
    F::meta(38, "scope_present", T::Bool, false),
    F::meta(39, "scope_name", T::Utf8, false),
    F::meta(40, "scope_version", T::Utf8, false),
    F::sensitive(41, "scope_attributes", T::Binary, false),
    F::meta(42, "scope_dropped_attributes_count", T::Int64, false),
    F::meta(43, "scope_schema_url", T::Utf8, false),
    F::meta(44, "service_name", T::Utf8, true),
    F::meta(45, "gen_ai_operation_name", T::Utf8, true),
    F::meta(46, "gen_ai_provider_name", T::Utf8, true),
    F::meta(47, "gen_ai_request_model", T::Utf8, true),
    F::meta(48, "gen_ai_conversation_id", T::Utf8, true),
    F::meta(49, "gen_ai_usage_input_tokens", T::Int64, true),
    F::meta(50, "gen_ai_usage_output_tokens", T::Int64, true),
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

    fn entity_bounds_mapping() -> Option<EntityBoundsMapping> {
        Some(EntityBoundsMapping {
            entity_kind: "trace".into(),
            entity_id_column: "trace_id".into(),
        })
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
