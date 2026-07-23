//! Shared identity parsers for card holders.

use serde_json::json;
use wyrd_semver::VersionBlock;
use wyrd_spec::envelope::CardKind;
use wyrd_spec::error::WyrdError;
use wyrd_spec::ids::{CardName, CardUid, SpaceName};
use wyrd_spec::metadata::{Annotations, Labels};

/// Apply the cached repository defaults used by local card constructors.
///
/// Broken workspace config is intentionally ignored here so explicit card
/// construction remains usable; callers that need diagnostics can load
/// [`wyrd_config::WyrdConfig`] directly.
pub(crate) fn apply_repo_defaults(
    kind: &CardKind,
    space: &mut Option<String>,
    labels: &mut Labels,
    annotations: &mut Annotations,
) {
    apply_repo_defaults_from(
        kind,
        space,
        labels,
        annotations,
        wyrd_config::resolve_repo_config(),
    );
}

fn apply_repo_defaults_from(
    kind: &CardKind,
    space: &mut Option<String>,
    labels: &mut Labels,
    annotations: &mut Annotations,
    result: Result<Option<wyrd_config::WyrdConfig>, wyrd_config::WyrdConfigError>,
) {
    if let Ok(Some(config)) = result {
        config.apply_defaults_fields(kind, space, labels, annotations);
    }
}

pub(crate) fn card_name(field: &str, value: &str) -> Result<CardName, WyrdError> {
    CardName::new(value).map_err(|error| invalid_identity(field, value, error))
}

pub(crate) fn version_block(value: &str) -> Result<VersionBlock, WyrdError> {
    VersionBlock::parse(value).map_err(|error| {
        validation_error(
            format!("invalid card version: {value}"),
            json!({
                "field": "version",
                "value": value,
                "source": error.to_string(),
            }),
        )
    })
}

pub(crate) fn space_name(value: &str) -> Result<SpaceName, WyrdError> {
    if value.is_empty() {
        return Err(validation_error(
            "card space is required and cannot be empty",
            json!({ "field": "space" }),
        ));
    }
    SpaceName::new(value).map_err(|error| invalid_identity("space", value, error))
}

pub(crate) fn optional_card_uid(value: &str) -> Result<Option<CardUid>, WyrdError> {
    if value.is_empty() {
        Ok(None)
    } else {
        CardUid::new(value)
            .map(Some)
            .map_err(|error| invalid_identity("uid", value, error))
    }
}

pub(crate) fn invalid_identity(
    field: &str,
    value: &str,
    error: impl std::fmt::Display,
) -> WyrdError {
    validation_error(
        format!("invalid card {field}: {value}"),
        json!({
            "field": field,
            "value": value,
            "source": error.to_string(),
        }),
    )
}

pub(crate) fn validation_error(
    message: impl Into<String>,
    details: serde_json::Value,
) -> WyrdError {
    WyrdError::Validation {
        message: message.into(),
        details,
    }
}

#[cfg(test)]
mod tests {
    use wyrd_spec::envelope::CardKind;
    use wyrd_spec::ids::SpaceName;
    use wyrd_spec::metadata::{
        AnnotationKey, AnnotationValue, Annotations, LabelKey, LabelValue, Labels,
    };

    use super::apply_repo_defaults_from;

    fn label(key: &str, value: &str) -> (LabelKey, LabelValue) {
        (
            LabelKey::new(key).expect("static label key is valid"),
            LabelValue::new(value).expect("static label value is valid"),
        )
    }

    fn annotation(key: &str, value: &str) -> (AnnotationKey, AnnotationValue) {
        (
            AnnotationKey::new(key).expect("static annotation key is valid"),
            AnnotationValue::new(value).expect("static annotation value is valid"),
        )
    }

    fn insert_label(labels: &mut Labels, key: &str, value: &str) {
        let (key, value) = label(key, value);
        labels.insert(key, value);
    }

    fn insert_annotation(annotations: &mut Annotations, key: &str, value: &str) {
        let (key, value) = annotation(key, value);
        annotations.insert(key, value);
    }

    fn apply_with_config(
        config: Result<Option<wyrd_config::WyrdConfig>, wyrd_config::WyrdConfigError>,
        kind: &CardKind,
    ) -> (
        Option<String>,
        wyrd_spec::metadata::Labels,
        wyrd_spec::metadata::Annotations,
    ) {
        let mut space = None;
        let mut labels = wyrd_spec::metadata::Labels::default();
        let mut annotations = wyrd_spec::metadata::Annotations::default();
        apply_repo_defaults_from(kind, &mut space, &mut labels, &mut annotations, config);
        (space, labels, annotations)
    }

    #[test]
    fn repository_defaults_use_kind_then_workspace_precedence() {
        let mut config = wyrd_config::WyrdConfig::empty();
        config.defaults.space = Some(SpaceName::new("workspace").expect("static space is valid"));
        insert_label(&mut config.defaults.labels, "team", "platform");
        insert_label(&mut config.defaults.labels, "env", "dev");
        insert_annotation(&mut config.defaults.annotations, "owner", "workspace");
        insert_annotation(&mut config.defaults.annotations, "source", "repo");

        let mut kind_labels = Labels::default();
        insert_label(&mut kind_labels, "team", "models");
        insert_label(&mut kind_labels, "kind", "model");
        let mut kind_annotations = Annotations::default();
        insert_annotation(&mut kind_annotations, "owner", "models");
        insert_annotation(&mut kind_annotations, "kind", "model");
        let kind_override = wyrd_config::KindOverride {
            space: Some(SpaceName::new("models").expect("static space is valid")),
            labels: kind_labels,
            annotations: kind_annotations,
        };
        config.kind_overrides.insert(CardKind::Model, kind_override);

        let mut space = Some("explicit".to_owned());
        let mut labels = wyrd_spec::metadata::Labels::default();
        insert_label(&mut labels, "team", "card");
        let mut annotations = wyrd_spec::metadata::Annotations::default();
        insert_annotation(&mut annotations, "owner", "card");

        apply_repo_defaults_from(
            &CardKind::Model,
            &mut space,
            &mut labels,
            &mut annotations,
            Ok(Some(config)),
        );

        assert_eq!(space.as_deref(), Some("explicit"));
        assert_eq!(
            labels
                .get(&label("team", "unused").0)
                .map(LabelValue::as_str),
            Some("card")
        );
        assert_eq!(
            labels
                .get(&label("env", "unused").0)
                .map(LabelValue::as_str),
            Some("dev")
        );
        assert_eq!(
            labels
                .get(&label("kind", "unused").0)
                .map(LabelValue::as_str),
            Some("model")
        );
        assert_eq!(
            annotations
                .get(&annotation("owner", "unused").0)
                .map(AnnotationValue::as_str),
            Some("card")
        );
        assert_eq!(
            annotations
                .get(&annotation("source", "unused").0)
                .map(AnnotationValue::as_str),
            Some("repo")
        );
        assert_eq!(
            annotations
                .get(&annotation("kind", "unused").0)
                .map(AnnotationValue::as_str),
            Some("model")
        );
    }

    #[test]
    fn repository_defaults_apply_to_each_card_kind() {
        let mut config = wyrd_config::WyrdConfig::empty();
        config.defaults.space = Some(SpaceName::new("workspace").expect("static space is valid"));
        insert_label(&mut config.defaults.labels, "env", "dev");
        insert_annotation(&mut config.defaults.annotations, "owner", "platform");
        for kind in [CardKind::Data, CardKind::Model, CardKind::Prompt] {
            let (space, labels, annotations) = apply_with_config(Ok(Some(config.clone())), &kind);
            assert_eq!(space.as_deref(), Some("workspace"));
            assert_eq!(
                labels
                    .get(&label("env", "unused").0)
                    .map(LabelValue::as_str),
                Some("dev")
            );
            assert_eq!(
                annotations
                    .get(&annotation("owner", "unused").0)
                    .map(AnnotationValue::as_str),
                Some("platform")
            );
        }
    }

    #[test]
    fn missing_repository_config_keeps_compiled_defaults_available() {
        let (space, labels, annotations) = apply_with_config(Ok(None), &CardKind::Data);
        assert!(space.is_none());
        assert!(labels.is_empty());
        assert!(annotations.is_empty());
    }

    #[test]
    fn malformed_repository_config_is_a_soft_failure() {
        let error = wyrd_config::WyrdConfigError::TomlParse {
            message: "invalid table header".to_owned(),
            path: "wyrd.toml".into(),
        };
        let (space, labels, annotations) = apply_with_config(Err(error), &CardKind::Prompt);
        assert!(space.is_none());
        assert!(labels.is_empty());
        assert!(annotations.is_empty());
    }
}
