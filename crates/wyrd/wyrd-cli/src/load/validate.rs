//! Local validation over fully resolved authored cards.

use std::collections::{BTreeSet, HashMap};

use wyrd_spec::envelope::{CardKind, Spec};
use wyrd_spec::error::WyrdError;
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
    diagnostics
}

fn check_duplicate_identities(cards: &[AuthoredCard], diagnostics: &mut Vec<Diagnostic>) {
    let mut seen: HashMap<_, &std::path::PathBuf> = HashMap::new();
    for card in cards {
        let identity = (
            &card.kind,
            card.metadata.space.as_ref().map(|space| space.as_str()),
            card.metadata.name.as_str(),
            card.metadata
                .version
                .as_ref()
                .map(|version| version.as_str()),
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

fn validate_card(card: &AuthoredCard, diagnostics: &mut Vec<Diagnostic>) {
    match &card.spec {
        Spec::Data(spec) => validate_publications(&spec.publishes_to, card, diagnostics),
        Spec::Model(spec) => validate_publications(&spec.publishes_to, card, diagnostics),
        Spec::Agent(spec) => validate_publications(&spec.publishes_to, card, diagnostics),
        Spec::Service(spec) => validate_publications(&spec.publishes_to, card, diagnostics),
        Spec::Eval(spec) => {
            if let Err(error) = spec.validate() {
                diagnostics.push(catalog_error(card, error.into()));
            }
        }
        Spec::Drift(spec) => {
            if let Err(error) = spec.validate() {
                diagnostics.push(catalog_error(card, error.into()));
            }
        }
        Spec::Trigger(spec) => validate_trigger(spec, card, diagnostics),
        Spec::Prompt(_)
        | Spec::Workflow(_)
        | Spec::Mcp(_)
        | Spec::Policy(_)
        | Spec::Audit(_)
        | Spec::Source(_)
        | Spec::Artifact(_)
        | Spec::Experiment(_)
        | Spec::Operator(_) => {}
    }
}

fn validate_publications(
    publications: &[Ref],
    card: &AuthoredCard,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let mut seen = BTreeSet::new();
    for publication in publications {
        let Some(target) = publication.as_card_ref() else {
            continue;
        };
        if !matches!(target.kind, CardKind::Eval | CardKind::Drift) {
            diagnostics.push(catalog_error(
                card,
                WyrdError::SpecInvalidPublishTargetKind {
                    message: format!(
                        "publishes_to target {} has kind {}",
                        target.name,
                        target.kind.wire_name()
                    ),
                    details: serde_json::json!({
                        "target": target,
                        "expected_kinds": ["Eval", "Drift"]
                    }),
                },
            ));
        }
        let identity = target.to_string();
        if !seen.insert(identity.clone()) {
            diagnostics.push(catalog_error(
                card,
                WyrdError::SpecDuplicatePublishTarget {
                    message: format!("duplicate publishes_to target {identity}"),
                    details: serde_json::json!({ "target": target }),
                },
            ));
        }
    }
}

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

fn check_resolved_references(card: &AuthoredCard, diagnostics: &mut Vec<Diagnostic>) {
    let mut spec = card.spec.clone();
    ReferenceSlotVisitor::visit(&mut spec, |slot| {
        let unresolved = match slot.value {
            SlotValue::Durable(reference) => matches!(reference, Ref::Path(_)),
            SlotValue::InlineablePrompt(reference) => {
                matches!(reference, wyrd_spec::reference::InlineableRef::Path(_))
            }
            SlotValue::InlineableAgent(reference) => {
                matches!(reference, wyrd_spec::reference::InlineableRef::Path(_))
            }
        };
        if unresolved {
            diagnostics.push(catalog_error(
                card,
                WyrdError::RegistryUnresolvedPathRef {
                    message: format!("unresolved path reference at {}", slot.path),
                    details: serde_json::json!({ "field": slot.path }),
                },
            ));
        }
    });
}

fn check_heavy_artifact_constraint(cards: &[AuthoredCard], diagnostics: &mut Vec<Diagnostic>) {
    if cards.len() <= 1 {
        return;
    }
    for card in cards.iter().filter(|card| !card.artifacts.is_empty()) {
        diagnostics.push(catalog_error(
            card,
            WyrdError::RegistryHeavyArtifactNotSoleSubmission {
                message: "a submission with artifacts must be loaded alone".to_owned(),
                details: serde_json::json!({
                    "artifact_count": card.artifacts.len(),
                    "submission_count": cards.len(),
                }),
            },
        ));
    }
}

fn catalog_error(card: &AuthoredCard, error: WyrdError) -> Diagnostic {
    Diagnostic::from_wyrd_error(card.source_path.clone(), Severity::Error, None, error)
}
