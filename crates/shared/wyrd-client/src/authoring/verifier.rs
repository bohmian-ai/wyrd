//! Verifier authoring: inputs, steps, requirements, actions, and checks.

use std::collections::BTreeMap;
use std::path::Path;

use serde_json::Value;
use wyrd_spec::card::agent::AgentSpec;
use wyrd_spec::card::drift::DriftSpec;
use wyrd_spec::card::prompt::ParameterName;
use wyrd_spec::card::verifier::{
    self as spec, CodeCheck, CodeRuntime, CodeSource, Condition, Decide, DriftCheck, InputSlot,
    OrderBy, Reference, TraceCheck, VerifierSpec, WorkspaceSource,
};
use wyrd_spec::envelope::CardKind;
use wyrd_spec::error::WyrdError;
use wyrd_spec::ids::SlotName;
use wyrd_spec::verification::{
    ComparisonOperator, ContextCapture, ExecutionPlan, JsonPath, TaskId,
};

use super::{ArtifactFile, Dependencies, Linked, Verifier, exact_metadata, invalid};

impl Verifier {
    /// Define Verifier `name` at exact `version` with no inputs, steps, or
    /// requirements, deciding by every requirement passing.
    ///
    /// # Errors
    /// Returns identity errors as [`super::CardDefinition::new`] does.
    pub fn new(name: &str, version: &str) -> Result<Self, WyrdError> {
        Ok(Self::with_metadata(
            CardKind::Verifier,
            exact_metadata(name, version)?,
            VerifierSpec {
                description: None,
                inputs: BTreeMap::new(),
                steps: Vec::new(),
                requirements: Vec::new(),
                decide: Decide::All,
                context_capture: None,
                order_by: None,
            },
        ))
    }

    /// Set the Verifier's human-readable description.
    pub fn set_description(&mut self, description: &str) {
        self.spec.description = Some(description.to_owned());
    }

    /// Declare the required JSON input `name`, constrained by the JSON Schema
    /// `schema` (`Value::Null` accepts any JSON value).
    ///
    /// # Errors
    /// As [`Self::add_slot`].
    pub fn add_input(&mut self, name: &str, schema: Value) -> Result<Reference, WyrdError> {
        self.add_input_with_required(name, schema, true)
    }

    /// Declare JSON input `name`, required or optional.
    ///
    /// # Errors
    /// As [`Self::add_slot`].
    pub fn add_input_with_required(
        &mut self,
        name: &str,
        schema: Value,
        required: bool,
    ) -> Result<Reference, WyrdError> {
        self.add_slot(
            name,
            InputSlot::Json {
                schema: (!schema.is_null()).then_some(schema),
                required,
            },
        )
    }

    /// Declare input `name` as any canonical slot, such as media, trace, or
    /// samples, and return its `inputs.<name>` reference.
    ///
    /// # Errors
    /// Returns [`WyrdError::Validation`] for an invalid slot name, and with
    /// rule `duplicate_input` for a name already declared.
    pub fn add_slot(&mut self, name: &str, slot: InputSlot) -> Result<Reference, WyrdError> {
        let path = format!("spec.inputs.{name}");
        let slot_name = SlotName::new(name).map_err(|error| invalid("slot_name", &path, error))?;
        if self.spec.inputs.contains_key(&slot_name) {
            return Err(invalid(
                "duplicate_input",
                &path,
                "input slot names must be unique within a Verifier",
            ));
        }
        let reference =
            Reference::input(name).map_err(|error| invalid("reference", &path, error))?;
        self.spec.inputs.insert(slot_name, slot);
        Ok(reference)
    }

    /// Append `step`, capturing its current value.
    ///
    /// # Errors
    /// Returns [`WyrdError::Validation`] with rule `duplicate_id` when the
    /// step or one of its requirements reuses a block ID of this Verifier.
    pub fn add_step(&mut self, step: Step) -> Result<(), WyrdError> {
        for id in std::iter::once(&step.step.id).chain(step.step.requirements.iter().map(|r| &r.id))
        {
            self.refuse_known_id(id)?;
        }
        self.spec.steps.push(step.step);
        self.dependencies.absorb(step.dependencies);
        Ok(())
    }

    /// Append a top-level `requirement`, capturing its current value.
    ///
    /// # Errors
    /// Returns [`WyrdError::Validation`] with rule `duplicate_id` when its ID
    /// is already a block ID of this Verifier.
    pub fn add_requirement(&mut self, requirement: Requirement) -> Result<(), WyrdError> {
        self.refuse_known_id(&requirement.requirement.id)?;
        self.spec.requirements.push(requirement.requirement);
        self.dependencies.absorb(requirement.dependencies);
        Ok(())
    }

    /// Decide the verdict by `decide`.
    pub fn set_decide(&mut self, decide: Decide) {
        self.spec.decide = decide;
    }

    /// Capture execution context by `capture`.
    pub fn set_context_capture(&mut self, capture: ContextCapture) {
        self.spec.context_capture = Some(capture);
    }

    /// Order a group Verifier's source executions by `order`.
    pub fn set_order_by(&mut self, order: OrderBy) {
        self.spec.order_by = Some(order);
    }

    /// The step stages the server would execute, from explicit
    /// `depends_on` edges only.
    ///
    /// # Errors
    /// Returns the shared Verifier validation's refusal.
    pub fn plan(&self) -> Result<ExecutionPlan, WyrdError> {
        self.spec.plan()
    }

    /// Run the shared Verifier validation the server applies at declaration.
    ///
    /// # Errors
    /// Returns its [`WyrdError::Validation`] with the failing rule and path.
    pub fn validate(&self) -> Result<(), WyrdError> {
        self.spec.validate()
    }

    /// Refuse `id` when a step or requirement already uses it.
    ///
    /// # Errors
    /// Returns rule `duplicate_id`, as the shared validation does.
    fn refuse_known_id(&self, id: &TaskId) -> Result<(), WyrdError> {
        let known = self
            .spec
            .steps
            .iter()
            .flat_map(|step| {
                std::iter::once(&step.id).chain(step.requirements.iter().map(|r| &r.id))
            })
            .chain(self.spec.requirements.iter().map(|r| &r.id))
            .any(|known| known == id);
        if known {
            return Err(invalid(
                "duplicate_id",
                id.as_str(),
                "step and requirement IDs must be unique across the Verifier",
            ));
        }
        Ok(())
    }
}

/// One Verifier step: an action, its explicit dependencies, an optional
/// condition, and the requirements judging its output.
#[derive(Debug, Clone, PartialEq)]
pub struct Step {
    /// The canonical step.
    step: spec::Step,
    /// Definitions and local Artifacts the step references.
    dependencies: Dependencies,
}

impl Step {
    /// A step `id` running `action`, with no dependencies, condition,
    /// requirements, timeout, or retries.
    ///
    /// # Errors
    /// Returns [`WyrdError::Validation`] for an invalid block ID.
    pub fn new(id: &str, action: Action) -> Result<Self, WyrdError> {
        Ok(Self {
            step: spec::Step {
                id: TaskId::new(id)?,
                action: action.action,
                depends_on: Vec::new(),
                when: None,
                requirements: Vec::new(),
                timeout_seconds: None,
                retries: 0,
            },
            dependencies: action.dependencies,
        })
    }

    /// The reference to `path` inside this step's output, such as
    /// `authorized` for `steps.<id>.output.authorized`; empty is the whole
    /// output.
    ///
    /// # Errors
    /// Returns [`WyrdError::Validation`] for a path that does not parse.
    pub fn output(&self, path: &str) -> Result<Reference, WyrdError> {
        Reference::step_output(self.step.id.as_str())
            .and_then(|output| output.at(path))
            .map_err(|error| {
                invalid(
                    "reference",
                    &format!("steps.{}", self.step.id.as_str()),
                    error,
                )
            })
    }

    /// The reference to this step's execution status.
    ///
    /// # Errors
    /// Returns [`WyrdError::Validation`] if the reference does not parse.
    pub fn status(&self) -> Result<Reference, WyrdError> {
        let path = format!("steps.{}.status", self.step.id.as_str());
        Reference::parse(&path).map_err(|error| invalid("reference", &path, error))
    }

    /// Run after `step` completes; only explicit dependencies order steps.
    pub fn add_depends_on(&mut self, step: &Step) {
        if !self.step.depends_on.contains(&step.step.id) {
            self.step.depends_on.push(step.step.id.clone());
        }
    }

    /// Run only when `reference` compares to `expected` by `operator`.
    pub fn set_when(
        &mut self,
        reference: Reference,
        operator: ComparisonOperator,
        expected: Value,
    ) {
        self.step.when = Some(Condition {
            path: reference,
            operator,
            expected,
        });
    }

    /// Judge this step's output with `requirement`.
    ///
    /// # Errors
    /// Returns [`WyrdError::Validation`] with rule `duplicate_id` when the
    /// step or one of its requirements already uses the ID.
    pub fn add_requirement(&mut self, requirement: Requirement) -> Result<(), WyrdError> {
        let id = &requirement.requirement.id;
        if self.step.id == *id || self.step.requirements.iter().any(|known| known.id == *id) {
            return Err(invalid(
                "duplicate_id",
                id.as_str(),
                "step and requirement IDs must be unique across the Verifier",
            ));
        }
        self.step.requirements.push(requirement.requirement);
        self.dependencies.absorb(requirement.dependencies);
        Ok(())
    }

    /// Stop each attempt after `seconds`.
    pub fn set_timeout_seconds(&mut self, seconds: u32) {
        self.step.timeout_seconds = Some(seconds);
    }

    /// Retry a failed attempt up to `retries` times.
    ///
    /// # Errors
    /// Returns [`WyrdError::Validation`] above the shared retry limit.
    pub fn set_retries(&mut self, retries: u32) -> Result<(), WyrdError> {
        if retries > spec::MAX_STEP_RETRIES {
            return Err(invalid(
                "retries",
                &format!("steps.{}.retries", self.step.id.as_str()),
                format!("at most {} retries", spec::MAX_STEP_RETRIES),
            ));
        }
        self.step.retries = retries;
        Ok(())
    }

    /// The canonical step.
    #[must_use]
    pub fn step(&self) -> &spec::Step {
        &self.step
    }
}

/// One declared requirement: an ID, the claim it judges, and its check.
#[derive(Debug, Clone, PartialEq)]
pub struct Requirement {
    /// The canonical requirement.
    requirement: spec::Requirement,
    /// Definitions and local Artifacts the check references.
    dependencies: Dependencies,
}

impl Requirement {
    /// Requirement `id` claiming `statement`, judged by `check`.
    ///
    /// # Errors
    /// Returns [`WyrdError::Validation`] for an invalid block ID.
    pub fn new(id: &str, statement: &str, check: Check) -> Result<Self, WyrdError> {
        Ok(Self {
            requirement: spec::Requirement {
                id: TaskId::new(id)?,
                statement: statement.to_owned(),
                check: check.check,
            },
            dependencies: check.dependencies,
        })
    }

    /// The canonical requirement.
    #[must_use]
    pub fn requirement(&self) -> &spec::Requirement {
        &self.requirement
    }
}

/// A step action: extract a media slot, call an Agent, or run code.
#[derive(Debug, Clone, PartialEq)]
pub struct Action {
    /// The canonical action.
    action: spec::Action,
    /// Definitions and local Artifacts the action references.
    dependencies: Dependencies,
}

impl Action {
    /// Extract text and structure from the media slot `from`.
    #[must_use]
    pub fn extract(from: Reference) -> Self {
        Self {
            action: spec::Action::Extract { from },
            dependencies: Dependencies::default(),
        }
    }

    /// Invoke an Agent.
    #[must_use]
    pub fn agent(call: AgentCall) -> Self {
        let (call, dependencies) = call.into_parts();
        Self {
            action: spec::Action::Agent(call),
            dependencies,
        }
    }

    /// Run sandboxed code returning JSON data.
    #[must_use]
    pub fn code(call: CodeCall) -> Self {
        Self {
            action: spec::Action::Code(call.call),
            dependencies: call.dependencies,
        }
    }

    /// The canonical action.
    #[must_use]
    pub fn action(&self) -> &spec::Action {
        &self.action
    }
}

/// A verdict-producing check: assert, trace, Agent, code, or drift.
#[derive(Debug, Clone, PartialEq)]
pub struct Check {
    /// The canonical check.
    check: spec::Check,
    /// Definitions and local Artifacts the check references.
    dependencies: Dependencies,
}

impl Check {
    /// Compare the one value at `path` to `expected` by `operator`.
    #[must_use]
    pub fn assert(path: Reference, operator: ComparisonOperator, expected: Value) -> Self {
        Self::plain(spec::Check::Assert {
            path: Some(path),
            each: None,
            operator,
            expected,
        })
    }

    /// Require `path` to equal `expected`.
    #[must_use]
    pub fn equals(path: Reference, expected: Value) -> Self {
        Self::assert(path, ComparisonOperator::Equals, expected)
    }

    /// Compare every value `each` selects through `[*]` to `expected`.
    #[must_use]
    pub fn each(each: Reference, operator: ComparisonOperator, expected: Value) -> Self {
        Self::plain(spec::Check::Assert {
            path: None,
            each: Some(each),
            operator,
            expected,
        })
    }

    /// Compare the spans `span_selector` selects in the trace slot `on`.
    ///
    /// # Errors
    /// Returns [`WyrdError::Validation`] for an invalid JSONPath selector.
    pub fn trace(
        on: Reference,
        span_selector: &str,
        operator: ComparisonOperator,
        expected: Value,
    ) -> Result<Self, WyrdError> {
        Ok(Self::plain(spec::Check::Trace(TraceCheck {
            on,
            span_selector: JsonPath::new(span_selector)?,
            operator,
            expected,
        })))
    }

    /// Judge with an Agent whose Prompt returns a verdict.
    #[must_use]
    pub fn agent(call: AgentCall) -> Self {
        let (call, dependencies) = call.into_parts();
        Self {
            check: spec::Check::Agent(call),
            dependencies,
        }
    }

    /// Judge with code returning a verdict.
    #[must_use]
    pub fn code(call: CodeCall) -> Self {
        Self::code_check(call, None)
    }

    /// Judge with code returning `format`, such as a named verdict format
    /// with a custom data schema.
    #[must_use]
    pub fn code_with_format(call: CodeCall, format: skald_spec::NamedResponseFormat) -> Self {
        Self::code_check(call, Some(format))
    }

    /// Judge the samples slot `on` for drift by `spec`.
    #[must_use]
    pub fn drift(on: Reference, spec: DriftSpec) -> Self {
        Self::plain(spec::Check::Drift(DriftCheck { on, spec }))
    }

    /// The canonical check.
    #[must_use]
    pub fn check(&self) -> &spec::Check {
        &self.check
    }

    /// A check referencing no definitions.
    fn plain(check: spec::Check) -> Self {
        Self {
            check,
            dependencies: Dependencies::default(),
        }
    }

    /// A code check returning `response_format`.
    fn code_check(
        call: CodeCall,
        response_format: Option<skald_spec::NamedResponseFormat>,
    ) -> Self {
        Self {
            check: spec::Check::Code(CodeCheck {
                call: call.call,
                response_format,
            }),
            dependencies: call.dependencies,
        }
    }
}

/// One Agent invocation, in a step action or a requirement check.
#[derive(Debug, Clone, PartialEq)]
pub struct AgentCall {
    /// The canonical call.
    call: spec::AgentCall,
    /// The Agent definition the call references.
    dependencies: Dependencies,
    /// The local Artifact each workspace destination currently mounts; a
    /// replaced destination drops its entry.
    workspace: BTreeMap<String, Dependencies>,
}

impl AgentCall {
    /// Invoke `agent`: an Agent definition declared in the same graph, a
    /// registered Agent's [`wyrd_spec::reference::CardRef`], or an inline
    /// [`AgentSpec`].
    #[must_use]
    pub fn new(agent: impl Into<Linked<AgentSpec>>) -> Self {
        let agent = agent.into();
        Self {
            call: spec::AgentCall {
                agent: agent.reference,
                inputs: BTreeMap::new(),
                workspace: BTreeMap::new(),
            },
            dependencies: agent.dependencies,
            workspace: BTreeMap::new(),
        }
    }

    /// Bind the Prompt variable `parameter` to `value`.
    ///
    /// # Errors
    /// Returns [`WyrdError::Validation`] for an invalid parameter name.
    pub fn add_input(&mut self, parameter: &str, value: Reference) -> Result<(), WyrdError> {
        self.call
            .inputs
            .insert(ParameterName::new(parameter)?, value);
        Ok(())
    }

    /// Mount the value of `value` as JSON at workspace `path`, replacing
    /// whatever `path` mounted.
    pub fn add_workspace_input(&mut self, path: &str, value: Reference) {
        self.call
            .workspace
            .insert(path.to_owned(), WorkspaceSource::Scope(value));
        self.workspace.remove(path);
    }

    /// Mount the fixed `file` of an exact Artifact version at workspace
    /// `path`, replacing whatever `path` mounted; a local Artifact is
    /// published before the graph.
    pub fn add_workspace_file(&mut self, path: &str, file: &ArtifactFile) {
        self.call.workspace.insert(
            path.to_owned(),
            WorkspaceSource::Artifact {
                artifact: file.selection(),
            },
        );
        self.workspace.insert(path.to_owned(), file.dependencies());
    }

    /// The canonical call.
    #[must_use]
    pub fn call(&self) -> &spec::AgentCall {
        &self.call
    }

    /// The canonical call and every definition and local Artifact its
    /// current Agent and workspace mounts reference, in workspace path order.
    fn into_parts(self) -> (spec::AgentCall, Dependencies) {
        let mut dependencies = self.dependencies;
        for mounted in self.workspace.into_values() {
            dependencies.absorb(mounted);
        }
        (self.call, dependencies)
    }
}

/// One sandboxed code invocation, in a step action or a requirement check.
#[derive(Debug, Clone, PartialEq)]
pub struct CodeCall {
    /// The canonical call.
    call: spec::CodeCall,
    /// The local Artifact holding the source, if any.
    dependencies: Dependencies,
}

impl CodeCall {
    /// Run `entrypoint` of the Python `source` embedded in the Verifier.
    #[must_use]
    pub fn inline(source: &str, entrypoint: &str) -> Self {
        Self::with_source(
            CodeSource::Inline(source.to_owned()),
            entrypoint,
            Dependencies::default(),
        )
    }

    /// Run `entrypoint` of the UTF-8 Python file at `path`, read now and
    /// embedded in the Verifier; later edits to the file change nothing.
    ///
    /// # Errors
    /// Returns [`WyrdError::Validation`] with rule `code_source` when the
    /// file cannot be read as UTF-8 text.
    pub fn from_file(path: impl AsRef<Path>, entrypoint: &str) -> Result<Self, WyrdError> {
        let path = path.as_ref();
        let source = std::fs::read_to_string(path)
            .map_err(|error| invalid("code_source", &path.display().to_string(), error))?;
        Ok(Self::inline(&source, entrypoint))
    }

    /// Run `entrypoint` of the fixed `file` of an exact Artifact version; a
    /// local Artifact is published before the graph.
    #[must_use]
    pub fn artifact(file: &ArtifactFile, entrypoint: &str) -> Self {
        let selection = file.selection();
        Self::with_source(
            CodeSource::Artifact {
                card: selection.card,
                path: selection.path,
            },
            entrypoint,
            file.dependencies(),
        )
    }

    /// Pass `value` as the keyword argument `parameter`.
    ///
    /// # Errors
    /// Returns [`WyrdError::Validation`] for an invalid parameter name.
    pub fn add_input(&mut self, parameter: &str, value: Reference) -> Result<(), WyrdError> {
        self.call
            .inputs
            .insert(ParameterName::new(parameter)?, value);
        Ok(())
    }

    /// Run under `runtime`.
    pub fn set_runtime(&mut self, runtime: CodeRuntime) {
        self.call.runtime = runtime;
    }

    /// The canonical call.
    #[must_use]
    pub fn call(&self) -> &spec::CodeCall {
        &self.call
    }

    /// A Python call of `entrypoint` in `source`.
    fn with_source(source: CodeSource, entrypoint: &str, dependencies: Dependencies) -> Self {
        Self {
            call: spec::CodeCall {
                runtime: CodeRuntime::Python,
                source,
                entrypoint: entrypoint.to_owned(),
                inputs: BTreeMap::new(),
            },
            dependencies,
        }
    }
}
