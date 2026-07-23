//! Upload wire contracts.

use crate::ids::CardUid;
use crate::storage::artifact::StoredObjectRef;
use crate::storage::backend::StorageBackendKind;
use crate::storage::ids::UploadId;
use serde::{Deserialize, Serialize};

/// HTTP header name for upload initialization idempotency.
///
/// Include this header on `POST /v1/cards/upload/init` to enable idempotent
/// replay. The server caches the response keyed on `(tenant, idempotency_key,
/// body_sha256)`. Reusing the same key with a different request body returns
/// a conflict error.
pub const IDEMPOTENCY_KEY_HEADER: &str = "idempotency-key";

/// Client request to initialize an upload.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields, rename_all = "snake_case")]
pub struct UploadInitRequest {
    /// Card that will own the uploaded artifact bytes.
    pub card_uid: CardUid,
    /// Object path under the card.
    pub relative_path: String,
    /// Expected base64-encoded SHA-256 digest.
    ///
    /// Per-backend verification semantics:
    /// - **S3**: server-verified-against-client via `x-amz-checksum-sha256` at
    ///   multipart complete; S3 rejects the commit on mismatch.
    /// - **GCS**: client-declared; GCS stores CRC32c and MD5 only. The declared
    ///   hash is recorded at init and trusted at complete.
    /// - **Azure**: client-declared only; no server-side SHA-256 recomputation
    ///   is available. SAS-restricted PUT and TLS prevent in-flight tampering.
    /// - **Local**: server-computed by streaming the on-disk file; the local
    ///   backend is the only signer where the server reads object bytes by design.
    pub expected_sha256: String,
    /// Expected byte length.
    pub expected_size_bytes: u64,
    /// Optional content type.
    #[serde(default)]
    pub content_type: Option<String>,
}

/// Server response to an upload-init request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields, rename_all = "snake_case")]
pub struct UploadInitResponse {
    /// Durable upload identifier.
    pub upload_id: UploadId,
    /// Configured storage backend.
    pub backend: StorageBackendKind,
    /// Backend-specific upload plan.
    pub plan: UploadPlan,
    /// Full tenant-scoped storage path.
    pub storage_path: String,
}

/// Tagged union describing how the client uploads bytes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(tag = "protocol", content = "data", rename_all = "snake_case")]
pub enum UploadPlan {
    /// Single PUT to a presigned URL.
    SinglePut {
        /// Presigned PUT URL.
        put_url: String,
        /// URL time-to-live in seconds.
        ttl_secs: u32,
        /// Headers the client must include on the PUT.
        #[serde(default)]
        required_headers: Vec<HeaderPair>,
    },
    /// Local filesystem auth-gated upload.
    ///
    /// Signals that the upload URL is an auth-gated server route for one
    /// server-minted upload capability, not a caller-selected storage path.
    LocalFs {
        /// Server route PUT URL.
        put_url: String,
        /// URL time-to-live in seconds.
        ttl_secs: u32,
    },
    /// AWS S3 multipart upload.
    S3Multipart {
        /// Number of parts planned.
        part_count: u32,
        /// Size of each non-final part.
        part_size_bytes: u64,
        /// Time-to-live for per-part URLs in seconds.
        part_url_ttl_secs: u32,
        /// Headers the client must include on each part PUT.
        #[serde(default)]
        required_headers: Vec<HeaderPair>,
    },
    /// Google Cloud Storage resumable upload.
    GcsResumable {
        /// Resumable session URI returned once to the client.
        session_uri: String,
        /// Recommended chunk size in bytes.
        chunk_size_bytes: u64,
    },
    /// Azure block blob upload.
    AzureBlockBlob {
        /// SAS URL returned once to the client.
        sas_url: String,
        /// Recommended block size in bytes.
        block_size_bytes: u64,
        /// Number of blocks the server expects to commit.
        block_count_planned: u32,
    },
}

/// Per-backend SHA-256 verification guarantee.
///
/// Describes how the server validates `expected_sha256` for each backend.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum VerificationGuarantee {
    /// Server independently computes SHA-256 from stored object data.
    ///
    /// Currently used only for the `Local` backend, which reads from the
    /// server's local filesystem (zero egress cost, no SDK limitation).
    ServerComputed,
    /// Server passes the client-declared SHA-256 to the backend at commit
    /// time; the backend rejects the commit on mismatch.
    ServerVerifiedAgainstClient,
    /// Client declares SHA-256 at upload init; the server has no independent
    /// verification path for this backend.
    ClientDeclared,
}

/// Return the SHA-256 verification guarantee for a storage backend.
#[must_use]
pub fn backend_verification_guarantee(backend: StorageBackendKind) -> VerificationGuarantee {
    match backend {
        StorageBackendKind::S3 => VerificationGuarantee::ServerVerifiedAgainstClient,
        StorageBackendKind::Gcs => VerificationGuarantee::ClientDeclared,
        StorageBackendKind::Azure => VerificationGuarantee::ClientDeclared,
        StorageBackendKind::Local => VerificationGuarantee::ServerComputed,
    }
}

/// Required HTTP header for a client upload request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields, rename_all = "snake_case")]
pub struct HeaderPair {
    /// Header name.
    pub name: String,
    /// Header value.
    pub value: String,
}

/// Client request to complete an upload.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(tag = "protocol", content = "data", rename_all = "snake_case")]
pub enum UploadCompleteRequest {
    /// Single PUT completion.
    SinglePut(SinglePutComplete),
    /// S3 multipart completion.
    S3Multipart(S3MultipartComplete),
    /// GCS resumable completion.
    GcsResumable(GcsResumableComplete),
    /// Azure block blob completion.
    AzureBlockBlob(AzureBlockBlobComplete),
}

/// Single PUT completion payload.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields, rename_all = "snake_case")]
pub struct SinglePutComplete {}

/// S3 multipart completion payload.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields, rename_all = "snake_case")]
pub struct S3MultipartComplete {
    /// Parts uploaded by the client.
    pub parts: Vec<S3CompletedPart>,
}

/// Completed S3 part descriptor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields, rename_all = "snake_case")]
pub struct S3CompletedPart {
    /// One-based part number.
    pub part_number: u32,
    /// S3 ETag returned by the part upload.
    pub e_tag: String,
}

/// GCS resumable completion payload.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields, rename_all = "snake_case")]
pub struct GcsResumableComplete {}

/// Azure block blob completion payload.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields, rename_all = "snake_case")]
pub struct AzureBlockBlobComplete {
    /// Number of blocks the client uploaded.
    pub block_count: u32,
}

/// Server response for a local-filesystem blob upload.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields, rename_all = "snake_case")]
pub struct LocalBlobUploadResponse {
    /// Whether the blob was written successfully.
    pub uploaded: bool,
}

/// Server response after an upload completes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields, rename_all = "snake_case")]
pub struct UploadCompleteResponse {
    /// Wire-only descriptor for the stored bytes.
    pub stored: StoredObjectRef,
}

/// Server response for a single multipart part presigned URL.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields, rename_all = "snake_case")]
pub struct PartUrlResponse {
    /// Presigned part URL.
    pub url: String,
    /// URL TTL in seconds.
    pub ttl_secs: u32,
}

/// Server response for an upload abort request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields, rename_all = "snake_case")]
pub struct AbortResponse {
    /// Whether the upload was marked aborted by this request.
    pub aborted: bool,
}
