//! Resolved Workflow validation and the immutable execution plan.
//!
//! Validation is synchronous and IO-free. [`ResolvedGraph`] checks what the
//! pure Card contract cannot: every step has a resolved Agent whose Prompt can
//! drive the Agent loop, every unresolved Prompt variable has exactly one
//! binding, step-output bindings select the payload their target produces,
//! output schemas compile, and external routes speak the Prompt's dialect.
//! [`ExecutionPlan`] then adds the invocation input and the execution
//! environment's route checks, so every pre-dispatch failure is returned
//! before any step runs.

use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;
use std::time::Duration;

use serde_json::Value;
use skald_agent::Agent;
use skald_agent::request_builder::validate_prompt_loop_request;
use skald_spec::ResponseType;
use wyrd_spec::card::common::ParameterValue;
use wyrd_spec::card::workflow::{
    LlmRoute, WorkflowBinding, WorkflowBindingSource, WorkflowSpec, WorkflowValidationError,
    jcs_len,
};
use wyrd_spec::error::WyrdError;

use crate::error::{WorkflowError, WorkflowResult};
use crate::output::OutputValidator;
use crate::route::{StepRoute, WorkflowExecutionDependencies, protocol_matches, route_unsupported};
use crate::workflow_surface::WorkflowInput;

/// Workflow attempts after the first when a step declares no retry policy.
pub const DEFAULT_MAX_RETRIES: u32 = 3;

/// One step whose Agent and bindings passed resolved validation.
pub(crate) struct ResolvedStep {
    /// Step ID.
    pub(crate) id: String,
    /// Resolved Agent.
    pub(crate) agent: Arc<Agent>,
    /// Prompt variable name and the binding that supplies it.
    pub(crate) bindings: Vec<(String, WorkflowBinding)>,
    /// Compiled output validator for JSON-schema Prompts.
    pub(crate) validator: Option<OutputValidator>,
}

/// A Workflow graph whose steps are resolved and cross-checked.
pub(crate) struct ResolvedGraph {
    /// Steps in declaration order.
    pub(crate) steps: Vec<ResolvedStep>,
}

impl ResolvedGraph {
    /// Validate `spec` against its resolved Agents.
    ///
    /// # Errors
    ///
    /// Returns the pure contract error first; then
    /// [`WorkflowError::AgentNotFound`] for an unresolved step; a
    /// field-specific `WYRD_WORKFLOW_422_VALIDATION` error for a Prompt that
    /// cannot drive the Agent loop, an unbound or extra Prompt variable, or a
    /// binding that selects a payload its target step does not produce;
    /// `WYRD_WORKFLOW_422_OUTPUT_SCHEMA` for an output schema that does not
    /// compile; and `WYRD_WORKFLOW_422_ROUTE_UNSUPPORTED` for an external
    /// route whose protocol differs from the Prompt's request dialect.
    pub(crate) fn resolve(
        spec: &WorkflowSpec,
        agents: &HashMap<String, Arc<Agent>>,
    ) -> WorkflowResult<Self> {
        spec.validate()?;
        let mut response_types = HashMap::with_capacity(spec.steps.len());
        let mut steps = Vec::with_capacity(spec.steps.len());
        for (position, step) in spec.steps.iter().enumerate() {
            let agent = agents
                .get(&step.id)
                .ok_or_else(|| WorkflowError::AgentNotFound(step.id.clone()))?;
            let prompt = agent.prompt.native();
            validate_prompt_loop_request(&agent.id, &prompt.request).map_err(|error| {
                WorkflowValidationError::invalid(
                    &format!("steps[{position}].action"),
                    &format!("prompt cannot drive the agent loop: {}", error.code()),
                )
            })?;
            if let LlmRoute::ExtGateway { protocol, .. } = spec.resolved_route(step)
                && !protocol_matches(*protocol, &prompt.request)
            {
                return Err(route_unsupported(
                    &format!("steps[{position}].llm_route"),
                    "external gateway protocol differs from the prompt request dialect",
                )
                .into());
            }
            for variable in &prompt.variables {
                if !step.inputs.contains_key(variable) {
                    return Err(WorkflowValidationError::invalid(
                        &format!("steps[{position}].inputs.{variable}"),
                        "unresolved prompt variable has no binding",
                    )
                    .into());
                }
            }
            for name in step.inputs.keys() {
                if !prompt.variables.contains(name) {
                    return Err(WorkflowValidationError::invalid(
                        &format!("steps[{position}].inputs.{name}"),
                        "binding does not name an unresolved prompt variable",
                    )
                    .into());
                }
            }
            response_types.insert(step.id.as_str(), &prompt.response_type);
            steps.push(ResolvedStep {
                id: step.id.clone(),
                agent: Arc::clone(agent),
                bindings: step
                    .inputs
                    .iter()
                    .map(|(name, binding)| (name.clone(), binding.clone()))
                    .collect(),
                validator: OutputValidator::compile(&step.id, &prompt.response_type)?,
            });
        }
        for (position, step) in spec.steps.iter().enumerate() {
            for (name, binding) in &step.inputs {
                require_payload_kind(
                    &format!("steps[{position}].inputs.{name}"),
                    binding,
                    &response_types,
                )?;
            }
        }
        for (name, binding) in &spec.outputs {
            require_payload_kind(&format!("outputs.{name}"), binding, &response_types)?;
        }
        Ok(Self { steps })
    }
}

/// Require that a step-output binding selects the payload its target makes.
///
/// # Errors
///
/// Returns a field-specific validation error when a text binding targets a
/// JSON-schema step or a structured binding targets a text step.
fn require_payload_kind(
    field: &str,
    binding: &WorkflowBinding,
    response_types: &HashMap<&str, &ResponseType>,
) -> Result<(), WorkflowValidationError> {
    let (step, wants_structured) = match binding.source() {
        WorkflowBindingSource::Input(_) => return Ok(()),
        WorkflowBindingSource::StepText(step) => (step, false),
        WorkflowBindingSource::StepStructured { step, .. } => (step, true),
    };
    let is_structured = matches!(
        response_types.get(step),
        Some(ResponseType::JsonSchema { .. })
    );
    if is_structured == wants_structured {
        Ok(())
    } else if wants_structured {
        Err(WorkflowValidationError::invalid(
            field,
            "structured binding targets a step with a text response",
        ))
    } else {
        Err(WorkflowValidationError::invalid(
            field,
            "text binding targets a step with a structured response",
        ))
    }
}

/// One step ready for execution.
pub(crate) struct PlannedStep {
    /// Step ID.
    pub(crate) id: String,
    /// Resolved Agent.
    pub(crate) agent: Arc<Agent>,
    /// Prompt variable bindings.
    pub(crate) bindings: Vec<(String, WorkflowBinding)>,
    /// Compiled output validator.
    pub(crate) validator: Option<OutputValidator>,
    /// Plan indices of dependents, which become ready when this step succeeds.
    pub(crate) dependents: Vec<usize>,
    /// Number of dependencies.
    pub(crate) dependency_count: usize,
    /// Route state checked against the environment.
    pub(crate) route: StepRoute,
    /// Per-attempt timeout.
    pub(crate) timeout: Option<Duration>,
    /// Workflow attempts after the first.
    pub(crate) max_retries: u32,
    /// Initial Workflow backoff in milliseconds; zero retries immediately.
    pub(crate) initial_backoff_ms: u64,
}

/// Immutable, fully validated execution plan.
///
/// Steps are ordered by `(stage, step ID)`, so a smaller plan index is both
/// the ready-queue priority and the primary-error precedence.
pub(crate) struct ExecutionPlan {
    /// Ordered steps.
    pub(crate) steps: Vec<PlannedStep>,
    /// Resolved invocation input with defaults merged.
    pub(crate) input: BTreeMap<String, Value>,
    /// Declared Workflow outputs.
    pub(crate) outputs: BTreeMap<String, WorkflowBinding>,
}

impl ExecutionPlan {
    /// Build the plan for one invocation.
    ///
    /// Order: pure contract and resolved validation, invocation input, then
    /// each step's route against `dependencies`.
    ///
    /// # Errors
    ///
    /// Returns the errors of [`ResolvedGraph::resolve`],
    /// `WYRD_WORKFLOW_422_RUN_REQUEST` for invalid input,
    /// `WYRD_WORKFLOW_413_INPUT_TOO_LARGE` for input above `max_input_bytes`,
    /// and the route errors of
    /// [`WorkflowExecutionDependencies::resolve_route`].
    pub(crate) fn build(
        spec: &WorkflowSpec,
        agents: &HashMap<String, Arc<Agent>>,
        dependencies: &WorkflowExecutionDependencies,
        input: WorkflowInput,
        max_input_bytes: Option<usize>,
    ) -> WorkflowResult<Self> {
        let graph = ResolvedGraph::resolve(spec, agents)?;
        let input = resolve_input(spec, input, max_input_bytes)?;
        let stages = stages(spec);
        let mut order: Vec<usize> = (0..spec.steps.len()).collect();
        order.sort_by(|&left, &right| {
            (stages[left], &spec.steps[left].id).cmp(&(stages[right], &spec.steps[right].id))
        });
        let index: BTreeMap<String, usize> = order
            .iter()
            .enumerate()
            .map(|(planned, &declared)| (spec.steps[declared].id.clone(), planned))
            .collect();
        let mut dependents: Vec<Vec<usize>> = vec![Vec::new(); order.len()];
        for step in &spec.steps {
            for dependency in &step.depends_on {
                if let (Some(&parent), Some(&child)) = (index.get(dependency), index.get(&step.id))
                {
                    dependents[parent].push(child);
                }
            }
        }
        let mut resolved: Vec<Option<ResolvedStep>> = graph.steps.into_iter().map(Some).collect();
        let mut steps = Vec::with_capacity(order.len());
        for &declared in &order {
            let step = &spec.steps[declared];
            let field = format!("steps[{declared}].llm_route");
            let route = dependencies.resolve_route(
                &field,
                spec.resolved_route(step),
                step.fallback.as_ref(),
            )?;
            let Some(resolved_step) = resolved[declared].take() else {
                return Err(WyrdError::WorkflowInternal {
                    message: "resolved step was planned twice".to_owned(),
                    details: serde_json::json!({ "step": step.id }),
                }
                .into());
            };
            let dependents = std::mem::take(&mut dependents[steps.len()]);
            steps.push(PlannedStep {
                id: resolved_step.id,
                agent: resolved_step.agent,
                bindings: resolved_step.bindings,
                validator: resolved_step.validator,
                dependents,
                dependency_count: step.depends_on.len(),
                route,
                timeout: step.timeout_seconds.map(Duration::from_secs),
                max_retries: step
                    .retry
                    .as_ref()
                    .map_or(DEFAULT_MAX_RETRIES, |retry| retry.max_retries),
                initial_backoff_ms: step
                    .retry
                    .as_ref()
                    .and_then(|retry| retry.initial_backoff_ms)
                    .unwrap_or(0),
            });
        }
        Ok(Self {
            steps,
            input,
            outputs: spec.outputs.clone(),
        })
    }
}

/// Compute each declared step's longest dependency depth.
///
/// Iterative Kahn traversal over an already validated acyclic graph.
fn stages(spec: &WorkflowSpec) -> Vec<usize> {
    let position: HashMap<&str, usize> = spec
        .steps
        .iter()
        .enumerate()
        .map(|(index, step)| (step.id.as_str(), index))
        .collect();
    let mut remaining: Vec<usize> = spec
        .steps
        .iter()
        .map(|step| step.depends_on.len())
        .collect();
    let mut dependents: Vec<Vec<usize>> = vec![Vec::new(); spec.steps.len()];
    for (index, step) in spec.steps.iter().enumerate() {
        for dependency in &step.depends_on {
            if let Some(&parent) = position.get(dependency.as_str()) {
                dependents[parent].push(index);
            }
        }
    }
    let mut stage = vec![0; spec.steps.len()];
    let mut ready: Vec<usize> = (0..spec.steps.len())
        .filter(|&index| remaining[index] == 0)
        .collect();
    while let Some(index) = ready.pop() {
        for &child in &dependents[index] {
            stage[child] = stage[child].max(stage[index] + 1);
            remaining[child] -= 1;
            if remaining[child] == 0 {
                ready.push(child);
            }
        }
    }
    stage
}

/// Resolve invocation input against declared inputs and their defaults.
///
/// Text shorthand is accepted only when the Workflow declares a string
/// `input`. Every supplied key must be declared and match its declared
/// variant (an integer for `int`, any number for `float`, a string, a
/// boolean, or any JSON value). Defaults fill missing keys; a non-finite
/// float default becomes `null`.
///
/// # Errors
///
/// Returns `WYRD_WORKFLOW_422_RUN_REQUEST` naming the offending input, or
/// `WYRD_WORKFLOW_413_INPUT_TOO_LARGE` when the JCS size of the resolved map
/// exceeds `max_input_bytes`.
pub(crate) fn resolve_input(
    spec: &WorkflowSpec,
    input: WorkflowInput,
    max_input_bytes: Option<usize>,
) -> WorkflowResult<BTreeMap<String, Value>> {
    let supplied = match input {
        WorkflowInput::Text(text) => {
            if !matches!(spec.inputs.get("input"), Some(ParameterValue::Str(_))) {
                return Err(run_request(
                    "input",
                    "text input requires a declared string input named 'input'",
                ));
            }
            serde_json::Map::from_iter([("input".to_owned(), Value::String(text))])
        }
        WorkflowInput::Vars(map) => map,
    };
    let mut resolved = BTreeMap::new();
    for (name, value) in supplied {
        let field = format!("input.{name}");
        let Some(declared) = spec.inputs.get(&name) else {
            return Err(run_request(&field, "input is not declared by the workflow"));
        };
        let matches = match declared {
            ParameterValue::Int(_) => value.is_i64(),
            ParameterValue::Float(_) => value.is_number(),
            ParameterValue::Str(_) => value.is_string(),
            ParameterValue::Bool(_) => value.is_boolean(),
            ParameterValue::Json(_) => true,
        };
        if !matches {
            return Err(run_request(
                &field,
                "input value does not match its declared type",
            ));
        }
        resolved.insert(name, value);
    }
    for (name, default) in &spec.inputs {
        resolved
            .entry(name.clone())
            .or_insert_with(|| parameter_json(default));
    }
    if let Some(limit) = max_input_bytes {
        let size = jcs_len(&resolved);
        if size > limit {
            return Err(WyrdError::WorkflowInputTooLarge {
                message: format!("workflow input is {size} bytes; the limit is {limit}"),
                details: serde_json::json!({ "limit": limit }),
            }
            .into());
        }
    }
    Ok(resolved)
}

/// Convert a native parameter value to JSON; non-finite floats become null.
fn parameter_json(value: &ParameterValue) -> Value {
    match value {
        ParameterValue::Int(value) => Value::from(*value),
        ParameterValue::Float(value) => {
            serde_json::Number::from_f64(*value).map_or(Value::Null, Value::Number)
        }
        ParameterValue::Str(value) => Value::String(value.clone()),
        ParameterValue::Bool(value) => Value::Bool(*value),
        ParameterValue::Json(value) => value.clone(),
    }
}

/// Build a run-request error naming `field`.
fn run_request(field: &str, reason: &str) -> WorkflowError {
    WyrdError::WorkflowRunRequest {
        message: format!("{field}: {reason}"),
        details: serde_json::json!({ "field": field, "reason": reason }),
    }
    .into()
}
