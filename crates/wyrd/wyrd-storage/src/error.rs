//! Internal storage errors.

use wyrd_spec::DataTenantId;
use wyrd_spec::error::storage::WyrdStorageError;
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
    /// Process-wide Rustls provider ownership conflicts with Wyrd.
    #[error(transparent)]
    CryptoProvider(#[from] wyrd_tls::InstallError),
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

impl From<StorageError> for WyrdStorageError {
    fn from(error: StorageError) -> Self {
        match error {
            StorageError::CryptoProvider(error) => Self::Backend {
                detail: error.to_string(),
            },
            StorageError::TenantPathMismatch(detail) => Self::TenantPathMismatch { detail },
            StorageError::ArtifactTooLarge { actual, limit } => {
                Self::ArtifactTooLarge { actual, limit }
            }
            StorageError::InvalidExpectedSha256(detail) => Self::Sha256Invalid { detail },
            StorageError::InvalidExpectedSize(value) => Self::SizeInvalid(value),
            StorageError::EncryptionMissing => Self::EncryptionMissing,
            StorageError::Sha256Mismatch { expected, actual } => {
                Self::Sha256Mismatch { expected, actual }
            }
            StorageError::SizeMismatch { expected, actual } => {
                Self::SizeMismatch { expected, actual }
            }
            StorageError::CredentialChain(backend) => Self::CredentialChain {
                backend: backend.to_owned(),
            },
            StorageError::LifecycleRuleMissing(_) => Self::LifecycleMissing,
            StorageError::PresignExpired(detail) => Self::PresignExpired { detail },
            StorageError::ObjectNotFound { storage_path } => Self::ObjectNotFound { storage_path },
            StorageError::BackendCapabilityMismatch { signer, op } => {
                tracing::error!(
                    ?signer,
                    op,
                    error_class = "capability_mismatch",
                    "backend signer does not support requested operation"
                );
                Self::Backend {
                    detail: format!("capability mismatch: {signer:?} does not support {op}"),
                }
            }
            StorageError::Backend {
                backend,
                op,
                message,
            } => {
                tracing::error!(
                    ?backend,
                    op,
                    message,
                    error_class = "backend",
                    "storage backend operation failed"
                );
                Self::Backend {
                    detail: format!("{backend:?} {op}: {message}"),
                }
            }
            StorageError::AdminPool { source } => {
                tracing::warn!(
                    error = ?source,
                    error_class = "admin_pool",
                    "storage admin pool unavailable"
                );
                Self::BackendUnavailable { status: 503 }
            }
            StorageError::Sql(error) => map_sql_error(error),
            StorageError::ConfigParse { var, source } => {
                tracing::error!(
                    var,
                    error = ?source,
                    error_class = "config_parse",
                    "storage configuration parse failed"
                );
                Self::ConfigInvalid {
                    detail: format!("{var}: {source}"),
                }
            }
            StorageError::InvalidUri(detail) => Self::InvalidUri { detail },
            StorageError::TenantPrefixInvalid(detail) => Self::TenantPrefixInvalid { detail },
            StorageError::TenantPrefixForeign { prefix, caller } => {
                tracing::warn!(
                    %prefix,
                    %caller,
                    error_class = "tenant_prefix_foreign",
                    "storage URI prefix does not match caller tenant"
                );
                Self::TenantPathForeign
            }
            StorageError::S3(error) => map_s3_error(*error),
            StorageError::Gcs(error) => map_gcs_error(*error),
            StorageError::Azure(error) => map_azure_error(*error),
            StorageError::Local(error) => map_local_error(error),
            StorageError::Io(error) => {
                tracing::error!(error = ?error, error_class = "io", "storage IO error");
                Self::Backend {
                    detail: format!("io: {error}"),
                }
            }
            StorageError::Reqwest(error) => map_reqwest_error(&error),
        }
    }
}

fn map_sql_error(error: wyrd_sql::SqlError) -> WyrdStorageError {
    match error {
        wyrd_sql::SqlError::RlsDenied { detail } => {
            tracing::warn!(
                detail,
                error_class = "sql_rls_denied",
                "storage SQL tenant isolation rejected request"
            );
            WyrdStorageError::TenantPathForeign
        }
        other => {
            tracing::error!(
                error = ?other,
                error_class = "sql",
                "storage SQL error mapped to public backend failure"
            );
            WyrdStorageError::Backend {
                detail: "storage sql error".to_owned(),
            }
        }
    }
}

fn map_s3_error(error: S3Error) -> WyrdStorageError {
    match error {
        S3Error::NoSuchKey { storage_path } => WyrdStorageError::ObjectNotFound { storage_path },
        S3Error::Lifecycle => WyrdStorageError::LifecycleMissing,
        S3Error::Throttled => WyrdStorageError::BackendUnavailable { status: 503 },
        S3Error::Presign(detail) => WyrdStorageError::Backend {
            detail: format!("s3 presign: {detail}"),
        },
        S3Error::MissingUploadId => WyrdStorageError::Backend {
            detail: "s3 create multipart response did not include upload_id".to_owned(),
        },
        S3Error::MissingEtag => WyrdStorageError::Backend {
            detail: "s3 part upload response did not include etag".to_owned(),
        },
        S3Error::Sdk(detail) => {
            tracing::error!(
                detail,
                backend = "s3",
                error_class = "sdk",
                "s3 operation failed"
            );
            WyrdStorageError::Backend {
                detail: "s3 operation failed".to_owned(),
            }
        }
    }
}

fn map_gcs_error(error: GcsError) -> WyrdStorageError {
    match error {
        GcsError::NotFound { storage_path } => WyrdStorageError::ObjectNotFound { storage_path },
        GcsError::Throttled => WyrdStorageError::BackendUnavailable { status: 503 },
        GcsError::MissingSessionUrl => WyrdStorageError::Backend {
            detail: "gcs resumable upload did not return a session url".to_owned(),
        },
        GcsError::Sdk(detail) => {
            tracing::error!(
                detail,
                backend = "gcs",
                error_class = "sdk",
                "gcs operation failed"
            );
            WyrdStorageError::Backend {
                detail: "gcs operation failed".to_owned(),
            }
        }
    }
}

fn map_azure_error(error: AzureError) -> WyrdStorageError {
    match error {
        AzureError::BlobNotFound { storage_path } => {
            WyrdStorageError::ObjectNotFound { storage_path }
        }
        AzureError::Throttled => WyrdStorageError::BackendUnavailable { status: 503 },
        AzureError::MissingAccount => WyrdStorageError::ConfigInvalid {
            detail: "azure storage account missing".to_owned(),
        },
        AzureError::Sdk(detail) => {
            tracing::error!(
                detail,
                backend = "azure",
                error_class = "sdk",
                "azure operation failed"
            );
            WyrdStorageError::Backend {
                detail: "azure operation failed".to_owned(),
            }
        }
    }
}

fn map_local_error(error: LocalError) -> WyrdStorageError {
    match error {
        LocalError::NotFound { storage_path } => WyrdStorageError::ObjectNotFound { storage_path },
        LocalError::NonAbsoluteRoot(path) => WyrdStorageError::ConfigInvalid {
            detail: format!("local root path is not absolute: {path}"),
        },
        LocalError::RootNotFound(path) => WyrdStorageError::ConfigInvalid {
            detail: format!("local root path does not exist: {path}"),
        },
        LocalError::Io(error) => {
            tracing::error!(error = ?error, backend = "local", error_class = "io", "local io operation failed");
            WyrdStorageError::Backend {
                detail: "local io operation failed".to_owned(),
            }
        }
    }
}

fn map_reqwest_error(error: &reqwest::Error) -> WyrdStorageError {
    if error.is_timeout() || error.is_connect() {
        tracing::warn!(
            error = ?error,
            error_class = "http_transient",
            "storage HTTP transport failed transiently"
        );
        return WyrdStorageError::BackendUnavailable { status: 503 };
    }

    tracing::error!(
        error = ?error,
        error_class = "http",
        "storage HTTP transport failed"
    );
    WyrdStorageError::Backend {
        detail: "http transport failed".to_owned(),
    }
}
