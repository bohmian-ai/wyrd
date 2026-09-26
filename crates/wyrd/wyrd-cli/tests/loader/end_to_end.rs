use std::path::{Path, PathBuf};

use wyrd_loader::{build_submissions, load};
use wyrd_spec::card::trigger::TriggerActivation;
use wyrd_spec::envelope::{CardKind, Spec};
use wyrd_spec::reference::{InlineableRef, Ref};
use wyrd_spec::refs::{ReferenceSlotVisitor, SlotValue};

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/loader")
        .join(name)
}

/// Load the canonical Service tree and prove all references resolve before submission.
///
/// The tree binds sibling Drift and Eval Verifiers through `verified_by`,
/// covering a referenced Trigger Card, an inline activation, and a referenced
/// failure Operator, so every binding slot must leave the loader resolved.
#[test]
fn load_end_to_end_reference_tree() {
    let tree = load(&fixture("end_to_end")).expect("canonical loader fixture must load");

    assert_eq!(tree.cards.len(), 11);
    assert!(tree.diagnostics.is_empty());
    assert_eq!(
        tree.cards.last().map(|card| &card.submission.kind),
        Some(&CardKind::Service)
    );
    let position = |name: &str| {
        tree.cards
            .iter()
            .position(|card| card.submission.metadata.name.as_str() == name)
            .expect("fixture card must be present")
    };
    let service_position = position("churn-response-service");
    assert!(position("churn-classifier-drift") < service_position);
    assert!(position("churn-classifier") < service_position);
    assert!(position("churn-triage-eval") < service_position);
    assert!(position("churn-triage") < service_position);
    assert!(position("retention-runbook") < service_position);
    assert!(position("slack-ops-alerts") < service_position);
    assert!(position("churn-classifier-drift-schedule") < service_position);
    assert!(position("churn-triage-eval-ready") < service_position);

    let mut saw_materialized_file = false;
    for loaded in &tree.cards {
        let mut spec =
            Spec::from_kind_and_value(&loaded.submission.kind, loaded.submission.spec.clone())
                .expect("loaded submission must retain a typed spec");
        ReferenceSlotVisitor::visit(&mut spec, |slot| match slot.value {
            SlotValue::Durable(reference) => {
                assert!(matches!(reference, Ref::Ref(_) | Ref::Sibling { .. }));
            }
            SlotValue::InlineablePrompt(reference) => {
                assert!(matches!(
                    reference,
                    InlineableRef::Ref(_)
                        | InlineableRef::Sibling { .. }
                        | InlineableRef::Inline(_)
                ));
            }
            SlotValue::InlineableAgent(reference) => {
                assert!(matches!(
                    reference,
                    InlineableRef::Ref(_)
                        | InlineableRef::Sibling { .. }
                        | InlineableRef::Inline(_)
                ));
            }
            SlotValue::InlineableTrigger(reference) => {
                assert!(matches!(
                    reference,
                    InlineableRef::Ref(_)
                        | InlineableRef::Sibling { .. }
                        | InlineableRef::Inline(_)
                ));
            }
            SlotValue::InlineableOperator(reference) => {
                assert!(matches!(
                    reference,
                    InlineableRef::Ref(_)
                        | InlineableRef::Sibling { .. }
                        | InlineableRef::Inline(_)
                ));
            }
        });
        if loaded.submission.metadata.name.as_str() == "retention-runbook" {
            saw_materialized_file = loaded
                .submission
                .spec
                .to_string()
                .contains("Walk the steps");
        }
        if loaded.submission.metadata.name.as_str() == "churn-response-service" {
            let Spec::Service(service) = &spec else {
                panic!("churn-response-service must remain a Service spec");
            };
            let [model_binding] = service.components[0].verified_by.as_slice() else {
                panic!("model component must carry exactly one verification binding");
            };
            assert!(matches!(model_binding.verifier, Ref::Sibling { .. }));
            assert!(matches!(
                model_binding.runs_on,
                InlineableRef::Sibling { .. }
            ));
            assert!(matches!(
                model_binding.on_failure.as_slice(),
                [InlineableRef::Sibling { .. }]
            ));
            let [agent_binding] = service.components[1].verified_by.as_slice() else {
                panic!("agent1 component must carry exactly one verification binding");
            };
            assert!(matches!(
                &agent_binding.runs_on,
                InlineableRef::Inline(trigger)
                    if matches!(trigger.activation, TriggerActivation::ObservationsReady { .. })
            ));

            let [service_binding] = service.verified_by.as_slice() else {
                panic!("the Service must carry exactly one Service-level binding");
            };
            assert!(matches!(service_binding.verifier, Ref::Ref(_)));
            assert!(matches!(service_binding.runs_on, InlineableRef::Ref(_)));
            assert!(matches!(
                service_binding.on_failure.as_slice(),
                [InlineableRef::Inline(_)]
            ));
        }
        if loaded.submission.metadata.name.as_str() == "retention-runbook" {
            let Spec::Agent(agent) = &spec else {
                panic!("retention-runbook must remain an Agent spec");
            };
            let [standalone_binding] = agent.verified_by.as_slice() else {
                panic!("the standalone Agent must carry exactly one binding");
            };
            assert!(matches!(standalone_binding.verifier, Ref::Ref(_)));
            assert!(matches!(standalone_binding.runs_on, InlineableRef::Ref(_)));
            assert!(matches!(
                standalone_binding.on_failure.as_slice(),
                [InlineableRef::Ref(_)]
            ));
        }
        if loaded.submission.metadata.name.as_str() == "churn-triage-eval-ready" {
            let Spec::Trigger(trigger) = &spec else {
                panic!("eval activation trigger must remain a Trigger spec");
            };
            assert_eq!(trigger.activation, TriggerActivation::ObservationsReady {});
        }
    }
    assert!(saw_materialized_file);

    let submissions = build_submissions(tree).expect("loaded tree is wire-ready");
    assert_eq!(submissions.len(), 11);
}

#[test]
fn load_end_to_end_config_defaults() {
    let path = fixture("config_defaults/service.yaml");
    let tree = load(&path).expect("workspace defaults fixture must load");
    let second = load(&path).expect("workspace defaults must be deterministic");

    assert_eq!(tree.cards.len(), 1);
    let metadata = &tree.cards[0].submission.metadata;
    assert_eq!(
        metadata.space.as_ref().map(ToString::to_string).as_deref(),
        Some("team-default")
    );
    assert_eq!(
        metadata
            .labels
            .iter()
            .find(|(key, _)| key.as_str() == "environment")
            .map(|(_, value)| value.as_str()),
        Some("test")
    );
    assert!(
        metadata
            .labels
            .iter()
            .any(|(key, value)| { key.as_str() == "tier" && value.as_str() == "entrypoint" })
    );
    assert_eq!(
        serde_json::to_value(metadata).expect("metadata serializes"),
        serde_json::to_value(&second.cards[0].submission.metadata).expect("metadata serializes")
    );
}
