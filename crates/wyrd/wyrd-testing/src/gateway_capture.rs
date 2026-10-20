//! A bound server whose gateway captures calls answered by a mock upstream,
//! and the upstream answers gateway journeys and capacity benchmarks share.
//!
//! `vala.gateway.calls` accepts only the reserved gateway capture principal,
//! so a journey that needs real call rows drives a real chat completion
//! through the gateway against a local `wiremock` provider.
//!
//! Every answer an `OpenAI`-compatible upstream gives is built here: the
//! buffered completion ([`GatewayCapture::completion`], or a sized one), and
//! the Chat Completions stream ([`GatewayCapture::chat_events`]) whose
//! frames [`PacedUpstream`] writes one at a time. `wiremock` returns one
//! whole body after one delay, so it cannot pace a stream; [`PacedUpstream`]
//! is the bounded local provider that does, recording when it sent each
//! frame.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use axum::Router;
use axum::body::{Body, Bytes};
use axum::extract::State;
use axum::http::header::CONTENT_TYPE;
use axum::response::{IntoResponse, Response};
use axum::routing::post;
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

    /// A buffered Chat Completions answer whose message content is
    /// `content_bytes` bytes, for answers of a chosen size.
    pub fn sized_completion(content_bytes: usize) -> Value {
        let mut completion = Self::completion();
        completion["choices"][0]["message"]["content"] = json!("x".repeat(content_bytes));
        completion
    }

    /// The frames of one Chat Completions stream with id `id`, each one
    /// complete server-sent event: `chunks` data events of one content
    /// character each (the first naming the assistant role, the last
    /// finishing with `stop`), then a usage event and the `[DONE]` sentinel.
    pub fn chat_event_frames(id: &str, chunks: usize) -> Vec<String> {
        let mut frames: Vec<String> = (0..chunks)
            .map(|index| {
                let delta = if index == 0 {
                    json!({"role": "assistant", "content": "x"})
                } else {
                    json!({"content": "x"})
                };
                let finish = (index + 1 == chunks).then_some("stop");
                let chunk = json!({
                    "id": id,
                    "object": "chat.completion.chunk",
                    "created": 1,
                    "model": "gpt-4o",
                    "choices": [{"index": 0, "delta": delta, "finish_reason": finish}],
                });
                format!("data: {chunk}\n\n")
            })
            .collect();
        let usage = json!({
            "id": id,
            "object": "chat.completion.chunk",
            "created": 1,
            "model": "gpt-4o",
            "choices": [],
            "usage": {"prompt_tokens": 11, "completion_tokens": 4, "total_tokens": 15},
        });
        frames.push(format!("data: {usage}\n\n"));
        frames.push("data: [DONE]\n\n".to_owned());
        frames
    }

    /// One whole Chat Completions stream: [`Self::chat_event_frames`] joined.
    pub fn chat_events(id: &str, chunks: usize) -> String {
        Self::chat_event_frames(id, chunks).concat()
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
                &[Permission::gateway_invoke(GatewayAccess::Deployment {
                    deployment: "openai".parse()?,
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
                "provider-deployments/openai",
                json!({
                    "name": "openai",
                    "adapter": "openai",
                    "auth": {"bearer": {"credential": "openai-key"}},
                    "capabilities": ["chat_completions"],
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

/// How [`PacedUpstream`] answers: each answer's delays and stream length.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pacing {
    /// Wait before a buffered answer's headers and body.
    pub buffered_delay: Duration,
    /// Content bytes of a buffered answer.
    pub buffered_bytes: usize,
    /// Wait before a stream's headers and first data event.
    pub stream_delay: Duration,
    /// Data events of a stream.
    pub chunks: usize,
    /// Wait between consecutive data events of a stream.
    pub chunk_interval: Duration,
}

/// Monotonic send times [`PacedUpstream`] recorded, by call marker.
type Sends = Arc<Mutex<HashMap<String, Vec<Instant>>>>;

/// What every [`PacedUpstream`] connection shares.
#[derive(Clone)]
struct Paced {
    /// The answer shape.
    pacing: Pacing,
    /// Frame send times of calls not yet taken.
    sends: Sends,
    /// Calls whose send times may be held at once; a later call is answered
    /// but not recorded.
    bound: usize,
}

/// A bounded local `OpenAI`-compatible provider that actually delays its
/// headers and paces its stream body, one data event per write.
///
/// It serves `POST /v1/chat/completions` under any path prefix. The call's
/// marker is its first message's content, which the caller makes unique. A
/// buffered call waits [`Pacing::buffered_delay`] and answers a sized
/// completion; a `stream: true` call waits [`Pacing::stream_delay`], then
/// writes [`GatewayCapture::chat_event_frames`] with the marker as its id,
/// [`Pacing::chunk_interval`] apart. Just before handing each data event (or
/// the buffered body) to the connection it records [`Instant::now`] under
/// the marker; a handed frame is not proof of receipt. Nothing else is
/// recorded, and at most `bound` calls' times are held until
/// [`Self::take`] removes them. Dropping it stops serving.
pub struct PacedUpstream {
    /// The bound address.
    address: SocketAddr,
    /// Recorded send times.
    sends: Sends,
    /// The serving task.
    serving: tokio::task::JoinHandle<()>,
}

impl PacedUpstream {
    /// Binds a loopback port and serves `pacing`, holding at most `bound`
    /// calls' send times.
    ///
    /// # Errors
    /// Returns the bind failure.
    pub async fn start(pacing: Pacing, bound: usize) -> std::io::Result<Self> {
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0)).await?;
        let address = listener.local_addr()?;
        let sends: Sends = Arc::default();
        let router = Router::new()
            .route("/{*path}", post(answer))
            .with_state(Paced {
                pacing,
                sends: Arc::clone(&sends),
                bound,
            });
        let serving = tokio::spawn(async move {
            if let Err(error) = axum::serve(listener, router).await {
                tracing::error!(%error, "the paced upstream stopped serving");
            }
        });
        Ok(Self {
            address,
            sends,
            serving,
        })
    }

    /// The base URL every path prefix hangs off.
    pub fn uri(&self) -> String {
        format!("http://{}", self.address)
    }

    /// Removes and returns the send times recorded for `marker`: one for a
    /// buffered answer, one per data event for a stream, in send order.
    pub fn take(&self, marker: &str) -> Vec<Instant> {
        self.sends
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(marker)
            .unwrap_or_default()
    }

    /// Calls whose send times are held, for bounding evidence.
    pub fn held(&self) -> usize {
        self.sends
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .len()
    }

    /// Drops every held send time, between measured windows.
    pub fn clear(&self) {
        self.sends
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clear();
    }
}

impl Drop for PacedUpstream {
    /// Stops serving.
    fn drop(&mut self) {
        self.serving.abort();
    }
}

impl Paced {
    /// Records one send for `marker` now, unless `bound` calls are held.
    fn record(&self, marker: &str) {
        let mut sends = self.sends.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(times) = sends.get_mut(marker) {
            times.push(Instant::now());
        } else if sends.len() < self.bound {
            sends.insert(marker.to_owned(), vec![Instant::now()]);
        }
    }
}

/// Answers one chat completion as [`PacedUpstream`] documents; a body that
/// is not a JSON chat request is refused with `400`.
async fn answer(State(paced): State<Paced>, body: Bytes) -> Response {
    let Ok(request) = serde_json::from_slice::<Value>(&body) else {
        return (axum::http::StatusCode::BAD_REQUEST, "not json").into_response();
    };
    let marker = request["messages"][0]["content"]
        .as_str()
        .unwrap_or_default()
        .to_owned();
    let pacing = paced.pacing;
    if request["stream"] != Value::Bool(true) {
        tokio::time::sleep(pacing.buffered_delay).await;
        let completion = GatewayCapture::sized_completion(pacing.buffered_bytes).to_string();
        paced.record(&marker);
        return ([(CONTENT_TYPE, "application/json")], completion).into_response();
    }
    tokio::time::sleep(pacing.stream_delay).await;
    let frames = GatewayCapture::chat_event_frames(&marker, pacing.chunks);
    let events = async_stream::stream! {
        for (index, frame) in frames.into_iter().enumerate() {
            if index > 0 && index < pacing.chunks {
                tokio::time::sleep(pacing.chunk_interval).await;
            }
            if index < pacing.chunks {
                paced.record(&marker);
            }
            yield Ok::<Bytes, std::convert::Infallible>(Bytes::from(frame));
        }
    };
    (
        [(CONTENT_TYPE, "text/event-stream")],
        Body::from_stream(events),
    )
        .into_response()
}
