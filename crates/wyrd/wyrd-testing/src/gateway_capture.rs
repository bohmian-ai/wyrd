//! A bound server whose gateway captures calls answered by a mock upstream.
//!
//! `vala.gateway.calls` accepts only the reserved gateway capture principal,
//! so a journey that needs real call rows drives a real chat completion
//! through the gateway against a local `wiremock` provider.

use serde_json::{Value, json};
use url::Url;
use wiremock::matchers::{body_partial_json, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};
use wyrd_runtime::Permission;
use wyrd_spec::DataTenantId;
use wyrd_spec::auth::GatewayAccess;

use crate::{Bootstrap, WyrdTestServer};

/// Error returned by the gateway capture fixture.
pub type GatewayCaptureError = Box<dyn std::error::Error + Send + Sync>;

/// The captured gateway calls and the JSON the resolved one sent and received.
pub struct CapturedCall {
    /// Request id the gateway answered with, which the captured row carries.
    pub request_id: String,
    /// Request id of a second call the upstream refused, so its captured row
    /// has no resolved model.
    pub unresolved_request_id: String,
    /// Body the caller sent.
    pub request: Value,
    /// Body the upstream provider answered with.
    pub response: Value,
}

/// One bound server with a mock gateway upstream, and its default tenant.
pub struct GatewayCapture {
    /// Bound server whose gateway adapters target `upstream`.
    pub server: WyrdTestServer,
    /// Mock `OpenAI` provider answering the captured call.
    pub upstream: MockServer,
    /// Tenant every captured row belongs to.
    pub tenant: DataTenantId,
}

impl GatewayCapture {
    /// Buffered Chat Completions answer the mock upstream returns.
    pub fn completion() -> Value {
        json!({
            "id": "chatcmpl-typed",
            "object": "chat.completion",
            "created": 1,
            "model": "gpt-4o",
            "choices": [{"index": 0, "message": {"role": "assistant", "content": "hi"}, "finish_reason": "stop", "logprobs": null}],
            "usage": {"prompt_tokens": 11, "completion_tokens": 4, "total_tokens": 15},
        })
    }

    /// Start a bound server whose gateway providers resolve to a mock
    /// upstream that answers every chat completion, except that one asking
    /// for a single completion token is refused, so no model resolves it.
    ///
    /// # Errors
    /// Returns a mock URL or server start error.
    pub async fn start() -> Result<Self, GatewayCaptureError> {
        let upstream = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .and(body_partial_json(json!({"max_completion_tokens": 1})))
            .respond_with(ResponseTemplate::new(400).set_body_json(json!({
                "error": {"message": "refused", "type": "invalid_request_error"},
            })))
            .with_priority(1)
            .mount(&upstream)
            .await;
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(Self::completion()))
            .mount(&upstream)
            .await;
        let server = Box::pin(
            WyrdTestServer::builder()
                .with_gateway_provider_root_for_test(Url::parse(&upstream.uri())?)
                .start_bound(),
        )
        .await?;
        let tenant = server.data_tenant_id();
        Ok(Self {
            server,
            upstream,
            tenant,
        })
    }

    /// Configure a provider and a request-and-response payload capture
    /// policy, then invoke one chat completion as an ordinary caller.
    ///
    /// # Errors
    /// Returns a seeding, exchange, administration, or call error.
    pub async fn capture_call(&self) -> Result<CapturedCall, GatewayCaptureError> {
        self.server
            .seed_role(
                "typed_invoker",
                &[Permission::gateway_invoke(GatewayAccess::Provider {
                    provider: "openai".parse()?,
                })],
            )
            .await?;
        let admin = self.exchange("typed_gateway_admin", &["admin"]).await?;
        let caller = self
            .exchange("typed_gateway_caller", &["typed_invoker"])
            .await?;
        let http = reqwest::Client::new();
        let base = self.server.base_url().ok_or("the server is not bound")?;
        for (route, body) in [
            (
                "provider-credentials/openai-key",
                json!({
                    "name": "openai-key",
                    "provider": "openai",
                    "source": {"managed_secret": {"secret": "sk-typed-upstream"}},
                }),
            ),
            (
                "provider-deployments/gpt-4o",
                json!({
                    "name": "gpt-4o",
                    "model": {"provider": "openai", "model": "gpt-4o"},
                    "adapter": "openai",
                    "auth": {"bearer": {"credential": "openai-key"}},
                    "capabilities": ["chat_completions"],
                    "routing_weight": 1,
                }),
            ),
            (
                "capture-policy",
                json!({"mode": "payload", "payload_fields": ["request", "response"]}),
            ),
        ] {
            let response = http
                .put(format!("{base}/v1/admin/gateway/{route}"))
                .header("x-wyrd-access-token", format!("Bearer {admin}"))
                .json(&body)
                .send()
                .await?;
            if !response.status().is_success() {
                return Err(
                    format!("{route}: {} {}", response.status(), response.text().await?).into(),
                );
            }
        }
        let request = json!({
            "model": "openai/gpt-4o",
            "max_completion_tokens": 16,
            "messages": [
                {"role": "system", "content": "be brief"},
                {"role": "user", "content": "hi"},
            ],
        });
        let answer = http
            .post(format!("{base}/v1/chat/completions"))
            .header("authorization", format!("Bearer {caller}"))
            .json(&request)
            .send()
            .await?;
        if answer.status().as_u16() != 200 {
            return Err(format!(
                "the call failed: {} {}",
                answer.status(),
                answer.text().await?
            )
            .into());
        }
        let request_id = answer
            .headers()
            .get("wyrd-request-id")
            .and_then(|value| value.to_str().ok())
            .ok_or("the answer carries no request id")?
            .to_owned();
        if self
            .upstream
            .received_requests()
            .await
            .is_none_or(|calls| calls.is_empty())
        {
            return Err("the call never reached the mock upstream".into());
        }
        let refused = http
            .post(format!("{base}/v1/chat/completions"))
            .header("authorization", format!("Bearer {caller}"))
            .json(&json!({
                "model": "openai/gpt-4o",
                "max_completion_tokens": 1,
                "messages": [{"role": "user", "content": "hi"}],
            }))
            .send()
            .await?;
        if refused.status().is_success() {
            return Err("the upstream refusal reached the caller as a success".into());
        }
        let unresolved_request_id = refused
            .headers()
            .get("wyrd-request-id")
            .and_then(|value| value.to_str().ok())
            .ok_or("the refused answer carries no request id")?
            .to_owned();
        Ok(CapturedCall {
            request_id,
            unresolved_request_id,
            request,
            response: Self::completion(),
        })
    }

    /// Bootstrap a service holding `roles` and exchange its key for a token.
    ///
    /// # Errors
    /// Returns the bootstrap or exchange error, or a user bootstrap.
    pub async fn exchange(
        &self,
        name: &str,
        roles: &[&str],
    ) -> Result<String, GatewayCaptureError> {
        let Bootstrap::Machine { api_key, .. } = self.server.bootstrap_service(name, roles).await?
        else {
            return Err("a service bootstrap returned a user".into());
        };
        Ok(self.server.exchange_api_key(&api_key).await?)
    }
}
