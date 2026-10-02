//! Classification of one Workflow step attempt.
//!
//! One attempt covers the Agent loop, response normalization, and declared
//! output validation; provider-internal retries happen inside it. This module
//! turns the Agent's result into either a step payload or a safe projected
//! error plus whether the Workflow retry policy may begin another attempt.

use serde_json::Value;
use skald_agent::{AgentError, AgentResult, AgentRun, FinishReason};
use skald_providers::ProviderError;
use skald_runtime::SkaldRuntimeError;
use wyrd_spec::card::workflow::{WorkflowRunError, jcs_len};
use wyrd_spec::error::WyrdError;

use crate::output::OutputValidator;

/// Gateway outcome codes that a Workflow attempt may retry; every other
/// gateway code is terminal regardless of HTTP status.
const RETRYABLE_GATEWAY_CODES: [&str; 3] = [
    "WYRD_GATEWAY_429_LIMIT_EXCEEDED",
    "WYRD_GATEWAY_502_UPSTREAM_UNAVAILABLE",
    "WYRD_GATEWAY_504_DEADLINE_EXCEEDED",
];

/// Output retained for a succeeded step.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct StepPayload {
    /// Final text, for a text-response Prompt.
    pub(crate) text: Option<String>,
    /// Structured output, for a JSON-schema Prompt.
    pub(crate) structured: Option<Value>,
}

impl StepPayload {
    /// Bytes charged against step and run budgets: UTF-8 text length plus the
    /// JCS size of the structured value.
    pub(crate) fn charged_bytes(&self) -> usize {
        let text = self.text.as_ref().map_or(0, String::len);
        let structured = self.structured.as_ref().map_or(0, jcs_len);
        text.saturating_add(structured)
    }
}

/// Result of one finished attempt.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum AttemptOutcome {
    /// The step produced its payload.
    Succeeded(StepPayload),
    /// The attempt failed.
    Failed {
        /// Safe, bounded projection of the failure.
        error: WorkflowRunError,
        /// Whether the Workflow retry policy may begin another attempt.
        retryable: bool,
    },
}

impl AttemptOutcome {
    /// Build a failed outcome from a catalog error.
    pub(crate) fn failed(error: &WyrdError, retryable: bool) -> Self {
        Self::Failed {
            error: WorkflowRunError::from_wyrd(error),
            retryable,
        }
    }

    /// Classify the Agent's result for step `step_id`.
    ///
    /// A run that ended for any reason other than the model stopping is a
    /// terminal failure carrying the run's own error. A JSON-schema Prompt
    /// keeps only its structured output, validated against `validator`
    /// (retryable on violation); a text Prompt keeps only its text. A payload
    /// above `max_step_result_bytes` is a terminal failure and is discarded.
    pub(crate) fn from_agent(
        step_id: &str,
        result: AgentResult<AgentRun>,
        validator: Option<&OutputValidator>,
        max_step_result_bytes: Option<usize>,
    ) -> Self {
        let run = match result {
            Ok(run) => run,
            Err(error) => {
                return Self::Failed {
                    error: project_agent_error(&error),
                    retryable: agent_error_retryable(&error),
                };
            }
        };
        if run.finish_reason != FinishReason::ModelStopped {
            let error = run.error.unwrap_or_else(|| WyrdError::WorkflowInternal {
                message: format!(
                    "step '{step_id}' agent run ended with {:?}",
                    run.finish_reason
                ),
                details: serde_json::json!({ "step": step_id }),
            });
            return Self::failed(&error, false);
        }
        let payload = match validator {
            Some(validator) => {
                let structured = run.structured_output.map(Value::Object);
                if let Err(error) = validator.validate(step_id, structured.as_ref()) {
                    return Self::failed(&error, true);
                }
                StepPayload {
                    text: None,
                    structured,
                }
            }
            None => StepPayload {
                text: Some(run.output),
                structured: None,
            },
        };
        if let Some(limit) = max_step_result_bytes
            && payload.charged_bytes() > limit
        {
            let error = WyrdError::WorkflowStepResultTooLarge {
                message: format!("step '{step_id}' result exceeds {limit} bytes"),
                details: serde_json::json!({ "step": step_id, "limit": limit }),
            };
            return Self::failed(&error, false);
        }
        Self::Succeeded(payload)
    }
}

/// Return whether an Agent failure is eligible for another Workflow attempt.
///
/// Provider connection, timeout, decode, and 408/429/5xx failures, response
/// decode failures, Agent timeouts, and structured-output decode failures are
/// retryable; gateway problems retry only for the three capacity/upstream/
/// deadline codes. Everything else — auth, permission, binding, route, tool,
/// callback, session, journal, max-iteration, and invariant failures — is
/// terminal.
pub(crate) fn agent_error_retryable(error: &AgentError) -> bool {
    match error {
        AgentError::Provider(SkaldRuntimeError::Provider { source, .. }) => {
            provider_error_retryable(source)
        }
        AgentError::Provider(SkaldRuntimeError::ResponseDecode { .. })
        | AgentError::Timeout { .. }
        | AgentError::StructuredOutputDecode { .. } => true,
        _ => false,
    }
}

/// Return whether a provider-layer failure is retryable.
fn provider_error_retryable(error: &ProviderError) -> bool {
    match error {
        ProviderError::Connect { .. }
        | ProviderError::Timeout { .. }
        | ProviderError::Decode { .. } => true,
        ProviderError::Status { status, .. } | ProviderError::Upstream { status, .. } => {
            matches!(status, 408 | 429 | 500..=599)
        }
        ProviderError::RemoteProblem { code, .. } => {
            RETRYABLE_GATEWAY_CODES.contains(&code.as_str())
        }
        _ => false,
    }
}

/// Project an Agent failure into a safe, bounded run error.
///
/// A remote Wyrd problem keeps its stable code, message, remediation, and only
/// its safe `field`. Other provider failures name only their stable provider
/// code, never the provider body. Every other failure uses the Agent's catalog
/// projection.
pub(crate) fn project_agent_error(error: &AgentError) -> WorkflowRunError {
    match error {
        AgentError::Provider(SkaldRuntimeError::Provider {
            source:
                ProviderError::RemoteProblem {
                    code,
                    message,
                    field,
                    remediation,
                    ..
                },
            ..
        }) => WorkflowRunError {
            code: code.clone(),
            message: message.clone(),
            details: field.as_ref().map_or_else(
                || serde_json::json!({}),
                |field| serde_json::json!({ "field": field }),
            ),
            remediation: remediation.clone(),
        }
        .bounded(),
        AgentError::Provider(source) => {
            let code = source.code();
            WorkflowRunError::from_wyrd(&WyrdError::AgentProviderCall {
                message: format!("provider call failed: {code}"),
                details: serde_json::json!({ "provider_code": code }),
            })
        }
        other => WorkflowRunError::from_wyrd(&WyrdError::from(other)),
    }
}
