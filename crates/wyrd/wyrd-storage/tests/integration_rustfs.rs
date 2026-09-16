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

const PART_SIZE: usize = 5 * 1024 * 1024;
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

fn fresh_path(suffix: &str) -> ValidatedPath {
    let tenant = DataTenantId::new_v7();
    let card_uid = uuid::Uuid::now_v7();
    let full = wyrd_storage::tenant_path::build(tenant, &card_uid.to_string(), suffix);
    wyrd_storage::tenant_path::validate(&full, tenant).expect("tenant path")
}

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
