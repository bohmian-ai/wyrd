use std::collections::HashSet;

use wyrd_spec::error::WyrdError;
use wyrd_spec::error::storage::WyrdStorageError;

#[test]
fn storage_error_codes_statuses_and_titles_are_stable() {
    let cases = storage_errors()
        .into_iter()
        .map(|error| (error.code(), error.status(), error.title()))
        .collect::<Vec<_>>();

    assert_eq!(
        cases,
        vec![
            (
                "WYRD_STORAGE_400_TENANT_PATH_MISMATCH",
                400,
                "Tenant path validation rejected upload init",
            ),
            (
                "WYRD_STORAGE_400_ARTIFACT_TOO_LARGE",
                400,
                "Artifact size exceeds backend capacity",
            ),
            (
                "WYRD_STORAGE_400_SHA256_INVALID",
                400,
                "Invalid SHA-256 in upload init",
            ),
            (
                "WYRD_STORAGE_400_SIZE_INVALID",
                400,
                "Invalid expected_size_bytes",
            ),
            (
                "WYRD_STORAGE_400_SHA256_MISMATCH",
                400,
                "Stored object SHA-256 does not match expected_sha256",
            ),
            (
                "WYRD_STORAGE_400_SIZE_MISMATCH",
                400,
                "Stored object byte length does not match expected_size_bytes",
            ),
            (
                "WYRD_STORAGE_400_INVALID_UPLOAD_ID",
                400,
                "Upload id not recognised",
            ),
            (
                "WYRD_STORAGE_400_INVALID_URI",
                400,
                "Source URI failed to parse",
            ),
            (
                "WYRD_STORAGE_400_TENANT_PREFIX_INVALID",
                400,
                "Source URI tenant prefix is not a UUIDv7",
            ),
            (
                "WYRD_STORAGE_403_UPLOAD_FOREIGN_TENANT",
                403,
                "Upload belongs to a different tenant",
            ),
            (
                "WYRD_STORAGE_404_OBJECT_NOT_FOUND",
                404,
                "Object not found in backend",
            ),
            (
                "WYRD_STORAGE_404_UPLOAD_NOT_FOUND",
                404,
                "Upload id not found",
            ),
            (
                "WYRD_STORAGE_409_UPLOAD_NOT_PENDING",
                409,
                "Cannot complete or abort a terminal upload",
            ),
            (
                "WYRD_STORAGE_409_ENCRYPTION_MISSING",
                409,
                "Server-side encryption verification failed",
            ),
            (
                "WYRD_STORAGE_412_PRECONDITION",
                412,
                "Object changed during download",
            ),
            (
                "WYRD_STORAGE_416_RANGE_NOT_SATISFIABLE",
                416,
                "Source changed mid-download",
            ),
            (
                "WYRD_STORAGE_500_BACKEND",
                500,
                "Backend storage operation failed",
            ),
            (
                "WYRD_STORAGE_500_CONFIG_INVALID",
                500,
                "Storage settings invalid",
            ),
            (
                "WYRD_STORAGE_500_CREDENTIAL_CHAIN",
                500,
                "Backend credential chain failed",
            ),
            (
                "WYRD_STORAGE_500_LIFECYCLE_MISSING",
                500,
                "S3 lifecycle missing required rule",
            ),
            (
                "WYRD_STORAGE_503_PRESIGN_EXPIRED",
                503,
                "Presigned URL expired",
            ),
            (
                "WYRD_STORAGE_503_BACKEND_UNAVAILABLE",
                503,
                "Backend transiently unavailable",
            ),
        ]
    );

    let unique = cases
        .iter()
        .map(|(code, _, _)| *code)
        .collect::<HashSet<_>>();
    assert_eq!(unique.len(), cases.len(), "duplicate storage error code");
}

#[test]
fn storage_error_lifts_into_problem_json_without_losing_code() {
    let error: WyrdError = WyrdStorageError::ObjectNotFound {
        storage_path: "tenant/cards/card/model.bin".to_owned(),
    }
    .into();

    let problem = error.as_problem_json();

    assert_eq!(error.code(), "WYRD_STORAGE_404_OBJECT_NOT_FOUND");
    assert_eq!(error.status(), 404);
    assert_eq!(problem["code"], "WYRD_STORAGE_404_OBJECT_NOT_FOUND");
    assert_eq!(problem["status"], 404);
    assert_eq!(problem["title"], "Object not found in backend");
    assert_eq!(
        problem["detail"],
        "object not found at tenant/cards/card/model.bin"
    );
    assert_eq!(
        problem["details"]["code"], "object_not_found",
        "storage details retain the storage enum discriminant"
    );
}

fn storage_errors() -> Vec<WyrdStorageError> {
    vec![
        WyrdStorageError::TenantPathMismatch { detail: "x".into() },
        WyrdStorageError::ArtifactTooLarge {
            actual: 2,
            limit: 1,
        },
        WyrdStorageError::Sha256Invalid { detail: "x".into() },
        WyrdStorageError::SizeInvalid(0),
        WyrdStorageError::Sha256Mismatch {
            expected: "a".into(),
            actual: "b".into(),
        },
        WyrdStorageError::SizeMismatch {
            expected: 1,
            actual: 2,
        },
        WyrdStorageError::InvalidUploadId { reason: "x".into() },
        WyrdStorageError::InvalidUri { detail: "x".into() },
        WyrdStorageError::TenantPrefixInvalid { detail: "x".into() },
        WyrdStorageError::TenantPathForeign,
        WyrdStorageError::ObjectNotFound {
            storage_path: "x".into(),
        },
        WyrdStorageError::UploadNotFound,
        WyrdStorageError::UploadNotPending,
        WyrdStorageError::EncryptionMissing,
        WyrdStorageError::PreconditionFailed,
        WyrdStorageError::RangeNotSatisfiable,
        WyrdStorageError::Backend { detail: "x".into() },
        WyrdStorageError::ConfigInvalid { detail: "x".into() },
        WyrdStorageError::CredentialChain {
            backend: "s3".into(),
        },
        WyrdStorageError::LifecycleMissing,
        WyrdStorageError::PresignExpired { detail: "x".into() },
        WyrdStorageError::BackendUnavailable { status: 503 },
    ]
}
