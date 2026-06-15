//! Upload wire contracts.

use crate::ids::CardUid;
use crate::storage::artifact::StoredObjectRef;
use crate::storage::backend::StorageBackendKind;
use crate::storage::ids::UploadId;
use serde::{Deserialize, Serialize};

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

/// Server response after an upload completes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields, rename_all = "snake_case")]
pub struct UploadCompleteResponse {
    /// Wire-only descriptor for the stored bytes.
    pub stored: StoredObjectRef,
}
