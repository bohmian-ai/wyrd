//! Closed upload wire protocol identifiers.

use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

/// On-wire upload protocol.
///
/// This is plan-shaped, not backend-shaped: one backend can emit different
/// protocols depending on artifact size. It is also the only protocol string
/// intended for SQL persistence and backend dispatch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum WireProtocol {
    /// Cloud single PUT to a presigned URL.
    SinglePutV1,
    /// AWS S3 multipart upload.
    S3MultipartV1,
    /// Google Cloud Storage resumable upload.
    GcsResumableV1,
    /// Azure block blob upload.
    AzureBlockBlobV1,
    /// Local filesystem upload through a server-owned temporary object.
    LocalFsV1,
}

impl fmt::Display for WireProtocol {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::SinglePutV1 => "single_put_v1",
            Self::S3MultipartV1 => "s3_multipart_v1",
            Self::GcsResumableV1 => "gcs_resumable_v1",
            Self::AzureBlockBlobV1 => "azure_block_blob_v1",
            Self::LocalFsV1 => "local_fs_v1",
        })
    }
}

impl FromStr for WireProtocol {
    type Err = WireProtocolParseError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "single_put_v1" => Ok(Self::SinglePutV1),
            "s3_multipart_v1" => Ok(Self::S3MultipartV1),
            "gcs_resumable_v1" => Ok(Self::GcsResumableV1),
            "azure_block_blob_v1" => Ok(Self::AzureBlockBlobV1),
            "local_fs_v1" => Ok(Self::LocalFsV1),
            other => Err(WireProtocolParseError(other.to_owned())),
        }
    }
}

/// Error returned when parsing an upload wire protocol fails.
#[derive(Debug, thiserror::Error)]
#[error("unknown wire protocol: {0}")]
pub struct WireProtocolParseError(String);
