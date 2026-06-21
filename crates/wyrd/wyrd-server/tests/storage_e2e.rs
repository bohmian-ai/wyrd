//! Storage HTTP e2e integration tests.
//!
//! Drives the full request chain an SDK client would walk: upload-init →
//! byte PUT → complete → download-init → byte GET → compare. Env-gated:
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
use std::time::Duration;
use wyrd_spec::ids::CardUid;
use wyrd_spec::storage::{
    AbortResponse, DownloadInitRequest, PartUrlResponse, S3CompletedPart, S3MultipartComplete,
    SinglePutComplete, UploadCompleteRequest, UploadInitRequest, UploadPlan,
};
use wyrd_storage::settings::S3Config;
use wyrd_storage::{BackendConfig, StorageSettings};
use wyrd_testing::WyrdTestServer;

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
    if skip_unless_e2e() {
        return;
    }
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

fn skip_unless_s3_integration() -> bool {
    if std::env::var("WYRD_STORAGE_INTEGRATION_S3").as_deref() != Ok("1") {
        eprintln!("skipping S3 multipart e2e; set WYRD_STORAGE_INTEGRATION_S3=1");
        return true;
    }
    false
}

#[tokio::test(flavor = "current_thread")]
async fn s3_multipart_upload_download_round_trip() {
    if skip_unless_e2e() {
        return;
    }
    if skip_unless_s3_integration() {
        return;
    }

    let settings = StorageSettings {
        backend: BackendConfig::S3(S3Config {
            bucket: "wyrd-storage-test".to_owned(),
            region: Some("us-east-1".to_owned()),
            endpoint_url: Some("http://localhost:9000".to_owned()),
            force_path_style: true,
        }),
        require_encryption: false,
        presign_ttl: Duration::from_secs(600),
        part_size_bytes: 5 * 1024 * 1024,
        multipart_threshold_bytes: 5 * 1024 * 1024,
        public_base_url: None,
    };

    let srv = WyrdTestServer::builder()
        .with_storage_settings(settings)
        .start_in_process()
        .await
        .expect("start env with S3");

    let token = bootstrap_service_jwt(&srv, "s3-multipart-writer", &["writer"]).await;

    const PART_SIZE: usize = 5 * 1024 * 1024;
    const PART_COUNT: u32 = 2;
    let part1 = vec![0x41u8; PART_SIZE];
    let part2 = vec![0x42u8; PART_SIZE];
    let full_content: Vec<u8> = [part1.as_slice(), part2.as_slice()].concat();
    let sha256 = sha256_b64(&full_content);
    let card_uid = CardUid::new(FIXED_CARD_UID).expect("card uid");

    let init_body = serde_json::to_vec(&UploadInitRequest {
        card_uid: card_uid.clone(),
        relative_path: "s3-multipart/weights.bin".to_owned(),
        expected_sha256: sha256.clone(),
        expected_size_bytes: full_content.len() as u64,
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
    assert_eq!(init_response.status(), StatusCode::OK, "upload/init must succeed");

    let init_bytes = to_bytes(init_response.into_body(), usize::MAX)
        .await
        .expect("init body");
    let init: wyrd_spec::storage::UploadInitResponse =
        serde_json::from_slice(&init_bytes).expect("init response deserializes");

    let upload_id = init.upload_id.clone();
    let UploadPlan::S3Multipart { part_count, .. } = &init.plan else {
        panic!("expected S3Multipart plan for S3 backend, got: {:?}", init.plan);
    };
    assert_eq!(*part_count, PART_COUNT, "server planned {PART_COUNT} parts");

    let parts_data = [part1, part2];
    let http_client = reqwest::Client::new();
    let mut completed_parts: Vec<S3CompletedPart> = Vec::new();

    for part_number in 1..=PART_COUNT {
        let part_url_response = srv
            .oneshot_authenticated(
                &token,
                request(
                    "POST",
                    &format!("/v1/cards/upload/{upload_id}/part-url?part_number={part_number}"),
                    Body::empty(),
                    None,
                ),
            )
            .await
            .expect("router responds");
        assert_eq!(
            part_url_response.status(),
            StatusCode::OK,
            "part-url for part {part_number} must succeed"
        );

        let part_url_bytes = to_bytes(part_url_response.into_body(), usize::MAX)
            .await
            .expect("part-url body");
        let part_url_resp: PartUrlResponse =
            serde_json::from_slice(&part_url_bytes).expect("PartUrlResponse deserializes");

        let put_response = http_client
            .put(&part_url_resp.url)
            .body(parts_data[(part_number - 1) as usize].clone())
            .send()
            .await
            .expect("PUT part to RustFS");
        assert!(
            put_response.status().is_success(),
            "part {part_number} PUT failed: {}",
            put_response.status()
        );

        let e_tag = put_response
            .headers()
            .get("etag")
            .expect("part PUT must return ETag header")
            .to_str()
            .expect("ETag is UTF-8")
            .to_owned();
        completed_parts.push(S3CompletedPart { part_number, e_tag });
    }

    let complete_body = serde_json::to_vec(&UploadCompleteRequest::S3Multipart(
        S3MultipartComplete {
            parts: completed_parts,
        },
    ))
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
        relative_path: "s3-multipart/weights.bin".to_owned(),
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

    assert_eq!(dl_init.sha256, sha256, "sha256 must match upload");
    assert_eq!(
        dl_init.size_bytes,
        full_content.len() as u64,
        "size must match upload"
    );

    let downloaded = http_client
        .get(&dl_init.plan.get_url)
        .send()
        .await
        .expect("GET from RustFS via presigned URL")
        .bytes()
        .await
        .expect("download bytes");

    assert_eq!(
        downloaded.len(),
        full_content.len(),
        "downloaded length must match"
    );
    assert_eq!(
        downloaded.as_ref(),
        full_content.as_slice(),
        "byte-equality across multipart round-trip"
    );

    srv.shutdown().await.expect("shutdown");
}
