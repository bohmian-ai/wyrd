//! Observation contract tests — envelope, closed kind taxonomy, and record
//! descriptor.

use wyrd_semver::VersionBlock;
use wyrd_spec::envelope::CardKind;
use wyrd_spec::ids::{CardName, SpaceName};
use wyrd_spec::reference::CardRef;
use wyrd_spec::vala::api::BifrostTableName;
use wyrd_spec::vala::ids::RunId;
use wyrd_spec::vala::observation::{ObservationEnvelope, ObservationKind, RecordObservation};

fn card_ref() -> CardRef {
    CardRef {
        kind: CardKind::Agent,
        name: CardName::new("my-agent").unwrap(),
        version: VersionBlock::parse("1.0.0").unwrap(),
        space: SpaceName::new("default").unwrap(),
        uid: None,
    }
}

fn record_obs() -> RecordObservation {
    RecordObservation {
        table: BifrostTableName::new("agent_runs"),
    }
}

fn envelope_with_run_id() -> ObservationEnvelope {
    ObservationEnvelope {
        card_ref: card_ref(),
        run_id: Some(RunId::from_string("test-run-1".to_owned())),
    }
}

fn envelope_no_run_id() -> ObservationEnvelope {
    ObservationEnvelope {
        card_ref: card_ref(),
        run_id: None,
    }
}

// --- round-trip serde ---

#[test]
fn observation_envelope_round_trips() {
    let env = envelope_with_run_id();
    let json = serde_json::to_string(&env).unwrap();
    let back: ObservationEnvelope = serde_json::from_str(&json).unwrap();
    assert_eq!(env, back);
}

#[test]
fn observation_kind_round_trips() {
    let kind = ObservationKind::Record(record_obs());
    let json = serde_json::to_string(&kind).unwrap();
    let back: ObservationKind = serde_json::from_str(&json).unwrap();
    assert_eq!(kind, back);
}

#[test]
fn record_observation_round_trips() {
    let rec = record_obs();
    let json = serde_json::to_string(&rec).unwrap();
    let back: RecordObservation = serde_json::from_str(&json).unwrap();
    assert_eq!(rec, back);
}

// --- wire encoding invariants ---

#[test]
fn observation_kind_tagged_union_encoding() {
    let kind = ObservationKind::Record(record_obs());
    let value = serde_json::to_value(&kind).unwrap();
    let obj = value.as_object().unwrap();
    assert_eq!(
        obj.get("kind").and_then(|v| v.as_str()),
        Some("record"),
        "ObservationKind must serialize with kind tag = 'record'"
    );
}

#[test]
fn envelope_run_id_omitted_when_none() {
    let env = envelope_no_run_id();
    let value = serde_json::to_value(&env).unwrap();
    let obj = value.as_object().unwrap();
    assert!(
        !obj.contains_key("run_id"),
        "run_id must be absent from the wire when None"
    );
}

#[test]
fn envelope_run_id_present_when_some() {
    let env = envelope_with_run_id();
    let value = serde_json::to_value(&env).unwrap();
    let obj = value.as_object().unwrap();
    assert!(
        obj.contains_key("run_id"),
        "run_id must appear on the wire when Some"
    );
}

#[test]
fn envelope_no_record_id_field() {
    let env = envelope_with_run_id();
    let value = serde_json::to_value(&env).unwrap();
    let obj = value.as_object().unwrap();
    assert!(
        !obj.contains_key("record_id"),
        "ObservationEnvelope must not carry record_id (removed with vala-client)"
    );
}

#[test]
fn envelope_card_ref_required() {
    let missing = r#"{"run_id": "r1"}"#;
    let err = serde_json::from_str::<ObservationEnvelope>(missing).unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("card_ref") || msg.contains("missing field"),
        "expected missing card_ref error, got: {msg}"
    );
}

// --- closed enum invariant ---

#[test]
fn observation_kind_is_closed_exhaustive_match() {
    // An exhaustive match with no wildcard arm. If `#[non_exhaustive]` were
    // present the compiler would reject this without a `_ => …` arm. The fact
    // that it compiles proves the enum is closed.
    let kind = ObservationKind::Record(record_obs());
    let _label = match kind {
        ObservationKind::Record(_) => "record",
    };
}

// --- schema drift ---

#[test]
fn observation_schema_drift() {
    use std::fs;
    use std::path::PathBuf;

    use schemars::schema_for;

    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/observation/schemas");

    fn assert_schema_matches<T: schemars::JsonSchema>(dir: &std::path::Path, name: &str) {
        let mut schema = schema_for!(T);
        schema.meta_schema = Some("https://json-schema.org/draft/2020-12/schema".to_string());
        let actual = format!(
            "{}\n",
            serde_json::to_string_pretty(&schema).expect("schema serializes")
        );
        let path = dir.join(format!("{name}.schema.json"));
        let expected = fs::read_to_string(&path).unwrap_or_else(|error| {
            panic!(
                "missing golden schema at {path:?}: {error}; regenerate with \
                 `cargo run -p wyrd-spec --example gen_schemas --features server`"
            )
        });
        assert_eq!(
            actual, expected,
            "schema drift in {name}.schema.json; run mise run codegen:regen"
        );
    }

    assert_schema_matches::<ObservationEnvelope>(&dir, "observation_envelope");
    assert_schema_matches::<ObservationKind>(&dir, "observation_kind");
    assert_schema_matches::<RecordObservation>(&dir, "record_observation");
}
