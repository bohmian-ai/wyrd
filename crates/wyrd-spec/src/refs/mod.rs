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
use crate::card::experiment::ExperimentSpec;
use crate::card::mcp::McpSpec;
use crate::card::model::ModelSpec;
use crate::card::operator::OperatorSpec;
use crate::card::service::ServiceSpec;
use crate::card::trigger::TriggerSpec;
use crate::card::verifier::{
    Action, AgentCall, Check, CodeCall, CodeSource, Requirement, VerificationBinding, VerifierSpec,
};
use crate::envelope::Spec;
use crate::reference::{CardRef, InlineableRef, Ref};
use skald_spec::Prompt;

/// A yielded reference slot with metadata about its kind and a mutable handle.
pub struct SlotEntry<'a> {
    /// The path to this slot in dot notation (e.g. `spec.verified_by[0].verifier`).
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
    /// An inlineable Trigger spec slot (`verified_by[..].runs_on`).
    InlineableTrigger(&'a mut InlineableRef<TriggerSpec>),
    /// An inlineable Operator spec slot (`verified_by[..].on_failure[..]` or
    /// `verified_by[..].on_verdict_change[..]`).
    InlineableOperator(&'a mut InlineableRef<OperatorSpec>),
}

impl SlotValue<'_> {
    /// Return the resolved [`CardRef`] this slot carries.
    ///
    /// Returns `None` for an unresolved `Path` and for an inline body, which
    /// has no separate Card identity.
    #[must_use]
    pub fn as_card_ref(&self) -> Option<&CardRef> {
        match self {
            Self::Durable(reference) => reference.as_card_ref(),
            Self::InlineablePrompt(reference) => reference.as_card_ref(),
            Self::InlineableAgent(reference) => reference.as_card_ref(),
            Self::InlineableTrigger(reference) => reference.as_card_ref(),
            Self::InlineableOperator(reference) => reference.as_card_ref(),
        }
    }

    /// Return the exact identity when this slot holds a loader-projected sibling.
    #[must_use]
    pub fn as_sibling(&self) -> Option<&CardRef> {
        match self {
            Self::Durable(reference) => reference.as_sibling(),
            Self::InlineablePrompt(reference) => reference.as_sibling(),
            Self::InlineableAgent(reference) => reference.as_sibling(),
            Self::InlineableTrigger(reference) => reference.as_sibling(),
            Self::InlineableOperator(reference) => reference.as_sibling(),
        }
    }

    /// Mutable variant of [`SlotValue::as_card_ref`].
    #[must_use]
    pub fn as_card_ref_mut(&mut self) -> Option<&mut CardRef> {
        match self {
            Self::Durable(reference) => reference.as_card_ref_mut(),
            Self::InlineablePrompt(reference) => reference.as_card_ref_mut(),
            Self::InlineableAgent(reference) => reference.as_card_ref_mut(),
            Self::InlineableTrigger(reference) => reference.as_card_ref_mut(),
            Self::InlineableOperator(reference) => reference.as_card_ref_mut(),
        }
    }

    /// Rewrite this slot from an authored path to a loader-projected sibling.
    ///
    /// Each slot shape keeps its own sibling form, so the loader does not need
    /// to know which concrete reference type it is holding. Slots that are not
    /// currently a `Path` are left untouched.
    pub fn resolve_path_to_sibling(&mut self, sibling: CardRef) {
        match self {
            Self::Durable(reference) => {
                if matches!(reference, Ref::Path(_)) {
                    **reference = Ref::Sibling { sibling };
                }
            }
            Self::InlineablePrompt(reference) => set_inlineable_sibling(reference, sibling),
            Self::InlineableAgent(reference) => set_inlineable_sibling(reference, sibling),
            Self::InlineableTrigger(reference) => set_inlineable_sibling(reference, sibling),
            Self::InlineableOperator(reference) => set_inlineable_sibling(reference, sibling),
        }
    }

    /// Return the authored local path when this slot is still unresolved.
    #[must_use]
    pub fn as_path(&self) -> Option<&std::path::Path> {
        match self {
            Self::Durable(Ref::Path(path))
            | Self::InlineablePrompt(InlineableRef::Path(path))
            | Self::InlineableAgent(InlineableRef::Path(path))
            | Self::InlineableTrigger(InlineableRef::Path(path))
            | Self::InlineableOperator(InlineableRef::Path(path)) => Some(path),
            _ => None,
        }
    }
}

/// Replace one inlineable slot's authored path with its sibling projection.
fn set_inlineable_sibling<T>(reference: &mut InlineableRef<T>, sibling: CardRef) {
    if matches!(reference, InlineableRef::Path(_)) {
        *reference = InlineableRef::Sibling { sibling };
    }
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
            Spec::Prompt(_)
            | Spec::Policy(_)
            | Spec::Trigger(_)
            | Spec::Source(_)
            | Spec::Operator(_) => {}
            Spec::Agent(agent) => agent.visit(&mut f),
            Spec::Verifier(verifier) => verifier.visit(&mut f),
            Spec::Service(service) => service.visit(&mut f),
            Spec::Mcp(mcp) => mcp.visit(&mut f),
            Spec::Audit(audit) => audit.visit(&mut f),
            Spec::Artifact(artifact) => artifact.visit(&mut f),
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
                path: format!("spec.card_refs[{i}]"),
                value: SlotValue::Durable(card_ref),
            });
        }
        for (label, split) in &mut self.splits {
            if let SplitStrategy::Materialized(card_ref) = &mut split.strategy {
                f(SlotEntry {
                    path: format!("spec.splits[{label}].strategy.Materialized"),
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
                path: format!("spec.card_refs[{i}]"),
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
                path: format!("spec.target_refs[{i}]"),
                value: SlotValue::Durable(card_ref),
            });
        }
        for (i, card_ref) in self.card_refs.iter_mut().enumerate() {
            f(SlotEntry {
                path: format!("spec.card_refs[{i}]"),
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

impl Visit for DriftSpec {
    /// Visit the baseline Data reference carried by a `Distribution` signal.
    fn visit<F>(&mut self, f: &mut F)
    where
        F: FnMut(SlotEntry<'_>),
    {
        self.visit_at("spec.implementation.spec", f);
    }
}

impl DriftSpec {
    /// Visit this Drift payload's reference slots under an explicit path prefix.
    ///
    /// The payload is only reachable inside a Verifier implementation, so the
    /// prefix names that enclosing slot rather than a bare `spec`.
    fn visit_at<F>(&mut self, prefix: &str, f: &mut F)
    where
        F: FnMut(SlotEntry<'_>),
    {
        match &mut self.signal {
            DriftSignal::Distribution { baseline_ref, .. } => {
                f(SlotEntry {
                    path: format!("{prefix}.signal.Distribution.baseline_ref"),
                    value: SlotValue::Durable(baseline_ref),
                });
            }
            DriftSignal::Metric { .. } => {}
        }
    }
}

impl Visit for VerifierSpec {
    /// Visit every Agent, code Artifact, and drift baseline reference in the
    /// Verifier's steps and requirements.
    fn visit<F>(&mut self, f: &mut F)
    where
        F: FnMut(SlotEntry<'_>),
    {
        for (i, step) in self.steps.iter_mut().enumerate() {
            let prefix = format!("spec.steps[{i}].action");
            match &mut step.action {
                Action::Extract { .. } => {}
                Action::Agent(call) => visit_agent_call(call, &prefix, f),
                Action::Code(call) => visit_code_call(call, &prefix, f),
            }
            visit_requirements(
                &mut step.requirements,
                &format!("spec.steps[{i}].requirements"),
                f,
            );
        }
        visit_requirements(&mut self.requirements, "spec.requirements", f);
    }
}

/// Visit the reference slots of each requirement check under `prefix`.
fn visit_requirements<F>(requirements: &mut [Requirement], prefix: &str, f: &mut F)
where
    F: FnMut(SlotEntry<'_>),
{
    for (i, requirement) in requirements.iter_mut().enumerate() {
        let prefix = format!("{prefix}[{i}].check");
        match &mut requirement.check {
            Check::Agent(call) => visit_agent_call(call, &prefix, f),
            Check::Code(call) => visit_code_call(call, &prefix, f),
            Check::Drift(drift) => drift.spec.visit_at(&prefix, f),
            Check::Assert { .. } | Check::Trace(_) => {}
        }
    }
}

/// Visit one Agent call's Agent slot and, when inline, the Agent's own slots.
fn visit_agent_call<F>(call: &mut AgentCall, prefix: &str, f: &mut F)
where
    F: FnMut(SlotEntry<'_>),
{
    let path = format!("{prefix}.agent");
    f(SlotEntry {
        path: path.clone(),
        value: SlotValue::InlineableAgent(&mut call.agent),
    });
    if let InlineableRef::Inline(agent) = &mut call.agent {
        visit_agent(agent, &path, f);
    }
}

/// Visit one code call's Artifact source slot.
fn visit_code_call<F>(call: &mut CodeCall, prefix: &str, f: &mut F)
where
    F: FnMut(SlotEntry<'_>),
{
    if let CodeSource::Artifact { card, .. } = &mut call.source {
        f(SlotEntry {
            path: format!("{prefix}.source.artifact.card"),
            value: SlotValue::Durable(card),
        });
    }
}

impl Visit for ServiceSpec {
    /// Visit component identities, component verification bindings, and Service bindings.
    fn visit<F>(&mut self, f: &mut F)
    where
        F: FnMut(SlotEntry<'_>),
    {
        for (i, component) in self.components.iter_mut().enumerate() {
            f(SlotEntry {
                path: format!("spec.components[{i}].ref"),
                value: SlotValue::Durable(&mut component.card_ref),
            });
            visit_verified_by(
                &mut component.verified_by,
                &format!("spec.components[{i}].verified_by"),
                f,
            );
        }
        visit_verified_by(&mut self.verified_by, "spec.verified_by", f);
    }
}

impl Visit for McpSpec {
    fn visit<F>(&mut self, f: &mut F)
    where
        F: FnMut(SlotEntry<'_>),
    {
        for (i, card_ref) in self.tool_refs.iter_mut().enumerate() {
            f(SlotEntry {
                path: format!("spec.tool_refs[{i}]"),
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
                path: format!("spec.subject_refs[{i}]"),
                value: SlotValue::Durable(card_ref),
            });
        }
        for (i, card_ref) in self.policy_refs.iter_mut().enumerate() {
            f(SlotEntry {
                path: format!("spec.policy_refs[{i}]"),
                value: SlotValue::Durable(card_ref),
            });
        }
        for (i, card_ref) in self.evidence_refs.iter_mut().enumerate() {
            f(SlotEntry {
                path: format!("spec.evidence_refs[{i}]"),
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

/// Visit one verification-binding list using the exact owning field path.
///
/// Each binding yields its Verifier reference, its `runs_on` Trigger slot when
/// one is authored, and
/// every `on_failure` and `on_verdict_change` Operator slot. Inline Triggers and Operators carry no
/// references of their own.
fn visit_verified_by<F>(bindings: &mut [VerificationBinding], path: &str, f: &mut F)
where
    F: FnMut(SlotEntry<'_>),
{
    for (index, binding) in bindings.iter_mut().enumerate() {
        f(SlotEntry {
            path: format!("{path}[{index}].verifier"),
            value: SlotValue::Durable(&mut binding.verifier),
        });
        if let Some(runs_on) = &mut binding.runs_on {
            f(SlotEntry {
                path: format!("{path}[{index}].runs_on"),
                value: SlotValue::InlineableTrigger(runs_on),
            });
        }
        for (list, operators) in binding.operator_lists_mut() {
            for (operator_index, operator) in operators.iter_mut().enumerate() {
                f(SlotEntry {
                    path: format!("{path}[{index}].{list}[{operator_index}]"),
                    value: SlotValue::InlineableOperator(operator),
                });
            }
        }
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
    visit_verified_by(&mut agent.verified_by, &format!("{prefix}.verified_by"), f);
}

#[cfg(test)]
mod blocks_tests {
    //! Reference visiting over staged Verifier building blocks.

    use serde_json::json;

    use super::{SlotValue, Visit};
    use crate::card::verifier::VerifierSpec;

    /// The staged Verifier yields every Agent, Artifact, Prompt, and
    /// baseline slot at its authored path.
    ///
    /// # Panics
    /// Panics when a slot is missing or yielded at the wrong path.
    #[test]
    fn blocks_verifier_yields_every_reference_slot() {
        let agent =
            json!({"kind": "Agent", "name": "judge", "version": "1.0.0"});
        let inline = json!({
            "prompt": {"kind": "Prompt", "name": "judge-prompt", "version": "1.0.0"},
            "execution_route": {"kind": "wyrd_gateway", "deployment": "openai-primary"}
        });
        let mut spec: VerifierSpec = serde_json::from_value(json!({
            "steps": [{"id": "a", "action": {"kind": "agent", "agent": inline},
                "requirements": [{"id": "b", "statement": "s", "check": {"kind": "code", "entrypoint": "f",
                    "source": {"artifact": {"card": {"kind": "Artifact", "name": "checks", "version": "1.0.0"}}}}}]}],
            "requirements": [
                {"id": "c", "statement": "s", "check": {"kind": "agent", "agent": agent}},
                {"id": "d", "statement": "s", "check": {"kind": "drift", "on": "inputs.x", "method": "Spc",
                    "signal": {"kind": "Distribution", "baseline_ref": {"kind": "Data", "name": "train", "version": "1.0.0"}, "features": ["x"]},
                    "condition": {"kind": "Statistical"}}}
            ]
        }))
        .expect("staged Verifier");
        let mut paths = Vec::new();
        spec.visit(&mut |entry| {
            let kind = match entry.value {
                SlotValue::Durable(_) => "durable",
                SlotValue::InlineablePrompt(_) => "prompt",
                SlotValue::InlineableAgent(_) => "agent",
                SlotValue::InlineableTrigger(_) | SlotValue::InlineableOperator(_) => "other",
            };
            paths.push(format!("{kind} {}", entry.path));
        });
        assert_eq!(
            paths,
            [
                "agent spec.steps[0].action.agent",
                "prompt spec.steps[0].action.agent.prompt",
                "durable spec.steps[0].requirements[0].check.source.artifact.card",
                "agent spec.requirements[0].check.agent",
                "durable spec.requirements[1].check.signal.Distribution.baseline_ref",
            ]
        );
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
    use crate::card::data::{
        ColorMode, DataInterface, DataSchema, DataSpec, DataSplit, DataStats, ImageFormat,
        ImageMeta, PandasMeta, ParquetCompression, SplitStrategy, TextMeta,
    };
    use crate::card::drift::{
        DriftCondition, DriftMethod, DriftProfile, DriftSignal, DriftSpec, PsiBinningStrategy,
        PsiProfile, PsiThreshold,
    };
    use crate::card::experiment::ExperimentSpec;
    use crate::card::field::FieldSpec;
    use crate::card::mcp::McpSpec;
    use crate::card::model::{ModelInterface, ModelSignature, ModelSpec, SklearnMeta, TaskType};
    use crate::card::operator::OperatorSpec;
    use crate::card::service::{ServiceComponent, ServiceSpec};
    use crate::card::trigger::{TriggerActivation, TriggerSpec};
    use crate::card::verifier::{
        AgentCall, Check, Decide, DriftCheck, Reference, Requirement, VerificationBinding,
        VerifierSpec,
    };
    use crate::envelope::{CardKind, Spec};
    use crate::ids::{CardName, ColumnName};
    use crate::reference::{CardRef, InlineableRef, Ref};
    use crate::verification::TaskId;
    use wyrd_semver::VersionBlock;

    fn card_ref(kind: CardKind, name: &str) -> CardRef {
        CardRef {
            kind,
            name: CardName::new(name).expect("fixture card name is valid"),
            version: VersionBlock::parse("1.0.0").expect("fixture version is valid"),
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
        )
        .expect("fixture prompt is valid")
    }

    /// A native-route Agent spec with an inline Prompt and a one-iteration run
    /// config, used as a nested reference slot.
    ///
    /// # Panics
    /// Panics when the fixture Prompt is invalid.
    fn agent() -> AgentSpec {
        AgentSpec {
            prompt: InlineableRef::Inline(Box::new(prompt())),
            execution_route: crate::card::agent::ExecutionRoute::Native {},
            tool_names: Vec::new(),
            run_config: AgentRunConfigSpec {
                max_iterations: Some(1),
                ..AgentRunConfigSpec::default()
            },
            verified_by: Vec::new(),
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

    /// A Verifier whose one requirement is an Agent check of [`agent`].
    ///
    /// # Panics
    /// Panics when the fixed requirement id stops validating.
    fn agent_verifier() -> VerifierSpec {
        VerifierSpec {
            description: None,
            inputs: BTreeMap::new(),
            steps: Vec::new(),
            requirements: vec![Requirement {
                id: TaskId::new("judge").expect("fixture requirement id is valid"),
                statement: "The answer is grounded.".to_owned(),
                check: Check::Agent(AgentCall {
                    agent: InlineableRef::Inline(Box::new(agent())),
                    inputs: BTreeMap::new(),
                    workspace: BTreeMap::new(),
                }),
            }],
            decide: Decide::All,
            context_capture: None,
            order_by: None,
        }
    }

    /// A Verifier whose one requirement is a drift check of [`drift`].
    ///
    /// # Panics
    /// Panics when the fixed requirement id or reference stops validating.
    fn drift_verifier() -> VerifierSpec {
        VerifierSpec {
            requirements: vec![Requirement {
                id: TaskId::new("stable").expect("fixture requirement id is valid"),
                statement: "Inputs are stable.".to_owned(),
                check: Check::Drift(DriftCheck {
                    on: Reference::parse("inputs.current").expect("fixture reference is valid"),
                    spec: drift(),
                }),
            }],
            ..agent_verifier()
        }
    }

    /// Build a PSI Drift payload whose baseline is the only reference slot.
    fn drift() -> DriftSpec {
        DriftSpec {
            description: None,
            method: DriftMethod::Psi,
            signal: DriftSignal::Distribution {
                baseline_ref: Ref::Ref(card_ref(CardKind::Data, "baseline")),
                features: vec!["value".parse().expect("fixture feature is valid")],
            },
            condition: DriftCondition::Statistical,
            profile: Some(DriftProfile::Psi(PsiProfile {
                binning_strategy: PsiBinningStrategy::Quantile { n_bins: 10 },
                categorical_features: Vec::new(),
                threshold: PsiThreshold::Fixed { value: 0.25 },
            })),
        }
    }

    /// An inline notify Operator, which carries no reference slots.
    ///
    /// # Panics
    /// Panics when the fixed notify body stops decoding as an Operator.
    fn notify_operator() -> OperatorSpec {
        serde_json::from_value(json!({
            "kind": "notify",
            "channel": {
                "kind": "pager_duty",
                "connection": "pagerduty",
                "route": "team-a",
                "severity": "critical",
                "summary": "verification failed",
            },
        }))
        .expect("fixture notify Operator decodes")
    }

    /// Build one binding with a referenced Trigger, referenced and inline
    /// failure Operators, and a verdict-change Operator so every
    /// `verified_by` slot shape and Operator list is visited.
    fn binding(verifier: &str) -> VerificationBinding {
        VerificationBinding {
            verifier: Ref::Ref(card_ref(CardKind::Verifier, verifier)),
            runs_on: Some(InlineableRef::Ref(card_ref(CardKind::Trigger, "hourly"))),
            on_failure: vec![
                InlineableRef::Ref(card_ref(CardKind::Operator, "page")),
                InlineableRef::Inline(Box::new(notify_operator())),
            ],
            on_verdict_change: vec![InlineableRef::Ref(card_ref(CardKind::Operator, "escalate"))],
        }
    }

    /// Project a visited spec into `(path, slot shape)` pairs in visit order.
    fn visit_paths(mut spec: Spec) -> Vec<(String, &'static str)> {
        let mut paths = Vec::new();
        ReferenceSlotVisitor::visit(&mut spec, |entry| {
            let kind = match entry.value {
                SlotValue::Durable(_) => "durable",
                SlotValue::InlineablePrompt(_) => "inlineable_prompt",
                SlotValue::InlineableAgent(_) => "inlineable_agent",
                SlotValue::InlineableTrigger(_) => "inlineable_trigger",
                SlotValue::InlineableOperator(_) => "inlineable_operator",
            };
            paths.push((entry.path, kind));
        });
        paths
    }

    /// Confirm the canonical visitor exposes every supported reference slot exactly once.
    ///
    /// # Panics
    /// Panics when a fixture is invalid or the visitor reports another set of
    /// slots.
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
                verified_by: vec![binding("component-quality")],
                source: None,
                config: BTreeMap::new(),
                credential_refs: Vec::new(),
            }],
            ..ServiceSpec::default()
        };
        let mut trigger = TriggerSpec {
            description: None,
            activation: TriggerActivation::Schedule {
                cron: "0 * * * *".to_owned(),
                tz: None,
            },
        };
        let mut operator = notify_operator();
        let mut drift = drift_verifier();
        let mut check = agent_verifier();
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
            Spec::Verifier(check.clone()),
            Spec::Verifier(drift.clone()),
            Spec::Service(service.clone()),
            Spec::Mcp(mcp.clone()),
            Spec::Audit(audit.clone()),
            Spec::Artifact(artifact.clone()),
            Spec::Trigger(trigger.clone()),
            Spec::Operator(operator.clone()),
        ] {
            count += visit_paths(spec).len();
        }
        assert_eq!(count, 24);
        assert!(visit_paths(Spec::Service(service.clone())).contains(&(
            "spec.components[0].verified_by[0].verifier".to_owned(),
            "durable"
        )));
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
        let _ = (&mut check, &mut drift, &mut service, &mut mcp);
        let _ = (&mut audit, &mut artifact, &mut trigger, &mut operator);
    }

    /// Pin the exact path and slot shape of every reference the visitor yields,
    /// including Verifier implementation payloads and each `verified_by`
    /// binding's `verifier`, `runs_on`, and `on_failure` slots.
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
            visit_paths(Spec::Verifier(agent_verifier())),
            vec![
                (
                    "spec.requirements[0].check.agent".to_owned(),
                    "inlineable_agent"
                ),
                (
                    "spec.requirements[0].check.agent.prompt".to_owned(),
                    "inlineable_prompt"
                ),
            ]
        );
        assert_eq!(
            visit_paths(Spec::Verifier(drift_verifier())),
            vec![(
                "spec.requirements[0].check.signal.Distribution.baseline_ref".to_owned(),
                "durable"
            )]
        );

        let binding_slots = |prefix: &str| {
            vec![
                (format!("{prefix}[0].verifier"), "durable"),
                (format!("{prefix}[0].runs_on"), "inlineable_trigger"),
                (format!("{prefix}[0].on_failure[0]"), "inlineable_operator"),
                (format!("{prefix}[0].on_failure[1]"), "inlineable_operator"),
                (
                    format!("{prefix}[0].on_verdict_change[0]"),
                    "inlineable_operator",
                ),
            ]
        };
        let service = ServiceSpec {
            components: vec![ServiceComponent {
                alias: "model".to_owned(),
                card_ref: Ref::Ref(card_ref(CardKind::Model, "component")),
                verified_by: vec![binding("component-quality")],
                source: None,
                config: BTreeMap::new(),
                credential_refs: Vec::new(),
            }],
            verified_by: vec![binding("service-quality")],
            ..ServiceSpec::default()
        };
        let mut expected = vec![("spec.components[0].ref".to_owned(), "durable")];
        expected.extend(binding_slots("spec.components[0].verified_by"));
        expected.extend(binding_slots("spec.verified_by"));
        assert_eq!(visit_paths(Spec::Service(service)), expected);

        let agent = AgentSpec {
            verified_by: vec![binding("agent-quality")],
            ..agent()
        };
        let mut expected = vec![("spec.prompt".to_owned(), "inlineable_prompt")];
        expected.extend(binding_slots("spec.verified_by"));
        assert_eq!(visit_paths(Spec::Agent(agent)), expected);
    }
}
