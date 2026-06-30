use std::collections::BTreeMap;
use std::io::Write;

use tempfile::NamedTempFile;
use wyrd_config::{WyrdConfig, apply_defaults};
use wyrd_spec::envelope::{CardKind, Metadata};
use wyrd_spec::ids::{CardName, SpaceName};
use wyrd_spec::metadata::{AnnotationKey, AnnotationValue, LabelKey, LabelValue};

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

    assert_eq!(
        meta.space.as_ref().map(SpaceName::as_str),
        Some("gov-prod"),
    );
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
