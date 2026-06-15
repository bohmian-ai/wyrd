use std::time::Duration;
use wyrd_spec::DataTenantId;
use wyrd_spec::storage::UploadPlan;
use wyrd_storage::factory::gcs::build_emulator_signer;

const EMULATOR_HOST: &str = "http://localhost:4443/storage/v1/";
const EMULATOR_BUCKET: &str = "wyrd-storage-test";

#[tokio::test]
async fn gcs_resumable_round_trip_when_enabled() {
    if std::env::var("WYRD_STORAGE_INTEGRATION_GCS").as_deref() != Ok("1") {
        eprintln!("skipping GCS emulator integration test; set WYRD_STORAGE_INTEGRATION_GCS=1");
        return;
    }

    let host = std::env::var("WYRD_GCS_EMULATOR_HOST")
        .unwrap_or_else(|_| EMULATOR_HOST.to_owned());

    let signer = build_emulator_signer(EMULATOR_BUCKET, &host)
        .await
        .expect("emulator signer");

    let tenant = DataTenantId::new_v7();
    let card_uid = uuid::Uuid::now_v7();
    let full = wyrd_storage::tenant_path::build(tenant, &card_uid.to_string(), "matrix/big.bin");
    let path = wyrd_storage::tenant_path::validate(&full, tenant).expect("tenant path");

    let content = b"hello gcs emulator";
    let sha256 = {
        use base64::Engine;
        use sha2::{Digest, Sha256};
        base64::engine::general_purpose::STANDARD.encode(Sha256::digest(content))
    };

    let init = signer
        .init_multipart(&path, 1, content.len() as u64, Duration::from_secs(600))
        .await
        .expect("init multipart");

    let UploadPlan::GcsResumable { session_uri, .. } = init.plan else {
        panic!("expected GcsResumable plan");
    };

    let http = reqwest::Client::new();
    let response = http
        .put(&session_uri)
        .header("Content-Type", "application/octet-stream")
        .header(
            "Content-Range",
            format!("bytes 0-{}/{}", content.len() - 1, content.len()),
        )
        .body(content.to_vec())
        .send()
        .await
        .expect("upload content");
    assert!(
        response.status().is_success() || response.status().as_u16() == 308,
        "upload failed: {}",
        response.status()
    );

    let head = signer.head(&path).await.expect("head after upload");
    assert_eq!(head.size_bytes, content.len() as u64);

    signer.verify_sha256(&path, &sha256, &head).await.expect("sha256 verify");
}
