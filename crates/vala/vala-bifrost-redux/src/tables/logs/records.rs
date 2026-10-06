//! `vala.logs.records` — the sole canonical durable table for the `OTLP` log
//! signal, including `OTel` events delivered through the Logs signal.
//!
//! [`LOG_FIELDS`] is the table's ledger. As with the other canonical signal
//! tables, every downstream representation is derived from it and nothing
//! outside this module restates a log column's order or meaning.

use arrow::array::RecordBatch;
use arrow::datatypes::Field;

use crate::tables::fields::{
    CanonicalField, CanonicalField as F, CanonicalType as T, canonical_arrow_fields,
};
use crate::tables::signal;
use crate::tables::{
    CanonicalBatchValidator, CorrelationPolicy, DomainTable, PayloadClass, hourly_layout,
    sort_asc_nulls_first, sort_desc,
};
use wyrd_spec::vala::BifrostError;
use wyrd_spec::vala::api::PhysicalLayoutWire;
use wyrd_spec::vala::managed_columns::WYRD_EVENT_TIME;

/// The canonical `vala.logs.records` ledger.
///
/// A log record stays a log record: nothing here translates it into a span
/// event, and its trace/span correlation columns are nullable because the
/// protocol permits an uncorrelated record. `event_name` is nullable because
/// the pinned protocol carries no presence bit for it, so an empty wire value
/// and an absent one are the same fact and both project to null. `body` is
/// null when the record carries no body and a Variant null when it carries a
/// present but unset value. The trailing promoted columns copy well-known
/// semantic-convention values out of the Variant collections so they can be
/// filtered without decoding; each is null when its source is absent or
/// carries the wrong type, and `body_text` is set only for a string body.
pub static LOG_FIELDS: &[CanonicalField] = &[
    F::meta("time_unix_nano", T::Int64, false),
    F::meta("observed_time_unix_nano", T::Int64, false),
    F::meta("severity_number", T::Int32, false),
    F::meta("severity_text", T::Utf8, false),
    F::meta("event_name", T::Utf8, true),
    F::sensitive("body", T::Variant, true),
    F::meta("trace_id", T::FixedSizeBinary(16), true),
    F::meta("span_id", T::FixedSizeBinary(8), true),
    F::meta("flags", T::Int64, false),
    F::sensitive("attributes", T::Variant, false),
    F::meta("dropped_attributes_count", T::Int64, false),
    F::meta("resource_present", T::Bool, false),
    F::sensitive("resource_attributes", T::Variant, false),
    F::meta("resource_dropped_attributes_count", T::Int64, false),
    F::meta("resource_schema_url", T::Utf8, false),
    F::sensitive(
        "resource_entity_refs",
        T::List(&signal::ENTITY_REF_ELEMENT),
        false,
    ),
    F::meta("scope_present", T::Bool, false),
    F::meta("scope_name", T::Utf8, false),
    F::meta("scope_version", T::Utf8, false),
    F::sensitive("scope_attributes", T::Variant, false),
    F::meta("scope_dropped_attributes_count", T::Int64, false),
    F::meta("scope_schema_url", T::Utf8, false),
    F::meta("service_name", T::Utf8, true),
    F::meta("service_version", T::Utf8, true),
    F::meta("deployment_environment", T::Utf8, true),
    F::meta("exception_type", T::Utf8, true),
    F::meta("exception_message", T::Utf8, true),
    F::meta("exception_stacktrace", T::Utf8, true),
    F::meta("body_text", T::Utf8, true),
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

    const CANONICAL_VALIDATOR: CanonicalBatchValidator = validate_canonical_user_batch_for_table;

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
/// Returns the catalogued refusal of the shared canonical validation when
/// the supplied schema drifts from the declared ledger or a Variant value
/// cannot be stored.
fn validate_canonical_user_batch_for_table(
    batch: &RecordBatch,
) -> Result<RecordBatch, BifrostError> {
    signal::validate_canonical_user_batch(LOG_FIELDS, batch)
}
