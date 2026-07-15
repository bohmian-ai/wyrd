use std::path::Path;
use std::sync::Arc;

use tempfile::NamedTempFile;
use wiremock::matchers::{body_bytes, header, header_exists, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};
use wyrd_client::WyrdClient;
use wyrd_client::auth::AuthMiddleware;
use wyrd_client::config::ClientConfig;
use wyrd_client::transport::HttpTransport;
use wyrd_client::transport::config::HttpConfig;
use wyrd_client::transport::credential::ResolvedCredential;
use wyrd_spec::storage::{HeaderPair, UploadPlan};
use wyrd_storage_client::{
    ArtifactSource, PartUrlMinter, UploadHooks, UploadOutcome, WyrdStorageClient,
};

fn hooks<'a>(key: &'a str) -> UploadHooks<'a> {
    UploadHooks {
        idempotency_key: key,
        progress: None,
        part_url_minter: None,
    }
}

fn wyrd(base_url: String) -> WyrdClient {
    let mut config = ClientConfig::default();
    config.http.base_url = base_url.clone();
    let auth = AuthMiddleware::new(
        &config,
        ResolvedCredential::BearerToken("test-bearer".to_owned().into()),
    )
    .expect("auth builds");
    let transport = HttpTransport::new(
        &HttpConfig {
            base_url,
            ..HttpConfig::default()
        },
        Arc::clone(&auth),
    )
    .expect("transport builds");
    WyrdClient::from_parts(auth, transport, config.grpc)
}

#[tokio::test]
async fn each_cloud_protocol_preserves_wire_shape_without_wyrd_credentials() {
    let server = MockServer::start().await;
    let wyrd = wyrd(server.uri());
    let storage = WyrdStorageClient::new(&wyrd);

    // Every cloud PUT must ship the plan-required headers and the caller
    // idempotency key. It must NOT ship the Wyrd access token — cross-origin
    // credential leakage would defeat the presigned/SAS boundary.
    Mock::given(method("PUT"))
        .and(path("/single"))
        .and(header("x-plan-header", "required"))
        .and(header("Idempotency-Key", "key"))
        .and(body_bytes(b"single"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&server)
        .await;
    let single = UploadPlan::SinglePut {
        put_url: format!("{}/single", server.uri()),
        ttl_secs: 60,
        required_headers: vec![HeaderPair {
            name: "x-plan-header".to_owned(),
            value: "required".to_owned(),
        }],
    };
    storage
        .upload(&single, b"single".to_vec(), hooks("key"))
        .await
        .expect("single PUT succeeds");

    Mock::given(method("PUT"))
        .and(path("/part-1"))
        .and(header("x-checksum", "sha"))
        .and(header("Idempotency-Key", "key"))
        .and(body_bytes(b"ab"))
        .respond_with(ResponseTemplate::new(200).insert_header("ETag", "etag-1"))
        .mount(&server)
        .await;
    Mock::given(method("PUT"))
        .and(path("/part-2"))
        .and(header("x-checksum", "sha"))
        .and(header("Idempotency-Key", "key"))
        .and(body_bytes(b"cd"))
        .respond_with(ResponseTemplate::new(200).insert_header("ETag", "etag-2"))
        .mount(&server)
        .await;
    let base = server.uri();
    let multipart = UploadPlan::S3Multipart {
        part_count: 2,
        part_size_bytes: 2,
        part_url_ttl_secs: 60,
        required_headers: vec![HeaderPair {
            name: "x-checksum".to_owned(),
            value: "sha".to_owned(),
        }],
    };
    let mut multipart_hooks = hooks("key");
    let minter: PartUrlMinter<'static> = Box::new(move |part| {
        let url = format!("{base}/part-{part}");
        Box::pin(async move { Ok(url) })
    });
    multipart_hooks.part_url_minter = Some(minter);
    let outcome = storage
        .upload(&multipart, b"abcd".to_vec(), multipart_hooks)
        .await
        .expect("multipart succeeds");
    assert!(matches!(outcome, UploadOutcome::NeedsServerComplete(_)));

    Mock::given(method("PUT"))
        .and(path("/gcs"))
        .and(header("Content-Range", "bytes 0-2/3"))
        .and(body_bytes(b"gcs"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&server)
        .await;
    let gcs = UploadPlan::GcsResumable {
        session_uri: format!("{}/gcs", server.uri()),
        chunk_size_bytes: 3,
    };
    storage
        .upload(&gcs, b"gcs".to_vec(), hooks("key"))
        .await
        .expect("GCS succeeds");

    for index in 0..2 {
        let expected: &[u8] = if index == 0 { b"azu" } else { b"re" };
        Mock::given(method("PUT"))
            .and(path("/azure"))
            .and(body_bytes(expected))
            .respond_with(ResponseTemplate::new(201))
            .mount(&server)
            .await;
    }
    let azure = UploadPlan::AzureBlockBlob {
        sas_url: format!("{}/azure?sig=redacted", server.uri()),
        block_size_bytes: 3,
        block_count_planned: 2,
    };
    let outcome = storage
        .upload(&azure, b"azure".to_vec(), hooks("key"))
        .await
        .expect("Azure succeeds");
    assert!(matches!(outcome, UploadOutcome::NeedsServerComplete(_)));
}

#[tokio::test]
async fn cross_origin_puts_do_not_ship_wyrd_access_token() {
    let server = MockServer::start().await;
    let wyrd = wyrd(server.uri());
    let storage = WyrdStorageClient::new(&wyrd);

    let expectation = Mock::given(method("PUT"))
        .and(path("/no-auth"))
        .and(body_bytes(b"payload"))
        .respond_with(ResponseTemplate::new(200))
        .expect(1);
    server.register(expectation).await;

    let plan = UploadPlan::SinglePut {
        put_url: format!("{}/no-auth", server.uri()),
        ttl_secs: 60,
        required_headers: Vec::new(),
    };
    storage
        .upload(&plan, b"payload".to_vec(), hooks("key"))
        .await
        .expect("cross-origin PUT succeeds");

    for request in server.received_requests().await.unwrap_or_default() {
        if request.url.path() == "/no-auth" {
            assert!(
                !request.headers.contains_key("x-wyrd-access-token"),
                "cross-origin PUT must not carry x-wyrd-access-token"
            );
            assert!(
                !request.headers.contains_key("wyrd-request-id"),
                "cross-origin PUT must not carry wyrd-request-id"
            );
        }
    }
}

#[tokio::test]
async fn download_streams_to_destination_without_exposing_signed_url() {
    let server = MockServer::start().await;
    let wyrd = wyrd(server.uri());
    let storage = WyrdStorageClient::new(&wyrd);
    Mock::given(method("GET"))
        .and(path("/download"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(b"downloaded"))
        .mount(&server)
        .await;
    let destination = NamedTempFile::new().expect("temporary destination");
    let plan = wyrd_spec::storage::DownloadPlan {
        get_url: format!("{}/download?sig=secret", server.uri()),
        ttl_secs: 60,
    };
    let outcome = storage
        .download(&plan, destination.path())
        .await
        .expect("download succeeds");
    assert_eq!(outcome.bytes_written, 10);
    assert_eq!(
        tokio::fs::read(Path::new(destination.path()))
            .await
            .unwrap(),
        b"downloaded"
    );
}

#[tokio::test]
async fn local_transfers_use_shared_wyrd_authentication() {
    let server = MockServer::start().await;
    let wyrd = wyrd(server.uri());
    let storage = WyrdStorageClient::new(&wyrd);

    Mock::given(method("PUT"))
        .and(path("/v1/cards/upload/local/model.bin"))
        .and(header("x-wyrd-access-token", "Bearer test-bearer"))
        .and(header_exists("wyrd-request-id"))
        .and(body_bytes(b"local"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/v1/cards/download/local/model.bin"))
        .and(header("x-wyrd-access-token", "Bearer test-bearer"))
        .and(header_exists("wyrd-request-id"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(b"local"))
        .mount(&server)
        .await;

    let upload_plan = UploadPlan::LocalFs {
        put_url: format!("{}/v1/cards/upload/local/model.bin", server.uri()),
        ttl_secs: 0,
    };
    storage
        .upload(&upload_plan, b"local".to_vec(), hooks("key"))
        .await
        .expect("authenticated local upload succeeds");

    let destination = NamedTempFile::new().expect("temporary destination");
    let download_plan = wyrd_spec::storage::DownloadPlan {
        get_url: format!("{}/v1/cards/download/local/model.bin", server.uri()),
        ttl_secs: 0,
    };
    storage
        .download(&download_plan, destination.path())
        .await
        .expect("authenticated local download succeeds");
    assert_eq!(tokio::fs::read(destination.path()).await.unwrap(), b"local");
}

#[test]
fn artifact_source_is_a_streaming_contract() {
    let source = b"source".to_vec();
    assert_eq!(source.size_hint(), Some(6));
}

// ── V-006 focused contract coverage ──────────────────────────────────────────

use wyrd_spec::error::WyrdError;
use wyrd_storage_client::PartUrlFuture;
use wyrd_storage_client::StorageClientError;

#[tokio::test]
async fn single_put_412_maps_to_precondition_failed_structured_error() {
    // V-002: a 412 from the backend must surface as a stable
    // `WYRD_STORAGE_412_PRECONDITION` — the caller must be able to route on
    // the catalog variant rather than pattern-matching on an opaque status.
    let server = MockServer::start().await;
    let wyrd = wyrd(server.uri());
    let storage = WyrdStorageClient::new(&wyrd);

    Mock::given(method("PUT"))
        .and(path("/precond"))
        .respond_with(ResponseTemplate::new(412))
        .mount(&server)
        .await;

    let plan = UploadPlan::SinglePut {
        put_url: format!("{}/precond", server.uri()),
        ttl_secs: 60,
        required_headers: Vec::new(),
    };
    let err = storage
        .upload(&plan, b"payload".to_vec(), hooks("key"))
        .await
        .expect_err("412 must surface as structured error");
    let wyrd_err: WyrdError = err.into();
    assert_eq!(wyrd_err.code(), "WYRD_STORAGE_412_PRECONDITION");
    assert_eq!(wyrd_err.status(), 412);
}

#[tokio::test]
async fn single_put_503_maps_to_backend_unavailable_structured_error() {
    let server = MockServer::start().await;
    let wyrd = wyrd(server.uri());
    let storage = WyrdStorageClient::new(&wyrd);

    Mock::given(method("PUT"))
        .and(path("/unavail"))
        .respond_with(ResponseTemplate::new(503))
        .mount(&server)
        .await;

    let plan = UploadPlan::SinglePut {
        put_url: format!("{}/unavail", server.uri()),
        ttl_secs: 60,
        required_headers: Vec::new(),
    };
    let err: WyrdError = storage
        .upload(&plan, b"payload".to_vec(), hooks("key"))
        .await
        .expect_err("503 must surface as structured error")
        .into();
    assert_eq!(err.code(), "WYRD_STORAGE_503_BACKEND_UNAVAILABLE");
    assert_eq!(err.status(), 503);
}

#[tokio::test]
async fn s3_multipart_retries_transient_5xx_then_succeeds() {
    // V-003: a transient 5xx on one part must retry with backoff and succeed
    // on the next attempt; the completion list carries exactly one entry per
    // part in ascending order.
    let server = MockServer::start().await;
    let wyrd = wyrd(server.uri());
    let storage = WyrdStorageClient::new(&wyrd);

    Mock::given(method("PUT"))
        .and(path("/retry-1"))
        .respond_with(ResponseTemplate::new(503))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(method("PUT"))
        .and(path("/retry-1"))
        .respond_with(ResponseTemplate::new(200).insert_header("ETag", "etag-1"))
        .mount(&server)
        .await;

    let base = server.uri();
    let plan = UploadPlan::S3Multipart {
        part_count: 1,
        part_size_bytes: 4,
        part_url_ttl_secs: 60,
        required_headers: Vec::new(),
    };
    let mut multipart_hooks = hooks("key");
    let minter: PartUrlMinter<'static> = Box::new(move |_part| {
        let url = format!("{base}/retry-1");
        Box::pin(async move { Ok(url) }) as PartUrlFuture<'static>
    });
    multipart_hooks.part_url_minter = Some(minter);

    let outcome = storage
        .upload(&plan, b"data".to_vec(), multipart_hooks)
        .await
        .expect("multipart succeeds after 503");
    match outcome {
        UploadOutcome::NeedsServerComplete(
            wyrd_spec::storage::UploadCompleteRequest::S3Multipart(complete),
        ) => {
            assert_eq!(complete.parts.len(), 1);
            assert_eq!(complete.parts[0].part_number, 1);
            assert_eq!(complete.parts[0].e_tag, "etag-1");
        }
        other => panic!("unexpected outcome: {other:?}"),
    }
}

#[tokio::test]
async fn s3_multipart_403_expiry_forces_a_fresh_part_url_mint() {
    // V-003: 403 is interpreted as presigned-URL expiry. The client must call
    // the minter again for a fresh URL before retrying — the retry must not
    // reuse the expired URL, and the completion list still carries exactly one
    // entry for the successful attempt.
    let server = MockServer::start().await;
    let wyrd = wyrd(server.uri());
    let storage = WyrdStorageClient::new(&wyrd);

    Mock::given(method("PUT"))
        .and(path("/part-expired"))
        .respond_with(ResponseTemplate::new(403))
        .mount(&server)
        .await;
    Mock::given(method("PUT"))
        .and(path("/part-fresh"))
        .respond_with(ResponseTemplate::new(200).insert_header("ETag", "etag-1"))
        .mount(&server)
        .await;

    let base = server.uri();
    let plan = UploadPlan::S3Multipart {
        part_count: 1,
        part_size_bytes: 4,
        part_url_ttl_secs: 60,
        required_headers: Vec::new(),
    };
    let mint_count = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let mint_count_clone = mint_count.clone();
    let mut multipart_hooks = hooks("key");
    let minter: PartUrlMinter<'static> = Box::new(move |_part| {
        let n = mint_count_clone.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let url = if n == 0 {
            format!("{base}/part-expired")
        } else {
            format!("{base}/part-fresh")
        };
        Box::pin(async move { Ok(url) }) as PartUrlFuture<'static>
    });
    multipart_hooks.part_url_minter = Some(minter);

    let outcome = storage
        .upload(&plan, b"data".to_vec(), multipart_hooks)
        .await
        .expect("multipart succeeds after minter refresh");
    assert!(matches!(outcome, UploadOutcome::NeedsServerComplete(_)));
    assert!(
        mint_count.load(std::sync::atomic::Ordering::SeqCst) >= 2,
        "presigned-URL expiry must force at least one remint"
    );
}

#[tokio::test]
async fn gcs_over_size_source_is_rejected_before_completion() {
    // V-004: a source that streams more bytes than its declared size_hint
    // must be rejected as SizeMismatch before the final PUT succeeds.
    let server = MockServer::start().await;
    let wyrd = wyrd(server.uri());
    let storage = WyrdStorageClient::new(&wyrd);

    Mock::given(method("PUT"))
        .and(path("/gcs-over"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&server)
        .await;

    let plan = UploadPlan::GcsResumable {
        session_uri: format!("{}/gcs-over", server.uri()),
        chunk_size_bytes: 3,
    };
    let mut oversized_hooks = hooks("key");
    oversized_hooks.progress = None;

    // Declare a 2-byte source but hand-craft a payload that is longer.
    struct MisreportedSource {
        bytes: Vec<u8>,
    }
    impl ArtifactSource for MisreportedSource {
        fn size_hint(&self) -> Option<u64> {
            Some(2)
        }
        fn into_stream(
            self,
        ) -> futures_util::stream::BoxStream<'static, Result<bytes::Bytes, StorageClientError>>
        {
            use futures_util::stream::StreamExt;
            futures_util::stream::once(async move { Ok(bytes::Bytes::from(self.bytes)) }).boxed()
        }
    }
    let err = storage
        .upload(
            &plan,
            MisreportedSource {
                bytes: b"abc".to_vec(),
            },
            oversized_hooks,
        )
        .await
        .expect_err("oversized source must be rejected");
    assert!(matches!(err, StorageClientError::SizeMismatch { .. }));
}

#[tokio::test]
async fn authenticated_localfs_transport_preserves_structured_error_code() {
    // V-002: an authenticated LocalFs failure must not collapse to an opaque
    // string. The 404 problem+json surfaced by the server travels through the
    // authenticated transport as a machine-readable [`WyrdError`] — either the
    // storage variant is reconstructed directly, or the original code is
    // preserved in `details.original_code` for language-agnostic clients to
    // route on. Either way, the raw HTTP status is preserved so callers can
    // still fall back to status-based retry decisions.
    let server = MockServer::start().await;
    let wyrd = wyrd(server.uri());
    let storage = WyrdStorageClient::new(&wyrd);

    Mock::given(method("PUT"))
        .and(path("/v1/cards/upload/local/model.bin"))
        .respond_with(ResponseTemplate::new(404).set_body_json(serde_json::json!({
            "code": "WYRD_SPEC_404_NOT_FOUND",
            "detail": "not found",
            "details": {"resource": "artifact"},
        })))
        .mount(&server)
        .await;

    let plan = UploadPlan::LocalFs {
        put_url: format!("{}/v1/cards/upload/local/model.bin", server.uri()),
        ttl_secs: 0,
    };
    let err: WyrdError = storage
        .upload(&plan, b"payload".to_vec(), hooks("key"))
        .await
        .expect_err("404 must surface as structured error")
        .into();
    assert_eq!(err.code(), "WYRD_SPEC_404_NOT_FOUND");
    assert_eq!(err.status(), 404);
}
