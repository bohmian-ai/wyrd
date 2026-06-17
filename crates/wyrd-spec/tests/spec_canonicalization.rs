use std::str::FromStr;

use wyrd_spec::envelope::{SpecHash, SpecHashParseError};

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
