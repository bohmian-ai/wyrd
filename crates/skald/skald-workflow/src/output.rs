//! Declared step-output validation.
//!
//! A step whose Agent Prompt declares a JSON-schema response type compiles
//! that schema once while the resolved plan is built. Each attempt validates
//! its final structured output against it before the step can succeed; a
//! violation is a retryable attempt failure.

use std::fmt;
use std::sync::Arc;

use jsonschema::{SchemaResolver, SchemaResolverError};
use serde_json::Value;
use skald_spec::ResponseType;
use url::Url;
use wyrd_spec::error::WyrdError;

/// Rejects all external schema URIs to prevent outbound HTTP during schema compile.
struct NoRemoteResolver;

impl SchemaResolver for NoRemoteResolver {
    /// Refuse every remote reference so schema compilation stays IO-free.
    fn resolve(
        &self,
        _root_schema: &Value,
        url: &Url,
        _original_reference: &str,
    ) -> Result<Arc<Value>, SchemaResolverError> {
        Err(SchemaResolverError::new(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            format!("remote schema resolution is disabled: {url}"),
        )))
    }
}

/// Compiled JSON-schema validator for one step's declared structured output.
pub(crate) struct OutputValidator {
    /// Schema name from the Prompt response type, used in diagnostics.
    schema_name: String,
    /// Compiled validator.
    validator: jsonschema::JSONSchema,
}

impl fmt::Debug for OutputValidator {
    /// Formats the schema name only; the compiled validator has no useful debug form.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OutputValidator")
            .field("schema_name", &self.schema_name)
            .finish_non_exhaustive()
    }
}

impl OutputValidator {
    /// Compile the validator for `response_type`; text responses have none.
    ///
    /// # Errors
    ///
    /// Returns `WYRD_WORKFLOW_422_OUTPUT_SCHEMA` naming `step_id` when the
    /// declared schema does not compile.
    pub(crate) fn compile(
        step_id: &str,
        response_type: &ResponseType,
    ) -> Result<Option<Self>, WyrdError> {
        let ResponseType::JsonSchema { name, schema, .. } = response_type else {
            return Ok(None);
        };
        jsonschema::JSONSchema::options()
            .with_resolver(NoRemoteResolver)
            .compile(schema)
            .map(|validator| {
                Some(Self {
                    schema_name: name.clone(),
                    validator,
                })
            })
            .map_err(|error| WyrdError::WorkflowOutputSchema {
                message: format!("step '{step_id}' output schema does not compile"),
                details: serde_json::json!({
                    "step": step_id,
                    "expected_schema": name,
                    "reason": error.to_string(),
                }),
            })
    }

    /// Validate one attempt's structured output.
    ///
    /// # Errors
    ///
    /// Returns `WYRD_WORKFLOW_422_OUTPUT_SCHEMA` naming `step_id` when the
    /// output is absent or violates the schema. The details carry validator
    /// paths only, never the output value.
    pub(crate) fn validate(&self, step_id: &str, output: Option<&Value>) -> Result<(), WyrdError> {
        let failure = |reason: String| WyrdError::WorkflowOutputSchema {
            message: format!("step '{step_id}' output failed schema validation"),
            details: serde_json::json!({
                "step": step_id,
                "expected_schema": self.schema_name,
                "reason": reason,
            }),
        };
        let Some(value) = output else {
            return Err(failure("no structured output".to_owned()));
        };
        self.validator.validate(value).map_err(|errors| {
            failure(
                errors
                    .map(|error| format!("schema {} at {}", error.schema_path, error.instance_path))
                    .collect::<Vec<_>>()
                    .join("; "),
            )
        })
    }
}
