use proptest::prelude::*;
use serde_json::json;
use wyrd_spec::envelope::CardKind;

#[test]
fn native_card_kind_count_is_locked() {
    assert_eq!(CardKind::native().len(), CardKind::NATIVE_COUNT);
    assert_eq!(CardKind::NATIVE_COUNT, 16);
}

#[test]
fn external_card_kind_wire_shape_carries_hash() {
    let kind = CardKind::External {
        name: "vendor.custom".to_string(),
        schema_hash: [7; 32],
    };
    let json = serde_json::to_value(&kind).unwrap();
    assert_eq!(json["kind"], "vendor.custom");
    assert_eq!(json["schema_hash"].as_str().unwrap().len(), 64);
    let decoded: CardKind = serde_json::from_value(json).unwrap();
    assert_eq!(decoded, kind);
}

#[test]
fn native_card_kind_wire_shape_is_pascal_case_string() {
    let json = serde_json::to_value(CardKind::Prompt).unwrap();
    assert_eq!(json, json!("Prompt"));
}

#[test]
fn external_card_kind_rejects_non_canonical_hash() {
    let json = json!({
        "kind": "vendor.custom",
        "schema_hash": "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"
    });
    let err = serde_json::from_value::<CardKind>(json).unwrap_err();
    assert!(err.to_string().contains("lowercase hexadecimal"));
}

#[test]
fn card_kind_schema_matches_wire_shape() {
    let schema = schemars::schema_for!(CardKind);
    let value = serde_json::to_value(schema).unwrap();
    let one_of = value["oneOf"].as_array().expect("oneOf schema");
    assert_eq!(one_of[0]["enum"][0], json!("Data"));
    assert_eq!(
        one_of[1]["properties"]["schema_hash"]["pattern"],
        json!("^[0-9a-f]{64}$")
    );
}

proptest! {
    #[test]
    fn native_kinds_round_trip(kind in prop::sample::select(CardKind::native().to_vec())) {
        let encoded = serde_json::to_string(&kind).unwrap();
        let decoded: CardKind = serde_json::from_str(&encoded).unwrap();
        prop_assert_eq!(decoded, kind);
    }
}
