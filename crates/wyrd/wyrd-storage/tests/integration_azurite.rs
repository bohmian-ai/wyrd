//! Azure signer contract tests against the Azurite emulator.
//!
//! Run by `test:storage:azurite` (part of `test:storage:matrix`), which selects
//! the emulator through `WYRD_STORAGE_URL` and `WYRD_STORAGE_ENDPOINT_URL`.

//! Azure Blob signer-layer integration tests against Azurite.
//!
//! The client-to-server transfer journey owns block upload and download
//! coverage. These tests retain Azure-specific planning, cleanup, head, and
//! capability behavior.

use std::time::Duration;

use wyrd_spec::DataTenantId;
use wyrd_spec::storage::{StorageBackendKind, UploadPlan, WireProtocol};
use wyrd_storage::azure::AzureSigner;
use wyrd_storage::cloud::CloudSigner;
use wyrd_storage::error::{AzureError, StorageError};
use wyrd_storage::settings::{self, BackendConfig};
use wyrd_storage::{BackendSigner, UploadPlanReplayInput, ValidatedPath, factory};

/// Block size used for multipart init and replay plans.
const BLOCK_SIZE: u64 = 256 * 1024;
/// Planned block count used for replay plans.
const BLOCK_COUNT: u32 = 3;

/// Build the Azure signer for the location and endpoint in `WYRD_STORAGE_URL`.
///
/// # Panics
/// Panics when the environment does not select Azure or signer construction fails.
async fn build_signer() -> AzureSigner {
    let BackendConfig::Azure(config) = settings::from_env().expect("storage settings").backend
    else {
        panic!("WYRD_STORAGE_URL must select az://");
    };
    factory::azure::build_signer(&config)
        .await
        .expect("Azure signer")
}

/// Tenant- and Card-unique validated path, so concurrent runs against a shared bucket never collide.
///
/// # Panics
/// Never in practice: the freshly built path is valid for its own tenant.
fn fresh_path(suffix: &str) -> ValidatedPath {
    let tenant = DataTenantId::new_v7();
    let card_uid = uuid::Uuid::now_v7();
    let full = wyrd_storage::tenant_path::build(tenant, &card_uid.to_string(), suffix);
    wyrd_storage::tenant_path::validate(&full, tenant).expect("tenant path")
}

/// Aborting an initialized Azure block upload leaves no committed blob.
///
/// Azure has no server-side upload to cancel, so abort may report the blob as
/// already absent; either way `head` must then see `BlobNotFound`.
///
/// # Panics
/// Panics when the environment does not select Azure, init fails, abort fails
/// with any other error, or the blob exists afterwards.
#[tokio::test]
async fn azure_abort_lifecycle() {
    let signer = build_signer().await;
    let path = fresh_path("abort/object.bin");
    signer
        .init_multipart(&path, 1, BLOCK_SIZE, Duration::from_mins(5))
        .await
        .expect("init multipart");
    match signer.abort_multipart(&path).await {
        Ok(()) => {}
        Err(StorageError::Azure(boxed)) if matches!(*boxed, AzureError::BlobNotFound { .. }) => {}
        Err(error) => panic!("abort multipart: {error}"),
    }
    assert!(matches!(
        signer.head(&path).await,
        Err(StorageError::Azure(boxed)) if matches!(*boxed, AzureError::BlobNotFound { .. })
    ));
}

/// Aborting a never-written blob through [`BackendSigner`] must reach
/// [`AzureSigner`] and fail, not dispatch to a capability mismatch or report
/// silent success.
///
/// # Panics
/// Panics when the environment does not select Azure, the abort succeeds, or
/// dispatch returns `BackendCapabilityMismatch`.
#[tokio::test]
async fn azure_abort_on_nonexistent_blob_returns_error() {
    let backend = BackendSigner::Cloud(Box::new(CloudSigner::Azure(build_signer().await)));
    let error = backend
        .abort_multipart(&fresh_path("never-written.bin"), "unused")
        .await
        .expect_err("Azure abort must return Err when no blob exists, not Ok(())");
    assert!(
        !matches!(error, StorageError::BackendCapabilityMismatch { .. }),
        "Azure abort must dispatch to AzureSigner, not return capability mismatch: {error}"
    );
}

/// Replaying a stored Azure block-blob plan yields the same block geometry.
///
/// # Panics
/// Panics when the environment does not select Azure, remint fails, or the plan
/// differs from the replay input.
#[tokio::test]
async fn azure_remint_plan_produces_valid_plan() {
    let signer = build_signer().await;
    let plan = signer
        .remint_plan(
            &fresh_path("remint/object.bin"),
            &UploadPlanReplayInput {
                wire_protocol: WireProtocol::AzureBlockBlobV1,
                backend_upload_id: None,
                part_count_planned: BLOCK_COUNT,
                part_size_bytes: BLOCK_SIZE,
                block_count_planned: Some(BLOCK_COUNT),
            },
            Duration::from_mins(5),
        )
        .await
        .expect("remint Azure plan");
    assert!(
        matches!(plan, UploadPlan::AzureBlockBlob { block_size_bytes, block_count_planned, .. } if block_size_bytes == BLOCK_SIZE && block_count_planned == BLOCK_COUNT)
    );
}

/// `head` on a never-written blob returns typed `BlobNotFound`.
///
/// # Panics
/// Panics when the environment does not select Azure or the error is untyped.
#[tokio::test]
async fn azure_head_on_missing_returns_blob_not_found() {
    assert!(matches!(
        build_signer().await.head(&fresh_path("never-written.bin")).await,
        Err(StorageError::Azure(boxed)) if matches!(*boxed, AzureError::BlobNotFound { .. })
    ));
}

/// `presign_part` through the backend dispatcher is a typed Azure capability mismatch.
///
/// # Panics
/// Panics when the environment does not select Azure or dispatch returns anything else.
#[tokio::test]
async fn azure_capability_mismatch_is_typed_for_presign_part() {
    let backend = BackendSigner::Cloud(Box::new(CloudSigner::Azure(build_signer().await)));
    let result = backend
        .presign_part(
            &fresh_path("capability/object.bin"),
            "ignored",
            1,
            Duration::from_secs(1),
        )
        .await;
    assert!(matches!(
        result,
        Err(StorageError::BackendCapabilityMismatch {
            signer: StorageBackendKind::Azure,
            op: "presign_part"
        })
    ));
}
