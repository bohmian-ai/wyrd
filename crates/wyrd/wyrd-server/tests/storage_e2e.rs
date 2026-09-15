//! Real client-to-server storage journeys.
//!
//! Every round trip uses a bound `WyrdTestServer`, `WyrdClient`, and
//! `WyrdStorageClient`. The registry/server path mints the plan, the storage
//! client performs all provider transfer details, and the same client streams
//! the bytes back with digest/size verification.

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use base64::Engine;
use sha2::{Digest, Sha256};
use tempfile::NamedTempFile;
use wyrd_client::WyrdClient;
use wyrd_client::auth::AuthMiddleware;
use wyrd_client::config::ClientConfig;
use wyrd_client::storage::WyrdStorageClient;
use wyrd_client::transport::HttpTransport;
use wyrd_client::transport::config::HttpConfig;
use wyrd_client::transport::credential::ResolvedCredential;
use wyrd_server::config::BifrostTarget;
use wyrd_spec::ids::CardUid;
use wyrd_spec::storage::{
    DownloadInitRequest, DownloadInitResponse, DownloadPlan, UploadInitRequest, UploadInitResponse,
    UploadPlan,
};
use wyrd_storage::cloud::CloudSigner;
use wyrd_storage::settings::{AzureConfig, GcsConfig, S3Config};
use wyrd_storage::{BackendConfig, BackendSigner, StorageHandle, StorageSettings};
use wyrd_testing::WyrdTestServer;

const FIXED_CARD_UID: &str = "018f0000-0000-7000-8000-000000000001";
const MULTIPART_PAYLOAD_BYTES: usize = 20 * 1024 * 1024;
const LOW_THRESHOLD_BYTES: u64 = 8 * 1024 * 1024;
const PLANNED_PART_BYTES: u64 = 16 * 1024 * 1024;

fn enabled(var: &str) -> bool {
    std::env::var(var).as_deref() == Ok("1")
}

fn sha256_b64(bytes: &[u8]) -> String {
    base64::engine::general_purpose::STANDARD.encode(Sha256::digest(bytes))
}

fn patterned_payload(len: usize) -> Vec<u8> {
    (0..len)
        .map(|index| u8::try_from(index % 251).expect("index modulo 251 fits u8"))
        .collect()
}

fn env_or(var: &str, default: &str) -> String {
    std::env::var(var).unwrap_or_else(|_| default.to_owned())
}

fn env_required(var: &str) -> String {
    std::env::var(var).unwrap_or_else(|_| panic!("{var} must be set for this cloud lane"))
}

fn client_for(srv: &WyrdTestServer, token: &str) -> WyrdClient {
    let base_url = srv
        .base_url()
        .expect("bound server exposes base URL")
        .to_owned();
    let config = ClientConfig {
        http: HttpConfig {
            base_url: base_url.clone(),
            ..HttpConfig::default()
        },
        ..ClientConfig::default()
    };
    let auth = AuthMiddleware::new(
        &config,
        ResolvedCredential::BearerToken(token.to_owned().into()),
    )
    .expect("client auth builds");
    let transport = HttpTransport::new(&config.http, Arc::clone(&auth)).expect("transport builds");
    WyrdClient::from_parts(auth, transport, config.grpc)
}

async fn bootstrap_service_jwt(srv: &WyrdTestServer, name: &str) -> String {
    let service = srv
        .bootstrap_service(name, &["writer", "reader"])
        .await
        .expect("bootstrap service");
    srv.exchange_api_key(service.api_key().expect("service API key"))
        .await
        .expect("exchange API key")
}

/// Starts a bound artifact-storage server over `settings`.
///
/// Storage journeys exercise artifact upload and download only, so the server
/// runs the API `Server` target without an embedded Forge worker: a worker
/// refuses staging backends lacking native `start_after` listing (Azure), which
/// is Forge behavior these journeys do not cover.
///
/// # Panics
///
/// Panics when the server fails to start or become ready.
async fn server_from_settings(settings: StorageSettings) -> WyrdTestServer {
    WyrdTestServer::builder()
        .with_bifrost_target_for_test(BifrostTarget::Server)
        .with_storage_settings(settings)
        .start_bound()
        .await
        .expect("start bound storage server")
}

fn cloud_settings(backend: BackendConfig) -> StorageSettings {
    StorageSettings {
        backend,
        require_encryption: false,
        presign_ttl: Duration::from_secs(600),
        part_size_bytes: PLANNED_PART_BYTES,
        multipart_threshold_bytes: LOW_THRESHOLD_BYTES,
        public_base_url: None,
    }
}

fn local_settings(root: &Path) -> StorageSettings {
    StorageSettings {
        backend: BackendConfig::Local {
            root: root.to_path_buf(),
        },
        require_encryption: false,
        presign_ttl: Duration::from_secs(600),
        part_size_bytes: PLANNED_PART_BYTES,
        multipart_threshold_bytes: LOW_THRESHOLD_BYTES,
        public_base_url: Some(String::new()),
    }
}

fn s3_emu_settings() -> StorageSettings {
    cloud_settings(BackendConfig::S3(S3Config {
        bucket: env_or("WYRD_STORAGE_S3_BUCKET", "wyrd-storage-test"),
        region: Some(env_or("WYRD_STORAGE_S3_REGION", "us-east-1")),
        endpoint_url: Some(env_or("WYRD_S3_EMULATOR_ENDPOINT", "http://localhost:9000")),
        force_path_style: true,
    }))
}

fn s3_cloud_settings() -> StorageSettings {
    cloud_settings(BackendConfig::S3(S3Config {
        bucket: env_required("WYRD_STORAGE_S3_BUCKET"),
        region: std::env::var("WYRD_STORAGE_S3_REGION").ok(),
        endpoint_url: std::env::var("WYRD_STORAGE_S3_ENDPOINT_URL").ok(),
        force_path_style: false,
    }))
}

fn gcs_emu_handle() -> Arc<StorageHandle> {
    let bucket = env_or("WYRD_STORAGE_GCS_BUCKET", "wyrd-storage-test");
    let endpoint = env_or("WYRD_GCS_EMULATOR_HOST", "http://localhost:4443");
    let signer = wyrd_storage::factory::gcs::build_emulator_signer(&bucket, &endpoint)
        .expect("GCS emulator signer");
    Arc::new(
        StorageHandle::for_testing_with_multipart_threshold(
            BackendSigner::Cloud(Box::new(CloudSigner::Gcs(signer))),
            BackendConfig::Gcs(GcsConfig {
                bucket,
                endpoint_url: Some(endpoint),
            }),
            LOW_THRESHOLD_BYTES,
        )
        .expect("GCS emulator storage handle"),
    )
}

fn gcs_cloud_settings() -> StorageSettings {
    cloud_settings(BackendConfig::Gcs(GcsConfig {
        bucket: env_required("WYRD_STORAGE_GCS_BUCKET"),
        endpoint_url: None,
    }))
}

fn azure_emu_handle() -> Arc<StorageHandle> {
    let container = env_or("WYRD_STORAGE_AZURE_CONTAINER", "wyrd-storage-test");
    let endpoint = env_or("WYRD_AZURE_EMULATOR_ENDPOINT", "http://127.0.0.1:10000");
    let signer = wyrd_storage::factory::azure::build_emulator_signer(&container, &endpoint)
        .expect("Azure emulator signer");
    Arc::new(
        StorageHandle::for_testing_with_multipart_threshold(
            BackendSigner::Cloud(Box::new(CloudSigner::Azure(signer))),
            BackendConfig::Azure(AzureConfig {
                account: "devstoreaccount1".to_owned(),
                container,
                endpoint_url: Some(endpoint),
            }),
            LOW_THRESHOLD_BYTES,
        )
        .expect("Azure emulator storage handle"),
    )
}

fn azure_cloud_settings() -> StorageSettings {
    cloud_settings(BackendConfig::Azure(AzureConfig {
        account: env_required("WYRD_STORAGE_AZURE_ACCOUNT"),
        container: env_required("WYRD_STORAGE_AZURE_CONTAINER"),
        endpoint_url: None,
    }))
}

/// Starts a bound artifact-storage server over a pre-built emulator `handle`.
///
/// Uses the API `Server` target for the same reason as
/// [`server_from_settings`]: storage journeys do not compose a Forge worker.
///
/// # Panics
///
/// Panics when the server fails to start or become ready.
async fn server_from_handle(handle: Arc<StorageHandle>) -> WyrdTestServer {
    WyrdTestServer::builder()
        .with_bifrost_target_for_test(BifrostTarget::Server)
        .with_storage_handle(handle)
        .start_bound()
        .await
        .expect("start bound storage server")
}

fn gcs_emulator_download_plan(storage_path: &str) -> DownloadPlan {
    let host = env_or("WYRD_GCS_EMULATOR_HOST", "http://localhost:4443");
    let bucket = env_or("WYRD_STORAGE_GCS_BUCKET", "wyrd-storage-test");
    DownloadPlan {
        get_url: format!(
            "{host}/storage/v1/b/{bucket}/o/{}?alt=media",
            storage_path.replace('/', "%2F")
        ),
        ttl_secs: 0,
    }
}

async fn run_client_server_journey(
    srv: WyrdTestServer,
    relative_path: &str,
    gcs_emulator_media: bool,
) {
    let token = bootstrap_service_jwt(&srv, "storage-client-journey").await;
    let client = client_for(&srv, &token);
    let storage = WyrdStorageClient::new(&client);
    let content_size = if relative_path.starts_with("local/") {
        512 * 1024
    } else {
        MULTIPART_PAYLOAD_BYTES
    };
    let content = patterned_payload(content_size);
    let card_uid = CardUid::new(FIXED_CARD_UID).expect("card UID");
    let idempotency_key = "storage-client-journey-001";
    let init: UploadInitResponse = client
        .submit_with_idempotency_key(
            reqwest::Method::POST,
            "/v1/cards/upload/init",
            &UploadInitRequest {
                card_uid: card_uid.clone(),
                relative_path: relative_path.to_owned(),
                expected_sha256: sha256_b64(&content),
                expected_size_bytes: content.len() as u64,
                content_type: None,
            },
            idempotency_key,
        )
        .await
        .expect("upload init succeeds");
    let upload_plan = init.plan.clone();
    storage
        .upload_artifact(
            &init.upload_id,
            &upload_plan,
            content.clone(),
            idempotency_key,
        )
        .await
        .expect("storage client uploads and completes artifact");

    let download: DownloadInitResponse = client
        .request_json(
            reqwest::Method::POST,
            "/v1/cards/download/init",
            Some(&DownloadInitRequest {
                card_uid,
                relative_path: relative_path.to_owned(),
                ttl_secs: None,
            }),
        )
        .await
        .expect("download init succeeds");
    let plan = if gcs_emulator_media {
        gcs_emulator_download_plan(&init.storage_path)
    } else if relative_path.starts_with("local/") {
        let base_url = srv.base_url().expect("bound server exposes base URL");
        DownloadPlan {
            get_url: format!("{base_url}/v1/cards/download/local/{}", init.storage_path),
            ttl_secs: 0,
        }
    } else {
        download.plan
    };
    let destination = NamedTempFile::new().expect("download destination");
    storage
        .download_verified(
            &plan,
            destination.path(),
            &sha256_b64(&content),
            content.len() as u64,
        )
        .await
        .expect("storage client downloads and verifies artifact");
    let downloaded = tokio::fs::read(destination.path())
        .await
        .expect("read downloaded artifact");
    assert_eq!(
        downloaded, content,
        "client/server round trip preserves bytes"
    );
    srv.shutdown().await.expect("shutdown server");
}

#[tokio::test(flavor = "current_thread")]
async fn local_client_server_round_trip() {
    if !enabled("WYRD_STORAGE_E2E") {
        return;
    }
    let storage_root = tempfile::tempdir().expect("storage root creates");
    let srv = server_from_settings(local_settings(storage_root.path())).await;
    run_client_server_journey(srv, "local/weights.bin", false).await;
}

/// A principal holding no Card permission is refused at both storage entry
/// routes, and each refusal is audited as its own denied decision.
///
/// Upload and download planning are receiving authorization boundaries, so the
/// verdict is recorded before anything about the Card or its objects is
/// disclosed. The journey drives both routes through the real client against a
/// bound server, requires the stable RBAC code on each, and reads staging to
/// prove exactly one `denied` row per route.
///
/// # Panics
/// Panics when the server or client cannot start, either route is not refused
/// with the RBAC code, or staging lacks exactly one `denied` row per route.
#[tokio::test(flavor = "current_thread")]
async fn storage_routes_refuse_and_audit_an_unprivileged_caller() {
    if !enabled("WYRD_STORAGE_E2E") {
        return;
    }
    // Both routes refuse before any storage IO, so the default bound server
    // suffices and no backend is configured.
    let srv = WyrdTestServer::start_bound()
        .await
        .expect("start bound server");
    let service = srv
        .bootstrap_service("storage-unprivileged", &[])
        .await
        .expect("bootstrap service without roles");
    let token = srv
        .exchange_api_key(service.api_key().expect("service API key"))
        .await
        .expect("exchange API key");
    let client = client_for(&srv, &token);
    let card_uid = CardUid::new(FIXED_CARD_UID).expect("card UID");

    let error = client
        .submit_with_idempotency_key::<_, UploadInitResponse>(
            reqwest::Method::POST,
            "/v1/cards/upload/init",
            &UploadInitRequest {
                card_uid: card_uid.clone(),
                relative_path: "local/denied.bin".to_owned(),
                expected_sha256: sha256_b64(b"denied"),
                expected_size_bytes: 6,
                content_type: None,
            },
            "storage-unprivileged-001",
        )
        .await
        .expect_err("a principal without card:write cannot initialize an upload");
    assert_eq!(error.code(), "WYRD_PERMISSION_403_DENIED_RBAC");

    let error = client
        .request_json::<_, DownloadInitResponse>(
            reqwest::Method::POST,
            "/v1/cards/download/init",
            Some(&DownloadInitRequest {
                card_uid: card_uid.clone(),
                relative_path: "local/denied.bin".to_owned(),
                ttl_secs: None,
            }),
        )
        .await
        .expect_err("a principal without card:read cannot plan a download");
    assert_eq!(error.code(), "WYRD_PERMISSION_403_DENIED_RBAC");

    let mut conn = srv
        .tenant_conn_for(srv.data_tenant_id())
        .await
        .expect("tenant connection opens");
    let denials: Vec<(String, String)> = sqlx::query_as(
        "SELECT operation, outcome FROM vala.audit_staging \
         WHERE operation IN ('storage.upload.init', 'storage.download.init') \
         ORDER BY operation",
    )
    .fetch_all(&mut **conn.transaction())
    .await
    .expect("storage decision rows read");
    conn.commit().await.expect("assertion transaction commits");
    assert_eq!(
        denials,
        vec![
            ("storage.download.init".to_owned(), "denied".to_owned()),
            ("storage.upload.init".to_owned(), "denied".to_owned()),
        ],
        "each refused storage route audits its own denial exactly once"
    );
    srv.shutdown().await.expect("test server shuts down");
}

#[tokio::test(flavor = "current_thread")]
async fn local_upload_capability_rejects_raw_paths_and_bad_bytes() {
    if !enabled("WYRD_STORAGE_E2E") {
        return;
    }
    let storage_root = tempfile::tempdir().expect("storage root creates");
    let srv = server_from_settings(local_settings(storage_root.path())).await;
    let token = bootstrap_service_jwt(&srv, "storage-local-negative").await;
    let client = client_for(&srv, &token);
    let content = b"good";
    let card_uid = CardUid::new(FIXED_CARD_UID).expect("card UID");
    let init: UploadInitResponse = client
        .submit_with_idempotency_key(
            reqwest::Method::POST,
            "/v1/cards/upload/init",
            &UploadInitRequest {
                card_uid: card_uid.clone(),
                relative_path: "local/negative.bin".to_owned(),
                expected_sha256: sha256_b64(content),
                expected_size_bytes: content.len() as u64,
                content_type: None,
            },
            "storage-local-negative-001",
        )
        .await
        .expect("upload init succeeds");
    let UploadPlan::LocalFs { put_url, .. } = &init.plan else {
        panic!("local storage returns a LocalFs capability");
    };

    let raw_path = format!("/v1/cards/upload/local/{}", init.storage_path);
    let error = client
        .request_stream(
            reqwest::Method::PUT,
            &raw_path,
            reqwest::Body::from(content.as_slice()),
        )
        .await
        .expect_err("raw storage paths are not upload capabilities");
    assert_eq!(
        error.code(),
        "WYRD_STORAGE_400_INVALID_UPLOAD_ID",
        "raw path rejection returned {error:?}"
    );

    let error = client
        .request_stream(reqwest::Method::PUT, put_url, reqwest::Body::from("bad"))
        .await
        .expect_err("wrong bytes are rejected before the local write");
    assert_eq!(error.code(), "WYRD_STORAGE_400_SIZE_MISMATCH");

    WyrdStorageClient::new(&client)
        .upload_artifact(
            &init.upload_id,
            &init.plan,
            content.to_vec(),
            "storage-local-negative-001",
        )
        .await
        .expect("the opaque capability still accepts the declared bytes");
    srv.shutdown().await.expect("test server shuts down");
}

#[tokio::test(flavor = "current_thread")]
async fn s3_multipart_e2e_emu() {
    if !enabled("WYRD_STORAGE_INTEGRATION_S3") {
        return;
    }
    run_client_server_journey(
        server_from_settings(s3_emu_settings()).await,
        "s3-multipart/weights.bin",
        false,
    )
    .await;
}

#[tokio::test(flavor = "current_thread")]
async fn s3_multipart_e2e_cloud() {
    if !enabled("WYRD_STORAGE_CLOUD_S3") {
        return;
    }
    run_client_server_journey(
        server_from_settings(s3_cloud_settings()).await,
        "s3-multipart/weights.bin",
        false,
    )
    .await;
}

#[tokio::test(flavor = "current_thread")]
async fn gcs_multipart_e2e_emu() {
    if !enabled("WYRD_STORAGE_INTEGRATION_GCS") {
        return;
    }
    run_client_server_journey(
        server_from_handle(gcs_emu_handle()).await,
        "gcs-multipart/weights.bin",
        true,
    )
    .await;
}

#[tokio::test(flavor = "current_thread")]
async fn gcs_multipart_e2e_cloud() {
    if !enabled("WYRD_STORAGE_CLOUD_GCS") {
        return;
    }
    run_client_server_journey(
        server_from_settings(gcs_cloud_settings()).await,
        "gcs-multipart/weights.bin",
        false,
    )
    .await;
}

#[tokio::test(flavor = "current_thread")]
async fn azure_multipart_e2e_emu() {
    if !enabled("WYRD_STORAGE_INTEGRATION_AZURE") {
        return;
    }
    run_client_server_journey(
        server_from_handle(azure_emu_handle()).await,
        "azure-multipart/weights.bin",
        false,
    )
    .await;
}

#[tokio::test(flavor = "current_thread")]
async fn azure_multipart_e2e_cloud() {
    if !enabled("WYRD_STORAGE_CLOUD_AZURE") {
        return;
    }
    run_client_server_journey(
        server_from_settings(azure_cloud_settings()).await,
        "azure-multipart/weights.bin",
        false,
    )
    .await;
}
