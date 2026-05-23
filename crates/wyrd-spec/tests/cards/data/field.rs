use std::collections::BTreeMap;

use wyrd_spec::card::{Dim, FieldSpec};
use wyrd_spec::ids::ColumnName;

#[test]
fn field_spec_constructor_builds_scalar_field() {
    let field = FieldSpec::new(ColumnName::new("amount").unwrap(), "float64");
    assert_eq!(field.name.as_str(), "amount");
    assert_eq!(field.dtype, "float64");
    assert!(field.shape.is_empty());
    assert!(!field.nullable);
    assert!(field.extra.is_empty());
}

#[test]
fn field_spec_round_trips_with_shape_and_extra() {
    let field = FieldSpec {
        name: ColumnName::new("embedding").unwrap(),
        dtype: "float32".to_string(),
        shape: vec![Dim::Fixed(768), Dim::Dynamic(Some("batch".to_string()))],
        nullable: true,
        extra: BTreeMap::from([("source".to_string(), "test".to_string())]),
    };

    let json = serde_json::to_string(&field).unwrap();
    let round_tripped: FieldSpec = serde_json::from_str(&json).unwrap();
    assert_eq!(round_tripped, field);
}

#[test]
fn card_column_names_accept_one_character_tokens() {
    assert!(ColumnName::new("x").is_ok());
    assert!(ColumnName::new("X").is_err());
}
