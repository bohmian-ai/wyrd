use std::path::{Path, PathBuf};

use wyrd_loader::{build_submissions, load};
use wyrd_spec::envelope::{CardKind, Spec};
use wyrd_spec::reference::{InlineableRef, Ref};
use wyrd_spec::refs::{ReferenceSlotVisitor, SlotValue};

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/loader")
        .join(name)
}

/// Load the canonical Service tree and prove all references resolve before submission.
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
    assert!(position("slack-ops-alerts") < position("churn-triage-eval-fail"));
    assert!(position("churn-triage") < position("churn-triage-eval-fail"));

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
            assert_eq!(service.components[0].publishes_to.len(), 1);
        }
        if loaded.submission.metadata.name.as_str() == "churn-triage-eval-fail" {
            let Spec::Trigger(trigger) = &spec else {
                panic!("eval failure trigger must remain a Trigger spec");
            };
            assert!(matches!(
                &trigger.operator_ref,
                Ref::Ref(_) | Ref::Sibling { .. }
            ));
            assert!(matches!(
                &trigger.source,
                Some(wyrd_spec::card::trigger::TriggerSource::EvalObservation {
                    eval_ref: Ref::Sibling { .. },
                    subject_filter: Some(Ref::Sibling { .. }),
                })
            ));
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
