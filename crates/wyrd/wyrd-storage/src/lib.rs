//! Server-tier storage handles and backend signers for Wyrd artifacts.
//!
//! This crate owns storage planning, tenant path validation, and backend
//! signer dispatch. It contains no Python boundary code and does not parse
//! environment variables outside the boot settings module.

#![deny(missing_docs)]
#![deny(rustdoc::broken_intra_doc_links)]

#[cfg(feature = "cloud")]
pub mod azure;
#[cfg(feature = "cloud")]
pub mod cloud;
pub mod encryption;
pub mod env_parse;
pub mod error;
pub mod factory;
#[cfg(feature = "cloud")]
pub mod gcs;
pub mod handle;
pub mod local;
pub mod plan;
pub mod preflight;
#[cfg(feature = "cloud")]
pub mod s3;
pub mod service;
pub mod settings;
pub mod sha;
pub mod signer;
pub mod sweeper;
pub mod tenant_path;

mod audit;

#[cfg(feature = "cloud")]
pub use azure::{AzureSasMode, AzureSigner};
#[cfg(feature = "cloud")]
pub use cloud::CloudSigner;
pub use error::{AzureError, ConfigParseError, GcsError, LocalError, S3Error, StorageError};
pub use handle::{StorageHandle, StorageHealthError};
pub use local::LocalSigner;
pub use plan::{PlanError, PlannedUpload, plan_upload};
pub use service::{StorageCaller, StoragePrincipalKind, StorageSubject};
pub use settings::{BackendConfig, StorageSettings};
pub use signer::{BackendSigner, CompletePayload, HeadInfo, MultipartInit, UploadPlanReplayInput};
pub use tenant_path::{TenantPathError, ValidatedPath};
