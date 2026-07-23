use arrow::datatypes::{DataType, Field, TimeUnit};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CorrelationPolicy {
    Observation,
}

pub(crate) trait DomainTable {
    const CORRELATION_POLICY: CorrelationPolicy;

    fn arrow_fields() -> Vec<Field>;
}

pub(crate) struct SpansTable;
pub(crate) struct PointsTable;
pub(crate) struct RecordsTable;

fn utf8(name: &str, nullable: bool) -> Field {
    Field::new(name, DataType::Utf8, nullable)
}

fn utf8_view(name: &str, nullable: bool) -> Field {
    Field::new(name, DataType::Utf8View, nullable)
}

fn timestamp(name: &str, nullable: bool) -> Field {
    Field::new(
        name,
        DataType::Timestamp(TimeUnit::Microsecond, Some("UTC".into())),
        nullable,
    )
}

fn fixed_binary(name: &str, width: i32, nullable: bool) -> Field {
    Field::new(name, DataType::FixedSizeBinary(width), nullable)
}

fn boolean(name: &str, nullable: bool) -> Field {
    Field::new(name, DataType::Boolean, nullable)
}

fn int32(name: &str, nullable: bool) -> Field {
    Field::new(name, DataType::Int32, nullable)
}

fn int64(name: &str, nullable: bool) -> Field {
    Field::new(name, DataType::Int64, nullable)
}

fn uint32(name: &str, nullable: bool) -> Field {
    Field::new(name, DataType::UInt32, nullable)
}

fn float64(name: &str, nullable: bool) -> Field {
    Field::new(name, DataType::Float64, nullable)
}

impl DomainTable for SpansTable {
    const CORRELATION_POLICY: CorrelationPolicy = CorrelationPolicy::Observation;

    fn arrow_fields() -> Vec<Field> {
        vec![
            fixed_binary("trace_id", 16, false),
            fixed_binary("span_id", 8, false),
            fixed_binary("parent_span_id", 8, true),
            uint32("flags", false),
            utf8("trace_state", true),
            utf8("name", false),
            utf8("kind", false),
            timestamp("start_time", false),
            timestamp("end_time", false),
            int64("duration_ms", false),
            utf8("status", false),
            utf8_view("attributes", true),
            uint32("dropped_attributes_count", false),
            uint32("dropped_events_count", false),
            uint32("dropped_links_count", false),
            utf8("scope_name", true),
            utf8("scope_version", true),
            utf8("service_name", false),
        ]
    }
}

impl DomainTable for PointsTable {
    const CORRELATION_POLICY: CorrelationPolicy = CorrelationPolicy::Observation;

    fn arrow_fields() -> Vec<Field> {
        vec![
            utf8("metric_name", false),
            timestamp("time", false),
            timestamp("start_time", true),
            utf8("description", true),
            utf8("unit", true),
            utf8("metric_type", false),
            utf8("temporality", true),
            boolean("is_monotonic", true),
            uint32("flags", true),
            float64("value", true),
            int64("count", true),
            float64("sum", true),
            float64("min", true),
            float64("max", true),
            utf8_view("bucket_counts", true),
            utf8_view("explicit_bounds", true),
            int32("scale", true),
            int64("zero_count", true),
            float64("zero_threshold", true),
            utf8_view("positive_buckets", true),
            utf8_view("negative_buckets", true),
            utf8_view("quantile_values", true),
            utf8_view("exemplars", true),
            utf8_view("attributes", true),
            utf8("service_name", false),
            utf8("scope_name", true),
            utf8("scope_version", true),
        ]
    }
}

impl DomainTable for RecordsTable {
    const CORRELATION_POLICY: CorrelationPolicy = CorrelationPolicy::Observation;

    fn arrow_fields() -> Vec<Field> {
        vec![
            timestamp("time", true),
            timestamp("observed_time", false),
            uint32("severity_number", true),
            utf8("severity_text", true),
            utf8("event_name", true),
            utf8_view("body", true),
            fixed_binary("trace_id", 16, true),
            fixed_binary("span_id", 8, true),
            uint32("trace_flags", true),
            utf8_view("attributes", true),
            uint32("dropped_attributes_count", false),
            utf8("service_name", true),
            utf8("scope_name", true),
            utf8("scope_version", true),
        ]
    }
}
