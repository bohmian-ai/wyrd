//! Pure merge function. No IO, no async, no allocation surprises.

use wyrd_spec::envelope::{CardKind, Metadata};

use crate::config::{KindOverride, WyrdConfig};

/// Splice workspace defaults into a card's metadata.
///
/// Precedence (most specific wins):
/// 1. Card YAML (already in `meta` — untouched if set).
/// 2. `cfg.kind_overrides[kind]`.
/// 3. `cfg.defaults`.
///
/// Maps merge per-key (insert-if-absent). Scalars fill only when
/// `None`. The card's own values are never overwritten.
///
/// `meta.name`, `meta.version`, `meta.uid`, `meta.bump`,
/// `meta.spec_hash`, and `meta.artifact_hash` are never read or
/// written by this function. The first three are author
/// identity / intent (lock L4 plus Q5 for `version`); the last three
/// are server-derived.
///
/// # Examples
///
/// ```
/// use wyrd_config::{WyrdConfig, apply_defaults};
/// use wyrd_spec::envelope::{CardKind, Metadata};
/// use wyrd_spec::ids::{CardName, SpaceName};
///
/// let mut cfg = WyrdConfig::empty();
/// cfg.defaults.space = Some(SpaceName::new("prod").unwrap());
///
/// let mut meta = Metadata {
///     name: CardName::new("churn-classifier").unwrap(),
///     version: None,
///     bump: None,
///     space: None,
///     uid: None,
///     labels: Default::default(),
///     annotations: Default::default(),
///     spec_hash: None,
///     artifact_hash: None,
///     origin: None,
/// };
/// apply_defaults(&mut meta, &CardKind::Model, &cfg);
/// assert_eq!(meta.space.as_ref().map(|s| s.as_str()), Some("prod"));
/// // version is never synthesized by the loader (Q5 / L7).
/// assert!(meta.version.is_none());
/// ```
pub fn apply_defaults(meta: &mut Metadata, kind: &CardKind, cfg: &WyrdConfig) {
    let kind_override: Option<&KindOverride> = cfg.kind_overrides.get(kind);

    // Scalar: space
    if meta.space.is_none() {
        if let Some(s) = kind_override.and_then(|k| k.space.clone()) {
            meta.space = Some(s);
        } else if let Some(s) = cfg.defaults.space.clone() {
            meta.space = Some(s);
        }
    }

    // Maps: kind first, then defaults; insert-if-absent so the
    // card's own keys always win.
    if let Some(k) = kind_override {
        for (key, val) in &k.labels {
            meta.labels
                .entry(key.clone())
                .or_insert_with(|| val.clone());
        }
        for (key, val) in &k.annotations {
            meta.annotations
                .entry(key.clone())
                .or_insert_with(|| val.clone());
        }
    }
    for (key, val) in &cfg.defaults.labels {
        meta.labels
            .entry(key.clone())
            .or_insert_with(|| val.clone());
    }
    for (key, val) in &cfg.defaults.annotations {
        meta.annotations
            .entry(key.clone())
            .or_insert_with(|| val.clone());
    }

    // Author-identity / intent fields are never touched. Server-
    // derived fields are never touched either.
    let _ = (
        &meta.name,
        &meta.version,
        &meta.uid,
        &meta.bump,
        &meta.spec_hash,
        &meta.artifact_hash,
    );
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;
    use wyrd_spec::ids::{CardName, SpaceName};
    use wyrd_spec::metadata::{LabelKey, LabelValue};

    fn label_pair(k: &str, v: &str) -> (LabelKey, LabelValue) {
        (LabelKey::new(k).unwrap(), LabelValue::new(v).unwrap())
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
    fn apply_defaults_is_idempotent() {
        let mut cfg = WyrdConfig::empty();
        cfg.defaults.space = Some(SpaceName::new("prod").unwrap());
        let (k, v) = label_pair("team", "ml");
        cfg.defaults.labels.insert(k, v);

        let mut meta = fresh_meta("card");
        apply_defaults(&mut meta, &CardKind::Model, &cfg);
        let snapshot = meta.clone();
        apply_defaults(&mut meta, &CardKind::Model, &cfg);
        assert_eq!(meta.space, snapshot.space);
        assert_eq!(meta.labels, snapshot.labels);
    }

    #[test]
    fn card_label_wins_over_default() {
        let mut cfg = WyrdConfig::empty();
        let (k, v) = label_pair("team", "platform");
        cfg.defaults.labels.insert(k.clone(), v);

        let mut meta = fresh_meta("card");
        meta.labels
            .insert(k.clone(), LabelValue::new("churn-ml").unwrap());
        apply_defaults(&mut meta, &CardKind::Model, &cfg);
        assert_eq!(
            meta.labels.get(&k).map(LabelValue::as_str),
            Some("churn-ml")
        );
    }
}

#[cfg(test)]
mod merge_tests {
    use std::collections::BTreeMap;

    use wyrd_spec::envelope::{CardKind, Metadata};
    use wyrd_spec::ids::{CardName, SpaceName};
    use wyrd_spec::metadata::{LabelKey, LabelValue};

    use crate::{WyrdConfig, apply_defaults};

    fn lk(k: &str) -> LabelKey {
        LabelKey::new(k).unwrap()
    }
    fn lv(v: &str) -> LabelValue {
        LabelValue::new(v).unwrap()
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
    fn merge_all_set_card_full_config_card_wins() {
        let mut cfg = WyrdConfig::empty();
        cfg.defaults.space = Some(SpaceName::new("prod").unwrap());
        cfg.defaults.labels.insert(lk("team"), lv("platform"));

        let mut ko = crate::KindOverride {
            space: Some(SpaceName::new("ml-prod").unwrap()),
            ..Default::default()
        };
        ko.labels.insert(lk("domain"), lv("ml"));
        cfg.kind_overrides.insert(CardKind::Model, ko);

        let mut meta = fresh_meta("churn");
        meta.space = Some(SpaceName::new("eval").unwrap());
        meta.labels.insert(lk("team"), lv("ml-team"));

        let orig_version = meta.version.clone();
        apply_defaults(&mut meta, &CardKind::Model, &cfg);

        assert_eq!(meta.space.as_ref().map(SpaceName::as_str), Some("eval"));
        assert_eq!(meta.version, orig_version);
        assert_eq!(
            meta.labels.get(&lk("team")).map(LabelValue::as_str),
            Some("ml-team")
        );
        assert_eq!(
            meta.labels.get(&lk("domain")).map(LabelValue::as_str),
            Some("ml")
        );
    }

    #[test]
    fn merge_empty_card_defaults_only() {
        let mut cfg = WyrdConfig::empty();
        cfg.defaults.space = Some(SpaceName::new("prod").unwrap());
        cfg.defaults.labels.insert(lk("team"), lv("ml"));

        let mut meta = fresh_meta("card");
        apply_defaults(&mut meta, &CardKind::Model, &cfg);

        assert_eq!(meta.space.as_ref().map(SpaceName::as_str), Some("prod"));
        assert_eq!(
            meta.labels.get(&lk("team")).map(LabelValue::as_str),
            Some("ml")
        );
    }

    #[test]
    fn merge_empty_card_defaults_and_kind_kind_wins() {
        let mut cfg = WyrdConfig::empty();
        cfg.defaults.space = Some(SpaceName::new("prod").unwrap());
        let ko = crate::KindOverride {
            space: Some(SpaceName::new("ml-prod").unwrap()),
            ..Default::default()
        };
        cfg.kind_overrides.insert(CardKind::Model, ko);

        let mut meta = fresh_meta("card");
        apply_defaults(&mut meta, &CardKind::Model, &cfg);

        assert_eq!(meta.space.as_ref().map(SpaceName::as_str), Some("ml-prod"));
    }

    #[test]
    fn merge_card_space_set_defaults_skipped() {
        let mut cfg = WyrdConfig::empty();
        cfg.defaults.space = Some(SpaceName::new("prod").unwrap());

        let mut meta = fresh_meta("card");
        meta.space = Some(SpaceName::new("eval").unwrap());
        apply_defaults(&mut meta, &CardKind::Model, &cfg);

        assert_eq!(meta.space.as_ref().map(SpaceName::as_str), Some("eval"));
    }

    #[test]
    fn merge_label_card_wins_for_existing_key_others_added() {
        let mut cfg = WyrdConfig::empty();
        cfg.defaults.labels.insert(lk("team"), lv("data"));
        cfg.defaults.labels.insert(lk("domain"), lv("customer"));

        let mut meta = fresh_meta("card");
        meta.labels.insert(lk("team"), lv("ml"));
        apply_defaults(&mut meta, &CardKind::Model, &cfg);

        assert_eq!(
            meta.labels.get(&lk("team")).map(LabelValue::as_str),
            Some("ml")
        );
        assert_eq!(
            meta.labels.get(&lk("domain")).map(LabelValue::as_str),
            Some("customer")
        );
    }

    #[test]
    fn merge_label_kind_wins_over_defaults_for_same_key() {
        let mut cfg = WyrdConfig::empty();
        cfg.defaults.labels.insert(lk("team"), lv("data"));
        let mut ko = crate::KindOverride::default();
        ko.labels.insert(lk("team"), lv("ml-kind"));
        cfg.kind_overrides.insert(CardKind::Model, ko);

        let mut meta = fresh_meta("card");
        apply_defaults(&mut meta, &CardKind::Model, &cfg);

        assert_eq!(
            meta.labels.get(&lk("team")).map(LabelValue::as_str),
            Some("ml-kind")
        );
    }

    #[test]
    fn merge_three_way_label_precedence_is_order_independent() {
        let mut cfg = WyrdConfig::empty();
        cfg.defaults.labels.insert(lk("bbb"), lv("defaults-b"));
        cfg.defaults.labels.insert(lk("ccc"), lv("defaults"));
        let mut ko = crate::KindOverride::default();
        ko.labels.insert(lk("bbb"), lv("kind"));
        cfg.kind_overrides.insert(CardKind::Model, ko);

        let mut meta = fresh_meta("card");
        meta.labels.insert(lk("aaa"), lv("card"));
        apply_defaults(&mut meta, &CardKind::Model, &cfg);

        assert_eq!(
            meta.labels.get(&lk("aaa")).map(LabelValue::as_str),
            Some("card")
        );
        assert_eq!(
            meta.labels.get(&lk("bbb")).map(LabelValue::as_str),
            Some("kind")
        );
        assert_eq!(
            meta.labels.get(&lk("ccc")).map(LabelValue::as_str),
            Some("defaults")
        );
    }

    #[test]
    fn merge_defaults_only_for_unconfigured_kind() {
        let mut cfg = WyrdConfig::empty();
        cfg.defaults.space = Some(SpaceName::new("prod").unwrap());
        let ko = crate::KindOverride {
            space: Some(SpaceName::new("gov-prod").unwrap()),
            ..Default::default()
        };
        cfg.kind_overrides.insert(CardKind::Policy, ko);

        let mut meta = fresh_meta("card");
        apply_defaults(&mut meta, &CardKind::Model, &cfg);

        assert_eq!(meta.space.as_ref().map(SpaceName::as_str), Some("prod"));
    }

    #[test]
    fn merge_name_is_never_touched() {
        let mut cfg = WyrdConfig::empty();
        cfg.defaults.space = Some(SpaceName::new("prod").unwrap());

        let mut meta = fresh_meta("churn");
        apply_defaults(&mut meta, &CardKind::Model, &cfg);

        assert_eq!(meta.name.as_str(), "churn");
    }

    #[test]
    fn merge_version_is_never_touched() {
        use wyrd_semver::VersionSpec;

        let cfg = WyrdConfig::empty();

        for version in [None, Some(VersionSpec::parse("1.0").unwrap())] {
            let mut meta = Metadata {
                name: CardName::new("churn").unwrap(),
                version: version.clone(),
                bump: None,
                space: None,
                uid: None,
                labels: BTreeMap::default(),
                annotations: BTreeMap::default(),
                spec_hash: None,
                artifact_hash: None,
                origin: None,
            };
            apply_defaults(&mut meta, &CardKind::Model, &cfg);
            assert_eq!(meta.version, version);
        }
    }
}
