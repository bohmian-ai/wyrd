//! Cross-backend `StorageHandle` data-plane CRUD suite.
//!
//! [`run_handle_crud`] is the single body exercised against every backend
//! through the public [`StorageHandle`] surface (`put_object` / `get_object` /
//! `list_objects` / `delete_object`). It is the server-side direct-CRUD path:
//! an agent request resolves a tenant from its JWT, a tenant-scoped
//! [`ValidatedPath`] is built, and the handle reads/writes/lists/deletes the
//! object itself (no client presign round-trip).
//!
//! Local always runs. S3/GCS/Azure each have two entrypoints that differ only
//! in how the handle is built:
//!
//! * `*_emu` — gated on `WYRD_STORAGE_INTEGRATION_{S3,GCS,AZURE}=1`, targets the
//!   local docker emulator (`RustFS` / fake-gcs / Azurite). Runs on any CI.
//! * `*_cloud` — gated on `WYRD_STORAGE_CLOUD_{S3,GCS,AZURE}=1`, boots the real
//!   handle via `from_settings` against the production backend. Runs only on
//!   merge-to-main CI.
//!
//! The assertions are identical across every backend and lane.

use std::sync::Arc;
use std::time::Duration;
use wyrd_spec::DataTenantId;
use wyrd_storage::cloud::CloudSigner;
use wyrd_storage::error::StorageError;
use wyrd_storage::factory::{azure, gcs, s3};
use wyrd_storage::settings::{AzureConfig, BackendConfig, GcsConfig, S3Config, StorageSettings};
use wyrd_storage::{BackendSigner, LocalSigner, StorageHandle, ValidatedPath, tenant_path};

fn enabled(var: &str) -> bool {
    std::env::var(var).as_deref() == Ok("1")
}

fn env_or(var: &str, default: &str) -> String {
    std::env::var(var).unwrap_or_else(|_| default.to_owned())
}

fn env_required(var: &str) -> String {
    std::env::var(var).unwrap_or_else(|_| panic!("{var} must be set"))
}

/// Write/read/list/delete round-trip plus a missing-key `NotFound`, all through
/// the public [`StorageHandle`] data-plane API.
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

// ---- Handle builders -------------------------------------------------------

fn s3_emu_handle() -> StorageHandle {
    let bucket = env_or("WYRD_STORAGE_S3_BUCKET", "wyrd-storage-test");
    let endpoint = env_or("WYRD_S3_EMULATOR_ENDPOINT", "http://localhost:9000");
    let signer = BackendSigner::Cloud(Box::new(CloudSigner::S3(
        s3::build_emulator_signer(&bucket, &endpoint).expect("s3 emu signer"),
    )));
    let config = BackendConfig::S3(S3Config {
        bucket,
        region: Some(env_or("WYRD_STORAGE_S3_REGION", "us-east-1")),
        endpoint_url: Some(endpoint),
        force_path_style: true,
    });
    StorageHandle::for_testing(signer, config).expect("s3 emu handle")
}

fn gcs_emu_handle() -> StorageHandle {
    let bucket = env_or("WYRD_STORAGE_GCS_BUCKET", "wyrd-storage-test");
    let host = env_or("WYRD_GCS_EMULATOR_HOST", "http://localhost:4443");
    let signer = BackendSigner::Cloud(Box::new(CloudSigner::Gcs(
        gcs::build_emulator_signer(&bucket, &host).expect("gcs emu signer"),
    )));
    let config = BackendConfig::Gcs(GcsConfig {
        bucket,
        endpoint_url: Some(host),
    });
    StorageHandle::for_testing(signer, config).expect("gcs emu handle")
}

fn azure_emu_handle() -> StorageHandle {
    let container = env_or("WYRD_STORAGE_AZURE_CONTAINER", "wyrd-storage-test");
    let base = env_or("WYRD_AZURE_EMULATOR_ENDPOINT", "http://127.0.0.1:10000");
    let signer = BackendSigner::Cloud(Box::new(CloudSigner::Azure(
        azure::build_emulator_signer(&container, &base).expect("azure emu signer"),
    )));
    let config = BackendConfig::Azure(AzureConfig {
        account: "devstoreaccount1".to_owned(),
        container,
        // Azurite path-style addressing requires the account name in the endpoint URL.
        endpoint_url: Some(format!("{base}/devstoreaccount1")),
    });
    StorageHandle::for_testing(signer, config).expect("azure emu handle")
}

fn cloud_settings(backend: BackendConfig) -> StorageSettings {
    StorageSettings {
        backend,
        require_encryption: false,
        presign_ttl: Duration::from_mins(15),
        part_size_bytes: 16 * 1024 * 1024,
        multipart_threshold_bytes: 100 * 1024 * 1024,
        public_base_url: None,
    }
}

async fn s3_cloud_handle() -> Arc<StorageHandle> {
    let backend = BackendConfig::S3(S3Config {
        bucket: env_required("WYRD_STORAGE_S3_BUCKET"),
        region: std::env::var("WYRD_STORAGE_S3_REGION").ok(),
        endpoint_url: std::env::var("WYRD_STORAGE_S3_ENDPOINT_URL").ok(),
        force_path_style: false,
    });
    StorageHandle::from_settings(cloud_settings(backend))
        .await
        .expect("s3 cloud handle")
}

async fn gcs_cloud_handle() -> Arc<StorageHandle> {
    let backend = BackendConfig::Gcs(GcsConfig {
        bucket: env_required("WYRD_STORAGE_GCS_BUCKET"),
        endpoint_url: None,
    });
    StorageHandle::from_settings(cloud_settings(backend))
        .await
        .expect("gcs cloud handle")
}

async fn azure_cloud_handle() -> Arc<StorageHandle> {
    let backend = BackendConfig::Azure(AzureConfig {
        account: env_required("WYRD_STORAGE_AZURE_ACCOUNT"),
        container: env_required("WYRD_STORAGE_AZURE_CONTAINER"),
        endpoint_url: None,
    });
    StorageHandle::from_settings(cloud_settings(backend))
        .await
        .expect("azure cloud handle")
}

// ---- Entrypoints -----------------------------------------------------------

#[tokio::test]
async fn local_handle_crud() {
    let dir = tempfile::tempdir().expect("tempdir");
    let signer =
        BackendSigner::Local(LocalSigner::new(dir.path().to_path_buf()).expect("local signer"));
    let handle = StorageHandle::new(signer);
    run_handle_crud(&handle).await;
}

#[tokio::test]
async fn s3_emu_handle_crud() {
    if !enabled("WYRD_STORAGE_INTEGRATION_S3") {
        eprintln!("skipping S3 handle emulator test; set WYRD_STORAGE_INTEGRATION_S3=1");
        return;
    }
    run_handle_crud(&s3_emu_handle()).await;
}

#[tokio::test]
async fn s3_cloud_handle_crud() {
    if !enabled("WYRD_STORAGE_CLOUD_S3") {
        eprintln!("skipping S3 handle cloud test; set WYRD_STORAGE_CLOUD_S3=1");
        return;
    }
    run_handle_crud(s3_cloud_handle().await.as_ref()).await;
}

#[tokio::test]
async fn gcs_emu_handle_crud() {
    if !enabled("WYRD_STORAGE_INTEGRATION_GCS") {
        eprintln!("skipping GCS handle emulator test; set WYRD_STORAGE_INTEGRATION_GCS=1");
        return;
    }
    run_handle_crud(&gcs_emu_handle()).await;
}

#[tokio::test]
async fn gcs_cloud_handle_crud() {
    if !enabled("WYRD_STORAGE_CLOUD_GCS") {
        eprintln!("skipping GCS handle cloud test; set WYRD_STORAGE_CLOUD_GCS=1");
        return;
    }
    run_handle_crud(gcs_cloud_handle().await.as_ref()).await;
}

#[tokio::test]
async fn azure_emu_handle_crud() {
    if !enabled("WYRD_STORAGE_INTEGRATION_AZURE") {
        eprintln!("skipping Azure handle emulator test; set WYRD_STORAGE_INTEGRATION_AZURE=1");
        return;
    }
    run_handle_crud(&azure_emu_handle()).await;
}

#[tokio::test]
async fn azure_cloud_handle_crud() {
    if !enabled("WYRD_STORAGE_CLOUD_AZURE") {
        eprintln!("skipping Azure handle cloud test; set WYRD_STORAGE_CLOUD_AZURE=1");
        return;
    }
    run_handle_crud(azure_cloud_handle().await.as_ref()).await;
}
