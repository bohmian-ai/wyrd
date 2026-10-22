//! Local validation over fully resolved authored cards.

use std::collections::HashMap;

use wyrd_spec::envelope::Spec;
use wyrd_spec::error::WyrdError;
use wyrd_spec::graph::spec_binding_errors;
use wyrd_spec::refs::ReferenceSlotVisitor;
use wyrd_spec::registry::validate_artifact_manifest;

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

/// Reject duplicate `(kind, name, version)` identities and report the
/// first authored source path for each duplicate.
fn check_duplicate_identities(cards: &[AuthoredCard], diagnostics: &mut Vec<Diagnostic>) {
    let mut seen: HashMap<_, &std::path::PathBuf> = HashMap::new();
    for card in cards {
        let identity = (
            &card.kind,
            card.metadata.name.as_str(),
            card.metadata.version.as_ref().map(ToString::to_string),
        );
        if let Some(first_path) = seen.get(&identity) {
            diagnostics.push(Diagnostic::invalid_envelope(
                card.source_path.clone(),
                format!(
                    "duplicate card identity for {}/{} (first authored at {})",
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
    for error in spec_binding_errors(&card.spec) {
        diagnostics.push(catalog_error(card, &error));
    }
    if let Spec::Verifier(spec) = &card.spec
        && let Err(error) = spec.validate()
    {
        diagnostics.push(catalog_error(card, &error));
    }
}

/// Confirm that every reference is fully resolved and obeys authored-reference
/// rules before the submission reaches the registration boundary.
fn check_resolved_references(card: &AuthoredCard, diagnostics: &mut Vec<Diagnostic>) {
    let mut spec = card.spec.clone();
    ReferenceSlotVisitor::visit(&mut spec, |slot| {
        let diagnostic = if slot.value.as_path().is_some() {
            Some(unresolved_path_diagnostic(card, &slot.path))
        } else if let Some(reference) = slot.value.as_card_ref() {
            reference
                .uid
                .is_some()
                .then(|| authored_uid_diagnostic(card, &slot.path))
        } else {
            None
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

/// Report a card whose artifact manifest repeats or nests a path, through
/// the registration rule the server enforces.
///
/// Only a sole submission may carry artifacts, so checking each card's own
/// manifest covers the loaded tree.
fn check_duplicate_artifact_paths(cards: &[AuthoredCard], diagnostics: &mut Vec<Diagnostic>) {
    for card in cards {
        if let Err(error) = validate_artifact_manifest(&card.artifacts) {
            diagnostics.push(catalog_error(card, &error));
        }
    }
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
    use wyrd_spec::card::trigger::{TriggerActivation, TriggerSpec};
    use wyrd_spec::card::verifier::VerificationBinding;
    use wyrd_spec::envelope::{CardKind, Metadata, Spec};
    use wyrd_spec::ids::{CardName};
    use wyrd_spec::reference::{CardRef, InlineableRef, Ref};

    /// Reject two `verified_by` bindings to the same Verifier version on one
    /// Service component occurrence before anything is submitted.
    #[test]
    fn validate_rejects_duplicate_component_publication_targets() {
        let verifier_ref = CardRef {
            kind: CardKind::Verifier,
            name: CardName::new("quality").expect("test card name is valid"),
            version: VersionBlock::parse("1.0.0").expect("test version is valid"),
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
                        uid: None,
                    }),
                    verified_by: vec![binding(&verifier_ref), binding(&verifier_ref)],
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
            diagnostic.code == "WYRD_SPEC_400_DUPLICATE_VERIFICATION_BINDING"
                && diagnostic
                    .message
                    .contains("spec.components[0].verified_by")
        }));
    }

    /// Build one binding to `verifier` that runs on an inline schedule.
    fn binding(verifier: &CardRef) -> VerificationBinding {
        VerificationBinding {
            verifier: Ref::Ref(verifier.clone()),
            runs_on: Some(InlineableRef::Inline(Box::new(TriggerSpec {
                description: None,
                activation: TriggerActivation::Schedule {
                    cron: "0 * * * *".to_owned(),
                    tz: None,
                },
            }))),
            on_failure: Vec::new(),
            on_verdict_change: Vec::new(),
        }
    }
}
