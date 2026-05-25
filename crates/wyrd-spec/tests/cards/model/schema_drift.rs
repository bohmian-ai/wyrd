use std::fs;

use schemars::schema_for;
use wyrd_spec::card::model::{
    HuggingFaceTask, ModelInterface, ModelSignature, ModelSpec, SampleInput, SampleInputKind,
    TaskType, TfSaveFormat, TorchSaveFormat,
};

fn snapshot_path(file_name: &str) -> String {
    format!("{}/tests/schemas/{file_name}", env!("CARGO_MANIFEST_DIR"))
}

fn assert_schema_matches_snapshot<T: schemars::JsonSchema>(file_name: &str) {
    let mut schema = schema_for!(T);
    schema.meta_schema = Some("https://json-schema.org/draft/2020-12/schema".to_string());
    let actual = format!("{}\n", serde_json::to_string_pretty(&schema).unwrap());
    let path = snapshot_path(file_name);
    let expected =
        fs::read_to_string(&path).unwrap_or_else(|error| panic!("missing {path}: {error}"));
    assert_eq!(actual, expected, "schema drift in {file_name}");
}

#[test]
fn model_spec_schema_matches_snapshot() {
    assert_schema_matches_snapshot::<ModelSpec>("model_spec.json");
}

#[test]
fn model_interface_schema_matches_snapshot() {
    assert_schema_matches_snapshot::<ModelInterface>("model_interface.json");
}

#[test]
fn task_type_schema_matches_snapshot() {
    assert_schema_matches_snapshot::<TaskType>("task_type.json");
}

#[test]
fn model_signature_schema_matches_snapshot() {
    assert_schema_matches_snapshot::<ModelSignature>("model_signature.json");
}

#[test]
fn sample_input_schemas_match_snapshots() {
    assert_schema_matches_snapshot::<SampleInput>("sample_input.json");
    assert_schema_matches_snapshot::<SampleInputKind>("sample_input_kind.json");
}

#[test]
fn torch_save_format_schema_matches_snapshot() {
    assert_schema_matches_snapshot::<TorchSaveFormat>("torch_save_format.json");
}

#[test]
fn tf_save_format_schema_matches_snapshot() {
    assert_schema_matches_snapshot::<TfSaveFormat>("tf_save_format.json");
}

#[test]
fn hugging_face_task_schema_matches_snapshot() {
    assert_schema_matches_snapshot::<HuggingFaceTask>("hugging_face_task.json");
}
