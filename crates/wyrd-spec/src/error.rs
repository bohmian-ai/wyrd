//! Stable Wyrd error hierarchy and error-code helpers.

use std::borrow::Cow;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::error::derive::WyrdError as WyrdErrorMeta;

/// Proc-macro re-export for Wyrd-coded error enums.
pub mod derive {
    pub use wyrd_error_derive::WyrdError;
}

/// Public storage error catalog.
pub mod storage {
    use serde::{Deserialize, Serialize};
    use thiserror::Error;

    use crate::error::derive::WyrdError;

    /// Wire-stable storage errors returned by Wyrd storage surfaces.
    #[derive(Debug, Clone, Error, Serialize, Deserialize, schemars::JsonSchema, WyrdError)]
    #[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
    #[serde(tag = "variant", content = "data", rename_all = "snake_case")]
    pub enum WyrdStorageError {
        /// Tenant-scoped storage path validation failed.
        #[error("tenant path validation rejected upload: {detail}")]
        #[wyrd_error(
            code = "WYRD_STORAGE_400_TENANT_PATH_MISMATCH",
            status = 400,
            title = "Tenant path validation rejected upload init",
            remediation = "Ensure relative_path contains no `..`, no leading `/`, and only [A-Za-z0-9._-] characters."
        )]
        TenantPathMismatch {
            /// Safe validation detail.
            detail: String,
        },
        /// Artifact exceeded backend capacity.
        #[error("artifact size {actual} exceeds backend limit {limit}")]
        #[wyrd_error(
            code = "WYRD_STORAGE_400_ARTIFACT_TOO_LARGE",
            status = 400,
            title = "Artifact size exceeds backend capacity",
            remediation = "Split the artifact into multiple cards or use a backend with higher per-object limits."
        )]
        ArtifactTooLarge {
            /// Requested artifact size.
            actual: u64,
            /// Backend size limit.
            limit: u64,
        },
        /// Expected SHA-256 failed base64 or length validation.
        #[error("expected_sha256 is not a valid base64 SHA-256: {detail}")]
        #[wyrd_error(
            code = "WYRD_STORAGE_400_SHA256_INVALID",
            status = 400,
            title = "Invalid SHA-256 in upload init",
            remediation = "Recompute SHA-256 of the artifact and base64-encode the 32-byte digest."
        )]
        Sha256Invalid {
            /// Safe validation detail.
            detail: String,
        },
        /// Expected byte size failed validation.
        #[error("expected_size_bytes is invalid: {0}")]
        #[wyrd_error(
            code = "WYRD_STORAGE_400_SIZE_INVALID",
            status = 400,
            title = "Invalid expected_size_bytes",
            remediation = "Pass a u64 byte length that matches the actual file size."
        )]
        SizeInvalid(u64),
        /// Stored SHA-256 did not match the expected digest.
        #[error("sha256 mismatch: expected {expected}, computed {actual}")]
        #[wyrd_error(
            code = "WYRD_STORAGE_400_SHA256_MISMATCH",
            status = 400,
            title = "Stored object SHA-256 does not match expected_sha256",
            remediation = "Recompute the SHA-256 of the artifact bytes and retry."
        )]
        Sha256Mismatch {
            /// Expected base64 SHA-256.
            expected: String,
            /// Actual base64 SHA-256.
            actual: String,
        },
        /// Stored byte size did not match the expected size.
        #[error("size mismatch: expected {expected}, stored {actual}")]
        #[wyrd_error(
            code = "WYRD_STORAGE_400_SIZE_MISMATCH",
            status = 400,
            title = "Stored object byte length does not match expected_size_bytes",
            remediation = "Verify the file size you reported in upload_init matches the bytes you uploaded."
        )]
        SizeMismatch {
            /// Expected bytes.
            expected: u64,
            /// Actual bytes.
            actual: u64,
        },
        /// Upload id failed parsing or protocol validation.
        #[error("invalid upload_id: {reason}")]
        #[wyrd_error(
            code = "WYRD_STORAGE_400_INVALID_UPLOAD_ID",
            status = 400,
            title = "Upload id not recognised",
            remediation = "Use the upload_id returned by POST /v1/cards/upload/init."
        )]
        InvalidUploadId {
            /// Safe validation reason.
            reason: String,
        },
        /// Source URI failed backend parsing.
        #[error("invalid source uri: {detail}")]
        #[wyrd_error(
            code = "WYRD_STORAGE_400_INVALID_URI",
            status = 400,
            title = "Source URI failed to parse",
            remediation = "Pass an absolute URI with a scheme and authority recognised by the configured backend."
        )]
        InvalidUri {
            /// Safe validation detail.
            detail: String,
        },
        /// Source URI tenant prefix was not a valid tenant id.
        #[error("source uri tenant prefix invalid: {detail}")]
        #[wyrd_error(
            code = "WYRD_STORAGE_400_TENANT_PREFIX_INVALID",
            status = 400,
            title = "Source URI tenant prefix is not a UUIDv7",
            remediation = "Object URIs must start with the caller's data_tenant_id."
        )]
        TenantPrefixInvalid {
            /// Safe validation detail.
            detail: String,
        },
        /// Upload or object belongs to a different tenant.
        #[error("upload belongs to a different tenant")]
        #[wyrd_error(
            code = "WYRD_STORAGE_403_UPLOAD_FOREIGN_TENANT",
            status = 403,
            title = "Upload belongs to a different tenant",
            remediation = "Authenticate with a token whose data_tenant_id matches the upload's tenant prefix."
        )]
        TenantPathForeign,
        /// Object was not present in the configured backend.
        #[error("object not found at {storage_path}")]
        #[wyrd_error(
            code = "WYRD_STORAGE_404_OBJECT_NOT_FOUND",
            status = 404,
            title = "Object not found in backend",
            remediation = "Verify card_uid and relative_path; re-upload if the object was deleted."
        )]
        ObjectNotFound {
            /// Tenant-scoped storage path.
            storage_path: String,
        },
        /// Upload id was not found.
        #[error("upload not found")]
        #[wyrd_error(
            code = "WYRD_STORAGE_404_UPLOAD_NOT_FOUND",
            status = 404,
            title = "Upload id not found",
            remediation = "Use the upload_id returned by POST /v1/cards/upload/init."
        )]
        UploadNotFound,
        /// Upload was already terminal.
        #[error("upload is not pending")]
        #[wyrd_error(
            code = "WYRD_STORAGE_409_UPLOAD_NOT_PENDING",
            status = 409,
            title = "Cannot complete or abort a terminal upload",
            remediation = "Inspect upload status; only pending uploads can be completed or aborted."
        )]
        UploadNotPending,
        /// Backend encryption verification failed.
        #[error("encryption required but backend HEAD did not advertise SSE")]
        #[wyrd_error(
            code = "WYRD_STORAGE_409_ENCRYPTION_MISSING",
            status = 409,
            title = "Server-side encryption verification failed",
            remediation = "Configure backend server-side encryption and retry."
        )]
        EncryptionMissing,
        /// Object changed during a conditional read.
        #[error("object precondition failed during read")]
        #[wyrd_error(
            code = "WYRD_STORAGE_412_PRECONDITION",
            status = 412,
            title = "Object changed during download",
            remediation = "Delete the partial file and re-issue download init for a fresh URL."
        )]
        PreconditionFailed,
        /// Requested byte range cannot be satisfied.
        #[error("range not satisfiable")]
        #[wyrd_error(
            code = "WYRD_STORAGE_416_RANGE_NOT_SATISFIABLE",
            status = 416,
            title = "Source changed mid-download",
            remediation = "Delete the partial file and sidecar, then re-download."
        )]
        RangeNotSatisfiable,
        /// Backend SDK or IO operation failed.
        #[error("backend storage operation failed: {detail}")]
        #[wyrd_error(
            code = "WYRD_STORAGE_500_BACKEND",
            status = 500,
            title = "Backend storage operation failed",
            remediation = "Retry the operation. If failures persist, check backend dashboard and storage server logs."
        )]
        Backend {
            /// Safe backend detail.
            detail: String,
        },
        /// Storage settings were invalid.
        #[error("storage settings invalid at boot: {detail}")]
        #[wyrd_error(
            code = "WYRD_STORAGE_500_CONFIG_INVALID",
            status = 500,
            title = "Storage settings invalid",
            remediation = "Inspect server startup logs; fix the named env var and restart."
        )]
        ConfigInvalid {
            /// Safe configuration detail.
            detail: String,
        },
        /// Backend credential chain returned no credentials.
        #[error("backend {backend} credential chain returned no credentials")]
        #[wyrd_error(
            code = "WYRD_STORAGE_500_CREDENTIAL_CHAIN",
            status = 500,
            title = "Backend credential chain failed",
            remediation = "Verify backend credentials are reachable in the server environment."
        )]
        CredentialChain {
            /// Backend name.
            backend: String,
        },
        /// Required S3 lifecycle rule was missing.
        #[error("S3 bucket lifecycle missing AbortIncompleteMultipartUpload rule")]
        #[wyrd_error(
            code = "WYRD_STORAGE_500_LIFECYCLE_MISSING",
            status = 500,
            title = "S3 lifecycle missing required rule",
            remediation = "Apply the recommended lifecycle policy for incomplete multipart uploads."
        )]
        LifecycleMissing,
        /// Presigned URL has expired.
        #[error("presigned URL refused as expired: {detail}")]
        #[wyrd_error(
            code = "WYRD_STORAGE_503_PRESIGN_EXPIRED",
            status = 503,
            title = "Presigned URL expired",
            remediation = "Call upload part-url or download init to obtain a fresh URL and retry."
        )]
        PresignExpired {
            /// Safe expiry detail.
            detail: String,
        },
        /// Backend is transiently unavailable.
        #[error("backend transiently unavailable: status {status}")]
        #[wyrd_error(
            code = "WYRD_STORAGE_503_BACKEND_UNAVAILABLE",
            status = 503,
            title = "Backend transiently unavailable",
            remediation = "Retry with exponential backoff."
        )]
        BackendUnavailable {
            /// Backend or synthetic status.
            status: u16,
        },
    }
}

pub use storage::WyrdStorageError;

/// Top-level Wyrd error value passed across crate and wire boundaries.
#[derive(Debug, Clone, Error, Serialize, Deserialize, schemars::JsonSchema, WyrdErrorMeta)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum WyrdError {
    /// Request payload failed validation.
    #[error("[WYRD_SPEC_400_VALIDATION] {message}")]
    #[wyrd_error(
        code = "WYRD_SPEC_400_VALIDATION",
        status = 400,
        title = "Validation failed",
        remediation = "Check the submitted Wyrd request fields against the published schema and retry."
    )]
    Validation {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// Requested entity was not found.
    #[error("[WYRD_SPEC_404_NOT_FOUND] {message}")]
    #[wyrd_error(
        code = "WYRD_SPEC_404_NOT_FOUND",
        status = 404,
        title = "Resource not found",
        remediation = "Check that the referenced Wyrd resource exists."
    )]
    NotFound {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// Write was rejected due to a duplicate key or concurrent change.
    #[error("[WYRD_SPEC_409_CONFLICT] {message}")]
    #[wyrd_error(
        code = "WYRD_SPEC_409_CONFLICT",
        status = 409,
        title = "Conflict",
        remediation = "Refresh the resource and retry with the current version or idempotency key."
    )]
    Conflict {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// Unexpected internal failure.
    #[error("[WYRD_SPEC_500_INTERNAL] {message}")]
    #[wyrd_error(
        code = "WYRD_SPEC_500_INTERNAL",
        status = 500,
        title = "Internal error",
        remediation = "Retry later or inspect server logs using the request ID."
    )]
    Internal {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// Upstream dependency failed.
    #[error("[WYRD_SPEC_502_UPSTREAM_FAILURE] {message}")]
    #[wyrd_error(
        code = "WYRD_SPEC_502_UPSTREAM_FAILURE",
        status = 502,
        title = "Upstream dependency failed",
        remediation = "Check the upstream dependency health and retry policy."
    )]
    UpstreamFailure {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// Operation exceeded its deadline.
    #[error("[WYRD_SPEC_504_TIMEOUT] {message}")]
    #[wyrd_error(
        code = "WYRD_SPEC_504_TIMEOUT",
        status = 504,
        title = "Timeout",
        remediation = "Retry with a longer timeout or reduce the request scope."
    )]
    Timeout {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// Vala eval task graph failed DAG validation (cycle, self-loop, or missing dependency).
    #[error("[WYRD_VALA_400_TASK_DAG_INVALID] {message}")]
    #[wyrd_error(
        code = "WYRD_VALA_400_TASK_DAG_INVALID",
        status = 400,
        title = "Eval task DAG validation failed",
        remediation = "Remove cycles, self-dependencies, or references to missing eval tasks."
    )]
    ValaTaskDagInvalid {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// Vala eval contract referenced a CardRef kind outside its allowlist.
    #[error("[WYRD_VALA_400_EVAL_REF_KIND_MISMATCH] {message}")]
    #[wyrd_error(
        code = "WYRD_VALA_400_EVAL_REF_KIND_MISMATCH",
        status = 400,
        title = "Eval reference kind mismatch",
        remediation = "Use the required CardRef kind for the eval field being validated."
    )]
    ValaEvalRefKindMismatch {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// Caller presented no credential or an unparseable one.
    #[error("[WYRD_AUTH_401_UNAUTHENTICATED] {message}")]
    #[wyrd_error(
        code = "WYRD_AUTH_401_UNAUTHENTICATED",
        status = 401,
        title = "Not authenticated",
        remediation = "Present a valid Wyrd token in the Authorization header."
    )]
    Unauthenticated {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// Token signature and shape were valid but the token is past its expiry.
    #[error("[WYRD_AUTH_401_TOKEN_EXPIRED] {message}")]
    #[wyrd_error(
        code = "WYRD_AUTH_401_TOKEN_EXPIRED",
        status = 401,
        title = "Token expired",
        remediation = "Re-authenticate to obtain a fresh token."
    )]
    TokenExpired {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// Token was malformed, had a bad signature, or failed a claim check.
    #[error("[WYRD_AUTH_401_INVALID_TOKEN] {message}")]
    #[wyrd_error(
        code = "WYRD_AUTH_401_INVALID_TOKEN",
        status = 401,
        title = "Invalid token",
        remediation = "Re-authenticate to obtain a valid token signed by the current issuer key."
    )]
    InvalidToken {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// Authorization header shape is malformed.
    #[error("[WYRD_AUTH_400_BAD_TOKEN_FORMAT] {message}")]
    #[wyrd_error(
        code = "WYRD_AUTH_400_BAD_TOKEN_FORMAT",
        status = 400,
        title = "Authorization header malformed",
        remediation = "Use `Authorization: Bearer <token>` with a single Bearer credential."
    )]
    BadTokenFormat {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// Auth token request used an unsupported grant type.
    #[error("[WYRD_AUTH_400_UNSUPPORTED_GRANT_TYPE] {message}")]
    #[wyrd_error(
        code = "WYRD_AUTH_400_UNSUPPORTED_GRANT_TYPE",
        status = 400,
        title = "Unsupported grant_type",
        remediation = "Use one of the supported grants listed in `details.supported_grant_types`."
    )]
    UnsupportedGrantType {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// OIDC callback state is missing, invalid, or replayed.
    #[error("[WYRD_AUTH_400_INVALID_STATE] {message}")]
    #[wyrd_error(
        code = "WYRD_AUTH_400_INVALID_STATE",
        status = 400,
        title = "Invalid OIDC state",
        remediation = "Restart the login flow. The callback state is single-use and must match the server-stored login state."
    )]
    InvalidState {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// OIDC ID token nonce did not match the stored login nonce.
    #[error("[WYRD_AUTH_400_INVALID_NONCE] {message}")]
    #[wyrd_error(
        code = "WYRD_AUTH_400_INVALID_NONCE",
        status = 400,
        title = "Invalid OIDC nonce",
        remediation = "Restart the login flow. The ID token nonce must match the server-stored login nonce."
    )]
    InvalidNonce {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// Rotated or revoked refresh token was presented and the token family was revoked.
    #[error("[WYRD_AUTH_401_REFRESH_REUSED] {message}")]
    #[wyrd_error(
        code = "WYRD_AUTH_401_REFRESH_REUSED",
        status = 401,
        title = "Refresh token reused",
        remediation = "Re-authenticate. The refresh token family has been revoked because a rotated token was reused."
    )]
    RefreshReused {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// Refresh token has been revoked or is no longer valid.
    #[error("[WYRD_AUTH_401_REFRESH_REVOKED] {message}")]
    #[wyrd_error(
        code = "WYRD_AUTH_401_REFRESH_REVOKED",
        status = 401,
        title = "Refresh token revoked",
        remediation = "Re-authenticate to obtain a new refresh token."
    )]
    RefreshRevoked {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// Token exchange would produce a delegation chain deeper than supported.
    #[error("[WYRD_AUTH_400_DELEGATION_DEPTH_EXCEEDED] {message}")]
    #[wyrd_error(
        code = "WYRD_AUTH_400_DELEGATION_DEPTH_EXCEEDED",
        status = 400,
        title = "Delegation chain exceeds maximum depth",
        remediation = "Reduce the delegation chain; the caller's existing `act` chain plus the requested hop must be at most 5 layers."
    )]
    DelegationDepthExceededIssue {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// CardRef kind cannot back a non-human principal.
    #[error("[WYRD_AUTH_400_PRINCIPAL_KIND_CARD_KIND_MISMATCH] {message}")]
    #[wyrd_error(
        code = "WYRD_AUTH_400_PRINCIPAL_KIND_CARD_KIND_MISMATCH",
        status = 400,
        title = "CardRef cannot be bound to a non-human principal",
        remediation = "Only Service and Agent cards bind to non-human principals. Re-issue the request with a `card_ref` referencing a Service or Agent card."
    )]
    PrincipalKindCardKindMismatch {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// Non-human principal CardRef version was not exact.
    #[error("[WYRD_AUTH_400_INVALID_CARD_REF_VERSION] {message}")]
    #[wyrd_error(
        code = "WYRD_AUTH_400_INVALID_CARD_REF_VERSION",
        status = 400,
        title = "Non-human principal CardRef.version must be an exact Pin",
        remediation = "Re-issue the request with `card_ref.version` set to an exact `Pin`, for example `1.2.3`; version requirements are rejected for card-bound principals."
    )]
    InvalidCardRefVersion {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// Credential (API key or refresh token) has been revoked.
    #[error("[WYRD_AUTH_401_CREDENTIAL_REVOKED] {message}")]
    #[wyrd_error(
        code = "WYRD_AUTH_401_CREDENTIAL_REVOKED",
        status = 401,
        title = "Credential revoked",
        remediation = "The credential was explicitly revoked. Re-authenticate or request a new credential."
    )]
    CredentialRevoked {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// API key credential could not be accepted.
    #[error("[WYRD_AUTH_401_API_KEY_INVALID] {message}")]
    #[wyrd_error(
        code = "WYRD_AUTH_401_API_KEY_INVALID",
        status = 401,
        title = "API key not found, revoked, or hash mismatch",
        remediation = "Re-issue a key with `wyrd auth issue-key <card_ref>` and update the deployment secret."
    )]
    ApiKeyInvalid {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// Card-bound token claim is absent or malformed.
    #[error("[WYRD_AUTH_401_INVALID_CARD_REF] {message}")]
    #[wyrd_error(
        code = "WYRD_AUTH_401_INVALID_CARD_REF",
        status = 401,
        title = "Non-User token card_ref claim absent or malformed",
        remediation = "Re-issue the token via `POST /auth/token`; ensure the bound non-human principal has a valid structured card_ref."
    )]
    InvalidCardRef {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// Card-bound principal graph produced a scope too large for one token.
    #[error("[WYRD_AUTH_413_CARD_SCOPE_TOO_LARGE] {message}")]
    #[wyrd_error(
        code = "WYRD_AUTH_413_CARD_SCOPE_TOO_LARGE",
        status = 413,
        title = "Card scope too large",
        remediation = "Reduce or split the principal's declared card-ref graph so its transitive observation-target scope fits the token size limit, or issue a narrower card-bound principal."
    )]
    CardScopeTooLarge {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// Presented token carries a delegation chain deeper than supported.
    #[error("[WYRD_AUTH_401_DELEGATION_DEPTH_EXCEEDED] {message}")]
    #[wyrd_error(
        code = "WYRD_AUTH_401_DELEGATION_DEPTH_EXCEEDED",
        status = 401,
        title = "Delegation chain in token exceeds maximum depth",
        remediation = "The presented JWT's `act` chain exceeds the verifier's maximum delegation depth. Re-issue from a shorter chain."
    )]
    DelegationDepthExceededVerify {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// Requested delegated principal was not found in the tenant.
    #[error("[WYRD_AUTH_404_PRINCIPAL_NOT_FOUND] {message}")]
    #[wyrd_error(
        code = "WYRD_AUTH_404_PRINCIPAL_NOT_FOUND",
        status = 404,
        title = "Requested principal not found in tenant",
        remediation = "Verify the `requested_subject` is a Service or Agent principal in the current tenant. Delegation to User principals is not supported in this Wyrd version."
    )]
    PrincipalNotFound {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// Tenant-admin create conflicted with an existing row or a live reference.
    #[error("[WYRD_AUTH_409_ADMIN_CONFLICT] {message}")]
    #[wyrd_error(
        code = "WYRD_AUTH_409_ADMIN_CONFLICT",
        status = 409,
        title = "Admin resource already exists or is still referenced",
        remediation = "A trusted issuer or workload binding with this key already exists, or the issuer still has live bindings. Use a distinct key, or delete the bindings first (or with --cascade)."
    )]
    AdminConflict {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// Tenant-admin get/delete targeted a row absent from the tenant.
    #[error("[WYRD_AUTH_404_ADMIN_NOT_FOUND] {message}")]
    #[wyrd_error(
        code = "WYRD_AUTH_404_ADMIN_NOT_FOUND",
        status = 404,
        title = "Admin resource not found in tenant",
        remediation = "Confirm the issuer URL (and subject for bindings) and that the resource exists in the current tenant."
    )]
    AdminNotFound {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// Auth verification backend is transiently unavailable.
    #[error("[WYRD_AUTH_503_VERIFY_UNAVAILABLE] {message}")]
    #[wyrd_error(
        code = "WYRD_AUTH_503_VERIFY_UNAVAILABLE",
        status = 503,
        title = "Auth verify backend unavailable",
        remediation = "Retry with backoff. Do not re-authenticate; this is an infrastructure failure, not a credential failure."
    )]
    AuthVerifyUnavailable {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// OIDC discovery document is unreachable or does not match the configured issuer.
    #[error("[WYRD_AUTH_503_DISCOVERY_UNAVAILABLE] {message}")]
    #[wyrd_error(
        code = "WYRD_AUTH_503_DISCOVERY_UNAVAILABLE",
        status = 503,
        title = "OIDC discovery unavailable",
        remediation = "Retry with backoff. Do not re-authenticate; this is an infrastructure or issuer-discovery failure."
    )]
    DiscoveryUnavailable {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// Auth preview routes are disabled on this deploy.
    #[error("[WYRD_AUTH_503_PREVIEW_DISABLED] {message}")]
    #[wyrd_error(
        code = "WYRD_AUTH_503_PREVIEW_DISABLED",
        status = 503,
        title = "Auth preview disabled",
        remediation = "This Wyrd deploy disables preview auth routes until the Card-Registry principal projection ships. Do not retry."
    )]
    AuthPreviewDisabled {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// Credential issuance audit insert failed.
    #[error("[WYRD_AUDIT_503_UNAVAILABLE] {message}")]
    #[wyrd_error(
        code = "WYRD_AUDIT_503_UNAVAILABLE",
        status = 503,
        title = "Credential audit unavailable",
        remediation = "Credential plaintext cannot be returned without a durable audit row. Inspect existing keys, revoke duplicates, and re-issue after audit is restored."
    )]
    AuditUnavailable {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// Authz check was called without a delegated token.
    #[error("[WYRD_AUTHZ_403_REQUIRES_DELEGATED_TOKEN] {message}")]
    #[wyrd_error(
        code = "WYRD_AUTHZ_403_REQUIRES_DELEGATED_TOKEN",
        status = 403,
        title = "authz check requires a delegated token",
        remediation = "Call `/v1/authz/check` with a delegated Service or Agent token that includes a card_ref and non-empty act chain."
    )]
    AuthzRequiresDelegatedToken {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// Required request field is missing.
    #[error("[WYRD_VALIDATION_400_MISSING_REQUIRED_FIELD] {message}")]
    #[wyrd_error(
        code = "WYRD_VALIDATION_400_MISSING_REQUIRED_FIELD",
        status = 400,
        title = "Required request field is missing",
        remediation = "Include the named field in the request; see the route's OpenAPI schema for the full required set."
    )]
    MissingRequiredField {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// Authz check policy evaluation denied the request.
    #[error("[WYRD_AUTHZ_403_POLICY_DENIED] {message}")]
    #[wyrd_error(
        code = "WYRD_AUTHZ_403_POLICY_DENIED",
        status = 403,
        title = "Policy denied request",
        remediation = "Inspect the policy denial reason and update the calling service, target service, or policy configuration before retrying."
    )]
    PolicyDenied {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// Authentication is required before a permission check can run.
    #[error("[WYRD_PERMISSION_401_UNAUTHENTICATED] {message}")]
    #[wyrd_error(
        code = "WYRD_PERMISSION_401_UNAUTHENTICATED",
        status = 401,
        title = "Authentication required",
        remediation = "Send a valid `Authorization: Bearer <token>` header before invoking permission-protected routes."
    )]
    PermissionUnauthenticated {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// Caller is authenticated but lacks the required RBAC permission.
    #[error("[WYRD_PERMISSION_403_DENIED_RBAC] {message}")]
    #[wyrd_error(
        code = "WYRD_PERMISSION_403_DENIED_RBAC",
        status = 403,
        title = "Permission denied (RBAC)",
        remediation = "Request the required role from a workspace admin."
    )]
    PermissionDeniedRbac {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// Durable role permissions JSONB no longer decodes.
    #[error("[WYRD_PERMISSION_500_ROLE_CORRUPT] {message}")]
    #[wyrd_error(
        code = "WYRD_PERMISSION_500_ROLE_CORRUPT",
        status = 500,
        title = "Role row's permissions JSONB is corrupt",
        remediation = "Inspect the offending row in `wyrd.auth_roles`; restore or rewrite the permissions JSONB with a valid permission array. This is not retryable."
    )]
    RoleCorrupt {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// Builtin role names are immutable.
    #[error("[WYRD_RBAC_409_BUILTIN_ROLE_IMMUTABLE_NAME] {message}")]
    #[wyrd_error(
        code = "WYRD_RBAC_409_BUILTIN_ROLE_IMMUTABLE_NAME",
        status = 409,
        title = "Builtin role names are immutable",
        remediation = "Builtin role names are durable identifiers used by JWT role resolution and re-seeding. Create a replacement role instead of renaming a builtin role."
    )]
    BuiltinRoleImmutableName {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// Metadata query string failed to parse.
    #[error("[WYRD_QUERY_400_INVALID_SYNTAX] {message}")]
    #[wyrd_error(
        code = "WYRD_QUERY_400_INVALID_SYNTAX",
        status = 400,
        title = "Invalid query syntax",
        remediation = "Fix the query string at the reported offset and retry. See the metadata query grammar."
    )]
    QueryInvalidSyntax {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// Query referenced a field or applied an operator the target surface rejects.
    #[error("[WYRD_QUERY_400_INVALID_FIELD] {message}")]
    #[wyrd_error(
        code = "WYRD_QUERY_400_INVALID_FIELD",
        status = 400,
        title = "Invalid query field or operator",
        remediation = "Reference a supported field and use an operator valid for its type. Ordering operators require a numeric or timestamp field."
    )]
    QueryInvalidField {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// Query exceeded the predicate-count or nesting-depth cap.
    #[error("[WYRD_QUERY_400_TOO_COMPLEX] {message}")]
    #[wyrd_error(
        code = "WYRD_QUERY_400_TOO_COMPLEX",
        status = 400,
        title = "Query too complex",
        remediation = "Reduce the number of predicates or the nesting depth and retry."
    )]
    QueryTooComplex {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// Card spec failed type-driven deserialization.
    #[error("[WYRD_REG_400_INVALID_CARD_SPEC] {message}")]
    #[wyrd_error(
        code = "WYRD_REG_400_INVALID_CARD_SPEC",
        status = 400,
        title = "Card spec failed type-driven deserialization",
        remediation = "Verify the kind/spec field combination matches the documented Wyrd v1 schema for that kind."
    )]
    RegistryInvalidCardSpec {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// The version block is invalid for this operation.
    #[error("[WYRD_REG_400_INVALID_VERSION_BLOCK] {message}")]
    #[wyrd_error(
        code = "WYRD_REG_400_INVALID_VERSION_BLOCK",
        status = 400,
        title = "The version block is invalid for this operation",
        remediation = "Check the error message for the specific constraint: Service and Agent cards require an exact semver pin; version components must fit i64; a scoped bump must stay within the authored range."
    )]
    RegistryInvalidVersionBlock {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// Card spec exceeds MAX_SPEC_BYTES (256 KiB).
    #[error("[WYRD_REG_400_SPEC_TOO_LARGE] {message}")]
    #[wyrd_error(
        code = "WYRD_REG_400_SPEC_TOO_LARGE",
        status = 400,
        title = "Card spec exceeds MAX_SPEC_BYTES (256 KiB)",
        remediation = "Reduce the spec size or split into multiple cards. If artifacts are inlined, move them to an Artifact card with object_store backing."
    )]
    RegistrySpecTooLarge {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// `metadata.version` was present but empty.
    #[error("[WYRD_REG_400_VERSION_REQUIRED] {message}")]
    #[wyrd_error(
        code = "WYRD_REG_400_VERSION_REQUIRED",
        status = 400,
        title = "`metadata.version` is present but empty",
        remediation = "Provide a valid semver string (e.g. `\"1.0.0\"`) or omit `metadata.version` entirely to let the server auto-assign the next version."
    )]
    RegistryVersionRequired {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// List `limit` is zero or exceeds the per-tenant cap.
    #[error("[WYRD_REG_400_LIST_LIMIT_OUT_OF_RANGE] {message}")]
    #[wyrd_error(
        code = "WYRD_REG_400_LIST_LIMIT_OUT_OF_RANGE",
        status = 400,
        title = "List `limit` is zero or exceeds the per-tenant cap",
        remediation = "Pass a `limit` in `1..=LIST_LIMIT_MAX` (currently 200)."
    )]
    RegistryListLimitOutOfRange {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// `card_ref.uid` was populated for a registry lookup that resolves by (space, name, version).
    #[error("[WYRD_REG_400_CARD_REF_UID_NOT_RESOLVABLE_HERE] {message}")]
    #[wyrd_error(
        code = "WYRD_REG_400_CARD_REF_UID_NOT_RESOLVABLE_HERE",
        status = 400,
        title = "`card_ref.uid` was populated for a registry lookup that resolves by (space, name, version)",
        remediation = "Submit the request with `card_ref.uid = None`; the registry resolves by identity tuple. Use `get_card_by_uid` if you have a `card_uid`."
    )]
    RegistryCardRefUidNotResolvableHere {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// `card_ref.version` is a Requirement; this endpoint accepts a Pin.
    #[error("[WYRD_REG_400_REQUIREMENT_NOT_RESOLVABLE_HERE] {message}")]
    #[wyrd_error(
        code = "WYRD_REG_400_REQUIREMENT_NOT_RESOLVABLE_HERE",
        status = 400,
        title = "`card_ref.version` is a Requirement; this endpoint accepts a Pin",
        remediation = "Resolve the Requirement to a Pin via the future `resolve_card_ref` endpoint, or pass a Pin directly."
    )]
    RegistryRequirementNotResolvableHere {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// No card matches the supplied identity.
    #[error("[WYRD_REG_404_CARD_NOT_FOUND] {message}")]
    #[wyrd_error(
        code = "WYRD_REG_404_CARD_NOT_FOUND",
        status = 404,
        title = "No card matches the supplied identity",
        remediation = "Check (space, kind, name, version) or the `card_uid` is correct and the card is registered in the current tenant."
    )]
    RegistryCardNotFound {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// Defense-in-depth: an existing card with the same identity has a different uid.
    #[error("[WYRD_REG_500_VERSION_CONFLICT] {message}")]
    #[wyrd_error(
        code = "WYRD_REG_500_VERSION_CONFLICT",
        status = 500,
        title = "Card uid mismatch for same identity",
        remediation = "Internal invariant violation; report with the request id and audit_id."
    )]
    RegistryVersionConflict {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// Re-apply with the same identity but a different spec_hash; same-version cards are immutable.
    #[error("[WYRD_REG_409_SPEC_DRIFT] {message}")]
    #[wyrd_error(
        code = "WYRD_REG_409_SPEC_DRIFT",
        status = 409,
        title = "Re-apply with the same identity but a different spec_hash; same-version cards are immutable",
        remediation = "Bump `metadata.version` to publish a new spec, or revert your spec to match the registered version."
    )]
    RegistrySpecDrift {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// Transient Postgres or RLS misconfiguration; retry with backoff.
    #[error("[WYRD_REG_503_REGISTRY_UNAVAILABLE] {message}")]
    #[wyrd_error(
        code = "WYRD_REG_503_REGISTRY_UNAVAILABLE",
        status = 503,
        title = "Transient Postgres or RLS misconfiguration; retry with backoff",
        remediation = "Retry with exponential backoff (60s cap). If persistent, check Postgres connectivity, the tenant row, and the RLS role bindings."
    )]
    RegistryUnavailable {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// Card registration requires an idempotency key.
    #[error("[WYRD_REGISTRY_400_IDEMPOTENCY_KEY_REQUIRED] {message}")]
    #[wyrd_error(
        code = "WYRD_REGISTRY_400_IDEMPOTENCY_KEY_REQUIRED",
        status = 400,
        title = "Idempotency key required",
        remediation = "Provide a stable Idempotency-Key header and retry the registration."
    )]
    RegistryIdempotencyKeyRequired {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// The supplied artifact manifest hash did not match the server hash.
    #[error("[WYRD_REGISTRY_400_MANIFEST_HASH_MISMATCH] {message}")]
    #[wyrd_error(
        code = "WYRD_REGISTRY_400_MANIFEST_HASH_MISMATCH",
        status = 400,
        title = "Artifact manifest hash mismatch",
        remediation = "Recompute the JCS manifest hash and retry."
    )]
    RegistryManifestHashMismatch {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// An artifact path failed relative-path validation.
    #[error("[WYRD_REGISTRY_400_INVALID_ARTIFACT_PATH] {message}")]
    #[wyrd_error(
        code = "WYRD_REGISTRY_400_INVALID_ARTIFACT_PATH",
        status = 400,
        title = "Invalid artifact path",
        remediation = "Use a relative forward-slash path without empty or parent-traversal segments."
    )]
    RegistryInvalidArtifactPath {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// A local upload was interrupted by the caller.
    #[error("[WYRD_REGISTRY_400_UPLOAD_INTERRUPTED] {message}")]
    #[wyrd_error(
        code = "WYRD_REGISTRY_400_UPLOAD_INTERRUPTED",
        status = 400,
        title = "Upload interrupted",
        remediation = "Retry the upload while the registration operation remains pending."
    )]
    RegistryUploadInterrupted {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// A list cursor does not match the request filters.
    #[error("[WYRD_REGISTRY_400_CURSOR_MISMATCH] {message}")]
    #[wyrd_error(
        code = "WYRD_REGISTRY_400_CURSOR_MISMATCH",
        status = 400,
        title = "Cursor does not match filters",
        remediation = "Discard the cursor and restart pagination with the current filters."
    )]
    RegistryCursorMismatch {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// An idempotency key was reused for different content.
    #[error("[WYRD_REGISTRY_409_IDEMPOTENCY_CONFLICT] {message}")]
    #[wyrd_error(
        code = "WYRD_REGISTRY_409_IDEMPOTENCY_CONFLICT",
        status = 409,
        title = "Idempotency key conflict",
        remediation = "Use the original request with this key or choose a new key for different content."
    )]
    RegistryIdempotencyConflict {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// A pending registration operation has expired.
    #[error("[WYRD_REGISTRY_410_OPERATION_EXPIRED] {message}")]
    #[wyrd_error(
        code = "WYRD_REGISTRY_410_OPERATION_EXPIRED",
        status = 410,
        title = "Registration operation expired",
        remediation = "Start a new registration with a new idempotency key."
    )]
    RegistryOperationExpired {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// Artifact verification failed at finalization.
    #[error("[WYRD_REGISTRY_507_ARTIFACT_VERIFY_FAILED] {message}")]
    #[wyrd_error(
        code = "WYRD_REGISTRY_507_ARTIFACT_VERIFY_FAILED",
        status = 507,
        title = "Artifact verification failed",
        remediation = "Re-upload the artifact and retry finalization."
    )]
    RegistryArtifactVerifyFailed {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// The backing Service or Agent card has been soft-deleted.
    #[error("[WYRD_AUTH_403_PRINCIPAL_ORPHANED] {message}")]
    #[wyrd_error(
        code = "WYRD_AUTH_403_PRINCIPAL_ORPHANED",
        status = 403,
        title = "The backing Service or Agent card has been soft-deleted",
        remediation = "Re-register the card or use a different principal to obtain a token."
    )]
    PrincipalOrphaned {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// Storage subsystem error with a stable storage-specific public code.
    #[error(transparent)]
    #[wyrd_error(delegate)]
    Storage {
        /// Stable storage error catalog value.
        #[from]
        #[serde(flatten)]
        error: WyrdStorageError,
    },
    /// Vala/Bifrost OLAP error with a stable `WYRD_VALA_*` public code.
    #[error(transparent)]
    #[wyrd_error(delegate)]
    Vala {
        /// Stable wire Bifrost error catalog value.
        #[from]
        #[serde(flatten)]
        error: crate::vala::error::BifrostError,
    },
    /// DataCard validation failed.
    #[error("[WYRD_DATA_400_VALIDATION] {message}")]
    #[wyrd_error(
        code = "WYRD_DATA_400_VALIDATION",
        status = 400,
        title = "DataCard validation failed",
        remediation = "Fix the DataCard schema, artifact, stats, or interface metadata."
    )]
    DataValidation {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// Python data input could not be classified as a supported DataCard source.
    #[error("[WYRD_DATA_400_UNKNOWN_DATA_TYPE] {message}")]
    #[wyrd_error(
        code = "WYRD_DATA_400_UNKNOWN_DATA_TYPE",
        status = 400,
        title = "Unknown DataCard data type",
        remediation = "Pass a supported data object, path, or explicit DataInterface."
    )]
    DataUnknownDataType {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// A DataCard split declaration is invalid.
    #[error("[WYRD_DATA_400_INVALID_SPLIT_RULE] {message}")]
    #[wyrd_error(
        code = "WYRD_DATA_400_INVALID_SPLIT_RULE",
        status = 400,
        title = "Invalid DataCard split rule",
        remediation = "Use a supported split operator and reference schema columns that exist."
    )]
    DataInvalidSplitRule {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// A DataCard target column is absent from the declared schema.
    #[error("[WYRD_DATA_400_TARGET_COLUMN_UNKNOWN] {message}")]
    #[wyrd_error(
        code = "WYRD_DATA_400_TARGET_COLUMN_UNKNOWN",
        status = 400,
        title = "Unknown DataCard target column",
        remediation = "Declare target columns that exist in the DataCard schema."
    )]
    DataTargetColumnUnknown {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// A Python DataCard interface option was invalid.
    #[error("[WYRD_DATA_400_INVALID_INTERFACE_OPTION] {message}")]
    #[wyrd_error(
        code = "WYRD_DATA_400_INVALID_INTERFACE_OPTION",
        status = 400,
        title = "Invalid DataCard interface option",
        remediation = "Use one of the supported interface option values."
    )]
    DataInvalidInterfaceOption {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// Interface metadata was required but could not be inferred.
    #[error("[WYRD_DATA_400_INTERFACE_METADATA_REQUIRED] {message}")]
    #[wyrd_error(
        code = "WYRD_DATA_400_INTERFACE_METADATA_REQUIRED",
        status = 400,
        title = "DataCard interface metadata required",
        remediation = "Pass an explicit DataInterface with enough metadata to describe the data source."
    )]
    DataInterfaceMetadataRequired {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// DriftCard validation failed.
    #[error("[WYRD_DRIFT_400_VALIDATION] {message}")]
    #[wyrd_error(
        code = "WYRD_DRIFT_400_VALIDATION",
        status = 400,
        title = "DriftCard validation failed",
        remediation = "Fix the DriftCard envelope, profile, condition, or signal fields."
    )]
    DriftValidation {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// A DriftCard signal variant is not compatible with the requested method.
    #[error("[WYRD_DRIFT_400_SIGNAL_METHOD_MISMATCH] {message}")]
    #[wyrd_error(
        code = "WYRD_DRIFT_400_SIGNAL_METHOD_MISMATCH",
        status = 400,
        title = "DriftCard signal/method mismatch",
        remediation = "Use a signal variant supported by the chosen DriftMethod (see DriftSpec docs)."
    )]
    DriftSignalMethodMismatch {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// A DriftCard method that requires a profile was registered without one.
    #[error("[WYRD_DRIFT_400_PROFILE_REQUIRED] {message}")]
    #[wyrd_error(
        code = "WYRD_DRIFT_400_PROFILE_REQUIRED",
        status = 400,
        title = "DriftCard profile required",
        remediation = "Attach the method-matching DriftProfile (PsiProfile, SpcProfile, or CustomProfile)."
    )]
    DriftProfileRequired {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// Python model input could not be classified as a supported ModelCard interface.
    #[error("[WYRD_MODEL_400_UNKNOWN_MODEL_TYPE] {message}")]
    #[wyrd_error(
        code = "WYRD_MODEL_400_UNKNOWN_MODEL_TYPE",
        status = 400,
        title = "Unknown ModelCard model type",
        remediation = "Pass a supported model object or an explicit ModelInterface."
    )]
    ModelUnknownModelType {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// Source card validation failed.
    #[error("[WYRD_SOURCE_400_VALIDATION] {message}")]
    #[wyrd_error(
        code = "WYRD_SOURCE_400_VALIDATION",
        status = 400,
        title = "Source card validation failed",
        remediation = "Fix Source card connection coordinates or auth env-var names and retry."
    )]
    SourceValidation {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// Card code-origin provenance failed validation.
    #[error("[WYRD_ORIGIN_400_VALIDATION] {message}")]
    #[wyrd_error(
        code = "WYRD_ORIGIN_400_VALIDATION",
        status = 400,
        title = "Card origin validation failed",
        remediation = "Provide a non-empty repo and a 7-40 character lowercase hex commit; omit empty path values."
    )]
    OriginValidation {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// ModelCard validation failed.
    #[error("[WYRD_MODEL_400_VALIDATION] {message}")]
    #[wyrd_error(
        code = "WYRD_MODEL_400_VALIDATION",
        status = 400,
        title = "ModelCard validation failed",
        remediation = "Fix the ModelCard signature, framework version, or interface configuration."
    )]
    ModelValidation {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// A ModelCard signature was missing required fields.
    #[error("[WYRD_MODEL_400_MISSING_SIGNATURE] {message}")]
    #[wyrd_error(
        code = "WYRD_MODEL_400_MISSING_SIGNATURE",
        status = 400,
        title = "Missing model signature",
        remediation = "Pass signature fields explicitly or build a ModelSignature before registration."
    )]
    ModelMissingSignature {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// A ModelCard signature dtype was not canonical.
    #[error("[WYRD_MODEL_400_DTYPE_NORMALIZE_FAILED] {message}")]
    #[wyrd_error(
        code = "WYRD_MODEL_400_DTYPE_NORMALIZE_FAILED",
        status = 400,
        title = "Could not normalize signature dtype",
        remediation = "Provide a FieldSpec with a canonical Arrow logical dtype string."
    )]
    ModelDtypeNormalizeFailed {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// A ModelCard signature shape was invalid.
    #[error("[WYRD_MODEL_400_SHAPE_INVALID] {message}")]
    #[wyrd_error(
        code = "WYRD_MODEL_400_SHAPE_INVALID",
        status = 400,
        title = "Invalid model signature shape",
        remediation = "Use Dim::Fixed(n) with n > 0 or Dim::Dynamic(name) for unknown dimensions."
    )]
    ModelShapeInvalid {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// A Hugging Face revision was invalid.
    #[error("[WYRD_MODEL_400_HF_REVISION_INVALID] {message}")]
    #[wyrd_error(
        code = "WYRD_MODEL_400_HF_REVISION_INVALID",
        status = 400,
        title = "Invalid HuggingFace revision",
        remediation = "Pass a 7-40 character lowercase hex revision from the HuggingFace repo."
    )]
    ModelHfRevisionInvalid {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// A Hugging Face task was missing.
    #[error("[WYRD_MODEL_400_HF_TASK_MISSING] {message}")]
    #[wyrd_error(
        code = "WYRD_MODEL_400_HF_TASK_MISSING",
        status = 400,
        title = "Missing HuggingFace task",
        remediation = "Set HuggingfaceMeta.hf_task before saving."
    )]
    ModelHfTaskMissing {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// A custom loader declaration was invalid.
    #[error("[WYRD_MODEL_400_CUSTOM_LOADER_INVALID] {message}")]
    #[wyrd_error(
        code = "WYRD_MODEL_400_CUSTOM_LOADER_INVALID",
        status = 400,
        title = "Invalid custom loader",
        remediation = "Set CustomMeta.loader_module and CustomMeta.loader_class to non-empty values."
    )]
    ModelCustomLoaderInvalid {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// A ModelCard sample or artifact serializer is unavailable.
    #[error("[WYRD_MODEL_501_SERIALIZER_UNAVAILABLE] {message}")]
    #[wyrd_error(
        code = "WYRD_MODEL_501_SERIALIZER_UNAVAILABLE",
        status = 501,
        title = "Required serializer unavailable",
        remediation = "Install the Wyrd Python extra for the required framework serializer."
    )]
    ModelSerializerUnavailable {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// PromptCard variable name was invalid.
    #[error("[WYRD_PROMPT_400_INVALID_VARIABLE_NAME] {message}")]
    #[wyrd_error(
        code = "WYRD_PROMPT_400_INVALID_VARIABLE_NAME",
        status = 400,
        title = "Invalid variable name",
        remediation = "Variable names must match ^[a-zA-Z_][a-zA-Z0-9_]*$."
    )]
    PromptInvalidVariableName {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// PromptCard declared the same variable more than once.
    #[error("[WYRD_PROMPT_409_DUPLICATE_VARIABLE] {message}")]
    #[wyrd_error(
        code = "WYRD_PROMPT_409_DUPLICATE_VARIABLE",
        status = 409,
        title = "Duplicate variable name",
        remediation = "Each entry in prompt.variables must be unique."
    )]
    PromptDuplicateVariable {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// PromptCard request referenced an undeclared placeholder.
    #[error("[WYRD_PROMPT_422_UNDECLARED_PLACEHOLDER] {message}")]
    #[wyrd_error(
        code = "WYRD_PROMPT_422_UNDECLARED_PLACEHOLDER",
        status = 422,
        title = "Undeclared placeholder in request",
        remediation = "Add the placeholder name to prompt.variables, or remove it from the request."
    )]
    PromptUndeclaredPlaceholder {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// PromptCard declared a variable that the request never references.
    #[error("[WYRD_PROMPT_422_UNREFERENCED_VARIABLE] {message}")]
    #[wyrd_error(
        code = "WYRD_PROMPT_422_UNREFERENCED_VARIABLE",
        status = 422,
        title = "Declared variable is never referenced",
        remediation = "Reference the variable somewhere in the native request, or drop it from prompt.variables."
    )]
    PromptUnreferencedVariable {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// PromptCard request referenced an undeclared media placeholder.
    #[error("[WYRD_PROMPT_422_UNDECLARED_MEDIA_PLACEHOLDER] {message}")]
    #[wyrd_error(
        code = "WYRD_PROMPT_422_UNDECLARED_MEDIA_PLACEHOLDER",
        status = 422,
        title = "Undeclared media placeholder in request",
        remediation = "Add the media placeholder name to prompt.media_variables, or remove it from the request."
    )]
    PromptUndeclaredMediaPlaceholder {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// PromptCard declared a media variable that the request never references.
    #[error("[WYRD_PROMPT_422_UNREFERENCED_MEDIA_VARIABLE] {message}")]
    #[wyrd_error(
        code = "WYRD_PROMPT_422_UNREFERENCED_MEDIA_VARIABLE",
        status = 422,
        title = "Declared media variable is never referenced",
        remediation = "Reference the media variable as ${media:name}, or drop it from prompt.media_variables."
    )]
    PromptUnreferencedMediaVariable {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// Media placeholder was not isolated in its own text part.
    #[error("[WYRD_PROMPT_422_MEDIA_PLACEHOLDER_NOT_ISOLATED] {message}")]
    #[wyrd_error(
        code = "WYRD_PROMPT_422_MEDIA_PLACEHOLDER_NOT_ISOLATED",
        status = 422,
        title = "Media placeholder is not isolated",
        remediation = "Ensure each ${media:name} token occupies its own native text part before binding."
    )]
    PromptMediaPlaceholderNotIsolated {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// Media placeholder appeared in system content.
    #[error("[WYRD_PROMPT_400_MEDIA_IN_SYSTEM_MESSAGE] {message}")]
    #[wyrd_error(
        code = "WYRD_PROMPT_400_MEDIA_IN_SYSTEM_MESSAGE",
        status = 400,
        title = "Media placeholder in system message",
        remediation = "Move media placeholders to user or assistant content."
    )]
    PromptMediaInSystemMessage {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// Provider does not support this media kind/source.
    #[error("[WYRD_PROMPT_400_UNSUPPORTED_MEDIA_FOR_PROVIDER] {message}")]
    #[wyrd_error(
        code = "WYRD_PROMPT_400_UNSUPPORTED_MEDIA_FOR_PROVIDER",
        status = 400,
        title = "Unsupported media for provider",
        remediation = "Use a media source supported by the selected provider."
    )]
    PromptUnsupportedMediaForProvider {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// Media MIME type or URI was invalid.
    #[error("[WYRD_PROMPT_400_INVALID_MEDIA_TYPE] {message}")]
    #[wyrd_error(
        code = "WYRD_PROMPT_400_INVALID_MEDIA_TYPE",
        status = 400,
        title = "Invalid media type",
        remediation = "Supply the required MIME type and a provider-supported URI."
    )]
    PromptInvalidMediaType {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// Prompt render call omitted a declared media variable.
    #[error("[WYRD_PROMPT_422_MISSING_MEDIA_VARIABLE] {message}")]
    #[wyrd_error(
        code = "WYRD_PROMPT_422_MISSING_MEDIA_VARIABLE",
        status = 422,
        title = "Media variable missing",
        remediation = "Bind every declared media variable before rendering the prompt."
    )]
    PromptMissingMediaVariable {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// Media path was not a regular file.
    #[error("[WYRD_PROMPT_400_MEDIA_NOT_REGULAR_FILE] {message}")]
    #[wyrd_error(
        code = "WYRD_PROMPT_400_MEDIA_NOT_REGULAR_FILE",
        status = 400,
        title = "Media path is not a regular file",
        remediation = "Pass a regular local file path."
    )]
    PromptMediaNotRegularFile {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// Media file exceeded the allowed byte limit.
    #[error("[WYRD_PROMPT_400_MEDIA_TOO_LARGE] {message}")]
    #[wyrd_error(
        code = "WYRD_PROMPT_400_MEDIA_TOO_LARGE",
        status = 400,
        title = "Media file too large",
        remediation = "Use a smaller media file or upload it to the provider and bind a file URI."
    )]
    PromptMediaTooLarge {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// Media file extension could not be mapped to a supported MIME type.
    #[error("[WYRD_PROMPT_400_MEDIA_INVALID_EXTENSION] {message}")]
    #[wyrd_error(
        code = "WYRD_PROMPT_400_MEDIA_INVALID_EXTENSION",
        status = 400,
        title = "Media file extension is unsupported",
        remediation = "Use a supported image or document extension."
    )]
    PromptMediaInvalidExtension {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// Media path helper IO failed.
    #[error("[WYRD_PROMPT_500_MEDIA_IO] {message}")]
    #[wyrd_error(
        code = "WYRD_PROMPT_500_MEDIA_IO",
        status = 500,
        title = "Media IO failure",
        remediation = "Inspect the OS error; usually missing path or permission denied."
    )]
    PromptMediaIo {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// PromptCard model string was empty.
    #[error("[WYRD_PROMPT_400_EMPTY_MODEL] {message}")]
    #[wyrd_error(
        code = "WYRD_PROMPT_400_EMPTY_MODEL",
        status = 400,
        title = "Prompt model is empty",
        remediation = "Set prompt.model to a non-empty provider model id."
    )]
    PromptEmptyModel {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// PromptCard JSON Schema response type was not an object schema.
    #[error("[WYRD_PROMPT_400_INVALID_RESPONSE_SCHEMA] {message}")]
    #[wyrd_error(
        code = "WYRD_PROMPT_400_INVALID_RESPONSE_SCHEMA",
        status = 400,
        title = "JsonSchema response_type schema is not an object",
        remediation = "The schema of ResponseType::JsonSchema must be a JSON object."
    )]
    PromptInvalidResponseSchema {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// Prompt output schema helper received an invalid shape.
    #[error("[WYRD_PROMPT_422_INVALID_OUTPUT_SCHEMA] {message}")]
    #[wyrd_error(
        code = "WYRD_PROMPT_422_INVALID_OUTPUT_SCHEMA",
        status = 422,
        title = "Invalid output schema",
        remediation = "Pass a dict[str, type], raw JSON Schema object, ResponseFormat, or pydantic BaseModel subclass."
    )]
    PromptInvalidOutputSchema {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// Prompt output schema helper needs pydantic for a class input.
    #[error("[WYRD_PROMPT_422_PYDANTIC_REQUIRED] {message}")]
    #[wyrd_error(
        code = "WYRD_PROMPT_422_PYDANTIC_REQUIRED",
        status = 422,
        title = "Pydantic required",
        remediation = "Install pydantic or use the stdlib dict[str, type] output schema path."
    )]
    PromptPydanticRequired {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// PromptCard codec extension was unsupported.
    #[error("[WYRD_PROMPT_400_LOADER_BAD_EXTENSION] {message}")]
    #[wyrd_error(
        code = "WYRD_PROMPT_400_LOADER_BAD_EXTENSION",
        status = 400,
        title = "Loader extension not supported",
        remediation = "Use .json, .yaml, or .yml."
    )]
    PromptLoaderBadExtension {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// PromptCard loader IO failed in an IO-owning crate.
    #[error("[WYRD_PROMPT_500_LOADER_IO] {message}")]
    #[wyrd_error(
        code = "WYRD_PROMPT_500_LOADER_IO",
        status = 500,
        title = "Loader IO failure",
        remediation = "Inspect the OS error; usually missing path or permission denied."
    )]
    PromptLoaderIo {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// PromptCard request serialization failed during validation.
    #[error("[WYRD_PROMPT_500_SERIALIZE_REQUEST] {message}")]
    #[wyrd_error(
        code = "WYRD_PROMPT_500_SERIALIZE_REQUEST",
        status = 500,
        title = "Serialize native request failed during validation",
        remediation = "The native request could not be serialized to JSON. Capture the spec and file an issue."
    )]
    PromptSerializeRequest {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// Provider authentication failed at a prompt boundary.
    #[error("[WYRD_PROMPT_401_PROVIDER_AUTH] {message}")]
    #[wyrd_error(
        code = "WYRD_PROMPT_401_PROVIDER_AUTH",
        status = 401,
        title = "Provider authentication failed",
        remediation = "Check the provider credentials environment variables."
    )]
    PromptProviderAuth {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// Provider rate-limited a prompt request.
    #[error("[WYRD_PROMPT_429_PROVIDER_RATE_LIMIT] {message}")]
    #[wyrd_error(
        code = "WYRD_PROMPT_429_PROVIDER_RATE_LIMIT",
        status = 429,
        title = "Provider rate limit",
        remediation = "Back off, lower concurrency, or upgrade provider tier."
    )]
    PromptProviderRateLimit {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// Provider returned an upstream failure for a prompt request.
    #[error("[WYRD_PROMPT_502_PROVIDER_UPSTREAM] {message}")]
    #[wyrd_error(
        code = "WYRD_PROMPT_502_PROVIDER_UPSTREAM",
        status = 502,
        title = "Provider upstream error",
        remediation = "Retry with backoff; provider returned a 5xx."
    )]
    PromptProviderUpstream {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// Provider prompt request timed out.
    #[error("[WYRD_PROMPT_504_PROVIDER_TIMEOUT] {message}")]
    #[wyrd_error(
        code = "WYRD_PROMPT_504_PROVIDER_TIMEOUT",
        status = 504,
        title = "Provider request timed out",
        remediation = "Retry; raise the per-request timeout if this is chronic."
    )]
    PromptProviderTimeout {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// Prompt request variant did not match the dialed provider.
    #[error("[WYRD_PROMPT_400_PROVIDER_MISMATCH] {message}")]
    #[wyrd_error(
        code = "WYRD_PROMPT_400_PROVIDER_MISMATCH",
        status = 400,
        title = "Request shape did not match the dialed provider",
        remediation = "Pick the matching provider client or dispatch through the runtime registry."
    )]
    PromptProviderMismatch {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// Declarative prompt draft failed to compile into a native prompt.
    #[error("[WYRD_PROMPT_400_DRAFT_INVALID] {message}")]
    #[wyrd_error(
        code = "WYRD_PROMPT_400_DRAFT_INVALID",
        status = 400,
        title = "Declarative prompt draft compile failed",
        remediation = "Check the provider, model, messages, and model_settings fields in your prompt YAML."
    )]
    PromptDraftInvalid {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// Prompt model_settings object did not match the selected provider.
    #[error("[WYRD_PROMPT_400_SETTINGS_PROVIDER_MISMATCH] {message}")]
    #[wyrd_error(
        code = "WYRD_PROMPT_400_SETTINGS_PROVIDER_MISMATCH",
        status = 400,
        title = "model_settings provider mismatch",
        remediation = "Pass the settings type for this provider, or pass a dict."
    )]
    PromptSettingsProviderMismatch {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// Prompt model_settings value failed to decode.
    #[error("[WYRD_PROMPT_400_SETTINGS_DECODE] {message}")]
    #[wyrd_error(
        code = "WYRD_PROMPT_400_SETTINGS_DECODE",
        status = 400,
        title = "model_settings decode failed",
        remediation = "Check model_settings field names and types; unknown fields are allowed and pass through."
    )]
    PromptSettingsDecode {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// Prompt render call omitted a declared variable.
    #[error("[WYRD_PROMPT_422_MISSING_VARIABLE] {message}")]
    #[wyrd_error(
        code = "WYRD_PROMPT_422_MISSING_VARIABLE",
        status = 422,
        title = "Render variable missing",
        remediation = "Supply every declared variable when calling Prompt::render."
    )]
    PromptMissingVariable {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// Prompt message handoff conversion is unsupported.
    #[error("[WYRD_PROMPT_501_UNSUPPORTED_HANDOFF] {message}")]
    #[wyrd_error(
        code = "WYRD_PROMPT_501_UNSUPPORTED_HANDOFF",
        status = 501,
        title = "Unsupported message conversion",
        remediation = "Use a same-provider handoff or convert manually."
    )]
    PromptUnsupportedHandoff {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// Prompt runtime response did not satisfy the response schema.
    #[error("[WYRD_PROMPT_422_RESPONSE_DECODE] {message}")]
    #[wyrd_error(
        code = "WYRD_PROMPT_422_RESPONSE_DECODE",
        status = 422,
        title = "Response did not match requested schema",
        remediation = "Inspect the provider response or relax the prompt response schema."
    )]
    PromptResponseDecode {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// AgentCard validation failed.
    #[error("[WYRD_AGENT_422_VALIDATION] {message}")]
    #[wyrd_error(
        code = "WYRD_AGENT_422_VALIDATION",
        status = 422,
        title = "AgentCard validation failed",
        remediation = "Fix the Agent Card envelope, metadata, prompt reference, tools, or run_config fields."
    )]
    AgentValidation {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// AgentCard save or envelope projection is missing a name.
    #[error("[WYRD_AGENT_422_MISSING_NAME] {message}")]
    #[wyrd_error(
        code = "WYRD_AGENT_422_MISSING_NAME",
        status = 422,
        title = "AgentCard name is missing",
        remediation = "Set a card name before saving or projecting the agent to a card envelope."
    )]
    AgentMissingName {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// AgentCard save or envelope projection is missing a version.
    #[error("[WYRD_AGENT_422_MISSING_VERSION] {message}")]
    #[wyrd_error(
        code = "WYRD_AGENT_422_MISSING_VERSION",
        status = 422,
        title = "AgentCard version is missing",
        remediation = "Set a concrete semantic version before saving or projecting the agent to a card envelope."
    )]
    AgentMissingVersion {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// Referenced Prompt Card was not available in the local prompt registry.
    #[error("[WYRD_AGENT_404_PROMPT_CARD] {message}")]
    #[wyrd_error(
        code = "WYRD_AGENT_404_PROMPT_CARD",
        status = 404,
        title = "Prompt Card not found",
        remediation = "Load or register the referenced Prompt Card in the local prompt registry before resolving the agent."
    )]
    AgentPromptCardNotFound {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// Runtime-local tool name was not found.
    #[error("[WYRD_AGENT_404_RUNTIME_LOCAL_TOOL_NOT_FOUND] {message}")]
    #[wyrd_error(
        code = "WYRD_AGENT_404_RUNTIME_LOCAL_TOOL_NOT_FOUND",
        status = 404,
        title = "Runtime-local tool not found",
        remediation = "Register the named tool in the local Skald tool registry before loading the Agent Card."
    )]
    AgentRuntimeLocalToolNotFound {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// Runtime-local tool names cannot be durably registered yet.
    #[error("[WYRD_AGENT_422_RUNTIME_LOCAL_TOOLS_NOT_REGISTRABLE] {message}")]
    #[wyrd_error(
        code = "WYRD_AGENT_422_RUNTIME_LOCAL_TOOLS_NOT_REGISTRABLE",
        status = 422,
        title = "Runtime-local tools are not registrable",
        remediation = "Remove runtime-local tool names before registration; local save, load, and run remain available."
    )]
    AgentRuntimeLocalToolsNotRegistrable {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// Python callback returned a value that could not replace the target.
    #[error("[WYRD_AGENT_422_CALLBACK_RETURN_TYPE] {message}")]
    #[wyrd_error(
        code = "WYRD_AGENT_422_CALLBACK_RETURN_TYPE",
        status = 422,
        title = "Callback returned wrong type",
        remediation = "Return None to continue, return a replacement value of the expected type, or raise an exception to abort the run."
    )]
    AgentCallbackReturnType {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// Agent callback aborted the run.
    #[error("[WYRD_AGENT_499_CALLBACK_ABORTED] {message}")]
    #[wyrd_error(
        code = "WYRD_AGENT_499_CALLBACK_ABORTED",
        status = 499,
        title = "Callback aborted run",
        remediation = "Inspect the callback exception and either return None to continue or return a valid replacement value."
    )]
    AgentCallbackAborted {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// Agent loop history contains a provider message type mismatch.
    #[error("[WYRD_AGENT_422_LOOP_MESSAGE_TYPE] {message}")]
    #[wyrd_error(
        code = "WYRD_AGENT_422_LOOP_MESSAGE_TYPE",
        status = 422,
        title = "Loop message type mismatch",
        remediation = "Re-render the conversation message against the active provider; loop messages must match the provider's message schema (OpenAi / Anthropic / Gemini / Custom)."
    )]
    AgentLoopMessageType {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// WorkflowCard validation failed.
    #[error("[WYRD_WORKFLOW_422_VALIDATION] {message}")]
    #[wyrd_error(
        code = "WYRD_WORKFLOW_422_VALIDATION",
        status = 422,
        title = "WorkflowCard validation failed",
        remediation = "Fix the Workflow Card envelope, metadata, steps, inputs, or outputs fields."
    )]
    WorkflowValidation {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// WorkflowCard save or envelope projection is missing a name.
    #[error("[WYRD_WORKFLOW_422_MISSING_NAME] {message}")]
    #[wyrd_error(
        code = "WYRD_WORKFLOW_422_MISSING_NAME",
        status = 422,
        title = "WorkflowCard name is missing",
        remediation = "Set a card name before saving or projecting the workflow to a card envelope."
    )]
    WorkflowMissingName {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// WorkflowCard save or envelope projection is missing a version.
    #[error("[WYRD_WORKFLOW_422_MISSING_VERSION] {message}")]
    #[wyrd_error(
        code = "WYRD_WORKFLOW_422_MISSING_VERSION",
        status = 422,
        title = "WorkflowCard version is missing",
        remediation = "Set a concrete semantic version before saving or projecting the workflow to a card envelope."
    )]
    WorkflowMissingVersion {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// Workflow DAG contains two steps that share the same id.
    #[error("[WYRD_WORKFLOW_422_DUPLICATE_STEP_ID] {message}")]
    #[wyrd_error(
        code = "WYRD_WORKFLOW_422_DUPLICATE_STEP_ID",
        status = 422,
        title = "Workflow contains a duplicate step id",
        remediation = "Give each workflow step a unique id."
    )]
    WorkflowDuplicateStepId {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// Workflow step depends on an id that no step defines.
    #[error("[WYRD_WORKFLOW_422_MISSING_DEPENDENCY] {message}")]
    #[wyrd_error(
        code = "WYRD_WORKFLOW_422_MISSING_DEPENDENCY",
        status = 422,
        title = "Workflow step depends on an unknown id",
        remediation = "Ensure every depends_on entry matches the id of a step defined in the workflow."
    )]
    WorkflowMissingDependency {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// Workflow DAG contains a dependency cycle.
    #[error("[WYRD_WORKFLOW_422_CYCLE] {message}")]
    #[wyrd_error(
        code = "WYRD_WORKFLOW_422_CYCLE",
        status = 422,
        title = "Workflow contains a dependency cycle",
        remediation = "Break the dependency cycle by removing or reordering depends_on edges."
    )]
    WorkflowCycle {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// Server is not ready to serve requests.
    #[error("[WYRD_SERVER_503_NOT_READY] {message}")]
    #[wyrd_error(
        code = "WYRD_SERVER_503_NOT_READY",
        status = 503,
        title = "Server not ready",
        remediation = "Wait for readiness probes to recover; check the `details.checks` object for the failing component and reason code."
    )]
    ServerNotReady {
        /// Human-readable error message.
        message: String,
        /// Structured probe detail payload.
        details: serde_json::Value,
    },
    /// Server is shedding load to protect in-flight requests.
    #[error("[WYRD_SERVER_503_SERVICE_UNAVAILABLE] {message}")]
    #[wyrd_error(
        code = "WYRD_SERVER_503_SERVICE_UNAVAILABLE",
        status = 503,
        title = "Service unavailable",
        remediation = "Retry with exponential backoff; the server is shedding load to protect inflight requests."
    )]
    ServiceUnavailable {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// Server's per-request timeout was exceeded.
    #[error("[WYRD_SERVER_504_REQUEST_TIMEOUT] {message}")]
    #[wyrd_error(
        code = "WYRD_SERVER_504_REQUEST_TIMEOUT",
        status = 504,
        title = "Request timeout",
        remediation = "Reduce the request size or split the operation; the server's per-request timeout was exceeded."
    )]
    RequestTimeout {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// Request body exceeds the configured size limit.
    #[error("[WYRD_SPEC_413_PAYLOAD_TOO_LARGE] {message}")]
    #[wyrd_error(
        code = "WYRD_SPEC_413_PAYLOAD_TOO_LARGE",
        status = 413,
        title = "Payload too large",
        remediation = "Reduce the request body size and retry."
    )]
    PayloadTooLarge {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// Test harness failed to start.
    #[error("[WYRD_TESTING_500_HARNESS_START] {message}")]
    #[wyrd_error(
        code = "WYRD_TESTING_500_HARNESS_START",
        status = 500,
        title = "Test harness failed to start",
        remediation = "Check the test fixture logs"
    )]
    HarnessStart {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// Test harness failed to bind listener.
    #[error("[WYRD_TESTING_500_HARNESS_BOUND] {message}")]
    #[wyrd_error(
        code = "WYRD_TESTING_500_HARNESS_BOUND",
        status = 500,
        title = "Test harness failed to bind listener",
        remediation = "Check the port availability"
    )]
    HarnessBound {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// Test harness failed to bootstrap default principal.
    #[error("[WYRD_TESTING_500_HARNESS_BOOTSTRAP] {message}")]
    #[wyrd_error(
        code = "WYRD_TESTING_500_HARNESS_BOOTSTRAP",
        status = 500,
        title = "Test harness failed to bootstrap default principal",
        remediation = "Check the role configuration"
    )]
    HarnessBootstrap {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },

    // --- WYRD_CFG_* — workspace config (wyrd.toml) ---
    /// `wyrd.toml` failed TOML syntax parsing or file IO.
    ///
    /// `details` JSON: `{ "path": <absolute path or "<cwd>">, "source":
    /// "toml" | "io" }`.
    #[error("[WYRD_CFG_400_INVALID_TOML] {message}")]
    #[wyrd_error(
        code = "WYRD_CFG_400_INVALID_TOML",
        status = 400,
        title = "wyrd.toml failed TOML syntax parse",
        remediation = "Check the TOML syntax in your wyrd.toml file, or the file path if the read itself failed."
    )]
    CfgInvalidToml {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload (`{ path, source }`).
        details: serde_json::Value,
    },

    /// `wyrd.toml` deserialized but failed the typed schema (unknown
    /// field, invalid validated newtype value, lowercase `[kind.x]`,
    /// unknown `CardKind`, etc.).
    ///
    /// `details` JSON: `{ "path": <abs>, "serde_message": <raw> }`.
    #[error("[WYRD_CFG_400_SCHEMA_MISMATCH] {message}")]
    #[wyrd_error(
        code = "WYRD_CFG_400_SCHEMA_MISMATCH",
        status = 400,
        title = "wyrd.toml schema mismatch",
        remediation = "Compare the file against the documented wyrd.toml schema in architecture/wyrd-design.md."
    )]
    CfgSchemaMismatch {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload (`{ path, serde_message }`).
        details: serde_json::Value,
    },

    /// `name` set under `[defaults]` or any `[kind.<X>]` table.
    ///
    /// `details` JSON: `{ "table": "[defaults]" | "[kind.Model]" | … }`.
    #[error("[WYRD_CFG_400_NAME_DEFAULT_REJECTED] {message}")]
    #[wyrd_error(
        code = "WYRD_CFG_400_NAME_DEFAULT_REJECTED",
        status = 400,
        title = "wyrd.toml cannot default `name`",
        remediation = "Remove the `name` key from [defaults] or [kind.<X>]. Every card must author its own name."
    )]
    CfgNameDefaultRejected {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload (`{ table }`).
        details: serde_json::Value,
    },

    // --- WYRD_EVAL_* — eval pull-protocol ---
    /// Eval run id not found in the in-memory run map.
    #[error("[WYRD_EVAL_404_RUN_NOT_FOUND] {message}")]
    #[wyrd_error(
        code = "WYRD_EVAL_404_RUN_NOT_FOUND",
        status = 404,
        title = "Eval run not found",
        remediation = "Re-open a run with POST /v1/eval/runs."
    )]
    EvalRunNotFound {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// Request lacked the bearer lease token on a protected eval-run route.
    #[error("[WYRD_EVAL_401_MISSING_LEASE] {message}")]
    #[wyrd_error(
        code = "WYRD_EVAL_401_MISSING_LEASE",
        status = 401,
        title = "Missing lease token",
        remediation = "Send the lease_token from EvalRunOpenResponse in the Authorization: Bearer <token> header."
    )]
    EvalMissingLease {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// Request carried a lease that does not match the run.
    #[error("[WYRD_EVAL_403_INVALID_LEASE] {message}")]
    #[wyrd_error(
        code = "WYRD_EVAL_403_INVALID_LEASE",
        status = 403,
        title = "Invalid lease token",
        remediation = "Re-open the run via POST /v1/eval/runs; leases are bound to one run."
    )]
    EvalInvalidLease {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// Submission did not match the outstanding directive.
    #[error("[WYRD_EVAL_409_SUBMISSION_MISMATCH] {message}")]
    #[wyrd_error(
        code = "WYRD_EVAL_409_SUBMISSION_MISMATCH",
        status = 409,
        title = "Eval submission did not match outstanding directive",
        remediation = "Call POST /v1/eval/runs/{run_id}/next to retrieve the outstanding directive and retry."
    )]
    EvalSubmissionMismatch {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// Engine, simulator, scenario loading, or server configuration failure.
    #[error("[WYRD_EVAL_500_RUN_FAILED] {message}")]
    #[wyrd_error(
        code = "WYRD_EVAL_500_RUN_FAILED",
        status = 500,
        title = "Eval run failed",
        remediation = "Inspect the eval route logs and retry after correcting the underlying server or provider issue."
    )]
    EvalRunFailed {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
    /// Too many concurrent eval runs; client must retry after existing runs complete.
    #[error("[WYRD_EVAL_429_TOO_MANY_RUNS] {message}")]
    #[wyrd_error(
        code = "WYRD_EVAL_429_TOO_MANY_RUNS",
        status = 429,
        title = "Too many concurrent eval runs",
        remediation = "Wait for an existing run to complete, then retry."
    )]
    EvalTooManyRuns {
        /// Human-readable error message.
        message: String,
        /// Structured detail payload.
        details: serde_json::Value,
    },
}

impl WyrdError {
    /// RFC 9457 JSON problem payload.
    #[must_use]
    pub fn as_problem_json(&self) -> serde_json::Value {
        let (message, details) = self.message_details();
        serde_json::json!({
            "type": format!("https://wyrd.dev/problems/{}", self.code()),
            "title": self.title(),
            "status": self.status(),
            "detail": message,
            "code": self.code(),
            "details": details,
            "remediation": self.remediation(),
        })
    }

    fn message_details(&self) -> (Cow<'_, str>, serde_json::Value) {
        match self {
            Self::Validation { message, details }
            | Self::NotFound { message, details }
            | Self::Conflict { message, details }
            | Self::Internal { message, details }
            | Self::UpstreamFailure { message, details }
            | Self::Timeout { message, details }
            | Self::ValaTaskDagInvalid { message, details }
            | Self::ValaEvalRefKindMismatch { message, details }
            | Self::Unauthenticated { message, details }
            | Self::TokenExpired { message, details }
            | Self::InvalidToken { message, details }
            | Self::BadTokenFormat { message, details }
            | Self::UnsupportedGrantType { message, details }
            | Self::InvalidState { message, details }
            | Self::InvalidNonce { message, details }
            | Self::RefreshReused { message, details }
            | Self::RefreshRevoked { message, details }
            | Self::DelegationDepthExceededIssue { message, details }
            | Self::PrincipalKindCardKindMismatch { message, details }
            | Self::InvalidCardRefVersion { message, details }
            | Self::CredentialRevoked { message, details }
            | Self::ApiKeyInvalid { message, details }
            | Self::InvalidCardRef { message, details }
            | Self::CardScopeTooLarge { message, details }
            | Self::DelegationDepthExceededVerify { message, details }
            | Self::PrincipalNotFound { message, details }
            | Self::AdminConflict { message, details }
            | Self::AdminNotFound { message, details }
            | Self::AuthVerifyUnavailable { message, details }
            | Self::DiscoveryUnavailable { message, details }
            | Self::AuthPreviewDisabled { message, details }
            | Self::AuditUnavailable { message, details }
            | Self::AuthzRequiresDelegatedToken { message, details }
            | Self::MissingRequiredField { message, details }
            | Self::PolicyDenied { message, details }
            | Self::PermissionUnauthenticated { message, details }
            | Self::PermissionDeniedRbac { message, details }
            | Self::RoleCorrupt { message, details }
            | Self::BuiltinRoleImmutableName { message, details }
            | Self::QueryInvalidSyntax { message, details }
            | Self::QueryInvalidField { message, details }
            | Self::QueryTooComplex { message, details }
            | Self::SourceValidation { message, details }
            | Self::OriginValidation { message, details }
            | Self::DataValidation { message, details }
            | Self::DataUnknownDataType { message, details }
            | Self::DataInvalidSplitRule { message, details }
            | Self::DataTargetColumnUnknown { message, details }
            | Self::DataInvalidInterfaceOption { message, details }
            | Self::DataInterfaceMetadataRequired { message, details }
            | Self::DriftValidation { message, details }
            | Self::DriftSignalMethodMismatch { message, details }
            | Self::DriftProfileRequired { message, details }
            | Self::ModelUnknownModelType { message, details }
            | Self::ModelValidation { message, details }
            | Self::ModelMissingSignature { message, details }
            | Self::ModelDtypeNormalizeFailed { message, details }
            | Self::ModelShapeInvalid { message, details }
            | Self::ModelHfRevisionInvalid { message, details }
            | Self::ModelHfTaskMissing { message, details }
            | Self::ModelCustomLoaderInvalid { message, details }
            | Self::ModelSerializerUnavailable { message, details }
            | Self::PromptInvalidVariableName { message, details }
            | Self::PromptDuplicateVariable { message, details }
            | Self::PromptUndeclaredPlaceholder { message, details }
            | Self::PromptUnreferencedVariable { message, details }
            | Self::PromptUndeclaredMediaPlaceholder { message, details }
            | Self::PromptUnreferencedMediaVariable { message, details }
            | Self::PromptMediaPlaceholderNotIsolated { message, details }
            | Self::PromptMediaInSystemMessage { message, details }
            | Self::PromptUnsupportedMediaForProvider { message, details }
            | Self::PromptInvalidMediaType { message, details }
            | Self::PromptMissingMediaVariable { message, details }
            | Self::PromptMediaNotRegularFile { message, details }
            | Self::PromptMediaTooLarge { message, details }
            | Self::PromptMediaInvalidExtension { message, details }
            | Self::PromptMediaIo { message, details }
            | Self::PromptEmptyModel { message, details }
            | Self::PromptInvalidResponseSchema { message, details }
            | Self::PromptInvalidOutputSchema { message, details }
            | Self::PromptPydanticRequired { message, details }
            | Self::PromptLoaderBadExtension { message, details }
            | Self::PromptLoaderIo { message, details }
            | Self::PromptSerializeRequest { message, details }
            | Self::PromptProviderAuth { message, details }
            | Self::PromptProviderRateLimit { message, details }
            | Self::PromptProviderUpstream { message, details }
            | Self::PromptProviderTimeout { message, details }
            | Self::PromptProviderMismatch { message, details }
            | Self::PromptSettingsProviderMismatch { message, details }
            | Self::PromptSettingsDecode { message, details }
            | Self::PromptMissingVariable { message, details }
            | Self::PromptUnsupportedHandoff { message, details }
            | Self::PromptResponseDecode { message, details }
            | Self::PromptDraftInvalid { message, details }
            | Self::AgentValidation { message, details }
            | Self::AgentMissingName { message, details }
            | Self::AgentMissingVersion { message, details }
            | Self::AgentPromptCardNotFound { message, details }
            | Self::AgentRuntimeLocalToolNotFound { message, details }
            | Self::AgentRuntimeLocalToolsNotRegistrable { message, details }
            | Self::AgentCallbackReturnType { message, details }
            | Self::AgentCallbackAborted { message, details }
            | Self::AgentLoopMessageType { message, details }
            | Self::WorkflowValidation { message, details }
            | Self::WorkflowMissingName { message, details }
            | Self::WorkflowMissingVersion { message, details }
            | Self::WorkflowDuplicateStepId { message, details }
            | Self::WorkflowMissingDependency { message, details }
            | Self::WorkflowCycle { message, details }
            | Self::CfgInvalidToml { message, details }
            | Self::CfgSchemaMismatch { message, details }
            | Self::CfgNameDefaultRejected { message, details }
            | Self::RegistryInvalidCardSpec { message, details }
            | Self::RegistryInvalidVersionBlock { message, details }
            | Self::RegistrySpecTooLarge { message, details }
            | Self::RegistryVersionRequired { message, details }
            | Self::RegistryListLimitOutOfRange { message, details }
            | Self::RegistryCardRefUidNotResolvableHere { message, details }
            | Self::RegistryRequirementNotResolvableHere { message, details }
            | Self::RegistryCardNotFound { message, details }
            | Self::RegistryVersionConflict { message, details }
            | Self::RegistrySpecDrift { message, details }
            | Self::RegistryUnavailable { message, details }
            | Self::RegistryIdempotencyKeyRequired { message, details }
            | Self::RegistryManifestHashMismatch { message, details }
            | Self::RegistryInvalidArtifactPath { message, details }
            | Self::RegistryUploadInterrupted { message, details }
            | Self::RegistryCursorMismatch { message, details }
            | Self::RegistryIdempotencyConflict { message, details }
            | Self::RegistryOperationExpired { message, details }
            | Self::RegistryArtifactVerifyFailed { message, details }
            | Self::PrincipalOrphaned { message, details }
            | Self::ServerNotReady { message, details }
            | Self::ServiceUnavailable { message, details }
            | Self::RequestTimeout { message, details }
            | Self::PayloadTooLarge { message, details }
            | Self::HarnessStart { message, details }
            | Self::HarnessBound { message, details }
            | Self::HarnessBootstrap { message, details }
            | Self::EvalRunNotFound { message, details }
            | Self::EvalMissingLease { message, details }
            | Self::EvalInvalidLease { message, details }
            | Self::EvalSubmissionMismatch { message, details }
            | Self::EvalRunFailed { message, details }
            | Self::EvalTooManyRuns { message, details } => {
                (Cow::Borrowed(message.as_str()), details.clone())
            }
            Self::Storage { error } => (
                Cow::Owned(error.to_string()),
                serde_json::to_value(error).unwrap_or_else(|_| serde_json::json!({})),
            ),
            Self::Vala { error } => (
                Cow::Owned(error.to_string()),
                serde_json::to_value(error).unwrap_or_else(|_| serde_json::json!({})),
            ),
        }
    }

    /// Creates an internal error that preserves the source Skald code.
    #[must_use]
    pub fn internal_from_skald(code: &'static str, message: String) -> Self {
        Self::Internal {
            message,
            details: serde_json::json!({ "skald_code": code }),
        }
    }

    /// Construct [`WyrdError::Internal`].
    #[must_use]
    pub fn internal(message: impl Into<String>) -> Self {
        Self::Internal {
            message: message.into(),
            details: serde_json::json!({}),
        }
    }

    /// Construct [`WyrdError::RegistryInvalidCardSpec`].
    #[must_use]
    pub fn registry_invalid_card_spec(message: impl Into<String>) -> Self {
        Self::RegistryInvalidCardSpec {
            message: message.into(),
            details: serde_json::json!({}),
        }
    }

    /// Construct [`WyrdError::RegistryInvalidVersionBlock`].
    #[must_use]
    pub fn registry_invalid_version_block(message: impl Into<String>) -> Self {
        Self::RegistryInvalidVersionBlock {
            message: message.into(),
            details: serde_json::json!({}),
        }
    }

    /// Construct [`WyrdError::RegistrySpecTooLarge`].
    #[must_use]
    pub fn registry_spec_too_large(actual: usize, limit: usize) -> Self {
        Self::RegistrySpecTooLarge {
            message: format!("spec is {actual} bytes, exceeds {limit} byte limit"),
            details: serde_json::json!({ "actual": actual, "limit": limit }),
        }
    }

    /// Construct [`WyrdError::RegistryVersionRequired`].
    #[must_use]
    pub fn registry_version_required(message: impl Into<String>) -> Self {
        Self::RegistryVersionRequired {
            message: message.into(),
            details: serde_json::json!({}),
        }
    }

    /// Construct [`WyrdError::QueryInvalidSyntax`].
    #[must_use]
    pub fn query_invalid_syntax(
        message: impl Into<String>,
        offset: usize,
        expected: impl Into<String>,
        found: impl Into<String>,
    ) -> Self {
        Self::QueryInvalidSyntax {
            message: message.into(),
            details: serde_json::json!({
                "offset": offset,
                "expected": expected.into(),
                "found": found.into(),
            }),
        }
    }

    /// Construct [`WyrdError::QueryInvalidField`] with the empty-group reason.
    #[must_use]
    pub fn query_empty_group() -> Self {
        Self::QueryInvalidField {
            message: "empty boolean group (and/or with no children)".into(),
            details: serde_json::json!({ "reason": "empty_group" }),
        }
    }

    /// Construct [`WyrdError::QueryInvalidField`] with full type/field context.
    #[must_use]
    pub fn query_invalid_field_detail(
        message: impl Into<String>,
        detail: crate::query::QueryFieldErrorDetail<'_>,
    ) -> Self {
        Self::QueryInvalidField {
            message: message.into(),
            details: detail.to_json(),
        }
    }

    /// Construct [`WyrdError::QueryTooComplex`].
    #[must_use]
    pub fn query_too_complex(
        message: impl Into<String>,
        cap: &'static str,
        actual: usize,
        limit: usize,
    ) -> Self {
        Self::QueryTooComplex {
            message: message.into(),
            details: serde_json::json!({ "cap": cap, "actual": actual, "limit": limit }),
        }
    }

    /// Construct [`WyrdError::RegistryCardNotFound`].
    #[must_use]
    pub fn registry_card_not_found(message: impl Into<String>) -> Self {
        Self::RegistryCardNotFound {
            message: message.into(),
            details: serde_json::json!({}),
        }
    }

    /// Construct [`WyrdError::RegistrySpecDrift`].
    #[must_use]
    pub fn registry_spec_drift(
        card_uid: impl Into<String>,
        stored_hash: impl Into<String>,
        submitted_hash: impl Into<String>,
    ) -> Self {
        let stored = stored_hash.into();
        let submitted = submitted_hash.into();
        let uid = card_uid.into();
        Self::RegistrySpecDrift {
            message: format!(
                "spec hash mismatch for card {uid}: stored {stored}, submitted {submitted}"
            ),
            details: serde_json::json!({
                "stored_hash": stored,
                "submitted_hash": submitted,
            }),
        }
    }

    /// Construct [`WyrdError::RegistryUnavailable`].
    #[must_use]
    pub fn registry_unavailable(message: impl Into<String>) -> Self {
        Self::RegistryUnavailable {
            message: message.into(),
            details: serde_json::json!({}),
        }
    }

    /// Construct [`WyrdError::RegistryListLimitOutOfRange`].
    #[must_use]
    pub fn registry_list_limit_out_of_range(limit: u32, max: u32) -> Self {
        Self::RegistryListLimitOutOfRange {
            message: format!("limit {limit} is out of range 1..={max}"),
            details: serde_json::json!({ "limit": limit, "max": max }),
        }
    }

    /// Construct [`WyrdError::RegistryCardRefUidNotResolvableHere`].
    #[must_use]
    pub fn registry_card_ref_uid_not_resolvable_here(message: impl Into<String>) -> Self {
        Self::RegistryCardRefUidNotResolvableHere {
            message: message.into(),
            details: serde_json::json!({}),
        }
    }

    /// Construct [`WyrdError::RegistryRequirementNotResolvableHere`].
    #[must_use]
    pub fn registry_requirement_not_resolvable_here(message: impl Into<String>) -> Self {
        Self::RegistryRequirementNotResolvableHere {
            message: message.into(),
            details: serde_json::json!({}),
        }
    }

    /// Convert an [`crate::ids::IdError`] to a [`WyrdError`] (registry context).
    #[must_use]
    pub fn from_card_uid_error(e: crate::ids::IdError) -> Self {
        Self::Internal {
            message: format!("card_uid construction failed: {e}"),
            details: serde_json::json!({}),
        }
    }

    /// Convert a [`crate::envelope::SpecCanonicalizationError`] to [`WyrdError`].
    #[must_use]
    pub fn from_spec_canonicalization(e: crate::envelope::SpecCanonicalizationError) -> Self {
        Self::RegistryInvalidCardSpec {
            message: format!("spec canonicalization failed: {e}"),
            details: serde_json::json!({}),
        }
    }

    /// Convert a spec serialization [`serde_json::Error`] to [`WyrdError`].
    #[must_use]
    pub fn from_spec_serialization(e: serde_json::Error) -> Self {
        Self::Internal {
            message: format!("spec serialization failed: {e}"),
            details: serde_json::json!({}),
        }
    }
}

impl From<skald_spec::SkaldError> for WyrdError {
    fn from(error: skald_spec::SkaldError) -> Self {
        let message = error.to_string();
        match error {
            skald_spec::SkaldError::MissingVariable(name) => Self::PromptMissingVariable {
                message,
                details: serde_json::json!({ "name": name }),
            },
            skald_spec::SkaldError::MediaPlaceholderNotFound { name } => {
                Self::PromptUndeclaredMediaPlaceholder {
                    message,
                    details: serde_json::json!({ "name": name }),
                }
            }
            skald_spec::SkaldError::MediaPlaceholderNotIsolated { name } => {
                Self::PromptMediaPlaceholderNotIsolated {
                    message,
                    details: serde_json::json!({ "name": name }),
                }
            }
            skald_spec::SkaldError::MediaInSystemMessage { name } => {
                Self::PromptMediaInSystemMessage {
                    message,
                    details: serde_json::json!({ "name": name }),
                }
            }
            skald_spec::SkaldError::UnsupportedMediaForProvider { provider, kind } => {
                Self::PromptUnsupportedMediaForProvider {
                    message,
                    details: serde_json::json!({
                        "provider": format!("{provider:?}"),
                        "kind": format!("{kind:?}"),
                    }),
                }
            }
            skald_spec::SkaldError::InvalidMediaType(source) => Self::PromptInvalidMediaType {
                message,
                details: serde_json::json!({ "source": source }),
            },
            skald_spec::SkaldError::MissingMediaVariable { name } => {
                Self::PromptMissingMediaVariable {
                    message,
                    details: serde_json::json!({ "name": name }),
                }
            }
            skald_spec::SkaldError::MediaNotRegularFile { path } => {
                Self::PromptMediaNotRegularFile {
                    message,
                    details: serde_json::json!({ "path": path }),
                }
            }
            skald_spec::SkaldError::MediaTooLarge { path, size, limit } => {
                Self::PromptMediaTooLarge {
                    message,
                    details: serde_json::json!({ "path": path, "size": size, "limit": limit }),
                }
            }
            skald_spec::SkaldError::MediaInvalidExtension { path, kind } => {
                Self::PromptMediaInvalidExtension {
                    message,
                    details: serde_json::json!({ "path": path, "kind": format!("{kind:?}") }),
                }
            }
            skald_spec::SkaldError::MediaIo(source) => Self::PromptMediaIo {
                message,
                details: serde_json::json!({ "source": source }),
            },
            skald_spec::SkaldError::UnsupportedConversion { src, dst } => {
                Self::PromptUnsupportedHandoff {
                    message,
                    details: serde_json::json!({
                        "src": format!("{src:?}"),
                        "dst": format!("{dst:?}"),
                    }),
                }
            }
            skald_spec::SkaldError::SettingsProviderMismatch { expected, got } => {
                Self::PromptSettingsProviderMismatch {
                    message,
                    details: serde_json::json!({
                        "expected": format!("{expected:?}"),
                        "got": format!("{got:?}"),
                    }),
                }
            }
            skald_spec::SkaldError::SettingsDecode {
                provider,
                message: source,
            } => Self::PromptSettingsDecode {
                message,
                details: serde_json::json!({
                    "provider": format!("{provider:?}"),
                    "source": source,
                }),
            },
            skald_spec::SkaldError::PromptDraftInvalid {
                provider,
                message: source,
            } => Self::PromptDraftInvalid {
                message,
                details: serde_json::json!({ "provider": provider, "source": source }),
            },
            other => Self::internal_from_skald(other.code(), other.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::WyrdError;

    #[test]
    fn auth_codes_match_status() {
        for error in auth_errors() {
            let status = error
                .code()
                .split('_')
                .nth(2)
                .expect("error code has status segment")
                .parse::<u16>()
                .expect("status segment is numeric");

            assert_eq!(status, error.status());
        }
    }

    #[test]
    fn auth_variant_problem_json_roundtrips() {
        let error = WyrdError::Unauthenticated {
            message: "missing bearer token".to_owned(),
            details: serde_json::json!({ "header": "authorization" }),
        };
        let problem = error.as_problem_json();

        assert_eq!(problem["code"], "WYRD_AUTH_401_UNAUTHENTICATED");
        assert_eq!(problem["status"], 401);
        assert_eq!(problem["title"], "Not authenticated");
        assert_eq!(
            problem["remediation"],
            "Present a valid Wyrd token in the Authorization header."
        );
        assert_eq!(problem["detail"], "missing bearer token");
        assert_eq!(problem["details"]["header"], "authorization");
        assert!(
            problem["type"]
                .as_str()
                .expect("problem type is a string")
                .ends_with("WYRD_AUTH_401_UNAUTHENTICATED")
        );
    }

    #[test]
    fn auth_variants_serde_tag() {
        let error = WyrdError::PermissionDeniedRbac {
            message: "card write is required".to_owned(),
            details: serde_json::json!({ "required": { "resource": "cards", "action": "write" } }),
        };
        let value = serde_json::to_value(error).expect("auth error serializes");

        assert_eq!(value["kind"], "permission_denied_rbac");
    }

    #[test]
    fn permission_rbac_problem_json_carries_required() {
        let error = WyrdError::PermissionDeniedRbac {
            message: "principal lacks cards/write".to_owned(),
            details: serde_json::json!({
                "required": { "resource": "cards", "action": "write" },
                "principal": "018f5f1f-0000-7000-8000-000000000001",
            }),
        };
        let problem = error.as_problem_json();

        assert_eq!(problem["code"], "WYRD_PERMISSION_403_DENIED_RBAC");
        assert_eq!(problem["status"], 403);
        assert_eq!(problem["title"], "Permission denied (RBAC)");
        assert_eq!(problem["details"]["required"]["resource"], "cards");
        assert_eq!(problem["details"]["required"]["action"], "write");
    }

    #[test]
    fn api_key_invalid_remediation_names_issue_key() {
        let error = WyrdError::ApiKeyInvalid {
            message: "api key rejected".to_owned(),
            details: serde_json::json!({}),
        };
        let problem = error.as_problem_json();

        assert_eq!(problem["code"], "WYRD_AUTH_401_API_KEY_INVALID");
        assert!(
            problem["remediation"]
                .as_str()
                .expect("remediation is string")
                .contains("wyrd auth issue-key")
        );
    }

    #[test]
    fn invalid_card_ref_version_is_distinct_from_verifier_code() {
        let issuer_error = WyrdError::InvalidCardRefVersion {
            message: "card_ref.version was a requirement".to_owned(),
            details: serde_json::json!({ "got": "^1" }),
        };
        let verifier_error = WyrdError::InvalidCardRef {
            message: "card_ref claim missing".to_owned(),
            details: serde_json::json!({}),
        };

        assert_eq!(
            issuer_error.code(),
            "WYRD_AUTH_400_INVALID_CARD_REF_VERSION"
        );
        assert_eq!(issuer_error.status(), 400);
        assert_eq!(verifier_error.code(), "WYRD_AUTH_401_INVALID_CARD_REF");
        assert_eq!(verifier_error.status(), 401);
    }

    #[test]
    fn token_expired_problem_json() {
        let error = WyrdError::TokenExpired {
            message: "token expired".to_owned(),
            details: serde_json::json!({}),
        };
        let problem = error.as_problem_json();

        assert_eq!(problem["code"], "WYRD_AUTH_401_TOKEN_EXPIRED");
        assert_eq!(problem["status"], 401);
        assert_eq!(problem["title"], "Token expired");
        assert!(
            problem["type"]
                .as_str()
                .expect("problem type is a string")
                .ends_with("WYRD_AUTH_401_TOKEN_EXPIRED")
        );
    }

    #[test]
    fn token_expired_serde_tag() {
        let error = WyrdError::TokenExpired {
            message: "token expired".to_owned(),
            details: serde_json::json!({}),
        };
        let value = serde_json::to_value(error).expect("auth error serializes");

        assert_eq!(value["kind"], "token_expired");
    }

    #[test]
    fn invalid_token_problem_json() {
        let error = WyrdError::InvalidToken {
            message: "signature mismatch".to_owned(),
            details: serde_json::json!({}),
        };
        let problem = error.as_problem_json();

        assert_eq!(problem["code"], "WYRD_AUTH_401_INVALID_TOKEN");
        assert_eq!(problem["status"], 401);
        assert_eq!(problem["title"], "Invalid token");
        assert!(
            problem["type"]
                .as_str()
                .expect("problem type is a string")
                .ends_with("WYRD_AUTH_401_INVALID_TOKEN")
        );
    }

    #[test]
    fn invalid_token_serde_tag() {
        let error = WyrdError::InvalidToken {
            message: "signature mismatch".to_owned(),
            details: serde_json::json!({}),
        };
        let value = serde_json::to_value(error).expect("auth error serializes");

        assert_eq!(value["kind"], "invalid_token");
    }

    #[test]
    fn credential_revoked_problem_json() {
        let error = WyrdError::CredentialRevoked {
            message: "api key wyrd_sk_abc revoked".to_owned(),
            details: serde_json::json!({ "prefix": "wyrd_sk_abc" }),
        };
        let problem = error.as_problem_json();

        assert_eq!(problem["code"], "WYRD_AUTH_401_CREDENTIAL_REVOKED");
        assert_eq!(problem["status"], 401);
        assert_eq!(problem["title"], "Credential revoked");
        assert!(
            problem["type"]
                .as_str()
                .expect("problem type is a string")
                .ends_with("WYRD_AUTH_401_CREDENTIAL_REVOKED")
        );
    }

    #[test]
    fn credential_revoked_serde_tag() {
        let error = WyrdError::CredentialRevoked {
            message: "credential revoked".to_owned(),
            details: serde_json::json!({}),
        };
        let value = serde_json::to_value(error).expect("auth error serializes");

        assert_eq!(value["kind"], "credential_revoked");
    }

    fn auth_errors() -> Vec<WyrdError> {
        vec![
            WyrdError::Unauthenticated {
                message: "missing bearer token".to_owned(),
                details: serde_json::json!({}),
            },
            WyrdError::TokenExpired {
                message: "token expired".to_owned(),
                details: serde_json::json!({}),
            },
            WyrdError::InvalidToken {
                message: "token rejected".to_owned(),
                details: serde_json::json!({}),
            },
            WyrdError::BadTokenFormat {
                message: "authorization header malformed".to_owned(),
                details: serde_json::json!({}),
            },
            WyrdError::UnsupportedGrantType {
                message: "unsupported grant_type".to_owned(),
                details: serde_json::json!({ "supported_grant_types": ["wyrd_api_key"] }),
            },
            WyrdError::InvalidState {
                message: "state was missing or replayed".to_owned(),
                details: serde_json::json!({}),
            },
            WyrdError::InvalidNonce {
                message: "id token nonce mismatch".to_owned(),
                details: serde_json::json!({}),
            },
            WyrdError::RefreshReused {
                message: "rotated refresh token reused".to_owned(),
                details: serde_json::json!({}),
            },
            WyrdError::RefreshRevoked {
                message: "refresh token revoked".to_owned(),
                details: serde_json::json!({}),
            },
            WyrdError::DelegationDepthExceededIssue {
                message: "delegation chain would exceed depth 5".to_owned(),
                details: serde_json::json!({ "max": 5 }),
            },
            WyrdError::PrincipalKindCardKindMismatch {
                message: "model cards cannot back service accounts".to_owned(),
                details: serde_json::json!({ "card_kind": "model" }),
            },
            WyrdError::InvalidCardRefVersion {
                message: "card_ref.version must be pinned".to_owned(),
                details: serde_json::json!({ "got": "^1" }),
            },
            WyrdError::ApiKeyInvalid {
                message: "api key rejected".to_owned(),
                details: serde_json::json!({}),
            },
            WyrdError::InvalidCardRef {
                message: "card_ref missing".to_owned(),
                details: serde_json::json!({}),
            },
            WyrdError::DelegationDepthExceededVerify {
                message: "delegation chain exceeds depth 5".to_owned(),
                details: serde_json::json!({ "depth": 6, "max": 5 }),
            },
            WyrdError::PrincipalNotFound {
                message: "requested subject not found".to_owned(),
                details: serde_json::json!({}),
            },
            WyrdError::AuthVerifyUnavailable {
                message: "permission resolver unavailable".to_owned(),
                details: serde_json::json!({}),
            },
            WyrdError::DiscoveryUnavailable {
                message: "oidc discovery unavailable".to_owned(),
                details: serde_json::json!({}),
            },
            WyrdError::AuthPreviewDisabled {
                message: "auth preview disabled".to_owned(),
                details: serde_json::json!({}),
            },
            WyrdError::AuditUnavailable {
                message: "credential audit unavailable".to_owned(),
                details: serde_json::json!({}),
            },
            WyrdError::AuthzRequiresDelegatedToken {
                message: "delegated token required".to_owned(),
                details: serde_json::json!({}),
            },
            WyrdError::PermissionUnauthenticated {
                message: "permission check requires authentication".to_owned(),
                details: serde_json::json!({}),
            },
            WyrdError::PermissionDeniedRbac {
                message: "permission missing".to_owned(),
                details: serde_json::json!({}),
            },
            WyrdError::RoleCorrupt {
                message: "role permissions failed to decode".to_owned(),
                details: serde_json::json!({ "role": "bad_role" }),
            },
            WyrdError::BuiltinRoleImmutableName {
                message: "builtin role name cannot change".to_owned(),
                details: serde_json::json!({ "constraint": "auth_builtin_role_immutable_name" }),
            },
            WyrdError::CredentialRevoked {
                message: "credential revoked".to_owned(),
                details: serde_json::json!({}),
            },
            WyrdError::PrincipalOrphaned {
                message: "backing card deleted".to_owned(),
                details: serde_json::json!({}),
            },
        ]
    }
}

#[cfg(test)]
mod wyrd_cfg_tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn wyrd_cfg_400_invalid_toml_round_trip() {
        let e = WyrdError::CfgInvalidToml {
            message: "expected `=` after key".into(),
            details: json!({ "path": "/tmp/wyrd.toml", "source": "toml" }),
        };
        let json_str = serde_json::to_string(&e).unwrap();
        let back: WyrdError = serde_json::from_str(&json_str).unwrap();
        assert_eq!(back.code(), "WYRD_CFG_400_INVALID_TOML");
        assert_eq!(back.status(), 400);
        assert!(!back.remediation().is_empty());
    }

    #[test]
    fn wyrd_cfg_400_schema_mismatch_round_trip() {
        let e = WyrdError::CfgSchemaMismatch {
            message: "unknown field `defaut`".into(),
            details: json!({
                "path": "/tmp/wyrd.toml",
                "serde_message": "unknown field `defaut`, expected one of …",
            }),
        };
        let json_str = serde_json::to_string(&e).unwrap();
        let back: WyrdError = serde_json::from_str(&json_str).unwrap();
        assert_eq!(back.code(), "WYRD_CFG_400_SCHEMA_MISMATCH");
        assert_eq!(back.status(), 400);
        assert!(!back.remediation().is_empty());
    }

    #[test]
    fn wyrd_cfg_400_name_default_rejected_round_trip() {
        let e = WyrdError::CfgNameDefaultRejected {
            message: "`name` may not be defaulted; remove from table `[defaults]`".into(),
            details: json!({ "table": "[defaults]" }),
        };
        let json_str = serde_json::to_string(&e).unwrap();
        let back: WyrdError = serde_json::from_str(&json_str).unwrap();
        assert_eq!(back.code(), "WYRD_CFG_400_NAME_DEFAULT_REJECTED");
        assert_eq!(back.status(), 400);
        assert!(!back.remediation().is_empty());
    }
}

#[cfg(test)]
mod error_eval_tests {
    use std::collections::HashSet;

    use crate::error::WyrdError;

    fn eval_errors() -> Vec<WyrdError> {
        vec![
            WyrdError::EvalRunNotFound {
                message: "eval run abc not found".to_owned(),
                details: serde_json::json!({}),
            },
            WyrdError::EvalMissingLease {
                message: "missing lease token on protected eval-run route".to_owned(),
                details: serde_json::json!({}),
            },
            WyrdError::EvalInvalidLease {
                message: "lease token does not match the lease issued for run abc".to_owned(),
                details: serde_json::json!({}),
            },
            WyrdError::EvalSubmissionMismatch {
                message: "submission rejected: wrong turn".to_owned(),
                details: serde_json::json!({}),
            },
            WyrdError::EvalRunFailed {
                message: "eval run failed: internal engine error".to_owned(),
                details: serde_json::json!({}),
            },
            WyrdError::EvalTooManyRuns {
                message: "too many concurrent eval runs; retry after an existing run completes"
                    .to_owned(),
                details: serde_json::json!({}),
            },
        ]
    }

    #[test]
    fn eval_error_codes_are_non_empty_and_have_wyrd_eval_prefix() {
        for err in eval_errors() {
            let problem = err.as_problem_json();
            let code = problem["code"].as_str().expect("code is a string");
            assert!(
                code.starts_with("WYRD_EVAL_"),
                "expected WYRD_EVAL_ prefix, got: {code}"
            );
            assert!(!code.is_empty(), "code must be non-empty");
            let status = problem["status"].as_u64().expect("status is a number");
            assert!(status >= 400, "expected error status, got {status}");
            assert!(
                problem["title"].as_str().is_some_and(|t| !t.is_empty()),
                "title must be non-empty for {code}"
            );
            assert!(
                problem["remediation"]
                    .as_str()
                    .is_some_and(|r| !r.is_empty()),
                "remediation must be non-empty for {code}"
            );
        }
    }

    #[test]
    fn eval_error_codes_are_unique() {
        let codes: Vec<&'static str> = eval_errors().iter().map(|e| e.code()).collect();
        let unique: HashSet<&str> = codes.iter().copied().collect();
        assert_eq!(
            unique.len(),
            codes.len(),
            "duplicate eval error code detected"
        );
    }

    #[test]
    fn eval_run_not_found_is_404() {
        let err = WyrdError::EvalRunNotFound {
            message: "not found".to_owned(),
            details: serde_json::json!({}),
        };
        assert_eq!(err.code(), "WYRD_EVAL_404_RUN_NOT_FOUND");
        assert_eq!(err.status(), 404);
    }

    #[test]
    fn eval_missing_lease_is_401() {
        let err = WyrdError::EvalMissingLease {
            message: "no lease".to_owned(),
            details: serde_json::json!({}),
        };
        assert_eq!(err.code(), "WYRD_EVAL_401_MISSING_LEASE");
        assert_eq!(err.status(), 401);
    }

    #[test]
    fn eval_invalid_lease_is_403() {
        let err = WyrdError::EvalInvalidLease {
            message: "bad lease".to_owned(),
            details: serde_json::json!({}),
        };
        assert_eq!(err.code(), "WYRD_EVAL_403_INVALID_LEASE");
        assert_eq!(err.status(), 403);
    }

    #[test]
    fn eval_submission_mismatch_is_409() {
        let err = WyrdError::EvalSubmissionMismatch {
            message: "mismatch".to_owned(),
            details: serde_json::json!({}),
        };
        assert_eq!(err.code(), "WYRD_EVAL_409_SUBMISSION_MISMATCH");
        assert_eq!(err.status(), 409);
    }

    #[test]
    fn eval_run_failed_is_500() {
        let err = WyrdError::EvalRunFailed {
            message: "engine died".to_owned(),
            details: serde_json::json!({}),
        };
        assert_eq!(err.code(), "WYRD_EVAL_500_RUN_FAILED");
        assert_eq!(err.status(), 500);
    }

    #[test]
    fn eval_too_many_runs_is_429() {
        let err = WyrdError::EvalTooManyRuns {
            message: "cap reached".to_owned(),
            details: serde_json::json!({}),
        };
        assert_eq!(err.code(), "WYRD_EVAL_429_TOO_MANY_RUNS");
        assert_eq!(err.status(), 429);
    }

    #[test]
    fn dropped_eval_codes_are_absent_from_wyrd_spec() {
        let all_codes: Vec<&str> = eval_errors().iter().map(|e| e.code()).collect();
        assert!(
            !all_codes.contains(&"WYRD_EVAL_401_API_KEY_INVALID"),
            "WYRD_EVAL_401_API_KEY_INVALID must not be present in WyrdError"
        );
        assert!(
            !all_codes.contains(&"WYRD_EVAL_500_RESULTS_PERSISTENCE_FAILED"),
            "WYRD_EVAL_500_RESULTS_PERSISTENCE_FAILED must not be present in WyrdError"
        );
    }

    #[test]
    fn eval_remediation_strings_use_v1_not_api_v1() {
        for err in eval_errors() {
            let remediation = err.remediation();
            assert!(
                !remediation.contains("/api/v1"),
                "remediation for {} must not contain /api/v1, got: {remediation}",
                err.code()
            );
        }
    }
}

#[cfg(test)]
mod error_registry_tests {
    use crate::error::WyrdError;

    fn registry_errors() -> Vec<WyrdError> {
        vec![
            WyrdError::RegistryInvalidCardSpec {
                message: "unknown kind".to_owned(),
                details: serde_json::json!({}),
            },
            WyrdError::RegistryInvalidVersionBlock {
                message: "service cards require a pin version".to_owned(),
                details: serde_json::json!({}),
            },
            WyrdError::RegistrySpecTooLarge {
                message: "spec exceeds 256 KiB".to_owned(),
                details: serde_json::json!({}),
            },
            WyrdError::RegistryVersionRequired {
                message: "metadata.version is required".to_owned(),
                details: serde_json::json!({}),
            },
            WyrdError::RegistryListLimitOutOfRange {
                message: "limit out of range".to_owned(),
                details: serde_json::json!({}),
            },
            WyrdError::RegistryCardRefUidNotResolvableHere {
                message: "uid not resolvable here".to_owned(),
                details: serde_json::json!({}),
            },
            WyrdError::RegistryRequirementNotResolvableHere {
                message: "requirement version not accepted here".to_owned(),
                details: serde_json::json!({}),
            },
            WyrdError::RegistryCardNotFound {
                message: "card not found".to_owned(),
                details: serde_json::json!({}),
            },
            WyrdError::RegistryVersionConflict {
                message: "version conflict".to_owned(),
                details: serde_json::json!({}),
            },
            WyrdError::RegistrySpecDrift {
                message: "spec changed for immutable version".to_owned(),
                details: serde_json::json!({}),
            },
            WyrdError::RegistryUnavailable {
                message: "registry unavailable".to_owned(),
                details: serde_json::json!({}),
            },
            WyrdError::PrincipalOrphaned {
                message: "backing card deleted".to_owned(),
                details: serde_json::json!({}),
            },
        ]
    }

    #[test]
    fn every_registry_variant_has_problem_json_round_trip() {
        for err in registry_errors() {
            let problem = err.as_problem_json();
            let code = problem["code"].as_str().expect("code is a string");
            assert!(
                code.starts_with("WYRD_REG_") || code.starts_with("WYRD_AUTH_"),
                "unexpected code prefix: {code}"
            );
            assert!(problem["status"].as_u64().unwrap() >= 400);
            assert!(problem["title"].as_str().is_some());
        }
    }

    #[test]
    fn registry_unavailable_status_is_503() {
        let err = WyrdError::RegistryUnavailable {
            message: "transient".to_owned(),
            details: serde_json::json!({}),
        };
        let problem = err.as_problem_json();
        assert_eq!(problem["status"], 503);
        assert_eq!(problem["code"], "WYRD_REG_503_REGISTRY_UNAVAILABLE");
    }

    #[test]
    fn spec_drift_status_is_409() {
        let err = WyrdError::RegistrySpecDrift {
            message: "drift".to_owned(),
            details: serde_json::json!({}),
        };
        let problem = err.as_problem_json();
        assert_eq!(problem["status"], 409);
        assert_eq!(problem["code"], "WYRD_REG_409_SPEC_DRIFT");
    }

    #[test]
    fn registry_card_not_found_status_is_404() {
        let err = WyrdError::RegistryCardNotFound {
            message: "not found".to_owned(),
            details: serde_json::json!({}),
        };
        let problem = err.as_problem_json();
        assert_eq!(problem["status"], 404);
        assert_eq!(problem["code"], "WYRD_REG_404_CARD_NOT_FOUND");
    }

    #[test]
    fn registry_400_variants_all_return_status_400() {
        let cases = vec![
            WyrdError::RegistryInvalidCardSpec {
                message: "bad spec".to_owned(),
                details: serde_json::json!({}),
            },
            WyrdError::RegistryInvalidVersionBlock {
                message: "bad version".to_owned(),
                details: serde_json::json!({}),
            },
            WyrdError::RegistrySpecTooLarge {
                message: "too large".to_owned(),
                details: serde_json::json!({}),
            },
            WyrdError::RegistryVersionRequired {
                message: "version required".to_owned(),
                details: serde_json::json!({}),
            },
            WyrdError::RegistryListLimitOutOfRange {
                message: "limit out of range".to_owned(),
                details: serde_json::json!({}),
            },
        ];
        for err in cases {
            let problem = err.as_problem_json();
            assert_eq!(
                problem["status"], 400,
                "expected status 400 for {}",
                problem["code"]
            );
        }
    }

    #[test]
    fn registry_version_conflict_status_is_500() {
        let err = WyrdError::RegistryVersionConflict {
            message: "uid mismatch".to_owned(),
            details: serde_json::json!({}),
        };
        let problem = err.as_problem_json();
        assert_eq!(problem["status"], 500);
        assert_eq!(problem["code"], "WYRD_REG_500_VERSION_CONFLICT");
    }

    #[test]
    fn principal_orphaned_status_matches_embedded_code() {
        let err = WyrdError::PrincipalOrphaned {
            message: "card deleted".to_owned(),
            details: serde_json::json!({}),
        };
        let problem = err.as_problem_json();
        let code = problem["code"].as_str().expect("code is a string");
        let status = problem["status"].as_u64().expect("status is a number");
        let code_status: u64 = code
            .split('_')
            .find(|s| s.chars().all(|c| c.is_ascii_digit()))
            .and_then(|s| s.parse().ok())
            .expect("code must embed a numeric status");
        assert_eq!(code_status, status);
    }
}

#[cfg(test)]
mod error_storage_tests {
    use std::collections::HashSet;

    use crate::error::WyrdError;
    use crate::error::storage::WyrdStorageError;

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
            problem["details"]["variant"], "object_not_found",
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
}

#[cfg(test)]
mod error_vala_tests {
    use crate::error::WyrdError;
    use crate::vala::error::BifrostError;

    #[test]
    fn bifrost_error_delegates_code_and_status_into_wyrd_error() {
        let cases = vec![
            (
                BifrostError::FingerprintMismatch {
                    table: "tenant.events".to_owned(),
                },
                "WYRD_VALA_409_BIFROST_FINGERPRINT_MISMATCH",
                409u16,
            ),
            (
                BifrostError::QueryInvalidSql {
                    detail: "not a SELECT".to_owned(),
                },
                "WYRD_VALA_400_QUERY_INVALID_SQL",
                400,
            ),
            (
                BifrostError::QueryResultTooLarge,
                "WYRD_VALA_413_QUERY_RESULT_TOO_LARGE",
                413,
            ),
            (
                BifrostError::QueryTimeout,
                "WYRD_VALA_504_QUERY_TIMEOUT",
                504,
            ),
        ];

        for (wire, code, status) in cases {
            let wire_code = wire.code();
            let wire_status = wire.status();
            let wire_title = wire.title();

            let error: WyrdError = wire.into();

            assert_eq!(error.code(), code);
            assert_eq!(error.status(), status);
            assert_eq!(error.code(), wire_code, "delegate forwards wire code");
            assert_eq!(error.status(), wire_status, "delegate forwards wire status");
            assert_eq!(error.title(), wire_title, "delegate forwards wire title");
        }
    }

    #[test]
    fn bifrost_error_lifts_into_problem_json_without_losing_code() {
        let error: WyrdError = BifrostError::FingerprintMismatch {
            table: "tenant.events".to_owned(),
        }
        .into();

        let problem = error.as_problem_json();

        assert_eq!(
            problem["code"],
            "WYRD_VALA_409_BIFROST_FINGERPRINT_MISMATCH"
        );
        assert_eq!(problem["status"], 409);
        assert_eq!(problem["title"], "Schema fingerprint mismatch");
        assert_eq!(
            problem["details"]["variant"], "fingerprint_mismatch",
            "vala details retain the wire enum discriminant"
        );
    }
}
