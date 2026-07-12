use std::time::Duration;
use wyrd_spec::DataTenantId;
use wyrd_spec::storage::{StorageBackendKind, UploadPlan, WireProtocol};
use wyrd_storage::error::{LocalError, StorageError};
use wyrd_storage::{BackendSigner, CompletePayload, LocalSigner, UploadPlanReplayInput};

#[tokio::test]
async fn local_signer_round_trips_single_put_contract() {
    let root = tempfile::tempdir().expect("temp dir");
    let signer = LocalSigner::new(root.path().to_path_buf()).expect("local signer");
    let tenant = DataTenantId::new_v7();
    let path = storage_path(tenant);

    signer
        .write_atomically(std::path::Path::new(&path.full), b"hello wyrd")
        .await
        .expect("write local object");

    let head = signer.head(&path).await.expect("head local object");
    assert_eq!(head.size_bytes, 10);
    assert_eq!(head.sse_marker.as_deref(), Some("none"));

    let plan = signer
        .presign_single_put(&path, head.size_bytes, Duration::from_mins(1))
        .await
        .expect("local upload plan");
    let UploadPlan::SinglePut {
        put_url,
        ttl_secs,
        required_headers,
    } = plan
    else {
        panic!("local signer should use single put");
    };

    assert!(put_url.starts_with("file://"));
    assert_eq!(ttl_secs, 60);
    assert!(required_headers.is_empty());
}

#[cfg_attr(not(feature = "cloud"), allow(irrefutable_let_patterns))]
#[tokio::test]
async fn backend_dispatch_remints_and_finalizes_local_uploads() {
    let root = tempfile::tempdir().expect("temp dir");
    let signer = BackendSigner::Local(LocalSigner::new(root.path().to_path_buf()).expect("signer"));
    let tenant = DataTenantId::new_v7();
    let path = storage_path(tenant);

    let input = UploadPlanReplayInput {
        wire_protocol: WireProtocol::LocalFsV1,
        backend_upload_id: None,
        part_count_planned: 1,
        part_size_bytes: 10,
        block_count_planned: None,
    };
    let plan = signer
        .remint_plan(&path, &input, Duration::from_secs(90))
        .await
        .expect("remint local plan");

    assert!(matches!(plan, UploadPlan::SinglePut { ttl_secs: 90, .. }));

    if let BackendSigner::Local(local) = &signer {
        local
            .write_atomically(std::path::Path::new(&path.full), b"pending")
            .await
            .expect("write pending local object");
    }

    signer
        .complete_server_side(&path, None, CompletePayload::Local)
        .await
        .expect("local finalize");
}

fn storage_path(tenant: DataTenantId) -> wyrd_storage::ValidatedPath {
    let card_uid = uuid::Uuid::now_v7();
    let full = wyrd_storage::tenant_path::build(tenant, &card_uid.to_string(), "matrix/object.bin");
    wyrd_storage::tenant_path::validate(&full, tenant).expect("tenant path")
}

#[tokio::test]
async fn local_head_on_missing_returns_not_found() {
    let root = tempfile::tempdir().expect("temp dir");
    let signer = LocalSigner::new(root.path().to_path_buf()).expect("local signer");
    let tenant = DataTenantId::new_v7();
    let path = storage_path(tenant);

    let result = signer.head(&path).await;
    match result {
        Err(StorageError::Local(LocalError::NotFound { .. })) => {}
        other => panic!("local head on missing must return typed Local::NotFound, got: {other:?}"),
    }
}

#[tokio::test]
async fn local_capability_mismatch_is_typed_for_non_local_protocols() {
    let root = tempfile::tempdir().expect("temp dir");
    let signer = LocalSigner::new(root.path().to_path_buf()).expect("local signer");
    let tenant = DataTenantId::new_v7();
    let path = storage_path(tenant);

    let input = UploadPlanReplayInput {
        wire_protocol: WireProtocol::S3MultipartV1,
        backend_upload_id: None,
        part_count_planned: 1,
        part_size_bytes: 10,
        block_count_planned: None,
    };
    let result = signer
        .remint_plan(&path, &input, Duration::from_mins(1))
        .await;
    match result {
        Err(StorageError::BackendCapabilityMismatch { signer, op }) => {
            assert_eq!(signer, StorageBackendKind::Local);
            assert_eq!(op, "remint_plan");
        }
        other => panic!(
            "Local remint_plan with non-Local protocol must return BackendCapabilityMismatch, \
             got: {other:?}"
        ),
    }
}
