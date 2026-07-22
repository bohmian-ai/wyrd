//! Façade tests for the real storage-client transfer boundary.

use std::path::Path;
use std::sync::Arc;

use base64::Engine;
use sha2::{Digest, Sha256};
use tempfile::NamedTempFile;
use wiremock::matchers::{body_bytes, header, method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};
use wyrd_client::WyrdClient;
use wyrd_client::auth::AuthMiddleware;
use wyrd_client::config::ClientConfig;
use wyrd_client::transport::HttpTransport;
use wyrd_client::transport::config::HttpConfig;
use wyrd_client::transport::credential::ResolvedCredential;
use wyrd_spec::storage::{HeaderPair, UploadId, UploadPlan};
use wyrd_storage_client::WyrdStorageClient;

fn client(base_url: String) -> WyrdClient {
    let config = ClientConfig {
        http: HttpConfig {
            base_url: base_url.clone(),
            ..HttpConfig::default()
        },
        ..ClientConfig::default()
    };
    let auth = AuthMiddleware::new(
        &config,
        ResolvedCredential::BearerToken("test-bearer".to_owned().into()),
    )
    .expect("test auth builds");
    let transport = HttpTransport::new(&config.http, Arc::clone(&auth)).expect("transport builds");
    WyrdClient::from_parts(auth, transport, config.grpc)
}

fn stored_response() -> serde_json::Value {
    serde_json::json!({
        "stored": {
            "storage_path": "tenant/card/object",
            "size_bytes": 4,
            "sha256": "digest",
            "content_type": null,
            "sse_marker": null,
            "created_at": "2026-01-01T00:00:00Z"
        }
    })
}

#[tokio::test]
async fn high_level_upload_dispatches_single_put_and_localfs() {
    let server = MockServer::start().await;
    let storage = WyrdStorageClient::new(&client(server.uri()));
    for (path_name, plan) in [
        (
            "/single",
            UploadPlan::SinglePut {
                put_url: format!("{}/single", server.uri()),
                ttl_secs: 60,
                required_headers: vec![HeaderPair {
                    name: "x-plan-header".to_owned(),
                    value: "required".to_owned(),
                }],
            },
        ),
        (
            "/v1/cards/upload/local",
            UploadPlan::LocalFs {
                put_url: format!("{}/v1/cards/upload/local", server.uri()),
                ttl_secs: 0,
            },
        ),
    ] {
        Mock::given(method("PUT"))
            .and(path(path_name))
            .respond_with(ResponseTemplate::new(200))
            .mount(&server)
            .await;
        storage
            .upload_artifact(&UploadId::new(), &plan, b"data".to_vec(), "key")
            .await
            .expect("single-plan upload succeeds");
    }
}

#[tokio::test]
async fn high_level_upload_dispatches_s3_and_completes_server_upload() {
    let server = MockServer::start().await;
    let storage = WyrdStorageClient::new(&client(server.uri()));
    let upload_id = UploadId::new();
    Mock::given(method("POST"))
        .and(path(format!("/v1/cards/upload/{upload_id}/part-url")))
        .and(query_param("part_number", "1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "url": format!("{}/part-1", server.uri()),
            "ttl_secs": 60
        })))
        .mount(&server)
        .await;
    Mock::given(method("PUT"))
        .and(path("/part-1"))
        .and(header("Idempotency-Key", "key"))
        .and(body_bytes(b"data"))
        .respond_with(ResponseTemplate::new(200).insert_header("ETag", "etag"))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path(format!("/v1/cards/upload/{upload_id}/complete")))
        .and(header("Idempotency-Key", "key"))
        .respond_with(ResponseTemplate::new(200).set_body_json(stored_response()))
        .mount(&server)
        .await;

    let plan = UploadPlan::S3Multipart {
        part_count: 1,
        part_size_bytes: 4,
        part_url_ttl_secs: 60,
        required_headers: Vec::new(),
    };
    storage
        .upload_artifact(&upload_id, &plan, b"data".to_vec(), "key")
        .await
        .expect("S3 multipart upload succeeds through façade");
}

#[tokio::test]
async fn high_level_upload_dispatches_gcs_and_azure_protocols() {
    let server = MockServer::start().await;
    let storage = WyrdStorageClient::new(&client(server.uri()));
    Mock::given(method("PUT"))
        .and(path("/gcs"))
        .and(body_bytes(b"gcs"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&server)
        .await;
    storage
        .upload_artifact(
            &UploadId::new(),
            &UploadPlan::GcsResumable {
                session_uri: format!("{}/gcs", server.uri()),
                chunk_size_bytes: 3,
            },
            b"gcs".to_vec(),
            "key",
        )
        .await
        .expect("GCS upload succeeds through façade");

    for body in [b"azu".as_slice(), b"re".as_slice()] {
        Mock::given(method("PUT"))
            .and(path("/azure"))
            .and(body_bytes(body))
            .respond_with(ResponseTemplate::new(201))
            .mount(&server)
            .await;
    }
    let upload_id = UploadId::new();
    Mock::given(method("POST"))
        .and(path(format!("/v1/cards/upload/{upload_id}/complete")))
        .respond_with(ResponseTemplate::new(200).set_body_json(stored_response()))
        .mount(&server)
        .await;
    storage
        .upload_artifact(
            &upload_id,
            &UploadPlan::AzureBlockBlob {
                sas_url: format!("{}/azure?sig=redacted", server.uri()),
                block_size_bytes: 3,
                block_count_planned: 2,
            },
            b"azure".to_vec(),
            "key",
        )
        .await
        .expect("Azure upload succeeds through façade");
}

#[tokio::test]
async fn download_and_download_verified_stream_bytes_and_check_digest_and_size() {
    let server = MockServer::start().await;
    let content = b"verified";
    Mock::given(method("GET"))
        .and(path("/download"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(content))
        .mount(&server)
        .await;
    let storage = WyrdStorageClient::new(&client(server.uri()));
    let destination = NamedTempFile::new().expect("destination");
    let digest = base64::engine::general_purpose::STANDARD.encode(Sha256::digest(content));
    storage
        .download_verified(
            &wyrd_spec::storage::DownloadPlan {
                get_url: format!("{}/download", server.uri()),
                ttl_secs: 60,
            },
            Path::new(destination.path()),
            &digest,
            content.len() as u64,
        )
        .await
        .expect("verified download succeeds");
    assert_eq!(
        tokio::fs::read(destination.path())
            .await
            .expect("read destination"),
        content
    );
}
