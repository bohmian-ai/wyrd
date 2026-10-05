//! Skald Workflow engine: explicit-binding DAG execution over Skald Agents.
//!
//! ## Model
//!
//! A [`Workflow`] declares typed inputs, Agent steps whose unresolved Prompt
//! variables are bound to exact sources (`input.<name>` or a dependency's
//! `steps.<id>.output.text|structured[.<field>...]`), and named outputs.
//! Dependency edges order execution and inject no data.
//!
//! ## Execution
//!
//! [`Workflow::run_with_options`] validates the resolved graph, input, and
//! routes before dispatch, then executes steps through the existing Agent loop
//! in one owned, bounded task set with Workflow retries, per-attempt timeouts,
//! a total deadline, and cancellation. The result is always the portable
//! [`WorkflowRun`] snapshot; step failures are recorded in it rather than
//! returned as errors.
//!
//! ## Routes
//!
//! Each step's model calls use its resolved [`wyrd_spec::card::workflow::LlmRoute`]:
//! the native provider registry, a governed Wyrd gateway through
//! [`WyrdGatewayCaller`], or a bound external gateway. The execution
//! environment supplies these through [`WorkflowExecutionDependencies`].
//!
//! ## Independence
//!
//! `skald-workflow` depends on Skald crates, `wyrd-spec`, and neutral
//! infrastructure only. It does not depend on Wyrd server or Vala crates.

#![deny(missing_docs)]

mod attempt;
mod bodies;
pub mod error;
mod output;
mod plan;
#[cfg(feature = "python")]
pub mod python;
pub mod route;
mod run;
#[cfg(test)]
mod test_support;
pub mod workflow;
pub mod workflow_surface;

pub use bodies::{AgentTools, CardBodies, card_body_dependencies};
pub use error::{WorkflowError, WorkflowResult};
pub use plan::DEFAULT_MAX_RETRIES;
pub use route::{
    DEFAULT_GATEWAY_CALL_TIMEOUT, ExternalEndpointProfile, ExternalGatewayBinding,
    ExternalGatewayBindings, WorkflowExecutionDependencies, WorkflowGatewayCorrelation,
    WyrdGatewayCall, WyrdGatewayCaller,
};
pub use workflow::{DEFAULT_MAX_CONCURRENCY, WorkflowExecutionLimits, WorkflowRunOptions};
pub use workflow_surface::{
    AgentResolver, PreparedWorkflowRun, Workflow, WorkflowBuilder, WorkflowInput, step_id_for_name,
};
pub use wyrd_spec::card::workflow::{
    WorkflowBinding, WorkflowRun, WorkflowRunError, WorkflowRunStatus, WorkflowStepResult,
    WorkflowStepStatus,
};
