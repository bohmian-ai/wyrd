//! Agent model calls through a governed Wyrd gateway.
//!
//! [`WyrdGatewayCaller`] is the narrow seam an execution environment supplies
//! to send an Agent's model calls through the Wyrd gateway instead of a native
//! provider. [`wyrd_gateway_registry`] wraps one caller in a one-adapter
//! [`ProviderRegistry`] keyed by the Prompt's provider, so the existing Agent
//! loop — tools, callbacks, sessions, structured output — dispatches every
//! call to the gateway through [`crate::Agent::with_provider_registry`].

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use skald_providers::{ProviderError, ProviderStream};
use skald_runtime::{Provider, ProviderRegistry, SkaldRuntimeError};
use skald_spec::{Prompt, ProviderName, ProviderRequest, ProviderResponse};
use tokio_util::sync::CancellationToken;
use wyrd_spec::gateway::ModelRef;
use wyrd_spec::ids::{ModelId, ProviderDeploymentName};

use crate::AgentError;

/// Budget every gateway call carries, so a caller always receives a finite
/// timeout.
pub const DEFAULT_GATEWAY_CALL_TIMEOUT: Duration = Duration::from_mins(10);

/// Gateway outcome codes a caller may retry; every other gateway code is
/// terminal regardless of HTTP status.
const RETRYABLE_GATEWAY_CODES: [&str; 3] = [
    "WYRD_GATEWAY_429_LIMIT_EXCEEDED",
    "WYRD_GATEWAY_502_UPSTREAM_UNAVAILABLE",
    "WYRD_GATEWAY_504_DEADLINE_EXCEEDED",
];

/// One governed model call submitted to a [`WyrdGatewayCaller`].
#[derive(Debug, Clone)]
pub struct WyrdGatewayCall {
    /// Native request exactly as the Agent loop produced it.
    pub request: ProviderRequest,
    /// Gateway address: the route's deployment and the Prompt's model.
    pub model: ModelRef,
    /// Prompt provider, which bounds the adapters the requested deployment
    /// may use.
    pub provider: ProviderName,
    /// Remaining time the call may take; the caller enforces it.
    pub timeout: Duration,
}

/// Narrow seam to the execution environment's governed Wyrd gateway.
///
/// Implementations submit one native request within its timeout, stop
/// promptly when `cancellation` fires, and return failures as
/// [`ProviderError`] — a remote Wyrd problem as
/// [`ProviderError::RemoteProblem`] — so callers can classify them with
/// [`agent_error_retryable`].
#[async_trait]
pub trait WyrdGatewayCaller: Send + Sync {
    /// Submit one gateway call.
    ///
    /// # Errors
    ///
    /// Returns the provider-layer failure of the call.
    async fn call(
        &self,
        call: WyrdGatewayCall,
        cancellation: &CancellationToken,
    ) -> Result<ProviderResponse, ProviderError>;
}

/// Build a provider registry serving `prompt`'s model calls through `caller`
/// at `<deployment>/<prompt model>`.
///
/// It holds one gateway adapter keyed by the Prompt's provider; each call
/// carries [`DEFAULT_GATEWAY_CALL_TIMEOUT`] and a never-cancelled token. The
/// deployment comes from the Agent's execution route and is never derived
/// from the Prompt provider. Returns `None` when the Prompt model is not a
/// gateway model identifier.
#[must_use]
pub fn wyrd_gateway_registry(
    caller: Arc<dyn WyrdGatewayCaller>,
    prompt: &Prompt,
    deployment: &ProviderDeploymentName,
) -> Option<ProviderRegistry> {
    let model = ModelRef {
        deployment: deployment.clone(),
        model: ModelId::new(&prompt.model).ok()?,
    };
    Some(ProviderRegistry::new().with(Arc::new(WyrdGatewayProvider {
        caller,
        model,
        cancellation: CancellationToken::new(),
        provider: prompt.provider(),
    })))
}

/// Return whether an Agent failure is eligible for another attempt.
///
/// Provider connection, timeout, decode, and 408/429/5xx failures, response
/// decode failures, and Agent timeouts are retryable; gateway problems retry
/// only for the three capacity/upstream/deadline codes. Everything else —
/// auth, permission, tool, callback, session, journal, max-iteration,
/// structured-output exhaustion, and invariant failures — is terminal.
#[must_use]
pub fn agent_error_retryable(error: &AgentError) -> bool {
    match error {
        AgentError::Provider(SkaldRuntimeError::Provider { source, .. }) => {
            provider_error_retryable(source)
        }
        AgentError::Provider(SkaldRuntimeError::ResponseDecode { .. })
        | AgentError::Timeout { .. } => true,
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
        ProviderError::RemoteProblem(problem) => {
            RETRYABLE_GATEWAY_CODES.contains(&problem.code.as_str())
        }
        _ => false,
    }
}

/// Provider adapter submitting an Agent's model calls to the Wyrd gateway.
struct WyrdGatewayProvider {
    /// Gateway caller.
    caller: Arc<dyn WyrdGatewayCaller>,
    /// Gateway model identity of the Agent's Prompt.
    model: ModelRef,
    /// Cancellation forwarded to the caller.
    cancellation: CancellationToken,
    /// Provider name the adapter registers under.
    provider: ProviderName,
}

#[async_trait]
impl Provider for WyrdGatewayProvider {
    /// Submit the request with the default gateway call budget.
    ///
    /// # Errors
    ///
    /// Returns the caller's failure.
    async fn send(&self, request: ProviderRequest) -> Result<ProviderResponse, ProviderError> {
        let call = WyrdGatewayCall {
            request,
            model: self.model.clone(),
            provider: self.provider.clone(),
            timeout: DEFAULT_GATEWAY_CALL_TIMEOUT,
        };
        self.caller.call(call, &self.cancellation).await
    }

    /// Streaming is not part of the gateway route contract.
    ///
    /// # Errors
    ///
    /// Always returns [`ProviderError::BadRequest`].
    async fn stream(&self, _request: ProviderRequest) -> Result<ProviderStream, ProviderError> {
        Err(ProviderError::bad_request(
            "wyrd_gateway",
            "streaming is not supported on gateway routes",
        ))
    }

    /// The Prompt's provider, so registry dispatch reaches this adapter.
    fn name(&self) -> ProviderName {
        self.provider.clone()
    }
}
