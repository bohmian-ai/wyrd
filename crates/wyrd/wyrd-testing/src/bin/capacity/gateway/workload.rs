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
//! `[DONE]` and body EOF. A `200` alone is never success. A [`Caller`]
//! prepares its URL and headers once; only the benchmark labels of the
//! current scope are read per call.

use std::collections::BTreeSet;
use std::time::{Duration, Instant};

use bytes::Bytes;
use futures_util::StreamExt as _;
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE, HeaderMap, HeaderValue};
use reqwest::{Client, RequestBuilder, Url};
use secrecy::{ExposeSecret as _, SecretString};
use serde_json::{Value, json};
use wyrd_benchmark::Observation;
use wyrd_client::bifrost::BifrostOptions;
use wyrd_client::config::ClientConfig;
use wyrd_client::gateway_admin::{
    GatewayCaptureMode, GatewayCapturePolicyWrite, GatewayPayloadField,
};
use wyrd_client::transport::trace::{benchmark_cohort, benchmark_headers};
use wyrd_client::{Bifrost, Gateway, GlobalConfig, Principals, WyrdClient};
use wyrd_spec::auth::CreateServicePrincipalRequest;
use wyrd_testing::gateway_capture::{GatewayCapture, Leaf, Step, pick};
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
    /// Configures `setup`'s tenant: one deployment per [`PREFIXES`] entry,
    /// each against the upstream base URL at the same position in
    /// `upstreams`, and `keys` invoking principals.
    ///
    /// # Errors
    ///
    /// Returns a client, administration, or principal failure.
    pub async fn provision(
        setup: &SetupTenant,
        upstreams: [&str; PREFIXES.len()],
        keys: usize,
    ) -> Result<Self> {
        let admin = connect(&setup.api_key)?;
        let gateway = Gateway::connect_with(&admin).await?;
        for (prefix, upstream) in PREFIXES.into_iter().zip(upstreams) {
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
    /// `bifrost.gateway.calls`, which only this tenant's calls may land in.
    ///
    /// # Errors
    ///
    /// Returns the connection or query failure.
    pub async fn captured(&self) -> Result<u64> {
        let bifrost = Bifrost::connect_with(&self.admin, BifrostOptions::default()).await?;
        let result = bifrost
            .sql("SELECT count(*) AS calls FROM bifrost.gateway.calls", &[])
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

/// One caller's prepared requests: the keep-alive client, the parsed chat
/// URL, and the caller's credential and content-type headers, built once and
/// reused by every call it makes.
///
/// The benchmark case and phase labels are not prepared: each request reads
/// those of the scope it is sent in, so warmup and measured windows keep
/// their own labels.
#[derive(Debug, Clone)]
pub struct Caller {
    /// The keep-alive client.
    http: Client,
    /// `POST /v1/chat/completions` of the server.
    url: Url,
    /// The bearer credential and JSON content type every call sends.
    headers: HeaderMap,
}

impl Caller {
    /// A caller presenting `key` to the release server over `http`.
    ///
    /// # Errors
    ///
    /// Returns the URL or header construction failure.
    pub fn new(http: Client, key: &SecretString) -> Result<Self> {
        Self::at(http, SERVER_URL, key)
    }

    /// A caller presenting `key` to the server at `base` over `http`.
    ///
    /// # Errors
    ///
    /// Returns the URL or header construction failure.
    fn at(http: Client, base: &str, key: &SecretString) -> Result<Self> {
        let mut bearer = HeaderValue::try_from(format!("Bearer {}", key.expose_secret()))?;
        bearer.set_sensitive(true);
        let mut headers = HeaderMap::with_capacity(2);
        headers.insert(AUTHORIZATION, bearer);
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        Ok(Self {
            http,
            url: Url::parse(&format!("{base}/v1/chat/completions"))?,
            headers,
        })
    }

    /// The request of one call with `body`, labelled by the current scope.
    fn request(&self, body: Bytes) -> RequestBuilder {
        benchmark_headers()
            .into_iter()
            .fold(
                self.http.post(self.url.clone()),
                |request, (name, value)| request.header(name, value),
            )
            .headers(self.headers.clone())
            .body(body)
    }
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
                {"role": "user", "content": GatewayCapture::prose(self.request_bytes)},
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
    /// When each complete stream data event carrying a choice arrived.
    pub events: Vec<Instant>,
    /// When the answer was complete and validated, or the call ended.
    pub done: Instant,
    /// How it ended.
    pub outcome: Outcome,
}

/// The benchmark root of one raw Gateway call, from its send to its
/// validated end, in the task's benchmark cohort.
pub const CALL_ROOT: &str = "client.gateway";

/// The benchmark root of a streamed call's head, from its send to its first
/// data event carrying a choice.
pub const HEAD_ROOT: &str = "client.gateway.head";

/// The benchmark root of a call's one send attempt, from its send to its
/// response headers or failure, as the SDK records its attempts.
pub const ATTEMPT_ROOT: &str = "client.http.attempt";

/// Sends one call of `shape` with `body` as `caller` and times it to its
/// validated end.
///
/// In a benchmark process the call is one [`CALL_ROOT`] observation, a
/// success only when answered, with its reached and disjoint `send`, `body`
/// (receipt, a stream's consumption outside validation), and `decode`
/// (answer validation) components, one [`ATTEMPT_ROOT`], and for a stream
/// one [`HEAD_ROOT`]; a call dropped before its end records them cancelled.
pub async fn call(caller: &Caller, shape: &Shape, body: Bytes) -> Timed {
    let mut observation = Observation::start(CALL_ROOT, benchmark_cohort);
    let mut head = shape.stream.then(|| observation.child(HEAD_ROOT));
    let sent = Instant::now();
    let mut timed = Timed {
        sent,
        events: Vec::new(),
        done: sent,
        outcome: Outcome::Answered,
    };
    let exchanged = exchange(caller, shape, body, &mut timed, &mut observation, &mut head);
    timed.outcome = match tokio::time::timeout(CALL_TIMEOUT, exchanged).await {
        Ok(outcome) => outcome,
        Err(_) => Outcome::TimedOut,
    };
    timed.done = Instant::now();
    if let Some(head) = head {
        head.finish(wyrd_benchmark::ERROR);
    }
    observation.finish(if timed.outcome.answered() {
        wyrd_benchmark::SUCCESS
    } else {
        wyrd_benchmark::ERROR
    });
    timed
}

/// Sends the request, with the benchmark labels scoped on this task, and
/// reads its answer into `timed`, returning how it ended; `observation` and
/// the stream's `head` record what it reached, `head` finishing at the first
/// choice event.
async fn exchange(
    caller: &Caller,
    shape: &Shape,
    body: Bytes,
    timed: &mut Timed,
    observation: &mut Observation,
    head: &mut Option<Observation>,
) -> Outcome {
    let attempt = observation.child(ATTEMPT_ROOT);
    let started = observation.now();
    let sent = caller.request(body).send().await;
    observation.since("send", started);
    attempt.finish(match &sent {
        Ok(response) if response.status().is_success() => wyrd_benchmark::SUCCESS,
        _ => wyrd_benchmark::ERROR,
    });
    let response = match sent {
        Ok(response) => response,
        Err(error) if error.is_timeout() => return Outcome::TimedOut,
        Err(error) => return Outcome::Failed(format!("transport: {error}")),
    };
    let status = response.status();
    let started = observation.now();
    if !status.is_success() {
        let text = response.text().await.unwrap_or_default();
        observation.since("body", started);
        return refusal(status.as_u16(), &text);
    }
    if !shape.stream {
        let bytes = response.bytes().await;
        observation.since("body", started);
        return match bytes {
            Ok(bytes) => {
                let started = observation.now();
                let outcome = buffered(&bytes, shape.expected);
                observation.since("decode", started);
                outcome
            }
            Err(error) => Outcome::Failed(format!("body: {error}")),
        };
    }
    let mut events = EventReader::default();
    let mut stream = response.bytes_stream();
    let mut decoding = std::time::Duration::ZERO;
    let outcome = loop {
        match stream.next().await {
            Some(Ok(chunk)) => {
                let started = observation.now();
                let pushed = events.push(&chunk, &mut timed.events);
                if let Some(started) = started {
                    decoding += started.elapsed();
                }
                if !timed.events.is_empty()
                    && let Some(head) = head.take()
                {
                    head.finish(wyrd_benchmark::SUCCESS);
                }
                if let Err(problem) = pushed {
                    break Outcome::Failed(problem);
                }
            }
            Some(Err(error)) => break Outcome::Failed(format!("stream body: {error}")),
            None => {
                let started = observation.now();
                let outcome = events.finish(shape.expected);
                if let Some(started) = started {
                    decoding += started.elapsed();
                }
                break outcome;
            }
        }
    };
    if let Some(started) = started {
        observation.component("body", started.elapsed().saturating_sub(decoding));
        observation.component("decode", decoding);
    }
    outcome
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

/// Path of a buffered answer's first choice content.
const CONTENT: &[Step] = &[
    Step::Key("choices"),
    Step::Index(0),
    Step::Key("message"),
    Step::Key("content"),
];

/// Path of a buffered answer's total token usage.
const TOTAL_TOKENS: &[Step] = &[Step::Key("usage"), Step::Key("total_tokens")];

/// Validates a buffered answer: a completion with one choice of
/// `content_bytes` content bytes and usage.
///
/// The whole answer is read under a decoded value's rules, but only the
/// content and the usage total are kept, the content borrowed unless it
/// holds escapes.
pub fn buffered(bytes: &[u8], content_bytes: usize) -> Outcome {
    let Ok([content, total]) = pick(bytes, [CONTENT, TOTAL_TOKENS]) else {
        return Outcome::Failed("buffered answer is not JSON".to_owned());
    };
    let content = match content {
        Some(Leaf::Str(content)) => Some(content.len()),
        _ => None,
    };
    if content != Some(content_bytes) {
        return Outcome::Failed("wrong buffered content".to_owned());
    }
    if !matches!(total, Some(Leaf::U64(_))) {
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
    use std::borrow::Cow;

    use secrecy::{ExposeSecret as _, SecretString};
    use serde_json::Value;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};
    use wyrd_client::transport::trace::{benchmark_headers, with_benchmark_labels};
    use wyrd_testing::gateway_capture::{GatewayCapture, Leaf, pick};

    use super::{CONTENT, Caller, EventReader, Outcome, Shape, TOTAL_TOKENS, buffered, call, http};

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

        let prose = GatewayCapture::prose(65_536);
        assert_eq!(prose.len(), 65_536);
        assert_eq!(prose, GatewayCapture::prose(65_536));
        assert!(prose.split(' ').count() > 10_000, "sized text is words");

        let answer = GatewayCapture::sized_completion(800).to_string();
        assert_eq!(buffered(answer.as_bytes(), 800), Outcome::Answered);
        assert!(matches!(
            buffered(answer.as_bytes(), 801),
            Outcome::Failed(_)
        ));
    }

    /// The former buffered predicate, over a decoded value.
    fn decoded(bytes: &[u8], content_bytes: usize) -> Outcome {
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

    /// Buffered validation judges every answer as the decoded-value
    /// predicate did, failing for the same reason: escaped and Unicode
    /// content, duplicate members with the last winning, extra choices and
    /// members, mistyped or missing content and usage, signed, float, and
    /// out-of-range usage, deep nesting, and malformed or trailing input.
    ///
    /// # Panics
    ///
    /// Panics when an outcome differs.
    #[test]
    fn buffered_validation_matches_the_existing_json_predicate() {
        let deep = |depth: usize| {
            format!(
                r#"{{"choices":[{{"message":{{"content":"abc"}}}}],"usage":{{"total_tokens":1}},"x":{}{}}}"#,
                "[".repeat(depth),
                "]".repeat(depth)
            )
        };
        let mut cases: Vec<String> = [
            r#"{"choices":[{"message":{"content":"abc"}}],"usage":{"total_tokens":3}}"#,
            r#"{"id":"c","choices":[{"index":0,"message":{"role":"assistant","content":"abc"},"finish_reason":"stop"},{"message":{"content":"other"}}],"usage":{"prompt_tokens":1,"total_tokens":3},"x":{"y":[null,true,1.5]}}"#,
            r#"{"choices":[{"message":{"content":"a\"c"}}],"usage":{"total_tokens":3}}"#,
            r#"{"choices":[{"message":{"content":"éc"}}],"usage":{"total_tokens":3}}"#,
            r#"{"choices":[{"message":{"content":"🦀"}}],"usage":{"total_tokens":3}}"#,
            r#"{"choices":[{"message":{"content":"\ud800"}}],"usage":{"total_tokens":3}}"#,
            r#"{"choices":[{"message":{"content":"xyz"}}],"choices":[{"message":{"content":"abcd"}}],"usage":{"total_tokens":3}}"#,
            r#"{"choices":[{"message":{"content":"abc"}}],"choices":[],"usage":{"total_tokens":3}}"#,
            r#"{"choices":[{"message":{"content":"abc","content":null}}],"usage":{"total_tokens":3}}"#,
            r#"{"choices":[{"message":{"content":"abc"}}],"usage":{"total_tokens":3},"usage":{}}"#,
            r#"{"choices":[{"message":{"content":"abc"}}],"usage":{"total_tokens":3,"total_tokens":-1}}"#,
            r#"{"choices":[{"message":{"content":"abc"}}],"usage":{"total_tokens":3}}"#,
            r#"{"choices":{"0":{"message":{"content":"abc"}}},"usage":{"total_tokens":3}}"#,
            r#"{"choices":[{"message":{"content":["abc"]}}],"usage":{"total_tokens":3}}"#,
            r#"{"choices":[{"message":"abc"}],"usage":{"total_tokens":3}}"#,
            r#"{"choices":[{"message":{"content":"abc"}}]}"#,
            r#"{"choices":[{"message":{"content":"abc"}}],"usage":{"total_tokens":"3"}}"#,
            r#"{"choices":[{"message":{"content":"abc"}}],"usage":{"total_tokens":-3}}"#,
            r#"{"choices":[{"message":{"content":"abc"}}],"usage":{"total_tokens":-0}}"#,
            r#"{"choices":[{"message":{"content":"abc"}}],"usage":{"total_tokens":3.0}}"#,
            r#"{"choices":[{"message":{"content":"abc"}}],"usage":{"total_tokens":18446744073709551615}}"#,
            r#"{"choices":[{"message":{"content":"abc"}}],"usage":{"total_tokens":18446744073709551616}}"#,
            r#"{"choices":[{"message":{"content":"abc"}}],"usage":{"total_tokens":1e400}}"#,
            r#"{"choices":[{"message":{"content":"abc"}}],"usage":{"total_tokens":3}} "#,
            r#"{"choices":[{"message":{"content":"abc"}}],"usage":{"total_tokens":3}}x"#,
            r#"{"choices":[{"message":{"content":"abc"}}],"usage":{"total_tokens":3}"#,
            r#"[{"choices":[{"message":{"content":"abc"}}],"usage":{"total_tokens":3}}]"#,
            "null",
            "",
        ]
        .map(str::to_owned)
        .into();
        cases.extend([deep(100), deep(124), deep(126), deep(400)]);
        cases.push(GatewayCapture::sized_completion(800).to_string());
        let mut answered = 0;
        for case in &cases {
            for content_bytes in [2, 3, 4, 800] {
                let expected = decoded(case.as_bytes(), content_bytes);
                assert_eq!(
                    buffered(case.as_bytes(), content_bytes),
                    expected,
                    "{case} with {content_bytes} content bytes"
                );
                answered += usize::from(expected.answered());
            }
        }
        assert_eq!(answered, 13, "answered judgements");
    }

    /// The content of a large unescaped answer is read in place from the
    /// received bytes, and escaped content is decoded and still measured in
    /// decoded bytes.
    ///
    /// # Panics
    ///
    /// Panics when unescaped content is copied or escaped content judged
    /// wrongly.
    #[test]
    fn buffered_validation_borrows_unescaped_content() {
        let answer = GatewayCapture::sized_completion(65_536).to_string();
        let [content, total] = pick(answer.as_bytes(), [CONTENT, TOTAL_TOKENS]).expect("json");
        let Some(Leaf::Str(Cow::Borrowed(content))) = content else {
            panic!("unescaped content is borrowed: {content:?}");
        };
        assert_eq!(content.len(), 65_536);
        assert!(answer.as_bytes().as_ptr_range().contains(&content.as_ptr()));
        assert!(matches!(total, Some(Leaf::U64(_))));
        assert_eq!(buffered(answer.as_bytes(), 65_536), Outcome::Answered);

        let escaped = r#"{"choices":[{"message":{"content":"a\"é"}}],"usage":{"total_tokens":1}}"#;
        let [content, _] = pick(escaped.as_bytes(), [CONTENT, TOTAL_TOKENS]).expect("json");
        assert_eq!(
            content,
            Some(Leaf::Str(Cow::Owned("a\"\u{e9}".to_owned()))),
            "escaped content is decoded"
        );
        assert_eq!(buffered(escaped.as_bytes(), 4), Outcome::Answered);
    }

    /// A prepared caller sends what the former per-call setup sent: the same
    /// method, URL, credential, content type, and body bytes, with the case
    /// and phase of whichever window the request is sent in, and none
    /// outside one, so two differently labelled windows never share labels.
    ///
    /// # Panics
    ///
    /// Panics when a request or its outcome differs.
    #[tokio::test]
    async fn prepared_requests_keep_dynamic_window_headers() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(GatewayCapture::sized_completion(16)),
            )
            .mount(&server)
            .await;
        let key = SecretString::from("wyrd_sk_prepared".to_owned());
        let caller = Caller::at(http().expect("client"), &server.uri(), &key).expect("caller");
        let shape = Shape {
            model: "bench-chat/gpt-4o".to_owned(),
            stream: false,
            request_bytes: 64,
            expected: 16,
        };
        let body = shape.body("marker");
        let former = |labels: Vec<(&'static str, String)>| {
            labels
                .into_iter()
                .fold(
                    http()
                        .expect("client")
                        .post(format!("{}/v1/chat/completions", server.uri())),
                    |request, (name, value)| request.header(name, value),
                )
                .bearer_auth(key.expose_secret())
                .header("content-type", "application/json")
                .body(body.clone())
                .send()
        };
        let windows = [
            Some(("case-a", "warmup")),
            Some(("case-b", "measured")),
            None,
        ];
        for window in windows {
            let both = async {
                former(benchmark_headers()).await.expect("former request");
                call(&caller, &shape, body.clone()).await
            };
            let timed = if let Some((case, phase)) = window {
                with_benchmark_labels(case, phase, both).await
            } else {
                both.await
            };
            assert_eq!(timed.outcome, Outcome::Answered, "{window:?}");
        }

        let received = server.received_requests().await.expect("recording");
        assert_eq!(received.len(), 2 * windows.len());
        let sent = |request: &wiremock::Request| {
            let mut headers: Vec<(String, String)> = request
                .headers
                .iter()
                .filter(|(name, _)| {
                    !matches!(
                        name.as_str(),
                        "host" | "content-length" | "accept" | "accept-encoding"
                    )
                })
                .map(|(name, value)| {
                    (
                        name.as_str().to_owned(),
                        value.to_str().expect("text").to_owned(),
                    )
                })
                .collect();
            headers.sort();
            (
                request.method.to_string(),
                request.url.path().to_owned(),
                headers,
                request.body.clone(),
            )
        };
        for (pair, window) in received.chunks(2).zip(windows) {
            let (former, prepared) = (sent(&pair[0]), sent(&pair[1]));
            assert_eq!(prepared, former, "{window:?}");
            let mut expected = vec![
                (
                    "authorization".to_owned(),
                    "Bearer wyrd_sk_prepared".to_owned(),
                ),
                ("content-type".to_owned(), "application/json".to_owned()),
            ];
            if let Some((case, phase)) = window {
                expected.push(("x-wyrd-benchmark-case".to_owned(), case.to_owned()));
                expected.push(("x-wyrd-benchmark-phase".to_owned(), phase.to_owned()));
            }
            expected.sort();
            assert_eq!(prepared.2, expected, "{window:?} headers");
            assert_eq!(prepared.3, body.to_vec(), "body bytes");
        }
    }
}
