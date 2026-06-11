//! No-`test-utils` conditional-compilation gate for `SecretRef::Inline`.

use wyrd_spec::security::SecretRef;

#[test]
#[cfg(not(feature = "test-utils"))]
fn inline_source_rejected_when_test_utils_off() {
    let payload = r#"{"source":"inline","value":"x"}"#;
    let result: Result<SecretRef, _> = serde_json::from_str(payload);
    let error = result.expect_err("inline must be absent when test-utils is off");
    let message = error.to_string();
    assert!(
        message.contains("unknown variant") && message.contains("inline"),
        "expected serde to refuse inline, got: {message}"
    );
}

#[test]
#[cfg(feature = "test-utils")]
fn inline_source_gate_skipped_when_test_utils_on() {
    let _ = std::any::type_name::<SecretRef>();
}
