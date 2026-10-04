//! `vala.logs.records` — the sole canonical durable table for the `OTLP` log
//! signal, including `OTel` events delivered through the Logs signal.
//!
//! [`LOG_FIELDS`] is the table's ledger. As with the other canonical signal
//! tables, every downstream representation is derived from it and nothing
//! outside this module restates a log column's order or meaning.

use arrow::datatypes::Field;

use crate::tables::fields::{
    CanonicalField, CanonicalField as F, CanonicalType as T, canonical_arrow_fields,
};
use crate::tables::{
    CorrelationPolicy, DomainTable, PayloadClass, hourly_layout, sort_asc_nulls_first, sort_desc,
};
use wyrd_spec::vala::api::PhysicalLayoutWire;
use wyrd_spec::vala::managed_columns::WYRD_EVENT_TIME;

/// Element declaration of the ordered resource entity-reference collection.
pub static LOG_ENTITY_REF_ELEMENT: F = F::sensitive("entity_ref", T::Binary, false);

/// The canonical `vala.logs.records` ledger.
///
/// A log record stays a log record: nothing here translates it into a span
/// event, and its trace/span correlation columns are nullable because the
/// protocol permits an uncorrelated record. `event_name` is nullable because
/// the pinned protocol carries no presence bit for it, so an empty wire value
/// and an absent one are the same fact and both project to null.
pub static LOG_FIELDS: &[CanonicalField] = &[
    F::meta("time_unix_nano", T::Int64, false),
    F::meta("observed_time_unix_nano", T::Int64, false),
    F::meta("severity_number", T::Int32, false),
    F::meta("severity_text", T::Utf8, false),
    F::meta("event_name", T::Utf8, true),
    F::sensitive("body", T::Binary, true),
    F::meta("trace_id", T::FixedSizeBinary(16), true),
    F::meta("span_id", T::FixedSizeBinary(8), true),
    F::meta("flags", T::Int64, false),
    F::sensitive("attributes", T::Binary, false),
    F::meta("dropped_attributes_count", T::Int64, false),
    F::meta("resource_present", T::Bool, false),
    F::sensitive("resource_attributes", T::Binary, false),
    F::meta("resource_dropped_attributes_count", T::Int64, false),
    F::meta("resource_schema_url", T::Utf8, false),
    F::sensitive(
        "resource_entity_refs",
        T::List(&LOG_ENTITY_REF_ELEMENT),
        false,
    ),
    F::meta("scope_present", T::Bool, false),
    F::meta("scope_name", T::Utf8, false),
    F::meta("scope_version", T::Utf8, false),
    F::sensitive("scope_attributes", T::Binary, false),
    F::meta("scope_dropped_attributes_count", T::Int64, false),
    F::meta("scope_schema_url", T::Utf8, false),
];

/// The canonical durable table for the `OTLP` log signal.
pub struct RecordsTable;

impl DomainTable for RecordsTable {
    const NAMESPACE: &'static str = "logs";
    const NAME: &'static str = "records";
    const CORRELATION_POLICY: CorrelationPolicy = CorrelationPolicy::Observation;
    const PAYLOAD_CLASS: PayloadClass = PayloadClass::Sensitive;
    const SENSITIVE_PAYLOAD_COLUMNS: &'static [&'static str] = &[
        "body",
        "attributes",
        "resource_attributes",
        "resource_entity_refs",
        "scope_attributes",
    ];

    const CANONICAL_VALIDATOR: Option<crate::tables::CanonicalBatchValidator> =
        Some(validate_canonical_user_batch_for_table);

    fn canonical_fields() -> Option<&'static [CanonicalField]> {
        Some(LOG_FIELDS)
    }

    fn arrow_fields() -> Vec<Field> {
        canonical_arrow_fields(LOG_FIELDS)
    }

    fn physical_layout() -> PhysicalLayoutWire {
        hourly_layout(
            vec![
                sort_desc(WYRD_EVENT_TIME),
                sort_asc_nulls_first("event_name"),
            ],
            &["severity_number", "trace_id"],
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
    crate::tables::signal::validate_canonical_user_batch(LOG_FIELDS, batch)
}
