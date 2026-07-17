//! Card validation - per-kind and cross-kind checks.

use std::collections::HashMap;

use wyrd_spec::envelope::Spec;

use super::error::Diagnostic;
use super::parse::AuthoredCard;

/// Validate the loaded tree and return every diagnostic found.
pub fn validate_tree(cards: &[AuthoredCard]) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    check_duplicate_identities(cards, &mut diagnostics);
    for card in cards {
        validate_card(card, &mut diagnostics);
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
                    "Duplicate card identity: kind={:?}, space={}, name={}, version={:?} (first seen in {})",
                    card.kind,
                    card.metadata.space.as_ref().map_or("<unset>", |space| space.as_str()),
                    card.metadata.name.as_str(),
                    card.metadata.version.as_ref().map(|version| version.as_str()),
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
        Spec::Eval(eval) => validate_eval(eval, card, diagnostics),
        Spec::Drift(drift) => validate_drift(drift, card, diagnostics),
        _ => {}
    }
}

fn validate_eval(
    eval: &wyrd_spec::card::eval::EvalSpec,
    card: &AuthoredCard,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if let Err(error) = wyrd_spec::vala::eval::plan::validate_dag(&eval.tasks) {
        diagnostics.push(Diagnostic {
            path: card.source_path.clone(),
            code: "WYRD_SPEC_400_INVALID_EVAL_DAG",
            message: format!("Invalid Eval task DAG: {error}"),
            context: None,
        });
    }
}

fn validate_drift(
    _drift: &wyrd_spec::card::drift::DriftSpec,
    _card: &AuthoredCard,
    _diagnostics: &mut Vec<Diagnostic>,
) {
    // Drift validation is performed by the spec validator during parsing.
}

fn check_heavy_artifact_constraint(cards: &[AuthoredCard], diagnostics: &mut Vec<Diagnostic>) {
    if cards.len() <= 1 {
        return;
    }
    for card in cards {
        if !card.artifacts.is_empty() {
            diagnostics.push(Diagnostic {
                path: card.source_path.clone(),
                code: "WYRD_REGISTRY_400_HEAVY_ARTIFACT_NOT_SOLE_SUBMISSION",
                message: "Card with artifacts cannot be part of a multi-card submission"
                    .to_string(),
                context: Some(serde_json::json!({
                    "artifact_count": card.artifacts.len(),
                    "batch_size": cards.len(),
                })),
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use wyrd_semver::VersionBlock;
    use wyrd_spec::api_version::ApiVersion;
    use wyrd_spec::card::service::ServiceSpec;
    use wyrd_spec::envelope::{CardKind, Metadata, Spec};
    use wyrd_spec::ids::{CardName, SpaceName};

    fn card(name: &str) -> AuthoredCard {
        AuthoredCard {
            source_path: PathBuf::from(format!("{name}.yaml")),
            api_version: ApiVersion::v1(),
            kind: CardKind::Service,
            metadata: Metadata {
                name: CardName::new(name).unwrap(),
                version: Some(VersionBlock::parse("1.0.0").unwrap().into()),
                bump: None,
                space: Some(SpaceName::new("default").unwrap()),
                uid: None,
                labels: Default::default(),
                annotations: Default::default(),
                spec_hash: None,
                artifact_hash: None,
                origin: None,
            },
            spec: Spec::Service(ServiceSpec::default()),
            artifacts: Vec::new(),
        }
    }

    #[test]
    fn validate_rejects_duplicate_identity() {
        let first = card("service");
        let mut second = first.clone();
        second.source_path = PathBuf::from("service-copy.yaml");

        let diagnostics = validate_tree(&[first, second]);

        assert!(
            diagnostics
                .iter()
                .any(|diagnostic| diagnostic.message.contains("Duplicate card identity"))
        );
    }
}
