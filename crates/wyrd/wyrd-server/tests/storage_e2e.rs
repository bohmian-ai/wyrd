//! Storage HTTP e2e integration tests.
//!
//! Drives the full request chain an SDK client would walk: upload-init →
//! byte PUT → complete → download-init → byte GET → compare. The local backend
//! round trip runs in the normal server gate; emulator/cloud cases are env-gated:
//!
//! ```
//! WYRD_STORAGE_E2E=1 \
//! WYRD_DATABASE_URL=postgres://wyrd_app:<pw>@localhost/wyrd \
//! WYRD_DATABASE_MIGRATOR_PASSWORD=<migrator_pw> \
//! cargo test -p wyrd-server --all-features --test storage_e2e -- --nocapture
//! ```
//!
//! Requires a running Postgres with applied migrations (see `mise run
//! storage:up` + `mise run check:db` for local setup).

use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode, header};
use base64::Engine;
use sha2::{Digest, Sha256};
use std::sync::Arc;
use std::time::Duration;
use wyrd_spec::ids::CardUid;
use wyrd_spec::storage::{
    AbortResponse, AzureBlockBlobComplete, DownloadInitRequest, DownloadInitResponse,
    GcsResumableComplete, PartUrlResponse, S3MultipartComplete, SinglePutComplete,
    UploadCompleteRequest, UploadInitRequest, UploadInitResponse, UploadPlan,
};
use wyrd_storage::cloud::CloudSigner;
use wyrd_storage::settings::{AzureConfig, GcsConfig, S3Config};
use wyrd_storage::{BackendConfig, BackendSigner, StorageHandle, StorageSettings};
use wyrd_testing::{MultipartClient, WyrdTestServer};

const FIXED_CARD_UID: &str = "018f0000-0000-7000-8000-000000000001";

fn skip_unless_e2e() -> bool {
    if std::env::var("WYRD_STORAGE_E2E").as_deref() != Ok("1") {
        eprintln!("skipping storage e2e tests; set WYRD_STORAGE_E2E=1");
        return true;
    }
    false
}

fn sha256_b64(bytes: &[u8]) -> String {
    base64::engine::general_purpose::STANDARD.encode(Sha256::digest(bytes))
}

fn request<B: Into<Body>>(
    method: &str,
    uri: &str,
    body: B,
    content_type: Option<&str>,
) -> Request<Body> {
    let mut builder = Request::builder().method(method).uri(uri);
    if let Some(ct) = content_type {
        builder = builder.header(header::CONTENT_TYPE, ct);
    }
    builder.body(body.into()).expect("request builds")
}

fn local_path(url: &str) -> &str {
    url.strip_prefix("https://wyrd.test").unwrap_or(url)
}

async fn bootstrap_service_jwt(srv: &WyrdTestServer, name: &str, roles: &[&str]) -> String {
    let service = srv
        .bootstrap_service(name, roles)
        .await
        .expect("bootstrap service");
    srv.exchange_api_key(
        service
            .api_key()
            .expect("service bootstrap returns api key"),
    )
    .await
    .expect("exchange api key")
}

#[tokio::test(flavor = "current_thread")]
async fn local_backend_upload_download_round_trip() {
    let srv = WyrdTestServer::start_in_process().await.expect("start env");
    let token = bootstrap_service_jwt(&srv, "storage-writer", &["writer"]).await;

    let content = b"wyrd storage e2e round-trip payload";
    let sha256 = sha256_b64(content);
    let card_uid = CardUid::new(FIXED_CARD_UID).expect("card uid");

    let init_body = serde_json::to_vec(&UploadInitRequest {
        card_uid: card_uid.clone(),
        relative_path: "model/weights.bin".to_owned(),
        expected_sha256: sha256.clone(),
        expected_size_bytes: content.len() as u64,
        content_type: None,
    })
    .expect("init body serializes");

    let init_response = srv
        .oneshot_authenticated(
            &token,
            request(
                "POST",
                "/v1/cards/upload/init",
                init_body,
                Some("application/json"),
            ),
        )
        .await
        .expect("router responds");

    assert_eq!(
        init_response.status(),
        StatusCode::OK,
        "upload/init must succeed"
    );
    let init_bytes = to_bytes(init_response.into_body(), usize::MAX)
        .await
        .expect("init body");
    let init: wyrd_spec::storage::UploadInitResponse =
        serde_json::from_slice(&init_bytes).expect("init response deserializes");

    let UploadPlan::LocalFs { put_url, .. } = &init.plan else {
        panic!(
            "expected LocalFs plan for local backend, got: {:?}",
            init.plan
        );
    };
    let upload_id = init.upload_id.clone();
    let put_path = local_path(put_url).to_owned();

    let put_response = srv
        .oneshot_authenticated(
            &token,
            request(
                "PUT",
                &put_path,
                content.to_vec(),
                Some("application/octet-stream"),
            ),
        )
        .await
        .expect("router responds");

    assert_eq!(
        put_response.status(),
        StatusCode::OK,
        "local blob PUT must succeed"
    );

    let complete_body = serde_json::to_vec(&UploadCompleteRequest::SinglePut(SinglePutComplete {}))
        .expect("complete body serializes");

    let complete_response = srv
        .oneshot_authenticated(
            &token,
            request(
                "POST",
                &format!("/v1/cards/upload/{upload_id}/complete"),
                complete_body,
                Some("application/json"),
            ),
        )
        .await
        .expect("router responds");

    assert_eq!(
        complete_response.status(),
        StatusCode::OK,
        "upload/complete must succeed"
    );

    let download_init_body = serde_json::to_vec(&DownloadInitRequest {
        card_uid: card_uid.clone(),
        relative_path: "model/weights.bin".to_owned(),
        ttl_secs: None,
    })
    .expect("download init body serializes");

    let dl_init_response = srv
        .oneshot_authenticated(
            &token,
            request(
                "POST",
                "/v1/cards/download/init",
                download_init_body,
                Some("application/json"),
            ),
        )
        .await
        .expect("router responds");

    assert_eq!(
        dl_init_response.status(),
        StatusCode::OK,
        "download/init must succeed"
    );
    let dl_init_bytes = to_bytes(dl_init_response.into_body(), usize::MAX)
        .await
        .expect("download init body");
    let dl_init: wyrd_spec::storage::DownloadInitResponse =
        serde_json::from_slice(&dl_init_bytes).expect("download init response deserializes");

    assert_eq!(dl_init.sha256, sha256, "returned sha256 must match upload");
    assert_eq!(
        dl_init.size_bytes,
        content.len() as u64,
        "returned size must match upload"
    );
    assert_eq!(
        dl_init.plan.ttl_secs, 0,
        "local backend ttl_secs sentinel is 0"
    );

    let get_path = local_path(&dl_init.plan.get_url).to_owned();
    let get_response = srv
        .oneshot_authenticated(&token, request("GET", &get_path, Body::empty(), None))
        .await
        .expect("router responds");

    assert_eq!(
        get_response.status(),
        StatusCode::OK,
        "local blob GET must succeed"
    );
    let downloaded = to_bytes(get_response.into_body(), usize::MAX)
        .await
        .expect("download body");
    assert_eq!(
        downloaded.as_ref(),
        content,
        "round-tripped bytes must match"
    );
    srv.shutdown().await.expect("shutdown");
}

#[tokio::test(flavor = "current_thread")]
async fn upload_without_permission_returns_403() {
    if skip_unless_e2e() {
        return;
    }
    let srv = WyrdTestServer::start_in_process().await.expect("start env");
    let token = bootstrap_service_jwt(&srv, "storage-no-permission", &[]).await;

    let card_uid = CardUid::new(FIXED_CARD_UID).expect("card uid");
    let content = b"scope check payload";

    let init_body = serde_json::to_vec(&UploadInitRequest {
        card_uid,
        relative_path: "scope-check.bin".to_owned(),
        expected_sha256: sha256_b64(content),
        expected_size_bytes: content.len() as u64,
        content_type: None,
    })
    .expect("body serializes");

    let response = srv
        .oneshot_authenticated(
            &token,
            request(
                "POST",
                "/v1/cards/upload/init",
                init_body,
                Some("application/json"),
            ),
        )
        .await
        .expect("router responds");

    assert_eq!(
        response.status(),
        StatusCode::FORBIDDEN,
        "upload without card:write permission must return 403"
    );
    srv.shutdown().await.expect("shutdown");
}

#[tokio::test(flavor = "current_thread")]
async fn abort_of_already_aborted_upload_returns_aborted_false() {
    if skip_unless_e2e() {
        return;
    }
    let srv = WyrdTestServer::start_in_process().await.expect("start env");
    let token = bootstrap_service_jwt(&srv, "storage-abort-writer", &["writer"]).await;

    let content = b"abort-race test payload";
    let card_uid = CardUid::new(FIXED_CARD_UID).expect("card uid");

    let init_body = serde_json::to_vec(&UploadInitRequest {
        card_uid,
        relative_path: "abort-race.bin".to_owned(),
        expected_sha256: sha256_b64(content),
        expected_size_bytes: content.len() as u64,
        content_type: None,
    })
    .expect("body serializes");

    let init_response = srv
        .oneshot_authenticated(
            &token,
            request(
                "POST",
                "/v1/cards/upload/init",
                init_body,
                Some("application/json"),
            ),
        )
        .await
        .expect("router responds");

    assert_eq!(init_response.status(), StatusCode::OK);
    let init_bytes = to_bytes(init_response.into_body(), usize::MAX)
        .await
        .unwrap();
    let init: wyrd_spec::storage::UploadInitResponse = serde_json::from_slice(&init_bytes).unwrap();
    let upload_id = init.upload_id;

    let first_abort = srv
        .oneshot_authenticated(
            &token,
            request(
                "POST",
                &format!("/v1/cards/upload/{upload_id}/abort"),
                Body::empty(),
                None,
            ),
        )
        .await
        .expect("router responds");

    assert_eq!(first_abort.status(), StatusCode::OK);
    let first_bytes = to_bytes(first_abort.into_body(), usize::MAX).await.unwrap();
    let first: AbortResponse = serde_json::from_slice(&first_bytes).unwrap();
    assert!(
        first.aborted,
        "first abort of initiating upload must succeed"
    );

    let second_abort = srv
        .oneshot_authenticated(
            &token,
            request(
                "POST",
                &format!("/v1/cards/upload/{upload_id}/abort"),
                Body::empty(),
                None,
            ),
        )
        .await
        .expect("router responds");

    assert_eq!(second_abort.status(), StatusCode::OK);
    let second_bytes = to_bytes(second_abort.into_body(), usize::MAX)
        .await
        .unwrap();
    let second: AbortResponse = serde_json::from_slice(&second_bytes).unwrap();
    assert!(
        !second.aborted,
        "aborting an already-aborted upload must return aborted: false"
    );
    srv.shutdown().await.expect("shutdown");
}

#[tokio::test(flavor = "current_thread")]
async fn reinit_to_same_path_after_completion_succeeds() {
    if skip_unless_e2e() {
        return;
    }
    let srv = WyrdTestServer::start_in_process().await.expect("start env");
    let token = bootstrap_service_jwt(&srv, "storage-reinit-writer", &["writer"]).await;

    let content = b"checkpoint payload v1";
    let card_uid = CardUid::new(FIXED_CARD_UID).expect("card uid");
    let relative_path = "model/checkpoint.bin".to_owned();

    let init_body = || {
        serde_json::to_vec(&UploadInitRequest {
            card_uid: card_uid.clone(),
            relative_path: relative_path.clone(),
            expected_sha256: sha256_b64(content),
            expected_size_bytes: content.len() as u64,
            content_type: None,
        })
        .expect("body serializes")
    };

    let first_init = srv
        .oneshot_authenticated(
            &token,
            request(
                "POST",
                "/v1/cards/upload/init",
                init_body(),
                Some("application/json"),
            ),
        )
        .await
        .expect("router responds");
    assert_eq!(first_init.status(), StatusCode::OK);
    let first_bytes = to_bytes(first_init.into_body(), usize::MAX).await.unwrap();
    let first: wyrd_spec::storage::UploadInitResponse =
        serde_json::from_slice(&first_bytes).unwrap();
    let first_upload_id = first.upload_id.clone();

    let UploadPlan::LocalFs { put_url, .. } = &first.plan else {
        panic!("expected LocalFs plan");
    };
    let put_path = local_path(put_url).to_owned();

    let put = srv
        .oneshot_authenticated(&token, request("PUT", &put_path, content.to_vec(), None))
        .await
        .expect("router responds");
    assert_eq!(put.status(), StatusCode::OK);

    let complete_body = serde_json::to_vec(&UploadCompleteRequest::SinglePut(SinglePutComplete {}))
        .expect("complete body");
    let complete = srv
        .oneshot_authenticated(
            &token,
            request(
                "POST",
                &format!("/v1/cards/upload/{first_upload_id}/complete"),
                complete_body,
                Some("application/json"),
            ),
        )
        .await
        .expect("router responds");
    assert_eq!(complete.status(), StatusCode::OK);

    let second_init = srv
        .oneshot_authenticated(
            &token,
            request(
                "POST",
                "/v1/cards/upload/init",
                init_body(),
                Some("application/json"),
            ),
        )
        .await
        .expect("router responds");
    assert_eq!(
        second_init.status(),
        StatusCode::OK,
        "re-init to a completed path must succeed, not 409"
    );
    let second_bytes = to_bytes(second_init.into_body(), usize::MAX).await.unwrap();
    let second: wyrd_spec::storage::UploadInitResponse =
        serde_json::from_slice(&second_bytes).unwrap();
    assert_ne!(
        second.upload_id, first_upload_id,
        "re-init must issue a new upload_id"
    );
    srv.shutdown().await.expect("shutdown");
}

// ============================================================================
// Parameterized cloud multipart e2e
//
// One harness, four backends, two lanes. The same upload→complete→download
// byte-equality flow runs for S3, GCS, and Azure against either the local
// docker emulator (any CI) or the real cloud (merge-to-main CI). The server
// planner forces a multipart upload because the payload (20 MiB) exceeds the
// per-backend `multipart_threshold_bytes` (lowered to 8 MiB), and the planner's
// fixed 16 MiB part size splits it into exactly two parts/chunks/blocks.
//
// `MultipartClient` is the single client-side upload driver shared with the
// signer-tier integration tests, so the two tiers cannot drift on how bytes
// reach the backend.
//
// GCS note: `fake-gcs-server` is anonymous and cannot mint signed download
// URLs, so the GCS emulator lane reads back through the emulator media API
// instead of the server's presigned `download/init`. The presign download path
// for GCS is covered by the cloud lane.
// ============================================================================

const MULTIPART_PAYLOAD_BYTES: usize = 20 * 1024 * 1024;
const LOW_THRESHOLD_BYTES: u64 = 8 * 1024 * 1024;
const PLANNED_PART_BYTES: u64 = 16 * 1024 * 1024;

fn enabled(var: &str) -> bool {
    std::env::var(var).as_deref() == Ok("1")
}

fn env_or(var: &str, default: &str) -> String {
    std::env::var(var).unwrap_or_else(|_| default.to_owned())
}

fn env_required(var: &str) -> String {
    std::env::var(var).unwrap_or_else(|_| panic!("{var} must be set for this cloud lane"))
}

fn patterned_payload(len: usize) -> Vec<u8> {
    (0..len)
        .map(|i| u8::try_from(i % 251).expect("mod 251 fits u8"))
        .collect()
}

fn split_chunks(content: &[u8], chunk: u64) -> Vec<Vec<u8>> {
    let chunk = usize::try_from(chunk).expect("chunk size fits usize");
    content.chunks(chunk).map(<[u8]>::to_vec).collect()
}

/// How the harness reads the object back after completion.
enum DownloadVia {
    /// Server `download/init` + presigned GET (works for every backend that can
    /// sign URLs: S3 emu/cloud, Azure emu/cloud, GCS cloud).
    ServerPresign,
    /// Direct GCS emulator media API GET (anonymous fake-gcs has no signing).
    GcsEmulatorMedia,
}

async fn post_json(
    srv: &WyrdTestServer,
    token: &str,
    uri: &str,
    body: Vec<u8>,
) -> axum::response::Response {
    srv.oneshot_authenticated(token, request("POST", uri, body, Some("application/json")))
        .await
        .expect("router responds")
}

async fn upload_init(
    srv: &WyrdTestServer,
    token: &str,
    card_uid: &CardUid,
    relative_path: &str,
    sha256: &str,
    size_bytes: u64,
) -> UploadInitResponse {
    let body = serde_json::to_vec(&UploadInitRequest {
        card_uid: card_uid.clone(),
        relative_path: relative_path.to_owned(),
        expected_sha256: sha256.to_owned(),
        expected_size_bytes: size_bytes,
        content_type: None,
    })
    .expect("init body serializes");
    let response = post_json(srv, token, "/v1/cards/upload/init", body).await;
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "upload/init must succeed"
    );
    let bytes = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("init body");
    serde_json::from_slice(&bytes).expect("init response deserializes")
}

async fn server_part_url(
    srv: &WyrdTestServer,
    token: &str,
    upload_id: &str,
    part_number: u32,
) -> String {
    let response = post_json(
        srv,
        token,
        &format!("/v1/cards/upload/{upload_id}/part-url?part_number={part_number}"),
        Vec::new(),
    )
    .await;
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "part-url for part {part_number} must succeed"
    );
    let bytes = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("part-url body");
    let parsed: PartUrlResponse =
        serde_json::from_slice(&bytes).expect("PartUrlResponse deserializes");
    parsed.url
}

/// Drive the client-side byte upload for the planned protocol and return the
/// matching completion request.
async fn drive_upload(
    srv: &WyrdTestServer,
    token: &str,
    mp: &MultipartClient,
    plan: &UploadPlan,
    upload_id: &str,
    content: &[u8],
) -> UploadCompleteRequest {
    match plan {
        UploadPlan::S3Multipart {
            part_count,
            part_size_bytes,
            ..
        } => {
            let parts = split_chunks(content, *part_size_bytes);
            assert_eq!(
                parts.len(),
                *part_count as usize,
                "client splits into the planned part count"
            );
            let mut urls = Vec::with_capacity(parts.len());
            for part_number in 1..=*part_count {
                urls.push(server_part_url(srv, token, upload_id, part_number).await);
            }
            let completed = mp
                .s3_multipart(
                    plan,
                    |part_number: u32| {
                        let url = urls[(part_number - 1) as usize].clone();
                        async move { Ok::<String, std::convert::Infallible>(url) }
                    },
                    |part_number: u32| parts[(part_number - 1) as usize].clone(),
                )
                .await
                .expect("s3 multipart upload");
            UploadCompleteRequest::S3Multipart(S3MultipartComplete { parts: completed })
        }
        UploadPlan::GcsResumable {
            chunk_size_bytes, ..
        } => {
            let chunks = split_chunks(content, *chunk_size_bytes);
            mp.gcs_resumable(plan, chunks)
                .await
                .expect("gcs resumable upload");
            UploadCompleteRequest::GcsResumable(GcsResumableComplete {})
        }
        UploadPlan::AzureBlockBlob {
            block_size_bytes,
            block_count_planned,
            ..
        } => {
            let blocks = split_chunks(content, *block_size_bytes);
            assert_eq!(
                blocks.len(),
                *block_count_planned as usize,
                "client stages the planned block count"
            );
            let block_count = mp
                .azure_block_blob(plan, blocks)
                .await
                .expect("azure block stage");
            UploadCompleteRequest::AzureBlockBlob(AzureBlockBlobComplete { block_count })
        }
        other => panic!("expected a multipart plan for a cloud backend, got: {other:?}"),
    }
}

async fn upload_complete(
    srv: &WyrdTestServer,
    token: &str,
    upload_id: &str,
    body: &UploadCompleteRequest,
) {
    let bytes = serde_json::to_vec(body).expect("complete body serializes");
    let response = post_json(
        srv,
        token,
        &format!("/v1/cards/upload/{upload_id}/complete"),
        bytes,
    )
    .await;
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "upload/complete must succeed"
    );
}

async fn server_download_init(
    srv: &WyrdTestServer,
    token: &str,
    card_uid: &CardUid,
    relative_path: &str,
) -> DownloadInitResponse {
    let body = serde_json::to_vec(&DownloadInitRequest {
        card_uid: card_uid.clone(),
        relative_path: relative_path.to_owned(),
        ttl_secs: None,
    })
    .expect("download init body serializes");
    let response = post_json(srv, token, "/v1/cards/download/init", body).await;
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "download/init must succeed"
    );
    let bytes = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("download init body");
    serde_json::from_slice(&bytes).expect("download init response deserializes")
}

fn gcs_emulator_media_url(storage_path: &str) -> String {
    let host = env_or("WYRD_GCS_EMULATOR_HOST", "http://localhost:4443");
    let bucket = env_or("WYRD_STORAGE_GCS_BUCKET", "wyrd-storage-test");
    let encoded = storage_path.replace('/', "%2F");
    format!("{host}/storage/v1/b/{bucket}/o/{encoded}?alt=media")
}

/// Full server-tier multipart round-trip with end-to-end byte equality.
async fn run_multipart_e2e(srv: WyrdTestServer, relative_path: &str, download: DownloadVia) {
    let token = bootstrap_service_jwt(&srv, "multipart-writer", &["writer"]).await;
    let content = patterned_payload(MULTIPART_PAYLOAD_BYTES);
    let sha256 = sha256_b64(&content);
    let card_uid = CardUid::new(FIXED_CARD_UID).expect("card uid");
    let mp = MultipartClient::new();

    let init = upload_init(
        &srv,
        &token,
        &card_uid,
        relative_path,
        &sha256,
        content.len() as u64,
    )
    .await;
    let upload_id = init.upload_id.to_string();

    let complete = drive_upload(&srv, &token, &mp, &init.plan, &upload_id, &content).await;
    upload_complete(&srv, &token, &upload_id, &complete).await;

    let downloaded = match download {
        DownloadVia::ServerPresign => {
            let dl = server_download_init(&srv, &token, &card_uid, relative_path).await;
            assert_eq!(dl.sha256, sha256, "download/init sha256 must match upload");
            assert_eq!(
                dl.size_bytes,
                content.len() as u64,
                "download/init size must match upload"
            );
            mp.download(&dl.plan.get_url)
                .await
                .expect("download via presigned url")
        }
        DownloadVia::GcsEmulatorMedia => mp
            .download(&gcs_emulator_media_url(&init.storage_path))
            .await
            .expect("download via gcs emulator media url"),
    };

    assert_eq!(
        downloaded.len(),
        content.len(),
        "downloaded length must match"
    );
    assert_eq!(
        downloaded, content,
        "byte-equality across multipart round-trip"
    );

    srv.shutdown().await.expect("shutdown");
}

// ---- Server builders -------------------------------------------------------

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

async fn server_from_settings(settings: StorageSettings) -> WyrdTestServer {
    WyrdTestServer::builder()
        .with_storage_settings(settings)
        .start_in_process()
        .await
        .expect("start server from storage settings")
}

async fn server_from_handle(handle: Arc<StorageHandle>) -> WyrdTestServer {
    WyrdTestServer::builder()
        .with_storage_handle(handle)
        .start_in_process()
        .await
        .expect("start server from storage handle")
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
    let signer = wyrd_storage::factory::gcs::build_emulator_signer(
        &env_or("WYRD_STORAGE_GCS_BUCKET", "wyrd-storage-test"),
        &env_or("WYRD_GCS_EMULATOR_HOST", "http://localhost:4443"),
    )
    .expect("gcs emulator signer");
    Arc::new(StorageHandle::from_signer(
        BackendSigner::Cloud(Box::new(CloudSigner::Gcs(signer))),
        LOW_THRESHOLD_BYTES,
    ))
}

fn gcs_cloud_settings() -> StorageSettings {
    cloud_settings(BackendConfig::Gcs(GcsConfig {
        bucket: env_required("WYRD_STORAGE_GCS_BUCKET"),
        endpoint_url: None,
    }))
}

fn azure_emu_handle() -> Arc<StorageHandle> {
    let signer = wyrd_storage::factory::azure::build_emulator_signer(
        &env_or("WYRD_STORAGE_AZURE_CONTAINER", "wyrd-storage-test"),
        &env_or("WYRD_AZURE_EMULATOR_ENDPOINT", "http://127.0.0.1:10000"),
    )
    .expect("azure emulator signer");
    Arc::new(StorageHandle::from_signer(
        BackendSigner::Cloud(Box::new(CloudSigner::Azure(signer))),
        LOW_THRESHOLD_BYTES,
    ))
}

fn azure_cloud_settings() -> StorageSettings {
    cloud_settings(BackendConfig::Azure(AzureConfig {
        account: env_required("WYRD_STORAGE_AZURE_ACCOUNT"),
        container: env_required("WYRD_STORAGE_AZURE_CONTAINER"),
        endpoint_url: None,
    }))
}

// ---- Entrypoints -----------------------------------------------------------

#[tokio::test(flavor = "current_thread")]
async fn s3_multipart_e2e_emu() {
    if !enabled("WYRD_STORAGE_INTEGRATION_S3") {
        eprintln!("skipping S3 multipart e2e (emu); set WYRD_STORAGE_INTEGRATION_S3=1");
        return;
    }
    let srv = server_from_settings(s3_emu_settings()).await;
    run_multipart_e2e(srv, "s3-multipart/weights.bin", DownloadVia::ServerPresign).await;
}

#[tokio::test(flavor = "current_thread")]
async fn s3_multipart_e2e_cloud() {
    if !enabled("WYRD_STORAGE_CLOUD_S3") {
        eprintln!("skipping S3 multipart e2e (cloud); set WYRD_STORAGE_CLOUD_S3=1");
        return;
    }
    let srv = server_from_settings(s3_cloud_settings()).await;
    run_multipart_e2e(srv, "s3-multipart/weights.bin", DownloadVia::ServerPresign).await;
}

#[tokio::test(flavor = "current_thread")]
async fn gcs_multipart_e2e_emu() {
    if !enabled("WYRD_STORAGE_INTEGRATION_GCS") {
        eprintln!("skipping GCS multipart e2e (emu); set WYRD_STORAGE_INTEGRATION_GCS=1");
        return;
    }
    let srv = server_from_handle(gcs_emu_handle()).await;
    run_multipart_e2e(
        srv,
        "gcs-multipart/weights.bin",
        DownloadVia::GcsEmulatorMedia,
    )
    .await;
}

#[tokio::test(flavor = "current_thread")]
async fn gcs_multipart_e2e_cloud() {
    if !enabled("WYRD_STORAGE_CLOUD_GCS") {
        eprintln!("skipping GCS multipart e2e (cloud); set WYRD_STORAGE_CLOUD_GCS=1");
        return;
    }
    let srv = server_from_settings(gcs_cloud_settings()).await;
    run_multipart_e2e(srv, "gcs-multipart/weights.bin", DownloadVia::ServerPresign).await;
}

#[tokio::test(flavor = "current_thread")]
async fn azure_multipart_e2e_emu() {
    if !enabled("WYRD_STORAGE_INTEGRATION_AZURE") {
        eprintln!("skipping Azure multipart e2e (emu); set WYRD_STORAGE_INTEGRATION_AZURE=1");
        return;
    }
    let srv = server_from_handle(azure_emu_handle()).await;
    run_multipart_e2e(
        srv,
        "azure-multipart/weights.bin",
        DownloadVia::ServerPresign,
    )
    .await;
}

#[tokio::test(flavor = "current_thread")]
async fn azure_multipart_e2e_cloud() {
    if !enabled("WYRD_STORAGE_CLOUD_AZURE") {
        eprintln!("skipping Azure multipart e2e (cloud); set WYRD_STORAGE_CLOUD_AZURE=1");
        return;
    }
    let srv = server_from_settings(azure_cloud_settings()).await;
    run_multipart_e2e(
        srv,
        "azure-multipart/weights.bin",
        DownloadVia::ServerPresign,
    )
    .await;
}
