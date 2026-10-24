//! The gateway benchmark's tenants and the calls they make.
//!
//! A tenant administrator configures the gateway as the production guide
//! does: one `openai_compatible` deployment per upstream answer shape, each
//! under its own path prefix of the benchmark's local upstream with `auth:
//! none`, an explicit capture policy, and machine principals whose `wyrd_sk_`
//! keys every call presents. Each call is raw HTTP/1.1 on a keep-alive
//! connection to `POST /v1/chat/completions`, timed from just before the
//! request is sent until its complete answer is received and validated: a
//! buffered answer's JSON, or a stream's every server-sent event through
//! `[DONE]` and body EOF. A `200` alone is never success.

use std::collections::BTreeSet;
use std::time::{Duration, Instant};

use bytes::Bytes;
use futures_util::StreamExt as _;
use reqwest::Client;
use secrecy::{ExposeSecret as _, SecretString};
use serde_json::{Value, json};
use wyrd_client::bifrost::BifrostOptions;
use wyrd_client::config::ClientConfig;
use wyrd_client::gateway::{GatewayCaptureMode, GatewayCapturePolicyWrite, GatewayPayloadField};
use wyrd_client::{Bifrost, Gateway, GlobalConfig, Principals, WyrdClient};
use wyrd_spec::auth::CreateServicePrincipalRequest;
use wyrd_testing::capacity::request_trace;
use wyrd_testing::release_server::{SERVER_URL, SetupTenant};

use crate::gateway::Result;

/// The builtin role whose gateway invoke scope covers every model.
const INVOKER_ROLE: &str = "editor";

/// How long one call may take before the driver counts it timed out; the
/// gateway's own deadline is longer.
pub const CALL_TIMEOUT: Duration = Duration::from_secs(30);

/// The deployment path prefixes a tenant configures, one per upstream
/// answer shape.
pub const PREFIXES: [&str; 3] = ["chat", "stream", "capture"];

/// What a tenant's gateway captures.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CapturePolicy {
    /// Call metadata only.
    Metadata,
    /// Metadata plus the request and response payloads.
    Payload,
}

/// One provisioned tenant: its administrator and invoking keys.
pub struct Tenant {
    /// The tenant administrator's client.
    admin: WyrdClient,
    /// Machine-principal `wyrd_sk_` keys, one per concurrent caller.
    pub keys: Vec<SecretString>,
}

impl Tenant {
    /// Configures `setup`'s tenant against `upstream`: one deployment per
    /// [`PREFIXES`] entry and `keys` invoking principals.
    ///
    /// # Errors
    ///
    /// Returns a client, administration, or principal failure.
    pub async fn provision(setup: &SetupTenant, upstream: &str, keys: usize) -> Result<Self> {
        let admin = connect(&setup.api_key)?;
        let gateway = Gateway::connect_with(&admin).await?;
        for prefix in PREFIXES {
            let deployment = serde_json::from_value(json!({
                "name": format!("bench-{prefix}"),
                "adapter": {"openai_compatible": {"base_url": format!("{upstream}/{prefix}/v1")}},
                "auth": "none",
                "capabilities": ["chat_completions"],
            }))?;
            gateway.put_deployment(&deployment).await?;
        }
        let principals = Principals::connect_with(&admin).await?;
        let mut issued = Vec::with_capacity(keys);
        for index in 0..keys {
            let created = principals
                .create_service_principal(&CreateServicePrincipalRequest {
                    name: format!("bench-caller-{index}"),
                    roles: vec![INVOKER_ROLE.to_owned()],
                    description: None,
                    workload_restriction: None,
                })
                .await?;
            issued.push(SecretString::from(created.credential.expose().to_owned()));
        }
        Ok(Self {
            admin,
            keys: issued,
        })
    }

    /// Replaces the tenant's capture policy with `policy`.
    ///
    /// # Errors
    ///
    /// Returns the administration failure.
    pub async fn capture(&self, policy: CapturePolicy) -> Result<()> {
        let write = match policy {
            CapturePolicy::Metadata => GatewayCapturePolicyWrite {
                mode: GatewayCaptureMode::Metadata,
                payload_fields: BTreeSet::default(),
            },
            CapturePolicy::Payload => GatewayCapturePolicyWrite {
                mode: GatewayCaptureMode::Payload,
                payload_fields: [GatewayPayloadField::Request, GatewayPayloadField::Response]
                    .into_iter()
                    .collect(),
            },
        };
        Gateway::connect_with(&self.admin)
            .await?
            .put_capture_policy(&write)
            .await?;
        Ok(())
    }

    /// The captured call rows this tenant's administrator can read in
    /// `vala.gateway.calls`, which only this tenant's calls may land in.
    ///
    /// # Errors
    ///
    /// Returns the connection or query failure.
    pub async fn captured(&self) -> Result<u64> {
        let bifrost = Bifrost::connect_with(&self.admin, BifrostOptions::default()).await?;
        let result = bifrost
            .sql("SELECT count(*) AS calls FROM vala.gateway.calls", &[])
            .await?;
        let mut calls = 0;
        for batch in result.batches() {
            let column = batch.column(0);
            let counts = column
                .as_any()
                .downcast_ref::<arrow::array::Int64Array>()
                .ok_or("count(*) is not an Int64 column")?;
            calls += counts.iter().flatten().sum::<i64>();
        }
        Ok(u64::try_from(calls)?)
    }
}

/// A public client of the server presenting `credential`.
///
/// # Errors
///
/// Returns the client configuration failure.
fn connect(credential: &SecretString) -> Result<WyrdClient> {
    Ok(WyrdClient::with_config(ClientConfig {
        credential: Some(SecretString::from(credential.expose_secret().to_owned())),
        ..ClientConfig::from_global_with_overrides(&GlobalConfig::default(), Some(SERVER_URL), None)
    })?)
}

/// A keep-alive HTTP/1.1 client for the gateway's public surface.
///
/// # Errors
///
/// Returns the client build failure.
pub fn http() -> Result<Client> {
    Ok(Client::builder()
        .http1_only()
        .timeout(CALL_TIMEOUT)
        .pool_idle_timeout(None)
        .build()?)
}

/// What one call asks for and expects back.
#[derive(Debug, Clone)]
pub struct Shape {
    /// `<deployment>/<model>` the call addresses.
    pub model: String,
    /// Whether the call streams.
    pub stream: bool,
    /// Bytes of user content the request carries.
    pub request_bytes: usize,
    /// Content bytes of the buffered answer, or data events of the stream.
    pub expected: usize,
}

impl Shape {
    /// The JSON request body for one call whose first message is `marker`,
    /// which the upstream echoes into each stream event's id.
    pub fn body(&self, marker: &str) -> Bytes {
        let mut body = json!({
            "model": self.model,
            "max_completion_tokens": 64,
            "messages": [
                {"role": "system", "content": marker},
                {"role": "user", "content": "x".repeat(self.request_bytes)},
            ],
        });
        if self.stream {
            body["stream"] = json!(true);
            body["stream_options"] = json!({"include_usage": true});
        }
        Bytes::from(body.to_string())
    }
}

/// How one call ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// A complete, valid answer.
    Answered,
    /// A non-2xx gateway refusal carrying its stable code.
    Refused(String),
    /// No answer within [`CALL_TIMEOUT`].
    TimedOut,
    /// A transport, body, stream, or validation failure.
    Failed(String),
}

impl Outcome {
    /// Whether the call ended with a complete, valid answer.
    pub const fn answered(&self) -> bool {
        matches!(self, Self::Answered)
    }

    /// Whether the gateway refused the call with a stable code.
    pub const fn refused(&self) -> bool {
        matches!(self, Self::Refused(_))
    }
}

/// One call's monotonic client timings and outcome.
#[derive(Debug, Clone)]
pub struct Timed {
    /// Just before the request was sent.
    pub sent: Instant,
    /// When the response headers arrived.
    pub headers: Option<Instant>,
    /// When each complete stream data event carrying a choice arrived.
    pub events: Vec<Instant>,
    /// When the answer was complete and validated, or the call ended.
    pub done: Instant,
    /// How it ended.
    pub outcome: Outcome,
    /// The trace id the request's `traceparent` carried, which its server
    /// timing spans share.
    pub trace: String,
}

/// Sends one call of `shape` with `body` as `key` under a fresh
/// [`request_trace`] and times it to its validated end.
pub async fn call(http: &Client, key: &SecretString, shape: &Shape, body: Bytes) -> Timed {
    let trace = request_trace();
    let sent = Instant::now();
    let mut timed = Timed {
        sent,
        headers: None,
        events: Vec::new(),
        done: sent,
        outcome: Outcome::Answered,
        trace: trace.trace_id.clone(),
    };
    timed.outcome = match tokio::time::timeout(
        CALL_TIMEOUT,
        exchange(http, key, shape, body, &trace.to_traceparent(), &mut timed),
    )
    .await
    {
        Ok(outcome) => outcome,
        Err(_) => Outcome::TimedOut,
    };
    timed.done = Instant::now();
    timed
}

/// Sends the request under `traceparent` and reads its answer into
/// `timed`, returning how it ended.
async fn exchange(
    http: &Client,
    key: &SecretString,
    shape: &Shape,
    body: Bytes,
    traceparent: &str,
    timed: &mut Timed,
) -> Outcome {
    let response = match http
        .post(format!("{SERVER_URL}/v1/chat/completions"))
        .bearer_auth(key.expose_secret())
        .header("content-type", "application/json")
        .header("traceparent", traceparent)
        .body(body)
        .send()
        .await
    {
        Ok(response) => response,
        Err(error) if error.is_timeout() => return Outcome::TimedOut,
        Err(error) => return Outcome::Failed(format!("transport: {error}")),
    };
    timed.headers = Some(Instant::now());
    let status = response.status();
    if !status.is_success() {
        let text = response.text().await.unwrap_or_default();
        return refusal(status.as_u16(), &text);
    }
    if !shape.stream {
        return match response.bytes().await {
            Ok(bytes) => buffered(&bytes, shape.expected),
            Err(error) => Outcome::Failed(format!("body: {error}")),
        };
    }
    let mut events = EventReader::default();
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        match chunk {
            Ok(chunk) => {
                if let Err(problem) = events.push(&chunk, &mut timed.events) {
                    return Outcome::Failed(problem);
                }
            }
            Err(error) => return Outcome::Failed(format!("stream body: {error}")),
        }
    }
    events.finish(shape.expected)
}

/// The outcome of a non-2xx answer: a gateway refusal with its stable code
/// in the `OpenAI` error envelope, or a failure.
fn refusal(status: u16, text: &str) -> Outcome {
    serde_json::from_str::<Value>(text)
        .ok()
        .and_then(|body| body["error"]["code"].as_str().map(str::to_owned))
        .map_or_else(
            || Outcome::Failed(format!("{status} without a gateway code")),
            Outcome::Refused,
        )
}

/// Validates a buffered answer: a completion with one choice of
/// `content_bytes` content bytes and usage.
pub fn buffered(bytes: &[u8], content_bytes: usize) -> Outcome {
    let Ok(answer) = serde_json::from_slice::<Value>(bytes) else {
        return Outcome::Failed("buffered answer is not JSON".to_owned());
    };
    let content = answer["choices"][0]["message"]["content"].as_str();
    if content.map(str::len) != Some(content_bytes) {
        return Outcome::Failed("wrong buffered content".to_owned());
    }
    if !answer["usage"]["total_tokens"].is_u64() {
        return Outcome::Failed("buffered answer has no usage".to_owned());
    }
    Outcome::Answered
}

/// Reads server-sent events from arbitrary network chunks and validates the
/// stream they form.
#[derive(Debug, Default)]
pub struct EventReader {
    /// Bytes of an event not yet terminated by a blank line.
    pending: Vec<u8>,
    /// Data events carrying a choice.
    chunks: usize,
    /// Whether a choice finished with `stop`.
    finished: bool,
    /// Whether a usage event arrived.
    usage: bool,
    /// Whether `[DONE]` arrived.
    done: bool,
}

impl EventReader {
    /// Consumes `bytes`, recording now in `arrivals` for each complete data
    /// event carrying a choice.
    ///
    /// # Errors
    ///
    /// Returns why the stream is wrong: an in-band error, a frame after
    /// `[DONE]`, or a data line that is not JSON.
    pub fn push(
        &mut self,
        bytes: &[u8],
        arrivals: &mut Vec<Instant>,
    ) -> std::result::Result<(), String> {
        self.pending.extend_from_slice(bytes);
        while let Some(end) = self.pending.windows(2).position(|pair| pair == b"\n\n") {
            let event: Vec<u8> = self.pending.drain(..end + 2).collect();
            let text = std::str::from_utf8(&event).map_err(|_| "event is not UTF-8".to_owned())?;
            for line in text.lines() {
                let Some(data) = line.strip_prefix("data: ") else {
                    continue;
                };
                if self.done {
                    return Err("data after [DONE]".to_owned());
                }
                if data == "[DONE]" {
                    self.done = true;
                    continue;
                }
                let frame: Value =
                    serde_json::from_str(data).map_err(|_| "data is not JSON".to_owned())?;
                if frame.get("error").is_some() {
                    return Err(format!("in-band error: {data}"));
                }
                if let Some(choice) = frame["choices"].get(0) {
                    self.chunks += 1;
                    self.finished |= choice["finish_reason"] == "stop";
                    arrivals.push(Instant::now());
                }
                self.usage |= frame["usage"]["total_tokens"].is_u64();
            }
        }
        Ok(())
    }

    /// Judges the ended stream: exactly `chunks` choice events, a `stop`
    /// finish, usage, `[DONE]`, and nothing left unterminated.
    pub fn finish(&self, chunks: usize) -> Outcome {
        if !self.pending.iter().all(u8::is_ascii_whitespace) {
            Outcome::Failed("stream ended inside an event".to_owned())
        } else if !self.done {
            Outcome::Failed("stream ended without [DONE]".to_owned())
        } else if self.chunks != chunks || !self.finished || !self.usage {
            Outcome::Failed(format!(
                "stream had {} of {chunks} chunks, finished {}, usage {}",
                self.chunks, self.finished, self.usage
            ))
        } else {
            Outcome::Answered
        }
    }
}

#[cfg(test)]
mod tests {
    use wyrd_testing::gateway_capture::GatewayCapture;

    use super::{EventReader, Outcome, buffered};

    /// A stream split at every byte still yields every event once, and a
    /// truncated, erroring, or short stream is wrong though it parsed.
    ///
    /// # Panics
    ///
    /// Panics when a stream is judged the wrong way.
    #[test]
    fn events_parse_across_any_split_and_judge_the_whole_stream() {
        let stream = GatewayCapture::chat_events("chatcmpl-1", 32);
        let mut reader = EventReader::default();
        let mut arrivals = Vec::new();
        for byte in stream.as_bytes() {
            reader.push(&[*byte], &mut arrivals).expect("valid stream");
        }
        assert_eq!(arrivals.len(), 32);
        assert_eq!(reader.finish(32), Outcome::Answered);
        assert!(matches!(reader.finish(31), Outcome::Failed(_)));

        let frames = GatewayCapture::chat_event_frames("chatcmpl-1", 32);
        let mut truncated = EventReader::default();
        truncated
            .push(frames[..31].concat().as_bytes(), &mut Vec::new())
            .expect("valid prefix");
        assert!(matches!(truncated.finish(32), Outcome::Failed(_)));

        let mut erroring = EventReader::default();
        let error = "data: {\"error\":{\"code\":\"WYRD_GATEWAY_502_UPSTREAM_UNAVAILABLE\"}}\n\ndata: [DONE]\n\n";
        assert!(erroring.push(error.as_bytes(), &mut Vec::new()).is_err());

        let answer = GatewayCapture::sized_completion(800).to_string();
        assert_eq!(buffered(answer.as_bytes(), 800), Outcome::Answered);
        assert!(matches!(
            buffered(answer.as_bytes(), 801),
            Outcome::Failed(_)
        ));
    }
}
