//! Workflow Card spec and portable Workflow run contracts.

use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::fmt;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::json;
use thiserror::Error;

use crate::api_version::ApiVersion;
use crate::auth::AbsoluteUrl;
use crate::card::agent::AgentSpec;
use crate::card::common::{Governance, NonSecretValue, ObservationHooks, ParameterValue};
use crate::card::prompt::is_valid_parameter_name;
use crate::envelope::{Card, CardKind, Metadata as EnvelopeMetadata, Relationships, Spec};
use crate::error::WyrdError;
use crate::gateway::GatewayFallbackOverride;
use crate::ids::{CardName, CardUid, CredentialBindingName, SpaceName, WorkflowRunId};
use crate::metadata::{Annotations, Labels};
use crate::reference::{CardRef, InlineableRef, Ref};
use crate::security::SecretRef;
use wyrd_semver::VersionBlock;

/// Maximum JCS UTF-8 size of one projected run or step error.
///
/// Terminal snapshot capacity reserves this many bytes for the run error and
/// for one error per declared step, so a terminal transition never lacks room
/// for its diagnostics.
pub const WORKFLOW_RUN_ERROR_MAX_BYTES: usize = 2 * 1024;

/// Declarative workflow definition.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct WorkflowSpec {
    /// Workflow description.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Declared workflow inputs and their native default values.
    ///
    /// Invocation input may override a default only with a value of the same
    /// `ParameterValue` variant; unknown invocation keys are rejected.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub inputs: BTreeMap<String, ParameterValue>,
    /// Workflow default model-call route; absent means [`LlmRoute::Native`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub llm_route: Option<LlmRoute>,
    /// Workflow steps.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub steps: Vec<WorkflowStep>,
    /// Named workflow outputs, each selecting one exact binding source.
    ///
    /// A succeeded run resolves every declared output; validation rejects an
    /// empty map.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub outputs: BTreeMap<String, WorkflowBinding>,
    /// Governance and audit controls.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub governance: Option<Governance>,
    /// Observation hooks.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub observation_hooks: Option<ObservationHooks>,
    /// Free-form details.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub details: BTreeMap<String, NonSecretValue>,
}

impl WorkflowSpec {
    /// Validate the declared Workflow graph, bindings, outputs, and routes.
    ///
    /// The check is pure and iterative, so arbitrarily deep graphs cannot
    /// exhaust the stack. It rejects an empty step list; invalid, empty, or
    /// duplicate step IDs; missing, self, or duplicate dependencies; cycles;
    /// invalid input and output names; step bindings that reference an
    /// undeclared input, the step itself, or a step outside its declared
    /// transitive dependencies; an empty output map or an output bound to an
    /// undeclared input or step; malformed external-gateway routes; a
    /// `max_retries` of `u32::MAX`; and a step fallback whose resolved route
    /// is not [`LlmRoute::WyrdGateway`].
    /// Cross-Card checks that need the resolved Agent Prompt (variable
    /// coverage and route dialect) belong to the runtime that resolved it.
    ///
    /// # Errors
    /// Returns a [`WorkflowValidationError`] naming the first offending field.
    pub fn validate(&self) -> Result<(), WorkflowValidationError> {
        if self.steps.is_empty() {
            return Err(WorkflowValidationError::invalid(
                "steps",
                "a workflow must declare at least one step",
            ));
        }
        for name in self.inputs.keys() {
            require_identifier(&format!("inputs.{name}"), name)?;
        }
        if let Some(route) = &self.llm_route {
            route.validate("llm_route")?;
        }
        let index = self.step_index()?;
        self.require_acyclic(&index)?;
        for (position, step) in self.steps.iter().enumerate() {
            self.validate_step(&index, position, step)?;
        }
        self.validate_outputs(&index)
    }

    /// Return the route a step resolves to.
    ///
    /// Resolution is `step.llm_route -> workflow.llm_route -> Native`.
    #[must_use]
    pub fn resolved_route<'a>(&'a self, step: &'a WorkflowStep) -> &'a LlmRoute {
        step.llm_route
            .as_ref()
            .or(self.llm_route.as_ref())
            .unwrap_or(&LlmRoute::Native)
    }

    /// Map step IDs to their declaration position, rejecting invalid or
    /// duplicate IDs.
    ///
    /// # Errors
    /// Returns a validation error for the first invalid or repeated ID.
    fn step_index(&self) -> Result<BTreeMap<&str, usize>, WorkflowValidationError> {
        let mut index = BTreeMap::new();
        for (position, step) in self.steps.iter().enumerate() {
            let field = format!("steps[{position}].id");
            require_identifier(&field, &step.id)?;
            if index.insert(step.id.as_str(), position).is_some() {
                return Err(WorkflowValidationError::DuplicateStep { field });
            }
        }
        for (position, step) in self.steps.iter().enumerate() {
            let mut seen = BTreeSet::new();
            for dependency in &step.depends_on {
                let field = format!("steps[{position}].depends_on");
                if dependency == &step.id {
                    return Err(WorkflowValidationError::invalid(
                        &field,
                        "a step cannot depend on itself",
                    ));
                }
                if !index.contains_key(dependency.as_str()) {
                    return Err(WorkflowValidationError::MissingDependency { field });
                }
                if !seen.insert(dependency.as_str()) {
                    return Err(WorkflowValidationError::invalid(
                        &field,
                        "a dependency may be declared only once",
                    ));
                }
            }
        }
        Ok(index)
    }

    /// Reject dependency cycles using Kahn's algorithm.
    ///
    /// # Errors
    /// Returns [`WorkflowValidationError::Cycle`] when some steps can never
    /// become ready.
    fn require_acyclic(
        &self,
        index: &BTreeMap<&str, usize>,
    ) -> Result<(), WorkflowValidationError> {
        let mut remaining: Vec<usize> = self.steps.iter().map(|s| s.depends_on.len()).collect();
        let mut dependents: Vec<Vec<usize>> = vec![Vec::new(); self.steps.len()];
        for (position, step) in self.steps.iter().enumerate() {
            for dependency in &step.depends_on {
                if let Some(&parent) = index.get(dependency.as_str()) {
                    dependents[parent].push(position);
                }
            }
        }
        let mut ready: Vec<usize> = (0..self.steps.len())
            .filter(|&position| remaining[position] == 0)
            .collect();
        let mut ordered = 0;
        while let Some(position) = ready.pop() {
            ordered += 1;
            for &child in &dependents[position] {
                remaining[child] -= 1;
                if remaining[child] == 0 {
                    ready.push(child);
                }
            }
        }
        if ordered != self.steps.len() {
            return Err(WorkflowValidationError::Cycle {
                field: "steps".to_owned(),
            });
        }
        Ok(())
    }

    /// Return true when `target` is a declared transitive dependency of the
    /// step at `position`.
    ///
    /// Walks `depends_on` edges with an explicit stack, so depth cannot exhaust
    /// the call stack; each step is visited at most once.
    fn reaches(&self, index: &BTreeMap<&str, usize>, position: usize, target: &str) -> bool {
        let mut visited = vec![false; self.steps.len()];
        let mut stack = vec![position];
        while let Some(current) = stack.pop() {
            for dependency in &self.steps[current].depends_on {
                if dependency == target {
                    return true;
                }
                if let Some(&parent) = index.get(dependency.as_str())
                    && !visited[parent]
                {
                    visited[parent] = true;
                    stack.push(parent);
                }
            }
        }
        false
    }

    /// Validate one step's input bindings, route, retry bound, and fallback.
    ///
    /// # Errors
    /// Returns a validation error for an invalid binding name, a binding that
    /// references an undeclared input or an invisible step, an invalid route,
    /// a `max_retries` of `u32::MAX` (its final attempt count cannot fit the
    /// `u32` attempts field), or a fallback on a non-WyrdGateway route.
    fn validate_step(
        &self,
        index: &BTreeMap<&str, usize>,
        position: usize,
        step: &WorkflowStep,
    ) -> Result<(), WorkflowValidationError> {
        for (name, binding) in &step.inputs {
            let field = format!("steps[{position}].inputs.{name}");
            require_identifier(&field, name)?;
            match binding.source() {
                WorkflowBindingSource::Input(input) => self.require_input(&field, input)?,
                WorkflowBindingSource::StepText(step_id)
                | WorkflowBindingSource::StepStructured { step: step_id, .. } => {
                    if !self.reaches(index, position, step_id) {
                        return Err(WorkflowValidationError::invalid(
                            &field,
                            "a step may bind only its declared transitive dependencies",
                        ));
                    }
                }
            }
        }
        if let Some(route) = &step.llm_route {
            route.validate(&format!("steps[{position}].llm_route"))?;
        }
        if step
            .retry
            .as_ref()
            .is_some_and(|retry| retry.max_retries == u32::MAX)
        {
            return Err(WorkflowValidationError::invalid(
                &format!("steps[{position}].retry.max_retries"),
                "must be below 4294967295 so every attempt count fits a u32",
            ));
        }
        if let Some(fallback) = &step.fallback {
            let field = format!("steps[{position}].fallback");
            if self.resolved_route(step) != &LlmRoute::WyrdGateway {
                return Err(WorkflowValidationError::invalid(
                    &field,
                    "fallback is valid only for the wyrd_gateway route",
                ));
            }
            fallback
                .validate()
                .map_err(|error| WorkflowValidationError::invalid(&field, &error.to_string()))?;
        }
        Ok(())
    }

    /// Validate the nonempty output projection.
    ///
    /// # Errors
    /// Returns a validation error for an empty map, an invalid output name, or
    /// a binding to an undeclared input or step.
    fn validate_outputs(
        &self,
        index: &BTreeMap<&str, usize>,
    ) -> Result<(), WorkflowValidationError> {
        if self.outputs.is_empty() {
            return Err(WorkflowValidationError::invalid(
                "outputs",
                "a workflow must declare at least one output",
            ));
        }
        for (name, binding) in &self.outputs {
            let field = format!("outputs.{name}");
            require_identifier(&field, name)?;
            match binding.source() {
                WorkflowBindingSource::Input(input) => self.require_input(&field, input)?,
                WorkflowBindingSource::StepText(step_id)
                | WorkflowBindingSource::StepStructured { step: step_id, .. } => {
                    if !index.contains_key(step_id) {
                        return Err(WorkflowValidationError::invalid(
                            &field,
                            "an output may select only a declared step",
                        ));
                    }
                }
            }
        }
        Ok(())
    }

    /// Require that `input` names a declared workflow input.
    ///
    /// # Errors
    /// Returns a validation error naming `field` when it is undeclared.
    fn require_input(&self, field: &str, input: &str) -> Result<(), WorkflowValidationError> {
        if self.inputs.contains_key(input) {
            Ok(())
        } else {
            Err(WorkflowValidationError::invalid(
                field,
                "binding references an undeclared workflow input",
            ))
        }
    }
}

/// Require the shared parameter identifier grammar `[A-Za-z_][A-Za-z0-9_]*`.
///
/// # Errors
/// Returns a validation error naming `field` when `value` does not match.
fn require_identifier(field: &str, value: &str) -> Result<(), WorkflowValidationError> {
    if is_valid_parameter_name(value) {
        Ok(())
    } else {
        Err(WorkflowValidationError::invalid(
            field,
            "must match [A-Za-z_][A-Za-z0-9_]*",
        ))
    }
}

/// One workflow step.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct WorkflowStep {
    /// Stable step ID using the parameter identifier grammar.
    pub id: String,
    /// Step action.
    pub action: WorkflowAction,
    /// Dependency step IDs. Edges order execution but inject no data.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub depends_on: Vec<String>,
    /// Prompt variable bindings, keyed by unresolved Prompt variable name.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub inputs: BTreeMap<String, WorkflowBinding>,
    /// Step route overriding the workflow default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub llm_route: Option<LlmRoute>,
    /// Ordered gateway fallback, valid only for the WyrdGateway route.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fallback: Option<GatewayFallbackOverride>,
    /// Per-attempt timeout in seconds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_seconds: Option<u64>,
    /// Retry policy.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retry: Option<WorkflowRetryPolicy>,
    /// Display metadata.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub display: BTreeMap<String, NonSecretValue>,
}

/// Workflow retry policy.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct WorkflowRetryPolicy {
    /// Additional Workflow attempts after the first.
    pub max_retries: u32,
    /// Initial backoff in milliseconds; absent or zero retries immediately.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub initial_backoff_ms: Option<u64>,
}

/// Workflow action target. Agent steps are the only executable action.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(tag = "type", content = "target", rename_all = "snake_case")]
pub enum WorkflowAction {
    /// Agent action — inline body or Agent Card reference.
    Agent(InlineableRef<AgentSpec>),
}

/// One exact Workflow binding source path.
///
/// The wire form is a single string with one of the roots `input.<name>`,
/// `steps.<step_id>.output.text`, `steps.<step_id>.output.structured`, or
/// `steps.<step_id>.output.structured.<field>...`. Every component uses the
/// parameter identifier grammar; whitespace, templates, expressions, multiple
/// sources, and array indexing are rejected at construction.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(transparent)]
pub struct WorkflowBinding(String);

/// Parsed view of a [`WorkflowBinding`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkflowBindingSource<'a> {
    /// `input.<name>`: a declared workflow input.
    Input(&'a str),
    /// `steps.<step_id>.output.text`: a step's text output.
    StepText(&'a str),
    /// `steps.<step_id>.output.structured[.<field>...]`: a step's structured
    /// output or a nested object field within it.
    StepStructured {
        /// Selected step ID.
        step: &'a str,
        /// Dot-separated nested object field path, empty for the whole value.
        path: &'a str,
    },
}

impl WorkflowBinding {
    /// Parse and validate one binding source path.
    ///
    /// # Errors
    /// Returns [`WorkflowValidationError::Invalid`] for field `binding` when
    /// the value is not exactly one supported root with valid components.
    pub fn new(value: impl Into<String>) -> Result<Self, WorkflowValidationError> {
        let value = value.into();
        if parse_binding(&value).is_none() {
            return Err(WorkflowValidationError::invalid(
                "binding",
                "must be input.<name>, steps.<id>.output.text, or steps.<id>.output.structured[.<field>...]",
            ));
        }
        Ok(Self(value))
    }

    /// Borrow the canonical wire string.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Return the parsed source this binding selects.
    ///
    /// # Panics
    /// Never panics in practice: every constructor validated the grammar.
    #[must_use]
    pub fn source(&self) -> WorkflowBindingSource<'_> {
        parse_binding(&self.0).expect("WorkflowBinding was validated at construction")
    }
}

impl fmt::Display for WorkflowBinding {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::str::FromStr for WorkflowBinding {
    type Err = WorkflowValidationError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::new(value)
    }
}

impl<'de> Deserialize<'de> for WorkflowBinding {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        Self::new(value).map_err(serde::de::Error::custom)
    }
}

/// Parse a binding string into its source, or `None` when malformed.
fn parse_binding(value: &str) -> Option<WorkflowBindingSource<'_>> {
    if let Some(name) = value.strip_prefix("input.") {
        return is_valid_parameter_name(name).then_some(WorkflowBindingSource::Input(name));
    }
    let rest = value.strip_prefix("steps.")?;
    let (step, rest) = rest.split_once('.')?;
    if !is_valid_parameter_name(step) {
        return None;
    }
    if rest == "output.text" {
        return Some(WorkflowBindingSource::StepText(step));
    }
    if rest == "output.structured" {
        return Some(WorkflowBindingSource::StepStructured { step, path: "" });
    }
    let path = rest.strip_prefix("output.structured.")?;
    path.split('.')
        .all(is_valid_parameter_name)
        .then_some(WorkflowBindingSource::StepStructured { step, path })
}

/// Workflow model-call route.
///
/// Resolution is `step.llm_route -> workflow.llm_route -> Native`. The Prompt
/// remains the sole owner of provider, model, request fields, and response
/// shape; the route only selects the execution boundary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum LlmRoute {
    /// Call the native provider declared by the Agent Prompt.
    Native,
    /// Call the execution environment's governed Wyrd gateway.
    WyrdGateway,
    /// Call a user-provided gateway with a named execution-environment binding.
    ExtGateway {
        /// Request dialect the gateway accepts; must match the Prompt request.
        protocol: ExternalGatewayProtocol,
        /// Absolute gateway base URL without userinfo, query, or fragment.
        base_url: AbsoluteUrl,
        /// Non-secret request headers; names are case-insensitively unique.
        #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
        headers: BTreeMap<String, String>,
        /// Execution-environment binding that authorizes the origin and
        /// supplies secret headers.
        credential_binding: CredentialBindingName,
    },
}

impl LlmRoute {
    /// Validate the declarative route contract.
    ///
    /// Only [`LlmRoute::ExtGateway`] carries checkable fields: its base URL must
    /// have no userinfo, query, or fragment, and its headers must be valid,
    /// case-insensitively unique, non-forbidden HTTP field names with values
    /// free of control characters. HTTPS enforcement depends on the execution
    /// profile and happens at runtime.
    ///
    /// # Errors
    /// Returns a validation error naming the offending field below `field`.
    pub fn validate(&self, field: &str) -> Result<(), WorkflowValidationError> {
        let Self::ExtGateway {
            base_url, headers, ..
        } = self
        else {
            return Ok(());
        };
        let parsed = url::Url::parse(base_url.as_str()).map_err(|error| {
            WorkflowValidationError::invalid(&format!("{field}.base_url"), &error.to_string())
        })?;
        if !parsed.username().is_empty()
            || parsed.password().is_some()
            || parsed.query().is_some()
            || parsed.fragment().is_some()
        {
            return Err(WorkflowValidationError::invalid(
                &format!("{field}.base_url"),
                "must not contain userinfo, a query, or a fragment",
            ));
        }
        let mut seen = BTreeSet::new();
        for (name, value) in headers {
            let header_field = format!("{field}.headers.{name}");
            let lower = name.to_ascii_lowercase();
            if !is_http_token(name) {
                return Err(WorkflowValidationError::invalid(
                    &header_field,
                    "is not a valid HTTP field name",
                ));
            }
            if is_forbidden_route_header(&lower) {
                return Err(WorkflowValidationError::invalid(
                    &header_field,
                    "is reserved and cannot be authored in a route",
                ));
            }
            if !seen.insert(lower) {
                return Err(WorkflowValidationError::invalid(
                    &header_field,
                    "header names must be case-insensitively unique",
                ));
            }
            if value.chars().any(|ch| ch.is_control() && ch != '\t') {
                return Err(WorkflowValidationError::invalid(
                    &header_field,
                    "header values must not contain control characters",
                ));
            }
        }
        Ok(())
    }
}

/// Return true for a non-empty RFC 9110 `token` field name.
fn is_http_token(name: &str) -> bool {
    !name.is_empty()
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&byte))
}

/// Return true when a lowercase header name may not appear in a Card route.
///
/// These names either control transport, routing, or Wyrd-internal behavior
/// ([`is_reserved_transport_header`]), or carry credentials that belong only
/// to an execution-environment binding.
#[must_use]
pub fn is_forbidden_route_header(lower: &str) -> bool {
    is_reserved_transport_header(lower)
        || lower == "authorization"
        || lower.contains("api-key")
        || lower.contains("token")
}

/// Return true when a lowercase header name controls transport framing,
/// virtual-host routing, forwarding or proxying, or carries Wyrd-internal
/// identity.
///
/// No Card route or execution-environment binding may set these names; a
/// binding may still supply the credential names a Card route may not.
#[must_use]
pub fn is_reserved_transport_header(lower: &str) -> bool {
    matches!(
        lower,
        "host"
            | "content-length"
            | "connection"
            | "transfer-encoding"
            | "te"
            | "trailer"
            | "upgrade"
            | "forwarded"
            | "x-wyrd-access-token"
            | "wyrd-request-id"
    ) || lower.starts_with("x-forwarded-")
        || lower.starts_with("proxy-")
}

/// Request dialect accepted by an external gateway.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub enum ExternalGatewayProtocol {
    /// OpenAI Chat Completions.
    #[serde(rename = "openai_chat")]
    OpenAiChat,
    /// OpenAI Responses.
    #[serde(rename = "openai_responses")]
    OpenAiResponses,
    /// Anthropic Messages.
    #[serde(rename = "anthropic_messages")]
    AnthropicMessages,
    /// Google Gemini GenerateContent.
    #[serde(rename = "gemini_generate_content")]
    GeminiGenerateContent,
    /// Google Vertex GenerateContent.
    #[serde(rename = "vertex_generate_content")]
    VertexGenerateContent,
}

/// Shared pure configuration for one external-gateway credential binding.
///
/// Local and server configuration owners wrap this shape. It carries secret
/// references, never plaintext secret values.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExternalGatewayBindingConfig {
    /// Protocol the binding authorizes.
    pub protocol: ExternalGatewayProtocol,
    /// Exact permitted endpoint origin.
    pub origin: url::Url,
    /// Secret request headers resolved from secret references.
    #[serde(default)]
    pub secret_headers: BTreeMap<String, SecretRef>,
}

/// Lifecycle status of a Workflow run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum WorkflowRunStatus {
    /// Accepted but not started.
    Queued,
    /// Executing steps.
    Running,
    /// Every step succeeded and every output resolved.
    Succeeded,
    /// A step or run-level failure ended the run.
    Failed,
    /// The caller cancelled the run.
    Cancelled,
    /// The total run deadline expired.
    TimedOut,
}

impl WorkflowRunStatus {
    /// Return true for the four terminal statuses.
    #[must_use]
    pub const fn is_terminal(self) -> bool {
        !matches!(self, Self::Queued | Self::Running)
    }
}

/// Lifecycle status of one Workflow step.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum WorkflowStepStatus {
    /// Waiting for dependencies or a concurrency slot.
    Pending,
    /// An attempt is executing.
    Running,
    /// The step produced its output.
    Succeeded,
    /// The step failed terminally.
    Failed,
    /// An active step was interrupted by cancellation or the run deadline.
    Cancelled,
    /// The run ended before the step started.
    Unstarted,
}

/// Terminal-snapshot projection of one derive-backed Wyrd error.
///
/// This is not a second catalog: `code` and `remediation` come from the
/// [`WyrdError`] catalog and `details` carries only safe fields.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct WorkflowRunError {
    /// Stable Wyrd error code.
    pub code: String,
    /// Safe diagnostic message.
    pub message: String,
    /// Safe structured details.
    #[serde(default)]
    pub details: serde_json::Value,
    /// Operator-facing remediation.
    pub remediation: String,
}

impl WorkflowRunError {
    /// Project a catalog error, bounded to [`WORKFLOW_RUN_ERROR_MAX_BYTES`].
    #[must_use]
    pub fn from_wyrd(error: &WyrdError) -> Self {
        let problem = error.as_problem_json();
        Self {
            code: error.code().to_owned(),
            message: problem["detail"].as_str().unwrap_or_default().to_owned(),
            details: problem["details"].clone(),
            remediation: error.remediation().to_owned(),
        }
        .bounded()
    }

    /// Bound this error's JCS UTF-8 size to [`WORKFLOW_RUN_ERROR_MAX_BYTES`].
    ///
    /// The stable code and remediation are always preserved. Oversized
    /// details are dropped first, then the message is shortened on a UTF-8
    /// character boundary until the projection fits.
    #[must_use]
    pub fn bounded(mut self) -> Self {
        if jcs_len(&self) <= WORKFLOW_RUN_ERROR_MAX_BYTES {
            return self;
        }
        self.details = json!({});
        while jcs_len(&self) > WORKFLOW_RUN_ERROR_MAX_BYTES && !self.message.is_empty() {
            let excess = jcs_len(&self) - WORKFLOW_RUN_ERROR_MAX_BYTES;
            let mut keep = self.message.len().saturating_sub(excess.max(1));
            while !self.message.is_char_boundary(keep) {
                keep -= 1;
            }
            self.message.truncate(keep);
        }
        self
    }
}

/// Portable result of one Workflow step.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct WorkflowStepResult {
    /// Step lifecycle status.
    pub status: WorkflowStepStatus,
    /// Final text output, retained only for a succeeded step.
    pub text: Option<String>,
    /// Structured output, retained only for a succeeded step.
    pub structured_output: Option<serde_json::Value>,
    /// Number of attempts begun.
    pub attempts: u32,
    /// First attempt start time.
    pub started_at: Option<DateTime<Utc>>,
    /// Terminal time.
    pub ended_at: Option<DateTime<Utc>>,
    /// Terminal error, present only for a failed step.
    pub error: Option<WorkflowRunError>,
}

impl WorkflowStepResult {
    /// Build the initial pending result.
    #[must_use]
    pub const fn pending() -> Self {
        Self {
            status: WorkflowStepStatus::Pending,
            text: None,
            structured_output: None,
            attempts: 0,
            started_at: None,
            ended_at: None,
            error: None,
        }
    }
}

/// Portable Workflow run snapshot shared by local and server execution.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct WorkflowRun {
    /// Run identity.
    pub run_id: WorkflowRunId,
    /// Pinned Workflow Card, absent only for an unregistered local run.
    pub workflow: Option<CardRef>,
    /// Run lifecycle status.
    pub status: WorkflowRunStatus,
    /// Declared outputs, populated only for a succeeded run.
    #[serde(default)]
    pub outputs: BTreeMap<String, serde_json::Value>,
    /// Step results keyed by step ID.
    #[serde(default)]
    pub steps: BTreeMap<String, WorkflowStepResult>,
    /// Creation time.
    pub created_at: DateTime<Utc>,
    /// Start time.
    pub started_at: Option<DateTime<Utc>>,
    /// Terminal time.
    pub ended_at: Option<DateTime<Utc>>,
    /// Primary run error for `failed` and `timed_out` runs.
    pub error: Option<WorkflowRunError>,
}

impl WorkflowRun {
    /// Return the JCS UTF-8 size used for snapshot budget accounting.
    #[must_use]
    pub fn canonical_len(&self) -> usize {
        jcs_len(self)
    }
}

/// Request body for `POST /v1/workflow-runs`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct CreateWorkflowRunRequest {
    /// Exact registered Workflow Card.
    pub workflow: CardRef,
    /// Invocation input object.
    #[serde(default)]
    pub input: BTreeMap<String, serde_json::Value>,
    /// Requested total timeout, bounded by server configuration.
    pub timeout_seconds: Option<u64>,
}

/// Return the JCS UTF-8 serialized size of `value`.
///
/// Serialization of these plain data types cannot fail; a failure would be an
/// invariant violation and is charged as `usize::MAX` so it can never fit a
/// budget.
#[must_use]
pub fn jcs_len<T: Serialize + ?Sized>(value: &T) -> usize {
    serde_jcs::to_vec(value).map_or(usize::MAX, |bytes| bytes.len())
}

/// Workflow validation failures, each naming the offending field.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum WorkflowValidationError {
    /// Duplicate step ID.
    #[error("{field}: workflow contains a duplicate step id")]
    DuplicateStep {
        /// Offending field path.
        field: String,
    },
    /// Missing dependency.
    #[error("{field}: workflow references a missing dependency")]
    MissingDependency {
        /// Offending field path.
        field: String,
    },
    /// Workflow graph contains a cycle.
    #[error("{field}: workflow contains a dependency cycle")]
    Cycle {
        /// Offending field path.
        field: String,
    },
    /// Any other contract violation.
    #[error("{field}: {reason}")]
    Invalid {
        /// Offending field path.
        field: String,
        /// Safe reason text.
        reason: String,
    },
}

impl WorkflowValidationError {
    /// Build an [`WorkflowValidationError::Invalid`] error.
    #[must_use]
    pub fn invalid(field: &str, reason: &str) -> Self {
        Self::Invalid {
            field: field.to_owned(),
            reason: reason.to_owned(),
        }
    }

    /// Return the offending field path.
    #[must_use]
    pub fn field(&self) -> &str {
        match self {
            Self::DuplicateStep { field }
            | Self::MissingDependency { field }
            | Self::Cycle { field }
            | Self::Invalid { field, .. } => field,
        }
    }
}

/// Local typed holder for a Wyrd Workflow Card envelope.
#[derive(Debug, Clone, PartialEq)]
pub struct WorkflowCard {
    /// Logical card space.
    pub space: String,
    /// User-authored card name.
    pub name: String,
    /// Exact card version.
    pub version: String,
    /// Server-assigned card UID, empty before registration.
    pub uid: String,
    /// Queryable labels.
    pub labels: Labels,
    /// Free-form annotations.
    pub annotations: Annotations,
    /// Workflow Card spec body.
    pub spec: WorkflowSpec,
    /// Derived cascade children (Agent, Prompt, governance, and route refs).
    pub cascade_children: Vec<CardRef>,
    /// Local creation timestamp.
    pub created_at: DateTime<Utc>,
}

impl WorkflowCard {
    /// Convert this typed holder into the shared Wyrd `Card` envelope.
    ///
    /// # Errors
    /// Returns validation errors for invalid identity fields.
    pub fn to_envelope(&self) -> Result<Card, WyrdError> {
        Ok(Card {
            api_version: ApiVersion::v1(),
            kind: CardKind::Workflow,
            metadata: EnvelopeMetadata {
                name: card_name("metadata.name", &self.name)?,
                version: Some(version_block("metadata.version", &self.version)?.into()),
                bump: None,
                space: Some(space_name(&self.space)?),
                uid: optional_card_uid(&self.uid)?,
                labels: self.labels.clone(),
                annotations: self.annotations.clone(),
                spec_hash: None,
                artifact_hash: None,
                origin: None,
            },
            spec: Spec::Workflow(self.spec.clone()),
            relationships: Relationships::default(),
            status: None,
        })
    }

    /// Convert a shared Wyrd `Card` envelope into a typed Workflow Card holder.
    ///
    /// # Errors
    /// Returns validation errors when the envelope is not a Workflow Card.
    pub fn from_envelope(card: Card) -> Result<Self, WyrdError> {
        if card.api_version.as_str() != ApiVersion::V1 {
            return Err(WorkflowCardError::validation(format!(
                "expected apiVersion wyrd/v1, got {}",
                card.api_version
            ))
            .into());
        }
        if card.kind != CardKind::Workflow {
            return Err(WorkflowCardError::validation(format!(
                "expected kind Workflow, got {}",
                card.kind.wire_name()
            ))
            .into());
        }

        let Spec::Workflow(spec) = card.spec else {
            return Err(WorkflowCardError::validation(
                "Workflow Card spec must be a Workflow spec",
            )
            .into());
        };

        let cascade_children = derive_cascade_children(&spec);
        Ok(Self {
            space: card
                .metadata
                .space
                .as_ref()
                .map_or_else(|| "default".to_owned(), ToString::to_string),
            name: card.metadata.name.to_string(),
            version: card
                .metadata
                .resolved_pin()
                .map(ToString::to_string)
                .ok_or_else(|| {
                    WorkflowCardError::validation(
                        "Workflow Card envelope missing resolved version pin",
                    )
                })?,
            uid: card
                .metadata
                .uid
                .as_ref()
                .map(ToString::to_string)
                .unwrap_or_default(),
            labels: card.metadata.labels,
            annotations: card.metadata.annotations,
            spec,
            cascade_children,
            created_at: Utc::now(),
        })
    }

    /// Convert this Workflow Card identity into a `CardRef`.
    ///
    /// # Errors
    /// Returns validation errors for invalid identity fields.
    pub fn card_ref(&self) -> Result<CardRef, WyrdError> {
        Ok(CardRef {
            kind: CardKind::Workflow,
            name: card_name("metadata.name", &self.name)?,
            version: version_block("metadata.version", &self.version)?,
            space: Some(space_name(&self.space)?),
            uid: optional_card_uid(&self.uid)?,
        })
    }
}

impl Serialize for WorkflowCard {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut value =
            serde_yaml::to_value(self.to_envelope().map_err(serde::ser::Error::custom)?)
                .map_err(serde::ser::Error::custom)?;
        if let serde_yaml::Value::Mapping(mapping) = &mut value {
            mapping.insert(
                serde_yaml::Value::String("relationships".to_owned()),
                serde_yaml::Value::Mapping({
                    let mut relationships = serde_yaml::Mapping::new();
                    relationships.insert(
                        serde_yaml::Value::String("outbound".to_owned()),
                        serde_yaml::Value::Sequence(Vec::new()),
                    );
                    relationships
                }),
            );
            mapping.insert(
                serde_yaml::Value::String("status".to_owned()),
                serde_yaml::Value::Null,
            );
        }
        value.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for WorkflowCard {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let card = Card::deserialize(deserializer)?;
        Self::from_envelope(card).map_err(serde::de::Error::custom)
    }
}

/// Workflow Card local boundary errors.
#[derive(Debug, Error)]
pub enum WorkflowCardError {
    /// Workflow Card validation failed.
    #[error("WorkflowCard validation failed: {detail}")]
    Validation {
        /// Validation detail.
        detail: String,
    },
    /// Workflow Card name is required.
    #[error("WorkflowCard name is required before saving")]
    MissingName,
    /// Workflow Card version is required.
    #[error("WorkflowCard version is required before saving")]
    MissingVersion,
    /// Workflow Card filesystem IO failed.
    #[error("WorkflowCard IO failed at {path}: {message}")]
    Io {
        /// Path being read or written.
        path: String,
        /// IO detail.
        message: String,
    },
    /// Workflow Card YAML codec failed.
    #[error("WorkflowCard YAML codec failed: {message}")]
    Yaml {
        /// YAML detail.
        message: String,
    },
}

impl WorkflowCardError {
    /// Build a Workflow Card validation error.
    pub fn validation(detail: impl Into<String>) -> Self {
        Self::Validation {
            detail: detail.into(),
        }
    }

    /// Build a Workflow Card IO error.
    pub fn io(path: impl Into<String>, error: &std::io::Error) -> Self {
        Self::Io {
            path: path.into(),
            message: error.to_string(),
        }
    }

    /// Build a Workflow Card YAML error.
    pub fn yaml(error: &serde_yaml::Error) -> Self {
        Self::Yaml {
            message: error.to_string(),
        }
    }
}

impl From<WorkflowCardError> for WyrdError {
    fn from(error: WorkflowCardError) -> Self {
        match error {
            WorkflowCardError::Validation { detail } => WyrdError::WorkflowValidation {
                message: detail.clone(),
                details: json!({ "detail": detail }),
            },
            WorkflowCardError::MissingName => WyrdError::WorkflowMissingName {
                message: "WorkflowCard name is required before saving".to_owned(),
                details: json!({ "field": "metadata.name" }),
            },
            WorkflowCardError::MissingVersion => WyrdError::WorkflowMissingVersion {
                message: "WorkflowCard version is required before saving".to_owned(),
                details: json!({ "field": "metadata.version" }),
            },
            WorkflowCardError::Io { path, message } => WyrdError::WorkflowValidation {
                message: format!("WorkflowCard IO failed at {path}: {message}"),
                details: json!({ "path": path, "source": message }),
            },
            WorkflowCardError::Yaml { message } => WyrdError::WorkflowValidation {
                message: format!("WorkflowCard YAML codec failed: {message}"),
                details: json!({ "source": message }),
            },
        }
    }
}

impl From<WorkflowValidationError> for WyrdError {
    fn from(error: WorkflowValidationError) -> Self {
        let message = error.to_string();
        match error {
            WorkflowValidationError::DuplicateStep { field } => {
                WyrdError::WorkflowDuplicateStepId {
                    message,
                    details: json!({ "field": field }),
                }
            }
            WorkflowValidationError::MissingDependency { field } => {
                WyrdError::WorkflowMissingDependency {
                    message,
                    details: json!({ "field": field }),
                }
            }
            WorkflowValidationError::Cycle { field } => WyrdError::WorkflowCycle {
                message,
                details: json!({ "field": field }),
            },
            WorkflowValidationError::Invalid { field, reason } => WyrdError::WorkflowValidation {
                message,
                details: json!({ "field": field, "reason": reason }),
            },
        }
    }
}

fn derive_cascade_children(spec: &WorkflowSpec) -> Vec<CardRef> {
    let mut out: Vec<CardRef> = Vec::new();
    if let Some(governance) = &spec.governance {
        out.extend(
            governance
                .policy_refs
                .iter()
                .filter_map(Ref::as_card_ref)
                .cloned(),
        );
        out.extend(
            governance
                .audit_ref
                .as_ref()
                .and_then(Ref::as_card_ref)
                .cloned(),
        );
    }
    if let Some(observation_hooks) = &spec.observation_hooks {
        out.extend(
            observation_hooks
                .route_refs
                .iter()
                .filter_map(Ref::as_card_ref)
                .cloned(),
        );
    }
    for step in &spec.steps {
        let WorkflowAction::Agent(agent_ref) = &step.action;
        out.extend(agent_ref.as_card_ref().cloned());
        if let Some(agent_spec) = agent_ref.as_inline() {
            out.extend(agent_spec.prompt.as_card_ref().cloned());
        }
    }
    out.sort_by(|a, b| {
        let a_key = (
            a.kind.wire_name(),
            a.space.as_ref().map(SpaceName::as_str),
            a.name.as_str(),
            a.version.to_string(),
        );
        let b_key = (
            b.kind.wire_name(),
            b.space.as_ref().map(SpaceName::as_str),
            b.name.as_str(),
            b.version.to_string(),
        );
        a_key.cmp(&b_key)
    });
    out.dedup();
    out
}

fn card_name(field: &str, value: &str) -> Result<CardName, WyrdError> {
    CardName::new(value).map_err(|error| {
        WorkflowCardError::validation(format!("{field} must be a valid CardName: {error}")).into()
    })
}

fn version_block(field: &str, value: &str) -> Result<VersionBlock, WyrdError> {
    VersionBlock::parse(value).map_err(|error| {
        WorkflowCardError::validation(format!("{field} must be a semantic version: {error}")).into()
    })
}

fn space_name(value: &str) -> Result<SpaceName, WyrdError> {
    if value.is_empty() {
        return Err(WorkflowCardError::validation(
            "metadata.space is required and cannot be empty",
        )
        .into());
    }
    SpaceName::new(value).map_err(|error| {
        WorkflowCardError::validation(format!("metadata.space is invalid: {error}")).into()
    })
}

fn optional_card_uid(value: &str) -> Result<Option<CardUid>, WyrdError> {
    if value.is_empty() {
        return Ok(None);
    }
    CardUid::new(value).map(Some).map_err(|error| {
        WorkflowCardError::validation(format!("metadata.uid is invalid: {error}")).into()
    })
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::collections::BTreeSet;

    use crate::card::agent::{AgentRunConfigSpec, AgentSpec};
    use crate::card::common::{Governance, ObservationHooks};
    use serde_json::json;

    use crate::auth::AbsoluteUrl;
    use crate::card::common::ParameterValue;
    use crate::card::workflow::{
        CreateWorkflowRunRequest, ExternalGatewayProtocol, LlmRoute, WORKFLOW_RUN_ERROR_MAX_BYTES,
        WorkflowAction, WorkflowBinding, WorkflowBindingSource, WorkflowCard, WorkflowRun,
        WorkflowRunError, WorkflowRunStatus, WorkflowSpec, WorkflowStep, WorkflowStepStatus,
        jcs_len,
    };
    use crate::envelope::CardKind;
    use crate::error::WyrdError;
    use crate::gateway::GatewayFallbackOverride;
    use crate::ids::CredentialBindingName;
    use crate::ids::SpaceName;
    use crate::metadata::{Annotations, Labels};
    use crate::reference::{CardRef, InlineableRef, Ref};

    fn card_ref(kind: CardKind, name: &str) -> CardRef {
        CardRef {
            kind,
            name: name.parse().expect("valid card name"),
            version: "0.1.0".parse().expect("valid version"),
            space: Some(SpaceName::new("default").expect("static space is valid")),
            uid: None,
        }
    }

    fn prompt() -> skald_spec::Prompt {
        skald_spec::Prompt::new(
            skald_spec::ProviderRequest::OpenAiChatCompletion(skald_spec::OpenAiChatRequest {
                model: "gpt-4o-mini".to_owned(),
                messages: vec![skald_spec::OpenAiChatMessage {
                    role: "user".to_owned(),
                    content: Some(skald_spec::wire::openai_chat::OpenAiMessageContent::Text(
                        "plan".to_owned(),
                    )),
                    name: None,
                    tool_calls: None,
                    tool_call_id: None,
                    refusal: None,
                    annotations: Vec::new(),
                    audio: None,
                }],
                response_format: None,
                stream: None,
                stream_options: None,
                tools: None,
                tool_choice: None,
                parallel_tool_calls: None,
                settings: skald_spec::OpenAiChatSettings::default(),
            }),
            "gpt-4o-mini",
            None,
            skald_spec::ResponseType::Text,
        )
        .expect("static prompt is valid")
    }

    fn inline_agent_step(id: &str) -> WorkflowStep {
        WorkflowStep {
            id: id.to_owned(),
            action: WorkflowAction::Agent(InlineableRef::from(AgentSpec {
                prompt: InlineableRef::from(prompt()),
                tool_names: vec![],
                run_config: AgentRunConfigSpec::default(),
                verified_by: Vec::new(),
            })),
            depends_on: vec![],
            inputs: BTreeMap::new(),
            llm_route: None,
            fallback: None,
            timeout_seconds: None,
            retry: None,
            display: BTreeMap::new(),
        }
    }

    fn card_ref_agent_step(id: &str, agent_name: &str) -> WorkflowStep {
        WorkflowStep {
            id: id.to_owned(),
            action: WorkflowAction::Agent(InlineableRef::from(CardRef {
                kind: CardKind::Agent,
                name: agent_name.parse().expect("valid card name"),
                version: "0.1.0".parse().expect("valid version"),
                space: Some(SpaceName::new("default").expect("static space is valid")),
                uid: None,
            })),
            depends_on: vec![],
            inputs: BTreeMap::new(),
            llm_route: None,
            fallback: None,
            timeout_seconds: None,
            retry: None,
            display: BTreeMap::new(),
        }
    }

    #[test]
    fn workflow_spec_inline_agent_roundtrips() {
        let spec = WorkflowSpec {
            steps: vec![inline_agent_step("planner")],
            ..WorkflowSpec::default()
        };

        let yaml = serde_yaml::to_string(&spec).expect("serialize");
        let decoded: WorkflowSpec = serde_yaml::from_str(&yaml).expect("deserialize");
        assert_eq!(decoded, spec);
    }

    #[test]
    fn workflow_spec_card_ref_agent_roundtrips() {
        let spec = WorkflowSpec {
            steps: vec![card_ref_agent_step("planner", "research-planner")],
            ..WorkflowSpec::default()
        };

        let yaml = serde_yaml::to_string(&spec).expect("serialize");
        let decoded: WorkflowSpec = serde_yaml::from_str(&yaml).expect("deserialize");
        assert_eq!(decoded, spec);
        assert!(
            yaml.contains("research-planner"),
            "expected card ref name in YAML, got: {yaml}"
        );
    }

    #[test]
    fn workflow_card_envelope_roundtrips() {
        let card = WorkflowCard {
            space: "default".to_owned(),
            name: "research".to_owned(),
            version: "0.1.0".to_owned(),
            uid: String::new(),
            labels: Labels::default(),
            annotations: Annotations::default(),
            spec: WorkflowSpec {
                steps: vec![card_ref_agent_step("planner", "research-planner")],
                ..WorkflowSpec::default()
            },
            cascade_children: vec![],
            created_at: chrono::Utc::now(),
        };

        let envelope = card.to_envelope().expect("to_envelope");
        let reloaded = WorkflowCard::from_envelope(envelope).expect("from_envelope");
        assert_eq!(reloaded.spec, card.spec);
        assert_eq!(reloaded.name, card.name);
        assert_eq!(reloaded.version, card.version);
        assert_eq!(reloaded.cascade_children.len(), 1);
        assert_eq!(reloaded.cascade_children[0].kind, CardKind::Agent);
    }

    #[test]
    fn workflow_card_cascade_inline_agent_with_card_prompt() {
        let inline_with_prompt_ref = AgentSpec {
            prompt: InlineableRef::from(CardRef {
                kind: CardKind::Prompt,
                name: "planner-prompt".parse().expect("valid card name"),
                version: "0.3.0".parse().expect("valid version"),
                space: Some(SpaceName::new("default").expect("static space is valid")),
                uid: None,
            }),
            tool_names: vec![],
            run_config: AgentRunConfigSpec::default(),
            verified_by: Vec::new(),
        };
        let step = WorkflowStep {
            id: "planner".to_owned(),
            action: WorkflowAction::Agent(InlineableRef::from(inline_with_prompt_ref)),
            depends_on: vec![],
            inputs: BTreeMap::new(),
            llm_route: None,
            fallback: None,
            timeout_seconds: None,
            retry: None,
            display: BTreeMap::new(),
        };
        let card = WorkflowCard {
            space: "default".to_owned(),
            name: "research".to_owned(),
            version: "0.1.0".to_owned(),
            uid: String::new(),
            labels: Labels::default(),
            annotations: Annotations::default(),
            spec: WorkflowSpec {
                steps: vec![step],
                ..WorkflowSpec::default()
            },
            cascade_children: vec![],
            created_at: chrono::Utc::now(),
        };

        let envelope = card.to_envelope().expect("to_envelope");
        let reloaded = WorkflowCard::from_envelope(envelope).expect("from_envelope");
        assert_eq!(reloaded.cascade_children.len(), 1);
        assert_eq!(reloaded.cascade_children[0].kind, CardKind::Prompt);
        assert_eq!(reloaded.cascade_children[0].name.as_str(), "planner-prompt");
    }

    #[test]
    fn workflow_card_cascade_card_ref_agent() {
        let card = WorkflowCard {
            space: "default".to_owned(),
            name: "research".to_owned(),
            version: "0.1.0".to_owned(),
            uid: String::new(),
            labels: Labels::default(),
            annotations: Annotations::default(),
            spec: WorkflowSpec {
                steps: vec![card_ref_agent_step("planner", "research-planner")],
                ..WorkflowSpec::default()
            },
            cascade_children: vec![],
            created_at: chrono::Utc::now(),
        };

        let envelope = card.to_envelope().expect("to_envelope");
        let reloaded = WorkflowCard::from_envelope(envelope).expect("from_envelope");
        assert_eq!(reloaded.cascade_children.len(), 1);
        assert_eq!(reloaded.cascade_children[0].kind, CardKind::Agent);
        assert_eq!(
            reloaded.cascade_children[0].name.as_str(),
            "research-planner"
        );
    }

    #[test]
    fn workflow_card_cascade_includes_governance_and_observation_refs() {
        let spec = WorkflowSpec {
            governance: Some(Governance {
                policy_refs: vec![Ref::from(card_ref(CardKind::Policy, "policy"))],
                audit_ref: Some(Ref::from(card_ref(CardKind::Audit, "audit"))),
                ..Governance::default()
            }),
            observation_hooks: Some(ObservationHooks {
                route_refs: vec![Ref::from(card_ref(CardKind::Service, "route"))],
                ..ObservationHooks::default()
            }),
            ..WorkflowSpec::default()
        };
        let card = WorkflowCard {
            space: "default".to_owned(),
            name: "research".to_owned(),
            version: "0.1.0".to_owned(),
            uid: String::new(),
            labels: Labels::default(),
            annotations: Annotations::default(),
            spec,
            cascade_children: vec![],
            created_at: chrono::Utc::now(),
        };

        let envelope = card.to_envelope().expect("to_envelope");
        let reloaded = WorkflowCard::from_envelope(envelope).expect("from_envelope");
        let children: BTreeSet<_> = reloaded
            .cascade_children
            .iter()
            .map(|child| (child.kind.clone(), child.name.as_str().to_owned()))
            .collect();
        assert_eq!(children.len(), 3);
        assert!(children.contains(&(CardKind::Policy, "policy".to_owned())));
        assert!(children.contains(&(CardKind::Audit, "audit".to_owned())));
        assert!(children.contains(&(CardKind::Service, "route".to_owned())));
    }

    #[test]
    fn workflow_card_cascade_dedups_repeated_refs() {
        let spec = WorkflowSpec {
            steps: vec![
                card_ref_agent_step("planner-a", "shared-planner"),
                card_ref_agent_step("planner-b", "shared-planner"),
            ],
            ..WorkflowSpec::default()
        };
        let card = WorkflowCard {
            space: "default".to_owned(),
            name: "research".to_owned(),
            version: "0.1.0".to_owned(),
            uid: String::new(),
            labels: Labels::default(),
            annotations: Annotations::default(),
            spec,
            cascade_children: vec![],
            created_at: chrono::Utc::now(),
        };

        let envelope = card.to_envelope().expect("to_envelope");
        let reloaded = WorkflowCard::from_envelope(envelope).expect("from_envelope");
        assert_eq!(
            reloaded.cascade_children.len(),
            1,
            "expected dedup, got: {:?}",
            reloaded.cascade_children
        );
    }

    /// Build a valid two-step spec whose second step binds the first.
    fn bound_spec() -> WorkflowSpec {
        let mut summarize = inline_agent_step("summarize");
        summarize
            .inputs
            .insert("topic".to_owned(), binding("input.topic"));
        let mut review = inline_agent_step("review");
        review.depends_on = vec!["summarize".to_owned()];
        review.inputs.insert(
            "summary".to_owned(),
            binding("steps.summarize.output.structured.summary"),
        );
        WorkflowSpec {
            inputs: BTreeMap::from([("topic".to_owned(), ParameterValue::Str("rust".to_owned()))]),
            steps: vec![summarize, review],
            outputs: BTreeMap::from([
                ("review".to_owned(), binding("steps.review.output.text")),
                ("topic".to_owned(), binding("input.topic")),
            ]),
            ..WorkflowSpec::default()
        }
    }

    /// Parse a known-good binding fixture.
    fn binding(value: &str) -> WorkflowBinding {
        WorkflowBinding::new(value).expect("fixture binding is valid")
    }

    /// Build a fixture external-gateway route with the given header.
    fn ext_route(base_url: &str, header: &str) -> LlmRoute {
        LlmRoute::ExtGateway {
            protocol: ExternalGatewayProtocol::OpenAiChat,
            base_url: AbsoluteUrl::new(base_url).expect("fixture url parses"),
            headers: BTreeMap::from([(header.to_owned(), "value".to_owned())]),
            credential_binding: CredentialBindingName::new("acme").expect("fixture binding name"),
        }
    }

    /// Fixes the explicit Workflow wire contract and its pure validation.
    ///
    /// Covers exact serde/schema shapes for bindings, routes, and run DTOs;
    /// the removal of Prompt/Mcp actions and `condition`; and table-driven
    /// rejection of every pure graph, binding, output, and route violation
    /// with a field-specific stable error. A 10,000-step chain proves the
    /// validator is stack-safe.
    #[test]
    fn explicit_workflow_contract() {
        // Table-driven pure validation failures.
        type Mutation = fn(&mut WorkflowSpec);
        bound_spec().validate().expect("bound spec is valid");

        // Binding grammar.
        for good in [
            "input.topic",
            "steps.a.output.text",
            "steps.a.output.structured",
            "steps.a.output.structured.x.y_2",
        ] {
            WorkflowBinding::new(good).expect(good);
        }
        for bad in [
            "",
            "input.",
            "input.a.b",
            "input.1a",
            " input.topic",
            "{{input.topic}}",
            "input.topic + input.other",
            "steps.a.output",
            "steps.a.output.texts",
            "steps.a.output.structured.items[0]",
            "steps.a.output.structured.",
            "steps..output.text",
            "outputs.a",
        ] {
            assert!(
                WorkflowBinding::new(bad).is_err(),
                "{bad:?} must be rejected"
            );
            assert!(
                serde_json::from_value::<WorkflowBinding>(json!(bad)).is_err(),
                "{bad:?} must not deserialize"
            );
        }
        assert_eq!(
            binding("steps.a.output.structured.x.y").source(),
            WorkflowBindingSource::StepStructured {
                step: "a",
                path: "x.y"
            }
        );

        // Removed actions and condition.
        for removed in [
            json!({"id": "a", "action": {"type": "prompt", "target": {"path": "p.yaml"}}}),
            json!({"id": "a", "action": {"type": "mcp", "target": {"path": "m.yaml"}}}),
        ] {
            assert!(serde_json::from_value::<WorkflowStep>(removed).is_err());
        }
        let step_json = serde_json::to_value(inline_agent_step("a")).expect("serialize step");
        assert!(step_json.get("condition").is_none());

        // Route wire names.
        assert_eq!(
            serde_json::to_value(LlmRoute::WyrdGateway).expect("route"),
            json!({"kind": "wyrd_gateway"})
        );
        assert_eq!(
            serde_json::to_value(ext_route("https://gw.example.com/v1", "x-portkey-provider"))
                .expect("route"),
            json!({
                "kind": "ext_gateway",
                "protocol": "openai_chat",
                "base_url": "https://gw.example.com/v1",
                "headers": {"x-portkey-provider": "value"},
                "credential_binding": "acme",
            })
        );
        for (protocol, wire) in [
            (ExternalGatewayProtocol::OpenAiResponses, "openai_responses"),
            (
                ExternalGatewayProtocol::AnthropicMessages,
                "anthropic_messages",
            ),
            (
                ExternalGatewayProtocol::GeminiGenerateContent,
                "gemini_generate_content",
            ),
            (
                ExternalGatewayProtocol::VertexGenerateContent,
                "vertex_generate_content",
            ),
        ] {
            assert_eq!(
                serde_json::to_value(protocol).expect("protocol"),
                json!(wire)
            );
        }
        assert!(
            serde_json::from_value::<LlmRoute>(json!({
                "kind": "ext_gateway", "protocol": "openai_chat",
                "base_url": "https://gw.example.com", "credential_binding": "acme",
                "api_key": "sk-secret",
            }))
            .is_err(),
            "unknown external-gateway fields are rejected"
        );

        // Run DTO wire shapes.
        let run: WorkflowRun = serde_json::from_value(json!({
            "run_id": "01890a5d-ac96-774b-bcce-b302099a8057",
            "workflow": null,
            "status": "timed_out",
            "created_at": "2026-01-01T00:00:00Z",
            "started_at": null,
            "ended_at": null,
            "error": null,
        }))
        .expect("run with defaulted maps decodes");
        assert!(run.outputs.is_empty() && run.steps.is_empty());
        assert_eq!(run.status, WorkflowRunStatus::TimedOut);
        assert!(
            serde_json::from_value::<WorkflowRun>(json!({
                "run_id": "01890a5d-ac96-474b-bcce-b302099a8057",
                "workflow": null, "status": "queued", "created_at": "2026-01-01T00:00:00Z",
                "started_at": null, "ended_at": null, "error": null,
            }))
            .is_err(),
            "non-v7 run id is rejected"
        );
        assert_eq!(
            serde_json::to_value(WorkflowStepStatus::Unstarted).expect("status"),
            json!("unstarted")
        );
        assert!(
            serde_json::from_value::<CreateWorkflowRunRequest>(json!({
                "workflow": {"kind": "Workflow", "name": "review", "version": "1.0.0"},
                "extra": true,
            }))
            .is_err(),
            "request denies unknown fields"
        );
        let request: CreateWorkflowRunRequest = serde_json::from_value(json!({
            "workflow": {"kind": "Workflow", "name": "review", "version": "1.0.0"},
        }))
        .expect("request with defaulted input decodes");
        assert!(request.input.is_empty() && request.timeout_seconds.is_none());

        // Error projection stays within its ceiling and keeps the code.
        let huge = WyrdError::WorkflowRunTimeout {
            message: "x".repeat(10_000),
            details: json!({"blob": "y".repeat(10_000)}),
        };
        let projected = WorkflowRunError::from_wyrd(&huge);
        assert!(jcs_len(&projected) <= WORKFLOW_RUN_ERROR_MAX_BYTES);
        assert_eq!(projected.code, "WYRD_WORKFLOW_504_RUN_TIMEOUT");
        assert_eq!(projected.remediation, huge.remediation());
        assert_eq!(projected.details, json!({}));

        let cases: Vec<(&str, Mutation, &str, &str)> = vec![
            (
                "empty steps",
                |s| s.steps.clear(),
                "steps",
                "WYRD_WORKFLOW_422_VALIDATION",
            ),
            (
                "empty id",
                |s| s.steps[0].id = String::new(),
                "steps[0].id",
                "WYRD_WORKFLOW_422_VALIDATION",
            ),
            (
                "bad id",
                |s| s.steps[0].id = "a-b".to_owned(),
                "steps[0].id",
                "WYRD_WORKFLOW_422_VALIDATION",
            ),
            (
                "duplicate id",
                |s| s.steps[1].id = "summarize".to_owned(),
                "steps[1].id",
                "WYRD_WORKFLOW_422_DUPLICATE_STEP_ID",
            ),
            (
                "missing dep",
                |s| s.steps[1].depends_on = vec!["ghost".to_owned()],
                "steps[1].depends_on",
                "WYRD_WORKFLOW_422_MISSING_DEPENDENCY",
            ),
            (
                "self dep",
                |s| s.steps[0].depends_on = vec!["summarize".to_owned()],
                "steps[0].depends_on",
                "WYRD_WORKFLOW_422_VALIDATION",
            ),
            (
                "duplicate dep",
                |s| s.steps[1].depends_on.push("summarize".to_owned()),
                "steps[1].depends_on",
                "WYRD_WORKFLOW_422_VALIDATION",
            ),
            (
                "cycle",
                |s| s.steps[0].depends_on = vec!["review".to_owned()],
                "steps",
                "WYRD_WORKFLOW_422_CYCLE",
            ),
            (
                "hidden step",
                |s| {
                    s.steps[1].depends_on.clear();
                },
                "steps[1].inputs.summary",
                "WYRD_WORKFLOW_422_VALIDATION",
            ),
            (
                "self reference",
                |s| {
                    s.steps[1].inputs.insert(
                        "own".to_owned(),
                        WorkflowBinding::new("steps.review.output.text").expect("valid"),
                    );
                },
                "steps[1].inputs.own",
                "WYRD_WORKFLOW_422_VALIDATION",
            ),
            (
                "undeclared input",
                |s| {
                    s.steps[0].inputs.insert(
                        "x".to_owned(),
                        WorkflowBinding::new("input.nope").expect("valid"),
                    );
                },
                "steps[0].inputs.x",
                "WYRD_WORKFLOW_422_VALIDATION",
            ),
            (
                "bad input name",
                |s| {
                    s.inputs
                        .insert("bad-name".to_owned(), ParameterValue::Int(1));
                },
                "inputs.bad-name",
                "WYRD_WORKFLOW_422_VALIDATION",
            ),
            (
                "empty outputs",
                |s| s.outputs.clear(),
                "outputs",
                "WYRD_WORKFLOW_422_VALIDATION",
            ),
            (
                "bad output name",
                |s| {
                    s.outputs.insert(
                        "bad name".to_owned(),
                        WorkflowBinding::new("input.topic").expect("valid"),
                    );
                },
                "outputs.bad name",
                "WYRD_WORKFLOW_422_VALIDATION",
            ),
            (
                "output unknown step",
                |s| {
                    s.outputs.insert(
                        "x".to_owned(),
                        WorkflowBinding::new("steps.ghost.output.text").expect("valid"),
                    );
                },
                "outputs.x",
                "WYRD_WORKFLOW_422_VALIDATION",
            ),
            (
                "output undeclared input",
                |s| {
                    s.outputs.insert(
                        "x".to_owned(),
                        WorkflowBinding::new("input.nope").expect("valid"),
                    );
                },
                "outputs.x",
                "WYRD_WORKFLOW_422_VALIDATION",
            ),
            (
                "native fallback",
                |s| s.steps[0].fallback = Some(fallback()),
                "steps[0].fallback",
                "WYRD_WORKFLOW_422_VALIDATION",
            ),
            (
                "ext fallback",
                |s| {
                    s.llm_route = Some(ext_route("https://gw.example.com", "x-a"));
                    s.steps[0].fallback = Some(fallback());
                },
                "steps[0].fallback",
                "WYRD_WORKFLOW_422_VALIDATION",
            ),
            (
                "empty fallback",
                |s| {
                    s.steps[0].llm_route = Some(LlmRoute::WyrdGateway);
                    s.steps[0].fallback = Some(GatewayFallbackOverride {
                        candidates: Vec::new(),
                    });
                },
                "steps[0].fallback",
                "WYRD_WORKFLOW_422_VALIDATION",
            ),
            (
                "userinfo",
                |s| s.llm_route = Some(ext_route("https://u:p@gw.example.com", "x-a")),
                "llm_route.base_url",
                "WYRD_WORKFLOW_422_VALIDATION",
            ),
            (
                "query",
                |s| s.llm_route = Some(ext_route("https://gw.example.com/v1?k=v", "x-a")),
                "llm_route.base_url",
                "WYRD_WORKFLOW_422_VALIDATION",
            ),
            (
                "fragment",
                |s| s.steps[1].llm_route = Some(ext_route("https://gw.example.com/#f", "x-a")),
                "steps[1].llm_route.base_url",
                "WYRD_WORKFLOW_422_VALIDATION",
            ),
            (
                "authorization header",
                |s| s.llm_route = Some(ext_route("https://gw.example.com", "Authorization")),
                "llm_route.headers.Authorization",
                "WYRD_WORKFLOW_422_VALIDATION",
            ),
            (
                "api-key header",
                |s| s.llm_route = Some(ext_route("https://gw.example.com", "x-api-key")),
                "llm_route.headers.x-api-key",
                "WYRD_WORKFLOW_422_VALIDATION",
            ),
            (
                "token header",
                |s| s.llm_route = Some(ext_route("https://gw.example.com", "x-session-token")),
                "llm_route.headers.x-session-token",
                "WYRD_WORKFLOW_422_VALIDATION",
            ),
            (
                "forwarded header",
                |s| s.llm_route = Some(ext_route("https://gw.example.com", "X-Forwarded-For")),
                "llm_route.headers.X-Forwarded-For",
                "WYRD_WORKFLOW_422_VALIDATION",
            ),
            (
                "proxy header",
                |s| s.llm_route = Some(ext_route("https://gw.example.com", "proxy-authorization")),
                "llm_route.headers.proxy-authorization",
                "WYRD_WORKFLOW_422_VALIDATION",
            ),
            (
                "host header",
                |s| s.llm_route = Some(ext_route("https://gw.example.com", "host")),
                "llm_route.headers.host",
                "WYRD_WORKFLOW_422_VALIDATION",
            ),
            (
                "invalid header",
                |s| s.llm_route = Some(ext_route("https://gw.example.com", "bad header")),
                "llm_route.headers.bad header",
                "WYRD_WORKFLOW_422_VALIDATION",
            ),
            (
                "case duplicate header",
                |s| {
                    if let Some(LlmRoute::ExtGateway { headers, .. }) = &mut s.llm_route {
                        headers.insert("X-A".to_owned(), "v".to_owned());
                    }
                },
                "llm_route.headers.x-a",
                "WYRD_WORKFLOW_422_VALIDATION",
            ),
        ];
        for (name, mutate, field, code) in cases {
            let mut spec = bound_spec();
            if name == "case duplicate header" {
                spec.llm_route = Some(ext_route("https://gw.example.com", "x-a"));
            }
            mutate(&mut spec);
            let error = spec.validate().expect_err(name);
            assert_eq!(error.field(), field, "{name}");
            let wyrd: WyrdError = error.into();
            assert_eq!(wyrd.code(), code, "{name}");
            assert_eq!(wyrd.as_problem_json()["details"]["field"], field, "{name}");
        }

        // Valid gateway fallback and a transitive dependency are accepted.
        let mut spec = bound_spec();
        spec.steps[0].llm_route = Some(LlmRoute::WyrdGateway);
        spec.steps[0].fallback = Some(fallback());
        let mut third = inline_agent_step("third");
        third.depends_on = vec!["review".to_owned()];
        third
            .inputs
            .insert("topic".to_owned(), binding("steps.summarize.output.text"));
        spec.steps.push(third);
        spec.validate()
            .expect("transitive binding and gateway fallback are valid");

        // Deep chains validate without recursion.
        let mut deep = bound_spec();
        deep.steps = (0..10_000)
            .map(|index| {
                let mut step = inline_agent_step(&format!("s{index}"));
                if index > 0 {
                    step.depends_on = vec![format!("s{}", index - 1)];
                }
                step
            })
            .collect();
        deep.steps[9_999]
            .inputs
            .insert("first".to_owned(), binding("steps.s0.output.text"));
        deep.outputs = BTreeMap::from([("last".to_owned(), binding("steps.s9999.output.text"))]);
        deep.validate().expect("deep chain is valid");
    }

    /// Build a valid single-candidate gateway fallback fixture.
    fn fallback() -> GatewayFallbackOverride {
        GatewayFallbackOverride {
            candidates: vec![
                crate::gateway::ModelRef::from_projection("openai/gpt-4o")
                    .expect("fixture model ref"),
            ],
        }
    }
}
