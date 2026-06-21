//! Typed storage settings parsed once at server boot.

use crate::env_parse::{
    env_optional, env_required, parse_bool, parse_u32_clamped, parse_u64_clamped,
};
use crate::error::{ConfigParseError, StorageError};
use std::path::PathBuf;
use std::time::Duration;
use wyrd_spec::storage::StorageBackendKind;

/// Default presign TTL in seconds.
pub const DEFAULT_PRESIGN_TTL_SECS: u32 = 600;
/// Minimum presign TTL in seconds.
pub const MIN_PRESIGN_TTL_SECS: u32 = 60;
/// Maximum presign TTL in seconds.
pub const MAX_PRESIGN_TTL_SECS: u32 = 3600;
/// Default upload part size in bytes.
pub const DEFAULT_PART_SIZE_BYTES: u64 = 16 * 1024 * 1024;

const MIB: u64 = 1024 * 1024;

/// Process-level storage settings.
#[derive(Debug, Clone)]
pub struct StorageSettings {
    /// Backend-specific settings.
    pub backend: BackendConfig,
    /// Require a backend encryption marker during upload completion.
    pub require_encryption: bool,
    /// Presigned URL TTL.
    pub presign_ttl: Duration,
    /// Default multipart part size.
    pub part_size_bytes: u64,
    /// Object size at or above which cloud backends switch to multipart upload.
    pub multipart_threshold_bytes: u64,
    /// Public server base URL used by local-mode routes.
    pub public_base_url: Option<String>,
}

/// Backend-specific storage configuration.
#[derive(Debug, Clone)]
pub enum BackendConfig {
    /// Local filesystem backend.
    Local {
        /// Absolute local storage root.
        root: PathBuf,
    },
    /// S3 backend.
    S3(S3Config),
    /// GCS backend.
    Gcs(GcsConfig),
    /// Azure backend.
    Azure(AzureConfig),
}

impl BackendConfig {
    /// Return the backend kind.
    #[must_use]
    pub fn kind(&self) -> StorageBackendKind {
        match self {
            Self::Local { .. } => StorageBackendKind::Local,
            Self::S3(_) => StorageBackendKind::S3,
            Self::Gcs(_) => StorageBackendKind::Gcs,
            Self::Azure(_) => StorageBackendKind::Azure,
        }
    }
}

/// S3 storage settings.
#[derive(Debug, Clone)]
pub struct S3Config {
    /// Bucket name without an `s3://` prefix.
    pub bucket: String,
    /// Optional region override.
    pub region: Option<String>,
    /// Optional endpoint URL for S3-compatible backends.
    pub endpoint_url: Option<String>,
    /// Force path-style addressing.
    pub force_path_style: bool,
}

/// GCS storage settings.
#[derive(Debug, Clone)]
pub struct GcsConfig {
    /// Bucket name without a `gs://` prefix.
    pub bucket: String,
}

/// Azure storage settings.
#[derive(Debug, Clone)]
pub struct AzureConfig {
    /// Storage account name.
    pub account: String,
    /// Blob container name.
    pub container: String,
}

/// Parse process environment into typed storage settings.
///
/// # Errors
/// Returns [`StorageError::ConfigParse`] with the exact variable name when an
/// environment variable is missing, malformed, or fails storage boot
/// validation.
pub fn from_env() -> Result<StorageSettings, StorageError> {
    let backend_value = env_required("WYRD_STORAGE_BACKEND")?;
    let backend = match backend_value.as_str() {
        "local" => BackendConfig::Local {
            root: local_root_from_env()?,
        },
        "s3" => BackendConfig::S3(S3Config {
            bucket: env_required("WYRD_STORAGE_S3_BUCKET")?,
            region: env_optional("WYRD_STORAGE_S3_REGION")?,
            endpoint_url: env_optional("WYRD_STORAGE_S3_ENDPOINT_URL")?,
            force_path_style: parse_bool("WYRD_STORAGE_S3_FORCE_PATH_STYLE", false)?,
        }),
        "gcs" => BackendConfig::Gcs(GcsConfig {
            bucket: env_required("WYRD_STORAGE_GCS_BUCKET")?,
        }),
        "azure" => BackendConfig::Azure(AzureConfig {
            account: env_required("WYRD_STORAGE_AZURE_ACCOUNT")?,
            container: env_required("WYRD_STORAGE_AZURE_CONTAINER")?,
        }),
        other => {
            return config_err(
                "WYRD_STORAGE_BACKEND",
                ConfigParseError::UnknownValue {
                    value: other.to_owned(),
                    expected: "local, s3, gcs, or azure",
                },
            );
        }
    };

    let require_encryption = parse_bool("WYRD_STORAGE_REQUIRE_ENCRYPTION", false)?;
    let presign_ttl_secs = parse_u32_clamped(
        "WYRD_STORAGE_PRESIGN_TTL_SECS",
        DEFAULT_PRESIGN_TTL_SECS,
        MIN_PRESIGN_TTL_SECS,
        MAX_PRESIGN_TTL_SECS,
    )?;
    let part_size_bytes = parse_u64_clamped(
        "WYRD_STORAGE_PART_SIZE_BYTES",
        DEFAULT_PART_SIZE_BYTES,
        crate::plan::MIN_PART_SIZE_BYTES,
        crate::plan::MAX_PART_SIZE_BYTES,
    )?;
    if part_size_bytes % MIB != 0 {
        return config_err(
            "WYRD_STORAGE_PART_SIZE_BYTES",
            ConfigParseError::PartSizeNotMiBAligned(part_size_bytes),
        );
    }
    let multipart_threshold_bytes = parse_u64_clamped(
        "WYRD_STORAGE_MULTIPART_THRESHOLD_BYTES",
        crate::plan::MULTIPART_THRESHOLD_BYTES,
        crate::plan::MIN_PART_SIZE_BYTES,
        crate::plan::MAX_OBJECT_SIZE_BYTES,
    )?;

    let public_base_url = env_optional("WYRD_PUBLIC_BASE_URL")?;
    if matches!(backend, BackendConfig::Local { .. }) && public_base_url.is_none() {
        return config_err(
            "WYRD_PUBLIC_BASE_URL",
            ConfigParseError::MissingEnv(std::env::VarError::NotPresent),
        );
    }

    Ok(StorageSettings {
        backend,
        require_encryption,
        presign_ttl: Duration::from_secs(u64::from(presign_ttl_secs)),
        part_size_bytes,
        multipart_threshold_bytes,
        public_base_url,
    })
}

fn local_root_from_env() -> Result<PathBuf, StorageError> {
    let root = PathBuf::from(env_required("WYRD_STORAGE_LOCAL_ROOT")?);
    if !root.is_absolute() {
        return config_err(
            "WYRD_STORAGE_LOCAL_ROOT",
            ConfigParseError::InvalidPath(format!("path is not absolute: {}", root.display())),
        );
    }
    if !root.exists() {
        return config_err(
            "WYRD_STORAGE_LOCAL_ROOT",
            ConfigParseError::InvalidPath(format!("path does not exist: {}", root.display())),
        );
    }
    Ok(root)
}

fn config_err<T>(var: &'static str, source: ConfigParseError) -> Result<T, StorageError> {
    Err(StorageError::ConfigParse { var, source })
}
