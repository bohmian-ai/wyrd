//! Service-root composition rules for composite Card submissions.

use std::collections::BTreeSet;

use super::{GraphError, RootPick, identity_key, submission_card_ref};
use crate::envelope::{CardKind, Spec};
use crate::reference::Ref;
use crate::registry::CardSubmission;

/// Validate peer ownership and observability publication links in a submission graph.
///
/// Service components represent runtime-owned dependencies. Eval, Drift,
/// Trigger, Operator, Audit, and Source remain peer cards and cannot appear in
/// `Service.spec.components`. When the selected root is a Service, every Eval
/// or Drift submitted in the same bundle must also be the target of a
/// `publishes_to` reference from a submitted Data, Model, Agent, or Service.
/// Standalone peer-card submissions are intentionally unaffected.
///
/// # Errors
/// Returns [`GraphError::InvalidSpec`] when a submitted spec cannot be decoded,
/// [`GraphError::InvalidServiceComponentKind`] when a Service owns a peer-only
/// card, or [`GraphError::UnpublishedObservabilityPeer`] when a Service-root
/// bundle includes an Eval or Drift without a local publisher.
pub fn validate_composition(
    submissions: &[CardSubmission],
    root: &RootPick,
) -> Result<(), GraphError> {
    let decoded = submissions
        .iter()
        .map(|submission| {
            Spec::from_kind_and_value(&submission.kind, submission.spec.clone()).map_err(|error| {
                GraphError::InvalidSpec {
                    message: error.to_string(),
                }
            })
        })
        .collect::<Result<Vec<_>, _>>()?;

    for (submission, spec) in submissions.iter().zip(&decoded) {
        let Spec::Service(service) = spec else {
            continue;
        };
        let service_ref = submission_card_ref(submission).ok_or(GraphError::Empty)?;
        for (index, component) in service.components.iter().enumerate() {
            let Some(component_ref) = component.card_ref.as_card_ref() else {
                continue;
            };
            if is_peer_only_component(&component_ref.kind) {
                return Err(GraphError::InvalidServiceComponentKind {
                    service: Box::new(service_ref),
                    alias: component.alias.clone(),
                    component: Box::new(component_ref.clone()),
                    field: format!("spec.components[{index}].ref"),
                });
            }
        }
    }

    if root.root.kind != CardKind::Service {
        return Ok(());
    }

    let published = decoded
        .iter()
        .flat_map(publications)
        .filter_map(Ref::as_card_ref)
        .map(identity_key)
        .collect::<BTreeSet<_>>();

    for submission in submissions {
        if !matches!(submission.kind, CardKind::Eval | CardKind::Drift) {
            continue;
        }
        let peer = submission_card_ref(submission).ok_or(GraphError::Empty)?;
        if !published.contains(&identity_key(&peer)) {
            return Err(GraphError::UnpublishedObservabilityPeer {
                root: Box::new(root.root.clone()),
                peer: Box::new(peer),
            });
        }
    }

    Ok(())
}

/// Return whether a Card kind participates beside a Service instead of inside it.
fn is_peer_only_component(kind: &CardKind) -> bool {
    matches!(
        kind,
        CardKind::Eval
            | CardKind::Drift
            | CardKind::Trigger
            | CardKind::Operator
            | CardKind::Audit
            | CardKind::Source
    )
}

/// Return the observability publication targets declared by a publisher spec.
fn publications(spec: &Spec) -> &[Ref] {
    match spec {
        Spec::Data(spec) => &spec.publishes_to,
        Spec::Model(spec) => &spec.publishes_to,
        Spec::Agent(spec) => &spec.publishes_to,
        Spec::Service(spec) => &spec.publishes_to,
        Spec::Prompt(_)
        | Spec::Workflow(_)
        | Spec::Mcp(_)
        | Spec::Policy(_)
        | Spec::Audit(_)
        | Spec::Drift(_)
        | Spec::Eval(_)
        | Spec::Source(_)
        | Spec::Trigger(_)
        | Spec::Artifact(_)
        | Spec::Experiment(_)
        | Spec::Operator(_) => &[],
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use wyrd_semver::{VersionBlock, VersionSpec};

    use super::validate_composition;
    use crate::api_version::ApiVersion;
    use crate::envelope::{CardKind, Metadata};
    use crate::graph::{GraphError, RootPick};
    use crate::reference::CardRef;
    use crate::registry::CardSubmission;

    /// Build a stable Card reference for composition fixtures.
    fn card_ref(kind: CardKind, name: &str) -> CardRef {
        CardRef {
            kind,
            name: name.parse().expect("test Card name is valid"),
            version: VersionBlock::parse("1.0.0").expect("test version is valid"),
            space: Some("default".parse().expect("test space is valid")),
            uid: None,
        }
    }

    /// Build a graph-ready submission around a JSON spec fixture.
    fn submission(kind: CardKind, name: &str, spec: serde_json::Value) -> CardSubmission {
        let card_ref = card_ref(kind.clone(), name);
        CardSubmission {
            api_version: ApiVersion::v1(),
            kind,
            metadata: Metadata {
                name: card_ref.name,
                version: Some(VersionSpec::Pin(card_ref.version)),
                bump: None,
                space: card_ref.space,
                uid: None,
                labels: Default::default(),
                annotations: Default::default(),
                spec_hash: None,
                artifact_hash: None,
                origin: None,
            },
            spec,
            artifacts: Vec::new(),
        }
    }

    /// Build the empty Service body used by composition tests.
    fn service_spec() -> serde_json::Value {
        json!({})
    }

    /// Build the smallest valid Eval body used by composition tests.
    fn eval_spec() -> serde_json::Value {
        json!({ "tasks": {} })
    }

    /// Reject an Eval modeled as a Service-owned component.
    #[test]
    fn rejects_peer_only_service_component() {
        let eval = card_ref(CardKind::Eval, "quality");
        let service = submission(
            CardKind::Service,
            "app",
            json!({
                "components": [{
                    "alias": "quality",
                    "ref": eval,
                }]
            }),
        );
        let root = RootPick {
            root: card_ref(CardKind::Service, "app"),
        };

        let error = validate_composition(&[service], &root)
            .expect_err("Eval cannot be a Service component");

        assert!(matches!(
            error,
            GraphError::InvalidServiceComponentKind {
                alias,
                field,
                component,
                ..
            } if alias == "quality"
                && field == "spec.components[0].ref"
                && component.kind == CardKind::Eval
        ));
    }

    /// Reject an Eval peer in a Service-root bundle when no submission publishes to it.
    #[test]
    fn rejects_unpublished_eval_in_service_root_bundle() {
        let service = submission(CardKind::Service, "app", service_spec());
        let eval = submission(CardKind::Eval, "quality", eval_spec());
        let root = RootPick {
            root: card_ref(CardKind::Service, "app"),
        };

        let error = validate_composition(&[service, eval], &root)
            .expect_err("Service-root Eval requires a local publisher");

        assert!(matches!(
            error,
            GraphError::UnpublishedObservabilityPeer { peer, .. }
                if peer.kind == CardKind::Eval && peer.name.as_str() == "quality"
        ));
    }

    /// Accept a Service-root Eval peer when a submitted Service publishes to it.
    #[test]
    fn accepts_published_eval_in_service_root_bundle() {
        let eval_ref = card_ref(CardKind::Eval, "quality");
        let service = submission(
            CardKind::Service,
            "app",
            json!({ "publishes_to": [eval_ref] }),
        );
        let eval = submission(CardKind::Eval, "quality", eval_spec());
        let root = RootPick {
            root: card_ref(CardKind::Service, "app"),
        };

        validate_composition(&[service, eval], &root)
            .expect("published Eval is a valid Service peer");
    }

    /// Preserve standalone Eval registration without requiring a publisher.
    #[test]
    fn accepts_standalone_eval_submission() {
        let eval = submission(CardKind::Eval, "quality", eval_spec());
        let root = RootPick {
            root: card_ref(CardKind::Eval, "quality"),
        };

        validate_composition(&[eval], &root).expect("standalone Eval remains valid");
    }
}
