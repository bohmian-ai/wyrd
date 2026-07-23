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
use wyrd_storage::{UploadPlanReplayInput, ValidatedPath};

const PART_SIZE: usize = 5 * 1024 * 1024;
const PART_COUNT: u32 = 3;

fn skip_unless_enabled() -> bool {
    if std::env::var("WYRD_STORAGE_INTEGRATION_S3").as_deref() != Ok("1") {
        eprintln!("skipping RustFS integration test; set WYRD_STORAGE_INTEGRATION_S3=1");
        return true;
    }
    false
}

async fn build_rustfs_signer() -> S3Signer {
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

fn fresh_path(suffix: &str) -> ValidatedPath {
    let tenant = DataTenantId::new_v7();
    let card_uid = uuid::Uuid::now_v7();
    let full = wyrd_storage::tenant_path::build(tenant, &card_uid.to_string(), suffix);
    wyrd_storage::tenant_path::validate(&full, tenant).expect("tenant path")
}

#[tokio::test]
async fn rustfs_abort_lifecycle() {
    if skip_unless_enabled() {
        return;
    }
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
    if skip_unless_enabled() {
        return;
    }
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
    if skip_unless_enabled() {
        return;
    }
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
    if skip_unless_enabled() {
        return;
    }
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
