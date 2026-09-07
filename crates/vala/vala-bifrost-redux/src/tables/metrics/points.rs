//! `vala.metrics.points` — the sole canonical durable table for the `OTLP`
//! metric signal.
//!
//! [`METRIC_FIELDS`] is the table's ledger. One row is one data point of one
//! metric; `metric_type` selects which kind-specific columns that row owns and
//! every column a different kind owns stays null. Nothing here narrows a value
//! shape: integer and double alternatives keep their own typed columns, bucket
//! and quantile collections stay nested typed collections, and doubles retain
//! their exact IEEE bits.

use arrow::datatypes::Field;

use crate::tables::fields::{
    CanonicalField, CanonicalField as F, CanonicalType as T, canonical_arrow_fields,
};
use crate::tables::{
    CorrelationPolicy, DomainTable, PayloadClass, hourly_layout, sort_asc, sort_desc,
};
use wyrd_spec::vala::api::PhysicalLayoutWire;
use wyrd_spec::vala::managed_columns::WYRD_EVENT_TIME;

/// Element declaration of an explicit-bucket count collection.
pub static BUCKET_COUNT_ELEMENT: F = F::payload(19, "item", T::UInt64, false);

/// Element declaration of an explicit-bound collection.
pub static EXPLICIT_BOUND_ELEMENT: F = F::payload(21, "item", T::Float64, false);

/// Element declaration of the positive exponential bucket counts.
pub static POSITIVE_BUCKET_COUNT_ELEMENT: F = F::payload(28, "item", T::UInt64, false);

/// Ordered fields of the positive exponential bucket collection.
pub static POSITIVE_BUCKET_FIELDS: [F; 2] = [
    F::payload(26, "offset", T::Int32, false),
    F::payload(
        27,
        "bucket_counts",
        T::List(&POSITIVE_BUCKET_COUNT_ELEMENT),
        false,
    ),
];

/// Element declaration of the negative exponential bucket counts.
pub static NEGATIVE_BUCKET_COUNT_ELEMENT: F = F::payload(32, "item", T::UInt64, false);

/// Ordered fields of the negative exponential bucket collection.
pub static NEGATIVE_BUCKET_FIELDS: [F; 2] = [
    F::payload(30, "offset", T::Int32, false),
    F::payload(
        31,
        "bucket_counts",
        T::List(&NEGATIVE_BUCKET_COUNT_ELEMENT),
        false,
    ),
];

/// Ordered fields of one summary quantile.
pub static QUANTILE_VALUE_FIELDS: [F; 2] = [
    F::payload(37, "quantile", T::Float64, false),
    F::payload(38, "value", T::Float64, false),
];

/// Element declaration of the ordered summary quantile collection.
pub static QUANTILE_VALUE_ELEMENT: F =
    F::payload(36, "quantile_value", T::Struct(&QUANTILE_VALUE_FIELDS), false);

/// Ordered fields of one exemplar.
///
/// An exemplar carries filtered caller attributes and the correlation ids of
/// the originating span, so the whole subtree is access-gated payload.
pub static EXEMPLAR_FIELDS: [F; 6] = [
    F::sensitive(41, "time_unix_nano", T::UInt64, false),
    F::sensitive(42, "int_value", T::Int64, true),
    F::sensitive(43, "double_value", T::Float64, true),
    F::sensitive(44, "filtered_attributes", T::Binary, false),
    F::sensitive(45, "trace_id", T::FixedSizeBinary(16), true),
    F::sensitive(46, "span_id", T::FixedSizeBinary(8), true),
];

/// Element declaration of the ordered exemplar collection.
pub static EXEMPLAR_ELEMENT: F = F::sensitive(40, "exemplar", T::Struct(&EXEMPLAR_FIELDS), false);

/// Element declaration of the ordered resource entity-reference collection.
pub static METRIC_ENTITY_REF_ELEMENT: F = F::sensitive(52, "entity_ref", T::Binary, false);

/// The canonical `vala.metrics.points` ledger.
///
/// Ids are table-local, immutable, and never renumbered or reused. Every
/// kind-specific column is nullable because a point of another kind does not
/// own it; the projector, not nullability, is what forbids a point from
/// carrying a shape its declared kind cannot own.
pub static METRIC_FIELDS: &[CanonicalField] = &[
    F::meta(1, "metric_name", T::Utf8, false),
    F::meta(2, "description", T::Utf8, false),
    F::meta(3, "unit", T::Utf8, false),
    F::sensitive(4, "metadata", T::Binary, false),
    F::meta(5, "metric_type", T::Utf8, false),
    F::meta(6, "time_unix_nano", T::UInt64, false),
    F::meta(7, "start_time_unix_nano", T::UInt64, false),
    F::meta(8, "flags", T::UInt32, false),
    F::sensitive(9, "attributes", T::Binary, false),
    F::meta(10, "int_value", T::Int64, true),
    F::meta(11, "double_value", T::Float64, true),
    F::meta(12, "aggregation_temporality", T::Int32, true),
    F::meta(13, "is_monotonic", T::Bool, true),
    F::meta(14, "histogram_count", T::UInt64, true),
    F::meta(15, "histogram_sum", T::Float64, true),
    F::meta(16, "histogram_min", T::Float64, true),
    F::meta(17, "histogram_max", T::Float64, true),
    F::payload(18, "bucket_counts", T::List(&BUCKET_COUNT_ELEMENT), true),
    F::payload(
        20,
        "explicit_bounds",
        T::List(&EXPLICIT_BOUND_ELEMENT),
        true,
    ),
    F::meta(22, "exponential_scale", T::Int32, true),
    F::meta(23, "exponential_zero_count", T::UInt64, true),
    F::meta(24, "exponential_zero_threshold", T::Float64, true),
    F::payload(
        25,
        "positive_buckets",
        T::Struct(&POSITIVE_BUCKET_FIELDS),
        true,
    ),
    F::payload(
        29,
        "negative_buckets",
        T::Struct(&NEGATIVE_BUCKET_FIELDS),
        true,
    ),
    F::meta(33, "summary_count", T::UInt64, true),
    F::meta(34, "summary_sum", T::Float64, true),
    F::payload(
        35,
        "quantile_values",
        T::List(&QUANTILE_VALUE_ELEMENT),
        true,
    ),
    F::sensitive(39, "exemplars", T::List(&EXEMPLAR_ELEMENT), false),
    F::meta(47, "resource_present", T::Bool, false),
    F::sensitive(48, "resource_attributes", T::Binary, false),
    F::meta(49, "resource_dropped_attributes_count", T::UInt32, false),
    F::meta(50, "resource_schema_url", T::Utf8, false),
    F::sensitive(
        51,
        "resource_entity_refs",
        T::List(&METRIC_ENTITY_REF_ELEMENT),
        false,
    ),
    F::meta(53, "scope_present", T::Bool, false),
    F::meta(54, "scope_name", T::Utf8, false),
    F::meta(55, "scope_version", T::Utf8, false),
    F::sensitive(56, "scope_attributes", T::Binary, false),
    F::meta(57, "scope_dropped_attributes_count", T::UInt32, false),
    F::meta(58, "scope_schema_url", T::Utf8, false),
];

/// The canonical durable table for the `OTLP` metric signal.
pub struct PointsTable;

impl DomainTable for PointsTable {
    const NAMESPACE: &'static str = "metrics";
    const NAME: &'static str = "points";
    const CORRELATION_POLICY: CorrelationPolicy = CorrelationPolicy::Observation;
    const PAYLOAD_CLASS: PayloadClass = PayloadClass::Sensitive;
    const SENSITIVE_PAYLOAD_COLUMNS: &'static [&'static str] = &[
        "metadata",
        "attributes",
        "exemplars",
        "resource_attributes",
        "resource_entity_refs",
        "scope_attributes",
    ];

    fn canonical_fields() -> Option<&'static [CanonicalField]> {
        Some(METRIC_FIELDS)
    }

    fn arrow_fields() -> Vec<Field> {
        canonical_arrow_fields(METRIC_FIELDS)
    }

    fn physical_layout() -> PhysicalLayoutWire {
        hourly_layout(
            vec![sort_desc(WYRD_EVENT_TIME), sort_asc("metric_name")],
            &["metric_name", "metric_type"],
        )
    }
}
