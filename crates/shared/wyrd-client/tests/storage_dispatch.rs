//! Façade tests for the real storage-client transfer boundary.

use std::path::Path;
use std::sync::{Arc, Mutex as StdMutex};

use base64::Engine;
use sha2::{Digest, Sha256};
use tempfile::NamedTempFile;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::Mutex;
use wyrd_client::WyrdClient;
use wyrd_client::auth::AuthMiddleware;
use wyrd_client::config::ClientConfig;
use wyrd_client::storage::{DownloadProgressSink, StorageClientError, WyrdStorageClient};
use wyrd_client::transport::HttpTransport;
use wyrd_client::transport::config::HttpConfig;
use wyrd_client::transport::credential::ResolvedCredential;
use wyrd_spec::storage::{HeaderPair, UploadId, UploadPlan};

#[derive(Clone, Debug)]
struct Request {
    method: String,
    target: String,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
}

#[derive(Clone, Debug)]
struct Response {
    status: u16,
    body: Vec<u8>,
    headers: Vec<(String, String)>,
}

struct TestServer {
    uri: String,
    requests: Arc<Mutex<Vec<Request>>>,
    task: tokio::task::JoinHandle<()>,
}

fn sha256_b64(bytes: &[u8]) -> String {
    base64::engine::general_purpose::STANDARD.encode(Sha256::digest(bytes))
}

impl TestServer {
    async fn start(responses: Vec<Response>) -> Self {
        Self::start_with(|_| responses).await
    }

    async fn start_with(build: impl FnOnce(&str) -> Vec<Response>) -> Self {
        let listener = TcpListener::bind(("127.0.0.1", 0))
            .await
            .expect("test listener binds");
        let uri = format!(
            "http://{}",
            listener.local_addr().expect("listener address")
        );
        let responses = build(&uri);
        let requests = Arc::new(Mutex::new(Vec::new()));
        let captured = Arc::clone(&requests);
        let task = tokio::spawn(async move {
            for response in responses {
                let (mut stream, _) = listener.accept().await.expect("test request connects");
                let request = read_request(&mut stream).await;
                captured.lock().await.push(request);
                write_response(&mut stream, response)
                    .await
                    .expect("test response writes");
            }
        });
        Self {
            uri,
            requests,
            task,
        }
    }

    async fn requests(&self) -> Vec<Request> {
        self.requests.lock().await.clone()
    }

    fn uri(&self) -> &str {
        &self.uri
    }
}

impl Drop for TestServer {
    fn drop(&mut self) {
        self.task.abort();
    }
}

async fn read_request(stream: &mut tokio::net::TcpStream) -> Request {
    let mut bytes = Vec::new();
    let mut chunk = [0_u8; 4096];
    let header_end = loop {
        let read = stream.read(&mut chunk).await.expect("test request reads");
        assert!(read > 0, "request ended before headers");
        bytes.extend_from_slice(&chunk[..read]);
        if let Some(end) = bytes.windows(4).position(|window| window == b"\r\n\r\n") {
            break end + 4;
        }
    };
    let header_text = String::from_utf8_lossy(&bytes[..header_end]).into_owned();
    let mut lines = header_text.lines();
    let request_line = lines.next().expect("request line exists");
    let mut parts = request_line.split_whitespace();
    let method = parts.next().expect("request method exists").to_owned();
    let target = parts.next().expect("request target exists").to_owned();
    let headers: Vec<_> = lines
        .filter_map(|line| line.split_once(':'))
        .map(|(name, value)| (name.to_ascii_lowercase(), value.trim().to_owned()))
        .collect();
    let content_length = headers
        .iter()
        .find(|(name, _)| name == "content-length")
        .and_then(|(_, value)| value.parse::<usize>().ok())
        .unwrap_or(0);
    let chunked = headers
        .iter()
        .any(|(name, value)| name == "transfer-encoding" && value.eq_ignore_ascii_case("chunked"));
    let body = read_body(
        stream,
        &mut bytes,
        &mut chunk,
        header_end,
        content_length,
        chunked,
    )
    .await;
    Request {
        method,
        target,
        headers,
        body,
    }
}

async fn read_body(
    stream: &mut tokio::net::TcpStream,
    bytes: &mut Vec<u8>,
    chunk: &mut [u8; 4096],
    header_end: usize,
    content_length: usize,
    chunked: bool,
) -> Vec<u8> {
    if !chunked {
        while bytes.len() < header_end + content_length {
            let read = stream.read(chunk).await.expect("test body reads");
            assert!(read > 0, "request ended before body");
            bytes.extend_from_slice(&chunk[..read]);
        }
        return bytes[header_end..header_end + content_length].to_vec();
    }

    let mut cursor = header_end;
    let mut body = Vec::new();
    loop {
        let line_end = loop {
            if let Some(offset) = bytes[cursor..]
                .windows(2)
                .position(|window| window == b"\r\n")
            {
                break cursor + offset;
            }
            let read = stream.read(chunk).await.expect("test chunk header reads");
            assert!(read > 0, "request ended before chunk header");
            bytes.extend_from_slice(&chunk[..read]);
        };
        let size = usize::from_str_radix(
            std::str::from_utf8(&bytes[cursor..line_end])
                .expect("chunk size is UTF-8")
                .split(';')
                .next()
                .expect("chunk size exists"),
            16,
        )
        .expect("chunk size is hexadecimal");
        cursor = line_end + 2;
        while bytes.len() < cursor + size + 2 {
            let read = stream.read(chunk).await.expect("test chunk reads");
            assert!(read > 0, "request ended before chunk body");
            bytes.extend_from_slice(&chunk[..read]);
        }
        if size == 0 {
            return body;
        }
        body.extend_from_slice(&bytes[cursor..cursor + size]);
        cursor += size + 2;
    }
}

async fn write_response(
    stream: &mut tokio::net::TcpStream,
    response: Response,
) -> std::io::Result<()> {
    let reason = match response.status {
        200 => "OK",
        201 => "Created",
        _ => "Test",
    };
    let head = format!(
        "HTTP/1.1 {} {}\r\ncontent-length: {}\r\ncontent-type: application/json\r\n{}connection: close\r\n\r\n",
        response.status,
        reason,
        response.body.len(),
        response
            .headers
            .iter()
            .map(|(name, value)| format!("{name}: {value}\r\n"))
            .collect::<String>()
    );
    stream.write_all(head.as_bytes()).await?;
    stream.write_all(&response.body).await
}

fn response(status: u16) -> Response {
    Response {
        status,
        body: Vec::new(),
        headers: Vec::new(),
    }
}

fn response_with_headers(status: u16, headers: Vec<(&str, &str)>) -> Response {
    Response {
        status,
        body: Vec::new(),
        headers: headers
            .into_iter()
            .map(|(name, value)| (name.to_owned(), value.to_owned()))
            .collect(),
    }
}

fn stored_response() -> Response {
    Response {
        status: 200,
        body: serde_json::to_vec(&serde_json::json!({
            "stored": {
                "storage_path": "tenant/card/object",
                "size_bytes": 4,
                "sha256": "digest",
                "content_type": null,
                "sse_marker": null,
                "created_at": "2026-01-01T00:00:00Z"
            }
        }))
        .expect("test JSON serializes"),
        headers: Vec::new(),
    }
}

fn client(base_url: impl Into<String>) -> WyrdClient {
    let base_url = base_url.into();
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

#[tokio::test]
async fn high_level_upload_dispatches_single_put_and_localfs() {
    let server = TestServer::start(vec![
        response(200),
        stored_response(),
        response(200),
        stored_response(),
    ])
    .await;
    let storage = WyrdStorageClient::new(&client(server.uri()));
    for plan in [
        UploadPlan::SinglePut {
            put_url: format!("{}/single", server.uri()),
            ttl_secs: 60,
            required_headers: vec![HeaderPair {
                name: "x-plan-header".to_owned(),
                value: "required".to_owned(),
            }],
        },
        UploadPlan::LocalFs {
            put_url: format!("{}/v1/cards/upload/local", server.uri()),
            ttl_secs: 0,
        },
    ] {
        let upload_id = UploadId::new();
        storage
            .upload_artifact(&upload_id, &plan, b"data".to_vec(), "key")
            .await
            .expect("single-plan upload succeeds");
    }
    let requests = server.requests().await;
    assert_eq!(requests.len(), 4);
    assert_eq!(requests[0].method, "PUT");
    assert_eq!(requests[0].target, "/single");
    assert_eq!(requests[0].body, b"data");
    assert_eq!(requests[1].method, "POST");
    assert!(requests[1].target.starts_with("/v1/cards/upload/"));
    assert!(
        requests[1]
            .headers
            .iter()
            .any(|(name, value)| name == "idempotency-key" && value == "key")
    );
    assert_eq!(requests[2].method, "PUT");
    assert_eq!(requests[2].target, "/v1/cards/upload/local");
}

#[tokio::test]
async fn high_level_upload_dispatches_s3_and_completes_server_upload() {
    let server = TestServer::start_with(|uri| {
        vec![
            Response {
                status: 200,
                body: serde_json::to_vec(&serde_json::json!({
                    "url": format!("{uri}/part-1"),
                    "ttl_secs": 60
                }))
                .expect("test JSON serializes"),
                headers: Vec::new(),
            },
            response_with_headers(200, vec![("ETag", "etag")]),
            stored_response(),
        ]
    })
    .await;
    let storage = WyrdStorageClient::new(&client(server.uri()));
    let upload_id = UploadId::new();

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
    let requests = server.requests().await;
    assert_eq!(requests[0].method, "POST");
    assert!(requests[0].target.starts_with("/v1/cards/upload/"));
    assert!(requests[0].target.ends_with("/part-url?part_number=1"));
    assert_eq!(requests[1].method, "PUT");
    assert_eq!(requests[1].body, b"data");
    assert_eq!(requests[2].method, "POST");
    assert!(requests[2].target.ends_with("/complete"));
}

#[tokio::test]
async fn high_level_upload_dispatches_gcs_and_azure_protocols() {
    let server = TestServer::start(vec![
        response(200),
        stored_response(),
        response(201),
        response(201),
        stored_response(),
    ])
    .await;
    let storage = WyrdStorageClient::new(&client(server.uri()));
    let gcs_upload_id = UploadId::new();
    storage
        .upload_artifact(
            &gcs_upload_id,
            &UploadPlan::GcsResumable {
                session_uri: format!("{}/gcs", server.uri()),
                chunk_size_bytes: 3,
            },
            b"gcs".to_vec(),
            "key",
        )
        .await
        .expect("GCS upload succeeds through façade");
    let upload_id = UploadId::new();
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
    let requests = server.requests().await;
    assert_eq!(requests[0].method, "PUT");
    assert_eq!(requests[0].target, "/gcs");
    assert_eq!(requests[0].body, b"gcs");
    assert_eq!(requests[1].method, "POST");
    assert!(requests[1].target.ends_with("/complete"));
    assert_eq!(requests[2].method, "PUT");
    assert!(
        requests[2]
            .target
            .starts_with("/azure?sig=redacted&comp=block&blockid=")
    );
    assert_eq!(requests[2].body, b"azu");
    assert_eq!(requests[3].body, b"re");
    assert_eq!(requests[4].method, "POST");
}

/// Verifies streamed download bytes, digest checks, and monotonic progress reports.
#[tokio::test]
async fn download_and_download_verified_stream_bytes_and_check_digest_and_size() {
    let content = vec![b'x'; 256 * 1024];
    let server = TestServer::start(vec![Response {
        status: 200,
        body: content.clone(),
        headers: Vec::new(),
    }])
    .await;
    let storage = WyrdStorageClient::new(&client(server.uri()));
    let destination = NamedTempFile::new().expect("destination");
    let digest = base64::engine::general_purpose::STANDARD.encode(Sha256::digest(&content));
    let positions = Arc::new(StdMutex::new(Vec::new()));
    let observed_positions = Arc::clone(&positions);
    let progress: DownloadProgressSink = Arc::new(move |written, total| {
        observed_positions
            .lock()
            .expect("test progress mutex is not poisoned")
            .push((written, total));
    });
    storage
        .download_verified_with_progress(
            &wyrd_spec::storage::DownloadPlan {
                get_url: format!("{}/download", server.uri()),
                ttl_secs: 60,
            },
            Path::new(destination.path()),
            &digest,
            content.len() as u64,
            progress,
        )
        .await
        .expect("verified download succeeds");
    assert_eq!(
        tokio::fs::read(destination.path())
            .await
            .expect("read destination"),
        content.as_slice()
    );
    let requests = server.requests().await;
    assert_eq!(requests[0].method, "GET");
    assert_eq!(requests[0].target, "/download");
    let positions = positions
        .lock()
        .expect("test progress mutex is not poisoned");
    assert_eq!(
        positions.last(),
        Some(&(content.len() as u64, Some(content.len() as u64)))
    );
    assert!(
        positions.len() > 1,
        "response must stream in multiple chunks"
    );
    assert!(positions.windows(2).all(|window| window[0].0 < window[1].0));
}

#[tokio::test]
async fn façade_rejects_invalid_provider_plans_before_transfer() {
    let server = TestServer::start(Vec::new()).await;
    let storage = WyrdStorageClient::new(&client(server.uri()));
    for plan in [
        UploadPlan::S3Multipart {
            part_count: 0,
            part_size_bytes: 1,
            part_url_ttl_secs: 60,
            required_headers: Vec::new(),
        },
        UploadPlan::GcsResumable {
            session_uri: format!("{}/gcs", server.uri()),
            chunk_size_bytes: 0,
        },
        UploadPlan::AzureBlockBlob {
            sas_url: format!("{}/azure", server.uri()),
            block_size_bytes: 1,
            block_count_planned: 0,
        },
    ] {
        let error = storage
            .upload_artifact(&UploadId::new(), &plan, b"data".to_vec(), "key")
            .await
            .expect_err("invalid provider plan is rejected");
        assert!(matches!(error, StorageClientError::PlanInvalid(_)));
    }
    assert!(server.requests().await.is_empty());
}

/// Preserves backend and verification errors without reporting false completion.
#[tokio::test]
async fn façade_preserves_backend_failure_and_download_verification_errors() {
    let server = TestServer::start(vec![
        response(503),
        Response {
            status: 200,
            body: b"wrong".to_vec(),
            headers: Vec::new(),
        },
    ])
    .await;
    let storage = WyrdStorageClient::new(&client(server.uri()));
    let error = storage
        .upload_artifact(
            &UploadId::new(),
            &UploadPlan::SinglePut {
                put_url: format!("{}/single", server.uri()),
                ttl_secs: 60,
                required_headers: Vec::new(),
            },
            b"data".to_vec(),
            "key",
        )
        .await
        .expect_err("backend failure is returned");
    let error: wyrd_spec::error::WyrdError = error.into();
    assert_eq!(error.code(), "WYRD_STORAGE_503_BACKEND_UNAVAILABLE");

    let destination = NamedTempFile::new().expect("destination");
    let positions = Arc::new(StdMutex::new(Vec::new()));
    let observed_positions = Arc::clone(&positions);
    let error = storage
        .download_verified_with_progress(
            &wyrd_spec::storage::DownloadPlan {
                get_url: format!("{}/download", server.uri()),
                ttl_secs: 60,
            },
            destination.path(),
            &sha256_b64(b"expected"),
            8,
            Arc::new(move |written, total| {
                observed_positions
                    .lock()
                    .expect("test progress mutex is not poisoned")
                    .push((written, total));
            }),
        )
        .await
        .expect_err("download verification failure is returned");
    assert!(matches!(error, StorageClientError::VerifyFailed { .. }));
    assert_eq!(
        positions
            .lock()
            .expect("test progress mutex is not poisoned")
            .last(),
        Some(&(b"wrong".len() as u64, Some(8)))
    );
}
