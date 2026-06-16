use wyrd_spec::DataTenantId;
use wyrd_storage::tenant_path::{build, strip_bucket, validate};

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

#[test]
fn strip_bucket_returns_tenant_prefixed_path_for_supported_schemes() {
    let tenant = DataTenantId::new_v7();
    let s3 = url::Url::parse(&format!("s3://bucket/{tenant}/cards/x/y.parquet")).expect("s3 uri");
    let gs = url::Url::parse(&format!("gs://bucket/{tenant}/cards/x/y.parquet")).expect("gs uri");
    let file =
        url::Url::parse(&format!("file://localhost/{tenant}/cards/x/y.parquet")).expect("file uri");
    let az = url::Url::parse(&format!(
        "az://account/container/{tenant}/cards/x/y.parquet"
    ))
    .expect("az uri");

    assert_eq!(
        strip_bucket("s3", &s3).expect("s3 path"),
        format!("{tenant}/cards/x/y.parquet")
    );
    assert_eq!(
        strip_bucket("gs", &gs).expect("gs path"),
        format!("{tenant}/cards/x/y.parquet")
    );
    assert_eq!(
        strip_bucket("file", &file).expect("file path"),
        format!("{tenant}/cards/x/y.parquet")
    );
    assert_eq!(
        strip_bucket("az", &az).expect("az path"),
        format!("{tenant}/cards/x/y.parquet")
    );
}

#[test]
fn strip_bucket_rejects_unknown_uri_scheme() {
    let uri = url::Url::parse("http://example.test/tenant/cards/x/y.parquet").expect("http uri");

    let error = strip_bucket("http", &uri).expect_err("unsupported scheme should fail");

    assert!(matches!(error, wyrd_storage::StorageError::InvalidUri(_)));
}

#[test]
fn strip_bucket_rejects_azure_uri_without_tenant_segment() {
    for value in ["az://account/container", "az://account/container/"] {
        let uri = url::Url::parse(value).expect("az uri");

        let error = strip_bucket("az", &uri).expect_err("missing tenant segment should fail");

        assert!(matches!(error, wyrd_storage::StorageError::InvalidUri(_)));
    }
}
