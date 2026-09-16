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
use wyrd_spec::DataTenantId;
use wyrd_spec::ids::CardUid;
use wyrd_spec::storage::{
    DownloadInitRequest, DownloadInitResponse, DownloadPlan, StorageBackendKind, UploadInitRequest,
    UploadInitResponse, UploadPlan,
};
use wyrd_storage::{BackendConfig, StorageHandle, StorageSettings, settings};
use wyrd_testing::WyrdTestServer;

/// Card identity every journey plans against; storage paths are scoped by tenant, so reuse is safe.
const FIXED_CARD_UID: &str = "018f0000-0000-7000-8000-000000000001";
/// Cloud payload size: above [`LOW_THRESHOLD_BYTES`], so cloud plans are multipart.
const MULTIPART_PAYLOAD_BYTES: usize = 20 * 1024 * 1024;
/// Multipart threshold lowered so a 20 MiB payload exercises multipart planning.
const LOW_THRESHOLD_BYTES: u64 = 8 * 1024 * 1024;
/// Planned part size, yielding two parts for [`MULTIPART_PAYLOAD_BYTES`].
const PLANNED_PART_BYTES: u64 = 16 * 1024 * 1024;

/// Base64 SHA-256 digest in the form upload init and download verification expect.
fn sha256_b64(bytes: &[u8]) -> String {
    base64::engine::general_purpose::STANDARD.encode(Sha256::digest(bytes))
}

/// Deterministic non-constant payload, so a misordered or truncated part changes the digest.
///
/// # Panics
/// Never in practice: `index % 251` always fits `u8`.
fn patterned_payload(len: usize) -> Vec<u8> {
    (0..len)
        .map(|index| u8::try_from(index % 251).expect("index modulo 251 fits u8"))
        .collect()
}

/// Builds a real `WyrdClient` bound to `srv` that authenticates with `token`.
///
/// # Panics
/// Panics when the server is not bound or client auth/transport construction fails.
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

/// Bootstraps a `writer`/`reader` service and exchanges its API key for a JWT.
///
/// # Panics
/// Panics when bootstrap or key exchange fails.
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

/// Local-filesystem storage settings rooted at `root`, using the journey's multipart tuning.
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

/// Load storage settings from the process environment for a cloud journey.
///
/// The mise task supplies `WYRD_STORAGE_URL`, an optional endpoint, and the
/// multipart tuning, so one journey targets either an emulator or the real
/// provider.
///
/// # Panics
/// Panics when the environment is incomplete or selects a backend other than
/// `kind`.
fn configured_settings(kind: StorageBackendKind) -> StorageSettings {
    let settings = settings::from_env().expect("WYRD_STORAGE_URL configures storage");
    assert_eq!(
        settings.backend.kind(),
        kind,
        "WYRD_STORAGE_URL selects the wrong backend"
    );
    settings
}

/// Delete every object a journey against `settings` left in the backend.
///
/// The journey writes one artifact under its own tenant prefix, and the running
/// server publishes audit-log Iceberg metadata under `tenants/<tenant>/` for
/// both the journey tenant and the system owner. Cloud lanes share one durable
/// bucket across runs, so each run removes exactly the prefixes it can produce
/// instead of accumulating 20 MiB artifacts and metadata forever.
///
/// Objects are listed and deleted one at a time rather than through opendal's
/// recursive delete, because `fake-gcs-server` refuses that backend's batch
/// delete request with HTTP 400 while real GCS accepts it.
///
/// # Panics
/// Panics when the cleanup handle cannot be built, a prefix cannot be listed,
/// or an object cannot be deleted, so a lane that silently stops cleaning fails
/// loudly.
async fn purge_journey_objects(settings: &StorageSettings, tenant: DataTenantId) {
    let handle = StorageHandle::from_settings(settings.clone())
        .await
        .expect("cleanup handle builds from the journey settings");
    let operator = handle.operator();
    for prefix in [
        format!("{tenant}/"),
        format!("tenants/{tenant}/"),
        format!("tenants/{}/", DataTenantId::SYSTEM_OWNER),
    ] {
        let entries = operator
            .list_with(&prefix)
            .recursive(true)
            .await
            .expect("journey prefix lists");
        for entry in entries {
            if entry.metadata().is_dir() {
                continue;
            }
            operator
                .delete(entry.path())
                .await
                .expect("journey object is deleted");
        }
    }
}

/// Drives one complete upload → download round trip through the real client and server.
///
/// Starts a bound server over `settings`, initializes an upload for
/// `relative_path` (512 KiB for `local/` paths, otherwise a multipart payload),
/// uploads and completes it through `WyrdStorageClient`, plans a download, and
/// streams it back with digest and size verification. The server is shut down
/// and every object the run could produce is purged *before* the download
/// outcome is asserted, so a failed transfer still cleans shared cloud buckets.
///
/// # Panics
/// Panics when server start, bootstrap, upload init/transfer/complete, download
/// init, shutdown, or cleanup fails, or when the verified bytes differ.
///
/// Cancellation or a panic before cleanup can leave the uploaded artifact and
/// audit metadata under this run's tenant prefixes; the next run's purge only
/// covers its own tenant, so such residue must be removed manually.
async fn run_client_server_journey(settings: StorageSettings, relative_path: &str) {
    let srv = server_from_settings(settings.clone()).await;
    let tenant = srv.data_tenant_id();
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
    let plan = if relative_path.starts_with("local/") {
        let base_url = srv.base_url().expect("bound server exposes base URL");
        DownloadPlan {
            get_url: format!("{base_url}/v1/cards/download/local/{}", init.storage_path),
            ttl_secs: 0,
        }
    } else {
        download.plan
    };
    // The download outcome is held rather than asserted so a transfer failure
    // still reaches the cleanup below; a panic here would strand the uploaded
    // artifact in a durable cloud bucket.
    let destination = NamedTempFile::new().expect("download destination");
    let outcome = storage
        .download_verified(
            &plan,
            destination.path(),
            &sha256_b64(&content),
            content.len() as u64,
        )
        .await;
    let downloaded = tokio::fs::read(destination.path()).await;
    srv.shutdown().await.expect("shutdown server");
    purge_journey_objects(&settings, tenant).await;

    outcome.expect("storage client downloads and verifies artifact");
    assert_eq!(
        downloaded.expect("read downloaded artifact"),
        content,
        "client/server round trip preserves bytes"
    );
}

/// Local-backend client/server round trip; always runs in the workspace lane.
///
/// # Panics
/// Panics when the temporary root cannot be created or the journey fails.
#[tokio::test(flavor = "current_thread")]
async fn local_client_server_round_trip() {
    let storage_root = tempfile::tempdir().expect("storage root creates");
    run_client_server_journey(local_settings(storage_root.path()), "local/weights.bin").await;
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

/// A local upload capability refuses raw storage paths and mismatched bytes, then accepts the declared bytes.
///
/// # Panics
/// Panics when setup fails, the plan is not `LocalFs`, either negative request is
/// accepted or refused with the wrong code, or the valid upload fails.
#[tokio::test(flavor = "current_thread")]
async fn local_upload_capability_rejects_raw_paths_and_bad_bytes() {
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

/// Multipart client/server round trip against the configured S3 or S3-compatible bucket.
///
/// Selected by the owning mise task; `WYRD_STORAGE_URL` must select S3.
///
/// # Panics
/// Panics when the environment selects another backend or the journey fails.
#[tokio::test(flavor = "current_thread")]
async fn s3_multipart_e2e() {
    run_client_server_journey(
        configured_settings(StorageBackendKind::S3),
        "s3-multipart/weights.bin",
    )
    .await;
}

/// Multipart client/server round trip against the configured GCS bucket or emulator.
///
/// Selected by the owning mise task; `WYRD_STORAGE_URL` must select GCS.
///
/// # Panics
/// Panics when the environment selects another backend or the journey fails.
#[tokio::test(flavor = "current_thread")]
async fn gcs_multipart_e2e() {
    run_client_server_journey(
        configured_settings(StorageBackendKind::Gcs),
        "gcs-multipart/weights.bin",
    )
    .await;
}

/// Multipart client/server round trip against the configured Azure container or emulator.
///
/// Selected by the owning mise task; `WYRD_STORAGE_URL` must select Azure.
///
/// # Panics
/// Panics when the environment selects another backend or the journey fails.
#[tokio::test(flavor = "current_thread")]
async fn azure_multipart_e2e() {
    run_client_server_journey(
        configured_settings(StorageBackendKind::Azure),
        "azure-multipart/weights.bin",
    )
    .await;
}
