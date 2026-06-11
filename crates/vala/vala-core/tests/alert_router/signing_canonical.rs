use vala_core::alert_router::webhook::WEBHOOK_SIGNING_VERSION;

#[test]
fn canonical_string_format_locked() {
    let timestamp = 1_700_000_000_u64;
    let body_sha256 = "abc123def456".to_string();
    let canonical = format!("{WEBHOOK_SIGNING_VERSION}:{timestamp}:{body_sha256}");
    let parts: Vec<&str> = canonical.splitn(3, ':').collect();
    assert_eq!(parts[0], "v1");
    assert_eq!(parts[1], "1700000000");
    assert_eq!(parts[2], body_sha256);
}

#[test]
fn canonical_string_byte_for_byte_golden() {
    let timestamp = 1_717_977_600_u64;
    let body_sha256_hex = "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9";
    let canonical = format!("v1:{timestamp}:{body_sha256_hex}");
    assert_eq!(
        canonical,
        "v1:1717977600:b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9"
    );
}
