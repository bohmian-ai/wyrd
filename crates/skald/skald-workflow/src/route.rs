//! Workflow model-call routes and the execution dependencies that serve them.
//!
//! A step's resolved [`LlmRoute`] selects the execution boundary for every
//! model call its Agent makes; the Prompt keeps sole ownership of provider,
//! model, request fields, and response shape. [`WorkflowExecutionDependencies`]
//! is the one owner of what an execution environment can offer: the native
//! provider registry, an optional governed Wyrd gateway caller, named external
//! gateway bindings, and the endpoint profile that screens external egress.
//!
//! Routes are checked against the environment once, before any step is
//! dispatched. Each attempt then receives a private, immutable provider adapter
//! carrying its own fallback, deadline, cancellation, and correlation, so no
//! route context is shared between steps or attempts.

use std::collections::{BTreeMap, HashMap};
use std::fmt;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use http::header::{HeaderMap, HeaderName, HeaderValue};
use secrecy::{ExposeSecret, SecretString};
use skald_providers::{EndpointPolicy, ExternalGatewayClient, ProviderError, ProviderStream};
use skald_runtime::{Provider, ProviderRegistry};
use skald_spec::{Prompt, ProviderName, ProviderRequest, ProviderResponse};
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;
use url::{Host, Url};
use wyrd_spec::card::workflow::{ExternalGatewayProtocol, LlmRoute, is_reserved_transport_header};
use wyrd_spec::error::WyrdError;
use wyrd_spec::gateway::{GatewayFallbackOverride, ModelRef};
use wyrd_spec::ids::{CredentialBindingName, ModelId, ProviderId, WorkflowRunId};

use crate::error::WorkflowResult;

/// Gateway call budget used when neither the step, the Agent, nor the run sets
/// a deadline, so a gateway caller always receives a finite timeout.
pub const DEFAULT_GATEWAY_CALL_TIMEOUT: Duration = Duration::from_mins(10);

/// Network profile for external-gateway egress.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ExternalEndpointProfile {
    /// Local or test execution: HTTPS anywhere the endpoint policy admits, and
    /// plain HTTP to loopback hosts for deterministic fixtures.
    #[default]
    Local,
    /// Production execution: HTTPS on port 443 to public addresses only.
    Production,
}

/// Correlation carried by every Wyrd gateway call a Workflow attempt makes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkflowGatewayCorrelation {
    /// Workflow run that made the call.
    pub run_id: WorkflowRunId,
    /// Step whose Agent made the call.
    pub step_id: String,
    /// Workflow attempt number, counting from 1.
    pub attempt: u32,
}

/// One governed model call submitted to a [`WyrdGatewayCaller`].
#[derive(Debug, Clone)]
pub struct WyrdGatewayCall {
    /// Native request exactly as the Agent loop produced it.
    pub request: ProviderRequest,
    /// Gateway model identity of the step's Prompt provider and model.
    pub model: ModelRef,
    /// Ordered fallback declared on the step, if any.
    pub fallback: Option<GatewayFallbackOverride>,
    /// Remaining time the call may take; the caller enforces it.
    pub timeout: Duration,
    /// Run, step, and attempt that made the call.
    pub correlation: WorkflowGatewayCorrelation,
}

/// Narrow seam to the execution environment's governed Wyrd gateway.
///
/// Implementations submit one native request with its fallback, timeout, and
/// correlation, stop promptly when `cancellation` fires, and return failures
/// as [`ProviderError`] — a remote Wyrd problem as
/// [`ProviderError::RemoteProblem`] — so the Workflow can classify them.
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

/// Execution-environment authorization for one external gateway.
///
/// The binding fixes the protocol and exact endpoint origin a Card route may
/// use and supplies the secret headers sent with each call. It is runtime-only
/// configuration: it is never serialized into a Card, schema, or result.
pub struct ExternalGatewayBinding {
    /// Name a route's `credential_binding` selects.
    pub name: CredentialBindingName,
    /// Protocol the binding authorizes.
    pub protocol: ExternalGatewayProtocol,
    /// Exact permitted origin: scheme, host, and effective port.
    pub origin: Url,
    /// Secret request headers, never logged or displayed.
    ///
    /// A hash map because [`HeaderName`] has no ordering; names are already
    /// case-normalized and unique.
    pub secret_headers: HashMap<HeaderName, SecretString>,
}

impl fmt::Debug for ExternalGatewayBinding {
    /// Formats identity, protocol, origin, and header names, never values.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ExternalGatewayBinding")
            .field("name", &self.name)
            .field("protocol", &self.protocol)
            .field("origin", &self.origin.as_str())
            .field(
                "secret_headers",
                &self.secret_headers.keys().collect::<Vec<_>>(),
            )
            .finish()
    }
}

/// External gateway bindings keyed by name.
#[derive(Debug, Default)]
pub struct ExternalGatewayBindings {
    /// Bindings keyed by their unique name.
    inner: BTreeMap<CredentialBindingName, ExternalGatewayBinding>,
}

impl ExternalGatewayBindings {
    /// Build an empty binding collection.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Add one binding after checking its origin and secret header values.
    ///
    /// # Errors
    ///
    /// Returns `WYRD_WORKFLOW_503_BINDING_UNAVAILABLE` when the name is already
    /// bound, the origin is not a bare `http`/`https` origin (no userinfo,
    /// path, query, or fragment), a secret header name is reserved for
    /// transport, routing, forwarding, proxying, or Wyrd-internal use, or a
    /// secret header value is not a valid HTTP field value. The error never includes a secret value.
    pub fn insert(&mut self, binding: ExternalGatewayBinding) -> Result<(), WyrdError> {
        let name = binding.name.as_str();
        if self.inner.contains_key(&binding.name) {
            return Err(binding_unavailable(
                name,
                "binding name is already configured",
            ));
        }
        let origin = &binding.origin;
        if !matches!(origin.scheme(), "http" | "https")
            || !origin.has_host()
            || !origin.username().is_empty()
            || origin.password().is_some()
            || origin.path() != "/"
            || origin.query().is_some()
            || origin.fragment().is_some()
        {
            return Err(binding_unavailable(
                name,
                "origin must be a bare http or https scheme, host, and port",
            ));
        }
        if binding
            .secret_headers
            .keys()
            .any(|header| is_reserved_transport_header(header.as_str()))
        {
            return Err(binding_unavailable(
                name,
                "secret headers may not set transport, routing, forwarding, proxy, or Wyrd-internal names",
            ));
        }
        if binding
            .secret_headers
            .values()
            .any(|value| HeaderValue::from_str(value.expose_secret()).is_err())
        {
            return Err(binding_unavailable(
                name,
                "secret header values must be valid HTTP field values",
            ));
        }
        self.inner.insert(binding.name.clone(), binding);
        Ok(())
    }
}

/// Everything an execution environment offers Workflow model calls.
pub struct WorkflowExecutionDependencies {
    /// Native provider registry serving `native` routes.
    native: ProviderRegistry,
    /// Governed gateway serving `wyrd_gateway` routes.
    gateway: Option<Arc<dyn WyrdGatewayCaller>>,
    /// Bindings serving `ext_gateway` routes.
    external: ExternalGatewayBindings,
    /// Egress profile for external gateways.
    profile: ExternalEndpointProfile,
}

impl fmt::Debug for WorkflowExecutionDependencies {
    /// Formats which capabilities are present without any secret material.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("WorkflowExecutionDependencies")
            .field("native_providers", &self.native.len())
            .field("wyrd_gateway", &self.gateway.is_some())
            .field("external", &self.external)
            .field("profile", &self.profile)
            .finish()
    }
}

impl WorkflowExecutionDependencies {
    /// Native-only dependencies under the [`ExternalEndpointProfile::Local`]
    /// profile.
    #[must_use]
    pub fn new(native: ProviderRegistry) -> Self {
        Self {
            native,
            gateway: None,
            external: ExternalGatewayBindings::new(),
            profile: ExternalEndpointProfile::Local,
        }
    }

    /// Serve `wyrd_gateway` routes through `gateway`.
    #[must_use]
    pub fn with_wyrd_gateway(mut self, gateway: Arc<dyn WyrdGatewayCaller>) -> Self {
        self.gateway = Some(gateway);
        self
    }

    /// Serve `ext_gateway` routes through `bindings`.
    #[must_use]
    pub fn with_external_gateways(mut self, bindings: ExternalGatewayBindings) -> Self {
        self.external = bindings;
        self
    }

    /// Set the egress profile for external gateways.
    #[must_use]
    pub fn with_endpoint_profile(mut self, profile: ExternalEndpointProfile) -> Self {
        self.profile = profile;
        self
    }

    /// Borrow the native provider registry.
    pub(crate) fn native(&self) -> &ProviderRegistry {
        &self.native
    }

    /// Check one step's route against this environment and build its route
    /// state.
    ///
    /// `field` names the route in errors. The request dialect was already
    /// matched to an external protocol during resolved validation.
    ///
    /// # Errors
    ///
    /// Returns `WYRD_WORKFLOW_503_BINDING_UNAVAILABLE` for a gateway route
    /// without a caller or an unknown external binding, and
    /// `WYRD_WORKFLOW_422_ROUTE_UNSUPPORTED` when a gateway route's Prompt
    /// provider and model are not a gateway model identity, the binding's protocol or
    /// origin differs from the route, the scheme is not allowed by the
    /// profile, an authored header is invalid or collides with a secret
    /// header, or the endpoint policy refuses the base URL.
    pub(crate) fn resolve_route(
        &self,
        field: &str,
        route: &LlmRoute,
        fallback: Option<&GatewayFallbackOverride>,
        prompt: &Prompt,
    ) -> WorkflowResult<StepRoute> {
        match route {
            LlmRoute::Native => Ok(StepRoute::Native),
            LlmRoute::WyrdGateway => {
                let caller =
                    self.gateway
                        .as_ref()
                        .ok_or_else(|| WyrdError::WorkflowBindingUnavailable {
                            message: format!("{field}: no Wyrd gateway is available"),
                            details: serde_json::json!({ "field": field }),
                        })?;
                let model = gateway_model(&prompt.provider(), &prompt.model).ok_or_else(|| {
                    route_unsupported(
                        field,
                        "Prompt provider and model are not a gateway model identity",
                    )
                })?;
                Ok(StepRoute::WyrdGateway {
                    caller: Arc::clone(caller),
                    fallback: fallback.cloned(),
                    model,
                })
            }
            LlmRoute::ExtGateway {
                protocol,
                base_url,
                headers,
                credential_binding,
            } => {
                let binding = self.external.inner.get(credential_binding).ok_or_else(|| {
                    WyrdError::WorkflowBindingUnavailable {
                        message: format!("{field}: external gateway binding is not configured"),
                        details: serde_json::json!({
                            "field": field,
                            "binding": credential_binding.as_str(),
                        }),
                    }
                })?;
                let url = Url::parse(base_url.as_str())
                    .map_err(|_| route_unsupported(field, "base_url is not an absolute URL"))?;
                if binding.protocol != *protocol {
                    return Err(
                        route_unsupported(field, "binding protocol differs from route").into(),
                    );
                }
                if url.origin() != binding.origin.origin() {
                    return Err(
                        route_unsupported(field, "base_url origin differs from binding").into(),
                    );
                }
                if url.scheme() != "https"
                    && !(self.profile == ExternalEndpointProfile::Local && is_loopback(&url))
                {
                    return Err(route_unsupported(
                        field,
                        "HTTPS is required outside loopback local execution",
                    )
                    .into());
                }
                let headers = merged_headers(field, headers, binding)?;
                let policy =
                    EndpointPolicy::new(self.profile == ExternalEndpointProfile::Production);
                let client = ExternalGatewayClient::new(policy, url, headers)
                    .map_err(|_| route_unsupported(field, "endpoint policy refuses base_url"))?;
                Ok(StepRoute::External {
                    client: Arc::new(client),
                })
            }
        }
    }
}

/// Route state for one step, checked against the environment before dispatch.
#[derive(Clone)]
pub(crate) enum StepRoute {
    /// Use the native provider registry.
    Native,
    /// Submit each model call to the governed Wyrd gateway.
    WyrdGateway {
        /// Gateway caller.
        caller: Arc<dyn WyrdGatewayCaller>,
        /// Step fallback forwarded with every call.
        fallback: Option<GatewayFallbackOverride>,
        /// Gateway model identity of the step's Prompt.
        model: ModelRef,
    },
    /// Post each model call to a bound external gateway.
    External {
        /// Screened client fixed to the bound origin and headers.
        client: Arc<ExternalGatewayClient>,
    },
}

/// Per-attempt context a route adapter carries.
pub(crate) struct AttemptRouteContext {
    /// Provider the Prompt targets; the adapter registers under this name.
    pub(crate) provider: ProviderName,
    /// Prompt model, used in the request path of Google dialects.
    pub(crate) model: String,
    /// Earliest of the step, Agent, and run deadlines.
    pub(crate) deadline: Option<Instant>,
    /// Run cancellation.
    pub(crate) cancellation: CancellationToken,
    /// Run, step, and attempt correlation.
    pub(crate) correlation: WorkflowGatewayCorrelation,
}

impl StepRoute {
    /// Build the private provider registry for one attempt.
    ///
    /// Returns `None` for native routes, which use the shared native registry
    /// unchanged. Gateway routes get a one-adapter registry keyed by the
    /// Prompt's provider, so the existing Agent loop dispatches to it.
    pub(crate) fn attempt_registry(
        &self,
        context: AttemptRouteContext,
    ) -> Option<ProviderRegistry> {
        let adapter: Arc<dyn Provider> = match self {
            Self::Native => return None,
            Self::WyrdGateway {
                caller,
                fallback,
                model,
            } => Arc::new(WyrdGatewayProvider {
                caller: Arc::clone(caller),
                fallback: fallback.clone(),
                model: model.clone(),
                deadline: context.deadline,
                cancellation: context.cancellation,
                correlation: context.correlation,
                provider: context.provider,
            }),
            Self::External { client } => Arc::new(ExternalGatewayProvider {
                client: Arc::clone(client),
                model: context.model,
                provider: context.provider,
            }),
        };
        Some(ProviderRegistry::new().with(adapter))
    }
}

/// Return true when `request`'s native dialect is the one `protocol` accepts.
///
/// Gemini and Vertex gateways both accept a Google GenerateContent body.
pub(crate) fn protocol_matches(
    protocol: ExternalGatewayProtocol,
    request: &ProviderRequest,
) -> bool {
    matches!(
        (protocol, request),
        (
            ExternalGatewayProtocol::OpenAiChat,
            ProviderRequest::OpenAiChatCompletion(_)
        ) | (
            ExternalGatewayProtocol::OpenAiResponses,
            ProviderRequest::OpenAiResponses(_)
        ) | (
            ExternalGatewayProtocol::AnthropicMessages,
            ProviderRequest::AnthropicMessage(_)
        ) | (
            ExternalGatewayProtocol::GeminiGenerateContent
                | ExternalGatewayProtocol::VertexGenerateContent,
            ProviderRequest::GeminiGenerateContent(_)
        )
    )
}

/// Build a route-unsupported error naming `field` and a safe reason.
pub(crate) fn route_unsupported(field: &str, reason: &str) -> WyrdError {
    WyrdError::WorkflowRouteUnsupported {
        message: format!("{field}: {reason}"),
        details: serde_json::json!({ "field": field, "reason": reason }),
    }
}

/// Project a Prompt's provider and model onto the gateway model identity.
///
/// Built-in providers map to the gateway's built-in provider IDs (Google AI
/// Studio is `gemini`); any other provider keeps its own name. Returns `None`
/// when either half is not a valid gateway identity.
fn gateway_model(provider: &ProviderName, model: &str) -> Option<ModelRef> {
    let provider = match provider {
        ProviderName::OpenAi => "openai",
        ProviderName::Anthropic => "anthropic",
        ProviderName::Google => "gemini",
        ProviderName::Vertex => "vertex",
        ProviderName::Custom(name) => name,
    };
    Some(ModelRef {
        provider: ProviderId::new(provider).ok()?,
        model: ModelId::new(model).ok()?,
    })
}

/// Build a binding-unavailable error for binding `name`.
fn binding_unavailable(name: &str, reason: &str) -> WyrdError {
    WyrdError::WorkflowBindingUnavailable {
        message: format!("external gateway binding '{name}': {reason}"),
        details: serde_json::json!({ "binding": name, "reason": reason }),
    }
}

/// Return true when `url` names a loopback host.
fn is_loopback(url: &Url) -> bool {
    match url.host() {
        Some(Host::Domain(domain)) => domain.eq_ignore_ascii_case("localhost"),
        Some(Host::Ipv4(ip)) => ip.is_loopback(),
        Some(Host::Ipv6(ip)) => ip.is_loopback(),
        None => false,
    }
}

/// Merge authored route headers with the binding's sensitive secret headers.
///
/// # Errors
///
/// Returns `WYRD_WORKFLOW_422_ROUTE_UNSUPPORTED` when an authored header name
/// or value is invalid or its name collides case-insensitively with a secret
/// header, and `WYRD_WORKFLOW_503_BINDING_UNAVAILABLE` when a secret value is
/// not a valid field value.
fn merged_headers(
    field: &str,
    authored: &BTreeMap<String, String>,
    binding: &ExternalGatewayBinding,
) -> WorkflowResult<HeaderMap> {
    let mut headers = HeaderMap::new();
    for (name, value) in &binding.secret_headers {
        let mut value = HeaderValue::from_str(value.expose_secret()).map_err(|_| {
            binding_unavailable(
                binding.name.as_str(),
                "secret header values must be valid HTTP field values",
            )
        })?;
        value.set_sensitive(true);
        headers.insert(name.clone(), value);
    }
    for (name, value) in authored {
        let header_name = HeaderName::from_bytes(name.as_bytes())
            .map_err(|_| route_unsupported(field, "header name is not a valid HTTP field name"))?;
        if headers.contains_key(&header_name) {
            return Err(route_unsupported(
                field,
                "authored header collides with a bound secret header",
            )
            .into());
        }
        let header_value = HeaderValue::from_str(value).map_err(|_| {
            route_unsupported(field, "header value is not a valid HTTP field value")
        })?;
        headers.insert(header_name, header_value);
    }
    Ok(headers)
}

/// Provider adapter submitting one attempt's model calls to the Wyrd gateway.
struct WyrdGatewayProvider {
    /// Gateway caller.
    caller: Arc<dyn WyrdGatewayCaller>,
    /// Step fallback.
    fallback: Option<GatewayFallbackOverride>,
    /// Gateway model identity of the step's Prompt.
    model: ModelRef,
    /// Absolute deadline for every call in this attempt.
    deadline: Option<Instant>,
    /// Run cancellation forwarded to the caller.
    cancellation: CancellationToken,
    /// Attempt correlation.
    correlation: WorkflowGatewayCorrelation,
    /// Provider name the adapter registers under.
    provider: ProviderName,
}

#[async_trait]
impl Provider for WyrdGatewayProvider {
    /// Submit the request with the attempt's remaining time.
    ///
    /// # Errors
    ///
    /// Returns [`ProviderError::Timeout`] when the deadline already passed,
    /// otherwise the caller's failure.
    async fn send(&self, request: ProviderRequest) -> Result<ProviderResponse, ProviderError> {
        let timeout = match self.deadline {
            Some(deadline) => {
                let remaining = deadline.saturating_duration_since(Instant::now());
                if remaining.is_zero() {
                    return Err(ProviderError::Timeout {
                        provider: "wyrd_gateway".to_owned(),
                    });
                }
                remaining
            }
            None => DEFAULT_GATEWAY_CALL_TIMEOUT,
        };
        let call = WyrdGatewayCall {
            request,
            model: self.model.clone(),
            fallback: self.fallback.clone(),
            timeout,
            correlation: self.correlation.clone(),
        };
        self.caller.call(call, &self.cancellation).await
    }

    /// Streaming is not part of the Workflow route contract.
    ///
    /// # Errors
    ///
    /// Always returns [`ProviderError::BadRequest`].
    async fn stream(&self, _request: ProviderRequest) -> Result<ProviderStream, ProviderError> {
        Err(ProviderError::bad_request(
            "wyrd_gateway",
            "streaming is not supported on workflow routes",
        ))
    }

    /// The Prompt's provider, so registry dispatch reaches this adapter.
    fn name(&self) -> ProviderName {
        self.provider.clone()
    }
}

/// Provider adapter posting one attempt's model calls to an external gateway.
struct ExternalGatewayProvider {
    /// Screened client.
    client: Arc<ExternalGatewayClient>,
    /// Prompt model.
    model: String,
    /// Provider name the adapter registers under.
    provider: ProviderName,
}

#[async_trait]
impl Provider for ExternalGatewayProvider {
    /// Post the native request to the bound gateway.
    ///
    /// # Errors
    ///
    /// Returns the client's transport, status, or decode failure.
    async fn send(&self, request: ProviderRequest) -> Result<ProviderResponse, ProviderError> {
        self.client.send(&self.model, request).await
    }

    /// Streaming is not part of the Workflow route contract.
    ///
    /// # Errors
    ///
    /// Always returns [`ProviderError::BadRequest`].
    async fn stream(&self, _request: ProviderRequest) -> Result<ProviderStream, ProviderError> {
        Err(ProviderError::bad_request(
            "ext_gateway",
            "streaming is not supported on workflow routes",
        ))
    }

    /// The Prompt's provider, so registry dispatch reaches this adapter.
    fn name(&self) -> ProviderName {
        self.provider.clone()
    }
}
