//! Internal storage errors.

use wyrd_spec::DataTenantId;
use wyrd_spec::storage::StorageBackendKind;

/// Storage configuration parse failure.
#[derive(Debug, thiserror::Error)]
pub enum ConfigParseError {
    /// Required environment variable was missing or unreadable.
    #[error("required environment variable is missing or unreadable: {0}")]
    MissingEnv(#[source] std::env::VarError),
    /// Environment variable used an unknown enum value.
    #[error("unknown value `{value}`; expected {expected}")]
    UnknownValue {
        /// Supplied value.
        value: String,
        /// Expected values.
        expected: &'static str,
    },
    /// Environment variable was not a recognized boolean.
    #[error("invalid boolean `{0}`; expected true/false, 1/0, yes/no, or on/off")]
    InvalidBool(String),
    /// Environment variable was not a valid unsigned 32-bit integer.
    #[error("invalid u32 `{value}`")]
    InvalidU32 {
        /// Supplied value.
        value: String,
        /// Parse source.
        #[source]
        source: std::num::ParseIntError,
    },
    /// Environment variable was not a valid unsigned 64-bit integer.
    #[error("invalid u64 `{value}`")]
    InvalidU64 {
        /// Supplied value.
        value: String,
        /// Parse source.
        #[source]
        source: std::num::ParseIntError,
    },
    /// Part size was not aligned to one MiB.
    #[error("part size {0} is not a multiple of 1 MiB")]
    PartSizeNotMiBAligned(u64),
    /// Environment variable path failed validation.
    #[error("invalid path `{0}`")]
    InvalidPath(String),
}

/// Top-level error returned by storage operations.
#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    /// Tenant path validation failed.
    #[error("tenant path mismatch: {0}")]
    TenantPathMismatch(String),
    /// Artifact size exceeds the supported cross-backend limit.
    #[error("artifact too large: {actual} bytes exceeds backend limit {limit} bytes")]
    ArtifactTooLarge {
        /// Actual byte size.
        actual: u64,
        /// Maximum accepted byte size.
        limit: u64,
    },
    /// Expected SHA-256 value failed validation.
    #[error("expected sha256 invalid: {0}")]
    InvalidExpectedSha256(String),
    /// Expected object size failed validation.
    #[error("expected size invalid: {0}")]
    InvalidExpectedSize(u64),
    /// Required server-side encryption marker was missing.
    #[error("encryption required but backend did not advertise server-side encryption")]
    EncryptionMissing,
    /// Stored object SHA-256 did not match.
    #[error("sha256 mismatch: expected {expected}, actual {actual}")]
    Sha256Mismatch {
        /// Expected base64 SHA-256.
        expected: String,
        /// Actual base64 SHA-256.
        actual: String,
    },
    /// Stored object byte length did not match.
    #[error("size mismatch: expected {expected}, actual {actual}")]
    SizeMismatch {
        /// Expected bytes.
        expected: u64,
        /// Actual bytes.
        actual: u64,
    },
    /// Credential chain failed for the named backend.
    #[error("credential chain returned no credentials for backend {0}")]
    CredentialChain(&'static str),
    /// Backend lifecycle rule is missing.
    #[error("backend lifecycle rule missing: {0}")]
    LifecycleRuleMissing(String),
    /// Presigned URL expired.
    #[error("presign expired: {0}")]
    PresignExpired(String),
    /// Object was not found.
    #[error("object not found: {storage_path}")]
    ObjectNotFound {
        /// Tenant-scoped object path.
        storage_path: String,
    },
    /// Backend does not support the requested operation.
    #[error("backend `{signer}` does not support operation `{op}`")]
    BackendCapabilityMismatch {
        /// Active backend.
        signer: StorageBackendKind,
        /// Requested operation.
        op: &'static str,
    },
    /// Backend call failed.
    #[error("backend `{backend}` failed during `{op}`: {message}")]
    Backend {
        /// Active backend.
        backend: StorageBackendKind,
        /// Operation name.
        op: &'static str,
        /// Safe diagnostic message.
        message: String,
    },
    /// Platform-admin pool acquisition failed.
    #[error("platform admin pool failed")]
    AdminPool {
        /// `SQLx` source error.
        #[source]
        source: sqlx::Error,
    },
    /// SQL query failed.
    #[error(transparent)]
    Sql(#[from] wyrd_sql::SqlError),
    /// Storage boot configuration failed to parse.
    #[error("config parse failed for `{var}`: {source}")]
    ConfigParse {
        /// Environment variable name.
        var: &'static str,
        /// Parse failure.
        #[source]
        source: ConfigParseError,
    },
    /// URI validation failed.
    #[error("invalid uri: {0}")]
    InvalidUri(String),
    /// Tenant prefix in a substrate URI was invalid.
    #[error("tenant prefix invalid: {0}")]
    TenantPrefixInvalid(String),
    /// Tenant prefix belonged to another tenant.
    #[error("tenant prefix foreign: prefix={prefix}, caller={caller}")]
    TenantPrefixForeign {
        /// Prefix tenant.
        prefix: DataTenantId,
        /// Caller tenant.
        caller: DataTenantId,
    },
    /// S3-specific error.
    #[error(transparent)]
    S3(#[from] Box<S3Error>),
    /// GCS-specific error.
    #[error(transparent)]
    Gcs(#[from] Box<GcsError>),
    /// Azure-specific error.
    #[error(transparent)]
    Azure(#[from] Box<AzureError>),
    /// Local-specific error.
    #[error(transparent)]
    Local(#[from] LocalError),
    /// Filesystem error.
    #[error(transparent)]
    Io(#[from] std::io::Error),
    /// HTTP client error.
    #[error(transparent)]
    Reqwest(#[from] reqwest::Error),
}

/// AWS S3 backend errors.
#[derive(Debug, thiserror::Error)]
pub enum S3Error {
    /// Object was not found.
    #[error("s3 object not found at {storage_path}")]
    NoSuchKey {
        /// Tenant-scoped object path.
        storage_path: String,
    },
    /// Lifecycle preflight failed.
    #[error("s3 lifecycle missing AbortIncompleteMultipartUpload")]
    Lifecycle,
    /// Backend throttled the request.
    #[error("s3 throttled request")]
    Throttled,
    /// Presigning failed.
    #[error("s3 presign failed: {0}")]
    Presign(String),
    /// Create-multipart response did not include an upload id.
    #[error("s3 create multipart response did not include upload_id")]
    MissingUploadId,
    /// A part-upload response did not include an `ETag`.
    #[error("s3 part upload response did not include etag")]
    MissingEtag,
    /// SDK call failed.
    #[error("s3 sdk error: {0}")]
    Sdk(String),
}

/// Google Cloud Storage backend errors.
#[derive(Debug, thiserror::Error)]
pub enum GcsError {
    /// Object was not found.
    #[error("gcs object not found at {storage_path}")]
    NotFound {
        /// Tenant-scoped object path.
        storage_path: String,
    },
    /// Backend throttled the request.
    #[error("gcs throttled request")]
    Throttled,
    /// Resumable upload init did not return a session URL.
    #[error("gcs resumable upload did not return a session url")]
    MissingSessionUrl,
    /// SDK call failed.
    #[error("gcs sdk error: {0}")]
    Sdk(String),
}

/// Azure Blob Storage backend errors.
#[derive(Debug, thiserror::Error)]
pub enum AzureError {
    /// Blob was not found.
    #[error("azure blob not found at {storage_path}")]
    BlobNotFound {
        /// Tenant-scoped object path.
        storage_path: String,
    },
    /// Backend throttled the request.
    #[error("azure throttled request")]
    Throttled,
    /// Storage account setting was missing.
    #[error("azure storage account missing")]
    MissingAccount,
    /// SDK call failed.
    #[error("azure sdk error: {0}")]
    Sdk(String),
}

/// Local filesystem backend errors.
#[derive(Debug, thiserror::Error)]
pub enum LocalError {
    /// Object was not found.
    #[error("local object not found at {storage_path}")]
    NotFound {
        /// Tenant-scoped object path.
        storage_path: String,
    },
    /// Root path is not absolute.
    #[error("local root path not absolute: {0}")]
    NonAbsoluteRoot(String),
    /// Root path does not exist.
    #[error("local root path does not exist: {0}")]
    RootNotFound(String),
    /// Filesystem call failed.
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

impl LocalError {
    /// Classify a filesystem error for a storage path.
    #[must_use]
    pub fn from_io(source: std::io::Error, storage_path: &str) -> Self {
        if source.kind() == std::io::ErrorKind::NotFound {
            Self::NotFound {
                storage_path: storage_path.to_owned(),
            }
        } else {
            Self::Io(source)
        }
    }
}
