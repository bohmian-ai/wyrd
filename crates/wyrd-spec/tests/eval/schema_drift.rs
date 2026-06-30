use std::fs;
use std::path::PathBuf;

use schemars::schema_for;
use wyrd_spec::vala::eval::{
    ComparisonOperator, DagError, EvalCondition, EvalPassGate, EvalSampling,
    EvalScenarioCollection, EvalSpec, EvalTask, ExecutionPlan,
};

fn fixture_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/eval/schemas")
}

fn assert_schema_matches<T: schemars::JsonSchema>(name: &str) {
    let mut schema = schema_for!(T);
    schema.meta_schema = Some("https://json-schema.org/draft/2020-12/schema".to_string());
    let actual = format!(
        "{}\n",
        serde_json::to_string_pretty(&schema).expect("schema serializes")
    );
    let path = fixture_dir().join(format!("{name}.schema.json"));
    let expected = fs::read_to_string(&path).unwrap_or_else(|error| {
        panic!(
            "missing golden schema at {path:?}: {error}; regenerate with \
             `cargo run -p wyrd-spec --example gen_schemas --features server`",
        )
    });
    assert_eq!(
        actual, expected,
        "schema drift in {name}.schema.json; run mise run codegen:regen"
    );
}

#[test]
fn eval_spec_schema() {
    assert_schema_matches::<EvalSpec>("eval_spec");
}

#[test]
fn eval_task_schema() {
    assert_schema_matches::<EvalTask>("eval_task");
}

#[test]
fn eval_condition_schema() {
    assert_schema_matches::<EvalCondition>("eval_condition");
}

#[test]
fn comparison_operator_schema() {
    assert_schema_matches::<ComparisonOperator>("comparison_operator");
}

#[test]
fn execution_plan_schema() {
    assert_schema_matches::<ExecutionPlan>("execution_plan");
}

#[test]
fn dag_error_schema() {
    assert_schema_matches::<DagError>("dag_error");
}

#[test]
fn eval_scenario_collection_schema() {
    assert_schema_matches::<EvalScenarioCollection>("eval_scenario_collection");
}

#[test]
fn eval_pass_gate_schema() {
    assert_schema_matches::<EvalPassGate>("eval_pass_gate");
}

#[test]
fn eval_sampling_schema() {
    assert_schema_matches::<EvalSampling>("eval_sampling");
}
