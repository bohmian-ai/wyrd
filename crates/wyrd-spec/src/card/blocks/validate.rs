//! Registration validation for [`VerifierSpec`].
//!
//! Every refusal is a [`WyrdError::Validation`] whose details carry a stable
//! `rule` and the authored `path`, except drift-specific refusals, which keep
//! the existing drift error codes. Validation is pure: referenced Agents are
//! inspected only when the caller resolves them.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use serde_json::{Value, json};
use skald_spec::{Prompt, ResponseType};

use super::extract::ExtractOutput;
use super::reference::{Reference, ReferenceRoot};
use super::schema_path::admits;
use super::{
    Action, AgentCall, Check, CheckEvidence, CodeCall, CodeSource, Condition, Decide, DriftCheck,
    InputSlot, MAX_STEP_RETRIES, Requirement, ScheduledInput, Step, VerifierSpec,
};
use crate::card::agent::AgentSpec;
use crate::card::drift::DriftSignal;
use crate::card::prompt::{ParameterName, is_valid_parameter_name};
use crate::envelope::CardKind;
use crate::error::WyrdError;
use crate::ids::{FeatureName, SlotName};
use crate::reference::InlineableRef;
use crate::vala::eval::{ExecutionPlan, TaskId, plan_stages};

/// Built-in tools a step-position Agent may select.
const STEP_TOOLS: [&str; 6] = [
    "fs.list",
    "fs.read",
    "fs.search",
    "code.run",
    "cards.get",
    "bifrost.query",
];

/// Fields a check-position Agent's response schema must require.
const VERDICT_FIELDS: [&str; 3] = ["outcome", "reason", "citations"];

/// An Agent and its Prompt, resolved by the caller for validation.
#[derive(Debug, Clone, Copy)]
pub struct ResolvedAgent<'a> {
    /// The Agent body.
    pub agent: &'a AgentSpec,
    /// The Agent's compiled Prompt.
    pub prompt: &'a Prompt,
}

/// Where a reference or Agent call appears.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Position<'a> {
    /// A step action or `when` of the named step.
    Step(&'a TaskId),
    /// A requirement check on the named step.
    StepCheck(&'a TaskId),
    /// A whole-Verifier requirement check.
    VerifierCheck,
}

/// Builds the structured refusal for one violated rule.
fn invalid(rule: &str, path: &str, message: impl std::fmt::Display) -> WyrdError {
    WyrdError::Validation {
        message: format!("{path}: {message}"),
        details: json!({ "rule": rule, "path": path }),
    }
}

impl VerifierSpec {
    /// Validates the Verifier, inspecting only inline Agents with inline
    /// Prompts.
    ///
    /// # Errors
    /// Returns the first violated rule; see [`VerifierSpec::validate_with`].
    pub fn validate(&self) -> Result<(), WyrdError> {
        self.validate_with(|agent| {
            let spec = agent.as_inline()?;
            Some(ResolvedAgent {
                agent: spec,
                prompt: spec.prompt.as_inline()?,
            })
        })
    }

    /// Validates the Verifier, inspecting each Agent `resolve` returns.
    ///
    /// Refuses duplicate block IDs, invalid slots, mixed Run/Window evidence,
    /// unknown or out-of-position references, wildcards outside an
    /// assertion's `each`, cycles, known-missing output fields, incompatible
    /// extraction or native-media bindings, Agent input/placeholder
    /// mismatches, check Agents with tools or a non-Verdict schema, invalid
    /// code sources, invalid operators, step budgets, and pass-rate
    /// thresholds outside `(0, 1]`.
    ///
    /// # Errors
    /// Returns [`WyrdError::Validation`] with `rule` and `path` details, or a
    /// drift error for an invalid drift check.
    pub fn validate_with<'a, F>(&'a self, resolve: F) -> Result<(), WyrdError>
    where
        F: Fn(&'a InlineableRef<AgentSpec>) -> Option<ResolvedAgent<'a>>,
    {
        Validator {
            spec: self,
            resolve,
            owners: self.requirement_owners()?,
            ancestors: BTreeMap::new(),
        }
        .run()
    }

    /// Plans step stages from `depends_on` plus every step reference in each
    /// step's action and `when`.
    ///
    /// # Errors
    /// Returns [`WyrdError::Validation`] for an unknown step, self
    /// dependency, or cycle.
    pub fn plan(&self) -> Result<ExecutionPlan, WyrdError> {
        let dependencies = self.dependencies();
        plan_stages(
            dependencies
                .iter()
                .map(|(id, deps)| (*id, deps.iter().copied())),
        )
        .map_err(|error| WyrdError::Validation {
            message: format!("steps: {error}"),
            details: json!({ "rule": "graph", "path": "steps", "error": error }),
        })
    }

    /// Returns each step's direct dependencies.
    fn dependencies(&self) -> BTreeMap<&TaskId, BTreeSet<&TaskId>> {
        self.steps
            .iter()
            .map(|step| {
                let mut deps: BTreeSet<&TaskId> = step.depends_on.iter().collect();
                deps.extend(step_references(step).filter_map(Reference::step));
                (&step.id, deps)
            })
            .collect()
    }

    /// Maps every step-requirement ID to its step, refusing any block ID
    /// used twice across steps and requirements.
    fn requirement_owners(&self) -> Result<HashMap<&TaskId, &TaskId>, WyrdError> {
        let mut seen = BTreeSet::new();
        let mut owners = HashMap::new();
        let ids = self
            .steps
            .iter()
            .flat_map(|step| {
                std::iter::once((&step.id, None)).chain(
                    step.requirements
                        .iter()
                        .map(|req| (&req.id, Some(&step.id))),
                )
            })
            .chain(self.requirements.iter().map(|req| (&req.id, None)));
        for (id, owner) in ids {
            if !seen.insert(id) {
                return Err(invalid(
                    "duplicate_id",
                    id.as_str(),
                    "step and requirement IDs must be unique across the Verifier",
                ));
            }
            if let Some(owner) = owner {
                owners.insert(id, owner);
            }
        }
        Ok(owners)
    }
}

/// Every reference in a step's action and `when`.
fn step_references(step: &Step) -> impl Iterator<Item = &Reference> {
    let action: Vec<&Reference> = match &step.action {
        Action::Extract { from } => vec![from],
        Action::Agent(call) => call
            .inputs
            .values()
            .chain(call.workspace.values())
            .collect(),
        Action::Code(call) => call.inputs.values().collect(),
    };
    action
        .into_iter()
        .chain(step.when.as_ref().map(|when| &when.path))
}

/// One validation pass with its resolver and derived lookups.
struct Validator<'a, F> {
    /// The validated spec.
    spec: &'a VerifierSpec,
    /// Caller-supplied Agent resolution.
    resolve: F,
    /// Step-requirement ID to owning step.
    owners: HashMap<&'a TaskId, &'a TaskId>,
    /// Each step's transitive upstream steps.
    ancestors: BTreeMap<&'a TaskId, BTreeSet<&'a TaskId>>,
}

impl<'a, F> Validator<'a, F>
where
    F: Fn(&'a InlineableRef<AgentSpec>) -> Option<ResolvedAgent<'a>>,
{
    /// Runs every rule in authoring order.
    fn run(mut self) -> Result<(), WyrdError> {
        let spec = self.spec;
        for (slot, input) in &spec.inputs {
            validate_slot(slot, input)?;
        }
        self.validate_evidence_mode()?;
        if let Decide::PassRate { threshold } = spec.decide
            && !(threshold.is_finite() && threshold > 0.0 && threshold <= 1.0)
        {
            return Err(invalid(
                "pass_rate",
                "decide.pass_rate.threshold",
                "threshold must be finite, above 0, and at most 1",
            ));
        }
        for (index, step) in spec.steps.iter().enumerate() {
            let path = format!("steps[{index}]");
            self.validate_step_shape(step, &path)?;
        }
        let plan = spec.plan()?;
        self.ancestors = ancestors(spec, &plan);
        for (index, step) in spec.steps.iter().enumerate() {
            for (req_index, requirement) in step.requirements.iter().enumerate() {
                let path = format!("steps[{index}].requirements[{req_index}]");
                self.validate_requirement(requirement, Position::StepCheck(&step.id), &path)?;
            }
        }
        for (index, requirement) in spec.requirements.iter().enumerate() {
            let path = format!("requirements[{index}]");
            self.validate_requirement(requirement, Position::VerifierCheck, &path)?;
        }
        Ok(())
    }

    /// Refuses mixing samples with other slots, Run with Window checks, and
    /// steps in a Window Verifier.
    fn validate_evidence_mode(&self) -> Result<(), WyrdError> {
        let spec = self.spec;
        let samples = spec
            .inputs
            .values()
            .filter(|slot| matches!(slot, InputSlot::Samples { .. }))
            .count();
        if samples > 0 && samples < spec.inputs.len() {
            return Err(invalid(
                "mixed_evidence",
                "inputs",
                "samples slots cannot be combined with other slot types",
            ));
        }
        let mut modes = spec
            .steps
            .iter()
            .flat_map(|step| &step.requirements)
            .chain(&spec.requirements)
            .map(|requirement| requirement.check.evidence());
        let first = modes.next();
        if modes.any(|mode| Some(mode) != first) {
            return Err(invalid(
                "mixed_evidence",
                "requirements",
                "drift checks cannot be combined with run-evidence checks",
            ));
        }
        if first == Some(CheckEvidence::Window) || samples > 0 {
            if first == Some(CheckEvidence::Run) {
                return Err(invalid(
                    "mixed_evidence",
                    "requirements",
                    "samples slots are judged only by drift checks",
                ));
            }
            if !spec.steps.is_empty() {
                return Err(invalid(
                    "window_steps",
                    "steps",
                    "a scheduled-window Verifier has no preparation steps",
                ));
            }
        }
        Ok(())
    }

    /// Validates one step's budgets, action, and `when` before planning.
    fn validate_step_shape(&self, step: &'a Step, path: &str) -> Result<(), WyrdError> {
        if step.timeout_seconds == Some(0) {
            return Err(invalid(
                "step_budget",
                &format!("{path}.timeout_seconds"),
                "timeout must be positive",
            ));
        }
        if step.retries > MAX_STEP_RETRIES {
            return Err(invalid(
                "step_budget",
                &format!("{path}.retries"),
                format!("at most {MAX_STEP_RETRIES} retries"),
            ));
        }
        for dep in &step.depends_on {
            if !self.spec.steps.iter().any(|other| &other.id == dep) {
                return Err(invalid(
                    "unknown_reference",
                    &format!("{path}.depends_on"),
                    format!("unknown step {}", dep.as_str()),
                ));
            }
        }
        let position = Position::Step(&step.id);
        if let Some(Condition {
            path: when,
            operator,
            ..
        }) = &step.when
        {
            let when_path = format!("{path}.when");
            self.validate_reference(when, position, false, &when_path)?;
            validate_operator(operator, &when_path)?;
        }
        let action_path = format!("{path}.action");
        match &step.action {
            Action::Extract { from } => {
                self.validate_reference(from, position, false, &action_path)?;
                let Some(InputSlot::Media { formats, .. }) = self.whole_slot(from) else {
                    return Err(invalid(
                        "extract_source",
                        &action_path,
                        "extract reads one whole media slot",
                    ));
                };
                if let Some(format) = formats.iter().find(|format| !format.is_extractable()) {
                    return Err(invalid(
                        "extract_format",
                        &action_path,
                        format!(
                            "{format} is not extractable; accept an OOXML, text, or PDF format"
                        ),
                    ));
                }
            }
            Action::Agent(call) => self.validate_agent(call, position, &action_path)?,
            Action::Code(call) => self.validate_code(call, position, &action_path)?,
        }
        Ok(())
    }

    /// Validates one requirement check in its position.
    fn validate_requirement(
        &self,
        requirement: &'a Requirement,
        position: Position<'a>,
        path: &str,
    ) -> Result<(), WyrdError> {
        let path = format!("{path}.check");
        match &requirement.check {
            Check::Assert {
                path: one,
                each,
                operator,
                ..
            } => {
                let (reference, wildcard) = match (one, each) {
                    (Some(reference), None) => (reference, false),
                    (None, Some(reference)) => (reference, true),
                    _ => {
                        return Err(invalid(
                            "assert_selector",
                            &path,
                            "set exactly one of path or each",
                        ));
                    }
                };
                self.validate_reference(reference, position, wildcard, &path)?;
                validate_operator(operator, &path)
            }
            Check::Trace(trace) => {
                self.validate_reference(&trace.on, position, false, &path)?;
                if !matches!(self.whole_slot(&trace.on), Some(InputSlot::Trace { .. })) {
                    return Err(invalid(
                        "trace_source",
                        &path,
                        "on must be one whole trace slot",
                    ));
                }
                validate_operator(&trace.operator, &path)
            }
            Check::Agent(call) => self.validate_agent(call, position, &path),
            Check::Code(call) => self.validate_code(call, position, &path),
            Check::Drift(drift) => self.validate_drift(drift, &path),
        }
    }

    /// Validates a drift check's slot binding, spec, and measurement names.
    fn validate_drift(&self, drift: &DriftCheck, path: &str) -> Result<(), WyrdError> {
        let Some(InputSlot::Samples { features, .. }) = self.whole_slot(&drift.on) else {
            return Err(invalid(
                "drift_source",
                path,
                "on must be one whole samples slot",
            ));
        };
        drift.spec.validate()?;
        let names: Vec<&str> = match &drift.spec.signal {
            DriftSignal::Distribution { features, .. } => {
                features.iter().map(FeatureName::as_str).collect()
            }
            DriftSignal::Metric { name } => vec![name.as_str()],
        };
        if let Some(missing) = names
            .into_iter()
            .find(|name| !features.iter().any(|feature| feature.as_str() == *name))
        {
            return Err(invalid(
                "drift_features",
                path,
                format!("{missing} is not a measurement of the samples slot"),
            ));
        }
        Ok(())
    }

    /// Validates a code call's source, entrypoint, and inputs.
    fn validate_code(
        &self,
        call: &CodeCall,
        position: Position<'a>,
        path: &str,
    ) -> Result<(), WyrdError> {
        match &call.source {
            CodeSource::Inline(source) if source.trim().is_empty() => {
                return Err(invalid("code_source", path, "inline source is empty"));
            }
            CodeSource::Artifact { card, .. }
                if card
                    .as_card_ref()
                    .is_some_and(|card| card.kind != CardKind::Artifact) =>
            {
                return Err(invalid(
                    "code_source",
                    path,
                    "source must reference an Artifact Card",
                ));
            }
            _ => {}
        }
        if !is_valid_parameter_name(&call.entrypoint) {
            return Err(invalid(
                "code_entrypoint",
                path,
                "entrypoint must be an identifier",
            ));
        }
        for reference in call.inputs.values() {
            self.validate_reference(reference, position, false, path)?;
        }
        Ok(())
    }

    /// Validates an Agent call: kind, references, workspace, and — when the
    /// Agent resolves — placeholders, media formats, tools, and schema.
    fn validate_agent(
        &self,
        call: &'a AgentCall,
        position: Position<'a>,
        path: &str,
    ) -> Result<(), WyrdError> {
        if call
            .agent
            .as_card_ref()
            .is_some_and(|card| card.kind != CardKind::Agent)
        {
            return Err(invalid(
                "agent_kind",
                path,
                "agent must reference an Agent Card",
            ));
        }
        for reference in call.inputs.values().chain(call.workspace.values()) {
            self.validate_reference(reference, position, false, path)?;
        }
        let in_step = matches!(position, Position::Step(_));
        if !in_step && !call.workspace.is_empty() {
            return Err(invalid(
                "check_agent_workspace",
                path,
                "check Agents have no workspace",
            ));
        }
        if let Some(bad) = call.workspace.keys().find(|key| !valid_workspace_path(key)) {
            return Err(invalid(
                "workspace_path",
                path,
                format!("{bad} must be /inputs/<name>[/<name>...] without . or .. segments"),
            ));
        }
        let Some(resolved) = (self.resolve)(&call.agent) else {
            return Ok(());
        };
        self.validate_placeholders(call, resolved.prompt, path)?;
        let tools = &resolved.agent.tool_names;
        if in_step {
            if let Some(tool) = tools
                .iter()
                .find(|tool| !STEP_TOOLS.contains(&tool.as_str()))
            {
                return Err(invalid(
                    "agent_tools",
                    path,
                    format!("{tool} is not an approved tool"),
                ));
            }
            let fs_tool = tools.iter().any(|tool| tool.starts_with("fs."));
            if fs_tool && call.workspace.is_empty() {
                return Err(invalid(
                    "agent_tools",
                    path,
                    "filesystem tools require a workspace",
                ));
            }
            let workspace_tool = fs_tool || tools.iter().any(|tool| tool == "code.run");
            if !call.workspace.is_empty() && !workspace_tool {
                return Err(invalid(
                    "agent_tools",
                    path,
                    "a workspace requires a filesystem or code.run tool",
                ));
            }
        } else {
            if !tools.is_empty() {
                return Err(invalid(
                    "check_agent_tools",
                    path,
                    "check Agents have no tools",
                ));
            }
            let required = match &resolved.prompt.response_type {
                ResponseType::JsonSchema { schema, .. } => schema.get("required"),
                ResponseType::Text => None,
            };
            let has = |field| {
                required
                    .and_then(Value::as_array)
                    .is_some_and(|items| items.iter().any(|item| item == field))
            };
            if !VERDICT_FIELDS.iter().all(|field| has(*field)) {
                return Err(invalid(
                    "check_agent_schema",
                    path,
                    "check Agent response schema must require outcome, reason, and citations",
                ));
            }
        }
        Ok(())
    }

    /// Requires Agent inputs to match the Prompt placeholders exactly, media
    /// placeholders to bind provider-native media slots, and text
    /// placeholders not to bind media slots.
    fn validate_placeholders(
        &self,
        call: &AgentCall,
        prompt: &Prompt,
        path: &str,
    ) -> Result<(), WyrdError> {
        let expected: BTreeSet<&str> = prompt
            .variables
            .iter()
            .chain(&prompt.media_variables)
            .map(String::as_str)
            .collect();
        let actual: BTreeSet<&str> = call.inputs.keys().map(ParameterName::as_str).collect();
        if expected != actual {
            return Err(WyrdError::Validation {
                message: format!("{path}: inputs must match the Prompt placeholders"),
                details: json!({
                    "rule": "agent_inputs", "path": path,
                    "expected": expected, "actual": actual,
                }),
            });
        }
        for (name, reference) in &call.inputs {
            let media_slot = match self.whole_slot(reference) {
                Some(InputSlot::Media { formats, .. }) => Some(formats),
                _ => None,
            };
            let is_media_placeholder = prompt.media_variables.iter().any(|m| m == name.as_str());
            match (is_media_placeholder, media_slot) {
                (true, Some(formats)) => {
                    if let Some(format) = formats.iter().find(|f| f.native_kind().is_none()) {
                        return Err(invalid(
                            "native_media",
                            path,
                            format!(
                                "{format} is not provider-native media; read it through a step workspace or supply a PDF or image rendition"
                            ),
                        ));
                    }
                }
                (true, None) => {
                    return Err(invalid(
                        "native_media",
                        path,
                        format!("media placeholder {name} must bind one whole media slot"),
                    ));
                }
                (false, Some(_)) => {
                    return Err(invalid(
                        "media_as_text",
                        path,
                        format!("text placeholder {name} cannot bind a media slot"),
                    ));
                }
                (false, None) => {}
            }
        }
        Ok(())
    }

    /// Validates one reference in its position.
    fn validate_reference(
        &self,
        reference: &Reference,
        position: Position<'a>,
        wildcard: bool,
        path: &str,
    ) -> Result<(), WyrdError> {
        let refuse =
            |rule, message: &str| Err(invalid(rule, path, format!("{reference}: {message}")));
        if reference.has_wildcard() && !wildcard {
            return refuse(
                "wildcard_position",
                "[*] is allowed only in an assertion's each",
            );
        }
        match &reference.root {
            ReferenceRoot::Input(slot) => {
                let Some(input) = self.spec.inputs.get(slot) else {
                    return refuse("unknown_reference", "unknown input slot");
                };
                if let InputSlot::Json {
                    schema: Some(schema),
                    ..
                } = input
                    && !admits(schema, schema, &reference.path)
                {
                    return refuse("missing_field", "the slot schema has no such field");
                }
                Ok(())
            }
            ReferenceRoot::RequirementId | ReferenceRoot::RequirementStatement => {
                if matches!(position, Position::Step(_)) {
                    return refuse(
                        "requirement_position",
                        "requirement.* is readable only in checks",
                    );
                }
                Ok(())
            }
            ReferenceRoot::StepOutput(step)
            | ReferenceRoot::StepStatus(step)
            | ReferenceRoot::StepVerdict { step, .. } => {
                let Some(target) = self.spec.steps.iter().find(|s| &s.id == step) else {
                    return refuse("unknown_reference", "unknown step");
                };
                if let ReferenceRoot::StepVerdict { requirement, .. } = &reference.root {
                    if self.owners.get(requirement) != Some(&step) {
                        return refuse("unknown_reference", "the step has no such requirement");
                    }
                    if !matches!(position, Position::Step(_)) {
                        return refuse(
                            "verdict_position",
                            "checks cannot read requirement verdicts",
                        );
                    }
                }
                if let Position::StepCheck(own) = position {
                    let upstream = self
                        .ancestors
                        .get(own)
                        .is_some_and(|set| set.contains(step));
                    if step != own && !upstream {
                        return refuse(
                            "step_position",
                            "a step requirement reads only its own step and upstream steps",
                        );
                    }
                }
                if matches!(reference.root, ReferenceRoot::StepOutput(_))
                    && let Some(schema) = self.output_schema(target)
                    && !admits(&schema, &schema, &reference.path)
                {
                    return refuse("missing_field", "the step output has no such field");
                }
                Ok(())
            }
        }
    }

    /// Returns the slot a reference names when it selects the whole slot.
    fn whole_slot(&self, reference: &Reference) -> Option<&'a InputSlot> {
        match &reference.root {
            ReferenceRoot::Input(slot) if reference.path.is_empty() => self.spec.inputs.get(slot),
            _ => None,
        }
    }

    /// Returns the known output schema of a step, when it has one.
    fn output_schema(&self, step: &'a Step) -> Option<Value> {
        match &step.action {
            Action::Extract { from } => match self.whole_slot(from)? {
                InputSlot::Media { formats, .. } => Some(ExtractOutput::schema_for(formats)),
                _ => None,
            },
            Action::Agent(call) => match &(self.resolve)(&call.agent)?.prompt.response_type {
                ResponseType::JsonSchema { schema, .. } => Some(schema.clone()),
                ResponseType::Text => Some(json!({ "type": "string" })),
            },
            Action::Code(_) => None,
        }
    }
}

/// Validates one slot's own constraints.
fn validate_slot(slot: &SlotName, input: &InputSlot) -> Result<(), WyrdError> {
    let path = format!("inputs.{slot}");
    match input {
        InputSlot::Media {
            formats, max_bytes, ..
        } => {
            if formats.is_empty() {
                return Err(invalid(
                    "slot",
                    &path,
                    "media slots accept at least one format",
                ));
            }
            if *max_bytes == Some(0) {
                return Err(invalid("slot", &path, "max_bytes must be positive"));
            }
        }
        InputSlot::Samples { features, .. } if features.is_empty() => {
            return Err(invalid(
                "slot",
                &path,
                "samples slots name at least one measurement",
            ));
        }
        InputSlot::Json {
            schema: Some(schema),
            ..
        } if !schema.is_object() && !schema.is_boolean() => {
            return Err(invalid(
                "slot",
                &path,
                "schema must be a JSON Schema object",
            ));
        }
        _ => {}
    }
    Ok(())
}

/// Validates an operator, keeping its message under this Verifier path.
fn validate_operator(
    operator: &crate::vala::eval::ComparisonOperator,
    path: &str,
) -> Result<(), WyrdError> {
    operator
        .validate()
        .map_err(|error| invalid("operator", path, error))
}

/// Returns whether a workspace key is `/inputs/` plus safe path segments.
fn valid_workspace_path(key: &str) -> bool {
    key.strip_prefix("/inputs/").is_some_and(|rest| {
        rest.split('/')
            .all(|segment| !segment.is_empty() && segment != "." && segment != "..")
    })
}

/// Computes each step's transitive upstream steps in stage order.
fn ancestors<'a>(
    spec: &'a VerifierSpec,
    plan: &ExecutionPlan,
) -> BTreeMap<&'a TaskId, BTreeSet<&'a TaskId>> {
    let dependencies = spec.dependencies();
    let mut ancestors: BTreeMap<&TaskId, BTreeSet<&TaskId>> = BTreeMap::new();
    for id in plan.stages.iter().flat_map(|stage| &stage.tasks) {
        let Some((&id, deps)) = dependencies.get_key_value(id) else {
            continue;
        };
        let mut all = deps.clone();
        for dep in deps {
            if let Some(upstream) = ancestors.get(dep) {
                all.extend(upstream.iter().copied());
            }
        }
        ancestors.insert(id, all);
    }
    ancestors
}

impl VerifierSpec {
    /// Validates a schedule binding's `inputs` map against this Verifier.
    ///
    /// Every required slot must be mapped, every mapped slot must be a
    /// samples slot of this Verifier, and each selector must name exactly
    /// the slot's measurements. Names are never inferred.
    ///
    /// # Errors
    /// Returns [`WyrdError::Validation`] with rule `scheduled_input`.
    pub fn validate_schedule(
        &self,
        inputs: &BTreeMap<SlotName, ScheduledInput>,
    ) -> Result<(), WyrdError> {
        for (slot, input) in &self.inputs {
            if input.required() && !inputs.contains_key(slot) {
                return Err(invalid(
                    "scheduled_input",
                    &format!("inputs.{slot}"),
                    "the schedule binding must map every required slot",
                ));
            }
        }
        for (slot, selector) in inputs {
            let path = format!("inputs.{slot}");
            let Some(InputSlot::Samples { features, .. }) = self.inputs.get(slot) else {
                return Err(invalid(
                    "scheduled_input",
                    &path,
                    "only samples slots of this Verifier are scheduled",
                ));
            };
            let selected: BTreeSet<_> = selector.names().iter().collect();
            if selected != features.iter().collect() {
                return Err(invalid(
                    "scheduled_input",
                    &path,
                    "the selector must name exactly the slot's measurements",
                ));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::super::{ScheduledInput, VerifierSpec};
    use crate::error::WyrdError;

    /// Parses an authored Verifier body.
    ///
    /// # Panics
    /// Panics when the body does not deserialize.
    fn parse(body: &Value) -> VerifierSpec {
        serde_json::from_value(body.clone()).expect("authored Verifier deserializes")
    }

    /// Returns the `rule` of a refusal.
    ///
    /// # Panics
    /// Panics when validation succeeds or the refusal carries no rule.
    fn rule(body: &Value) -> String {
        match parse(body).validate() {
            Err(WyrdError::Validation { details, .. }) => details["rule"]
                .as_str()
                .expect("refusal names its rule")
                .to_owned(),
            other => panic!("expected a validation refusal for {body}, got {other:?}"),
        }
    }

    /// Returns `body` with the JSON-pointer target set to `value`, inserting
    /// the final object key when it is absent.
    ///
    /// # Panics
    /// Panics when the pointer's parent does not exist.
    fn with(mut body: Value, pointer: &str, value: Value) -> Value {
        let (parent, key) = pointer.rsplit_once('/').expect("pointer has a parent");
        match body.pointer_mut(parent).expect(pointer) {
            Value::Object(fields) => {
                fields.insert(key.to_owned(), value);
            }
            target => {
                *target
                    .get_mut(key.parse::<usize>().expect("array index"))
                    .expect(pointer) = value;
            }
        }
        body
    }

    /// An inline Agent with an inline portable Prompt.
    fn agent(tools: &[&str], content: Value, response_format: Option<Value>) -> Value {
        let mut prompt = json!({
            "provider": "openai",
            "model": "gpt-4o",
            "messages": [{"role": "user", "content": content}],
        });
        if let Some(schema) = response_format {
            prompt["response_format"] = schema;
        }
        let mut agent = json!({
            "prompt": prompt,
            "execution_route": {"kind": "wyrd_gateway", "deployment": "openai-primary"},
        });
        if !tools.is_empty() {
            agent["tool_names"] = json!(tools);
        }
        agent
    }

    /// The Verdict response schema every check-position Agent returns.
    fn verdict_schema() -> Value {
        json!({
            "type": "object",
            "properties": {
                "outcome": {"type": "string"},
                "reason": {"type": "string"},
                "citations": {"type": "array"}
            },
            "required": ["outcome", "reason", "citations"],
            "additionalProperties": false
        })
    }

    /// A multi-step Verifier exercising every action and Run check kind.
    fn review() -> Value {
        json!({
            "inputs": {
                "answer": {"type": "json", "schema": {"type": "object", "properties": {"text": {"type": "string"}}, "additionalProperties": false}},
                "policy": {"type": "media", "formats": ["pdf"]},
                "deck": {"type": "media", "formats": ["pptx"], "required": false},
                "trace": {"type": "trace"}
            },
            "steps": [
                {"id": "structure", "action": {"kind": "extract", "from": "inputs.deck"},
                 "requirements": [{"id": "titles", "statement": "Every slide has a title",
                     "check": {"kind": "assert", "each": "steps.structure.output.slides[*].title", "operator": "is_not_null"}}]},
                {"id": "findings",
                 "when": {"path": "steps.structure.requirements.titles.outcome", "operator": "equals", "expected": "passed"},
                 "action": {"kind": "agent",
                     "agent": agent(&["fs.read"], json!("Review {{outline}}"), Some(json!({"type": "object", "properties": {"score": {"type": "integer"}}, "required": ["score"], "additionalProperties": false}))),
                     "inputs": {"outline": "steps.structure.output"},
                     "workspace": {"/inputs/policy.pdf": "inputs.policy"}},
                 "timeout_seconds": 60, "retries": 2,
                 "requirements": [{"id": "score", "statement": "Score at least 4",
                     "check": {"kind": "assert", "path": "steps.findings.output.score", "operator": "greater_than_or_equals", "expected": 4}}]},
                {"id": "count", "depends_on": ["findings"],
                 "action": {"kind": "code", "entrypoint": "count", "source": {"inline": "def count(text):\n    return len(text)"}, "inputs": {"text": "inputs.answer.text"}}}
            ],
            "requirements": [
                {"id": "no-guarantee", "statement": "No guaranteed refunds",
                 "check": {"kind": "assert", "path": "inputs.answer.text", "operator": "not_contains", "expected": "guaranteed refund"}},
                {"id": "traced", "statement": "A retrieval span exists",
                 "check": {"kind": "trace", "on": "inputs.trace", "span_selector": "$.spans[0].name", "operator": "equals", "expected": "retrieve"}},
                {"id": "judged", "statement": "The answer follows the policy",
                 "check": {"kind": "agent",
                     "agent": agent(&[], json!([{"type": "document", "source": "${media:policy}"}, {"type": "text", "text": "{{claim}} {{answer}} {{findings}}"}]), Some(verdict_schema())),
                     "inputs": {"claim": "requirement.statement", "answer": "inputs.answer.text", "policy": "inputs.policy", "findings": "steps.findings.output"}}},
                {"id": "counted", "statement": "Counting code agrees",
                 "check": {"kind": "code", "entrypoint": "check", "source": {"artifact": {"card": {"kind": "Artifact", "space": "support", "name": "checks", "version": "1.0.0"}, "path": "check.py"}},
                     "inputs": {"count": "steps.count.output", "statement": "requirement.statement"}}}
            ],
            "decide": {"pass_rate": {"threshold": 0.75}}
        })
    }

    /// A scheduled drift Verifier.
    fn drift() -> Value {
        json!({
            "inputs": {"current": {"type": "samples", "features": ["income", "dti"]}},
            "requirements": [{"id": "stable", "statement": "Inputs are stable",
                "check": {"kind": "drift", "on": "inputs.current", "method": "Spc",
                    "signal": {"kind": "Distribution", "baseline_ref": {"kind": "Data", "space": "risk", "name": "train", "version": "1.0.0"}, "features": ["income"]},
                    "condition": {"kind": "Statistical"},
                    "profile": {"kind": "Spc", "sample_size": 5}}}]
        })
    }

    /// Authored Verifiers round-trip and validate, with defaults omitted.
    ///
    /// # Panics
    /// Panics when a valid Verifier is refused or does not round-trip.
    #[test]
    fn verifier_spec_round_trips_and_validates() {
        for body in [
            review(),
            drift(),
            json!({}),
            json!({"inputs": {"a": {"type": "json"}}}),
        ] {
            let spec = parse(&body);
            spec.validate().expect("valid Verifier");
            assert_eq!(serde_json::to_value(&spec).expect("serialize"), body);
        }
        let spec = parse(&review());
        assert_eq!(
            spec.inputs.values().filter(|slot| slot.required()).count(),
            3
        );
        let plan = spec.plan().expect("plan");
        let stages: Vec<Vec<&str>> = plan
            .stages
            .iter()
            .map(|stage| {
                stage
                    .tasks
                    .iter()
                    .map(crate::vala::eval::TaskId::as_str)
                    .collect()
            })
            .collect();
        assert_eq!(
            stages,
            vec![vec!["structure"], vec!["findings"], vec!["count"]]
        );
        let explicit =
            json!({"inputs": {"a": {"type": "json", "required": true}}, "decide": "all"});
        assert_eq!(
            serde_json::to_value(parse(&explicit)).expect("serialize"),
            json!({"inputs": {"a": {"type": "json"}}})
        );
    }

    /// Pass-rate thresholds must be finite and in `(0, 1]`.
    ///
    /// # Panics
    /// Panics when a threshold boundary is classified incorrectly.
    #[test]
    fn verifier_spec_pass_rate_bounds() {
        for (threshold, valid) in [
            (1.0, true),
            (0.01, true),
            (0.0, false),
            (1.5, false),
            (-0.1, false),
        ] {
            let body = json!({"decide": {"pass_rate": {"threshold": threshold}}});
            assert_eq!(parse(&body).validate().is_ok(), valid, "{threshold}");
        }
    }

    /// Unknown authored fields and malformed shapes are refused at decode.
    ///
    /// # Panics
    /// Panics when a malformed body deserializes.
    #[test]
    fn verifier_spec_refuses_unknown_fields() {
        for body in [
            json!({"implementation": {}}),
            json!({"inputs": {"a": {"type": "json", "extra": 1}}}),
            json!({"inputs": {"a": {"type": "blob"}}}),
            json!({"requirements": [{"id": "a", "statement": "s", "check": {"kind": "assert", "path": "inputs.a", "operator": "equals", "expected": 1, "extra": 1}}]}),
            json!({"inputs": {"s": {"type": "samples", "features": ["x"]}}, "requirements": [{"id": "a", "statement": "s", "check": {"kind": "drift", "on": "inputs.s", "method": "Spc", "signal": {"kind": "Metric", "name": "x"}, "condition": {"kind": "Statistical"}, "extra": 1}}]}),
            json!({"steps": [{"id": "a", "action": {"kind": "fetch"}}]}),
            json!({"decide": {"majority": {}}}),
        ] {
            assert!(
                serde_json::from_value::<VerifierSpec>(body.clone()).is_err(),
                "{body}"
            );
        }
    }

    /// Each graph, reference, and evidence rule refuses its violation.
    ///
    /// # Panics
    /// Panics when a violation is accepted or refused under the wrong rule.
    #[test]
    fn verifier_spec_refuses_invalid_graphs_and_references() {
        let with = |pointer: &str, value: Value| with(review(), pointer, value);
        let cases = [
            (with("/requirements/0/id", json!("titles")), "duplicate_id"),
            (with("/steps/2/id", json!("structure")), "duplicate_id"),
            (
                with("/requirements/0/check/path", json!("inputs.missing")),
                "unknown_reference",
            ),
            (
                with("/requirements/0/check/path", json!("inputs.answer.missing")),
                "missing_field",
            ),
            (
                with(
                    "/requirements/0/check/path",
                    json!("steps.findings.output.nope"),
                ),
                "missing_field",
            ),
            (
                with(
                    "/steps/0/requirements/0/check/each",
                    json!("steps.structure.output.slides[*].heading"),
                ),
                "missing_field",
            ),
            (
                with("/requirements/0/check/path", json!("steps.ghost.output")),
                "unknown_reference",
            ),
            (
                with(
                    "/requirements/0/check/path",
                    json!("steps.findings.requirements.titles.outcome"),
                ),
                "unknown_reference",
            ),
            (
                with(
                    "/requirements/0/check/path",
                    json!("steps.structure.requirements.titles.outcome"),
                ),
                "verdict_position",
            ),
            (
                with("/requirements/0/check/path", json!("inputs.answer[*]")),
                "wildcard_position",
            ),
            (
                with(
                    "/steps/0/requirements/0/check/each",
                    json!("steps.count.output"),
                ),
                "step_position",
            ),
            (with("/steps/2/depends_on", json!(["count"])), "graph"),
            (with("/steps/0/depends_on", json!(["count"])), "graph"),
            (
                with("/steps/2/depends_on", json!(["ghost"])),
                "unknown_reference",
            ),
            (
                with(
                    "/steps/2/action/inputs/text",
                    json!("requirement.statement"),
                ),
                "requirement_position",
            ),
            (
                with("/requirements/0/check/each", json!("inputs.answer.text")),
                "assert_selector",
            ),
            (
                with(
                    "/requirements/0/check/operator",
                    json!({"kind": "matches_regex", "pattern": "("}),
                ),
                "operator",
            ),
            (
                with(
                    "/inputs/trace",
                    json!({"type": "samples", "features": ["x"]}),
                ),
                "mixed_evidence",
            ),
            (with("/inputs/policy/formats", json!([])), "slot"),
            (with("/inputs/policy/max_bytes", json!(0)), "slot"),
            (with("/steps/1/timeout_seconds", json!(0)), "step_budget"),
            (with("/steps/1/retries", json!(6)), "step_budget"),
            (
                with("/steps/0/action/from", json!("inputs.policy.x")),
                "extract_source",
            ),
            (
                with("/inputs/deck/formats", json!(["ppt"])),
                "extract_format",
            ),
            (
                with("/inputs/deck/formats", json!(["zip"])),
                "extract_format",
            ),
            (
                with("/requirements/1/check/on", json!("inputs.answer")),
                "trace_source",
            ),
            (
                with("/steps/2/action/entrypoint", json!("not valid")),
                "code_entrypoint",
            ),
            (
                with("/steps/2/action/source/inline", json!("  ")),
                "code_source",
            ),
            (
                with(
                    "/requirements/3/check/source/artifact/card/kind",
                    json!("Data"),
                ),
                "code_source",
            ),
            (
                with(
                    "/steps/1/action/workspace",
                    json!({"/inputs/../etc": "inputs.policy"}),
                ),
                "workspace_path",
            ),
        ];
        for (body, expected) in cases {
            assert_eq!(rule(&body), expected, "{body}");
        }
        let mut drift = drift();
        drift["requirements"][0]["check"]["signal"]["features"] = json!(["fico"]);
        assert_eq!(rule(&drift), "drift_features");
        let mut mixed = drift.clone();
        mixed["requirements"][0]["check"]["signal"]["features"] = json!(["income"]);
        mixed["requirements"]
            .as_array_mut()
            .expect("requirements")
            .push(json!({"id": "x", "statement": "s", "check": {"kind": "assert", "path": "inputs.current", "operator": "is_not_null"}}));
        assert_eq!(rule(&mixed), "mixed_evidence");
        let mut stepped = self::drift();
        stepped["steps"] = json!([{"id": "a", "action": {"kind": "code", "entrypoint": "f", "source": {"inline": "def f(): pass"}}}]);
        assert_eq!(rule(&stepped), "window_steps");
        let mut bad = self::drift();
        bad["requirements"][0]["check"]["on"] = json!("inputs.current.income");
        assert_eq!(rule(&bad), "drift_source");
    }

    /// Agent calls follow their Prompt placeholders and position rules.
    ///
    /// # Panics
    /// Panics when an Agent rule is not enforced.
    #[test]
    fn verifier_spec_enforces_agent_positions_and_media() {
        let with = |pointer: &str, value: Value| with(review(), pointer, value);
        let text_judge = agent(
            &[],
            json!("{{claim}} {{answer}} {{policy}} {{findings}}"),
            Some(verdict_schema()),
        );
        let cases = [
            (
                with("/requirements/2/check/inputs/extra", json!("inputs.answer")),
                "agent_inputs",
            ),
            (with("/steps/1/action/inputs", json!({})), "agent_inputs"),
            (
                with(
                    "/requirements/2/check/inputs/policy",
                    json!("inputs.answer.text"),
                ),
                "native_media",
            ),
            (
                with("/inputs/policy/formats", json!(["pdf", "docx"])),
                "native_media",
            ),
            (
                with("/requirements/2/check/agent", text_judge),
                "media_as_text",
            ),
            (
                with(
                    "/requirements/2/check/agent",
                    agent(
                        &["fs.read"],
                        json!([{"type": "document", "source": "${media:policy}"}, {"type": "text", "text": "{{claim}} {{answer}} {{findings}}"}]),
                        Some(verdict_schema()),
                    ),
                ),
                "check_agent_tools",
            ),
            (
                with(
                    "/requirements/2/check/agent",
                    agent(
                        &[],
                        json!([{"type": "document", "source": "${media:policy}"}, {"type": "text", "text": "{{claim}} {{answer}} {{findings}}"}]),
                        None,
                    ),
                ),
                "check_agent_schema",
            ),
            (
                with(
                    "/requirements/2/check/workspace",
                    json!({"/inputs/p.pdf": "inputs.policy"}),
                ),
                "check_agent_workspace",
            ),
            (
                with("/steps/1/action/agent/tool_names", json!(["http.fetch"])),
                "agent_tools",
            ),
            (with("/steps/1/action/workspace", json!({})), "agent_tools"),
            (
                with("/steps/1/action/agent/tool_names", json!(["cards.get"])),
                "agent_tools",
            ),
            (
                with(
                    "/steps/1/action/agent",
                    json!({"kind": "Prompt", "space": "support", "name": "answer-prompt", "version": "1.0.0"}),
                ),
                "agent_kind",
            ),
        ];
        for (body, expected) in cases {
            assert_eq!(rule(&body), expected, "{body}");
        }
        let registered = with(
            "/requirements/2/check/agent",
            json!({"kind": "Agent", "space": "support", "name": "judge", "version": "1.0.0"}),
        );
        parse(&registered)
            .validate()
            .expect("an unresolved registered Agent is validated by its resolver");
    }

    /// Schedule bindings map every required samples slot by exact names.
    ///
    /// # Panics
    /// Panics when a binding mapping is classified incorrectly.
    #[test]
    fn verifier_spec_validates_schedule_inputs() {
        let spec = parse(&drift());
        let mapping =
            |value: Value| -> std::collections::BTreeMap<crate::ids::SlotName, ScheduledInput> {
                serde_json::from_value(value).expect("mapping")
            };
        spec.validate_schedule(&mapping(
            json!({"current": {"features": ["dti", "income"]}}),
        ))
        .expect("exact selector");
        for invalid in [
            json!({}),
            json!({"current": {"features": ["income"]}}),
            json!({"current": {"metric": "income"}}),
            json!({"current": {"features": ["income", "dti"]}, "other": {"metric": "x"}}),
        ] {
            assert!(
                spec.validate_schedule(&mapping(invalid.clone())).is_err(),
                "{invalid}"
            );
        }
    }
}
