//! Cross-backend `StorageHandle` data-plane CRUD suite.
//!
//! [`run_handle_crud`] is the single body exercised against every backend
//! through the public [`StorageHandle`] surface (`put_object` / `get_object` /
//! `list_objects` / `delete_object`). It is the server-side direct-CRUD path:
//! an agent request resolves a tenant from its JWT, a tenant-scoped
//! [`ValidatedPath`] is built, and the handle reads/writes/lists/deletes the
//! object itself (no client presign round-trip).
//!
//! Local runs over a temporary root. The S3, GCS, and Azure entrypoints boot
//! the handle from `settings::from_env()`, so their mise task decides whether
//! `WYRD_STORAGE_URL` (plus `WYRD_STORAGE_ENDPOINT_URL`) targets a docker
//! emulator or the real provider. Each asserts the configured backend kind, so
//! a mis-wired task fails instead of exercising another backend.

use std::sync::Arc;
use wyrd_spec::DataTenantId;
use wyrd_spec::storage::StorageBackendKind;
use wyrd_storage::error::StorageError;
use wyrd_storage::settings;
use wyrd_storage::{BackendSigner, LocalSigner, StorageHandle, ValidatedPath, tenant_path};

/// Write/read/list/delete round-trip plus a missing-key `NotFound`, all through
/// the public [`StorageHandle`] data-plane API.
///
/// Every object lives under a fresh tenant and Card prefix and is deleted before
/// returning, so shared cloud buckets stay clean on success.
///
/// # Panics
/// Panics when any put, get, list, or delete fails, the listing differs, or a
/// deleted or missing key is not `ObjectNotFound`. A panic or cancellation
/// mid-run can leave up to three small objects under that unique prefix.
async fn run_handle_crud(handle: &StorageHandle) {
    let tenant = DataTenantId::new_v7();
    let card = uuid::Uuid::now_v7().to_string();
    let path = |rel: &str| -> ValidatedPath {
        let full = tenant_path::build(tenant, &card, rel);
        tenant_path::validate(&full, tenant).expect("valid tenant path")
    };

    handle
        .put_object(&path("crud/a.bin"), b"aaa".to_vec())
        .await
        .expect("put a");
    handle
        .put_object(&path("crud/b.bin"), b"bbb".to_vec())
        .await
        .expect("put b");
    handle
        .put_object(&path("crud/c.bin"), b"ccc".to_vec())
        .await
        .expect("put c");

    let bytes = handle.get_object(&path("crud/a.bin")).await.expect("get a");
    assert_eq!(bytes, b"aaa", "read content mismatch");

    let listed = handle.list_objects(&path("crud")).await.expect("list crud");
    assert_eq!(
        listed.len(),
        3,
        "expected 3 objects under prefix, got {listed:?}"
    );
    let a_key = path("crud/a.bin").full.clone();
    assert!(
        listed.contains(&a_key),
        "listed paths must use full backend key format; got {listed:?}"
    );

    handle
        .delete_object(&path("crud/a.bin"))
        .await
        .expect("delete a");
    let err = handle
        .get_object(&path("crud/a.bin"))
        .await
        .expect_err("get after delete");
    assert!(
        matches!(err, StorageError::ObjectNotFound { .. }),
        "expected ObjectNotFound after delete, got {err:?}"
    );

    let err = handle
        .get_object(&path("crud/never-written.bin"))
        .await
        .expect_err("get missing");
    assert!(
        matches!(err, StorageError::ObjectNotFound { .. }),
        "expected ObjectNotFound for missing key, got {err:?}"
    );

    // Leave the bucket clean for cloud lanes.
    handle
        .delete_object(&path("crud/b.bin"))
        .await
        .expect("delete b");
    handle
        .delete_object(&path("crud/c.bin"))
        .await
        .expect("delete c");
}

/// Boot the handle described by the process storage environment.
///
/// # Panics
/// Panics when the environment is incomplete, selects a backend other than
/// `kind`, or the handle cannot be built.
async fn configured_handle(kind: StorageBackendKind) -> Arc<StorageHandle> {
    let settings = settings::from_env().expect("WYRD_STORAGE_URL configures storage");
    assert_eq!(
        settings.backend.kind(),
        kind,
        "WYRD_STORAGE_URL selects the wrong backend"
    );
    StorageHandle::from_settings(settings)
        .await
        .expect("storage handle")
}

/// CRUD over a local handle rooted in a temporary directory.
///
/// Credential-free; runs under `test:storage:handle:emulators`.
///
/// # Panics
/// Panics when the temporary root or signer cannot be built or CRUD fails.
#[tokio::test]
async fn local_handle_crud() {
    let dir = tempfile::tempdir().expect("tempdir");
    let signer =
        BackendSigner::Local(LocalSigner::new(dir.path().to_path_buf()).expect("local signer"));
    let handle = StorageHandle::new(signer);
    run_handle_crud(&handle).await;
}

/// CRUD against the configured S3 or S3-compatible bucket.
///
/// Selected by the owning mise task; `WYRD_STORAGE_URL` must select S3.
///
/// # Panics
/// Panics when the environment selects another backend or CRUD fails.
#[tokio::test]
async fn s3_handle_crud() {
    run_handle_crud(configured_handle(StorageBackendKind::S3).await.as_ref()).await;
}

/// CRUD against the configured GCS bucket or emulator.
///
/// Selected by the owning mise task; `WYRD_STORAGE_URL` must select GCS.
///
/// # Panics
/// Panics when the environment selects another backend or CRUD fails.
#[tokio::test]
async fn gcs_handle_crud() {
    run_handle_crud(configured_handle(StorageBackendKind::Gcs).await.as_ref()).await;
}

/// CRUD against the configured Azure container or emulator.
///
/// Selected by the owning mise task; `WYRD_STORAGE_URL` must select Azure.
///
/// # Panics
/// Panics when the environment selects another backend or CRUD fails.
#[tokio::test]
async fn azure_handle_crud() {
    run_handle_crud(configured_handle(StorageBackendKind::Azure).await.as_ref()).await;
}
