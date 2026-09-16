//! Typed storage settings parsed once at server boot.

use crate::env_parse::{
    env_optional, env_required, parse_bool, parse_u32_clamped, parse_u64_clamped,
};
use crate::error::{ConfigParseError, StorageError};
use std::path::PathBuf;
use std::time::Duration;
use url::Url;
use wyrd_spec::storage::StorageBackendKind;

/// Default presign TTL in seconds.
pub const DEFAULT_PRESIGN_TTL_SECS: u32 = 600;
/// Minimum presign TTL in seconds.
pub const MIN_PRESIGN_TTL_SECS: u32 = 60;
/// Maximum presign TTL in seconds.
pub const MAX_PRESIGN_TTL_SECS: u32 = 3600;
/// Default upload part size in bytes.
pub const DEFAULT_PART_SIZE_BYTES: u64 = 16 * 1024 * 1024;

/// Required variable naming the storage location as `file:`, `s3:`, `gs:`, or `az:` URL.
const STORAGE_URL_VAR: &str = "WYRD_STORAGE_URL";
/// Optional variable pointing a cloud backend at an S3-compatible service or emulator.
const ENDPOINT_URL_VAR: &str = "WYRD_STORAGE_ENDPOINT_URL";

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
///
/// Derived once from `WYRD_STORAGE_URL` so the storage handle, server storage,
/// and the Iceberg catalog all bind to the same backend description.
#[derive(Debug, Clone, PartialEq, Eq)]
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

    /// Parse a storage URL and optional endpoint override into a backend.
    ///
    /// `file:///absolute/path` selects an existing local root, `s3://bucket`
    /// and `gs://bucket` select a bucket, and `az://account/container` selects
    /// an Azure container. `region` is recorded only for S3. Every URL rejects
    /// credentials, ports, queries, fragments, and extra path segments. The
    /// endpoint must be an HTTP(S) URL without credentials, query, or fragment
    /// and is refused for local storage, which has no service to address.
    ///
    /// # Errors
    /// Returns [`StorageError::ConfigParse`] naming `WYRD_STORAGE_URL` or
    /// `WYRD_STORAGE_ENDPOINT_URL` with [`ConfigParseError::InvalidUrl`] when a
    /// value violates this contract, or with [`ConfigParseError::InvalidPath`]
    /// when the local root does not exist.
    pub fn from_url(
        url: &str,
        endpoint_url: Option<String>,
        region: Option<String>,
    ) -> Result<Self, StorageError> {
        let parsed = parse_url(STORAGE_URL_VAR, url)?;
        if parsed.port().is_some() {
            return Err(url_error(STORAGE_URL_VAR, "port is not allowed"));
        }
        if let Some(endpoint) = &endpoint_url {
            validate_endpoint(endpoint)?;
        }
        match parsed.scheme() {
            "file" => {
                if endpoint_url.is_some() {
                    return Err(url_error(
                        ENDPOINT_URL_VAR,
                        "endpoint is not supported for file storage",
                    ));
                }
                Ok(Self::Local {
                    root: local_root(url, &parsed)?,
                })
            }
            "s3" => Ok(Self::S3(S3Config {
                bucket: bucket(&parsed)?,
                region,
                endpoint_url,
            })),
            "gs" => Ok(Self::Gcs(GcsConfig {
                bucket: bucket(&parsed)?,
                endpoint_url,
            })),
            "az" => {
                let account = required_host(&parsed, "account")?;
                let container = parsed
                    .path()
                    .strip_prefix('/')
                    .map(|path| path.strip_suffix('/').unwrap_or(path))
                    .filter(|path| !path.is_empty() && !path.contains('/'))
                    .ok_or_else(|| url_error(STORAGE_URL_VAR, "expected az://account/container"))?;
                Ok(Self::Azure(AzureConfig {
                    account,
                    container: container.to_owned(),
                    endpoint_url,
                }))
            }
            other => Err(url_error(
                STORAGE_URL_VAR,
                format!("unsupported scheme `{other}`; expected file, s3, gs, or az"),
            )),
        }
    }
}

/// S3 storage settings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct S3Config {
    /// Bucket name without an `s3://` prefix.
    pub bucket: String,
    /// Region from `AWS_REGION`, when set.
    pub region: Option<String>,
    /// Endpoint for S3-compatible services; its presence selects path-style addressing.
    pub endpoint_url: Option<String>,
}

/// GCS storage settings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GcsConfig {
    /// Bucket name without a `gs://` prefix.
    pub bucket: String,
    /// Emulator endpoint; honored only by builds with the `emulator` feature.
    pub endpoint_url: Option<String>,
}

/// Azure storage settings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AzureConfig {
    /// Storage account name.
    pub account: String,
    /// Blob container name.
    pub container: String,
    /// Complete emulator service URL including the account; honored only by
    /// builds with the `emulator` feature.
    pub endpoint_url: Option<String>,
}

/// Parse process environment into typed storage settings.
///
/// `WYRD_STORAGE_URL` selects the backend through [`BackendConfig::from_url`],
/// `WYRD_STORAGE_ENDPOINT_URL` optionally overrides the service endpoint, and
/// `AWS_REGION` supplies the S3 region. Tuning variables keep their defaults.
///
/// # Errors
/// Returns [`StorageError::ConfigParse`] with the exact variable name when an
/// environment variable is missing, malformed, or fails storage boot
/// validation.
pub fn from_env() -> Result<StorageSettings, StorageError> {
    let backend = BackendConfig::from_url(
        &env_required(STORAGE_URL_VAR)?,
        env_optional(ENDPOINT_URL_VAR)?,
        env_optional("AWS_REGION")?,
    )?;

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

/// Parse `value` and reject userinfo, queries, and fragments.
///
/// The rejected value is never echoed, so a mistakenly embedded secret does not
/// reach logs or error payloads.
///
/// # Errors
/// Returns an `InvalidUrl` configuration error for `var` when the value does
/// not parse or carries a username, password, query, or fragment.
fn parse_url(var: &'static str, value: &str) -> Result<Url, StorageError> {
    let url = Url::parse(value).map_err(|source| url_error(var, source.to_string()))?;
    if !url.username().is_empty() || url.password().is_some() {
        return Err(url_error(var, "credentials are not allowed"));
    }
    if url.query().is_some() || url.fragment().is_some() {
        return Err(url_error(
            var,
            "query strings and fragments are not allowed",
        ));
    }
    Ok(url)
}

/// Validate `WYRD_STORAGE_ENDPOINT_URL` as a credential-free HTTP(S) URL.
///
/// # Errors
/// Returns an `InvalidUrl` configuration error when the endpoint does not
/// parse, uses another scheme, lacks a host, or carries credentials, a query,
/// or a fragment.
fn validate_endpoint(value: &str) -> Result<(), StorageError> {
    let url = parse_url(ENDPOINT_URL_VAR, value)?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err(url_error(ENDPOINT_URL_VAR, "expected an http or https URL"));
    }
    if url.host_str().is_none_or(str::is_empty) {
        return Err(url_error(ENDPOINT_URL_VAR, "host is required"));
    }
    Ok(())
}

/// Resolve a `file:` URL into an existing absolute local root.
///
/// The raw value must spell an absolute path after `file:` because URL parsing
/// would otherwise silently anchor `file:relative` at the filesystem root.
///
/// # Errors
/// Returns an `InvalidUrl` configuration error for relative or host-bearing
/// URLs and an `InvalidPath` configuration error when the root does not exist.
fn local_root(raw: &str, url: &Url) -> Result<PathBuf, StorageError> {
    let absolute = raw
        .get(url.scheme().len() + 1..)
        .is_some_and(|rest| rest.starts_with('/'));
    if !absolute || url.host_str().is_some_and(|host| !host.is_empty()) {
        return Err(url_error(STORAGE_URL_VAR, "expected file:///absolute/path"));
    }
    let root = url
        .to_file_path()
        .map_err(|()| url_error(STORAGE_URL_VAR, "expected file:///absolute/path"))?;
    if !root.exists() {
        return config_err(
            STORAGE_URL_VAR,
            ConfigParseError::InvalidPath(format!("path does not exist: {}", root.display())),
        );
    }
    Ok(root)
}

/// Read the bucket from `s3://bucket` or `gs://bucket`.
///
/// # Errors
/// Returns an `InvalidUrl` configuration error when the bucket is missing or
/// the URL carries path segments.
fn bucket(url: &Url) -> Result<String, StorageError> {
    let bucket = required_host(url, "bucket")?;
    if !matches!(url.path(), "" | "/") {
        return Err(url_error(STORAGE_URL_VAR, "path segments are not allowed"));
    }
    Ok(bucket)
}

/// Read the non-empty URL host naming a bucket or account.
///
/// # Errors
/// Returns an `InvalidUrl` configuration error naming `what` when the host is
/// absent or empty.
fn required_host(url: &Url, what: &str) -> Result<String, StorageError> {
    url.host_str()
        .filter(|host| !host.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| url_error(STORAGE_URL_VAR, format!("{what} is required")))
}

/// Build an `InvalidUrl` configuration error for `var`.
fn url_error(var: &'static str, reason: impl Into<String>) -> StorageError {
    StorageError::ConfigParse {
        var,
        source: ConfigParseError::InvalidUrl(reason.into()),
    }
}

fn config_err<T>(var: &'static str, source: ConfigParseError) -> Result<T, StorageError> {
    Err(StorageError::ConfigParse { var, source })
}

#[cfg(test)]
mod tests {
    use super::{
        AzureConfig, BackendConfig, DEFAULT_PART_SIZE_BYTES, DEFAULT_PRESIGN_TTL_SECS,
        ENDPOINT_URL_VAR, GcsConfig, MAX_PRESIGN_TTL_SECS, MIN_PRESIGN_TTL_SECS, S3Config,
        STORAGE_URL_VAR, from_env,
    };
    use crate::error::{ConfigParseError, StorageError};
    use std::path::Path;

    /// Every variable `from_env` reads, cleared unless a test provides it.
    const ENV_KEYS: &[&str] = &[
        "WYRD_STORAGE_URL",
        "WYRD_STORAGE_ENDPOINT_URL",
        "AWS_REGION",
        "WYRD_STORAGE_REQUIRE_ENCRYPTION",
        "WYRD_STORAGE_PRESIGN_TTL_SECS",
        "WYRD_STORAGE_PART_SIZE_BYTES",
        "WYRD_STORAGE_MULTIPART_THRESHOLD_BYTES",
        "GOOGLE_APPLICATION_CREDENTIALS",
        "GOOGLE_APPLICATION_CREDENTIALS_JSON",
        "GOOGLE_ACCOUNT_JSON_BASE64",
        "WYRD_PUBLIC_BASE_URL",
    ];

    /// Every accepted storage URL maps to its backend, and every forbidden
    /// shape is refused with `InvalidUrl` naming the offending variable.
    #[test]
    fn storage_url_contract() {
        let root = tempfile::tempdir().expect("temp dir");
        let root_url = file_url(root.path());
        let local = BackendConfig::Local {
            root: root.path().to_path_buf(),
        };
        let (rustfs, fake_gcs) = (Some("http://localhost:9000"), Some("http://localhost:4443"));
        let azurite = Some("http://127.0.0.1:10000/devstoreaccount1");
        let valid = [
            (root_url.as_str(), None, local),
            ("s3://bucket", None, s3(None)),
            ("s3://bucket/", None, s3(None)),
            ("s3://bucket", rustfs, s3(rustfs)),
            ("gs://bucket", None, gcs(None)),
            ("gs://bucket", fake_gcs, gcs(fake_gcs)),
            ("az://account/container", None, azure("account", None)),
            (
                "az://devstoreaccount1/container",
                azurite,
                azure("devstoreaccount1", azurite),
            ),
        ];
        for (url, endpoint, expected) in valid {
            let backend =
                BackendConfig::from_url(url, owned(endpoint), Some("us-east-1".to_owned()))
                    .unwrap_or_else(|error| panic!("{url} must parse: {error}"));
            assert_eq!(backend, expected, "{url}");
        }

        let (url_var, endpoint_var) = (STORAGE_URL_VAR, ENDPOINT_URL_VAR);
        let rejected = [
            ("not a url", None, url_var),
            ("ceph://bucket", None, url_var),
            ("s3://", None, url_var),
            ("gs://", None, url_var),
            ("az://account", None, url_var),
            ("az://account/", None, url_var),
            ("az:///container", None, url_var),
            ("s3://bucket/extra", None, url_var),
            ("gs://bucket/extra", None, url_var),
            ("az://account/container/extra", None, url_var),
            ("s3://bucket:9000", None, url_var),
            ("file:relative/root", None, url_var),
            ("file://host/root", None, url_var),
            ("s3://user@bucket", None, url_var),
            ("s3://user:secret@bucket", None, url_var),
            ("s3://bucket?region=us-east-1", None, url_var),
            ("s3://bucket#fragment", None, url_var),
            ("s3://bucket", Some("localhost:9000"), endpoint_var),
            ("s3://bucket", Some("ftp://localhost:9000"), endpoint_var),
            (
                "s3://bucket",
                Some("http://user:pw@localhost:9000"),
                endpoint_var,
            ),
            (
                "s3://bucket",
                Some("http://localhost:9000?x=1"),
                endpoint_var,
            ),
            ("s3://bucket", Some("http://localhost:9000#x"), endpoint_var),
            (
                root_url.as_str(),
                Some("http://localhost:9000"),
                endpoint_var,
            ),
        ];
        for (url, endpoint, var) in rejected {
            let error = BackendConfig::from_url(url, owned(endpoint), None).expect_err(url);
            assert_config_var(&error, var);
            assert!(
                matches!(
                    error,
                    StorageError::ConfigParse {
                        source: ConfigParseError::InvalidUrl(_),
                        ..
                    }
                ),
                "{url} with {endpoint:?} returned {error:?}"
            );
        }
    }

    /// A local URL with a public base URL parses with every tuning default.
    #[test]
    fn parses_local_backend_defaults() {
        let root = tempfile::tempdir().expect("temp dir");
        with_clean_env(local_vars(root.path()), || {
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
        });
    }

    /// The S3 bucket, endpoint, and standard `AWS_REGION` flow into the config.
    #[test]
    fn parses_backend_specific_settings() {
        with_clean_env(
            vec![
                ("WYRD_STORAGE_URL", Some("s3://artifacts".to_owned())),
                (
                    "WYRD_STORAGE_ENDPOINT_URL",
                    Some("http://localhost:9000".to_owned()),
                ),
                ("AWS_REGION", Some("us-east-2".to_owned())),
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
                assert!(settings.require_encryption);
            },
        );
    }

    /// A malformed boolean names its own variable.
    #[test]
    fn malformed_bool_reports_exact_var() {
        let root = tempfile::tempdir().expect("temp dir");
        let mut vars = local_vars(root.path());
        vars.push(("WYRD_STORAGE_REQUIRE_ENCRYPTION", Some("maybe".to_owned())));
        with_clean_env(vars, || {
            let err = from_env().expect_err("invalid bool");
            assert_config_var(&err, "WYRD_STORAGE_REQUIRE_ENCRYPTION");
            assert!(matches!(
                err,
                StorageError::ConfigParse {
                    source: ConfigParseError::InvalidBool(_),
                    ..
                }
            ));
        });
    }

    /// Boot refuses to guess a backend when `WYRD_STORAGE_URL` is absent.
    #[test]
    fn missing_storage_url_reports_exact_var() {
        with_clean_env(Vec::new(), || {
            let err = from_env().expect_err("missing storage url");
            assert_config_var(&err, "WYRD_STORAGE_URL");
        });
    }

    /// Presign TTL values outside the supported window clamp to its bounds.
    #[test]
    fn clamps_ttl_bounds() {
        let root = tempfile::tempdir().expect("temp dir");
        for (ttl, expected) in [
            ("5", MIN_PRESIGN_TTL_SECS),
            ("999999", MAX_PRESIGN_TTL_SECS),
        ] {
            let mut vars = local_vars(root.path());
            vars.push(("WYRD_STORAGE_PRESIGN_TTL_SECS", Some(ttl.to_owned())));
            with_clean_env(vars, || {
                let settings = from_env().expect("settings parse");
                assert_eq!(settings.presign_ttl.as_secs(), u64::from(expected));
            });
        }
    }

    /// Part sizes that are not MiB-aligned name the part-size variable.
    #[test]
    fn rejects_part_size_not_multiple_of_mib_with_exact_var() {
        let root = tempfile::tempdir().expect("temp dir");
        let mut vars = local_vars(root.path());
        vars.push(("WYRD_STORAGE_PART_SIZE_BYTES", Some("5242881".to_owned())));
        with_clean_env(vars, || {
            let err = from_env().expect_err("bad part size");
            assert_config_var(&err, "WYRD_STORAGE_PART_SIZE_BYTES");
            assert!(matches!(
                err,
                StorageError::ConfigParse {
                    source: ConfigParseError::PartSizeNotMiBAligned(_),
                    ..
                }
            ));
        });
    }

    /// Local storage needs a public base URL for its download routes.
    #[test]
    fn local_requires_public_base_url() {
        let root = tempfile::tempdir().expect("temp dir");
        with_clean_env(
            vec![("WYRD_STORAGE_URL", Some(file_url(root.path())))],
            || {
                let err = from_env().expect_err("missing public base url");
                assert_config_var(&err, "WYRD_PUBLIC_BASE_URL");
            },
        );
    }

    /// A well-formed local URL whose root does not exist is an invalid path.
    #[test]
    fn rejects_nonexistent_local_root_with_exact_var() {
        let parent = tempfile::tempdir().expect("temp dir");
        with_clean_env(local_vars(&parent.path().join("missing-root")), || {
            let err = from_env().expect_err("nonexistent root");
            assert_config_var(&err, "WYRD_STORAGE_URL");
            assert!(matches!(
                err,
                StorageError::ConfigParse {
                    source: ConfigParseError::InvalidPath(_),
                    ..
                }
            ));
        });
    }

    /// Copy an optional borrowed endpoint into the owned form `from_url` takes.
    fn owned(value: Option<&str>) -> Option<String> {
        value.map(str::to_owned)
    }

    /// Expected S3 config for bucket `bucket` in `us-east-1`.
    fn s3(endpoint: Option<&str>) -> BackendConfig {
        BackendConfig::S3(S3Config {
            bucket: "bucket".to_owned(),
            region: Some("us-east-1".to_owned()),
            endpoint_url: owned(endpoint),
        })
    }

    /// Expected GCS config for bucket `bucket`.
    fn gcs(endpoint: Option<&str>) -> BackendConfig {
        BackendConfig::Gcs(GcsConfig {
            bucket: "bucket".to_owned(),
            endpoint_url: owned(endpoint),
        })
    }

    /// Expected Azure config for container `container` in `account`.
    fn azure(account: &str, endpoint: Option<&str>) -> BackendConfig {
        BackendConfig::Azure(AzureConfig {
            account: account.to_owned(),
            container: "container".to_owned(),
            endpoint_url: owned(endpoint),
        })
    }

    /// Render an absolute path as a `file:` storage URL.
    fn file_url(path: &Path) -> String {
        url::Url::from_file_path(path)
            .expect("test paths are absolute")
            .to_string()
    }

    /// Minimal environment for a local backend rooted at `root`.
    fn local_vars(root: &Path) -> Vec<(&'static str, Option<String>)> {
        vec![
            ("WYRD_STORAGE_URL", Some(file_url(root))),
            ("WYRD_PUBLIC_BASE_URL", Some("https://wyrd.test".to_owned())),
        ]
    }

    /// Run `f` with `vars` set and every other storage variable cleared.
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

    /// Assert that `err` is a configuration error for `expected`.
    fn assert_config_var(err: &StorageError, expected: &'static str) {
        match err {
            StorageError::ConfigParse { var, .. } => assert_eq!(*var, expected),
            other => panic!("expected ConfigParse for {expected}, got {other:?}"),
        }
    }
}
