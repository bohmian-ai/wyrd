use std::io::Write;
use tempfile::NamedTempFile;
use wyrd_config::{WyrdConfig, WyrdConfigError};

fn parse(toml: &str) -> Result<WyrdConfig, WyrdConfigError> {
    let mut tmp = NamedTempFile::new().unwrap();
    tmp.write_all(toml.as_bytes()).unwrap();
    WyrdConfig::load(Some(tmp.path()))
}

#[test]
fn parse_empty_file_yields_empty_config() {
    let cfg = parse("").unwrap();
    assert!(cfg.defaults.space.is_none());
    assert!(cfg.kind_overrides.is_empty());
}

#[test]
fn parse_defaults_only() {
    let cfg = parse("[defaults]\nspace = \"prod\"\n").unwrap();
    assert_eq!(cfg.defaults.space.as_ref().map(|s| s.as_str()), Some("prod"));
    assert!(cfg.kind_overrides.is_empty());
}

#[test]
fn parse_kind_only() {
    let cfg = parse("[kind.Model]\nspace = \"mls\"\n").unwrap();
    assert!(cfg.defaults.space.is_none());
    use wyrd_spec::envelope::CardKind;
    let ko = cfg.kind_overrides.get(&CardKind::Model).unwrap();
    assert_eq!(ko.space.as_ref().map(|s| s.as_str()), Some("mls"));
}

#[test]
fn parse_defaults_and_kind() {
    let toml = "[defaults]\nspace = \"prod\"\n[kind.Model]\nspace = \"mls\"\n";
    let cfg = parse(toml).unwrap();
    assert_eq!(cfg.defaults.space.as_ref().map(|s| s.as_str()), Some("prod"));
    use wyrd_spec::envelope::CardKind;
    assert!(cfg.kind_overrides.contains_key(&CardKind::Model));
}

#[test]
fn parse_unknown_top_level_table_rejected() {
    let err = parse("[bogus]\nk = \"v\"\n").unwrap_err();
    assert!(matches!(err, WyrdConfigError::Schema { .. }), "got {err:?}");
}

#[test]
fn parse_typo_default_singular_rejected() {
    let err = parse("[default]\nspace = \"prod\"\n").unwrap_err();
    assert!(matches!(err, WyrdConfigError::Schema { .. }), "got {err:?}");
}

#[test]
fn parse_invalid_space_value_rejected() {
    let err = parse("[defaults]\nspace = \"PROD\"\n").unwrap_err();
    assert!(matches!(err, WyrdConfigError::Schema { .. }), "got {err:?}");
}

#[test]
fn parse_invalid_label_key_rejected() {
    let err = parse("[defaults.labels]\n\"BAD KEY\" = \"v\"\n").unwrap_err();
    assert!(matches!(err, WyrdConfigError::Schema { .. }), "got {err:?}");
}

#[test]
fn parse_lowercase_kind_key_rejected() {
    let err = parse("[kind.model]\nspace = \"ml\"\n").unwrap_err();
    assert!(matches!(err, WyrdConfigError::Schema { .. }), "got {err:?}");
}

#[test]
fn parse_unknown_kind_rejected() {
    let err = parse("[kind.NotAKind]\nspace = \"ml\"\n").unwrap_err();
    assert!(matches!(err, WyrdConfigError::Schema { .. }), "got {err:?}");
}

#[test]
fn parse_external_kind_rejected() {
    let toml = "[kind.External]\nspace = \"mls\"\n";
    let err = parse(toml).unwrap_err();
    match &err {
        WyrdConfigError::Schema { message, .. } => {
            assert!(message.contains("[kind.External]"), "got {message:?}");
        }
        other => panic!("expected Schema, got {other:?}"),
    }
}

#[test]
fn parse_name_in_defaults_is_rejected() {
    let toml = "[defaults]\nname = \"foo\"\n";
    let err = parse(toml).unwrap_err();
    assert!(
        matches!(
            &err,
            WyrdConfigError::NameDefaultRejected { table } if table == "[defaults]"
        ),
        "got {err:?}"
    );
}

#[test]
fn parse_name_in_kind_table_is_rejected() {
    let toml = "[kind.Model]\nname = \"foo\"\n";
    let err = parse(toml).unwrap_err();
    assert!(
        matches!(
            &err,
            WyrdConfigError::NameDefaultRejected { table } if table == "[kind.Model]"
        ),
        "got {err:?}"
    );
}

#[test]
fn parse_version_under_defaults_rejected() {
    let err = parse("[defaults]\nversion = \"1.0.0\"\n").unwrap_err();
    assert!(matches!(err, WyrdConfigError::Schema { .. }), "got {err:?}");
}
