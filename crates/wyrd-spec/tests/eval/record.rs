use std::fs;
use std::path::PathBuf;

use chrono::TimeZone;
use schemars::schema_for;
use uuid::Uuid;
use wyrd_spec::envelope::CardKind;
use wyrd_spec::ids::CardName;
use wyrd_spec::reference::CardRef;
use wyrd_spec::vala::eval::record::EvalRecordObservation;
use wyrd_spec::vala::ids::{RecordId, RunId, SessionId, SpanId};
use wyrd_spec::version::VersionBlock;

fn eval_ref(name: &str) -> CardRef {
    CardRef {
        kind: CardKind::Eval,
        name: CardName::new(name).unwrap(),
        version: VersionBlock::parse("1.0.0").unwrap(),
        space: None,
        uid: None,
    }
}

fn fixture() -> EvalRecordObservation {
    EvalRecordObservation {
        record_id: RecordId(Uuid::nil()),
        run_id: RunId::from_string("r1".to_owned()),
        session_id: Some(SessionId(Uuid::nil())),
        eval_ref: eval_ref("retriever-quality"),
        context: serde_json::json!({"response": "ok"}),
        trace_id: None,
        span_id: None,
        created_at: chrono::Utc.with_ymd_and_hms(2026, 6, 10, 0, 0, 0).unwrap(),
    }
}

fn fixture_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/eval/schemas")
}

#[test]
fn round_trips() {
    let rec = fixture();
    let json = serde_json::to_string(&rec).unwrap();
    let back: EvalRecordObservation = serde_json::from_str(&json).unwrap();
    assert_eq!(rec, back);
}

#[test]
fn span_without_trace_rejects() {
    let mut rec = fixture();
    rec.span_id = Some(SpanId::from_hex("0102030405060708").unwrap());
    let err = rec.validate().unwrap_err();
    assert_eq!(err.code(), "WYRD_SPEC_400_VALIDATION");
}

#[test]
fn deny_unknown_fields() {
    let mut value = serde_json::to_value(fixture()).unwrap();
    value
        .as_object_mut()
        .unwrap()
        .insert("agent_id".into(), serde_json::json!("a1"));
    let err = serde_json::from_value::<EvalRecordObservation>(value).unwrap_err();
    assert!(
        err.to_string().contains("unknown field"),
        "expected unknown-field rejection, got {err}"
    );
}

#[test]
fn schema_matches_golden() {
    let mut schema = schema_for!(EvalRecordObservation);
    schema.meta_schema = Some("https://json-schema.org/draft/2020-12/schema".to_string());
    let actual = format!(
        "{}\n",
        serde_json::to_string_pretty(&schema).expect("schema serializes")
    );
    let path = fixture_dir().join("eval_record_observation.schema.json");
    let expected = fs::read_to_string(&path).unwrap_or_else(|error| {
        panic!(
            "missing golden schema at {path:?}: {error}; regenerate with \
             `cargo run -p wyrd-spec --example gen_schemas --features server`",
        )
    });
    assert_eq!(
        actual, expected,
        "schema drift in eval_record_observation.schema.json; run mise run codegen:regen"
    );
}
