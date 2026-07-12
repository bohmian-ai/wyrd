#[tokio::test]
async fn azure_abort_is_not_silent_success_when_no_blob_exists() {
    use azure_storage::StorageCredentials;
    use azure_storage_blobs::prelude::BlobServiceClient;
    use wyrd_spec::DataTenantId;
    use wyrd_storage::azure::{AzureSasMode, AzureSigner};
    use wyrd_storage::cloud::CloudSigner;
    use wyrd_storage::signer::BackendSigner;
    use wyrd_storage::tenant_path::ValidatedPath;

    let service_client = BlobServiceClient::new(
        "devstoreaccount1",
        StorageCredentials::access_key(
            "devstoreaccount1",
            "Eby8vdM02xNOcqFlqUwJPLlmEtlCDXJ1OUzFT50uSRZ6IFsuFq2UVErCz4I6tq/K1SZFPTOtr/KBHBeksoGMGw==",
        ),
    );
    let tenant = DataTenantId::new_v7();
    let signer = BackendSigner::Cloud(Box::new(CloudSigner::Azure(AzureSigner::new(
        service_client,
        "test-container".to_owned(),
        AzureSasMode::AccountKey,
    ))));
    let path = ValidatedPath {
        full: format!("{tenant}/cards/card/model.bin"),
        data_tenant_id: tenant,
        card_uid: "card".to_owned(),
        relative_path: "model.bin".to_owned(),
    };

    let result = signer.abort_multipart(&path, "unused").await;

    assert!(
        result.is_err(),
        "Azure abort must return Err when no blob exists, not Ok(())"
    );
    let err = result.unwrap_err();
    assert!(
        !matches!(
            err,
            wyrd_storage::error::StorageError::BackendCapabilityMismatch { .. }
        ),
        "Azure abort must dispatch to AzureSigner, not return capability mismatch"
    );
}
