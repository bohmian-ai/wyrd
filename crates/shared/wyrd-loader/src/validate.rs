//! Local validation over fully resolved authored cards.

use std::collections::HashMap;

use wyrd_spec::envelope::{CardKind, Spec};
use wyrd_spec::error::WyrdError;
use wyrd_spec::graph::publication_validation_errors;
use wyrd_spec::reference::Ref;
use wyrd_spec::refs::{ReferenceSlotVisitor, SlotValue};

use super::error::{Diagnostic, Severity};
use super::parse::AuthoredCard;

/// Validate the loaded tree and return every diagnostic found.
#[must_use]
pub fn validate_tree(cards: &[AuthoredCard]) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    check_duplicate_identities(cards, &mut diagnostics);
    for card in cards {
        validate_card(card, &mut diagnostics);
        check_resolved_references(card, &mut diagnostics);
    }
    check_heavy_artifact_constraint(cards, &mut diagnostics);
    check_duplicate_artifact_paths(cards, &mut diagnostics);
    diagnostics
}

/// Reject duplicate `(kind, space, name, version)` identities and report the
/// first authored source path for each duplicate.
fn check_duplicate_identities(cards: &[AuthoredCard], diagnostics: &mut Vec<Diagnostic>) {
    let mut seen: HashMap<_, &std::path::PathBuf> = HashMap::new();
    for card in cards {
        let identity = (
            &card.kind,
            card.metadata
                .space
                .as_ref()
                .map(wyrd_spec::ids::SpaceName::as_str),
            card.metadata.name.as_str(),
            card.metadata.version.as_ref().map(ToString::to_string),
        );
        if let Some(first_path) = seen.get(&identity) {
            diagnostics.push(Diagnostic::invalid_envelope(
                card.source_path.clone(),
                format!(
                    "duplicate card identity for {}/{}/{} (first authored at {})",
                    card.metadata
                        .space
                        .as_ref()
                        .map_or("<missing-space>", |space| space.as_str()),
                    card.kind.wire_name(),
                    card.metadata.name,
                    first_path.display()
                ),
            ));
        } else {
            seen.insert(identity, &card.source_path);
        }
    }
}

/// Run the kind-specific local validation that is not covered by envelope
/// parsing or reference resolution.
fn validate_card(card: &AuthoredCard, diagnostics: &mut Vec<Diagnostic>) {
    match &card.spec {
        Spec::Agent(spec) => {
            validate_publications(&spec.publishes_to, "spec.publishes_to", card, diagnostics);
        }
        Spec::Service(spec) => {
            validate_publications(&spec.publishes_to, "spec.publishes_to", card, diagnostics);
            for (index, component) in spec.components.iter().enumerate() {
                validate_publications(
                    &component.publishes_to,
                    &format!("spec.components[{index}].publishes_to"),
                    card,
                    diagnostics,
                );
            }
        }
        Spec::Eval(spec) => {
            if let Err(error) = spec.validate() {
                diagnostics.push(catalog_error(card, &error.into()));
            }
        }
        Spec::Drift(spec) => {
            if let Err(error) = spec.validate() {
                diagnostics.push(catalog_error(card, &error.into()));
            }
        }
        Spec::Trigger(spec) => validate_trigger(spec, card, diagnostics),
        Spec::Prompt(_)
        | Spec::Workflow(_)
        | Spec::Mcp(_)
        | Spec::Policy(_)
        | Spec::Audit(_)
        | Spec::Source(_)
        | Spec::Data(_)
        | Spec::Model(_)
        | Spec::Artifact(_)
        | Spec::Experiment(_)
        | Spec::Operator(_) => {}
    }
}

/// Validate publication target kinds and reject duplicate publication targets.
fn validate_publications(
    publications: &[Ref],
    field: &str,
    card: &AuthoredCard,
    diagnostics: &mut Vec<Diagnostic>,
) {
    for error in publication_validation_errors(publications, field) {
        diagnostics.push(catalog_error(card, &error));
    }
}

/// Validate the card kinds referenced by a trigger's operator and source.
fn validate_trigger(
    trigger: &wyrd_spec::card::trigger::TriggerSpec,
    card: &AuthoredCard,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if trigger
        .operator_ref
        .as_card_ref()
        .is_some_and(|reference| reference.kind != CardKind::Operator)
    {
        diagnostics.push(Diagnostic::invalid_envelope(
            card.source_path.clone(),
            "Trigger operator_ref must reference an Operator".to_owned(),
        ));
    }
    let source_ref = trigger.source.as_ref().map(|source| match source {
        wyrd_spec::card::trigger::TriggerSource::DriftObservation { drift_ref, .. } => {
            (drift_ref, CardKind::Drift)
        }
        wyrd_spec::card::trigger::TriggerSource::EvalObservation { eval_ref, .. } => {
            (eval_ref, CardKind::Eval)
        }
    });
    if let Some((reference, expected)) = source_ref
        && reference
            .as_card_ref()
            .is_some_and(|reference| reference.kind != expected)
    {
        diagnostics.push(Diagnostic::invalid_envelope(
            card.source_path.clone(),
            format!(
                "Trigger source must reference a {} card",
                expected.wire_name()
            ),
        ));
    }
}

/// Confirm that every reference is fully resolved and obeys authored-reference
/// rules before the submission reaches the registration boundary.
fn check_resolved_references(card: &AuthoredCard, diagnostics: &mut Vec<Diagnostic>) {
    let mut spec = card.spec.clone();
    ReferenceSlotVisitor::visit(&mut spec, |slot| {
        let diagnostic = match slot.value {
            SlotValue::Durable(reference) => match reference {
                Ref::Path(_) => Some(unresolved_path_diagnostic(card, &slot.path)),
                _ if reference
                    .as_card_ref()
                    .is_some_and(|reference| reference.uid.is_some()) =>
                {
                    Some(authored_uid_diagnostic(card, &slot.path))
                }
                _ if reference
                    .as_card_ref()
                    .is_some_and(|reference| reference.space.is_none()) =>
                {
                    Some(missing_space_diagnostic(card, &slot.path))
                }
                _ => None,
            },
            SlotValue::InlineablePrompt(reference) => match reference {
                wyrd_spec::reference::InlineableRef::Path(_) => {
                    Some(unresolved_path_diagnostic(card, &slot.path))
                }
                _ if reference
                    .as_card_ref()
                    .is_some_and(|reference| reference.uid.is_some()) =>
                {
                    Some(authored_uid_diagnostic(card, &slot.path))
                }
                _ if reference
                    .as_card_ref()
                    .is_some_and(|reference| reference.space.is_none()) =>
                {
                    Some(missing_space_diagnostic(card, &slot.path))
                }
                _ => None,
            },
            SlotValue::InlineableAgent(reference) => match reference {
                wyrd_spec::reference::InlineableRef::Path(_) => {
                    Some(unresolved_path_diagnostic(card, &slot.path))
                }
                _ if reference
                    .as_card_ref()
                    .is_some_and(|reference| reference.uid.is_some()) =>
                {
                    Some(authored_uid_diagnostic(card, &slot.path))
                }
                _ if reference
                    .as_card_ref()
                    .is_some_and(|reference| reference.space.is_none()) =>
                {
                    Some(missing_space_diagnostic(card, &slot.path))
                }
                _ => None,
            },
        };
        if let Some(diagnostic) = diagnostic {
            diagnostics.push(diagnostic);
        }
    });
}

/// Build the diagnostic emitted when a path reference survived resolution.
fn unresolved_path_diagnostic(card: &AuthoredCard, field: &str) -> Diagnostic {
    catalog_error(
        card,
        &WyrdError::RegistryUnresolvedPathRef {
            message: format!("unresolved path reference at {field}"),
            details: serde_json::json!({ "field": field }),
        },
    )
}

/// Build the diagnostic emitted when a reference still lacks its inherited
/// registration space.
fn missing_space_diagnostic(card: &AuthoredCard, field: &str) -> Diagnostic {
    Diagnostic::invalid_envelope(
        card.source_path.clone(),
        format!("reference has no resolved space at {field}"),
    )
}

/// Build the diagnostic emitted when authored input supplies a server-managed
/// reference UID.
fn authored_uid_diagnostic(card: &AuthoredCard, field: &str) -> Diagnostic {
    Diagnostic::invalid_envelope(
        card.source_path.clone(),
        format!("reference at {field} must omit server-managed uid"),
    )
}

/// Enforce the rule that a card carrying artifacts must be the only loaded
/// submission.
fn check_heavy_artifact_constraint(cards: &[AuthoredCard], diagnostics: &mut Vec<Diagnostic>) {
    if cards.len() <= 1 {
        return;
    }
    for card in cards.iter().filter(|card| !card.artifacts.is_empty()) {
        diagnostics.push(catalog_error(
            card,
            &WyrdError::RegistryHeavyArtifactNotSoleSubmission {
                message: "a submission with artifacts must be loaded alone".to_owned(),
                details: serde_json::json!({
                    "artifact_count": card.artifacts.len(),
                    "submission_count": cards.len(),
                }),
            },
        ));
    }
}

/// Reject artifact manifests whose paths overlap as files and directories.
///
/// An artifact publication cannot contain both `a` and `a/b`: the former is a
/// file target while the latter requires it to be a directory. The check also
/// rejects exact reuse before a manifest reaches storage materialization.
fn check_duplicate_artifact_paths(cards: &[AuthoredCard], diagnostics: &mut Vec<Diagnostic>) {
    let mut seen = HashMap::<String, &std::path::PathBuf>::new();
    for card in cards {
        for artifact in &card.artifacts {
            let path = artifact.relative_path.as_str();
            if let Some((first_artifact, first_path)) = seen
                .iter()
                .find(|(existing, _)| artifact_paths_conflict(existing, path))
            {
                diagnostics.push(Diagnostic::invalid_envelope(
                    card.source_path.clone(),
                    format!(
                        "duplicate artifact path {} conflicts with {} (first authored at {})",
                        artifact.relative_path,
                        first_artifact,
                        first_path.display()
                    ),
                ));
            }
            seen.insert(path.to_owned(), &card.source_path);
        }
    }
}

/// Return whether two validated artifact paths are identical or prefix-conflict.
///
/// Artifact paths use `/` as their canonical separator, so string boundaries
/// are sufficient and avoid platform-specific path semantics.
#[must_use]
fn artifact_paths_conflict(left: &str, right: &str) -> bool {
    left == right
        || left
            .strip_prefix(right)
            .is_some_and(|suffix| suffix.starts_with('/'))
        || right
            .strip_prefix(left)
            .is_some_and(|suffix| suffix.starts_with('/'))
}

/// Convert a catalogued specification error into a source-aware diagnostic.
fn catalog_error(card: &AuthoredCard, error: &WyrdError) -> Diagnostic {
    Diagnostic::from_wyrd_error(card.source_path.clone(), Severity::Error, None, error)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::path::PathBuf;

    use super::validate_tree;
    use crate::parse::AuthoredCard;
    use wyrd_semver::VersionBlock;
    use wyrd_spec::api_version::ApiVersion;
    use wyrd_spec::card::service::{ServiceComponent, ServiceSpec};
    use wyrd_spec::envelope::{CardKind, Metadata, Spec};
    use wyrd_spec::ids::{CardName, SpaceName};
    use wyrd_spec::reference::{CardRef, Ref};

    #[test]
    fn validate_rejects_reference_without_resolved_space() {
        let card = AuthoredCard {
            source_path: PathBuf::from("service.yaml"),
            api_version: ApiVersion::v1(),
            kind: CardKind::Service,
            metadata: Metadata {
                name: CardName::new("service").expect("test card name is valid"),
                version: Some(
                    VersionBlock::parse("1.0.0")
                        .expect("test version is valid")
                        .into(),
                ),
                bump: None,
                space: Some(SpaceName::new("default").expect("test space is valid")),
                uid: None,
                labels: BTreeMap::new(),
                annotations: BTreeMap::new(),
                spec_hash: None,
                artifact_hash: None,
                origin: None,
            },
            spec: Spec::Service(ServiceSpec {
                components: vec![ServiceComponent {
                    alias: "child".to_owned(),
                    card_ref: Ref::Ref(CardRef {
                        kind: CardKind::Prompt,
                        name: CardName::new("child").expect("test card name is valid"),
                        version: VersionBlock::parse("1.0.0").expect("test version is valid"),
                        space: None,
                        uid: None,
                    }),
                    publishes_to: Vec::new(),
                    source: None,
                    config: BTreeMap::new(),
                    credential_refs: Vec::new(),
                }],
                ..Default::default()
            }),
            artifacts: Vec::new(),
        };

        let diagnostics = validate_tree(&[card]);
        assert!(
            diagnostics
                .iter()
                .any(|diagnostic| { diagnostic.message.contains("no resolved space") })
        );
    }

    /// Reject duplicate publication targets within one Service component binding.
    #[test]
    fn validate_rejects_duplicate_component_publication_targets() {
        let eval_ref = CardRef {
            kind: CardKind::Eval,
            name: CardName::new("quality").expect("test card name is valid"),
            version: VersionBlock::parse("1.0.0").expect("test version is valid"),
            space: Some(SpaceName::new("default").expect("test space is valid")),
            uid: None,
        };
        let card = AuthoredCard {
            source_path: PathBuf::from("service.yaml"),
            api_version: ApiVersion::v1(),
            kind: CardKind::Service,
            metadata: Metadata {
                name: CardName::new("service").expect("test card name is valid"),
                version: Some(
                    VersionBlock::parse("1.0.0")
                        .expect("test version is valid")
                        .into(),
                ),
                bump: None,
                space: Some(SpaceName::new("default").expect("test space is valid")),
                uid: None,
                labels: BTreeMap::new(),
                annotations: BTreeMap::new(),
                spec_hash: None,
                artifact_hash: None,
                origin: None,
            },
            spec: Spec::Service(ServiceSpec {
                components: vec![ServiceComponent {
                    alias: "model".to_owned(),
                    card_ref: Ref::Ref(CardRef {
                        kind: CardKind::Model,
                        name: CardName::new("classifier").expect("test card name is valid"),
                        version: VersionBlock::parse("1.0.0").expect("test version is valid"),
                        space: Some(SpaceName::new("default").expect("test space is valid")),
                        uid: None,
                    }),
                    publishes_to: vec![Ref::Ref(eval_ref.clone()), Ref::Ref(eval_ref)],
                    source: None,
                    config: BTreeMap::new(),
                    credential_refs: Vec::new(),
                }],
                ..Default::default()
            }),
            artifacts: Vec::new(),
        };

        let diagnostics = validate_tree(&[card]);
        assert!(diagnostics.iter().any(|diagnostic| {
            diagnostic.code == "WYRD_SPEC_400_DUPLICATE_PUBLISH_TARGET"
                && diagnostic
                    .message
                    .contains("spec.components[0].publishes_to")
        }));
    }
}
