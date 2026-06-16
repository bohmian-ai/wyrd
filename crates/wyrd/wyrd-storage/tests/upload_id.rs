use std::str::FromStr;
use wyrd_spec::storage::UploadId;

#[test]
fn upload_id_round_trips_display_parse() {
    let upload_id = UploadId::new();
    let parsed = UploadId::from_str(&upload_id.to_string()).expect("valid upload id parses");

    assert_eq!(parsed, upload_id);
    assert!(upload_id.to_string().starts_with("wyu_"));
}

#[test]
fn upload_id_rejects_missing_prefix() {
    let err = UploadId::from_str("01890f28-7c4a-7cc3-98e7-4f4a3c2d1b00")
        .expect_err("missing prefix fails");

    assert!(err.to_string().contains("wyu_"));
}

#[test]
fn upload_id_rejects_bad_uuid_body() {
    let err = UploadId::from_str("wyu_not-a-uuid").expect_err("bad body fails");

    assert!(err.to_string().contains("UUIDv7"));
}

#[test]
fn upload_id_rejects_non_v7_uuid_body() {
    let err =
        UploadId::from_str("wyu_550e8400-e29b-41d4-a716-446655440000").expect_err("non-v7 fails");

    assert!(err.to_string().contains("UUIDv7"));
}
