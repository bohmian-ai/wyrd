use wyrd_spec::vala::eval::agent::AgentAssertionTask;
use wyrd_spec::vala::eval::assertion::AssertionTask;
use wyrd_spec::vala::eval::ids::{JsonPath, TaskId};
use wyrd_spec::vala::eval::operator::ComparisonOperator;
use wyrd_spec::vala::eval::task::EvalTask;
use wyrd_spec::vala::eval::trace::TraceAssertionTask;

fn assertion(id: &str, deps: Vec<&str>) -> EvalTask {
    EvalTask::Assertion(AssertionTask {
        id: TaskId::new(id).unwrap(),
        context_path: Some(JsonPath::new("$.x").unwrap()),
        item_context_path: None,
        operator: ComparisonOperator::IsNotNull,
        expected: serde_json::Value::Null,
        depends_on: deps.into_iter().map(|d| TaskId::new(d).unwrap()).collect(),
        condition: None,
    })
}

#[test]
fn assertion_variant_round_trip() {
    let task = assertion("a", vec![]);
    let serialized = serde_json::to_string(&task).unwrap();
    assert!(serialized.starts_with(r#"{"kind":"assertion","#));
    let back: EvalTask = serde_json::from_str(&serialized).unwrap();
    assert_eq!(task, back);
}

#[test]
fn id_accessor_matches_inner() {
    let task = assertion("alpha", vec![]);
    assert_eq!(task.id().as_str(), "alpha");
}

#[test]
fn depends_on_accessor_matches_inner() {
    let task = assertion("beta", vec!["alpha"]);
    assert_eq!(task.depends_on().len(), 1);
    assert_eq!(task.depends_on()[0].as_str(), "alpha");
}

#[test]
fn condition_accessor_returns_inner_option() {
    use wyrd_spec::vala::eval::condition::EvalCondition;

    let mut assertion_task = match assertion("c", vec![]) {
        EvalTask::Assertion(assertion_task) => assertion_task,
        _ => unreachable!(),
    };
    assertion_task.condition = Some(EvalCondition {
        path: JsonPath::new("$.flag").unwrap(),
        operator: ComparisonOperator::IsTruthy,
        expected: serde_json::Value::Null,
        combinator: None,
        subsequent: None,
    });
    let task = EvalTask::Assertion(assertion_task);
    assert!(task.condition().is_some());
}

#[test]
fn discriminator_matches_serde_tag() {
    let cases: &[(EvalTask, &str)] = &[
        (assertion("a", vec![]), "assertion"),
        (
            EvalTask::TraceAssertion(TraceAssertionTask {
                id: TaskId::new("t").unwrap(),
                span_selector: JsonPath::new("$.spans").unwrap(),
                operator: ComparisonOperator::IsNonEmpty,
                expected: serde_json::Value::Null,
                depends_on: vec![],
                condition: None,
            }),
            "trace_assertion",
        ),
        (
            EvalTask::AgentAssertion(AgentAssertionTask {
                id: TaskId::new("ag").unwrap(),
                workflow_field_path: JsonPath::new("$.tool_calls").unwrap(),
                operator: ComparisonOperator::IsNonEmpty,
                expected: serde_json::Value::Null,
                depends_on: vec![],
                condition: None,
            }),
            "agent_assertion",
        ),
    ];
    for (task, want) in cases {
        assert_eq!(task.discriminator(), *want);
        let value = serde_json::to_value(task).unwrap();
        assert_eq!(value["kind"].as_str().unwrap(), *want);
    }
}

#[test]
fn rejects_unknown_kind() {
    let json = r#"{"kind":"conditional","id":"x"}"#;
    let result: Result<EvalTask, _> = serde_json::from_str(json);
    assert!(
        result.is_err(),
        "predecessor Conditional kind must be rejected"
    );
}

#[test]
fn rejects_human_validation_kind() {
    let json = r#"{"kind":"human_validation","id":"x"}"#;
    let result: Result<EvalTask, _> = serde_json::from_str(json);
    assert!(
        result.is_err(),
        "predecessor HumanValidation kind must be rejected"
    );
}
