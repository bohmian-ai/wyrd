//! Server-tier storage handles and backend signers for Wyrd artifacts.
//!
//! This crate owns storage planning, tenant path validation, and backend
//! signer dispatch. It contains no Python boundary code and does not parse
//! environment variables in this commit.

#![deny(missing_docs)]
#![deny(rustdoc::broken_intra_doc_links)]

pub mod azure;
pub mod encryption;
pub mod error;
pub mod gcs;
pub mod handle;
pub mod local;
pub mod plan;
pub mod s3;
pub mod sha;
pub mod signer;
pub mod tenant_path;
pub mod upload_id;

pub use azure::{AzureSasMode, AzureSigner};
pub use error::{AzureError, GcsError, LocalError, S3Error, StorageError};
pub use handle::StorageHandle;
pub use plan::{PlanError, PlannedUpload, plan_upload};
pub use signer::{BackendSigner, CompletePayload, HeadInfo, MultipartInit, UploadPlanReplayInput};
pub use tenant_path::{TenantPathError, ValidatedPath};
pub use upload_id::{UploadId, UploadIdParseError};
