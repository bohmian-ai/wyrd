use wyrd_spec::DataTenantId;
use wyrd_storage::tenant_path::{build, validate};

#[test]
fn validate_accepts_current_uuid7_card_uid_shape() {
    let tenant = DataTenantId::new_v7();
    let card_uid = uuid::Uuid::now_v7().to_string();
    let path = build(tenant, &card_uid, "models/model.bin");

    let validated = validate(&path, tenant).expect("valid path");

    assert_eq!(validated.full, path);
    assert_eq!(validated.data_tenant_id, tenant);
    assert_eq!(validated.card_uid, card_uid);
    assert_eq!(validated.relative_path, "models/model.bin");
}

#[test]
fn validate_rejects_foreign_tenant() {
    let tenant = DataTenantId::new_v7();
    let other = DataTenantId::new_v7();
    let card_uid = uuid::Uuid::now_v7().to_string();
    let path = build(other, &card_uid, "artifact.bin");

    let err = validate(&path, tenant).expect_err("foreign tenant should fail");

    assert!(err.to_string().contains("does not match"));
}

#[test]
fn validate_rejects_parent_segments() {
    let tenant = DataTenantId::new_v7();
    let card_uid = uuid::Uuid::now_v7().to_string();
    let path = build(tenant, &card_uid, "models/../model.bin");

    let err = validate(&path, tenant).expect_err("parent segment should fail");

    assert!(err.to_string().contains("forbidden segment"));
}

#[test]
fn validate_rejects_backslashes() {
    let tenant = DataTenantId::new_v7();
    let card_uid = uuid::Uuid::now_v7().to_string();
    let path = build(tenant, &card_uid, "models\\model.bin");

    let err = validate(&path, tenant).expect_err("backslash should fail");

    assert!(err.to_string().contains("required shape"));
}
