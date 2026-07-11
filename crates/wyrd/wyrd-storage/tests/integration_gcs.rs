//! GCS signer-layer integration tests against `fake-gcs-server`.
//!
//! Every test gates on `WYRD_STORAGE_INTEGRATION_GCS=1`. The `MultipartClient`
//! helper drives reqwest against the emulator's resumable session URI; no bytes
//! transit the Wyrd server on the upload path.
//!
//! `fake-gcs-server` runs as an anonymous bucket — signed URLs (which require a
//! service account key) are NOT exercised here. Single-PUT presigning and
//! `presign_get` are covered by the cloud-credentialed test lane.

use std::time::Duration;
use wyrd_spec::DataTenantId;
use wyrd_spec::storage::{StorageBackendKind, UploadPlan, WireProtocol};
use wyrd_storage::cloud::CloudSigner;
use wyrd_storage::error::{GcsError, StorageError};
use wyrd_storage::factory::gcs::build_emulator_signer;
use wyrd_storage::gcs::GcsSigner;
use wyrd_storage::{BackendSigner, UploadPlanReplayInput, ValidatedPath};
use wyrd_testing::MultipartClient;

const EMULATOR_HOST: &str = "http://localhost:4443";
const EMULATOR_BUCKET: &str = "wyrd-storage-test";
const CHUNK_SIZE: usize = 256 * 1024;
const CHUNK_COUNT: u32 = 3;

fn skip_unless_enabled() -> bool {
    if std::env::var("WYRD_STORAGE_INTEGRATION_GCS").as_deref() != Ok("1") {
        eprintln!("skipping GCS emulator integration test; set WYRD_STORAGE_INTEGRATION_GCS=1");
        return true;
    }
    false
}

fn emulator_host() -> String {
    std::env::var("WYRD_GCS_EMULATOR_HOST").unwrap_or_else(|_| EMULATOR_HOST.to_owned())
}

fn build_signer() -> GcsSigner {
    build_emulator_signer(EMULATOR_BUCKET, &emulator_host()).expect("emulator signer")
}

fn fresh_path(suffix: &str) -> ValidatedPath {
    let tenant = DataTenantId::new_v7();
    let card_uid = uuid::Uuid::now_v7();
    let full = wyrd_storage::tenant_path::build(tenant, &card_uid.to_string(), suffix);
    wyrd_storage::tenant_path::validate(&full, tenant).expect("tenant path")
}

fn pattern_chunk(index: u32) -> Vec<u8> {
    let byte = u8::try_from((index & 0xff) | 0x40).expect("byte fits u8");
    vec![byte; CHUNK_SIZE]
}

fn assemble_full() -> Vec<u8> {
    let mut bytes = Vec::with_capacity((CHUNK_COUNT as usize) * CHUNK_SIZE);
    for chunk_index in 0..CHUNK_COUNT {
        bytes.extend_from_slice(&pattern_chunk(chunk_index));
    }
    bytes
}

fn emulator_download_url(path: &ValidatedPath) -> String {
    let encoded = path.full.replace('/', "%2F");
    format!(
        "{}/storage/v1/b/{}/o/{}?alt=media",
        emulator_host(),
        EMULATOR_BUCKET,
        encoded,
    )
}

#[tokio::test]
async fn gcs_resumable_round_trip_with_byte_equality() {
    if skip_unless_enabled() {
        return;
    }
    let signer = build_signer();
    let path = fresh_path("multi/large.bin");
    let expected_bytes = assemble_full();
    let client = MultipartClient::new();

    let init = signer
        .init_multipart(
            &path,
            CHUNK_COUNT,
            CHUNK_SIZE as u64,
            Duration::from_mins(10),
        )
        .await
        .expect("init resumable");
    assert!(matches!(init.plan, UploadPlan::GcsResumable { .. }));

    let chunks: Vec<Vec<u8>> = (0..CHUNK_COUNT).map(pattern_chunk).collect();
    client
        .gcs_resumable(&init.plan, chunks)
        .await
        .expect("drive resumable upload");

    let head = signer.head(&path).await.expect("head after upload");
    assert_eq!(head.size_bytes, expected_bytes.len() as u64);

    let downloaded = client
        .download(&emulator_download_url(&path))
        .await
        .expect("download bytes");
    assert_eq!(
        downloaded.len(),
        expected_bytes.len(),
        "reassembled length must match"
    );
    assert_eq!(
        downloaded, expected_bytes,
        "byte-equality across reassembled resumable upload"
    );
}

#[tokio::test]
async fn gcs_abort_returns_capability_mismatch() {
    if skip_unless_enabled() {
        return;
    }
    let backend = BackendSigner::Cloud(CloudSigner::Gcs(build_signer()));
    let path = fresh_path("abort/object.bin");

    let result = backend.abort_multipart(&path, "ignored").await;
    match result {
        Err(StorageError::BackendCapabilityMismatch { signer, op }) => {
            assert_eq!(signer, StorageBackendKind::Gcs);
            assert_eq!(op, "abort_multipart");
        }
        other => {
            panic!("GCS abort_multipart must return BackendCapabilityMismatch, got: {other:?}")
        }
    }
}

#[tokio::test]
async fn gcs_remint_plan_produces_valid_plan() {
    if skip_unless_enabled() {
        return;
    }
    let signer = build_signer();
    let path = fresh_path("remint/object.bin");

    let input = UploadPlanReplayInput {
        wire_protocol: WireProtocol::GcsResumableV1,
        backend_upload_id: None,
        part_count_planned: CHUNK_COUNT,
        part_size_bytes: CHUNK_SIZE as u64,
        block_count_planned: None,
    };
    let plan = signer
        .remint_plan(&path, &input, Duration::from_mins(5))
        .await
        .expect("remint GCS plan");
    let UploadPlan::GcsResumable {
        session_uri,
        chunk_size_bytes,
    } = plan
    else {
        panic!("expected GcsResumable plan after remint");
    };
    assert_eq!(chunk_size_bytes, CHUNK_SIZE as u64);
    assert!(
        !session_uri.is_empty(),
        "reminted session URI must not be empty"
    );
}

#[tokio::test]
async fn gcs_head_on_missing_returns_not_found() {
    if skip_unless_enabled() {
        return;
    }
    let signer = build_signer();
    let path = fresh_path("never-written.bin");

    let result = signer.head(&path).await;
    match result {
        Err(StorageError::Gcs(boxed)) => match *boxed {
            GcsError::NotFound { .. } => {}
            other => panic!("expected typed GcsError::NotFound, got: {other:?}"),
        },
        other => panic!("expected typed Gcs error on missing head, got: {other:?}"),
    }
}

#[tokio::test]
async fn gcs_capability_mismatch_is_typed_for_presign_part() {
    if skip_unless_enabled() {
        return;
    }
    let backend = BackendSigner::Cloud(CloudSigner::Gcs(build_signer()));
    let path = fresh_path("capability/object.bin");

    let result = backend
        .presign_part(&path, "ignored", 1, Duration::from_secs(1))
        .await;
    match result {
        Err(StorageError::BackendCapabilityMismatch { signer, op }) => {
            assert_eq!(signer, StorageBackendKind::Gcs);
            assert_eq!(op, "presign_part");
        }
        other => panic!("GCS presign_part must return BackendCapabilityMismatch, got: {other:?}"),
    }

    let input = UploadPlanReplayInput {
        wire_protocol: WireProtocol::S3MultipartV1,
        backend_upload_id: None,
        part_count_planned: CHUNK_COUNT,
        part_size_bytes: CHUNK_SIZE as u64,
        block_count_planned: None,
    };
    let remint_result = build_signer()
        .remint_plan(&path, &input, Duration::from_mins(5))
        .await;
    match remint_result {
        Err(StorageError::BackendCapabilityMismatch { signer, op }) => {
            assert_eq!(signer, StorageBackendKind::Gcs);
            assert_eq!(op, "remint_plan");
        }
        other => panic!(
            "GCS remint_plan with non-GCS protocol must return BackendCapabilityMismatch, \
             got: {other:?}"
        ),
    }
}
