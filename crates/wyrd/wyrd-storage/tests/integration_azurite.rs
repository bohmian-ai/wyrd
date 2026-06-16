use base64::Engine;
use std::time::Duration;
use wyrd_spec::DataTenantId;
use wyrd_spec::storage::UploadPlan;
use wyrd_storage::factory::azure::build_emulator_signer;

const AZURITE_ENDPOINT: &str = "http://127.0.0.1:10000";
const AZURITE_CONTAINER: &str = "wyrd-storage-test";

#[tokio::test]
async fn azure_block_blob_round_trip_when_enabled() {
    if std::env::var("WYRD_STORAGE_INTEGRATION_AZURE").as_deref() != Ok("1") {
        eprintln!("skipping Azurite integration test; set WYRD_STORAGE_INTEGRATION_AZURE=1");
        return;
    }

    let endpoint = std::env::var("WYRD_AZURE_EMULATOR_ENDPOINT")
        .unwrap_or_else(|_| AZURITE_ENDPOINT.to_owned());

    let signer = build_emulator_signer(AZURITE_CONTAINER, &endpoint).expect("azurite signer");

    let tenant = DataTenantId::new_v7();
    let card_uid = uuid::Uuid::now_v7();
    let full = wyrd_storage::tenant_path::build(tenant, &card_uid.to_string(), "model.bin");
    let path = wyrd_storage::tenant_path::validate(&full, tenant).expect("tenant path");

    let content = b"hello azurite emulator";

    let init = signer
        .init_multipart(&path, 1, content.len() as u64, Duration::from_mins(10))
        .await
        .expect("init multipart");

    let UploadPlan::AzureBlockBlob { sas_url, .. } = init.plan else {
        panic!("expected AzureBlockBlob plan");
    };

    // Block ID must be base64-encoded; match the azure_block_id(0) format used by complete_blocklist_server.
    let block_id_b64 = base64::engine::general_purpose::STANDARD.encode(b"000000");
    let block_url = format!("{sas_url}&comp=block&blockid={block_id_b64}");

    let http = reqwest::Client::new();
    let response = http
        .put(&block_url)
        .body(content.to_vec())
        .send()
        .await
        .expect("stage block");
    assert!(
        response.status().is_success(),
        "stage block failed: {}",
        response.status()
    );

    signer
        .complete_blocklist_server(&path, 1)
        .await
        .expect("commit block list");

    let head = signer.head(&path).await.expect("head after commit");
    assert_eq!(head.size_bytes, content.len() as u64);
}

#[tokio::test]
async fn azure_abort_is_not_silent_success_with_emulator() {
    if std::env::var("WYRD_STORAGE_INTEGRATION_AZURE").as_deref() != Ok("1") {
        eprintln!("skipping Azurite integration test; set WYRD_STORAGE_INTEGRATION_AZURE=1");
        return;
    }

    let endpoint = std::env::var("WYRD_AZURE_EMULATOR_ENDPOINT")
        .unwrap_or_else(|_| AZURITE_ENDPOINT.to_owned());

    let signer = build_emulator_signer(AZURITE_CONTAINER, &endpoint).expect("azurite signer");

    let tenant = DataTenantId::new_v7();
    let full = wyrd_storage::tenant_path::build(
        tenant,
        &uuid::Uuid::now_v7().to_string(),
        "nonexistent.bin",
    );
    let path = wyrd_storage::tenant_path::validate(&full, tenant).expect("tenant path");

    let result = signer.abort_multipart(&path).await;

    assert!(
        result.is_err(),
        "aborting a non-existent blob must return Err, not Ok(())"
    );
}
