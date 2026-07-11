//! Azure Blob Storage signer-layer integration tests against Azurite.
//!
//! Every test gates on `WYRD_STORAGE_INTEGRATION_AZURE=1`. The `MultipartClient`
//! helper drives reqwest against Azurite via SAS URLs; no bytes transit the
//! Wyrd server on the upload path.

use std::time::Duration;
use wyrd_spec::DataTenantId;
use wyrd_spec::storage::{StorageBackendKind, UploadPlan, WireProtocol};
use wyrd_storage::azure::AzureSigner;
use wyrd_storage::cloud::CloudSigner;
use wyrd_storage::error::{AzureError, StorageError};
use wyrd_storage::factory::azure::build_emulator_signer;
use wyrd_storage::{BackendSigner, UploadPlanReplayInput, ValidatedPath};
use wyrd_testing::MultipartClient;

const AZURITE_ENDPOINT: &str = "http://127.0.0.1:10000";
const AZURITE_CONTAINER: &str = "wyrd-storage-test";
const BLOCK_SIZE: usize = 256 * 1024;
const BLOCK_COUNT: u32 = 3;

fn skip_unless_enabled() -> bool {
    if std::env::var("WYRD_STORAGE_INTEGRATION_AZURE").as_deref() != Ok("1") {
        eprintln!("skipping Azurite integration test; set WYRD_STORAGE_INTEGRATION_AZURE=1");
        return true;
    }
    false
}

fn endpoint() -> String {
    std::env::var("WYRD_AZURE_EMULATOR_ENDPOINT").unwrap_or_else(|_| AZURITE_ENDPOINT.to_owned())
}

fn build_signer() -> AzureSigner {
    build_emulator_signer(AZURITE_CONTAINER, &endpoint()).expect("azurite signer")
}

fn fresh_path(suffix: &str) -> ValidatedPath {
    let tenant = DataTenantId::new_v7();
    let card_uid = uuid::Uuid::now_v7();
    let full = wyrd_storage::tenant_path::build(tenant, &card_uid.to_string(), suffix);
    wyrd_storage::tenant_path::validate(&full, tenant).expect("tenant path")
}

fn pattern_block(index: u32) -> Vec<u8> {
    let byte = u8::try_from((index & 0xff) | 0x40).expect("byte fits u8");
    vec![byte; BLOCK_SIZE]
}

fn assemble_full() -> Vec<u8> {
    let mut bytes = Vec::with_capacity((BLOCK_COUNT as usize) * BLOCK_SIZE);
    for index in 0..BLOCK_COUNT {
        bytes.extend_from_slice(&pattern_block(index));
    }
    bytes
}

#[tokio::test]
async fn azure_single_put_round_trip() {
    if skip_unless_enabled() {
        return;
    }
    let signer = build_signer();
    let path = fresh_path("single/object.bin");
    let payload = b"wyrd azurite single put payload".to_vec();
    let client = MultipartClient::new();

    let plan = signer
        .presign_single_put(&path, payload.len() as u64, Duration::from_mins(5))
        .await
        .expect("presign single put");
    client
        .single_put(&plan, payload.clone())
        .await
        .expect("single put PUT");

    let head = signer.head(&path).await.expect("head after put");
    assert_eq!(head.size_bytes, payload.len() as u64);

    let get_url = signer
        .presign_get(&path, Duration::from_mins(5))
        .await
        .expect("presign get");
    let downloaded = client.download(&get_url).await.expect("download bytes");
    assert_eq!(downloaded, payload, "round-tripped bytes must match");
}

#[tokio::test]
async fn azure_multipart_round_trip_with_byte_equality() {
    if skip_unless_enabled() {
        return;
    }
    let signer = build_signer();
    let path = fresh_path("multi/large.bin");
    let expected = assemble_full();
    let client = MultipartClient::new();

    let init = signer
        .init_multipart(
            &path,
            BLOCK_COUNT,
            BLOCK_SIZE as u64,
            Duration::from_mins(10),
        )
        .await
        .expect("init multipart");
    assert!(matches!(init.plan, UploadPlan::AzureBlockBlob { .. }));

    let blocks: Vec<Vec<u8>> = (0..BLOCK_COUNT).map(pattern_block).collect();
    let count = client
        .azure_block_blob(&init.plan, blocks)
        .await
        .expect("stage blocks");
    assert_eq!(count, BLOCK_COUNT);

    signer
        .complete_blocklist_server(&path, BLOCK_COUNT)
        .await
        .expect("commit block list");

    let head = signer.head(&path).await.expect("head completed object");
    assert_eq!(head.size_bytes, expected.len() as u64);

    let get_url = signer
        .presign_get(&path, Duration::from_mins(5))
        .await
        .expect("presign get");
    let downloaded = client.download(&get_url).await.expect("download bytes");
    assert_eq!(
        downloaded.len(),
        expected.len(),
        "reassembled length must match"
    );
    assert_eq!(
        downloaded, expected,
        "byte-equality across reassembled block blob"
    );
}

#[tokio::test]
async fn azure_abort_lifecycle() {
    if skip_unless_enabled() {
        return;
    }
    let signer = build_signer();
    let path = fresh_path("abort/object.bin");
    let client = MultipartClient::new();

    let init = signer
        .init_multipart(&path, 1, BLOCK_SIZE as u64, Duration::from_mins(5))
        .await
        .expect("init multipart");
    client
        .azure_block_blob(&init.plan, vec![pattern_block(0)])
        .await
        .expect("stage single block");
    signer
        .complete_blocklist_server(&path, 1)
        .await
        .expect("commit block list before abort");

    signer
        .abort_multipart(&path)
        .await
        .expect("abort deletes committed blob");

    let head_result = signer.head(&path).await;
    match head_result {
        Err(StorageError::Azure(boxed)) => match *boxed {
            AzureError::BlobNotFound { .. } => {}
            other => panic!("expected BlobNotFound after abort, got: {other:?}"),
        },
        other => panic!("expected typed Azure error after abort, got: {other:?}"),
    }
}

#[tokio::test]
async fn azure_abort_on_nonexistent_blob_returns_error() {
    if skip_unless_enabled() {
        return;
    }
    let signer = build_signer();
    let path = fresh_path("never-written.bin");

    let result = signer.abort_multipart(&path).await;
    assert!(
        result.is_err(),
        "abort on never-written blob must return Err, not Ok(())"
    );
}

#[tokio::test]
async fn azure_remint_plan_produces_valid_plan() {
    if skip_unless_enabled() {
        return;
    }
    let signer = build_signer();
    let path = fresh_path("remint/object.bin");

    let input = UploadPlanReplayInput {
        wire_protocol: WireProtocol::AzureBlockBlobV1,
        backend_upload_id: None,
        part_count_planned: BLOCK_COUNT,
        part_size_bytes: BLOCK_SIZE as u64,
        block_count_planned: Some(BLOCK_COUNT),
    };
    let plan = signer
        .remint_plan(&path, &input, Duration::from_mins(5))
        .await
        .expect("remint Azure plan");
    let UploadPlan::AzureBlockBlob {
        sas_url,
        block_size_bytes,
        block_count_planned,
    } = plan
    else {
        panic!("expected AzureBlockBlob plan after remint");
    };
    assert_eq!(block_size_bytes, BLOCK_SIZE as u64);
    assert_eq!(block_count_planned, BLOCK_COUNT);
    assert!(!sas_url.is_empty(), "reminted SAS URL must not be empty");

    let client = MultipartClient::new();
    client
        .azure_block_blob(
            &UploadPlan::AzureBlockBlob {
                sas_url,
                block_size_bytes,
                block_count_planned,
            },
            vec![pattern_block(0)],
        )
        .await
        .expect("post-remint block stage");
    signer.abort_multipart(&path).await.ok();
}

#[tokio::test]
async fn azure_head_on_missing_returns_blob_not_found() {
    if skip_unless_enabled() {
        return;
    }
    let signer = build_signer();
    let path = fresh_path("never-written.bin");

    let result = signer.head(&path).await;
    match result {
        Err(StorageError::Azure(boxed)) => match *boxed {
            AzureError::BlobNotFound { .. } => {}
            other => panic!("expected typed AzureError::BlobNotFound, got: {other:?}"),
        },
        other => panic!("expected typed Azure error on missing head, got: {other:?}"),
    }
}

#[tokio::test]
async fn azure_capability_mismatch_is_typed_for_presign_part() {
    if skip_unless_enabled() {
        return;
    }
    let backend = BackendSigner::Cloud(CloudSigner::Azure(build_signer()));
    let path = fresh_path("capability/object.bin");

    let result = backend
        .presign_part(&path, "ignored", 1, Duration::from_secs(1))
        .await;
    match result {
        Err(StorageError::BackendCapabilityMismatch { signer, op }) => {
            assert_eq!(signer, StorageBackendKind::Azure);
            assert_eq!(op, "presign_part");
        }
        other => panic!("Azure presign_part must return BackendCapabilityMismatch, got: {other:?}"),
    }

    let input = UploadPlanReplayInput {
        wire_protocol: WireProtocol::S3MultipartV1,
        backend_upload_id: None,
        part_count_planned: BLOCK_COUNT,
        part_size_bytes: BLOCK_SIZE as u64,
        block_count_planned: Some(BLOCK_COUNT),
    };
    let result = build_signer()
        .remint_plan(&path, &input, Duration::from_mins(5))
        .await;
    match result {
        Err(StorageError::BackendCapabilityMismatch { signer, op }) => {
            assert_eq!(signer, StorageBackendKind::Azure);
            assert_eq!(op, "remint_plan");
        }
        other => panic!(
            "Azure remint_plan with non-Azure protocol must return BackendCapabilityMismatch, \
             got: {other:?}"
        ),
    }
}
