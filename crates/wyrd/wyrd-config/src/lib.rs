//! Wyrd workspace configuration.
//!
//! Parses `wyrd.toml` and merges its defaults into card metadata
//! before the CLI or SDK sends a card to the server. Loader-side
//! only — the server never reads this file.
//!
//! See `architecture/wyrd-design.md` §"Workspace config (`wyrd.toml`)".

#![deny(missing_docs)]

use std::sync::RwLock;

mod config;
mod discovery;
mod error;
mod merge;

#[cfg(feature = "python")]
mod py;

pub use config::{Defaults, KindOverride, WyrdConfig};
pub use error::WyrdConfigError;
pub use merge::apply_defaults;

type CachedResult = Result<Option<WyrdConfig>, WyrdConfigError>;

static REPO_CONFIG: RwLock<Option<CachedResult>> = RwLock::new(None);

/// Resolve the repository `wyrd.toml` once per process.
///
/// Both successful loads and errors are cached. The returned configuration is
/// cloned so tests can safely reset the cache without invalidating a caller's
/// value.
pub fn resolve_repo_config() -> Result<Option<WyrdConfig>, WyrdConfigError> {
    let mut cache = REPO_CONFIG
        .write()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if let Some(result) = cache.as_ref() {
        return result.clone();
    }
    let result = config::WyrdConfig::load(None).map(|config| {
        if config.root_path.is_some() {
            Some(config)
        } else {
            None
        }
    });
    let returned = result.clone();
    *cache = Some(result);
    returned
}

/// Clear the repository config cache for tests that change the working tree.
#[cfg(test)]
pub fn reset_repo_config_cache_for_tests() {
    let mut cache = REPO_CONFIG
        .write()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    *cache = None;
}

#[cfg(test)]
mod cache_tests {
    use std::fs;

    use tempfile::TempDir;

    use super::{WyrdConfigError, reset_repo_config_cache_for_tests, resolve_repo_config};

    /// Run a repository-cache assertion from a temporary git root.
    fn in_temp_repo(contents: Option<&str>, test: impl FnOnce()) {
        let original = std::env::current_dir().unwrap();
        let directory = TempDir::new().unwrap();
        fs::create_dir(directory.path().join(".git")).unwrap();
        if let Some(contents) = contents {
            fs::write(directory.path().join("wyrd.toml"), contents).unwrap();
        }
        std::env::set_current_dir(directory.path()).unwrap();
        reset_repo_config_cache_for_tests();
        test();
        reset_repo_config_cache_for_tests();
        std::env::set_current_dir(original).unwrap();
    }

    #[test]
    #[serial_test::serial]
    fn missing_repo_config_is_cached_as_none() {
        in_temp_repo(None, || {
            assert!(matches!(resolve_repo_config(), Ok(None)));
            assert!(matches!(resolve_repo_config(), Ok(None)));
        });
    }

    #[test]
    #[serial_test::serial]
    fn valid_repo_config_is_cached() {
        in_temp_repo(Some("[defaults]\nspace = \"journey\"\n"), || {
            let first = resolve_repo_config().unwrap().unwrap();
            let second = resolve_repo_config().unwrap().unwrap();
            assert_eq!(first.defaults.space, second.defaults.space);
            assert_eq!(first.root_path, second.root_path);
        });
    }

    #[test]
    #[serial_test::serial]
    fn invalid_repo_config_error_is_cached() {
        in_temp_repo(Some("[defaults\n"), || {
            let first = resolve_repo_config().unwrap_err();
            let second = resolve_repo_config().unwrap_err();
            assert!(matches!(first, WyrdConfigError::TomlParse { .. }));
            assert!(matches!(second, WyrdConfigError::TomlParse { .. }));
        });
    }
}

#[cfg(feature = "python")]
pub use py::register;

#[cfg(test)]
mod e2e {
    use std::collections::BTreeMap;
    use std::io::Write;

    use tempfile::NamedTempFile;
    use wyrd_spec::envelope::{CardKind, Metadata};
    use wyrd_spec::ids::{CardName, SpaceName};
    use wyrd_spec::metadata::{AnnotationKey, AnnotationValue, LabelKey, LabelValue};

    use crate::{WyrdConfig, apply_defaults};

    const FIXTURE: &str = r#"
[defaults]
space = "prod"

[defaults.labels]
team = "platform"
env = "production"

[defaults.annotations]
"org.wyrd/owner" = "ml-platform"

[kind.Model]
space = "ml-prod"

[kind.Model.labels]
domain = "ml"
env = "ml-prod"

[kind.Policy]
space = "gov-prod"
"#;

    fn load_fixture() -> WyrdConfig {
        let mut tmp = NamedTempFile::new().unwrap();
        tmp.write_all(FIXTURE.as_bytes()).unwrap();
        WyrdConfig::load(Some(tmp.path())).unwrap()
    }

    fn lk(k: &str) -> LabelKey {
        LabelKey::new(k).unwrap()
    }

    fn fresh_meta(name: &str) -> Metadata {
        Metadata {
            name: CardName::new(name).unwrap(),
            version: None,
            bump: None,
            space: None,
            uid: None,
            labels: BTreeMap::default(),
            annotations: BTreeMap::default(),
            spec_hash: None,
            artifact_hash: None,
            origin: None,
        }
    }

    #[test]
    fn e2e_root_path_is_set_after_load() {
        let mut tmp = NamedTempFile::new().unwrap();
        tmp.write_all(FIXTURE.as_bytes()).unwrap();
        let cfg = WyrdConfig::load(Some(tmp.path())).unwrap();
        assert_eq!(cfg.root_path.as_deref(), Some(tmp.path()));
    }

    #[test]
    fn e2e_model_card_gets_kind_space_and_merged_labels() {
        let cfg = load_fixture();
        let mut meta = fresh_meta("churn-classifier");
        apply_defaults(&mut meta, &CardKind::Model, &cfg);

        assert_eq!(
            meta.space.as_ref().map(SpaceName::as_str),
            Some("ml-prod"),
            "kind override space should win over defaults"
        );
        assert_eq!(
            meta.labels.get(&lk("domain")).map(LabelValue::as_str),
            Some("ml"),
            "kind label 'domain' should be applied"
        );
        assert_eq!(
            meta.labels.get(&lk("env")).map(LabelValue::as_str),
            Some("ml-prod"),
            "kind label 'env' should win over default label"
        );
        assert_eq!(
            meta.labels.get(&lk("team")).map(LabelValue::as_str),
            Some("platform"),
            "default label 'team' should be filled when kind does not set it"
        );
        assert_eq!(
            meta.annotations
                .get(&"org.wyrd/owner".parse::<AnnotationKey>().unwrap())
                .map(AnnotationValue::as_str),
            Some("ml-platform"),
            "default annotation should be applied"
        );
    }

    #[test]
    fn e2e_card_space_wins_over_kind_and_defaults() {
        let cfg = load_fixture();
        let mut meta = fresh_meta("churn-classifier");
        meta.space = Some(SpaceName::new("eval").unwrap());
        apply_defaults(&mut meta, &CardKind::Model, &cfg);

        assert_eq!(
            meta.space.as_ref().map(SpaceName::as_str),
            Some("eval"),
            "card-level space must not be overwritten"
        );
    }

    #[test]
    fn e2e_policy_card_gets_policy_kind_space() {
        let cfg = load_fixture();
        let mut meta = fresh_meta("data-retention");
        apply_defaults(&mut meta, &CardKind::Policy, &cfg);

        assert_eq!(meta.space.as_ref().map(SpaceName::as_str), Some("gov-prod"),);
        assert_eq!(
            meta.labels.get(&lk("team")).map(LabelValue::as_str),
            Some("platform"),
            "defaults labels should still apply when kind has no label overrides"
        );
    }

    #[test]
    fn e2e_unconfigured_kind_falls_back_to_defaults() {
        let cfg = load_fixture();
        let mut meta = fresh_meta("embeddings");
        apply_defaults(&mut meta, &CardKind::Data, &cfg);

        assert_eq!(
            meta.space.as_ref().map(SpaceName::as_str),
            Some("prod"),
            "dataset has no kind override so defaults space applies"
        );
        assert_eq!(
            meta.labels.get(&lk("team")).map(LabelValue::as_str),
            Some("platform"),
        );
    }

    #[test]
    fn e2e_card_label_wins_over_kind_and_defaults() {
        let cfg = load_fixture();
        let mut meta = fresh_meta("churn-classifier");
        meta.labels
            .insert(lk("env"), LabelValue::new("staging").unwrap());
        apply_defaults(&mut meta, &CardKind::Model, &cfg);

        assert_eq!(
            meta.labels.get(&lk("env")).map(LabelValue::as_str),
            Some("staging"),
            "card's own label must not be overwritten"
        );
    }

    #[test]
    fn e2e_name_and_version_are_never_touched() {
        let cfg = load_fixture();
        let mut meta = fresh_meta("churn-classifier");
        apply_defaults(&mut meta, &CardKind::Model, &cfg);

        assert_eq!(meta.name.as_str(), "churn-classifier");
        assert!(meta.version.is_none());
    }
}
