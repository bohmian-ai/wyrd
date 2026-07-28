//! Shared submission graph ordering for the loader.

use wyrd_spec::error::WyrdError;
use wyrd_spec::graph::GraphError;
use wyrd_spec::registry::CardSubmission;

use super::error::Diagnostic;
use super::parse::AuthoredCard;

/// Order authored cards with the canonical graph implementation.
pub fn order_cards(cards: &[AuthoredCard]) -> Result<Vec<usize>, Vec<Diagnostic>> {
    if cards.is_empty() {
        return Err(vec![Diagnostic::invalid_envelope(
            "<loader>".into(),
            "Cannot order an empty card tree".to_string(),
        )]);
    }

    let submissions = cards
        .iter()
        .map(submission_for)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|diagnostic| vec![diagnostic])?;
    // The loader and registration service intentionally share both graph
    // construction and sorting. The loader only projects the returned nodes
    // back to its local provenance IDs.
    let graph_submissions = wyrd_spec::graph::graph_ready_submissions(&submissions)
        .map_err(|error| vec![graph_diagnostic(&error, cards, &[])])?;
    let (nodes, edges) = wyrd_spec::graph::build(&graph_submissions)
        .map_err(|error| vec![graph_diagnostic(&error, cards, &[])])?;
    let card_refs = nodes
        .iter()
        .map(|node| node.card_ref.clone())
        .collect::<Vec<_>>();

    let order = wyrd_spec::graph::topo_sort(&nodes, &edges)
        .map_err(|error| vec![graph_diagnostic(&error, cards, &card_refs)])?;
    let root = wyrd_spec::graph::pick_root(&order)
        .map_err(|error| vec![graph_diagnostic(&error, cards, &card_refs)])?;
    wyrd_spec::graph::validate_composition(&graph_submissions, &root)
        .map_err(|error| vec![graph_diagnostic(&error, cards, &card_refs)])?;
    let order = wyrd_spec::graph::root_last(order)
        .map_err(|error| vec![graph_diagnostic(&error, cards, &card_refs)])?;

    order
        .nodes
        .iter()
        .map(|node| {
            card_refs
                .iter()
                .position(|card_ref| card_ref.same_identity(&node.card_ref))
                .ok_or_else(|| {
                    Diagnostic::invalid_envelope(
                        "<loader>".into(),
                        format!(
                            "Shared graph returned an unknown card: {}",
                            node.card_ref.name
                        ),
                    )
                })
        })
        .collect::<Result<Vec<_>, _>>()
        .map_err(|diagnostic| vec![diagnostic])
}

/// Project an authored envelope into the canonical client/server submission.
pub(crate) fn submission_for(card: &AuthoredCard) -> Result<CardSubmission, Diagnostic> {
    let spec = serde_json::to_value(&card.spec).map_err(|error| {
        Diagnostic::invalid_envelope(
            card.source_path.clone(),
            format!(
                "Unable to serialize {} spec: {error}",
                card.kind.wire_name()
            ),
        )
    })?;

    Ok(CardSubmission {
        api_version: card.api_version.clone(),
        kind: card.kind.clone(),
        metadata: card.metadata.clone(),
        spec,
        artifacts: card.artifacts.clone(),
    })
}

/// Convert a shared graph failure into a source-bound loader diagnostic.
fn graph_diagnostic(
    error: &GraphError,
    cards: &[AuthoredCard],
    card_refs: &[wyrd_spec::reference::CardRef],
) -> Diagnostic {
    let path = match error {
        GraphError::Cycle { cycle } => cycle.first(),
        GraphError::InvalidServiceComponentKind { service, .. } => Some(service),
        GraphError::UnpublishedObservabilityPeer { peer, .. } => Some(peer),
        _ => None,
    }
    .and_then(|card_ref| {
        card_refs
            .iter()
            .position(|candidate| candidate.same_identity(card_ref))
    })
    .map(|index| cards[index].source_path.clone())
    .unwrap_or_else(|| "<loader>".into());

    let message = error.to_string();
    let error = match error {
        GraphError::Cycle { cycle } => WyrdError::RegistryDependencyCycle {
            message,
            details: serde_json::json!({ "participants": cycle }),
        },
        GraphError::InvalidServiceComponentKind {
            service,
            alias,
            component,
            field,
        } => WyrdError::SpecInvalidServiceComponentKind {
            message,
            details: serde_json::json!({
                "service": service,
                "alias": alias,
                "component_ref": component,
                "field": field,
            }),
        },
        GraphError::UnpublishedObservabilityPeer { root, peer } => {
            WyrdError::SpecUnpublishedObservabilityPeer {
                message,
                details: serde_json::json!({
                    "root": root,
                    "peer": peer,
                    "publisher_kinds": ["Data", "Model", "Agent", "Service"],
                }),
            }
        }
        GraphError::DuplicateIdentity { .. }
        | GraphError::Empty
        | GraphError::MissingSpace
        | GraphError::MultipleRoots { .. }
        | GraphError::InvalidSpec { .. } => WyrdError::LoaderInvalidEnvelope {
            message,
            details: serde_json::json!({ "graph_error": error.to_string() }),
        },
    };
    Diagnostic::from_wyrd_error(path, super::Severity::Error, None, &error)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use std::path::PathBuf;
    use wyrd_semver::VersionBlock;
    use wyrd_spec::api_version::ApiVersion;
    use wyrd_spec::card::service::{ServiceComponent, ServiceSpec};
    use wyrd_spec::envelope::{CardKind, Metadata, Spec};
    use wyrd_spec::ids::{CardName, SpaceName};
    use wyrd_spec::reference::Ref;

    fn metadata(name: &str) -> Metadata {
        Metadata {
            name: CardName::new(name).unwrap(),
            version: Some(VersionBlock::parse("1.0.0").unwrap().into()),
            bump: None,
            space: Some(SpaceName::new("default").unwrap()),
            uid: None,
            labels: BTreeMap::new(),
            annotations: BTreeMap::new(),
            spec_hash: None,
            artifact_hash: None,
            origin: None,
        }
    }

    /// Build an authored Card from a typed JSON spec fixture.
    fn authored(name: &str, kind: CardKind, spec: serde_json::Value) -> AuthoredCard {
        AuthoredCard {
            source_path: PathBuf::from(format!("{name}.yaml")),
            api_version: ApiVersion::v1(),
            kind: kind.clone(),
            metadata: metadata(name),
            spec: Spec::from_kind_and_value(&kind, spec).expect("test spec is valid"),
            artifacts: Vec::new(),
        }
    }

    /// Build an exact local reference for graph-order tests.
    fn card_ref(kind: CardKind, name: &str) -> wyrd_spec::reference::CardRef {
        wyrd_spec::reference::CardRef {
            kind,
            name: CardName::new(name).unwrap(),
            version: VersionBlock::parse("1.0.0").unwrap(),
            space: Some(SpaceName::new("default").unwrap()),
            uid: None,
        }
    }

    /// Build a Service fixture with an optional Service dependency.
    fn service(name: &str, dependency: Option<&str>) -> AuthoredCard {
        let components = dependency
            .map(|dependency| {
                vec![ServiceComponent {
                    alias: dependency.to_string(),
                    card_ref: Ref::Sibling {
                        sibling: wyrd_spec::reference::CardRef {
                            kind: CardKind::Service,
                            name: CardName::new(dependency).unwrap(),
                            version: VersionBlock::parse("1.0.0").unwrap(),
                            space: Some(SpaceName::new("default").unwrap()),
                            uid: None,
                        },
                    },
                    source: None,
                    config: BTreeMap::new(),
                    credential_refs: Vec::new(),
                }]
            })
            .unwrap_or_default();
        AuthoredCard {
            source_path: PathBuf::from(format!("{name}.yaml")),
            api_version: ApiVersion::v1(),
            kind: CardKind::Service,
            metadata: metadata(name),
            spec: Spec::Service(ServiceSpec {
                components,
                ..Default::default()
            }),
            artifacts: Vec::new(),
        }
    }

    #[test]
    fn order_reuses_shared_graph_and_places_dependencies_first() {
        let cards = vec![service("service", Some("model")), service("model", None)];
        let order = order_cards(&cards).unwrap();

        assert_eq!(order, vec![1, 0]);
    }

    #[test]
    fn order_surfaces_shared_graph_cycle_error() {
        let cards = vec![service("aaa", Some("bbb")), service("bbb", Some("aaa"))];
        let diagnostics = order_cards(&cards).unwrap_err();

        assert_eq!(diagnostics[0].code, "WYRD_REGISTRY_400_DEPENDENCY_CYCLE");
    }

    #[test]
    fn order_keeps_same_named_versions_as_distinct_nodes() {
        let v1 = service("service", None);
        let mut v2 = service("service", Some("service"));
        v2.metadata.version = Some(VersionBlock::parse("2.0.0").unwrap().into());

        let order = order_cards(&[v2, v1]).expect("different versions form one graph");

        assert_eq!(order, vec![1, 0]);
    }

    /// Surface the stable peer-component diagnostic at the authored Service path.
    #[test]
    fn order_rejects_eval_service_component_with_stable_code() {
        let mut service = service("app", None);
        let Spec::Service(spec) = &mut service.spec else {
            panic!("test Service fixture has a Service spec");
        };
        spec.components.push(ServiceComponent {
            alias: "quality".to_owned(),
            card_ref: Ref::Sibling {
                sibling: card_ref(CardKind::Eval, "quality"),
            },
            source: None,
            config: BTreeMap::new(),
            credential_refs: Vec::new(),
        });
        let eval = authored(
            "quality",
            CardKind::Eval,
            serde_json::json!({ "tasks": {} }),
        );

        let diagnostics = order_cards(&[service, eval]).expect_err("Eval component is invalid");

        assert_eq!(
            diagnostics[0].code,
            "WYRD_SPEC_400_INVALID_SERVICE_COMPONENT_KIND"
        );
        assert_eq!(diagnostics[0].path, PathBuf::from("app.yaml"));
    }

    /// Surface the stable orphan-peer diagnostic at the authored Eval path.
    #[test]
    fn order_rejects_unpublished_eval_peer_with_stable_code() {
        let service = service("app", None);
        let eval = authored(
            "quality",
            CardKind::Eval,
            serde_json::json!({ "tasks": {} }),
        );

        let diagnostics =
            order_cards(&[service, eval]).expect_err("Service-root Eval needs a publisher");

        assert_eq!(
            diagnostics[0].code,
            "WYRD_SPEC_400_UNPUBLISHED_OBSERVABILITY_PEER"
        );
        assert_eq!(diagnostics[0].path, PathBuf::from("quality.yaml"));
    }
}
