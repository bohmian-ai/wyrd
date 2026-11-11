//! Shared submission graph ordering for the loader.

use wyrd_spec::error::WyrdError;
use wyrd_spec::graph::{GraphError, GraphSubmission, spec_binding_errors};
use wyrd_spec::registry::CardSubmission;

use super::error::Diagnostic;
use super::parse::AuthoredCard;

/// Order authored cards dependencies first through the canonical graph.
///
/// The loader and the registration service share graph construction and
/// sorting, so a tree the loader orders is the tree registration accepts.
/// Each card is projected to its submission, sibling targets are matched
/// against the whole tree, and binding uniqueness is checked on those
/// matched targets before the graph is built. Each returned index points
/// into `cards` beside that card's submission, dependencies first and the
/// single root last.
///
/// # Errors
///
/// Returns every [`Diagnostic`] found, each at the authored path it came
/// from: an invalid envelope for an empty tree or a card that does not
/// project to a submission, the binding diagnostics of the matched targets,
/// and the graph diagnostic for an unresolved, ambiguous, or cyclic
/// reference, a tree without exactly one root, or an invalid composition.
pub fn order_cards(
    cards: &[AuthoredCard],
) -> Result<Vec<(usize, CardSubmission)>, Vec<Diagnostic>> {
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
    let binding_diagnostics = matched_binding_diagnostics(cards, &graph_submissions);
    if !binding_diagnostics.is_empty() {
        return Err(binding_diagnostics);
    }
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

    let mut submissions = submissions.into_iter().map(Some).collect::<Vec<_>>();
    order
        .nodes
        .iter()
        .map(|node| {
            card_refs
                .iter()
                .position(|card_ref| card_ref.same_identity(&node.card_ref))
                .and_then(|index| Some((index, submissions[index].take()?)))
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

/// Binding refusals each card carries once its siblings name their matched
/// targets.
///
/// Authored validation already ran [`spec_binding_errors`] on the authored
/// spellings; an omitted and an explicit sibling naming one submitted
/// Verifier or Operator only converge after matching, so the same owner runs
/// again on the graph-ready specs.
fn matched_binding_diagnostics(
    cards: &[AuthoredCard],
    graph_submissions: &[GraphSubmission],
) -> Vec<Diagnostic> {
    cards
        .iter()
        .zip(graph_submissions)
        .flat_map(|(card, graph)| {
            spec_binding_errors(&graph.spec).into_iter().map(|error| {
                Diagnostic::from_wyrd_error(
                    card.source_path.clone(),
                    super::Severity::Error,
                    None,
                    &error,
                )
            })
        })
        .collect()
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
        GraphError::InvalidServiceComponentKind { service, .. } => Some(service.as_ref()),
        GraphError::UnboundVerifierPeer { peer, .. } => Some(peer.as_ref()),
        _ => None,
    }
    .and_then(|card_ref| {
        card_refs
            .iter()
            .position(|candidate| candidate.same_identity(card_ref))
    })
    .map_or_else(
        || "<loader>".into(),
        |index| cards[index].source_path.clone(),
    );

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
        GraphError::UnboundVerifierPeer { root, peer } => WyrdError::SpecUnboundVerifierPeer {
            message,
            details: serde_json::json!({
                "root": root,
                "peer": peer,
                "binding_owner_kinds": ["Service", "Agent"],
            }),
        },
        GraphError::AmbiguousSibling {
            sibling,
            candidates,
        } => WyrdError::RegistryUnresolvedDependency {
            message,
            details: serde_json::json!({ "card_ref": sibling, "candidates": candidates }),
        },
        GraphError::DuplicateIdentity { .. }
        | GraphError::Empty
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
    use wyrd_spec::ids::CardName;
    use wyrd_spec::reference::Ref;

    fn metadata(name: &str) -> Metadata {
        Metadata {
            name: CardName::new(name).unwrap(),
            version: Some(VersionBlock::parse("1.0.0").unwrap().into()),
            bump: None,
            uid: None,
            labels: BTreeMap::new(),
            annotations: BTreeMap::new(),
            spec_hash: None,
            artifact_hash: None,
            origin: None,
        }
    }

    /// Build an authored Card from a typed JSON spec fixture.
    fn authored(name: &str, kind: &CardKind, spec: serde_json::Value) -> AuthoredCard {
        AuthoredCard {
            source_path: PathBuf::from(format!("{name}.yaml")),
            api_version: ApiVersion::v1(),
            kind: kind.clone(),
            metadata: metadata(name),
            spec: Spec::from_kind_and_value(kind, spec).expect("test spec is valid"),
            artifacts: Vec::new(),
        }
    }

    /// Build an exact local reference for graph-order tests.
    fn card_ref(kind: CardKind, name: &str) -> wyrd_spec::reference::CardRef {
        wyrd_spec::reference::CardRef {
            kind,
            name: CardName::new(name).unwrap(),
            version: VersionBlock::parse("1.0.0").unwrap(),
            uid: None,
        }
    }

    /// Build a Service fixture with an optional Service dependency.
    ///
    /// # Panics
    ///
    /// Panics when `dependency` is not a valid Card name.
    fn service(name: &str, dependency: Option<&str>) -> AuthoredCard {
        let components = dependency
            .map(|dependency| {
                vec![ServiceComponent {
                    alias: dependency.to_string(),
                    card_ref: Ref::Sibling {
                        sibling: wyrd_spec::reference::SiblingRef {
                            kind: CardKind::Service,
                            name: CardName::new(dependency).unwrap(),
                            version: Some(VersionBlock::parse("1.0.0").unwrap()),
                        },
                    },
                    verified_by: Vec::new(),
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

    /// The card indices of an order.
    fn indices(order: &[(usize, CardSubmission)]) -> Vec<usize> {
        order.iter().map(|(index, _)| *index).collect()
    }

    #[test]
    fn order_reuses_shared_graph_and_places_dependencies_first() {
        let cards = vec![service("service", Some("model")), service("model", None)];
        let order = order_cards(&cards).unwrap();

        assert_eq!(indices(&order), vec![1, 0]);
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

        assert_eq!(indices(&order), vec![1, 0]);
    }

    /// Surface the stable peer-component diagnostic when a Verifier is a Service component.
    ///
    /// # Panics
    ///
    /// Panics when the tree orders or its diagnostic carries another code or
    /// path.
    #[test]
    fn order_rejects_verifier_service_component_with_stable_code() {
        let mut service = service("app", None);
        let Spec::Service(spec) = &mut service.spec else {
            panic!("test Service fixture has a Service spec");
        };
        spec.components.push(ServiceComponent {
            alias: "quality".to_owned(),
            card_ref: Ref::Sibling {
                sibling: (&card_ref(CardKind::Verifier, "quality")).into(),
            },
            verified_by: Vec::new(),
            source: None,
            config: BTreeMap::new(),
            credential_refs: Vec::new(),
        });
        let verifier = authored(
            "quality",
            &CardKind::Verifier,
            serde_json::json!({
                "inputs": { "answer": { "type": "json" } },
                "requirements": [{
                    "id": "answered",
                    "statement": "The agent produced an answer.",
                    "check": { "kind": "assert", "path": "inputs.answer", "operator": "is_not_null" }
                }]
            }),
        );

        let diagnostics =
            order_cards(&[service, verifier]).expect_err("Verifier component is invalid");

        assert_eq!(
            diagnostics[0].code,
            "WYRD_SPEC_400_INVALID_SERVICE_COMPONENT_KIND"
        );
        assert_eq!(diagnostics[0].path, PathBuf::from("app.yaml"));
    }

    /// Surface the stable unbound-peer diagnostic at the authored Verifier path.
    #[test]
    fn order_rejects_unbound_verifier_peer_with_stable_code() {
        let service = service("app", None);
        let verifier = authored(
            "quality",
            &CardKind::Verifier,
            serde_json::json!({
                "inputs": { "answer": { "type": "json" } },
                "requirements": [{
                    "id": "answered",
                    "statement": "The agent produced an answer.",
                    "check": { "kind": "assert", "path": "inputs.answer", "operator": "is_not_null" }
                }]
            }),
        );

        let diagnostics =
            order_cards(&[service, verifier]).expect_err("Service-root Verifier needs a binding");

        assert_eq!(diagnostics[0].code, "WYRD_SPEC_400_UNBOUND_VERIFIER_PEER");
        assert_eq!(diagnostics[0].path, PathBuf::from("quality.yaml"));
    }

    /// An omitted and an explicit sibling naming one submitted Verifier, or
    /// one submitted Operator, are one binding target and are refused as a
    /// duplicate once matched.
    ///
    /// # Panics
    ///
    /// Panics when a mixed-spelling binding orders, or its diagnostic carries
    /// another code or path.
    #[test]
    fn order_refuses_mixed_sibling_spellings_of_one_binding_target() {
        let verifier = authored(
            "quality",
            &CardKind::Verifier,
            serde_json::json!({
                "inputs": { "answer": { "type": "json" } },
                "requirements": [{
                    "id": "answered",
                    "statement": "The agent produced an answer.",
                    "check": { "kind": "assert", "path": "inputs.answer", "operator": "is_not_null" }
                }]
            }),
        );
        let operator = authored(
            "alert",
            &CardKind::Operator,
            serde_json::json!({ "kind": "http", "method": "post", "url": "https://hooks.example.com/alert" }),
        );
        let omitted = |kind: &str, name: &str| serde_json::json!({ "sibling": { "kind": kind, "name": name } });
        let explicit = |kind: &str, name: &str| serde_json::json!({ "sibling": { "kind": kind, "name": name, "version": "1.0.0" } });
        let cases = [
            (
                serde_json::json!([
                    { "verifier": omitted("Verifier", "quality") },
                    { "verifier": explicit("Verifier", "quality") },
                ]),
                "WYRD_SPEC_400_DUPLICATE_VERIFICATION_BINDING",
            ),
            (
                serde_json::json!([{
                    "verifier": omitted("Verifier", "quality"),
                    "on_failure": [omitted("Operator", "alert"), explicit("Operator", "alert")],
                }]),
                "WYRD_SPEC_400_DUPLICATE_BINDING_OPERATOR",
            ),
        ];
        for (verified_by, code) in cases {
            let agent = authored(
                "agent",
                &CardKind::Agent,
                serde_json::json!({
                    "prompt": { "kind": "Prompt", "name": "prompt", "version": "1.0.0" },
                    "execution_route": { "kind": "native" },
                    "verified_by": verified_by,
                }),
            );
            let diagnostics = order_cards(&[agent, verifier.clone(), operator.clone()])
                .expect_err("mixed spellings name one target twice");

            assert_eq!(diagnostics[0].code, code);
            assert_eq!(diagnostics[0].path, PathBuf::from("agent.yaml"));
        }
    }
}
