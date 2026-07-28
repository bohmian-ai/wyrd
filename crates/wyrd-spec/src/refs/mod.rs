//! Reference slot visitor for traversing and mutating Card references within specs.
//!
//! This module provides the canonical visitor pattern for iterating over all reference
//! slots in a Card's spec. The visitor yields mutable handles to each reference position,
//! enabling reference resolution, validation, and transformation during the loader pipeline.

use crate::card::agent::AgentSpec;
use crate::card::artifact::ArtifactSpec;
use crate::card::audit::AuditSpec;
use crate::card::data::{DataInterface, DataSpec, SplitStrategy};
use crate::card::drift::{DriftSignal, DriftSpec};
use crate::card::eval::EvalSpec;
use crate::card::experiment::ExperimentSpec;
use crate::card::mcp::McpSpec;
use crate::card::model::ModelSpec;
use crate::card::operator::{OperatorAction, OperatorSpec};
use crate::card::service::ServiceSpec;
use crate::card::trigger::{TriggerSource, TriggerSpec};
use crate::card::workflow::{WorkflowAction, WorkflowSpec};
use crate::envelope::Spec;
use crate::reference::{InlineableRef, Ref};
use crate::vala::eval::EvalTask;
use skald_spec::Prompt;

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
    InlineablePrompt(&'a mut InlineableRef<Prompt>),
    /// An inlineable Agent spec slot.
    InlineableAgent(&'a mut InlineableRef<AgentSpec>),
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

impl Visit for DataSpec {
    /// Visit intrinsic Data lineage and materialization references.
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
            if let SplitStrategy::Materialized(card_ref) = &mut split.strategy {
                f(SlotEntry {
                    path: format!("spec.splits[{}].strategy.Materialized", label),
                    value: SlotValue::Durable(card_ref),
                });
            }
        }
        match &mut self.interface {
            DataInterface::Image(meta) => {
                if let Some(card_ref) = &mut meta.manifest_ref {
                    f(SlotEntry {
                        path: "spec.interface.Image.manifest_ref".to_owned(),
                        value: SlotValue::Durable(card_ref),
                    });
                }
            }
            DataInterface::Text(meta) => {
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

impl Visit for ModelSpec {
    /// Visit intrinsic Model artifact and lineage references.
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

impl Visit for ExperimentSpec {
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

impl Visit for AgentSpec {
    fn visit<F>(&mut self, f: &mut F)
    where
        F: FnMut(SlotEntry<'_>),
    {
        visit_agent(self, "spec", f);
    }
}

impl Visit for WorkflowSpec {
    fn visit<F>(&mut self, f: &mut F)
    where
        F: FnMut(SlotEntry<'_>),
    {
        if let Some(governance) = &mut self.governance {
            for (index, policy_ref) in governance.policy_refs.iter_mut().enumerate() {
                f(SlotEntry {
                    path: format!("spec.governance.policy_refs[{}]", index),
                    value: SlotValue::Durable(policy_ref),
                });
            }
            if let Some(audit_ref) = &mut governance.audit_ref {
                f(SlotEntry {
                    path: "spec.governance.audit_ref".to_owned(),
                    value: SlotValue::Durable(audit_ref),
                });
            }
        }
        if let Some(observation_hooks) = &mut self.observation_hooks {
            for (index, route_ref) in observation_hooks.route_refs.iter_mut().enumerate() {
                f(SlotEntry {
                    path: format!("spec.observation_hooks.route_refs[{}]", index),
                    value: SlotValue::Durable(route_ref),
                });
            }
        }

        for (step_idx, step) in self.steps.iter_mut().enumerate() {
            match &mut step.action {
                WorkflowAction::Agent(agent_ref) => {
                    let path = format!("spec.steps[{}].action.Agent", step_idx);
                    f(SlotEntry {
                        path: path.clone(),
                        value: SlotValue::InlineableAgent(agent_ref),
                    });
                    if let InlineableRef::Inline(agent) = agent_ref {
                        visit_agent(agent, &path, f);
                    }
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

impl Visit for EvalSpec {
    fn visit<F>(&mut self, f: &mut F)
    where
        F: FnMut(SlotEntry<'_>),
    {
        if let Some(dataset_ref) = &mut self.dataset {
            f(SlotEntry {
                path: "spec.dataset".to_owned(),
                value: SlotValue::Durable(&mut dataset_ref.0),
            });
        }

        for (task_id, task) in self.tasks.iter_mut() {
            if let EvalTask::LlmJudge(llm_judge_task) = task {
                let path = format!("spec.tasks[{}].LlmJudge.judge_ref", task_id.as_str());
                f(SlotEntry {
                    path: path.clone(),
                    value: SlotValue::InlineableAgent(&mut llm_judge_task.judge_ref),
                });
                if let InlineableRef::Inline(agent) = &mut llm_judge_task.judge_ref {
                    visit_agent(agent, &path, f);
                }
            }
        }
    }
}

impl Visit for DriftSpec {
    fn visit<F>(&mut self, f: &mut F)
    where
        F: FnMut(SlotEntry<'_>),
    {
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

impl Visit for ServiceSpec {
    /// Visit component identities, component publication bindings, and Service publications.
    fn visit<F>(&mut self, f: &mut F)
    where
        F: FnMut(SlotEntry<'_>),
    {
        for (i, component) in self.components.iter_mut().enumerate() {
            f(SlotEntry {
                path: format!("spec.components[{i}].ref"),
                value: SlotValue::Durable(&mut component.card_ref),
            });
            visit_publishes_to(
                &mut component.publishes_to,
                &format!("spec.components[{i}].publishes_to"),
                f,
            );
        }
        visit_publishes_to(&mut self.publishes_to, "spec.publishes_to", f);
    }
}

impl Visit for McpSpec {
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

impl Visit for AuditSpec {
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

impl Visit for ArtifactSpec {
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

impl Visit for TriggerSpec {
    fn visit<F>(&mut self, f: &mut F)
    where
        F: FnMut(SlotEntry<'_>),
    {
        f(SlotEntry {
            path: "spec.operator_ref".to_owned(),
            value: SlotValue::Durable(&mut self.operator_ref),
        });

        match &mut self.source {
            Some(TriggerSource::DriftObservation {
                drift_ref,
                subject_filter,
            }) => {
                f(SlotEntry {
                    path: "spec.source.drift_observation.drift_ref".to_owned(),
                    value: SlotValue::Durable(drift_ref),
                });
                visit_subject_filter(subject_filter, f);
            }
            Some(TriggerSource::EvalObservation {
                eval_ref,
                subject_filter,
            }) => {
                f(SlotEntry {
                    path: "spec.source.eval_observation.eval_ref".to_owned(),
                    value: SlotValue::Durable(eval_ref),
                });
                visit_subject_filter(subject_filter, f);
            }
            None => {}
        }
    }
}

impl Visit for OperatorSpec {
    fn visit<F>(&mut self, f: &mut F)
    where
        F: FnMut(SlotEntry<'_>),
    {
        if let OperatorAction::Workflow { workflow_ref } = &mut self.action {
            f(SlotEntry {
                path: "spec.action.workflow_ref".to_owned(),
                value: SlotValue::Durable(workflow_ref),
            });
        }
    }
}

/// Visit one publication list using the exact owning field path.
fn visit_publishes_to<F>(publishes_to: &mut [Ref], path: &str, f: &mut F)
where
    F: FnMut(SlotEntry<'_>),
{
    for (index, card_ref) in publishes_to.iter_mut().enumerate() {
        f(SlotEntry {
            path: format!("{path}[{index}]"),
            value: SlotValue::Durable(card_ref),
        });
    }
}

fn visit_agent<F>(agent: &mut AgentSpec, prefix: &str, f: &mut F)
where
    F: FnMut(SlotEntry<'_>),
{
    f(SlotEntry {
        path: format!("{prefix}.prompt"),
        value: SlotValue::InlineablePrompt(&mut agent.prompt),
    });
    for (index, card_ref) in agent.publishes_to.iter_mut().enumerate() {
        f(SlotEntry {
            path: format!("{prefix}.publishes_to[{index}]"),
            value: SlotValue::Durable(card_ref),
        });
    }
}

fn visit_subject_filter<F>(subject_filter: &mut Option<Ref>, f: &mut F)
where
    F: FnMut(SlotEntry<'_>),
{
    if let Some(subject_filter) = subject_filter {
        f(SlotEntry {
            path: "spec.source.subject_filter".to_owned(),
            value: SlotValue::Durable(subject_filter),
        });
    }
}

#[cfg(test)]
mod completeness_tests {
    use std::collections::{BTreeMap, HashMap};

    use serde_json::json;
    use skald_spec::Prompt;

    use super::{ReferenceSlotVisitor, SlotValue};
    use crate::card::agent::{AgentRunConfigSpec, AgentSpec};
    use crate::card::artifact::ArtifactSpec;
    use crate::card::audit::AuditSpec;
    use crate::card::common::{Governance, ObservationHooks};
    use crate::card::data::{
        ColorMode, DataInterface, DataSchema, DataSpec, DataSplit, DataStats, ImageFormat,
        ImageMeta, PandasMeta, ParquetCompression, SplitStrategy, TextMeta,
    };
    use crate::card::drift::{DriftCondition, DriftMethod, DriftSignal, DriftSpec};
    use crate::card::experiment::ExperimentSpec;
    use crate::card::field::FieldSpec;
    use crate::card::mcp::McpSpec;
    use crate::card::model::{ModelInterface, ModelSignature, ModelSpec, SklearnMeta, TaskType};
    use crate::card::operator::{OperatorAction, OperatorSpec};
    use crate::card::service::{ServiceComponent, ServiceSpec};
    use crate::card::trigger::{TriggerSchedule, TriggerSource, TriggerSpec};
    use crate::card::workflow::{WorkflowAction, WorkflowSpec, WorkflowStep};
    use crate::envelope::{CardKind, Spec};
    use crate::ids::{CardName, ColumnName, SpaceName};
    use crate::reference::{CardRef, InlineableRef, Ref};
    use crate::vala::eval::ids::TaskId;
    use crate::vala::eval::{ComparisonOperator, DatasetRef, EvalSpec, EvalTask, LlmJudgeTask};
    use wyrd_semver::VersionBlock;

    fn card_ref(kind: CardKind, name: &str) -> CardRef {
        CardRef {
            kind,
            name: CardName::new(name).expect("fixture card name is valid"),
            version: VersionBlock::parse("1.0.0").expect("fixture version is valid"),
            space: Some(SpaceName::new("default").expect("fixture space is valid")),
            uid: None,
        }
    }

    fn prompt() -> Prompt {
        Prompt::new(
            skald_spec::ProviderRequest::OpenAiChatCompletion(skald_spec::OpenAiChatRequest {
                model: "gpt-test".to_owned(),
                messages: vec![skald_spec::OpenAiChatMessage {
                    role: "user".to_owned(),
                    content: Some(skald_spec::wire::openai_chat::OpenAiMessageContent::Text(
                        "judge ${context}".to_owned(),
                    )),
                    ..Default::default()
                }],
                response_format: None,
                stream: None,
                stream_options: None,
                tools: None,
                tool_choice: None,
                parallel_tool_calls: None,
                settings: skald_spec::OpenAiChatSettings::default(),
            }),
            "gpt-test",
            None,
            skald_spec::ResponseType::JsonSchema {
                name: "judge_result".to_owned(),
                schema: json!({"type": "object"}),
            },
        )
        .expect("fixture prompt is valid")
    }

    fn agent() -> AgentSpec {
        AgentSpec {
            prompt: InlineableRef::Inline(Box::new(prompt())),
            tool_names: Vec::new(),
            run_config: AgentRunConfigSpec {
                max_iterations: Some(1),
                ..AgentRunConfigSpec::default()
            },
            publishes_to: Vec::new(),
        }
    }

    fn data(interface: DataInterface) -> DataSpec {
        DataSpec {
            interface,
            schema: DataSchema::new(vec![FieldSpec::new(
                ColumnName::new("value").expect("fixture column is valid"),
                "int64",
            )]),
            card_refs: vec![Ref::Ref(card_ref(CardKind::Artifact, "data-artifact"))],
            splits: HashMap::from([(
                crate::ids::SplitName::new("train").expect("fixture split is valid"),
                DataSplit {
                    label: crate::ids::SplitName::new("train").expect("fixture split is valid"),
                    strategy: SplitStrategy::Materialized(Ref::Ref(card_ref(
                        CardKind::Artifact,
                        "split-artifact",
                    ))),
                },
            )]),
            target_columns: Vec::new(),
            sql: None,
            stats: DataStats {
                row_count: Some(1),
                col_count: Some(1),
                byte_count: 1,
                sha256: "a".repeat(64),
            },
        }
    }

    fn model() -> ModelSpec {
        ModelSpec {
            interface: ModelInterface::Sklearn(SklearnMeta {
                framework_version: "1.0".to_owned(),
                model_subtype: None,
            }),
            task_type: TaskType::Regression,
            signature: ModelSignature::new(
                vec![FieldSpec::new(
                    ColumnName::new("input").expect("fixture column is valid"),
                    "float32",
                )],
                vec![FieldSpec::new(
                    ColumnName::new("output").expect("fixture column is valid"),
                    "float32",
                )],
            ),
            sample_input: None,
            card_refs: vec![Ref::Ref(card_ref(CardKind::Artifact, "model-artifact"))],
        }
    }

    fn workflow() -> WorkflowSpec {
        WorkflowSpec {
            governance: Some(Governance {
                policy_refs: vec![Ref::Ref(card_ref(CardKind::Policy, "policy"))],
                audit_ref: Some(Ref::Ref(card_ref(CardKind::Audit, "audit"))),
                ..Governance::default()
            }),
            observation_hooks: Some(ObservationHooks {
                route_refs: vec![Ref::Ref(card_ref(CardKind::Service, "route"))],
                ..ObservationHooks::default()
            }),
            steps: vec![
                WorkflowStep {
                    id: "agent".to_owned(),
                    action: WorkflowAction::Agent(InlineableRef::Inline(Box::new(agent()))),
                    depends_on: Vec::new(),
                    inputs: BTreeMap::new(),
                    condition: None,
                    timeout_seconds: None,
                    retry: None,
                    display: BTreeMap::new(),
                },
                WorkflowStep {
                    id: "mcp".to_owned(),
                    action: WorkflowAction::Mcp(Ref::Ref(card_ref(CardKind::Mcp, "mcp"))),
                    depends_on: Vec::new(),
                    inputs: BTreeMap::new(),
                    condition: None,
                    timeout_seconds: None,
                    retry: None,
                    display: BTreeMap::new(),
                },
                WorkflowStep {
                    id: "prompt".to_owned(),
                    action: WorkflowAction::Prompt(Ref::Ref(card_ref(CardKind::Prompt, "prompt"))),
                    depends_on: Vec::new(),
                    inputs: BTreeMap::new(),
                    condition: None,
                    timeout_seconds: None,
                    retry: None,
                    display: BTreeMap::new(),
                },
            ],
            ..WorkflowSpec::default()
        }
    }

    fn eval() -> EvalSpec {
        let task_id = TaskId::new("judge").expect("fixture task id is valid");
        let judge = LlmJudgeTask::new(
            task_id.clone(),
            agent(),
            ComparisonOperator::Equals,
            json!(true),
        )
        .expect("fixture judge is valid");
        let mut eval = EvalSpec::new(BTreeMap::from([(task_id, EvalTask::LlmJudge(judge))]))
            .expect("fixture eval is valid");
        eval.dataset = Some(DatasetRef(Ref::Ref(card_ref(CardKind::Data, "dataset"))));
        eval
    }

    fn visit_paths(mut spec: Spec) -> Vec<(String, &'static str)> {
        let mut paths = Vec::new();
        ReferenceSlotVisitor::visit(&mut spec, |entry| {
            let kind = match entry.value {
                SlotValue::Durable(_) => "durable",
                SlotValue::InlineablePrompt(_) => "inlineable_prompt",
                SlotValue::InlineableAgent(_) => "inlineable_agent",
            };
            paths.push((entry.path, kind));
        });
        paths
    }

    /// Confirm the canonical visitor exposes every supported reference slot exactly once.
    #[test]
    fn every_spec_ref_field_is_ref_or_inlineable_ref() {
        let image = data(DataInterface::Image(ImageMeta {
            format: ImageFormat::Png,
            manifest_ref: Some(Ref::Ref(card_ref(CardKind::Artifact, "manifest"))),
            color_mode: ColorMode::Rgb,
        }));
        let text = data(DataInterface::Text(TextMeta {
            encoding: "utf-8".to_owned(),
            manifest_ref: Some(Ref::Ref(card_ref(CardKind::Artifact, "text-manifest"))),
        }));
        let mut experiment = ExperimentSpec {
            target_refs: vec![Ref::Ref(card_ref(CardKind::Model, "target"))],
            card_refs: vec![Ref::Ref(card_ref(CardKind::Artifact, "artifact"))],
            ..ExperimentSpec::default()
        };
        let mut artifact = ArtifactSpec {
            artifact_kind: "file".to_owned(),
            schema_ref: Some(Ref::Ref(card_ref(CardKind::Data, "schema"))),
            ..ArtifactSpec::default()
        };
        let mut audit = AuditSpec {
            subject_refs: vec![Ref::Ref(card_ref(CardKind::Model, "subject"))],
            policy_refs: vec![Ref::Ref(card_ref(CardKind::Policy, "policy"))],
            evidence_refs: vec![Ref::Ref(card_ref(CardKind::Artifact, "evidence"))],
            ..AuditSpec::default()
        };
        let mut mcp = McpSpec {
            server_name: "fixture".to_owned(),
            tool_refs: vec![Ref::Ref(card_ref(CardKind::Service, "tool"))],
            ..McpSpec::default()
        };
        let mut service = ServiceSpec {
            components: vec![ServiceComponent {
                alias: "model".to_owned(),
                card_ref: Ref::Ref(card_ref(CardKind::Model, "component")),
                publishes_to: vec![Ref::Ref(card_ref(CardKind::Eval, "component-quality"))],
                source: None,
                config: BTreeMap::new(),
                credential_refs: Vec::new(),
            }],
            ..ServiceSpec::default()
        };
        let mut trigger = TriggerSpec {
            description: None,
            schedule: TriggerSchedule {
                cron: "0 * * * *".to_owned(),
                tz: None,
            },
            source: Some(TriggerSource::DriftObservation {
                drift_ref: Ref::Ref(card_ref(CardKind::Drift, "drift")),
                subject_filter: Some(Ref::Ref(card_ref(CardKind::Model, "subject"))),
            }),
            operator_ref: Ref::Ref(card_ref(CardKind::Operator, "operator")),
        };
        let mut operator = OperatorSpec {
            description: None,
            action: OperatorAction::Workflow {
                workflow_ref: Ref::Ref(card_ref(CardKind::Workflow, "workflow")),
            },
            budget: None,
        };
        let mut drift = DriftSpec {
            description: None,
            method: DriftMethod::External,
            signal: DriftSignal::External {
                source_ref: Ref::Ref(card_ref(CardKind::Source, "source")),
            },
            condition: DriftCondition::Statistical,
            profile: None,
            details: BTreeMap::new(),
        };
        let mut workflow = workflow();
        let mut eval = eval();
        let mut image = image;
        let mut text = text;
        let mut model = model();
        let mut agent = agent();

        let mut count = 0;
        for spec in [
            Spec::Data(image.clone()),
            Spec::Data(text.clone()),
            Spec::Model(model.clone()),
            Spec::Experiment(experiment.clone()),
            Spec::Agent(agent.clone()),
            Spec::Workflow(workflow.clone()),
            Spec::Eval(eval.clone()),
            Spec::Drift(drift.clone()),
            Spec::Service(service.clone()),
            Spec::Mcp(mcp.clone()),
            Spec::Audit(audit.clone()),
            Spec::Artifact(artifact.clone()),
            Spec::Trigger(trigger.clone()),
            Spec::Operator(operator.clone()),
        ] {
            count += visit_paths(spec).len();
        }
        assert_eq!(count, 32);
        assert!(
            visit_paths(Spec::Service(service.clone()))
                .contains(&("spec.components[0].publishes_to[0]".to_owned(), "durable"))
        );
        assert!(
            visit_paths(Spec::Service(service.clone()))
                .contains(&("spec.components[0].ref".to_owned(), "durable"))
        );

        let _ = (
            &mut image,
            &mut text,
            &mut model,
            &mut experiment,
            &mut agent,
        );
        let _ = (&mut workflow, &mut eval, &mut drift, &mut service, &mut mcp);
        let _ = (&mut audit, &mut artifact, &mut trigger, &mut operator);
    }

    #[test]
    fn visitor_covers_every_slot_in_the_migration_table() {
        let expected = vec![
            ("spec.card_refs[0]".to_owned(), "durable"),
            (
                "spec.splits[train].strategy.Materialized".to_owned(),
                "durable",
            ),
        ];
        let actual = visit_paths(Spec::Data(data(DataInterface::Pandas(PandasMeta {
            framework_version: "2".to_owned(),
            compression: ParquetCompression::Snappy,
        }))));
        assert_eq!(actual, expected);

        assert_eq!(
            visit_paths(Spec::Workflow(workflow())),
            vec![
                ("spec.governance.policy_refs[0]".to_owned(), "durable"),
                ("spec.governance.audit_ref".to_owned(), "durable"),
                ("spec.observation_hooks.route_refs[0]".to_owned(), "durable"),
                ("spec.steps[0].action.Agent".to_owned(), "inlineable_agent"),
                (
                    "spec.steps[0].action.Agent.prompt".to_owned(),
                    "inlineable_prompt"
                ),
                ("spec.steps[1].action.Mcp".to_owned(), "durable"),
                ("spec.steps[2].action.Prompt".to_owned(), "durable"),
            ]
        );
        assert_eq!(
            visit_paths(Spec::Eval(eval())),
            vec![
                ("spec.dataset".to_owned(), "durable"),
                (
                    "spec.tasks[judge].LlmJudge.judge_ref".to_owned(),
                    "inlineable_agent"
                ),
                (
                    "spec.tasks[judge].LlmJudge.judge_ref.prompt".to_owned(),
                    "inlineable_prompt"
                ),
            ]
        );
    }
}
