use std::str::FromStr;
use wyrd_storage::UploadId;

#[test]
fn upload_id_round_trips_display_parse() {
    let upload_id = UploadId::new();
    let parsed = UploadId::from_str(&upload_id.to_string()).expect("valid upload id parses");

    assert_eq!(parsed, upload_id);
    assert!(upload_id.to_string().starts_with("wyu_"));
}

#[test]
fn upload_id_rejects_missing_prefix() {
    let err = UploadId::from_str("01J0Z4M7JZ4M7JZ4M7JZ4M7JZ4").expect_err("missing prefix fails");

    assert!(err.to_string().contains("wyu_"));
}

#[test]
fn upload_id_rejects_bad_ulid_body() {
    let err = UploadId::from_str("wyu_not-a-ulid").expect_err("bad body fails");

    assert!(err.to_string().contains("ULID"));
}
