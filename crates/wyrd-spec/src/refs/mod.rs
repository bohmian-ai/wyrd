//! Reference slot visitor for traversing and mutating Card references within specs.
//!
//! This module provides the canonical visitor pattern for iterating over all reference
//! slots in a Card's spec. The visitor yields mutable handles to each reference position,
//! enabling reference resolution, validation, and transformation during the loader pipeline.

use crate::envelope::Spec;
use crate::reference::{InlineableRef, Ref};

/// A yielded reference slot with metadata about its kind and a mutable handle.
pub struct SlotEntry<'a> {
    /// The path to this slot in dot notation (e.g. `spec.publishes_to[0]`).
    pub path: String,
    /// The slot value yielded for inspection or mutation.
    pub value: SlotValue<'a>,
}

/// The slot value identifies the concrete reference shape at the slot.
pub enum SlotValue<'a> {
    /// A durable-only reference slot.
    Durable(&'a mut Ref),
    /// An inlineable Skald prompt slot.
    InlineablePrompt(&'a mut InlineableRef<skald_spec::Prompt>),
    /// An inlineable Agent spec slot.
    InlineableAgent(&'a mut InlineableRef<crate::card::agent::AgentSpec>),
}

/// Canonical visitor that yields every reference slot on a `Spec`.
pub struct ReferenceSlotVisitor;

impl ReferenceSlotVisitor {
    /// Visit every reference slot on the given spec.
    ///
    /// Calls `f` once per slot with the slot's path, kind, and a mutable handle.
    /// The visitor yields slots in declaration order.
    pub fn visit<F>(spec: &mut Spec, mut f: F)
    where
        F: FnMut(SlotEntry<'_>),
    {
        match spec {
            Spec::Data(data) => data.visit(&mut f),
            Spec::Model(model) => model.visit(&mut f),
            Spec::Experiment(experiment) => experiment.visit(&mut f),
            Spec::Prompt(_) => {}
            Spec::Agent(agent) => agent.visit(&mut f),
            Spec::Workflow(workflow) => workflow.visit(&mut f),
            Spec::Eval(eval) => eval.visit(&mut f),
            Spec::Drift(drift) => drift.visit(&mut f),
            Spec::Service(service) => service.visit(&mut f),
            Spec::Policy(_) => {}
            Spec::Mcp(mcp) => mcp.visit(&mut f),
            Spec::Audit(audit) => audit.visit(&mut f),
            Spec::Artifact(artifact) => artifact.visit(&mut f),
            Spec::Trigger(trigger) => trigger.visit(&mut f),
            Spec::Operator(operator) => operator.visit(&mut f),
            Spec::Source(_) => {}
        }
    }
}

/// Trait for specs that carry reference slots.
pub trait Visit {
    /// Visit every reference slot on this spec, invoking `f` for each.
    fn visit<F>(&mut self, f: &mut F)
    where
        F: FnMut(SlotEntry<'_>);
}

impl Visit for crate::card::data::DataSpec {
    fn visit<F>(&mut self, f: &mut F)
    where
        F: FnMut(SlotEntry<'_>),
    {
        for (i, card_ref) in self.card_refs.iter_mut().enumerate() {
            f(SlotEntry {
                path: format!("spec.card_refs[{}]", i),
                value: SlotValue::Durable(card_ref),
            });
        }
        for (label, split) in &mut self.splits {
            if let crate::card::data::SplitStrategy::Materialized(card_ref) = &mut split.strategy {
                f(SlotEntry {
                    path: format!("spec.splits[{}].strategy.Materialized", label),
                    value: SlotValue::Durable(card_ref),
                });
            }
        }
        match &mut self.interface {
            crate::card::data::DataInterface::Image(meta) => {
                if let Some(card_ref) = &mut meta.manifest_ref {
                    f(SlotEntry {
                        path: "spec.interface.Image.manifest_ref".to_owned(),
                        value: SlotValue::Durable(card_ref),
                    });
                }
            }
            crate::card::data::DataInterface::Text(meta) => {
                if let Some(card_ref) = &mut meta.manifest_ref {
                    f(SlotEntry {
                        path: "spec.interface.Text.manifest_ref".to_owned(),
                        value: SlotValue::Durable(card_ref),
                    });
                }
            }
            _ => {}
        }
    }
}

impl Visit for crate::card::model::ModelSpec {
    fn visit<F>(&mut self, f: &mut F)
    where
        F: FnMut(SlotEntry<'_>),
    {
        for (i, card_ref) in self.card_refs.iter_mut().enumerate() {
            f(SlotEntry {
                path: format!("spec.card_refs[{}]", i),
                value: SlotValue::Durable(card_ref),
            });
        }
    }
}

impl Visit for crate::card::experiment::ExperimentSpec {
    fn visit<F>(&mut self, f: &mut F)
    where
        F: FnMut(SlotEntry<'_>),
    {
        for (i, card_ref) in self.target_refs.iter_mut().enumerate() {
            f(SlotEntry {
                path: format!("spec.target_refs[{}]", i),
                value: SlotValue::Durable(card_ref),
            });
        }
        for (i, card_ref) in self.card_refs.iter_mut().enumerate() {
            f(SlotEntry {
                path: format!("spec.card_refs[{}]", i),
                value: SlotValue::Durable(card_ref),
            });
        }
    }
}

impl Visit for crate::card::agent::AgentSpec {
    fn visit<F>(&mut self, f: &mut F)
    where
        F: FnMut(SlotEntry<'_>),
    {
        f(SlotEntry {
            path: "spec.prompt".to_owned(),
            value: SlotValue::InlineablePrompt(&mut self.prompt),
        });
    }
}

impl Visit for crate::card::workflow::WorkflowSpec {
    fn visit<F>(&mut self, f: &mut F)
    where
        F: FnMut(SlotEntry<'_>),
    {
        use crate::card::workflow::WorkflowAction;

        for (step_idx, step) in self.steps.iter_mut().enumerate() {
            match &mut step.action {
                WorkflowAction::Agent(agent_ref) => {
                    f(SlotEntry {
                        path: format!("spec.steps[{}].action.Agent", step_idx),
                        value: SlotValue::InlineableAgent(agent_ref),
                    });
                }
                WorkflowAction::Mcp(card_ref) => {
                    f(SlotEntry {
                        path: format!("spec.steps[{}].action.Mcp", step_idx),
                        value: SlotValue::Durable(card_ref),
                    });
                }
                WorkflowAction::Prompt(card_ref) => {
                    f(SlotEntry {
                        path: format!("spec.steps[{}].action.Prompt", step_idx),
                        value: SlotValue::Durable(card_ref),
                    });
                }
            }
        }
    }
}

impl Visit for crate::card::eval::EvalSpec {
    fn visit<F>(&mut self, f: &mut F)
    where
        F: FnMut(SlotEntry<'_>),
    {
        use crate::vala::eval::EvalTask;

        if let Some(dataset_ref) = &mut self.dataset {
            f(SlotEntry {
                path: "spec.dataset".to_owned(),
                value: SlotValue::Durable(&mut dataset_ref.0),
            });
        }

        if let Some(subject_ref) = &mut self.subject_ref {
            f(SlotEntry {
                path: "spec.subject_ref".to_owned(),
                value: SlotValue::Durable(subject_ref),
            });
        }

        for (task_id, task) in self.tasks.iter_mut() {
            if let EvalTask::LlmJudge(llm_judge_task) = task {
                f(SlotEntry {
                    path: format!("spec.tasks[{}].LlmJudge.judge_ref", task_id.as_str()),
                    value: SlotValue::InlineableAgent(&mut llm_judge_task.judge_ref),
                });
            }
        }
    }
}

impl Visit for crate::card::drift::DriftSpec {
    fn visit<F>(&mut self, f: &mut F)
    where
        F: FnMut(SlotEntry<'_>),
    {
        use crate::card::drift::DriftSignal;

        f(SlotEntry {
            path: "spec.subject_ref".to_owned(),
            value: SlotValue::Durable(&mut self.subject_ref),
        });

        match &mut self.signal {
            DriftSignal::Distribution { baseline_ref, .. } => {
                f(SlotEntry {
                    path: "spec.signal.Distribution.baseline_ref".to_owned(),
                    value: SlotValue::Durable(baseline_ref),
                });
            }
            DriftSignal::EvalScore { eval_ref } => {
                f(SlotEntry {
                    path: "spec.signal.EvalScore.eval_ref".to_owned(),
                    value: SlotValue::Durable(eval_ref),
                });
            }
            DriftSignal::External { source_ref } => {
                f(SlotEntry {
                    path: "spec.signal.External.source_ref".to_owned(),
                    value: SlotValue::Durable(source_ref),
                });
            }
            DriftSignal::Metric { .. } => {}
        }
    }
}

impl Visit for crate::card::service::ServiceSpec {
    fn visit<F>(&mut self, f: &mut F)
    where
        F: FnMut(SlotEntry<'_>),
    {
        for (i, component) in self.components.iter_mut().enumerate() {
            f(SlotEntry {
                path: format!("spec.components[{}].card_ref", i),
                value: SlotValue::Durable(&mut component.card_ref),
            });
        }
    }
}

impl Visit for crate::card::mcp::McpSpec {
    fn visit<F>(&mut self, f: &mut F)
    where
        F: FnMut(SlotEntry<'_>),
    {
        for (i, card_ref) in self.tool_refs.iter_mut().enumerate() {
            f(SlotEntry {
                path: format!("spec.tool_refs[{}]", i),
                value: SlotValue::Durable(card_ref),
            });
        }
    }
}

impl Visit for crate::card::audit::AuditSpec {
    fn visit<F>(&mut self, f: &mut F)
    where
        F: FnMut(SlotEntry<'_>),
    {
        for (i, card_ref) in self.subject_refs.iter_mut().enumerate() {
            f(SlotEntry {
                path: format!("spec.subject_refs[{}]", i),
                value: SlotValue::Durable(card_ref),
            });
        }
        for (i, card_ref) in self.policy_refs.iter_mut().enumerate() {
            f(SlotEntry {
                path: format!("spec.policy_refs[{}]", i),
                value: SlotValue::Durable(card_ref),
            });
        }
        for (i, card_ref) in self.evidence_refs.iter_mut().enumerate() {
            f(SlotEntry {
                path: format!("spec.evidence_refs[{}]", i),
                value: SlotValue::Durable(card_ref),
            });
        }
    }
}

impl Visit for crate::card::artifact::ArtifactSpec {
    fn visit<F>(&mut self, f: &mut F)
    where
        F: FnMut(SlotEntry<'_>),
    {
        if let Some(schema_ref) = &mut self.schema_ref {
            f(SlotEntry {
                path: "spec.schema_ref".to_owned(),
                value: SlotValue::Durable(schema_ref),
            });
        }
    }
}

impl Visit for crate::card::trigger::TriggerSpec {
    fn visit<F>(&mut self, f: &mut F)
    where
        F: FnMut(SlotEntry<'_>),
    {
        use crate::card::trigger::TriggerSource;

        f(SlotEntry {
            path: "spec.target".to_owned(),
            value: SlotValue::Durable(&mut self.target),
        });

        match &mut self.source {
            TriggerSource::DriftObservation { card } => {
                f(SlotEntry {
                    path: "spec.source.DriftObservation.card".to_owned(),
                    value: SlotValue::Durable(card),
                });
            }
            TriggerSource::EvalObservation { card } => {
                f(SlotEntry {
                    path: "spec.source.EvalObservation.card".to_owned(),
                    value: SlotValue::Durable(card),
                });
            }
            TriggerSource::Schedule { .. } => {}
        }
    }
}

impl Visit for crate::card::operator::OperatorSpec {
    fn visit<F>(&mut self, f: &mut F)
    where
        F: FnMut(SlotEntry<'_>),
    {
        // pre_invoke and post_invoke are Policy refs
        for (i, card_ref) in self.pre_invoke.iter_mut().enumerate() {
            f(SlotEntry {
                path: format!("spec.pre_invoke[{}]", i),
                value: SlotValue::Durable(card_ref),
            });
        }

        for (i, card_ref) in self.post_invoke.iter_mut().enumerate() {
            f(SlotEntry {
                path: format!("spec.post_invoke[{}]", i),
                value: SlotValue::Durable(card_ref),
            });
        }
    }
}
