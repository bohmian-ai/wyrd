use std::collections::BTreeMap;

use wyrd_spec::vala::eval::{Workflow, WorkflowFieldType};

#[test]
fn workflow_serializes_field_map_ordered() {
    let mut fields = BTreeMap::new();
    fields.insert("response".to_string(), WorkflowFieldType::String);
    fields.insert("score".to_string(), WorkflowFieldType::Float);

    let workflow = Workflow { fields };
    let json = serde_json::to_string(&workflow).expect("workflow serializes");
    assert_eq!(json, r#"{"fields":{"response":"string","score":"float"}}"#);
}

#[test]
fn workflow_field_type_round_trips() {
    for field_type in [
        WorkflowFieldType::String,
        WorkflowFieldType::Integer,
        WorkflowFieldType::Float,
        WorkflowFieldType::Boolean,
        WorkflowFieldType::Object,
        WorkflowFieldType::Array,
        WorkflowFieldType::Any,
    ] {
        let json = serde_json::to_string(&field_type).expect("field type serializes");
        let round_trip: WorkflowFieldType =
            serde_json::from_str(&json).expect("field type deserializes");
        assert_eq!(field_type, round_trip);
    }
}
