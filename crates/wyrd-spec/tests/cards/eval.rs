use std::collections::BTreeMap;

use wyrd_spec::card::eval::EvalSpec;
use wyrd_spec::envelope::{Card, CardKind, Metadata, Relationships, Spec};
use wyrd_spec::format;
use wyrd_spec::ids::CardName;
use wyrd_spec::vala::eval::{AssertionTask, ComparisonOperator, EvalTask, JsonPath, TaskId};
use wyrd_spec::version::{ApiVersion, VersionBlock};

#[test]
fn card_body_eval_uses_vala_eval_shape() {
    let spec = eval_spec();
    let serialized = serde_json::to_string(&spec).unwrap();
    let back: EvalSpec = serde_json::from_str(&serialized).unwrap();

    assert_eq!(spec, back);
}

#[test]
fn eval_card_envelope_round_trips_with_vala_eval_shape() {
    let card = Card {
        api_version: ApiVersion::v1(),
        kind: CardKind::Eval,
        metadata: Metadata {
            name: CardName::new("quality_eval").unwrap(),
            version: VersionBlock::parse("1.0.0").unwrap(),
            space: None,
            uid: None,
            labels: BTreeMap::new(),
            annotations: BTreeMap::new(),
            spec_hash: None,
            artifact_hash: None,
        },
        spec: Spec::Eval(eval_spec()),
        relationships: Relationships::default(),
        status: None,
    };

    let yaml = format::yaml::to_string(&card).unwrap();
    assert!(yaml.contains("kind: Eval"));
    assert!(yaml.contains("tasks:"));
    assert!(yaml.contains("kind: assertion"));
    assert!(!yaml.contains("eval_type:"));
    assert!(!yaml.contains("type: Eval"));

    let decoded: Card = format::yaml::from_str(&yaml).unwrap();
    assert_eq!(decoded, card);
}

#[test]
fn legacy_eval_profile_json_no_longer_deserializes() {
    let legacy = r#"{
        "eval_type": "judge",
        "judge_refs": [],
        "assertions": [{"name": "x", "rule": "$.x != null"}]
    }"#;

    let result: Result<EvalSpec, _> = serde_json::from_str(legacy);

    assert!(result.is_err());
}

fn eval_spec() -> EvalSpec {
    let mut tasks = BTreeMap::new();
    let id = TaskId::new("a").unwrap();
    tasks.insert(
        id.clone(),
        EvalTask::Assertion(AssertionTask {
            id,
            context_path: Some(JsonPath::new("$.x").unwrap()),
            item_context_path: None,
            operator: ComparisonOperator::IsNotNull,
            expected: serde_json::Value::Null,
            depends_on: vec![],
            condition: None,
        }),
    );
    EvalSpec::new(tasks).expect("static eval spec is valid")
}
