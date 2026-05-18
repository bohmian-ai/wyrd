use proptest::prelude::*;
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

proptest! {
    #[test]
    fn native_kinds_round_trip(kind in prop::sample::select(CardKind::native().to_vec())) {
        let encoded = serde_json::to_string(&kind).unwrap();
        let decoded: CardKind = serde_json::from_str(&encoded).unwrap();
        prop_assert_eq!(decoded, kind);
    }
}
