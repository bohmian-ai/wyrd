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
    /// Optional endpoint URL for GCS-compatible backends or emulators.
    pub endpoint_url: Option<String>,
}

/// Azure storage settings.
#[derive(Debug, Clone)]
pub struct AzureConfig {
    /// Storage account name.
    pub account: String,
    /// Blob container name.
    pub container: String,
    /// Optional endpoint URL for Azure-compatible backends or emulators.
    pub endpoint_url: Option<String>,
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
            endpoint_url: env_optional("WYRD_STORAGE_GCS_ENDPOINT_URL")?,
        }),
        "azure" => BackendConfig::Azure(AzureConfig {
            account: env_required("WYRD_STORAGE_AZURE_ACCOUNT")?,
            container: env_required("WYRD_STORAGE_AZURE_CONTAINER")?,
            endpoint_url: env_optional("WYRD_STORAGE_AZURE_ENDPOINT_URL")?,
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

#[cfg(test)]
mod tests {
    use super::{
        BackendConfig, DEFAULT_PART_SIZE_BYTES, DEFAULT_PRESIGN_TTL_SECS, MAX_PRESIGN_TTL_SECS,
        MIN_PRESIGN_TTL_SECS, from_env,
    };
    use crate::error::{ConfigParseError, StorageError};

    const ENV_KEYS: &[&str] = &[
        "WYRD_STORAGE_BACKEND",
        "WYRD_STORAGE_REQUIRE_ENCRYPTION",
        "WYRD_STORAGE_PRESIGN_TTL_SECS",
        "WYRD_STORAGE_PART_SIZE_BYTES",
        "WYRD_STORAGE_S3_BUCKET",
        "WYRD_STORAGE_S3_REGION",
        "WYRD_STORAGE_S3_ENDPOINT_URL",
        "WYRD_STORAGE_S3_FORCE_PATH_STYLE",
        "WYRD_STORAGE_GCS_BUCKET",
        "GOOGLE_APPLICATION_CREDENTIALS",
        "GOOGLE_APPLICATION_CREDENTIALS_JSON",
        "GOOGLE_ACCOUNT_JSON_BASE64",
        "WYRD_STORAGE_AZURE_ACCOUNT",
        "WYRD_STORAGE_AZURE_CONTAINER",
        "WYRD_STORAGE_LOCAL_ROOT",
        "WYRD_PUBLIC_BASE_URL",
    ];

    #[test]
    fn parses_local_backend_defaults() {
        let root = tempfile::tempdir().expect("temp dir");
        with_clean_env(
            vec![
                ("WYRD_STORAGE_BACKEND", Some("local".to_owned())),
                (
                    "WYRD_STORAGE_LOCAL_ROOT",
                    Some(root.path().display().to_string()),
                ),
                ("WYRD_PUBLIC_BASE_URL", Some("https://wyrd.test".to_owned())),
            ],
            || {
                let settings = from_env().expect("settings parse");
                assert!(matches!(settings.backend, BackendConfig::Local { .. }));
                assert!(!settings.require_encryption);
                assert_eq!(
                    settings.presign_ttl.as_secs(),
                    u64::from(DEFAULT_PRESIGN_TTL_SECS)
                );
                assert_eq!(settings.part_size_bytes, DEFAULT_PART_SIZE_BYTES);
                assert_eq!(
                    settings.public_base_url.as_deref(),
                    Some("https://wyrd.test")
                );
            },
        );
    }

    #[test]
    fn parses_backend_specific_settings() {
        with_clean_env(
            vec![
                ("WYRD_STORAGE_BACKEND", Some("s3".to_owned())),
                ("WYRD_STORAGE_S3_BUCKET", Some("artifacts".to_owned())),
                ("WYRD_STORAGE_S3_REGION", Some("us-east-2".to_owned())),
                (
                    "WYRD_STORAGE_S3_ENDPOINT_URL",
                    Some("http://localhost:9000".to_owned()),
                ),
                ("WYRD_STORAGE_S3_FORCE_PATH_STYLE", Some("on".to_owned())),
                ("WYRD_STORAGE_REQUIRE_ENCRYPTION", Some("yes".to_owned())),
            ],
            || {
                let settings = from_env().expect("settings parse");
                let BackendConfig::S3(config) = settings.backend else {
                    panic!("expected s3 backend");
                };
                assert_eq!(config.bucket, "artifacts");
                assert_eq!(config.region.as_deref(), Some("us-east-2"));
                assert_eq!(
                    config.endpoint_url.as_deref(),
                    Some("http://localhost:9000")
                );
                assert!(config.force_path_style);
                assert!(settings.require_encryption);
            },
        );
    }

    #[test]
    fn malformed_bool_reports_exact_var() {
        let root = tempfile::tempdir().expect("temp dir");
        with_clean_env(
            vec![
                ("WYRD_STORAGE_BACKEND", Some("local".to_owned())),
                (
                    "WYRD_STORAGE_LOCAL_ROOT",
                    Some(root.path().display().to_string()),
                ),
                ("WYRD_PUBLIC_BASE_URL", Some("https://wyrd.test".to_owned())),
                ("WYRD_STORAGE_REQUIRE_ENCRYPTION", Some("maybe".to_owned())),
            ],
            || {
                let err = from_env().expect_err("invalid bool");
                assert_config_var(&err, "WYRD_STORAGE_REQUIRE_ENCRYPTION");
                assert!(matches!(
                    err,
                    StorageError::ConfigParse {
                        source: ConfigParseError::InvalidBool(_),
                        ..
                    }
                ));
            },
        );
    }

    #[test]
    fn missing_backend_requirement_reports_exact_var() {
        with_clean_env(
            vec![
                ("WYRD_STORAGE_BACKEND", Some("s3".to_owned())),
                ("WYRD_STORAGE_S3_BUCKET", None),
            ],
            || {
                let err = from_env().expect_err("missing bucket");
                assert_config_var(&err, "WYRD_STORAGE_S3_BUCKET");
            },
        );
    }

    #[test]
    fn unknown_backend_reports_backend_var() {
        with_clean_env(
            vec![("WYRD_STORAGE_BACKEND", Some("ceph".to_owned()))],
            || {
                let err = from_env().expect_err("unknown backend");
                assert_config_var(&err, "WYRD_STORAGE_BACKEND");
            },
        );
    }

    #[test]
    fn clamps_ttl_bounds() {
        let root = tempfile::tempdir().expect("temp dir");
        with_clean_env(
            vec![
                ("WYRD_STORAGE_BACKEND", Some("local".to_owned())),
                (
                    "WYRD_STORAGE_LOCAL_ROOT",
                    Some(root.path().display().to_string()),
                ),
                ("WYRD_PUBLIC_BASE_URL", Some("https://wyrd.test".to_owned())),
                ("WYRD_STORAGE_PRESIGN_TTL_SECS", Some("5".to_owned())),
            ],
            || {
                let settings = from_env().expect("settings parse");
                assert_eq!(
                    settings.presign_ttl.as_secs(),
                    u64::from(MIN_PRESIGN_TTL_SECS)
                );
            },
        );

        with_clean_env(
            vec![
                ("WYRD_STORAGE_BACKEND", Some("local".to_owned())),
                (
                    "WYRD_STORAGE_LOCAL_ROOT",
                    Some(root.path().display().to_string()),
                ),
                ("WYRD_PUBLIC_BASE_URL", Some("https://wyrd.test".to_owned())),
                ("WYRD_STORAGE_PRESIGN_TTL_SECS", Some("999999".to_owned())),
            ],
            || {
                let settings = from_env().expect("settings parse");
                assert_eq!(
                    settings.presign_ttl.as_secs(),
                    u64::from(MAX_PRESIGN_TTL_SECS)
                );
            },
        );
    }

    #[test]
    fn rejects_part_size_not_multiple_of_mib_with_exact_var() {
        let root = tempfile::tempdir().expect("temp dir");
        with_clean_env(
            vec![
                ("WYRD_STORAGE_BACKEND", Some("local".to_owned())),
                (
                    "WYRD_STORAGE_LOCAL_ROOT",
                    Some(root.path().display().to_string()),
                ),
                ("WYRD_PUBLIC_BASE_URL", Some("https://wyrd.test".to_owned())),
                ("WYRD_STORAGE_PART_SIZE_BYTES", Some("5242881".to_owned())),
            ],
            || {
                let err = from_env().expect_err("bad part size");
                assert_config_var(&err, "WYRD_STORAGE_PART_SIZE_BYTES");
                assert!(matches!(
                    err,
                    StorageError::ConfigParse {
                        source: ConfigParseError::PartSizeNotMiBAligned(_),
                        ..
                    }
                ));
            },
        );
    }

    #[test]
    fn local_requires_public_base_url() {
        let root = tempfile::tempdir().expect("temp dir");
        with_clean_env(
            vec![
                ("WYRD_STORAGE_BACKEND", Some("local".to_owned())),
                (
                    "WYRD_STORAGE_LOCAL_ROOT",
                    Some(root.path().display().to_string()),
                ),
            ],
            || {
                let err = from_env().expect_err("missing public base url");
                assert_config_var(&err, "WYRD_PUBLIC_BASE_URL");
            },
        );
    }

    #[test]
    fn rejects_relative_local_root_with_exact_var() {
        with_clean_env(
            vec![
                ("WYRD_STORAGE_BACKEND", Some("local".to_owned())),
                ("WYRD_STORAGE_LOCAL_ROOT", Some("relative/root".to_owned())),
                ("WYRD_PUBLIC_BASE_URL", Some("https://wyrd.test".to_owned())),
            ],
            || {
                let err = from_env().expect_err("relative root");
                assert_config_var(&err, "WYRD_STORAGE_LOCAL_ROOT");
                assert!(matches!(
                    err,
                    StorageError::ConfigParse {
                        source: ConfigParseError::InvalidPath(_),
                        ..
                    }
                ));
            },
        );
    }

    #[test]
    fn rejects_nonexistent_local_root_with_exact_var() {
        let parent = tempfile::tempdir().expect("temp dir");
        let missing = parent.path().join("missing-root");
        with_clean_env(
            vec![
                ("WYRD_STORAGE_BACKEND", Some("local".to_owned())),
                (
                    "WYRD_STORAGE_LOCAL_ROOT",
                    Some(missing.display().to_string()),
                ),
                ("WYRD_PUBLIC_BASE_URL", Some("https://wyrd.test".to_owned())),
            ],
            || {
                let err = from_env().expect_err("nonexistent root");
                assert_config_var(&err, "WYRD_STORAGE_LOCAL_ROOT");
                assert!(matches!(
                    err,
                    StorageError::ConfigParse {
                        source: ConfigParseError::InvalidPath(_),
                        ..
                    }
                ));
            },
        );
    }

    fn with_clean_env(vars: Vec<(&'static str, Option<String>)>, f: impl FnOnce()) {
        let provided = vars.iter().map(|(key, _)| *key).collect::<Vec<_>>();
        let mut all = ENV_KEYS
            .iter()
            .filter(|key| !provided.contains(key))
            .map(|key| (*key, None))
            .collect::<Vec<(&'static str, Option<String>)>>();
        all.extend(vars);
        temp_env::with_vars(all, f);
    }

    fn assert_config_var(err: &StorageError, expected: &'static str) {
        match err {
            StorageError::ConfigParse { var, .. } => assert_eq!(*var, expected),
            other => panic!("expected ConfigParse for {expected}, got {other:?}"),
        }
    }
}
