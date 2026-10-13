//! GCS signer contract tests.
//!
//! `test:storage:gcs-emu` (part of `test:storage:matrix`) runs the typed HEAD
//! test against the `fake-gcs-server` emulator. The `gcs_multipart_*` tests run
//! only in `test:storage:gcs:cloud` against real GCS, because the emulator does
//! not implement GCS XML multipart uploads. The client-to-server transfer
//! journey owns multipart byte and download coverage; these tests retain GCS
//! planning, cleanup, head, and part signing.

use std::time::Duration;

use wyrd_spec::DataTenantId;
use wyrd_spec::storage::{StorageBackendKind, UploadPlan, WireProtocol};
use wyrd_storage::error::{GcsError, StorageError};
use wyrd_storage::settings;
use wyrd_storage::{BackendSigner, UploadPlanReplayInput, ValidatedPath, factory};

/// Smallest multipart part size.
const PART_SIZE: u64 = 5 * 1024 * 1024;
/// Planned part count for multipart init and replay.
const PART_COUNT: u32 = 3;

/// Build the signer for the GCS location and endpoint in `WYRD_STORAGE_URL`.
///
/// # Panics
/// Panics when the environment does not select GCS or signer construction fails.
async fn build_signer() -> BackendSigner {
    let backend = settings::from_env().expect("storage settings").backend;
    assert_eq!(
        backend.kind(),
        StorageBackendKind::Gcs,
        "WYRD_STORAGE_URL must select gs://"
    );
    factory::build_signer(&backend).await.expect("GCS signer")
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

/// Aborting an initialized GCS multipart upload succeeds and leaves no object.
///
/// # Panics
/// Panics when the environment does not select GCS, init or abort fails, or
/// the object exists afterwards.
#[tokio::test]
async fn gcs_multipart_abort_lifecycle() {
    let signer = build_signer().await;
    let path = fresh_path("abort/object.bin");
    let init = signer
        .init_multipart(&path, PART_COUNT, PART_SIZE, Duration::from_mins(5))
        .await
        .expect("init multipart");
    assert!(
        !init.backend_upload_id.is_empty(),
        "GCS returns an upload id"
    );
    signer
        .abort_multipart(&path, &init.backend_upload_id)
        .await
        .expect("abort multipart");
    assert!(signer.head_for_verification(&path).await.is_err());
}

/// Replaying a stored GCS multipart plan yields the same part geometry, then
/// aborts the upload.
///
/// # Panics
/// Panics when the environment does not select GCS, init, remint, or cleanup
/// abort fails, or the plan differs.
#[tokio::test]
async fn gcs_multipart_remint_plan_produces_valid_plan() {
    let signer = build_signer().await;
    let path = fresh_path("remint/object.bin");
    let init = signer
        .init_multipart(&path, PART_COUNT, PART_SIZE, Duration::from_mins(5))
        .await
        .expect("init multipart");
    let plan = signer
        .remint_plan(
            &path,
            &UploadPlanReplayInput {
                wire_protocol: WireProtocol::MultipartV1,
                backend_upload_id: Some(init.backend_upload_id.clone()),
                part_count_planned: PART_COUNT,
                part_size_bytes: PART_SIZE,
                block_count_planned: None,
            },
            Duration::from_mins(5),
        )
        .await
        .expect("remint GCS plan");
    assert!(
        matches!(plan, UploadPlan::Multipart { part_count, part_size_bytes, .. } if part_count == PART_COUNT && part_size_bytes == PART_SIZE)
    );
    signer
        .abort_multipart(&path, &init.backend_upload_id)
        .await
        .expect("cleanup abort");
}

/// `head` on a never-written object returns typed GCS `NotFound`.
///
/// # Panics
/// Panics when the environment does not select GCS or the error is untyped.
#[tokio::test]
async fn gcs_head_on_missing_returns_not_found() {
    let result = build_signer()
        .await
        .head_for_verification(&fresh_path("never-written.bin"))
        .await;
    match result {
        Err(StorageError::Gcs(boxed)) => assert!(matches!(*boxed, GcsError::NotFound { .. })),
        other => panic!("expected typed GCS not-found error, got: {other:?}"),
    }
}

/// A GCS part URL addresses the XML multipart upload by part number and
/// upload id.
///
/// # Panics
/// Panics when the environment does not select GCS, init, signing, or cleanup
/// abort fails, or the URL does not name the part and upload.
#[tokio::test]
async fn gcs_multipart_presign_part_addresses_the_upload_part() {
    let signer = build_signer().await;
    let path = fresh_path("part/object.bin");
    let init = signer
        .init_multipart(&path, PART_COUNT, PART_SIZE, Duration::from_mins(5))
        .await
        .expect("init multipart");
    let url = url::Url::parse(
        &signer
            .presign_part(&path, &init.backend_upload_id, 1, Duration::from_mins(5))
            .await
            .expect("presign part"),
    )
    .expect("part url parses");
    let query: Vec<(String, String)> = url.query_pairs().into_owned().collect();
    assert!(query.contains(&("partNumber".to_owned(), "1".to_owned())));
    assert!(query.contains(&("uploadId".to_owned(), init.backend_upload_id.clone())));
    signer
        .abort_multipart(&path, &init.backend_upload_id)
        .await
        .expect("cleanup abort");
}
