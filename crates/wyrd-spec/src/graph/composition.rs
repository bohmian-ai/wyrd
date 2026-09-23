//! Service-root composition rules for composite Card submissions.

use std::collections::BTreeSet;

use super::{GraphError, RootPick, identity_key, submission_card_ref};
use crate::card::agent::AgentSpec;
use crate::card::operator::{OperatorAction, OperatorSpec};
use crate::card::trigger::TriggerSpec;
use crate::card::verifier::{OWNER_OCCURRENCE_KEY, VerificationBinding, VerifierImplementation};
use crate::card::workflow::WorkflowAction;
use crate::envelope::{CardKind, Spec};
use crate::error::WyrdError;
use crate::reference::InlineableRef;
use crate::registry::CardSubmission;
use crate::vala::eval::{EvalSpec, EvalTask};

/// Validate peer ownership and verification binding links in a submission graph.
///
/// Service components represent runtime-owned dependencies. Verifier, Trigger,
/// Operator, Audit, and Source remain peer cards and cannot appear in
/// `Service.spec.components`. When the selected root is a Service, every
/// Verifier submitted in the same bundle must also be the target of a
/// `verified_by` binding on the Service, one of its components, or a submitted
/// standalone Agent. Standalone peer-card submissions are intentionally
/// unaffected.
///
/// # Errors
/// Returns [`GraphError::InvalidSpec`] when a submitted spec cannot be decoded
/// or a Service repeats a component alias or uses the reserved
/// [`OWNER_OCCURRENCE_KEY`] as one (the alias keys a component's verification
/// bindings, so it must be unique within its Service and distinct from the
/// owner's own occurrence),
/// [`GraphError::InvalidServiceComponentKind`] when a Service owns a peer-only
/// card, or [`GraphError::UnboundVerifierPeer`] when a Service-root bundle
/// includes a Verifier no local subject binds.
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
        let mut aliases = BTreeSet::new();
        for (index, component) in service.components.iter().enumerate() {
            if component.alias == OWNER_OCCURRENCE_KEY {
                return Err(GraphError::InvalidSpec {
                    message: format!(
                        "spec.components[{index}].alias {OWNER_OCCURRENCE_KEY:?} is reserved for the Service's own bindings"
                    ),
                });
            }
            if !aliases.insert(component.alias.as_str()) {
                return Err(GraphError::InvalidSpec {
                    message: format!(
                        "spec.components[{index}].alias {:?} repeats an earlier component alias",
                        component.alias
                    ),
                });
            }
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

    let bound = decoded
        .iter()
        .flat_map(Spec::binding_sites)
        .filter(|site| !site.nested)
        .flat_map(|site| site.bindings)
        .filter_map(|binding| binding.verifier.as_card_ref())
        .map(identity_key)
        .collect::<BTreeSet<_>>();

    for submission in submissions {
        if submission.kind != CardKind::Verifier {
            continue;
        }
        let peer = submission_card_ref(submission).ok_or(GraphError::Empty)?;
        if !bound.contains(&identity_key(&peer)) {
            return Err(GraphError::UnboundVerifierPeer {
                root: Box::new(root.root.clone()),
                peer: Box::new(peer),
            });
        }
    }

    Ok(())
}

/// Return every stable validation error for one `verified_by` list.
///
/// Verification bindings are part of the shared Card contract, so loaders and
/// server registration use this pure projection rather than maintaining
/// independent kind, duplicate, and unsupported-action checks. Path forms are
/// resolved by their owning boundary; only resolved durable references and
/// inline bodies participate.
///
/// The checks are: the Verifier reference names a `Verifier` Card; one subject
/// occurrence binds each Verifier version at most once; `runs_on` names a
/// `Trigger` Card or is inline; each `on_failure` entry names an `Operator`
/// Card or is inline; one binding lists each Operator at most once; and no
/// bound Operator uses the non-invocable `workflow` action.
///
/// Duplicate detection uses the canonical named identity
/// ([`crate::reference::CardRef::identity_key`]) for referenced Verifiers and
/// Operators, so the optional server-managed `uid` cannot make one exact
/// version look distinct,
/// and typed [`OperatorSpec`] equality for inline Operators, so an author
/// cannot repeat the same side effect by inlining it twice.
#[must_use]
pub fn binding_validation_errors(bindings: &[VerificationBinding], field: &str) -> Vec<WyrdError> {
    let mut seen_verifiers = BTreeSet::new();
    let mut errors = Vec::new();
    for (index, binding) in bindings.iter().enumerate() {
        let binding_field = format!("{field}[{index}]");
        if let Some(verifier) = binding.verifier.as_card_ref() {
            if verifier.kind != CardKind::Verifier {
                errors.push(WyrdError::SpecInvalidVerifierRefKind {
                    message: format!(
                        "{binding_field}.verifier {} has kind {}",
                        verifier.name,
                        verifier.kind.wire_name()
                    ),
                    details: serde_json::json!({
                        "field": format!("{binding_field}.verifier"),
                        "target": verifier,
                        "expected_kind": "Verifier"
                    }),
                });
            }
            if !seen_verifiers.insert(verifier.identity_key()) {
                errors.push(WyrdError::SpecDuplicateVerificationBinding {
                    message: format!("duplicate {field} Verifier {verifier}"),
                    details: serde_json::json!({ "field": binding_field, "verifier": verifier }),
                });
            }
        }
        check_runs_on(&binding.runs_on, &binding_field, &mut errors);
        check_on_failure(&binding.on_failure, &binding_field, &mut errors);
    }
    errors
}

/// Push an error when a referenced `runs_on` names a non-Trigger Card.
fn check_runs_on(
    runs_on: &InlineableRef<TriggerSpec>,
    binding_field: &str,
    errors: &mut Vec<WyrdError>,
) {
    let Some(trigger) = runs_on.as_card_ref() else {
        return;
    };
    if trigger.kind != CardKind::Trigger {
        errors.push(WyrdError::SpecInvalidBindingRefKind {
            message: format!(
                "{binding_field}.runs_on {} has kind {}",
                trigger.name,
                trigger.kind.wire_name()
            ),
            details: serde_json::json!({
                "field": format!("{binding_field}.runs_on"),
                "target": trigger,
                "expected_kind": "Trigger"
            }),
        });
    }
}

/// Push kind, duplicate, and unsupported-action errors for one `on_failure` list.
fn check_on_failure(
    on_failure: &[InlineableRef<OperatorSpec>],
    binding_field: &str,
    errors: &mut Vec<WyrdError>,
) {
    let mut seen_operators = BTreeSet::new();
    // ponytail: inline specs have no total order, so equality is a linear scan
    // over one binding's `on_failure` list. Key them if a list ever grows large.
    let mut seen_inline_operators: Vec<&OperatorSpec> = Vec::new();
    for (index, operator) in on_failure.iter().enumerate() {
        let operator_field = format!("{binding_field}.on_failure[{index}]");
        match operator {
            InlineableRef::Inline(spec) => {
                if matches!(spec.action, OperatorAction::Workflow { .. }) {
                    errors.push(unsupported_workflow_action(&operator_field));
                    continue;
                }
                if seen_inline_operators
                    .iter()
                    .any(|seen| *seen == spec.as_ref())
                {
                    errors.push(WyrdError::SpecDuplicateBindingOperator {
                        message: format!(
                            "duplicate {binding_field}.on_failure inline Operator at {operator_field}"
                        ),
                        details: serde_json::json!({
                            "field": operator_field,
                            "operator": spec
                        }),
                    });
                    continue;
                }
                seen_inline_operators.push(spec.as_ref());
            }
            InlineableRef::Ref(card_ref) | InlineableRef::Sibling { sibling: card_ref } => {
                if card_ref.kind != CardKind::Operator {
                    errors.push(WyrdError::SpecInvalidBindingRefKind {
                        message: format!(
                            "{operator_field} {} has kind {}",
                            card_ref.name,
                            card_ref.kind.wire_name()
                        ),
                        details: serde_json::json!({
                            "field": operator_field,
                            "target": card_ref,
                            "expected_kind": "Operator"
                        }),
                    });
                    continue;
                }
                if !seen_operators.insert(card_ref.identity_key()) {
                    errors.push(WyrdError::SpecDuplicateBindingOperator {
                        message: format!(
                            "duplicate {binding_field}.on_failure Operator {card_ref}"
                        ),
                        details: serde_json::json!({
                            "field": operator_field,
                            "operator": card_ref
                        }),
                    });
                }
            }
            InlineableRef::Path(_) => {}
        }
    }
}

/// One `verified_by` list reachable from a submitted spec, with its field path.
///
/// [`Spec::binding_sites`] is the single enumeration of these locations. A new
/// binding location is added there once, and every validator that consumes it
/// — loader diagnostics, registration refusal, and composite peer ownership —
/// picks it up without re-deriving the list.
pub struct BindingSite<'a> {
    /// Dotted field path of the list on the submitted spec.
    pub field: String,
    /// The bindings authored at this location.
    pub bindings: &'a [VerificationBinding],
    /// Whether the list was reached through an inline Agent body.
    ///
    /// Only a Service, a Service component occurrence, and a standalone Agent
    /// may own a binding. An inline Agent decodes `verified_by` because it is
    /// the same `AgentSpec`, so a nested site exists structurally and must be
    /// refused rather than resolved.
    pub nested: bool,
}

impl Spec {
    /// Enumerate every `verified_by` list this spec carries, legal or not.
    ///
    /// Top-level sites are the three legal binding owners. Nested sites are the
    /// inline Agent bodies a Workflow step or an Eval LLM judge can hold; they
    /// are enumerated so that [`spec_binding_errors`] can refuse them instead
    /// of letting resolution, UID pinning, and relationship derivation run on a
    /// binding the contract does not allow.
    #[must_use]
    pub fn binding_sites(&self) -> Vec<BindingSite<'_>> {
        let mut sites = Vec::new();
        match self {
            Self::Agent(agent) => {
                sites.push(BindingSite {
                    field: "spec.verified_by".to_owned(),
                    bindings: &agent.verified_by,
                    nested: false,
                });
            }
            Self::Service(service) => {
                sites.push(BindingSite {
                    field: "spec.verified_by".to_owned(),
                    bindings: &service.verified_by,
                    nested: false,
                });
                for (index, component) in service.components.iter().enumerate() {
                    sites.push(BindingSite {
                        field: format!("spec.components[{index}].verified_by"),
                        bindings: &component.verified_by,
                        nested: false,
                    });
                }
            }
            Self::Workflow(workflow) => {
                for (index, step) in workflow.steps.iter().enumerate() {
                    if let WorkflowAction::Agent(InlineableRef::Inline(agent)) = &step.action {
                        push_nested_agent(
                            agent,
                            &format!("spec.steps[{index}].action.Agent"),
                            &mut sites,
                        );
                    }
                }
            }
            Self::Verifier(verifier) => {
                if let VerifierImplementation::Eval(eval) = &verifier.implementation {
                    push_nested_judges(eval, "spec.implementation.spec", &mut sites);
                }
            }
            Self::Data(_)
            | Self::Model(_)
            | Self::Prompt(_)
            | Self::Mcp(_)
            | Self::Policy(_)
            | Self::Audit(_)
            | Self::Source(_)
            | Self::Trigger(_)
            | Self::Artifact(_)
            | Self::Experiment(_)
            | Self::Operator(_) => {}
        }
        sites
    }
}

/// Record one inline Agent body's binding list as a nested site.
fn push_nested_agent<'a>(agent: &'a AgentSpec, prefix: &str, sites: &mut Vec<BindingSite<'a>>) {
    sites.push(BindingSite {
        field: format!("{prefix}.verified_by"),
        bindings: &agent.verified_by,
        nested: true,
    });
}

/// Record every inline LLM-judge Agent body inside one Eval payload.
fn push_nested_judges<'a>(eval: &'a EvalSpec, prefix: &str, sites: &mut Vec<BindingSite<'a>>) {
    for (task_id, task) in &eval.tasks {
        let EvalTask::LlmJudge(judge) = task else {
            continue;
        };
        if let InlineableRef::Inline(agent) = &judge.judge_ref {
            push_nested_agent(
                agent,
                &format!("{prefix}.tasks[{}].LlmJudge.judge_ref", task_id.as_str()),
                sites,
            );
        }
    }
}

/// Return every verification-binding error one submitted spec carries.
///
/// This is the one entry point for binding validation. It refuses any binding
/// authored inside an inline Agent body and runs
/// [`binding_validation_errors`] on each legal top-level list, so the loader
/// and the registration path cannot drift apart on either question.
#[must_use]
pub fn spec_binding_errors(spec: &Spec) -> Vec<WyrdError> {
    spec.binding_sites()
        .into_iter()
        .flat_map(|site| {
            if site.nested {
                if site.bindings.is_empty() {
                    Vec::new()
                } else {
                    vec![nested_binding_error(&site.field)]
                }
            } else {
                binding_validation_errors(site.bindings, &site.field)
            }
        })
        .collect()
}

/// Build the refusal for a binding authored outside a legal location.
fn nested_binding_error(field: &str) -> WyrdError {
    WyrdError::registry_invalid_card_spec(format!(
        "{field} is not a verification binding location; bind on a Service, \
         a Service component occurrence, or a standalone Agent"
    ))
}

/// Build the stable refusal for a `workflow` Operator bound to `on_failure`.
fn unsupported_workflow_action(field: &str) -> WyrdError {
    WyrdError::SpecUnsupportedOperatorAction {
        message: format!("{field} uses the workflow action, which is not invocable"),
        details: serde_json::json!({ "field": field, "action": "workflow" }),
    }
}

/// Return whether a Card kind participates beside a Service instead of inside it.
fn is_peer_only_component(kind: &CardKind) -> bool {
    matches!(
        kind,
        CardKind::Verifier
            | CardKind::Trigger
            | CardKind::Operator
            | CardKind::Audit
            | CardKind::Source
    )
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use wyrd_semver::{VersionBlock, VersionSpec};

    use super::{binding_validation_errors, spec_binding_errors, validate_composition};
    use crate::api_version::ApiVersion;
    use crate::card::verifier::{OWNER_OCCURRENCE_KEY, VerificationBinding};
    use crate::envelope::Spec;
    use crate::envelope::{CardKind, Metadata};
    use crate::graph::{GraphError, RootPick};
    use crate::reference::CardRef;
    use crate::registry::CardSubmission;

    /// Build a stable Card reference for composition fixtures.
    ///
    /// # Panics
    /// Panics when the fixture Card name, version, or space is not a valid
    /// typed identifier.
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
    ///
    /// # Panics
    /// Panics when the fixture Card name, version, or space is not a valid
    /// typed identifier.
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

    /// Build the smallest valid Eval-backed Verifier body used by composition tests.
    fn verifier_spec() -> serde_json::Value {
        json!({ "implementation": { "kind": "eval", "spec": { "tasks": {} } } })
    }

    /// Build one `verified_by` entry binding `verifier` with an inline activation.
    fn binding(verifier: &CardRef) -> serde_json::Value {
        json!({ "verifier": verifier, "runs_on": { "kind": "observations_ready" } })
    }

    /// Reject a Service whose components repeat an alias.
    ///
    /// # Panics
    /// Panics when the submission is accepted or refused for another reason.
    #[test]
    fn rejects_duplicate_component_alias() {
        let prompt = card_ref(CardKind::Prompt, "system");
        let service = submission(
            CardKind::Service,
            "app",
            json!({ "components": [
                { "alias": "retriever", "ref": prompt },
                { "alias": "retriever", "ref": prompt },
            ] }),
        );
        let root = RootPick {
            root: card_ref(CardKind::Service, "app"),
        };

        let error = validate_composition(&[service], &root)
            .expect_err("component aliases are unique within a Service");

        assert!(matches!(
            error,
            GraphError::InvalidSpec { message } if message.contains("spec.components[1].alias")
        ));
    }

    /// Reject a component alias equal to the reserved owner occurrence key.
    ///
    /// # Panics
    /// Panics when the submission is accepted or refused for another reason.
    #[test]
    fn rejects_reserved_owner_occurrence_alias() {
        let prompt = card_ref(CardKind::Prompt, "system");
        let service = submission(
            CardKind::Service,
            "app",
            json!({ "components": [{ "alias": OWNER_OCCURRENCE_KEY, "ref": prompt }] }),
        );
        let root = RootPick {
            root: card_ref(CardKind::Service, "app"),
        };

        let error = validate_composition(&[service], &root)
            .expect_err("the owner occurrence key is not a component alias");

        assert!(matches!(
            error,
            GraphError::InvalidSpec { message } if message.contains("is reserved")
        ));
    }

    /// Reject a Verifier modeled as a Service-owned component.
    ///
    /// # Panics
    /// Panics when the submission is accepted, or when the refusal is not the
    /// peer-only component error naming that alias, field, and Verifier.
    #[test]
    fn rejects_peer_only_service_component() {
        let verifier = card_ref(CardKind::Verifier, "quality");
        let service = submission(
            CardKind::Service,
            "app",
            json!({
                "components": [{
                    "alias": "quality",
                    "ref": verifier,
                }]
            }),
        );
        let root = RootPick {
            root: card_ref(CardKind::Service, "app"),
        };

        let error = validate_composition(&[service], &root)
            .expect_err("Verifier cannot be a Service component");

        assert!(matches!(
            error,
            GraphError::InvalidServiceComponentKind {
                alias,
                field,
                component,
                ..
            } if alias == "quality"
                && field == "spec.components[0].ref"
                && component.kind == CardKind::Verifier
        ));
    }

    /// Reject a Verifier peer in a Service-root bundle when no submission binds it.
    ///
    /// # Panics
    /// Panics when the bundle is accepted, or when the refusal is not the
    /// unbound-peer error naming that Verifier.
    #[test]
    fn rejects_unbound_verifier_in_service_root_bundle() {
        let service = submission(CardKind::Service, "app", service_spec());
        let verifier = submission(CardKind::Verifier, "quality", verifier_spec());
        let root = RootPick {
            root: card_ref(CardKind::Service, "app"),
        };

        let error = validate_composition(&[service, verifier], &root)
            .expect_err("Service-root Verifier requires a local binding");

        assert!(matches!(
            error,
            GraphError::UnboundVerifierPeer { peer, .. }
                if peer.kind == CardKind::Verifier && peer.name.as_str() == "quality"
        ));
    }

    /// Accept a Service-root Verifier peer when the submitted Service binds it.
    ///
    /// # Panics
    /// Panics when the bound Verifier peer is refused.
    #[test]
    fn accepts_bound_verifier_in_service_root_bundle() {
        let verifier_ref = card_ref(CardKind::Verifier, "quality");
        let service = submission(
            CardKind::Service,
            "app",
            json!({ "verified_by": [binding(&verifier_ref)] }),
        );
        let verifier = submission(CardKind::Verifier, "quality", verifier_spec());
        let root = RootPick {
            root: card_ref(CardKind::Service, "app"),
        };

        validate_composition(&[service, verifier], &root)
            .expect("bound Verifier is a valid Service peer");
    }

    /// Accept a Verifier peer bound to one component occurrence in a Service-root bundle.
    ///
    /// # Panics
    /// Panics when the component-bound Verifier peer is refused.
    #[test]
    fn accepts_component_bound_verifier_in_service_root_bundle() {
        let verifier_ref = card_ref(CardKind::Verifier, "quality");
        let model_ref = card_ref(CardKind::Model, "classifier");
        let service = submission(
            CardKind::Service,
            "app",
            json!({
                "components": [{
                    "alias": "classifier",
                    "ref": model_ref,
                    "verified_by": [binding(&verifier_ref)],
                }]
            }),
        );
        let verifier = submission(CardKind::Verifier, "quality", verifier_spec());
        let root = RootPick {
            root: card_ref(CardKind::Service, "app"),
        };

        validate_composition(&[service, verifier], &root)
            .expect("component-bound Verifier is a valid Service peer");
    }

    /// Preserve standalone Verifier registration without requiring a binding.
    ///
    /// # Panics
    /// Panics when a standalone Verifier submission is refused.
    #[test]
    fn accepts_standalone_verifier_submission() {
        let verifier = submission(CardKind::Verifier, "quality", verifier_spec());
        let root = RootPick {
            root: card_ref(CardKind::Verifier, "quality"),
        };

        validate_composition(&[verifier], &root).expect("standalone Verifier remains valid");
    }

    /// Reject two bindings to the same exact Verifier version on one subject.
    ///
    /// # Panics
    /// Panics when the binding fixtures fail to decode, or when the repeated
    /// Verifier is not reported as the single stable duplicate-binding error.
    #[test]
    fn publication_validation_rejects_duplicate_targets() {
        let verifier = card_ref(CardKind::Verifier, "quality");
        let bindings: Vec<VerificationBinding> =
            serde_json::from_value(json!([binding(&verifier), binding(&verifier)]))
                .expect("binding fixtures decode");

        let errors = binding_validation_errors(&bindings, "spec.verified_by");

        assert_eq!(errors.len(), 1, "{errors:?}");
        assert_eq!(
            errors[0].code(),
            "WYRD_SPEC_400_DUPLICATE_VERIFICATION_BINDING"
        );
    }

    /// Reject a `verifier` reference naming any Card kind but `Verifier`.
    ///
    /// # Panics
    /// Panics when the binding fixture fails to decode, or when the wrong-kind
    /// target is not reported as the single stable invalid-Verifier-ref error.
    #[test]
    fn binding_validation_rejects_non_verifier_verifier_ref() {
        let bindings: Vec<VerificationBinding> =
            serde_json::from_value(json!([binding(&card_ref(CardKind::Model, "classifier"))]))
                .expect("binding fixture decodes");

        let errors = binding_validation_errors(&bindings, "spec.verified_by");

        assert_eq!(errors.len(), 1, "{errors:?}");
        assert_eq!(errors[0].code(), "WYRD_SPEC_400_INVALID_VERIFIER_REF_KIND");
    }

    /// Reject a referenced `runs_on` naming any Card kind but `Trigger`.
    ///
    /// # Panics
    /// Panics when the binding fixture fails to decode, or when the wrong-kind
    /// `runs_on` is not reported as the single stable invalid-binding-ref error.
    #[test]
    fn binding_validation_rejects_non_trigger_runs_on_ref() {
        let bindings: Vec<VerificationBinding> = serde_json::from_value(json!([{
            "verifier": card_ref(CardKind::Verifier, "quality"),
            "runs_on": card_ref(CardKind::Data, "hourly"),
        }]))
        .expect("binding fixture decodes");

        let errors = binding_validation_errors(&bindings, "spec.verified_by");

        assert_eq!(errors.len(), 1, "{errors:?}");
        assert_eq!(errors[0].code(), "WYRD_SPEC_400_INVALID_BINDING_REF_KIND");
    }

    /// Reject an `on_failure` reference naming any Card kind but `Operator`.
    ///
    /// # Panics
    /// Panics when the binding fixture fails to decode, or when the wrong-kind
    /// `on_failure` entry is not reported as the single stable
    /// invalid-binding-ref error.
    #[test]
    fn binding_validation_rejects_non_operator_on_failure_ref() {
        let bindings: Vec<VerificationBinding> = serde_json::from_value(json!([{
            "verifier": card_ref(CardKind::Verifier, "quality"),
            "runs_on": { "kind": "observations_ready" },
            "on_failure": [card_ref(CardKind::Workflow, "rollback")],
        }]))
        .expect("binding fixture decodes");

        let errors = binding_validation_errors(&bindings, "spec.verified_by");

        assert_eq!(errors.len(), 1, "{errors:?}");
        assert_eq!(errors[0].code(), "WYRD_SPEC_400_INVALID_BINDING_REF_KIND");
    }

    /// Reject one binding listing the same Operator identity twice.
    ///
    /// # Panics
    /// Panics when the binding fixture fails to decode, or when the repeated
    /// Operator is not reported as the single stable duplicate-Operator error.
    #[test]
    fn binding_validation_rejects_duplicate_on_failure_operator() {
        let operator = card_ref(CardKind::Operator, "page");
        let bindings: Vec<VerificationBinding> = serde_json::from_value(json!([{
            "verifier": card_ref(CardKind::Verifier, "quality"),
            "runs_on": { "kind": "observations_ready" },
            "on_failure": [operator, operator],
        }]))
        .expect("binding fixture decodes");

        let errors = binding_validation_errors(&bindings, "spec.verified_by");

        assert_eq!(errors.len(), 1, "{errors:?}");
        assert_eq!(errors[0].code(), "WYRD_SPEC_400_DUPLICATE_BINDING_OPERATOR");
    }

    /// Reject two bindings to one Verifier version that differ only by UID.
    ///
    /// Registry resolution, authorization, and graph ordering all key on
    /// [`crate::reference::CardRef::identity_key`], which ignores the optional
    /// server-managed UID. Duplicate detection has to use the same key, or a
    /// direct HTTP caller can bind one exact version twice by presenting the
    /// UID on only one copy.
    ///
    /// # Panics
    /// Panics when the binding fixtures fail to decode, when the test UID is
    /// not a valid [`crate::ids::CardUid`], or when the duplicate is not
    /// reported as the single stable duplicate-binding error.
    #[test]
    fn binding_validation_rejects_duplicate_verifier_differing_only_by_uid() {
        let verifier = card_ref(CardKind::Verifier, "quality");
        let mut uid_bearing = verifier.clone();
        uid_bearing.uid = Some(
            crate::ids::CardUid::new("01890f28-7c4a-7cc3-98e7-4f4a3c2d1b11")
                .expect("test uid is valid"),
        );
        let bindings: Vec<VerificationBinding> =
            serde_json::from_value(json!([binding(&verifier), binding(&uid_bearing)]))
                .expect("binding fixtures decode");

        let errors = binding_validation_errors(&bindings, "spec.verified_by");

        assert_eq!(errors.len(), 1, "{errors:?}");
        assert_eq!(
            errors[0].code(),
            "WYRD_SPEC_400_DUPLICATE_VERIFICATION_BINDING"
        );
    }

    /// Reject one binding listing one Operator twice, differing only by UID.
    ///
    /// The `on_failure` list keys referenced Operators by the same canonical
    /// identity as the Verifier target, so UID presentation cannot smuggle a
    /// repeated side effect past the duplicate check.
    ///
    /// # Panics
    /// Panics when the binding fixture fails to decode, when the test UID is
    /// not a valid [`crate::ids::CardUid`], or when the duplicate is not
    /// reported as the single stable duplicate-Operator error.
    #[test]
    fn binding_validation_rejects_duplicate_on_failure_operator_differing_only_by_uid() {
        let operator = card_ref(CardKind::Operator, "page");
        let mut uid_bearing = operator.clone();
        uid_bearing.uid = Some(
            crate::ids::CardUid::new("01890f28-7c4a-7cc3-98e7-4f4a3c2d1b12")
                .expect("test uid is valid"),
        );
        let bindings: Vec<VerificationBinding> = serde_json::from_value(json!([{
            "verifier": card_ref(CardKind::Verifier, "quality"),
            "runs_on": { "kind": "observations_ready" },
            "on_failure": [operator, uid_bearing],
        }]))
        .expect("binding fixture decodes");

        let errors = binding_validation_errors(&bindings, "spec.verified_by");

        assert_eq!(errors.len(), 1, "{errors:?}");
        assert_eq!(errors[0].code(), "WYRD_SPEC_400_DUPLICATE_BINDING_OPERATOR");
    }

    /// Reject one binding that inlines the same Operator body twice.
    ///
    /// An inline Operator carries no reference identity, so its duplicate
    /// check is typed [`OperatorSpec`] equality. Without it an author can
    /// repeat one side effect by inlining rather than referencing it.
    ///
    /// # Panics
    /// Panics when the binding fixture fails to decode, or when the repeated
    /// inline body is not reported as the single stable duplicate-Operator
    /// error.
    #[test]
    fn binding_validation_rejects_duplicate_inline_on_failure_operator() {
        let inline = json!({
            "kind": "notify",
            "channel": {
                "kind": "pager_duty",
                "severity": "critical",
                "summary": "verification failed",
            },
        });
        let bindings: Vec<VerificationBinding> = serde_json::from_value(json!([{
            "verifier": card_ref(CardKind::Verifier, "quality"),
            "runs_on": { "kind": "observations_ready" },
            "on_failure": [inline, inline],
        }]))
        .expect("binding fixture decodes");

        let errors = binding_validation_errors(&bindings, "spec.verified_by");

        assert_eq!(errors.len(), 1, "{errors:?}");
        assert_eq!(errors[0].code(), "WYRD_SPEC_400_DUPLICATE_BINDING_OPERATOR");
    }

    /// Reject an inline `on_failure` Operator that uses the `workflow` action.
    ///
    /// # Panics
    /// Panics when the binding fixture fails to decode, or when no unsupported
    /// operator-action error is reported for the inline `workflow` body.
    #[test]
    fn binding_validation_rejects_inline_workflow_operator() {
        let bindings: Vec<VerificationBinding> = serde_json::from_value(json!([{
            "verifier": card_ref(CardKind::Verifier, "quality"),
            "runs_on": { "kind": "observations_ready" },
            "on_failure": [{
                "kind": "workflow",
                "workflow_ref": card_ref(CardKind::Workflow, "rollback"),
            }],
        }]))
        .expect("binding fixture decodes");

        let errors = binding_validation_errors(&bindings, "spec.verified_by");

        assert!(
            errors
                .iter()
                .any(|error| error.code() == "WYRD_SPEC_400_UNSUPPORTED_OPERATOR_ACTION"),
            "{errors:?}"
        );
    }

    /// Refuse a binding carried by an inline Agent inside a Workflow step.
    ///
    /// `AgentSpec.verified_by` decodes wherever an Agent body is inlined, so
    /// the location restriction has to be enforced at the one binding-site
    /// owner rather than by each validator matching the top-level spec.
    ///
    /// # Panics
    /// Panics when the Workflow spec fails to decode, or when the nested binding
    /// is not refused as the single stable invalid-card-spec error.
    #[test]
    fn spec_binding_errors_refuse_a_workflow_step_inline_agent_binding() {
        let spec = Spec::from_kind_and_value(
            &CardKind::Workflow,
            json!({
                "steps": [{
                    "id": "judge",
                    "action": {
                        "type": "agent",
                        "target": {
                            "prompt": card_ref(CardKind::Prompt, "judge"),
                            "verified_by": [binding(&card_ref(CardKind::Model, "classifier"))],
                        },
                    },
                }],
            }),
        )
        .expect("the inline Agent body still decodes");

        let errors = spec_binding_errors(&spec);

        assert_eq!(errors.len(), 1, "{errors:?}");
        assert_eq!(errors[0].code(), "WYRD_REGISTRY_400_INVALID_CARD_SPEC");
    }

    /// Refuse a binding carried by an Eval Verifier's inline LLM-judge Agent.
    ///
    /// # Panics
    /// Panics when the Verifier spec fails to decode, or when the nested binding
    /// is not refused as the single stable invalid-card-spec error.
    #[test]
    fn spec_binding_errors_refuse_an_eval_judge_inline_agent_binding() {
        let spec = Spec::from_kind_and_value(
            &CardKind::Verifier,
            json!({
                "implementation": {
                    "kind": "eval",
                    "spec": {
                        "tasks": {
                            "helpful": {
                                "kind": "llm_judge",
                                "id": "helpful",
                                "judge_ref": {
                                    "prompt": card_ref(CardKind::Prompt, "judge"),
                                    "verified_by": [
                                        binding(&card_ref(CardKind::Verifier, "quality"))
                                    ],
                                },
                                "operator": "greater_than_or_equals",
                                "expected": 0.5,
                            },
                        },
                    },
                },
            }),
        )
        .expect("the inline judge Agent body still decodes");

        let errors = spec_binding_errors(&spec);

        assert_eq!(errors.len(), 1, "{errors:?}");
        assert_eq!(errors[0].code(), "WYRD_REGISTRY_400_INVALID_CARD_SPEC");
    }

    /// Keep an inline Agent with no bindings acceptable.
    ///
    /// # Panics
    /// Panics when the Workflow spec fails to decode, or when a binding-free
    /// inline Agent produces any error.
    #[test]
    fn spec_binding_errors_accept_an_inline_agent_without_bindings() {
        let spec = Spec::from_kind_and_value(
            &CardKind::Workflow,
            json!({
                "steps": [{
                    "id": "judge",
                    "action": {
                        "type": "agent",
                        "target": { "prompt": card_ref(CardKind::Prompt, "judge") },
                    },
                }],
            }),
        )
        .expect("the inline Agent body decodes");

        assert!(spec_binding_errors(&spec).is_empty());
    }
}
