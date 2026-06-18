//! S3 (`RustFS`) signer-layer integration tests.
//!
//! Every test gates on `WYRD_STORAGE_INTEGRATION_S3=1`. The `MultipartClient`
//! helper drives reqwest against the `RustFS` emulator directly; no bytes ever
//! transit the `Wyrd` server on these paths.

use base64::Engine;
use sha2::{Digest, Sha256};
use std::time::Duration;
use wyrd_spec::DataTenantId;
use wyrd_spec::storage::{S3CompletedPart, StorageBackendKind, UploadPlan, WireProtocol};
use wyrd_storage::error::{S3Error, StorageError};
use wyrd_storage::s3::S3Signer;
use wyrd_storage::{UploadPlanReplayInput, ValidatedPath};
use wyrd_testing::MultipartClient;

const PART_SIZE: usize = 5 * 1024 * 1024;
const PART_COUNT: u32 = 3;

fn skip_unless_enabled() -> bool {
    if std::env::var("WYRD_STORAGE_INTEGRATION_S3").as_deref() != Ok("1") {
        eprintln!("skipping RustFS integration test; set WYRD_STORAGE_INTEGRATION_S3=1");
        return true;
    }
    false
}

async fn build_rustfs_signer() -> S3Signer {
    use aws_config::BehaviorVersion;
    use aws_sdk_s3::Client;
    use aws_sdk_s3::config::{Builder, Credentials, Region};

    let shared = aws_config::defaults(BehaviorVersion::latest()).load().await;
    let config = Builder::from(&shared)
        .region(Region::new("us-east-1"))
        .endpoint_url("http://localhost:9000")
        .force_path_style(true)
        .credentials_provider(Credentials::new(
            "wyrd-test-key",
            "wyrd-test-secret",
            None,
            None,
            "static",
        ))
        .build();

    S3Signer::new(Client::from_conf(config), "wyrd-storage-test".to_owned())
}

fn fresh_path(suffix: &str) -> ValidatedPath {
    let tenant = DataTenantId::new_v7();
    let card_uid = uuid::Uuid::now_v7();
    let full = wyrd_storage::tenant_path::build(tenant, &card_uid.to_string(), suffix);
    wyrd_storage::tenant_path::validate(&full, tenant).expect("tenant path")
}

fn pattern_part(part_number: u32) -> Vec<u8> {
    let byte = u8::try_from((part_number & 0xff) | 0x40).expect("byte fits u8");
    vec![byte; PART_SIZE]
}

fn assemble_full() -> (Vec<u8>, String) {
    let mut bytes = Vec::with_capacity((PART_COUNT as usize) * PART_SIZE);
    for part in 1..=PART_COUNT {
        bytes.extend_from_slice(&pattern_part(part));
    }
    let sha = base64::engine::general_purpose::STANDARD.encode(Sha256::digest(&bytes));
    (bytes, sha)
}

#[tokio::test]
async fn rustfs_single_put_round_trip() {
    if skip_unless_enabled() {
        return;
    }
    let signer = build_rustfs_signer().await;
    let path = fresh_path("single/object.bin");
    let payload = b"wyrd rustfs single put payload".to_vec();
    let client = MultipartClient::new();

    let plan = signer
        .presign_single_put(&path, payload.len() as u64, Duration::from_mins(5))
        .await
        .expect("presign single put");
    client
        .single_put(&plan, payload.clone())
        .await
        .expect("single put PUT");

    let head = signer.head(&path).await.expect("head after put");
    assert_eq!(head.size_bytes, payload.len() as u64);

    let get_url = signer
        .presign_get(&path, Duration::from_mins(5))
        .await
        .expect("presign get");
    let downloaded = client.download(&get_url).await.expect("download bytes");
    assert_eq!(
        downloaded, payload,
        "round-tripped bytes must match the source"
    );
}

#[tokio::test]
async fn rustfs_multipart_round_trip_with_byte_equality() {
    if skip_unless_enabled() {
        return;
    }
    let signer = build_rustfs_signer().await;
    let path = fresh_path("multi/large.bin");
    let (expected_bytes, expected_sha) = assemble_full();
    let client = MultipartClient::new();

    let init = signer
        .init_multipart(&path, PART_COUNT, PART_SIZE as u64, Duration::from_mins(10))
        .await
        .expect("init multipart");
    assert!(
        matches!(init.plan, UploadPlan::S3Multipart { part_count, .. } if part_count == PART_COUNT)
    );

    let backend_upload_id = init.backend_upload_id.clone();
    let url_minter = |part_number: u32| {
        let signer = &signer;
        let path = &path;
        let upload_id = backend_upload_id.clone();
        async move {
            signer
                .presign_part(path, &upload_id, part_number, Duration::from_mins(10))
                .await
        }
    };
    let parts: Vec<S3CompletedPart> = client
        .s3_multipart(&init.plan, url_minter, pattern_part)
        .await
        .expect("drive s3 multipart");
    assert_eq!(
        u32::try_from(parts.len()).expect("part count fits u32"),
        PART_COUNT
    );

    signer
        .complete_multipart(&path, &init.backend_upload_id, &parts, &expected_sha)
        .await
        .expect("complete multipart");

    let head = signer.head(&path).await.expect("head completed object");
    assert_eq!(head.size_bytes, expected_bytes.len() as u64);

    let get_url = signer
        .presign_get(&path, Duration::from_mins(5))
        .await
        .expect("presign get");
    let downloaded = client.download(&get_url).await.expect("download bytes");
    assert_eq!(
        downloaded.len(),
        expected_bytes.len(),
        "reassembled length must match"
    );
    assert_eq!(
        downloaded, expected_bytes,
        "byte-equality across reassembled multipart object"
    );
}

#[tokio::test]
async fn rustfs_abort_lifecycle() {
    if skip_unless_enabled() {
        return;
    }
    let signer = build_rustfs_signer().await;
    let path = fresh_path("abort/object.bin");
    let client = MultipartClient::new();

    let init = signer
        .init_multipart(&path, PART_COUNT, PART_SIZE as u64, Duration::from_mins(5))
        .await
        .expect("init multipart");
    let part_url = signer
        .presign_part(&path, &init.backend_upload_id, 1, Duration::from_mins(5))
        .await
        .expect("presign part 1");
    let upload = client
        .http()
        .put(&part_url)
        .body(pattern_part(1))
        .send()
        .await
        .expect("part 1 upload");
    assert!(upload.status().is_success(), "part 1 upload must succeed");

    signer
        .abort_multipart(&path, &init.backend_upload_id)
        .await
        .expect("abort multipart");

    let head_result = signer.head(&path).await;
    assert!(
        head_result.is_err(),
        "head on aborted (never-completed) object must error, got: {head_result:?}"
    );

    let dummy_parts = vec![S3CompletedPart {
        part_number: 1,
        e_tag: "\"dummy\"".to_owned(),
    }];
    let complete_after_abort = signer
        .complete_multipart(&path, &init.backend_upload_id, &dummy_parts, "")
        .await;
    assert!(
        complete_after_abort.is_err(),
        "complete after abort must error, got Ok"
    );
}

#[tokio::test]
async fn rustfs_remint_plan_produces_valid_plan() {
    if skip_unless_enabled() {
        return;
    }
    let signer = build_rustfs_signer().await;
    let path = fresh_path("remint/object.bin");
    let client = MultipartClient::new();

    let init = signer
        .init_multipart(&path, PART_COUNT, PART_SIZE as u64, Duration::from_mins(5))
        .await
        .expect("init multipart");
    let input = UploadPlanReplayInput {
        wire_protocol: WireProtocol::S3MultipartV1,
        backend_upload_id: Some(init.backend_upload_id.clone()),
        part_count_planned: PART_COUNT,
        part_size_bytes: PART_SIZE as u64,
        block_count_planned: None,
    };
    let reminted = signer
        .remint_plan(&path, &input, Duration::from_mins(5))
        .await
        .expect("remint S3 plan");
    let UploadPlan::S3Multipart {
        part_count,
        part_size_bytes,
        ..
    } = reminted
    else {
        panic!("expected S3Multipart plan after remint");
    };
    assert_eq!(part_count, PART_COUNT);
    assert_eq!(part_size_bytes, PART_SIZE as u64);

    let part_url = signer
        .presign_part(&path, &init.backend_upload_id, 1, Duration::from_mins(5))
        .await
        .expect("presign part after remint");
    let response = client
        .http()
        .put(&part_url)
        .body(pattern_part(1))
        .send()
        .await
        .expect("post-remint PUT");
    assert!(
        response.status().is_success(),
        "post-remint PUT must succeed: {}",
        response.status()
    );

    signer
        .abort_multipart(&path, &init.backend_upload_id)
        .await
        .expect("cleanup abort");
}

#[tokio::test]
async fn rustfs_head_on_missing_returns_error() {
    if skip_unless_enabled() {
        return;
    }
    let signer = build_rustfs_signer().await;
    let path = fresh_path("never-written.bin");

    let result = signer.head(&path).await;
    match result {
        Err(StorageError::S3(boxed)) => match *boxed {
            S3Error::NoSuchKey { .. } | S3Error::Sdk(_) => {}
            other => panic!("expected NoSuchKey/Sdk S3 error on missing head, got: {other:?}"),
        },
        Err(other) => panic!("expected typed S3 error on missing head, got: {other:?}"),
        Ok(head) => panic!("head on never-written path must error, got Ok({head:?})"),
    }
}

#[tokio::test]
async fn rustfs_capability_mismatch_is_typed_for_non_s3_protocols() {
    if skip_unless_enabled() {
        return;
    }
    let signer = build_rustfs_signer().await;
    let path = fresh_path("capability/object.bin");

    let input = UploadPlanReplayInput {
        wire_protocol: WireProtocol::GcsResumableV1,
        backend_upload_id: None,
        part_count_planned: PART_COUNT,
        part_size_bytes: PART_SIZE as u64,
        block_count_planned: None,
    };
    let result = signer
        .remint_plan(&path, &input, Duration::from_mins(5))
        .await;
    match result {
        Err(StorageError::BackendCapabilityMismatch { signer, op }) => {
            assert_eq!(signer, StorageBackendKind::S3);
            assert_eq!(op, "remint_plan");
        }
        other => panic!(
            "expected BackendCapabilityMismatch when reminting non-S3 protocol on S3 signer, \
             got: {other:?}"
        ),
    }

    let input = UploadPlanReplayInput {
        wire_protocol: WireProtocol::AzureBlockBlobV1,
        backend_upload_id: None,
        part_count_planned: PART_COUNT,
        part_size_bytes: PART_SIZE as u64,
        block_count_planned: Some(PART_COUNT),
    };
    let azure_remint = signer
        .remint_plan(&path, &input, Duration::from_mins(5))
        .await;
    match azure_remint {
        Err(StorageError::BackendCapabilityMismatch { signer, op }) => {
            assert_eq!(signer, StorageBackendKind::S3);
            assert_eq!(op, "remint_plan");
        }
        other => panic!(
            "expected BackendCapabilityMismatch when reminting AzureBlockBlobV1 on S3 signer, \
             got: {other:?}"
        ),
    }
}
