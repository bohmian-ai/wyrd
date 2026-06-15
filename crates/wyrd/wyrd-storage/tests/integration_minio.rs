use std::time::Duration;
use wyrd_spec::DataTenantId;
use wyrd_spec::storage::{S3CompletedPart, UploadPlan};
use wyrd_storage::s3::S3Signer;

#[tokio::test]
async fn minio_s3_multipart_round_trip_when_enabled() {
    if std::env::var("WYRD_STORAGE_INTEGRATION_S3").as_deref() != Ok("1") {
        eprintln!("skipping MinIO integration test; set WYRD_STORAGE_INTEGRATION_S3=1");
        return;
    }

    let signer = build_minio_signer().await;
    let tenant = DataTenantId::new_v7();
    let card_uid = uuid::Uuid::now_v7();
    let full = wyrd_storage::tenant_path::build(tenant, &card_uid.to_string(), "matrix/big.bin");
    let path = wyrd_storage::tenant_path::validate(&full, tenant).expect("tenant path");

    let init = signer
        .init_multipart(&path, 2, 5 * 1024 * 1024, Duration::from_mins(10))
        .await
        .expect("init multipart");
    assert!(matches!(
        init.plan,
        UploadPlan::S3Multipart {
            part_count: 2,
            part_size_bytes: 5_242_880,
            ..
        }
    ));

    let http = reqwest::Client::new();
    let mut parts = Vec::new();
    for part_number in 1..=2 {
        let payload_byte = if part_number == 1 { 1 } else { 2 };
        let url = signer
            .presign_part(
                &path,
                &init.backend_upload_id,
                part_number,
                Duration::from_mins(10),
            )
            .await
            .expect("presign part");
        let response = http
            .put(url)
            .body(vec![payload_byte; 5 * 1024 * 1024])
            .send()
            .await
            .expect("upload part");
        assert!(
            response.status().is_success(),
            "part upload failed: {}",
            response.status()
        );
        let e_tag = response
            .headers()
            .get("etag")
            .expect("etag header")
            .to_str()
            .expect("etag string")
            .to_owned();
        parts.push(S3CompletedPart { part_number, e_tag });
    }

    signer
        .complete_multipart(&path, &init.backend_upload_id, &parts)
        .await
        .expect("complete multipart");
    let head = signer.head(&path).await.expect("head completed object");
    assert_eq!(head.size_bytes, 10 * 1024 * 1024);
}

async fn build_minio_signer() -> S3Signer {
    use aws_config::BehaviorVersion;
    use aws_sdk_s3::Client;
    use aws_sdk_s3::config::{Builder, Credentials, Region};

    let shared = aws_config::defaults(BehaviorVersion::latest()).load().await;
    let config = Builder::from(&shared)
        .region(Region::new("us-east-1"))
        .endpoint_url("http://localhost:9000")
        .force_path_style(true)
        .credentials_provider(Credentials::new(
            "wyrd-test-key",
            "wyrd-test-secret",
            None,
            None,
            "static",
        ))
        .build();

    S3Signer::new(Client::from_conf(config), "wyrd-storage-test".to_owned())
}
