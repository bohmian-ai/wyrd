use chrono::{DateTime, Duration, TimeZone, Timelike, Utc};

use wyrd_spec::DataTenantId;
use wyrd_spec::vala::ids::{SpanId, TraceId};
use wyrd_spec::vala::trace::{
    InstrumentationScope, Resource, SpanKind, SpanStatus, TraceSummaryRecord,
};

/// Floor `t` to the start of the minute it falls in.
fn floor_to_minute(t: DateTime<Utc>) -> DateTime<Utc> {
    t.with_second(0).unwrap().with_nanosecond(0).unwrap()
}

fn summary() -> TraceSummaryRecord {
    let start = Utc.timestamp_opt(1_700_000_030, 0).unwrap();
    let end = start + Duration::milliseconds(2_500);
    TraceSummaryRecord {
        trace_id: TraceId::from_hex("0123456789abcdef0123456789abcdef").unwrap(),
        data_tenant_id: DataTenantId::new_v7(),
        bucket_time: floor_to_minute(start),
        start_time: start,
        end_time: end,
        duration_ms: 2_500,
        span_count: 8,
        error_count: 1,
        root_span_id: Some(SpanId::from_hex("0123456789abcdef").unwrap()),
        root_name: Some("GET /checkout".into()),
        root_kind: Some(SpanKind::Server),
        root_status: SpanStatus::Error {
            description: Some("upstream timeout".into()),
        },
        root_scope: Some(InstrumentationScope {
            name: "wyrd-tracing".into(),
            version: Some("0.3.1".into()),
            attributes: serde_json::Map::new(),
        }),
        resource: Resource {
            service_name: "checkout".into(),
            service_namespace: Some("payments".into()),
            service_version: Some("1.2.3".into()),
            service_instance_id: Some("worker-0".into()),
            attributes: serde_json::Map::new(),
        },
    }
}

#[test]
fn trace_summary_record_round_trip_full() {
    let s = summary();
    let j = serde_json::to_string(&s).unwrap();
    let back: TraceSummaryRecord = serde_json::from_str(&j).unwrap();
    assert_eq!(back, s);
}

#[test]
fn trace_summary_record_round_trip_anonymous() {
    let mut s = summary();
    s.root_span_id = None;
    s.root_name = None;
    s.root_kind = None;
    s.root_status = SpanStatus::Unset;
    s.root_scope = None;
    let j = serde_json::to_string(&s).unwrap();
    assert!(!j.contains("root_span_id"));
    assert!(!j.contains("root_name"));
    assert!(!j.contains("root_kind"));
    assert!(!j.contains("root_scope"));
    let back: TraceSummaryRecord = serde_json::from_str(&j).unwrap();
    assert_eq!(back, s);
}

#[test]
fn trace_summary_record_deny_unknown_fields() {
    let mut obj = serde_json::to_value(summary())
        .unwrap()
        .as_object()
        .unwrap()
        .clone();
    obj.insert("rogue".into(), serde_json::json!(true));
    let s = serde_json::to_string(&obj).unwrap();
    let r: Result<TraceSummaryRecord, _> = serde_json::from_str(&s);
    assert!(r.is_err());
}

#[test]
fn trace_summary_record_validate_happy_path() {
    summary().validate().unwrap();
}

#[test]
fn trace_summary_record_validate_rejects_zero_span_count() {
    let mut s = summary();
    s.span_count = 0;
    s.error_count = 0;
    assert!(s.validate().is_err());
}

#[test]
fn trace_summary_record_validate_rejects_error_count_exceeding_span_count() {
    let mut s = summary();
    s.span_count = 3;
    s.error_count = 4;
    assert!(s.validate().is_err());
}

#[test]
fn trace_summary_record_validate_accepts_error_count_equal_to_span_count() {
    let mut s = summary();
    s.span_count = 3;
    s.error_count = 3;
    s.validate().unwrap();
}

#[test]
fn trace_summary_record_validate_rejects_end_before_start() {
    let mut s = summary();
    s.end_time = s.start_time - Duration::seconds(1);
    s.duration_ms = 0;
    assert!(s.validate().is_err());
}

#[test]
fn trace_summary_record_validate_rejects_mismatched_duration() {
    let mut s = summary();
    s.duration_ms = 999;
    assert!(s.validate().is_err());
}

#[test]
fn trace_summary_record_validate_rejects_bucket_after_start() {
    let mut s = summary();
    s.bucket_time = s.start_time + Duration::seconds(1);
    assert!(s.validate().is_err());
}

#[test]
fn trace_summary_record_validate_accepts_bucket_equal_to_start() {
    let mut s = summary();
    s.bucket_time = s.start_time;
    s.validate().unwrap();
}

#[test]
fn trace_summary_record_validate_propagates_resource_failure() {
    let mut s = summary();
    s.resource.service_name = String::new();
    assert!(s.validate().is_err());
}

#[test]
fn trace_summary_record_validate_propagates_root_scope_failure() {
    let mut s = summary();
    s.root_scope = Some(InstrumentationScope {
        name: String::new(),
        version: None,
        attributes: serde_json::Map::new(),
    });
    assert!(s.validate().is_err());
}

#[test]
fn trace_summary_record_validate_rejects_empty_root_name() {
    let mut s = summary();
    s.root_name = Some(String::new());
    assert!(s.validate().is_err());
}

#[test]
fn trace_summary_record_validate_rejects_overlong_root_name() {
    let mut s = summary();
    s.root_name = Some("a".repeat(257));
    assert!(s.validate().is_err());
}

#[test]
fn trace_summary_record_validate_accepts_unset_root_status() {
    let mut s = summary();
    s.root_span_id = None;
    s.root_name = None;
    s.root_kind = None;
    s.root_status = SpanStatus::Unset;
    s.root_scope = None;
    s.validate().unwrap();
}

#[test]
fn trace_summary_record_bucket_time_at_minute_boundary_round_trips() {
    let t = Utc.timestamp_opt(1_700_000_040, 0).unwrap();
    let floored = floor_to_minute(t);
    assert_eq!(floored, t);
}

#[test]
fn trace_summary_record_bucket_time_below_start_within_minute() {
    let s = summary();
    let diff = s.start_time - s.bucket_time;
    assert!(diff <= Duration::seconds(60));
}
