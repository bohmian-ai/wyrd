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
    IDEMPOTENCY_KEY_HEADER, LocalBlobUploadResponse, PartUrlResponse, S3CompletedPart,
    S3MultipartComplete, SinglePutComplete, UploadCompleteRequest, UploadCompleteResponse,
    UploadInitRequest, UploadInitResponse, UploadPlan, VerificationGuarantee,
    backend_verification_guarantee,
};

#[cfg(test)]
mod storage_contracts_tests {
    use crate::storage::{StorageBackendKind, UploadId, UploadPlan, WireProtocol};
    use serde_json::json;
    use std::str::FromStr;

    #[test]
    fn storage_upload_id_parse_display_roundtrip() {
        let upload_id = UploadId::new();
        let parsed = UploadId::from_str(upload_id.as_str()).expect("upload id parses");

        assert_eq!(parsed, upload_id);
    }

    #[test]
    fn storage_backend_kind_sql_roundtrip_strings() {
        assert_eq!(StorageBackendKind::S3.to_string(), "s3");
        assert_eq!(
            "azure".parse::<StorageBackendKind>().unwrap(),
            StorageBackendKind::Azure
        );
    }

    #[test]
    fn storage_wire_protocol_sql_roundtrip_strings() {
        assert_eq!(WireProtocol::LocalFsV1.to_string(), "local_fs_v1");
        assert_eq!(
            "gcs_resumable_v1".parse::<WireProtocol>().unwrap(),
            WireProtocol::GcsResumableV1
        );
    }

    #[test]
    fn storage_upload_plan_rejects_unknown_protocol_tag() {
        let err = serde_json::from_value::<UploadPlan>(json!({
            "protocol": "future_backend_v1",
            "data": {}
        }))
        .expect_err("unknown upload protocol must not deserialize");

        assert!(err.to_string().contains("unknown variant"));
    }

    #[test]
    fn storage_wire_protocol_rejects_unknown_string() {
        let parse_err = "future_backend_v1"
            .parse::<WireProtocol>()
            .expect_err("unknown protocol string must fail");
        assert!(parse_err.to_string().contains("unknown wire protocol"));

        let serde_err = serde_json::from_value::<WireProtocol>(json!("future_backend_v1"))
            .expect_err("unknown protocol JSON must fail");
        assert!(serde_err.to_string().contains("unknown variant"));
    }
}
