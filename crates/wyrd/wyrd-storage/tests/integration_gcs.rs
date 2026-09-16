//! GCS signer-layer integration tests against `fake-gcs-server`.
//!
//! The client-to-server transfer journey owns resumable byte and download
//! coverage. These tests retain GCS-specific planning and capability checks.

use std::time::Duration;

use wyrd_spec::DataTenantId;
use wyrd_spec::storage::{StorageBackendKind, UploadPlan, WireProtocol};
use wyrd_storage::cloud::CloudSigner;
use wyrd_storage::error::{GcsError, StorageError};
use wyrd_storage::gcs::GcsSigner;
use wyrd_storage::settings::{self, BackendConfig};
use wyrd_storage::{BackendSigner, UploadPlanReplayInput, ValidatedPath, factory};

const CHUNK_SIZE: u64 = 256 * 1024;
const CHUNK_COUNT: u32 = 3;

/// Build the GCS signer for the location and endpoint in `WYRD_STORAGE_URL`.
///
/// # Panics
/// Panics when the environment does not select GCS or signer construction fails.
async fn build_signer() -> GcsSigner {
    let BackendConfig::Gcs(config) = settings::from_env().expect("storage settings").backend else {
        panic!("WYRD_STORAGE_URL must select gs://");
    };
    factory::gcs::build_signer(&config)
        .await
        .expect("GCS signer")
}

fn fresh_path(suffix: &str) -> ValidatedPath {
    let tenant = DataTenantId::new_v7();
    let card_uid = uuid::Uuid::now_v7();
    let full = wyrd_storage::tenant_path::build(tenant, &card_uid.to_string(), suffix);
    wyrd_storage::tenant_path::validate(&full, tenant).expect("tenant path")
}

#[tokio::test]
async fn gcs_abort_returns_capability_mismatch() {
    let backend = BackendSigner::Cloud(Box::new(CloudSigner::Gcs(build_signer().await)));
    let result = backend
        .abort_multipart(&fresh_path("abort/object.bin"), "ignored")
        .await;
    assert!(matches!(
        result,
        Err(StorageError::BackendCapabilityMismatch {
            signer: StorageBackendKind::Gcs,
            op: "abort_multipart"
        })
    ));
}

#[tokio::test]
async fn gcs_remint_plan_produces_valid_plan() {
    let signer = build_signer().await;
    let path = fresh_path("remint/object.bin");
    let plan = signer
        .remint_plan(
            &path,
            &UploadPlanReplayInput {
                wire_protocol: WireProtocol::GcsResumableV1,
                backend_upload_id: None,
                part_count_planned: CHUNK_COUNT,
                part_size_bytes: CHUNK_SIZE,
                block_count_planned: None,
            },
            Duration::from_mins(5),
        )
        .await
        .expect("remint GCS plan");
    assert!(
        matches!(plan, UploadPlan::GcsResumable { chunk_size_bytes, .. } if chunk_size_bytes == CHUNK_SIZE)
    );
}

#[tokio::test]
async fn gcs_head_on_missing_returns_not_found() {
    let result = build_signer()
        .await
        .head(&fresh_path("never-written.bin"))
        .await;
    match result {
        Err(StorageError::Gcs(boxed)) => assert!(matches!(*boxed, GcsError::NotFound { .. })),
        other => panic!("expected typed GCS not-found error, got: {other:?}"),
    }
}

#[tokio::test]
async fn gcs_capability_mismatch_is_typed_for_presign_part() {
    let backend = BackendSigner::Cloud(Box::new(CloudSigner::Gcs(build_signer().await)));
    let path = fresh_path("capability/object.bin");
    let result = backend
        .presign_part(&path, "ignored", 1, Duration::from_secs(1))
        .await;
    assert!(matches!(
        result,
        Err(StorageError::BackendCapabilityMismatch {
            signer: StorageBackendKind::Gcs,
            op: "presign_part"
        })
    ));
}
