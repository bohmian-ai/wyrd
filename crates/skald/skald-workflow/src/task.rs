//! Live task state and output validation.

use std::fmt;
use std::sync::Arc;

use jsonschema::{SchemaResolver, SchemaResolverError};
use serde_json::Value;
use skald_spec::{Prompt, ProviderResponse, ResponseType};
use url::Url;

use crate::def::TaskDef;
use crate::error::{WorkflowError, WorkflowResult};

type OutputValidator = jsonschema::JSONSchema;

/// Rejects all external schema URIs to prevent outbound HTTP during schema compile.
struct NoRemoteResolver;

impl SchemaResolver for NoRemoteResolver {
    fn resolve(
        &self,
        _root_schema: &Value,
        url: &Url,
        _original_reference: &str,
    ) -> Result<Arc<Value>, SchemaResolverError> {
        Err(anyhow::anyhow!(
            "remote schema resolution is disabled: {url}"
        ))
    }
}

/// Lifecycle status of a task during execution.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskStatus {
    /// Not yet eligible to run, or eligible but not started.
    Pending,
    /// Currently executing.
    Running,
    /// Finished successfully.
    Completed,
    /// Failed; if retries remain, may be reset to pending.
    Failed,
}

/// Live task built from a declarative task definition.
pub struct Task {
    /// Stable id from the definition.
    pub id: String,
    /// Owning agent id.
    pub agent_id: String,
    /// Native prompt the task runs.
    pub prompt: Prompt,
    /// Task ids this task depends on.
    pub(crate) dependencies: Vec<String>,
    /// Current lifecycle status.
    pub status: TaskStatus,
    /// Maximum execution retries.
    pub max_retries: u32,
    /// Provider response captured on successful completion.
    pub result: Option<ProviderResponse>,
    /// Number of completed retries.
    pub retry_count: u32,
    /// Compiled output validator. Text responses do not have one.
    pub output_validator: Option<OutputValidator>,
}

impl fmt::Debug for Task {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Task")
            .field("id", &self.id)
            .field("agent_id", &self.agent_id)
            .field("prompt", &self.prompt)
            .field("dependencies", &self.dependencies)
            .field("status", &self.status)
            .field("max_retries", &self.max_retries)
            .field("result", &self.result)
            .field("retry_count", &self.retry_count)
            .field("output_validator", &self.output_validator.is_some())
            .finish()
    }
}

impl Task {
    /// Returns the task's dependencies as a read-only slice.
    pub fn dependencies(&self) -> &[String] {
        &self.dependencies
    }

    /// Build a live task from its declarative form.
    ///
    /// # Errors
    ///
    /// Returns `WorkflowError` when the task prompt cannot drive the agent
    /// loop or when its JSON schema response validator fails to compile.
    pub fn from_def(def: TaskDef) -> WorkflowResult<Self> {
        skald_agent::request_builder::validate_prompt_loop_request(&def.id, &def.prompt.request)?;
        let output_validator = compile_validator(&def.id, &def.prompt.response_type)?;
        Ok(Self {
            id: def.id,
            agent_id: def.agent_id,
            prompt: def.prompt,
            dependencies: def.dependencies,
            status: TaskStatus::Pending,
            max_retries: def.max_retries,
            result: None,
            retry_count: 0,
            output_validator,
        })
    }

    /// Validate a provider response against this task's output validator.
    ///
    /// # Errors
    ///
    /// Returns `WorkflowError::ResponseValidationFailed` when structured output
    /// is missing, cannot be parsed, or does not satisfy the compiled schema.
    pub fn validate_response(&self, response: &ProviderResponse) -> WorkflowResult<()> {
        let Some(validator) = &self.output_validator else {
            return Ok(());
        };
        let raw = response.adapter().structured_output().ok_or_else(|| {
            WorkflowError::ResponseValidationFailed {
                task_id: self.id.clone(),
                expected_schema: response_schema_name(&self.prompt.response_type),
                received: "no structured output".to_owned(),
            }
        })?;
        let value: Value = serde_json::from_str(raw.get()).map_err(|err| {
            WorkflowError::ResponseValidationFailed {
                task_id: self.id.clone(),
                expected_schema: response_schema_name(&self.prompt.response_type),
                received: err.to_string(),
            }
        })?;
        validator.validate(&value).map_err(|errors| {
            let received = errors
                .map(|err| err.to_string())
                .collect::<Vec<_>>()
                .join("; ");
            WorkflowError::ResponseValidationFailed {
                task_id: self.id.clone(),
                expected_schema: response_schema_name(&self.prompt.response_type),
                received,
            }
        })?;
        Ok(())
    }
}

fn compile_validator(
    task_id: &str,
    response_type: &ResponseType,
) -> WorkflowResult<Option<OutputValidator>> {
    match response_type {
        ResponseType::Text => Ok(None),
        ResponseType::JsonSchema { schema, .. } => jsonschema::JSONSchema::options()
            .with_resolver(NoRemoteResolver)
            .compile(schema)
            .map(Some)
            .map_err(|err| WorkflowError::ResponseValidationFailed {
                task_id: task_id.to_owned(),
                expected_schema: response_schema_name(response_type),
                received: format!("schema compile failed: {err}"),
            }),
    }
}

fn response_schema_name(response_type: &ResponseType) -> String {
    match response_type {
        ResponseType::JsonSchema { name, .. } => name.clone(),
        ResponseType::Text => String::new(),
    }
}
