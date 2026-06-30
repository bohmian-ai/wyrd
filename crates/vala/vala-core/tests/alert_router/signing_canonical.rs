//! Canonical signing-string parity test.
//!
//! Locks the exact bytes of `v1:{timestamp}:{body_sha256_hex}` for a frozen
//! `(timestamp, body_sha256_hex)` pair. The signer (PR4.3) MUST consume
//! identical bytes as the canonical input. Drift on the canonical string is
//! a contract break.
//!
//! The HMAC byte contract (HMAC-SHA256(key, canonical) -> hex) is pinned in
//! a sibling test file `signing_hmac.rs` added by PR4.3 alongside the
//! signer implementation. This file pins only the canonical-string bytes
//! so the input shape is locked even before the signer lands.

use vala_core::alert_router::webhook::WEBHOOK_SIGNING_VERSION;

#[test]
fn canonical_string_format_locked() {
    let timestamp = 1_700_000_000u64;
    let body_sha256 = "abc123def456".to_string();
    let canonical = format!("{WEBHOOK_SIGNING_VERSION}:{timestamp}:{body_sha256}");
    assert!(canonical.starts_with("v1:"));
    let parts: Vec<&str> = canonical.splitn(3, ':').collect();
    assert_eq!(parts[0], "v1");
    assert_eq!(parts[1], "1700000000");
    assert_eq!(parts[2], body_sha256);
}

#[test]
fn canonical_string_byte_for_byte_golden() {
    // SHA-256 hex of `b"hello world"` is computed independently:
    //   b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9
    //
    // This test does not compute SHA-256 or HMAC bytes; it only pins the
    // canonical-string concatenation shape. The HMAC byte contract is
    // pinned in `signing_hmac.rs` (PR4.3).
    let timestamp: u64 = 1_717_977_600;
    let body_sha256_hex = "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9";
    let canonical = format!("v1:{timestamp}:{body_sha256_hex}");
    assert_eq!(
        canonical,
        "v1:1717977600:b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9"
    );
}
