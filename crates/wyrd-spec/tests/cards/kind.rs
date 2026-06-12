use proptest::prelude::*;
use serde_json::json;
use wyrd_spec::envelope::CardKind;

#[test]
fn native_card_kind_count_is_locked() {
    assert_eq!(CardKind::native().len(), CardKind::NATIVE_COUNT);
    assert_eq!(CardKind::NATIVE_COUNT, 16);
    assert!(
        CardKind::native()
            .iter()
            .any(|kind| matches!(kind, CardKind::Trigger))
    );
    assert!(
        CardKind::native()
            .iter()
            .any(|kind| matches!(kind, CardKind::Operator))
    );
    assert!(
        CardKind::native()
            .iter()
            .any(|kind| matches!(kind, CardKind::External))
    );
}

#[test]
fn native_card_kind_wire_shape_is_pascal_case_string() {
    let json = serde_json::to_value(CardKind::Prompt).unwrap();
    assert_eq!(json, json!("Prompt"));
}

#[test]
fn external_card_kind_wire_shape_is_string() {
    let json = serde_json::to_value(CardKind::External).unwrap();
    assert_eq!(json, json!("External"));
    let decoded: CardKind = serde_json::from_value(json).unwrap();
    assert_eq!(decoded, CardKind::External);
}

#[test]
fn card_kind_schema_is_string_enum_with_all_kinds() {
    let schema = schemars::schema_for!(CardKind);
    let value = serde_json::to_value(schema).unwrap();
    let enum_values = value["enum"].as_array().expect("enum schema");
    assert!(enum_values.contains(&json!("Data")));
    assert!(enum_values.contains(&json!("Trigger")));
    assert!(enum_values.contains(&json!("Operator")));
    assert!(enum_values.contains(&json!("External")));
    assert!(!enum_values.contains(&json!("Tool")));
    assert!(!enum_values.contains(&json!("Skill")));
    assert!(!enum_values.contains(&json!("SubAgent")));
    assert_eq!(enum_values.len(), 16);
}

proptest! {
    #[test]
    fn native_kinds_round_trip(kind in prop::sample::select(CardKind::native().to_vec())) {
        let encoded = serde_json::to_string(&kind).unwrap();
        let decoded: CardKind = serde_json::from_str(&encoded).unwrap();
        prop_assert_eq!(decoded, kind);
    }
}
