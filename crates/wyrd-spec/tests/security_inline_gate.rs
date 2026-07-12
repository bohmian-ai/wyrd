//! No-`test-utils` conditional-compilation gate.
//!
//! Proves that `SecretRef::Inline` is absent from `wyrd-spec` when the
//! `test-utils` feature is off: the shape of every production build and the
//! shape of the committed JSON Schema goldens.
//!
//! `check:security-inline-gate` runs this test with
//! `--no-default-features --features server`.

use wyrd_spec::security::SecretRef;

#[test]
#[cfg(not(feature = "test-utils"))]
fn inline_source_rejected_when_test_utils_off() {
    let payload = r#"{"source":"inline","value":"x"}"#;
    let r: Result<SecretRef, _> = serde_json::from_str(payload);
    let err = r.expect_err("Inline must NOT be constructable when `test-utils` is off");
    let msg = err.to_string();
    assert!(
        msg.contains("unknown variant") && msg.contains("inline"),
        "expected serde to refuse `inline`, got: {msg}"
    );
}

#[test]
#[cfg(feature = "test-utils")]
fn inline_source_gate_skipped_when_test_utils_on() {
    // No-op in `--all-features` runs. The gate is meaningful only in the
    // `check:security-inline-gate` invocation.
    let _ = std::any::type_name::<SecretRef>();
}
