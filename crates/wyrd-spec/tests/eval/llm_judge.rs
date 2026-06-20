use wyrd_semver::VersionBlock;
use wyrd_spec::envelope::CardKind;
use wyrd_spec::ids::{CardName, SpaceName};
use wyrd_spec::reference::CardRef;
use wyrd_spec::vala::eval::ids::TaskId;
use wyrd_spec::vala::eval::llm_judge::LlmJudgeTask;
use wyrd_spec::vala::eval::operator::ComparisonOperator;

fn prompt_ref(name: &str) -> CardRef {
    CardRef {
        kind: CardKind::Prompt,
        name: CardName::new(name).unwrap(),
        version: VersionBlock::parse("1.0.0").unwrap(),
        space: SpaceName::new("default").unwrap(),
        uid: None,
    }
}

fn data_ref(name: &str) -> CardRef {
    CardRef {
        kind: CardKind::Data,
        name: CardName::new(name).unwrap(),
        version: VersionBlock::parse("1.0.0").unwrap(),
        space: SpaceName::new("default").unwrap(),
        uid: None,
    }
}

#[test]
fn new_accepts_prompt_kind() {
    let task = LlmJudgeTask::new(
        TaskId::new("judge_one").unwrap(),
        prompt_ref("factuality-judge"),
        ComparisonOperator::Equals,
        serde_json::json!("pass"),
    )
    .expect("prompt-kind ref is valid");

    assert_eq!(task.max_retries, 2);
}

#[test]
fn new_rejects_non_prompt_kind() {
    let result = LlmJudgeTask::new(
        TaskId::new("judge_two").unwrap(),
        data_ref("dataset"),
        ComparisonOperator::Equals,
        serde_json::json!("pass"),
    );
    let err = result.unwrap_err();
    assert_eq!(err.code(), "WYRD_VALA_400_EVAL_REF_KIND_MISMATCH");
}

#[test]
fn validate_catches_deserialized_kind_mismatch() {
    let task = LlmJudgeTask::new(
        TaskId::new("judge").unwrap(),
        prompt_ref("prompt"),
        ComparisonOperator::Equals,
        serde_json::json!("ok"),
    )
    .unwrap();
    let mut value = serde_json::to_value(&task).unwrap();
    value["judge_ref"]["kind"] = serde_json::json!("Data");
    let bad: LlmJudgeTask = serde_json::from_value(value).unwrap();

    let err = bad.validate().unwrap_err();
    assert_eq!(err.code(), "WYRD_VALA_400_EVAL_REF_KIND_MISMATCH");
}

#[test]
fn round_trip() {
    let task = LlmJudgeTask::new(
        TaskId::new("judge").unwrap(),
        prompt_ref("prompt"),
        ComparisonOperator::ContainsIgnoreCase,
        serde_json::json!("paris"),
    )
    .unwrap();

    let serialized = serde_json::to_string(&task).unwrap();
    let deserialized: LlmJudgeTask = serde_json::from_str(&serialized).unwrap();
    assert_eq!(task, deserialized);
}
