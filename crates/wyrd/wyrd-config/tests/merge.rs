use wyrd_config::{WyrdConfig, apply_defaults};
use wyrd_spec::envelope::{CardKind, Metadata};
use wyrd_spec::ids::{CardName, SpaceName};
use wyrd_spec::metadata::{LabelKey, LabelValue};

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
        labels: Default::default(),
        annotations: Default::default(),
        spec_hash: None,
        artifact_hash: None,
    }
}

#[test]
fn merge_all_set_card_full_config_card_wins() {
    let mut cfg = WyrdConfig::empty();
    cfg.defaults.space = Some(SpaceName::new("prod").unwrap());
    cfg.defaults.labels.insert(lk("team"), lv("platform"));

    let mut ko = wyrd_config::KindOverride::default();
    ko.space = Some(SpaceName::new("ml-prod").unwrap());
    ko.labels.insert(lk("domain"), lv("ml"));
    cfg.kind_overrides.insert(CardKind::Model, ko);

    let mut meta = fresh_meta("churn");
    meta.space = Some(SpaceName::new("eval").unwrap());
    meta.labels.insert(lk("team"), lv("ml-team"));

    let orig_version = meta.version.clone();
    apply_defaults(&mut meta, &CardKind::Model, &cfg);

    assert_eq!(meta.space.as_ref().map(|s| s.as_str()), Some("eval"));
    assert_eq!(meta.version, orig_version);
    assert_eq!(
        meta.labels.get(&lk("team")).map(|v| v.as_str()),
        Some("ml-team")
    );
    assert_eq!(
        meta.labels.get(&lk("domain")).map(|v| v.as_str()),
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

    assert_eq!(meta.space.as_ref().map(|s| s.as_str()), Some("prod"));
    assert_eq!(meta.labels.get(&lk("team")).map(|v| v.as_str()), Some("ml"));
}

#[test]
fn merge_empty_card_defaults_and_kind_kind_wins() {
    let mut cfg = WyrdConfig::empty();
    cfg.defaults.space = Some(SpaceName::new("prod").unwrap());
    let mut ko = wyrd_config::KindOverride::default();
    ko.space = Some(SpaceName::new("ml-prod").unwrap());
    cfg.kind_overrides.insert(CardKind::Model, ko);

    let mut meta = fresh_meta("card");
    apply_defaults(&mut meta, &CardKind::Model, &cfg);

    assert_eq!(meta.space.as_ref().map(|s| s.as_str()), Some("ml-prod"));
}

#[test]
fn merge_card_space_set_defaults_skipped() {
    let mut cfg = WyrdConfig::empty();
    cfg.defaults.space = Some(SpaceName::new("prod").unwrap());

    let mut meta = fresh_meta("card");
    meta.space = Some(SpaceName::new("eval").unwrap());
    apply_defaults(&mut meta, &CardKind::Model, &cfg);

    assert_eq!(meta.space.as_ref().map(|s| s.as_str()), Some("eval"));
}

#[test]
fn merge_label_card_wins_for_existing_key_others_added() {
    let mut cfg = WyrdConfig::empty();
    cfg.defaults.labels.insert(lk("team"), lv("data"));
    cfg.defaults.labels.insert(lk("domain"), lv("customer"));

    let mut meta = fresh_meta("card");
    meta.labels.insert(lk("team"), lv("ml"));
    apply_defaults(&mut meta, &CardKind::Model, &cfg);

    assert_eq!(meta.labels.get(&lk("team")).map(|v| v.as_str()), Some("ml"));
    assert_eq!(
        meta.labels.get(&lk("domain")).map(|v| v.as_str()),
        Some("customer")
    );
}

#[test]
fn merge_label_kind_wins_over_defaults_for_same_key() {
    let mut cfg = WyrdConfig::empty();
    cfg.defaults.labels.insert(lk("team"), lv("data"));
    let mut ko = wyrd_config::KindOverride::default();
    ko.labels.insert(lk("team"), lv("ml-kind"));
    cfg.kind_overrides.insert(CardKind::Model, ko);

    let mut meta = fresh_meta("card");
    apply_defaults(&mut meta, &CardKind::Model, &cfg);

    assert_eq!(
        meta.labels.get(&lk("team")).map(|v| v.as_str()),
        Some("ml-kind")
    );
}

#[test]
fn merge_three_way_label_precedence_is_order_independent() {
    let mut cfg = WyrdConfig::empty();
    cfg.defaults.labels.insert(lk("bbb"), lv("defaults-b"));
    cfg.defaults.labels.insert(lk("ccc"), lv("defaults"));
    let mut ko = wyrd_config::KindOverride::default();
    ko.labels.insert(lk("bbb"), lv("kind"));
    cfg.kind_overrides.insert(CardKind::Model, ko);

    let mut meta = fresh_meta("card");
    meta.labels.insert(lk("aaa"), lv("card"));
    apply_defaults(&mut meta, &CardKind::Model, &cfg);

    assert_eq!(
        meta.labels.get(&lk("aaa")).map(|v| v.as_str()),
        Some("card")
    );
    assert_eq!(
        meta.labels.get(&lk("bbb")).map(|v| v.as_str()),
        Some("kind")
    );
    assert_eq!(
        meta.labels.get(&lk("ccc")).map(|v| v.as_str()),
        Some("defaults")
    );
}

#[test]
fn merge_defaults_only_for_unconfigured_kind() {
    let mut cfg = WyrdConfig::empty();
    cfg.defaults.space = Some(SpaceName::new("prod").unwrap());
    let mut ko = wyrd_config::KindOverride::default();
    ko.space = Some(SpaceName::new("gov-prod").unwrap());
    cfg.kind_overrides.insert(CardKind::Policy, ko);

    let mut meta = fresh_meta("card");
    apply_defaults(&mut meta, &CardKind::Model, &cfg);

    assert_eq!(meta.space.as_ref().map(|s| s.as_str()), Some("prod"));
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
            labels: Default::default(),
            annotations: Default::default(),
            spec_hash: None,
            artifact_hash: None,
        };
        apply_defaults(&mut meta, &CardKind::Model, &cfg);
        assert_eq!(meta.version, version);
    }
}
