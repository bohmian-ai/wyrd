use wyrd_spec::storage::StorageBackendKind;
use wyrd_storage::plan::{
    DEFAULT_PART_SIZE_BYTES, MAX_OBJECT_SIZE_BYTES, MULTIPART_THRESHOLD_BYTES, PlannedUpload,
    plan_upload,
};

#[test]
fn plan_upload_uses_single_put_below_threshold() {
    let planned = plan_upload(
        MULTIPART_THRESHOLD_BYTES - 1,
        StorageBackendKind::S3,
        MULTIPART_THRESHOLD_BYTES,
    )
    .expect("below threshold plans");

    assert_eq!(planned, PlannedUpload::SinglePut);
}

#[test]
fn plan_upload_uses_multipart_at_threshold() {
    let planned = plan_upload(
        MULTIPART_THRESHOLD_BYTES,
        StorageBackendKind::S3,
        MULTIPART_THRESHOLD_BYTES,
    )
    .expect("threshold plans");

    assert_eq!(
        planned,
        PlannedUpload::Multipart {
            part_count: 7,
            part_size_bytes: DEFAULT_PART_SIZE_BYTES
        }
    );
}

#[test]
fn plan_upload_honors_lowered_threshold() {
    let threshold = 8 * 1024 * 1024;
    let size = 20 * 1024 * 1024;
    let planned =
        plan_upload(size, StorageBackendKind::S3, threshold).expect("lowered threshold plans");

    assert_eq!(
        planned,
        PlannedUpload::Multipart {
            part_count: 2,
            part_size_bytes: DEFAULT_PART_SIZE_BYTES
        },
        "20MiB over an 8MiB threshold splits into two 16MiB-part multipart upload"
    );
}

#[test]
fn plan_upload_keeps_local_single_put() {
    let planned = plan_upload(
        MULTIPART_THRESHOLD_BYTES * 4,
        StorageBackendKind::Local,
        MULTIPART_THRESHOLD_BYTES,
    )
    .expect("local plans");

    assert_eq!(planned, PlannedUpload::SinglePut);
}

#[test]
fn plan_upload_rejects_cross_backend_cap() {
    let err = plan_upload(
        MAX_OBJECT_SIZE_BYTES + 1,
        StorageBackendKind::S3,
        MULTIPART_THRESHOLD_BYTES,
    )
    .expect_err("oversized object should fail");

    assert!(err.to_string().contains("exceeds"));
}
