use wyrd_storage::error::{ConfigParseError, StorageError};
use wyrd_storage::settings::{
    BackendConfig, DEFAULT_PART_SIZE_BYTES, DEFAULT_PRESIGN_TTL_SECS, MAX_PRESIGN_TTL_SECS,
    MIN_PRESIGN_TTL_SECS, from_env,
};

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
