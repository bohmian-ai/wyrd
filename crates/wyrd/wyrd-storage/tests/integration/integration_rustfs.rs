//! S3 signer contract tests against the `RustFS` emulator.
//!
//! Run by `test:storage:rustfs` (part of `test:storage:matrix`), which selects
//! the emulator through `WYRD_STORAGE_URL` and `WYRD_STORAGE_ENDPOINT_URL`.

//! S3 (`RustFS`) signer-layer integration tests.
//!
//! Transfer round trips live in the server journey and use
//! `WyrdStorageClient`. These tests retain only signer-specific planning,
//! cleanup, head, and capability behavior.

use std::time::Duration;

use wyrd_spec::DataTenantId;
use wyrd_spec::storage::{StorageBackendKind, UploadPlan, WireProtocol};
use wyrd_storage::error::{S3Error, StorageError};
use wyrd_storage::s3::S3Signer;
use wyrd_storage::settings::{self, BackendConfig};
use wyrd_storage::{UploadPlanReplayInput, ValidatedPath, factory};

/// Smallest S3 multipart part size.
const PART_SIZE: usize = 5 * 1024 * 1024;
/// Planned part count for multipart init and replay.
const PART_COUNT: u32 = 3;

/// Build the S3 signer for the bucket and endpoint in `WYRD_STORAGE_URL`.
///
/// # Panics
/// Panics when the environment does not select S3 or the bucket probe fails.
async fn build_rustfs_signer() -> S3Signer {
    let BackendConfig::S3(config) = settings::from_env().expect("storage settings").backend else {
        panic!("WYRD_STORAGE_URL must select s3://");
    };
    factory::s3::build_signer(&config).await.expect("S3 signer")
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

/// Aborting an initialized S3 multipart upload leaves no object.
///
/// # Panics
/// Panics when the environment does not select S3, init or abort fails, or the
/// object exists afterwards.
#[tokio::test]
async fn rustfs_abort_lifecycle() {
    let signer = build_rustfs_signer().await;
    let path = fresh_path("abort/object.bin");
    let init = signer
        .init_multipart(&path, PART_COUNT, PART_SIZE as u64, Duration::from_mins(5))
        .await
        .expect("init multipart");
    signer
        .abort_multipart(&path, &init.backend_upload_id)
        .await
        .expect("abort multipart");
    assert!(signer.head(&path).await.is_err());
}

/// Replaying a stored S3 multipart plan yields the same part geometry, then aborts the upload.
///
/// # Panics
/// Panics when the environment does not select S3, init, remint, or cleanup abort
/// fails, or the plan differs. A panic before cleanup leaves one incomplete
/// multipart upload for bucket lifecycle rules to expire.
#[tokio::test]
async fn rustfs_remint_plan_produces_valid_plan() {
    let signer = build_rustfs_signer().await;
    let path = fresh_path("remint/object.bin");
    let init = signer
        .init_multipart(&path, PART_COUNT, PART_SIZE as u64, Duration::from_mins(5))
        .await
        .expect("init multipart");
    let input = UploadPlanReplayInput {
        wire_protocol: WireProtocol::S3MultipartV1,
        backend_upload_id: Some(init.backend_upload_id.clone()),
        part_count_planned: PART_COUNT,
        part_size_bytes: PART_SIZE as u64,
        block_count_planned: None,
    };
    let reminted = signer
        .remint_plan(&path, &input, Duration::from_mins(5))
        .await
        .expect("remint S3 plan");
    assert!(
        matches!(reminted, UploadPlan::S3Multipart { part_count, part_size_bytes, .. } if part_count == PART_COUNT && part_size_bytes == PART_SIZE as u64)
    );
    signer
        .abort_multipart(&path, &init.backend_upload_id)
        .await
        .expect("cleanup abort");
}

/// `head` on a never-written key returns a typed S3 error.
///
/// # Panics
/// Panics when the environment does not select S3, `head` succeeds, or the error is untyped.
#[tokio::test]
async fn rustfs_head_on_missing_returns_error() {
    let signer = build_rustfs_signer().await;
    let result = signer.head(&fresh_path("never-written.bin")).await;
    match result {
        Err(StorageError::S3(boxed)) => match *boxed {
            S3Error::NoSuchKey { .. } | S3Error::Sdk(_) => {}
            other => panic!("expected typed missing-key error, got: {other:?}"),
        },
        Err(other) => panic!("expected typed S3 error, got: {other:?}"),
        Ok(head) => panic!("missing path returned {head:?}"),
    }
}

/// Replaying GCS or Azure plans through the S3 signer is a typed capability mismatch.
///
/// # Panics
/// Panics when the environment does not select S3 or any replay returns anything else.
#[tokio::test]
async fn rustfs_capability_mismatch_is_typed_for_non_s3_protocols() {
    let signer = build_rustfs_signer().await;
    let path = fresh_path("capability/object.bin");
    for (wire_protocol, block_count_planned) in [
        (WireProtocol::GcsResumableV1, None),
        (WireProtocol::AzureBlockBlobV1, Some(PART_COUNT)),
    ] {
        let result = signer
            .remint_plan(
                &path,
                &UploadPlanReplayInput {
                    wire_protocol,
                    backend_upload_id: None,
                    part_count_planned: PART_COUNT,
                    part_size_bytes: PART_SIZE as u64,
                    block_count_planned,
                },
                Duration::from_mins(5),
            )
            .await;
        assert!(matches!(
            result,
            Err(StorageError::BackendCapabilityMismatch {
                signer: StorageBackendKind::S3,
                op: "remint_plan"
            })
        ));
    }
}
