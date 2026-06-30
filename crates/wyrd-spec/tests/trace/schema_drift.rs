//! Golden-file schema drift gate for trace fixtures.

use std::env;
use std::fs;
use std::path::PathBuf;

use pretty_assertions::assert_eq;
use schemars::schema_for;

use wyrd_spec::vala::trace::{
    AttributeValue, GenAiEvalResult, GenAiSpanRecord, InstrumentationScope, Resource, SpanEvent,
    SpanKind, SpanLink, SpanRecord, SpanStatus, TraceSummaryRecord,
};

fn fixture_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/trace/schemas")
}

fn assert_schema_matches<T: schemars::JsonSchema>(name: &str) {
    let mut schema = schema_for!(T);
    schema.meta_schema = Some("https://json-schema.org/draft/2020-12/schema".to_string());
    let actual = format!(
        "{}\n",
        serde_json::to_string_pretty(&schema).expect("schema serializes")
    );
    let path = fixture_dir().join(format!("{name}.schema.json"));

    if env::var_os("WYRD_SCHEMA_BLESS").is_some() {
        fs::create_dir_all(fixture_dir()).expect("trace schema fixture directory is created");
        fs::write(&path, &actual).expect("trace schema fixture is written");
        return;
    }

    let expected = fs::read_to_string(&path).unwrap_or_else(|error| {
        panic!(
            "missing golden schema at {path:?}: {error}; regenerate with \
             `WYRD_SCHEMA_BLESS=1 cargo test -p wyrd-spec --all-features --test trace schema_drift`",
        )
    });
    assert_eq!(
        actual, expected,
        "schema drift in {name}.schema.json; run the trace schema bless command"
    );
}

#[test]
fn span_record_schema() {
    assert_schema_matches::<SpanRecord>("span_record");
}

#[test]
fn span_kind_schema() {
    assert_schema_matches::<SpanKind>("span_kind");
}

#[test]
fn span_status_schema() {
    assert_schema_matches::<SpanStatus>("span_status");
}

#[test]
fn span_event_schema() {
    assert_schema_matches::<SpanEvent>("span_event");
}

#[test]
fn span_link_schema() {
    assert_schema_matches::<SpanLink>("span_link");
}

#[test]
fn resource_schema() {
    assert_schema_matches::<Resource>("resource");
}

#[test]
fn instrumentation_scope_schema() {
    assert_schema_matches::<InstrumentationScope>("instrumentation_scope");
}

#[test]
fn trace_summary_record_schema() {
    assert_schema_matches::<TraceSummaryRecord>("trace_summary_record");
}

#[test]
fn gen_ai_span_record_schema() {
    assert_schema_matches::<GenAiSpanRecord>("gen_ai_span_record");
}

#[test]
fn gen_ai_eval_result_schema() {
    assert_schema_matches::<GenAiEvalResult>("gen_ai_eval_result");
}

#[test]
fn attribute_value_schema() {
    assert_schema_matches::<AttributeValue>("attribute_value");
}
