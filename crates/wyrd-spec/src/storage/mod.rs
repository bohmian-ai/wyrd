//! Storage wire contracts.
//!
//! This module is PyO3-free, async-free, IO-free, and cloud-SDK-free. It
//! defines the typed request and response payloads shared by HTTP, MCP, SDK,
//! and generated schema surfaces.

pub mod artifact;
pub mod backend;
pub mod download;
pub mod ids;
pub mod protocol;
pub mod upload;

pub use artifact::StoredObjectRef;
pub use backend::{MetadataBackend, StorageBackendKind};
pub use download::{DownloadInitRequest, DownloadInitResponse, DownloadPlan};
pub use ids::{UploadId, UploadIdParseError};
pub use protocol::{WireProtocol, WireProtocolParseError};
pub use upload::{
    AbortResponse, AzureBlockBlobComplete, GcsResumableComplete, HeaderPair,
    IDEMPOTENCY_KEY_HEADER, PartUrlResponse, S3CompletedPart, S3MultipartComplete,
    SinglePutComplete, UploadCompleteRequest, UploadCompleteResponse, UploadInitRequest,
    UploadInitResponse, UploadPlan, VerificationGuarantee, backend_verification_guarantee,
};
