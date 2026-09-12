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

mod telemetry;

#[cfg(feature = "cloud")]
pub use azure::{AzureSasMode, AzureSigner};
#[cfg(feature = "cloud")]
pub use cloud::CloudSigner;
pub use error::{AzureError, ConfigParseError, GcsError, LocalError, S3Error, StorageError};
pub use handle::{StorageHandle, StorageHealthError};
pub use local::LocalSigner;
pub use plan::{PlanError, PlannedUpload, plan_upload};
pub use service::StorageCaller;
pub use settings::{BackendConfig, StorageSettings};
pub use signer::{BackendSigner, CompletePayload, HeadInfo, MultipartInit, UploadPlanReplayInput};
pub use tenant_path::{TenantPathError, ValidatedPath};

#[cfg(test)]
mod upload_id_tests {
    use std::str::FromStr;
    use wyrd_spec::storage::UploadId;

    #[test]
    fn upload_id_round_trips_display_parse() {
        let upload_id = UploadId::new();
        let parsed = UploadId::from_str(&upload_id.to_string()).expect("valid upload id parses");

        assert_eq!(parsed, upload_id);
        assert!(upload_id.to_string().starts_with("wyu_"));
    }

    #[test]
    fn upload_id_rejects_missing_prefix() {
        let err = UploadId::from_str("01890f28-7c4a-7cc3-98e7-4f4a3c2d1b00")
            .expect_err("missing prefix fails");

        assert!(err.to_string().contains("wyu_"));
    }

    #[test]
    fn upload_id_rejects_bad_uuid_body() {
        let err = UploadId::from_str("wyu_not-a-uuid").expect_err("bad body fails");

        assert!(err.to_string().contains("UUIDv7"));
    }

    #[test]
    fn upload_id_rejects_non_v7_uuid_body() {
        let err = UploadId::from_str("wyu_550e8400-e29b-41d4-a716-446655440000")
            .expect_err("non-v7 fails");

        assert!(err.to_string().contains("UUIDv7"));
    }
}
