//! A bound server whose gateway captures calls answered by a mock upstream,
//! and the upstream answers gateway journeys and capacity benchmarks share.
//!
//! `bifrost.gateway.calls` accepts only the reserved gateway capture principal,
//! so a journey that needs real call rows drives a real chat completion
//! through the gateway against a local `wiremock` provider.
//!
//! Every answer an `OpenAI`-compatible upstream gives is built here: the
//! buffered completion ([`GatewayCapture::completion`], or a sized one), and
//! the Chat Completions stream ([`GatewayCapture::chat_events`]) whose
//! frames [`PacedUpstream`] writes one at a time. `wiremock` returns one
//! whole body after one delay, so it cannot pace a stream; [`PacedUpstream`]
//! is the bounded local provider that does, recording when it sent each
//! frame. [`pick`] reads only the request and answer members a fixture
//! judges or routes by, under a decoded value's rules.

use std::borrow::Cow;
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
use serde::Deserialize;
use serde::de::{DeserializeSeed, Deserializer, MapAccess, SeqAccess, Visitor};
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
    /// Request id the gateway answered with, which the captured row carries
    /// in its payload `request_id` column.
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
    /// `content_bytes` bytes of [`Self::prose`], for answers of a chosen size.
    pub fn sized_completion(content_bytes: usize) -> Value {
        let mut completion = Self::completion();
        completion["choices"][0]["message"]["content"] = json!(Self::prose(content_bytes));
        completion
    }

    /// Exactly `bytes` bytes of ASCII English-like text, identical on every
    /// call, so sized messages compress like chat rather than like one
    /// repeated byte.
    pub fn prose(bytes: usize) -> String {
        const WORDS: [&str; 64] = [
            "the",
            "model",
            "returns",
            "a",
            "summary",
            "of",
            "each",
            "request",
            "and",
            "its",
            "context",
            "when",
            "users",
            "ask",
            "about",
            "billing",
            "orders",
            "shipping",
            "we",
            "should",
            "check",
            "account",
            "status",
            "before",
            "answering",
            "please",
            "confirm",
            "your",
            "email",
            "address",
            "so",
            "that",
            "I",
            "can",
            "look",
            "up",
            "recent",
            "invoices",
            "refund",
            "policy",
            "allows",
            "returns",
            "within",
            "thirty",
            "days",
            "after",
            "delivery",
            "if",
            "item",
            "is",
            "unused",
            "otherwise",
            "contact",
            "support",
            "team",
            "for",
            "help",
            "with",
            "this",
            "issue",
            "today",
            "thanks",
            "again",
            "next",
        ];
        let mut text = String::with_capacity(bytes + 16);
        let mut state: u64 = 0x9e37_79b9_7f4a_7c15;
        let mut sentence = 0;
        while text.len() < bytes {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            // The modulus keeps the index below `WORDS.len()`.
            let word = WORDS[usize::try_from(state % 64).unwrap_or_default()];
            text.push_str(word);
            sentence += 1;
            text.push_str(if sentence % 12 == 0 { ". " } else { " " });
        }
        text.truncate(bytes);
        text
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
    /// The buffered answer's body, identical on every call, so it is built
    /// once.
    buffered: Bytes,
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
                buffered: Bytes::from(
                    GatewayCapture::sized_completion(pacing.buffered_bytes).to_string(),
                ),
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

/// Waits `delay`, or not at all when it is zero, so an instant answer
/// costs no timer tick.
async fn pause(delay: Duration) {
    if !delay.is_zero() {
        tokio::time::sleep(delay).await;
    }
}

/// Path of a chat request's marker: its first message's content.
const MARKER: &[Step] = &[Step::Key("messages"), Step::Index(0), Step::Key("content")];

/// Path of a chat request's `stream` member.
const STREAM: &[Step] = &[Step::Key("stream")];

/// Answers one chat completion as [`PacedUpstream`] documents; a body that
/// is not a JSON chat request is refused with `400`.
async fn answer(State(paced): State<Paced>, body: Bytes) -> Response {
    let Ok([marker, stream]) = pick(&body, [MARKER, STREAM]) else {
        return (axum::http::StatusCode::BAD_REQUEST, "not json").into_response();
    };
    let marker = match marker {
        Some(Leaf::Str(marker)) => marker,
        _ => Cow::Borrowed(""),
    };
    let pacing = paced.pacing;
    if stream != Some(Leaf::Bool(true)) {
        pause(pacing.buffered_delay).await;
        paced.record(&marker);
        return ([(CONTENT_TYPE, "application/json")], paced.buffered).into_response();
    }
    let marker = marker.into_owned();
    pause(pacing.stream_delay).await;
    let frames = GatewayCapture::chat_event_frames(&marker, pacing.chunks);
    let events = async_stream::stream! {
        for (index, frame) in frames.into_iter().enumerate() {
            if index > 0 && index < pacing.chunks {
                pause(pacing.chunk_interval).await;
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

/// One step of a [`pick`] path: an object member or an array element.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    /// The member of this name.
    Key(&'static str),
    /// The element at this position.
    Index(usize),
}

/// The scalar [`pick`] found at a path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Leaf<'a> {
    /// A string, borrowed from the document unless it holds escapes.
    Str(Cow<'a, str>),
    /// An integer a decoded [`Value`] holds as unsigned.
    U64(u64),
    /// A Boolean.
    Bool(bool),
}

/// Reads `json` as a [`Value`] decode reads it, with the same syntax, number,
/// string, nesting, and trailing-input rules and the last of duplicate
/// members winning, but keeps only the scalar at each of `paths`, so no other
/// member is materialized and an unescaped string is borrowed from `json`.
///
/// A path holds `None` where indexing the decoded value along it reaches no
/// string, unsigned integer, or Boolean.
///
/// # Errors
///
/// Returns the JSON error of a document that does not decode as a [`Value`].
///
/// # Panics
///
/// Fails to compile for more than 64 paths.
pub fn pick<'a, const N: usize>(
    json: &'a [u8],
    paths: [&[Step]; N],
) -> serde_json::Result<[Option<Leaf<'a>>; N]> {
    const { assert!(N <= 64, "pick reads at most 64 paths") };
    let mut picked = std::array::from_fn(|_| None);
    let mut deserializer = serde_json::Deserializer::from_slice(json);
    Walk {
        paths: &paths,
        picked: &mut picked,
        matching: (0..N).fold(0, |mask, path| mask | 1 << path),
        depth: 0,
    }
    .deserialize(&mut deserializer)?;
    deserializer.end()?;
    Ok(picked)
}

/// One value of [`pick`]'s document at `depth`, under which the paths of the
/// `matching` mask continue.
struct Walk<'s, 'de> {
    /// Every requested path.
    paths: &'s [&'s [Step]],
    /// The scalar found for each path so far.
    picked: &'s mut [Option<Leaf<'de>>],
    /// Paths this value lies on, one bit per path.
    matching: u64,
    /// Steps from the document to this value.
    depth: usize,
}

impl<'de> Walk<'_, 'de> {
    /// The matching paths that `keep` accepts.
    fn select(&self, keep: impl Fn(&[Step]) -> bool) -> u64 {
        (0..self.paths.len())
            .filter(|&index| self.matching & 1 << index != 0 && keep(self.paths[index]))
            .fold(0, |mask, index| mask | 1 << index)
    }

    /// A walk of this value's child on the paths of `matching`.
    fn child(&mut self, matching: u64) -> Walk<'_, 'de> {
        Walk {
            paths: self.paths,
            picked: &mut *self.picked,
            matching,
            depth: self.depth + 1,
        }
    }

    /// Stores the scalar `leaf` makes for every path ending at this value.
    fn leaf(self, leaf: impl FnOnce() -> Option<Leaf<'de>>) {
        let ending = self.select(|path| path.len() == self.depth);
        if ending != 0 {
            let leaf = leaf();
            for (index, picked) in self.picked.iter_mut().enumerate() {
                if ending & 1 << index != 0 {
                    picked.clone_from(&leaf);
                }
            }
        }
    }
}

impl<'de> DeserializeSeed<'de> for Walk<'_, 'de> {
    type Value = ();

    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<(), D::Error> {
        deserializer.deserialize_any(self)
    }
}

impl<'de> Visitor<'de> for Walk<'_, 'de> {
    type Value = ();

    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("a JSON value")
    }

    fn visit_bool<E>(self, value: bool) -> Result<(), E> {
        self.leaf(|| Some(Leaf::Bool(value)));
        Ok(())
    }

    fn visit_i64<E>(self, value: i64) -> Result<(), E> {
        self.leaf(|| u64::try_from(value).ok().map(Leaf::U64));
        Ok(())
    }

    fn visit_u64<E>(self, value: u64) -> Result<(), E> {
        self.leaf(|| Some(Leaf::U64(value)));
        Ok(())
    }

    fn visit_f64<E>(self, _: f64) -> Result<(), E> {
        self.leaf(|| None);
        Ok(())
    }

    fn visit_unit<E>(self) -> Result<(), E> {
        self.leaf(|| None);
        Ok(())
    }

    fn visit_borrowed_str<E>(self, value: &'de str) -> Result<(), E> {
        self.leaf(|| Some(Leaf::Str(Cow::Borrowed(value))));
        Ok(())
    }

    fn visit_str<E>(self, value: &str) -> Result<(), E> {
        self.leaf(|| Some(Leaf::Str(Cow::Owned(value.to_owned()))));
        Ok(())
    }

    fn visit_seq<A: SeqAccess<'de>>(mut self, mut seq: A) -> Result<(), A::Error> {
        let mut index = 0;
        loop {
            let under = self.select(|path| path.get(self.depth) == Some(&Step::Index(index)));
            if seq.next_element_seed(self.child(under))?.is_none() {
                return Ok(());
            }
            index += 1;
        }
    }

    fn visit_map<A: MapAccess<'de>>(mut self, mut map: A) -> Result<(), A::Error> {
        while let Some(Name(name)) = map.next_key()? {
            let under = self.select(
                |path| matches!(path.get(self.depth), Some(Step::Key(key)) if *key == name),
            );
            // A later duplicate replaces the earlier value whole.
            for (index, picked) in self.picked.iter_mut().enumerate() {
                if under & 1 << index != 0 {
                    *picked = None;
                }
            }
            map.next_value_seed(self.child(under))?;
        }
        Ok(())
    }
}

/// An object member name, borrowed from the document unless it holds escapes.
#[derive(Deserialize)]
#[serde(transparent)]
struct Name<'a>(#[serde(borrow)] Cow<'a, str>);

#[cfg(test)]
mod tests {
    use std::borrow::Cow;
    use std::time::Duration;

    use serde_json::Value;

    use super::{GatewayCapture, Leaf, MARKER, PacedUpstream, Pacing, STREAM, pick};

    /// The upstream's former projection of `body`: its marker and whether it
    /// streams, from a decoded value.
    fn decoded(body: &[u8]) -> Option<(String, bool)> {
        let request = serde_json::from_slice::<Value>(body).ok()?;
        let marker = request["messages"][0]["content"]
            .as_str()
            .unwrap_or_default()
            .to_owned();
        Some((marker, request["stream"] == Value::Bool(true)))
    }

    /// The upstream's projection of `body` through [`pick`].
    fn picked(body: &[u8]) -> Option<(Cow<'_, str>, bool)> {
        let [marker, stream] = pick(body, [MARKER, STREAM]).ok()?;
        let marker = match marker {
            Some(Leaf::Str(marker)) => marker,
            _ => Cow::Borrowed(""),
        };
        Some((marker, stream == Some(Leaf::Bool(true))))
    }

    /// The paced upstream reads a request's marker and stream flag exactly
    /// as its decoded-value projection did, over escaped, duplicate,
    /// mistyped, numeric, deep, and malformed bodies, borrowing an unescaped
    /// marker from the body; served, a non-JSON body is refused, a buffered
    /// call is answered and recorded under its marker, and only `stream:
    /// true` streams.
    ///
    /// # Panics
    ///
    /// Panics when a projection or an answer differs.
    #[tokio::test]
    async fn paced_request_projection_preserves_validation_and_routing() {
        let deep = |depth: usize| {
            format!(
                r#"{{"messages":[{{"content":"m"}}],"x":{}{}}}"#,
                "[".repeat(depth),
                "]".repeat(depth)
            )
        };
        let mut cases: Vec<String> = [
            r#"{"model":"m","messages":[{"role":"system","content":"plain"},{"content":"x"}]}"#,
            r#"{"messages":[{"content":"plain"}],"stream":true}"#,
            r#"{"messages":[{"content":"plain"}],"stream":false}"#,
            r#"{"messages":[{"content":"plain"}],"stream":"true"}"#,
            r#"{"messages":[{"content":"plain"}],"stream":1}"#,
            r#"{"messages":[{"content":"plain"}],"stream":null}"#,
            r#"{"messages":[{"content":"q\"é\n"}]}"#,
            r#"{"messages":[{"content":"a"}],"messages":[{"content":"b"}]}"#,
            r#"{"messages":[{"content":"a"}],"messages":[]}"#,
            r#"{"messages":[{"content":"a","content":7}]}"#,
            r#"{"stream":true,"stream":false,"messages":[{"content":"a"}]}"#,
            r#"{"stream":false,"stream":true,"messages":[{"content":"a"}]}"#,
            r#"{"messages":[{"content":"escaped name"}],"stream":true}"#,
            r#"{"messages":{"0":{"content":"object"}}}"#,
            r#"{"messages":[["content"]]}"#,
            r#"{"messages":[{"content":["a"]}]}"#,
            r#"{"messages":[{"content":{"text":"a"}}]}"#,
            r#"[{"messages":[{"content":"a"}]}]"#,
            r#""messages""#,
            "{}",
            r#"{"messages":[{"content":"a"}],"n":1e400}"#,
            r#"{"messages":[{"content":"a"}],"n":-0,"f":1.5,"i":-7}"#,
            r#"{"messages":[{"content":"\ud800"}]}"#,
            r#"{"messages":[{"content":"🦀"}]}"#,
            r#"{"messages":[{"content":"a"}]} "#,
            r#"{"messages":[{"content":"a"}]}x"#,
            r#"{"messages":[{"content":"a"}"#,
            "",
        ]
        .map(str::to_owned)
        .into();
        cases.extend([deep(100), deep(126), deep(127), deep(400)]);
        let mut decodable = 0;
        for case in &cases {
            let expected = decoded(case.as_bytes());
            let got = picked(case.as_bytes());
            assert_eq!(
                got.as_ref()
                    .map(|(marker, stream)| (marker.as_ref(), *stream)),
                expected
                    .as_ref()
                    .map(|(marker, stream)| (marker.as_str(), *stream)),
                "{case}"
            );
            decodable += usize::from(expected.is_some());
        }
        assert_eq!(decodable, 25, "decodable bodies");

        let body = format!(
            r#"{{"messages":[{{"content":"marker-1"}},{{"content":"{}"}}]}}"#,
            GatewayCapture::prose(65_536)
        );
        let (marker, _) = picked(body.as_bytes()).expect("decodes");
        let Cow::Borrowed(marker) = marker else {
            panic!("an unescaped marker is borrowed");
        };
        assert!(body.as_bytes().as_ptr_range().contains(&marker.as_ptr()));
        assert!(matches!(
            picked(br#"{"messages":[{"content":"a\nb"}]}"#),
            Some((Cow::Owned(_), false))
        ));

        let upstream = PacedUpstream::start(
            Pacing {
                buffered_delay: Duration::ZERO,
                buffered_bytes: 16,
                stream_delay: Duration::ZERO,
                chunks: 3,
                chunk_interval: Duration::ZERO,
            },
            8,
        )
        .await
        .expect("upstream starts");
        let http = reqwest::Client::new();
        let send = |body: String| {
            http.post(format!("{}/chat/v1/chat/completions", upstream.uri()))
                .header("content-type", "application/json")
                .body(body)
                .send()
        };
        let refused = send("not json".to_owned()).await.expect("answers");
        assert_eq!(refused.status(), 400);
        let buffered = send(body.clone()).await.expect("answers");
        assert_eq!(buffered.status(), 200);
        let answer: Value = buffered.json().await.expect("json answer");
        assert_eq!(
            answer,
            GatewayCapture::sized_completion(16),
            "the buffered answer"
        );
        assert_eq!(upstream.take("marker-1").len(), 1, "one buffered send");
        let streamed = send(r#"{"messages":[{"content":"q\"m"}],"stream":true}"#.to_owned())
            .await
            .expect("answers")
            .text()
            .await
            .expect("stream body");
        assert_eq!(streamed, GatewayCapture::chat_events("q\"m", 3));
        assert_eq!(upstream.take("q\"m").len(), 3, "one send per data event");
        let unstreamed = send(r#"{"messages":[{"content":"s"}],"stream":"true"}"#.to_owned())
            .await
            .expect("answers");
        assert_eq!(
            unstreamed.headers()["content-type"],
            "application/json",
            "only a Boolean true streams"
        );
    }
}
