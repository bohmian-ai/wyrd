use std::str::FromStr;

use wyrd_spec::card::policy::PolicySpec;
use wyrd_spec::envelope::{Spec, SpecHash, SpecHashParseError};

#[test]
fn test_spec_hash_round_trips_through_text_form() {
    let bytes = b"hello world";
    let h = SpecHash::from_canonical_bytes(bytes);
    let s = h.as_str();
    assert_eq!(s.len(), 64);
    let h2 = SpecHash::from_str(s).expect("valid hex");
    assert_eq!(h, h2);
}

#[test]
fn test_spec_hash_rejects_invalid_hex_length() {
    let err = SpecHash::from_str("abc").unwrap_err();
    assert!(matches!(err, SpecHashParseError::InvalidLength { len: 3 }));

    let short = "a".repeat(63);
    let err2 = SpecHash::from_str(&short).unwrap_err();
    assert!(matches!(
        err2,
        SpecHashParseError::InvalidLength { len: 63 }
    ));
}

#[test]
fn test_spec_hash_rejects_uppercase_hex_chars() {
    let upper = "A".repeat(64);
    let err = SpecHash::from_str(&upper).unwrap_err();
    assert!(matches!(err, SpecHashParseError::InvalidChar { pos: 0 }));
}

#[test]
fn test_spec_hash_display_matches_as_str() {
    let bytes = b"test display";
    let h = SpecHash::from_canonical_bytes(bytes);
    assert_eq!(format!("{h}"), h.as_str());
}

#[test]
fn canonical_hash_is_deterministic() {
    let spec = Spec::Policy(PolicySpec::default());
    let h1 = spec.canonical_hash().expect("canonicalization must succeed");
    let h2 = spec.canonical_hash().expect("canonicalization must succeed");
    assert_eq!(h1, h2);
    assert_eq!(h1.as_str().len(), 64);
}

#[test]
fn canonical_hash_is_field_order_invariant() {
    let json_a = r#"{"description":"test","enforcement":"warn"}"#;
    let json_b = r#"{"enforcement":"warn","description":"test"}"#;
    let spec_a: PolicySpec = serde_json::from_str(json_a).expect("valid policy spec");
    let spec_b: PolicySpec = serde_json::from_str(json_b).expect("valid policy spec");
    let h_a = Spec::Policy(spec_a)
        .canonical_hash()
        .expect("canonicalization must succeed");
    let h_b = Spec::Policy(spec_b)
        .canonical_hash()
        .expect("canonicalization must succeed");
    assert_eq!(h_a, h_b, "JCS must produce the same hash regardless of field order");
}

#[test]
fn canonical_hash_changes_on_spec_mutation() {
    let spec_a = Spec::Policy(PolicySpec::default());
    let spec_b = Spec::Policy(PolicySpec {
        description: Some("mutated".to_owned()),
        ..Default::default()
    });
    let h_a = spec_a.canonical_hash().expect("canonicalization must succeed");
    let h_b = spec_b.canonical_hash().expect("canonicalization must succeed");
    assert_ne!(h_a, h_b, "mutating a field must change the hash");
}
