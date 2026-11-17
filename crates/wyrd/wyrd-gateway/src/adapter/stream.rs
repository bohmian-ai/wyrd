//! Incremental relay of provider server-sent event answers.
//!
//! [`relay`] reads one upstream stream in a spawned task and hands frames to
//! the caller through a bounded channel: a full channel pauses upstream reads,
//! and a dropped receiver, the deadline, or cancellation drops the upstream
//! response, which aborts the request. Native frames relay byte for byte;
//! Anthropic and Google events become typed `OpenAI` chat completion chunks.
//! Every event is parsed from a bounded copy for usage. The relay always ends
//! by sending one [`StreamEnd`] carrying the terminal outcome, usage, and,
//! only when response capture was selected, the bounded credential-scrubbed
//! relayed content.
//!
//! Success requires the upstream protocol's terminator: `data: [DONE]` for
//! Chat Completions, a terminal `response.*` event for Responses,
//! `message_stop` for Anthropic, and a finish reason on every candidate for
//! Google. Truncation, transport failure, an oversized or untranslatable
//! event, the deadline, and cancellation or drain end the stream with the
//! ingress protocol's terminal error when it can be appended at an event
//! boundary, and otherwise leave it unterminated so the caller aborts its
//! transport. Either way usage stays unknown. A Responses stream that ends in
//! `response.failed` or `response.incomplete` is terminated but failed.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use chrono::Utc;
use secrecy::{ExposeSecret, SecretString};
use serde_json::{Map, Value, json};
use skald_providers::stream::SseDecoder;
use skald_providers::{MediaAnswer, ProviderByteStream};
use skald_spec::wire::anthropic_messages::{
    AnthropicContentBlock, AnthropicDelta, AnthropicMessageDeltaFields,
};
use skald_spec::wire::google_generate::{GoogleGenerateContentResponse, GooglePart};
use skald_spec::wire::openai_chat::{
    OpenAiChatChoiceDelta, OpenAiChatStreamChoice, OpenAiChatStreamChunk, OpenAiUsage,
};
use tokio::sync::{mpsc, oneshot};
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;
use wyrd_spec::error::WyrdError;
use wyrd_spec::gateway::{GATEWAY_JSON_MAX_BYTES, GatewayCallOutcome};

use super::{Wire, anthropic, google, openai_error_body, usage};
use crate::credential::ProviderSecret;
use crate::engine::{
    AttemptUsage, EventStream, Frame, ProviderAttempt, ResponseCapture, StreamEnd,
};
use wyrd_runtime::resources::ByteCharge;

/// Frames buffered between the upstream reader and the caller; once full,
/// upstream reads wait for the caller.
pub(crate) const FRAME_BUFFER: usize = 16;

/// Largest frame the relay retains; a longer upstream chunk is relayed as
/// several frames, so the frames a slow caller leaves queued hold at most
/// [`FRAME_BUFFER`] times this many bytes.
pub(crate) const FRAME_BYTES: usize = 64 * 1024;

/// Most usage members one stream remembers; typed usage reads only a few,
/// so further new members a provider sends are ignored while those kept
/// still update.
const USAGE_MEMBERS: usize = 32;

/// The terminal frame of a successful Chat Completions stream.
pub(crate) const DONE_FRAME: &[u8] = b"data: [DONE]\n\n";

/// Longest wait to hand a terminal error frame to a caller that is not
/// reading; past it the stream stays unterminated and the caller aborts.
const TERMINAL_GRACE: Duration = Duration::from_secs(5);

/// Conversion applied to upstream events.
pub(crate) enum Frames {
    /// Native passthrough: upstream bytes relay unchanged.
    Native {
        /// Whether the stream is an `OpenAI` Responses stream rather than
        /// Chat Completions; ignored for other wires.
        responses: bool,
    },
    /// Chat completion chunks translated from Anthropic or Google events.
    Chat(ChatChunks),
}

/// Why a stream stopped before its protocol's success terminator.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Stop {
    /// Truncation, transport failure, or an oversized, undecodable, or
    /// untranslatable event.
    Upstream,
    /// The caller deadline passed.
    Timeout,
    /// The caller cancelled or the server is draining.
    Cancelled,
    /// Process memory could not hold the next frame.
    Memory,
}

impl Stop {
    /// Human-readable reason reported in the terminal frame.
    const fn message(self) -> &'static str {
        match self {
            Self::Upstream => "the provider stream ended before completing",
            Self::Timeout => "the gateway call exceeded its deadline",
            Self::Cancelled => "the gateway stream was cancelled",
            Self::Memory => "process memory cannot hold the gateway stream now",
        }
    }

    /// Terminal call outcome of this stop.
    const fn outcome(self) -> GatewayCallOutcome {
        match self {
            Self::Upstream | Self::Memory => GatewayCallOutcome::Failed,
            Self::Timeout => GatewayCallOutcome::TimedOut,
            Self::Cancelled => GatewayCallOutcome::Cancelled,
        }
    }

    /// Stable gateway error whose code and status the terminal frame carries.
    fn error(self) -> WyrdError {
        let message = self.message().to_owned();
        let details = json!({});
        match self {
            Self::Upstream => WyrdError::GatewayUpstreamUnavailable { message, details },
            Self::Timeout => WyrdError::GatewayDeadlineExceeded { message, details },
            Self::Cancelled | Self::Memory => WyrdError::ServiceUnavailable { message, details },
        }
    }
}

/// Relayed content of one stream whose response capture was selected, kept
/// within [`GATEWAY_JSON_MAX_BYTES`] and scrubbed of the attempt's credential
/// as each event is kept.
pub(crate) struct StreamCapture {
    /// Credential removed from every kept event, when the attempt had one;
    /// dropped with the relay.
    secret: Option<SecretString>,
    /// Kept events in delivery order; `None` once the relayed content exceeded
    /// the ceiling or an event had no JSON form, which drops capture.
    events: Option<Vec<Value>>,
    /// Relayed event bytes counted so far.
    bytes: usize,
    /// Process memory charged for the kept events, by their relayed size; a
    /// sibling of the call's charge, returned when the capture drops.
    charge: ByteCharge,
}

impl StreamCapture {
    /// Starts an empty capture scrubbing `credential`, charging what it
    /// keeps as a sibling of the call's `memory`.
    pub(crate) fn new(credential: Option<&ProviderSecret>, memory: &ByteCharge) -> Self {
        Self {
            secret: credential
                .map(ProviderSecret::expose)
                .filter(|secret| !secret.is_empty())
                .map(|secret| SecretString::from(secret.to_owned())),
            events: Some(Vec::new()),
            bytes: 0,
            charge: memory.sibling(),
        }
    }

    /// Keeps `event`, relayed as `bytes` bytes, without the credential,
    /// charging those bytes; an event with no JSON form, one past the
    /// ceiling, one holding the credential as base64, or one process memory
    /// cannot hold drops the capture and frees what was kept.
    fn push(&mut self, event: Option<Value>, bytes: usize) {
        self.bytes = self.bytes.saturating_add(bytes);
        let Some(events) = &mut self.events else {
            return;
        };
        let secret = self.secret.as_ref().map(ExposeSecret::expose_secret);
        if let Some(ResponseCapture::Json(event)) = event
            .filter(|_| self.bytes <= GATEWAY_JSON_MAX_BYTES)
            .filter(|_| {
                self.charge
                    .try_grow(u64::try_from(bytes).unwrap_or(u64::MAX))
                    .is_ok()
            })
            .and_then(|event| ResponseCapture::json(event, secret))
        {
            events.push(event);
        } else {
            self.events = None;
            self.charge.shrink(self.charge.bytes());
        }
    }

    /// Kept events as one JSON array, or `None` when capture was dropped.
    fn finish(self) -> Option<ResponseCapture> {
        self.events
            .map(|events| ResponseCapture::Json(Value::Array(events)))
    }
}

/// Relayed bytes of one media stream, such as generated speech, whose response
/// capture was selected.
///
/// Bytes are kept as they are delivered, within the relay's own answer bound,
/// and become a capture only once the body ends cleanly without holding the
/// attempt's credential. Nothing is kept when capture is unselected, because
/// no `MediaCapture` exists then.
pub(crate) struct MediaCapture {
    /// Credential the kept bytes must not hold, when the attempt had one;
    /// dropped with the relay.
    secret: Option<SecretString>,
    /// Content type the upstream answer declared.
    content_type: String,
    /// Delivered bytes in order; `None` once process memory could not hold
    /// them, which drops capture.
    bytes: Option<Vec<u8>>,
    /// Process memory charged for `bytes`; a sibling of the call's charge,
    /// returned when the capture drops.
    charge: ByteCharge,
}

impl MediaCapture {
    /// Starts an empty capture of `content_type` bytes judged against
    /// `credential`, charging what it keeps as a sibling of the call's
    /// `memory`.
    pub(crate) fn new(
        credential: Option<&ProviderSecret>,
        content_type: &str,
        memory: &ByteCharge,
    ) -> Self {
        Self {
            secret: credential.map(|secret| SecretString::from(secret.expose().to_owned())),
            content_type: content_type.to_owned(),
            bytes: Some(Vec::new()),
            charge: memory.sibling(),
        }
    }

    /// Keeps delivered `bytes`, charging them; bytes process memory cannot
    /// hold drop the capture and free what was kept.
    fn keep(&mut self, bytes: &[u8]) {
        let Some(kept) = &mut self.bytes else {
            return;
        };
        if self
            .charge
            .try_grow(u64::try_from(bytes.len()).unwrap_or(u64::MAX))
            .is_ok()
        {
            kept.extend_from_slice(bytes);
        } else {
            self.bytes = None;
            self.charge.shrink(self.charge.bytes());
        }
    }

    /// The kept bytes as a media capture, or `None` when they hold the
    /// credential or capture was dropped.
    fn finish(self) -> Option<ResponseCapture> {
        ResponseCapture::media(
            MediaAnswer {
                content_type: self.content_type,
                bytes: self.bytes?.into(),
            },
            self.secret.as_ref().map(ExposeSecret::expose_secret),
        )
    }
}

/// How a pumped stream ended.
enum Ended {
    /// The protocol terminator arrived: `Succeeded`, or `Failed` for a failed
    /// or incomplete Responses stream; carries the stream's usage.
    Terminated {
        /// Outcome the terminator selected.
        outcome: GatewayCallOutcome,
        /// Usage the stream's events carried.
        usage: AttemptUsage,
    },
    /// The stream stopped abnormally; `boundary` tells whether every relayed
    /// byte belongs to a complete event, so a terminal event may follow.
    Failed {
        /// Abnormal stop, which selects the terminal error or the abort.
        stop: Stop,
        /// Whether a terminal event can be appended.
        boundary: bool,
    },
    /// The caller dropped the stream; nobody is left to tell.
    Detached,
}

/// Terminal error frame of the stream's ingress protocol, or `None` when the
/// protocol has no in-band error (Google `GenerateContent`, and Jev, which
/// never streams).
///
/// Chat Completions append an `OpenAI` error envelope and `[DONE]`; Responses
/// an `error` event; Anthropic an `error` event.
fn error_frame(wire: Wire, frames: &Frames, stop: Stop) -> Option<Vec<u8>> {
    let error = stop.error();
    let message = stop.message();
    let (event, data) = match (frames, wire) {
        (Frames::Chat(_), _) | (Frames::Native { responses: false }, Wire::OpenAi) => {
            let body = openai_error_body(error.status(), message, Some(error.code()), None);
            let mut frame = format!("data: {body}\n\n").into_bytes();
            frame.extend_from_slice(DONE_FRAME);
            return Some(frame);
        }
        (Frames::Native { responses: true }, Wire::OpenAi) => (
            "error",
            json!({"type": "error", "code": error.code(), "message": message, "param": null}),
        ),
        (Frames::Native { .. }, Wire::Anthropic) => {
            let kind = if stop == Stop::Timeout {
                "timeout_error"
            } else {
                "api_error"
            };
            (
                "error",
                json!({"type": "error", "error": {"type": kind, "message": message}}),
            )
        }
        (Frames::Native { .. }, Wire::Google | Wire::Jev) => return None,
    };
    Some(format!("event: {event}\ndata: {data}\n\n").into_bytes())
}

/// Whether upstream event `data` is its protocol's terminator.
///
/// `OpenAI` Chat Completions end with `[DONE]`; Responses with a
/// `response.completed`, `response.incomplete`, or `response.failed` event;
/// Anthropic with `message_stop`; Google with a response whose every
/// candidate carries a finish reason. Jev never streams, so no event of it
/// terminates a stream.
fn terminates(wire: Wire, responses: bool, data: &str, event: Option<&Value>) -> bool {
    match wire {
        Wire::Jev => false,
        Wire::OpenAi if !responses => data == "[DONE]",
        Wire::OpenAi => event
            .and_then(|event| event.get("type"))
            .and_then(Value::as_str)
            .is_some_and(|kind| {
                matches!(
                    kind,
                    "response.completed" | "response.incomplete" | "response.failed"
                )
            }),
        Wire::Anthropic => {
            event.and_then(|event| event.get("type")) == Some(&Value::from("message_stop"))
        }
        Wire::Google => event
            .and_then(|event| event.get("candidates"))
            .and_then(Value::as_array)
            .is_some_and(|candidates| {
                !candidates.is_empty()
                    && candidates.iter().all(|candidate| {
                        candidate.get("finishReason").is_some()
                            || candidate.get("finish_reason").is_some()
                    })
            }),
    }
}

/// Translation state of one chat completion chunk stream.
pub(crate) struct ChatChunks {
    /// Chunk identifier; an Anthropic stream adopts its message id.
    id: String,
    /// Model named in every chunk.
    model: String,
    /// Creation time shared by every chunk.
    created: u64,
    /// Whether the caller asked for a final usage chunk.
    include_usage: bool,
    /// Choices the caller asked for, `n`; a provider choice index at or
    /// beyond it is a protocol failure.
    choices: u32,
    /// Choice indexes whose first chunk already carried the assistant role,
    /// at most `choices` of them.
    started: Vec<u32>,
}

impl ChatChunks {
    /// Starts a chunk stream named `id` for `model` relaying `choices`
    /// choices, at least one.
    pub(crate) fn new(id: String, model: &str, include_usage: bool, choices: u32) -> Self {
        Self {
            id,
            model: model.to_owned(),
            created: super::now(),
            include_usage,
            choices: choices.max(1),
            started: Vec::new(),
        }
    }

    /// Chunk of choice `index` carrying `content` and `finish`; the choice's
    /// first chunk also announces the assistant role.
    fn chunk(
        &mut self,
        index: u32,
        content: Option<String>,
        finish: Option<&str>,
    ) -> OpenAiChatStreamChunk {
        let role = (!self.started.contains(&index)).then(|| {
            self.started.push(index);
            "assistant".to_owned()
        });
        self.envelope(
            vec![OpenAiChatStreamChoice {
                index,
                delta: OpenAiChatChoiceDelta {
                    role,
                    content,
                    ..OpenAiChatChoiceDelta::default()
                },
                finish_reason: finish.map(str::to_owned),
                logprobs: None,
            }],
            None,
        )
    }

    /// Chunk envelope around `choices` and `usage`.
    fn envelope(
        &self,
        choices: Vec<OpenAiChatStreamChoice>,
        usage: Option<OpenAiUsage>,
    ) -> OpenAiChatStreamChunk {
        OpenAiChatStreamChunk {
            id: self.id.clone(),
            object: "chat.completion.chunk".to_owned(),
            created: self.created,
            model: self.model.clone(),
            choices,
            usage,
        }
    }

    /// Chunks of one `wire` event, or `None` when the event has no chat
    /// completion representation.
    fn event(&mut self, wire: Wire, event: Value) -> Option<Vec<OpenAiChatStreamChunk>> {
        match wire {
            Wire::Anthropic => self.anthropic(event),
            Wire::Google => self.google(event),
            Wire::OpenAi | Wire::Jev => None,
        }
    }

    /// Chunks of one Anthropic event: text becomes content and the stop
    /// reason a finish reason. Pings and block or message boundaries yield
    /// nothing; thinking, tool, citation, error, and unknown events have no
    /// representation.
    fn anthropic(&mut self, mut event: Value) -> Option<Vec<OpenAiChatStreamChunk>> {
        let mut member = |name: &str| event.get_mut(name).map(Value::take);
        let kind = member("type")?;
        match kind.as_str()? {
            "message_start" => {
                if let Some(Value::String(id)) =
                    member("message").and_then(|mut message| message.get_mut("id").map(Value::take))
                {
                    self.id = id;
                }
                Some(vec![self.chunk(0, None, None)])
            }
            "content_block_start" => match serde_json::from_value(member("content_block")?).ok()? {
                AnthropicContentBlock::Text {
                    text,
                    citations: None,
                    cache_control: _,
                } => Some(
                    (!text.is_empty())
                        .then(|| self.chunk(0, Some(text), None))
                        .into_iter()
                        .collect(),
                ),
                _ => None,
            },
            "content_block_delta" => match serde_json::from_value(member("delta")?).ok()? {
                AnthropicDelta::TextDelta { text } => Some(vec![self.chunk(0, Some(text), None)]),
                _ => None,
            },
            "message_delta" => {
                let delta: AnthropicMessageDeltaFields =
                    serde_json::from_value(member("delta")?).ok()?;
                let finish = anthropic::finish(&delta.stop_reason?)?;
                Some(vec![self.chunk(0, None, Some(finish))])
            }
            "content_block_stop" | "message_stop" | "ping" => Some(Vec::new()),
            _ => None,
        }
    }

    /// Chunks of one Google response: one per candidate, carrying its text
    /// and finish reason. Thought parts are omitted; other parts and
    /// unrepresentable finish reasons have no representation.
    fn google(&mut self, event: Value) -> Option<Vec<OpenAiChatStreamChunk>> {
        let native: GoogleGenerateContentResponse = serde_json::from_value(event).ok()?;
        let mut chunks = Vec::with_capacity(native.candidates.len());
        for (position, candidate) in native.candidates.into_iter().enumerate() {
            let index = candidate
                .index
                .unwrap_or_else(|| u32::try_from(position).unwrap_or(u32::MAX));
            if index >= self.choices {
                return None;
            }
            let mut content = String::new();
            for part in candidate.content.parts {
                match part {
                    GooglePart::Text { text } => content.push_str(&text),
                    GooglePart::Thought { .. } => {}
                    _ => return None,
                }
            }
            let finish = match candidate.finish_reason {
                Some(reason) => Some(google::finish(&reason, false)?),
                None => None,
            };
            chunks.push(self.chunk(index, (!content.is_empty()).then_some(content), finish));
        }
        Some(chunks)
    }

    /// Final usage chunk from the `merged` provider usage, when the caller
    /// asked for one and the usage decodes.
    fn usage(&self, wire: Wire, merged: &Map<String, Value>) -> Option<OpenAiChatStreamChunk> {
        if !self.include_usage {
            return None;
        }
        let merged = Value::Object(merged.clone());
        let usage = match wire {
            Wire::Anthropic => anthropic::chat_usage(&serde_json::from_value(merged).ok()?),
            Wire::Google => google::chat_usage(&serde_json::from_value(merged).ok()?),
            Wire::OpenAi | Wire::Jev => return None,
        };
        Some(self.envelope(Vec::new(), Some(usage)))
    }
}

/// Relays a raw media answer, such as generated speech, chunk by chunk from a
/// spawned task.
///
/// Chunks pass unchanged through the bounded frame channel, so a slow caller
/// pauses upstream reads, and the task ends when the upstream body ends or
/// fails, once more than `limit` bytes arrived, at `deadline`, on `cancel`,
/// or when the caller drops the stream; dropping the upstream answer aborts
/// the request. Raw media has no in-band error, so only a body that ends
/// within `limit` marks the stream terminated; every other end leaves it
/// unterminated so the caller aborts its transport. Either way one
/// [`StreamEnd`] reports the outcome with empty usage; it carries `capture`'s
/// bytes only when capture was selected and the body ended cleanly.
pub(crate) fn relay_media(
    upstream: ProviderByteStream,
    limit: usize,
    deadline: Instant,
    cancel: CancellationToken,
    capture: Option<MediaCapture>,
    memory: ByteCharge,
) -> EventStream {
    let (sender, receiver) = mpsc::channel(FRAME_BUFFER);
    let (end_sender, end) = oneshot::channel();
    let terminated = Arc::new(AtomicBool::new(false));
    let relay = MediaRelay {
        limit,
        deadline,
        cancel,
        sender,
        memory,
        capture,
        last_byte: None,
    };
    tokio::spawn(relay.run(upstream, end_sender, Arc::clone(&terminated)));
    EventStream::new(receiver, terminated, end)
}

/// One raw media relay: the caller's chunk channel plus the total answer bound
/// and the stop signals every read and delivery honours.
struct MediaRelay {
    /// Largest total answer, in bytes.
    limit: usize,
    /// Instant after which the relay stops.
    deadline: Instant,
    /// Call cancellation, which stops the relay.
    cancel: CancellationToken,
    /// Bounded channel to the caller's stream.
    sender: mpsc::Sender<Frame>,
    /// The call's empty memory charge; each frame is charged as a sibling.
    memory: ByteCharge,
    /// Selected response bytes kept for capture; `None` keeps nothing.
    capture: Option<MediaCapture>,
    /// When the last upstream body chunk arrived.
    last_byte: Option<std::time::Instant>,
}

impl MediaRelay {
    /// Relays `upstream`, stores `terminated` when the body ended cleanly, and
    /// then sends the [`StreamEnd`] on `end` while the chunk channel is still
    /// open.
    async fn run(
        mut self,
        upstream: ProviderByteStream,
        end: oneshot::Sender<StreamEnd>,
        terminated: Arc<AtomicBool>,
    ) {
        let outcome = self.pump(upstream).await;
        if outcome == GatewayCallOutcome::Succeeded {
            terminated.store(true, Ordering::Release);
        }
        let capture = self
            .capture
            .take()
            .filter(|_| outcome == GatewayCallOutcome::Succeeded)
            .and_then(MediaCapture::finish);
        // A caller that stopped accounting has nothing left to record.
        end.send(StreamEnd {
            outcome,
            terminal_at: Utc::now(),
            usage: AttemptUsage::default(),
            capture,
            last_byte: self.last_byte,
            relay: Duration::ZERO,
        })
        .ok();
    }

    /// Moves `upstream` chunks to the caller until the body ends, keeping a
    /// copy of each delivered chunk when capture was selected.
    ///
    /// Returns `Succeeded` at a clean end within `limit`, `Failed` on an
    /// upstream error or an oversized answer, `TimedOut` at the deadline, and
    /// `Cancelled` on cancellation or when the caller dropped the stream.
    async fn pump(&mut self, mut upstream: ProviderByteStream) -> GatewayCallOutcome {
        let mut relayed = 0_usize;
        loop {
            let chunk = tokio::select! {
                biased;
                () = self.sender.closed() => return GatewayCallOutcome::Cancelled,
                () = self.cancel.cancelled() => return GatewayCallOutcome::Cancelled,
                () = tokio::time::sleep_until(self.deadline) => return GatewayCallOutcome::TimedOut,
                chunk = upstream.chunk() => chunk,
            };
            let bytes = match chunk {
                Ok(Some(bytes)) => bytes,
                Ok(None) => return GatewayCallOutcome::Succeeded,
                Err(_) => return GatewayCallOutcome::Failed,
            };
            self.last_byte = Some(std::time::Instant::now());
            relayed = relayed.saturating_add(bytes.len());
            if relayed > self.limit {
                return GatewayCallOutcome::Failed;
            }
            if let Some(capture) = &mut self.capture {
                capture.keep(&bytes);
            }
            // Raw media has no in-band error: a refused chunk fails the
            // stream, which the caller aborts.
            let Ok(frames) = Frame::split(bytes, FRAME_BYTES, &self.memory) else {
                return GatewayCallOutcome::Failed;
            };
            for frame in frames {
                let sent = tokio::select! {
                    biased;
                    () = self.cancel.cancelled() => return GatewayCallOutcome::Cancelled,
                    () = tokio::time::sleep_until(self.deadline) => return GatewayCallOutcome::TimedOut,
                    sent = self.sender.send(frame) => sent,
                };
                if sent.is_err() {
                    return GatewayCallOutcome::Cancelled;
                }
            }
        }
    }
}

/// Relays `upstream` to a bounded frame channel from a spawned task.
///
/// `limit` bounds each buffered event. The task ends when the upstream
/// terminates or fails, at `deadline`, on `cancel`, or when the caller drops
/// the stream; dropping the upstream answer aborts the request. A stream whose
/// terminator arrived marks itself terminated and reports usage. An abnormal
/// stop at an event boundary appends the ingress protocol's terminal error,
/// waiting at most [`TERMINAL_GRACE`] for the caller to accept it, and marks
/// the stream terminated once delivered; otherwise the stream stays
/// unterminated so the caller aborts. Every end sends one [`StreamEnd`],
/// carrying the relayed content when `attempt` selected response capture.
/// Deadline, cancellation, credential, and the memory charged for retained
/// frames and capture all come from `attempt`.
pub(crate) fn relay(
    upstream: ProviderByteStream,
    wire: Wire,
    frames: Frames,
    limit: usize,
    attempt: &ProviderAttempt<'_>,
) -> EventStream {
    let deadline = attempt.deadline;
    let cancel = attempt.cancel.clone();
    let capture = attempt
        .capture
        .then(|| StreamCapture::new(attempt.credential, attempt.memory));
    let memory = attempt.memory.sibling();
    let (sender, receiver) = mpsc::channel(FRAME_BUFFER);
    let (end_sender, end) = oneshot::channel();
    let terminated = Arc::new(AtomicBool::new(false));
    let relay = Relay {
        wire,
        frames,
        limit,
        deadline,
        cancel,
        sender,
        memory,
        capture,
        last_byte: None,
        busy: Duration::ZERO,
    };
    tokio::spawn(relay.run(upstream, end_sender, Arc::clone(&terminated)));
    EventStream::new(receiver, terminated, end)
}

/// One stream relay: the caller's frame channel plus the bounds and stop
/// signals every delivery honours.
///
/// It owns the invariants shared by every step of a relay: each buffered event
/// stays within `limit`, and every wait ends at `deadline`, on `cancel`, or when
/// the caller drops `sender`.
struct Relay {
    /// Upstream wire protocol, which selects usage parsing and terminators.
    wire: Wire,
    /// Native passthrough or Chat Completions translation state.
    frames: Frames,
    /// Largest buffered upstream event, in bytes.
    limit: usize,
    /// Instant after which the relay stops with a timeout.
    deadline: Instant,
    /// Call cancellation, which stops the relay.
    cancel: CancellationToken,
    /// Bounded channel to the caller's stream.
    sender: mpsc::Sender<Frame>,
    /// The call's empty memory charge; each frame is charged as a sibling.
    memory: ByteCharge,
    /// Selected response content kept for capture; `None` keeps nothing.
    capture: Option<StreamCapture>,
    /// When the last upstream body chunk arrived.
    last_byte: Option<std::time::Instant>,
    /// Time spent decoding, translating, and framing chunks, excluding waits
    /// on the provider and on the caller.
    busy: Duration,
}

impl Relay {
    /// Pumps `upstream` to the caller, then settles the stream.
    ///
    /// A terminator stores `terminated`. An abnormal stop at an event boundary
    /// sends the ingress terminal error within [`TERMINAL_GRACE`] and stores
    /// `terminated` once it is accepted; every other end leaves the stream
    /// unterminated so the caller aborts. Every end then sends the
    /// [`StreamEnd`] on `end` while the frame channel is still open: the
    /// terminator's outcome and usage, the stop's outcome with unknown usage,
    /// or `Cancelled` when the caller detached.
    async fn run(
        mut self,
        upstream: ProviderByteStream,
        end: oneshot::Sender<StreamEnd>,
        terminated: Arc<AtomicBool>,
    ) {
        let (outcome, usage) = match self.pump(upstream).await {
            Ended::Terminated { outcome, usage } => {
                terminated.store(true, Ordering::Release);
                (outcome, usage)
            }
            Ended::Failed { stop, boundary } => {
                if let Some(frame) = boundary
                    .then(|| error_frame(self.wire, &self.frames, stop))
                    .flatten()
                    && let Ok(Ok(())) = tokio::time::timeout(
                        TERMINAL_GRACE,
                        self.sender.send(Frame::mandatory(frame, &self.memory)),
                    )
                    .await
                {
                    terminated.store(true, Ordering::Release);
                }
                (stop.outcome(), AttemptUsage::default())
            }
            Ended::Detached => (GatewayCallOutcome::Cancelled, AttemptUsage::default()),
        };
        // A caller that stopped accounting has nothing left to record.
        end.send(StreamEnd {
            outcome,
            terminal_at: Utc::now(),
            usage,
            capture: self.capture.take().and_then(StreamCapture::finish),
            last_byte: self.last_byte,
            relay: self.busy,
        })
        .ok();
    }

    /// Charges `frame` and sends it as pieces of at most [`FRAME_BYTES`]
    /// unless process memory cannot hold it, which discards it, the caller
    /// is gone, the deadline passes, or cancellation fires.
    ///
    /// # Errors
    ///
    /// Returns the [`Ended`] state the interruption implies. It is at an
    /// event boundary only when no piece of `frame` was sent yet.
    async fn deliver(&self, frame: Vec<u8>) -> Result<(), Ended> {
        let Ok(pieces) = Frame::split(frame, FRAME_BYTES, &self.memory) else {
            return Err(Ended::Failed {
                stop: Stop::Memory,
                boundary: true,
            });
        };
        for (sent, piece) in pieces.enumerate() {
            let boundary = sent == 0;
            let stop = |stop| Ended::Failed { stop, boundary };
            tokio::select! {
                biased;
                () = self.cancel.cancelled() => return Err(stop(Stop::Cancelled)),
                () = tokio::time::sleep_until(self.deadline) => return Err(stop(Stop::Timeout)),
                sent = self.sender.send(piece) => sent.map_err(|_| Ended::Detached)?,
            }
        }
        Ok(())
    }

    /// Moves upstream events to the caller until the upstream ends.
    ///
    /// Each chunk is decoded one line at a time, so the relay knows exactly
    /// which byte completes the terminator. Native chunks forward unchanged
    /// through that byte after their events are read for usage and the
    /// terminator; translated chunks are sent as `data:` frames, followed at
    /// the end by the optional usage chunk and `[DONE]`. Once the terminator
    /// is delivered the stream settles immediately: the unread upstream body,
    /// including any later bytes, is dropped rather than awaited. When capture
    /// is selected, each native JSON event or translated chunk is kept as it
    /// is relayed; a native non-JSON event other than the Chat Completions
    /// `[DONE]` sentinel drops the capture.
    ///
    /// Returns [`Ended::Terminated`] with the stream's usage once the
    /// terminator is delivered, [`Ended::Detached`] when the caller left, and
    /// otherwise [`Ended::Failed`], whose boundary is the decoder's idle state
    /// for native streams and always true for translated ones.
    async fn pump(&mut self, mut upstream: ProviderByteStream) -> Ended {
        let mut decoder = SseDecoder::default();
        let mut merged = Map::new();
        let mut finished = false;
        let mut outcome = GatewayCallOutcome::Succeeded;
        let wire = self.wire;
        let responses = matches!(self.frames, Frames::Native { responses: true });
        loop {
            let failed = |stop, decoder: &SseDecoder, frames: &Frames| Ended::Failed {
                stop,
                boundary: matches!(frames, Frames::Chat(_)) || decoder.is_idle(),
            };
            let chunk = tokio::select! {
                biased;
                () = self.sender.closed() => return Ended::Detached,
                () = self.cancel.cancelled() => return failed(Stop::Cancelled, &decoder, &self.frames),
                () = tokio::time::sleep_until(self.deadline) => return failed(Stop::Timeout, &decoder, &self.frames),
                chunk = upstream.chunk() => chunk,
            };
            let mut bytes = match chunk {
                Ok(Some(bytes)) => bytes,
                Ok(None) => break,
                Err(_) => return failed(Stop::Upstream, &decoder, &self.frames),
            };
            let received = std::time::Instant::now();
            self.last_byte = Some(received);
            let mut out = Vec::new();
            let mut read = 0;
            while read < bytes.len() && !finished {
                let line = bytes[read..]
                    .iter()
                    .position(|byte| *byte == b'\n')
                    .map_or(bytes.len(), |end| read + end + 1);
                let Ok(events) = decoder.push("gateway", &bytes[read..line], self.limit) else {
                    return failed(Stop::Upstream, &decoder, &self.frames);
                };
                read = line;
                for data in events {
                    let event = serde_json::from_str::<Value>(&data).ok();
                    if !finished && terminates(wire, responses, &data, event.as_ref()) {
                        finished = true;
                        if responses
                            && event.as_ref().and_then(|event| event.get("type"))
                                != Some(&Value::from("response.completed"))
                        {
                            outcome = GatewayCallOutcome::Failed;
                        }
                    }
                    if self
                        .relay_event(&data, event, &mut merged, &mut out)
                        .is_none()
                    {
                        return failed(Stop::Upstream, &decoder, &self.frames);
                    }
                }
            }
            if matches!(self.frames, Frames::Native { .. }) {
                bytes.truncate(read);
                out.push(bytes);
            }
            self.busy += received.elapsed();
            for frame in out {
                if let Err(ended) = self.deliver(frame).await {
                    return match ended {
                        Ended::Failed {
                            stop,
                            boundary: true,
                        } => failed(stop, &decoder, &self.frames),
                        other => other,
                    };
                }
            }
            if finished {
                break;
            }
        }
        drop(upstream);
        if !finished {
            return Ended::Failed {
                stop: Stop::Upstream,
                boundary: matches!(self.frames, Frames::Chat(_)) || decoder.is_idle(),
            };
        }
        self.settle(merged, outcome).await
    }

    /// Relays one decoded event `data`, with its JSON `event` when it parsed.
    ///
    /// Native events add their usage to `merged` and are kept for a selected
    /// capture (a native non-JSON event other than the Chat Completions
    /// `[DONE]` sentinel drops it); their bytes are forwarded by the caller.
    /// Translated events add their usage and push each resulting Chat chunk
    /// frame onto `out`, keeping each chunk for a selected capture.
    ///
    /// Returns `None` when a translated event is not JSON, does not translate,
    /// or yields a chunk that cannot be framed; the stream then fails upstream.
    fn relay_event(
        &mut self,
        data: &str,
        event: Option<Value>,
        merged: &mut Map<String, Value>,
        out: &mut Vec<Vec<u8>>,
    ) -> Option<()> {
        let wire = self.wire;
        match (&mut self.frames, event) {
            (Frames::Native { .. }, Some(event)) => {
                merge_usage(wire, &event, merged);
                if let Some(capture) = &mut self.capture {
                    capture.push(Some(event), data.len());
                }
            }
            (Frames::Native { responses }, None) => {
                let sentinel = wire == Wire::OpenAi && !*responses && data == "[DONE]";
                if let Some(capture) = self.capture.as_mut().filter(|_| !sentinel) {
                    capture.push(None, data.len());
                }
            }
            (Frames::Chat(chat), Some(event)) => {
                merge_usage(wire, &event, merged);
                for chunk in chat.event(wire, event)? {
                    let frame = frame(&chunk)?;
                    if let Some(capture) = &mut self.capture {
                        capture.push(serde_json::to_value(&chunk).ok(), frame.len());
                    }
                    out.push(frame);
                }
            }
            (Frames::Chat(_), None) => return None,
        }
        Some(())
    }

    /// Ends a stream whose terminator was delivered.
    ///
    /// A translated stream first delivers its optional usage chunk and
    /// `[DONE]`; the stream's usage is then parsed from `merged`, the usage
    /// objects its events carried.
    ///
    /// Returns [`Ended::Terminated`] with the terminator's `outcome` and that
    /// usage, or the [`Ended`] state of a tail frame that could not be encoded
    /// or delivered.
    async fn settle(&mut self, merged: Map<String, Value>, outcome: GatewayCallOutcome) -> Ended {
        let wire = self.wire;
        if let Frames::Chat(chat) = &self.frames {
            let mut tail = Vec::new();
            if let Some(chunk) = chat.usage(wire, &merged) {
                let Some(frame) = frame(&chunk) else {
                    return Ended::Failed {
                        stop: Stop::Upstream,
                        boundary: true,
                    };
                };
                if let Some(capture) = &mut self.capture {
                    capture.push(serde_json::to_value(&chunk).ok(), frame.len());
                }
                tail.push(frame);
            }
            tail.push(DONE_FRAME.to_vec());
            for frame in tail {
                if let Err(ended) = self.deliver(frame).await {
                    return ended;
                }
            }
        }
        if merged.is_empty() {
            return Ended::Terminated {
                outcome,
                usage: AttemptUsage::default(),
            };
        }
        let key = if wire == Wire::Google {
            "usageMetadata"
        } else {
            "usage"
        };
        Ended::Terminated {
            outcome,
            usage: usage(
                wire,
                &Value::Object(Map::from_iter([(key.to_owned(), Value::Object(merged))])),
            ),
        }
    }
}

/// Overlays the usage object `event` carries onto `merged`: `OpenAI` `usage`
/// or a Responses event's `response.usage`, Anthropic `message.usage` then
/// each later `usage`, Google `usageMetadata`, or Jev `usage`, though Jev
/// never streams. At most [`USAGE_MEMBERS`] members are remembered.
fn merge_usage(wire: Wire, event: &Value, merged: &mut Map<String, Value>) {
    let found = match wire {
        Wire::Jev => event.get("usage"),
        Wire::OpenAi => event
            .get("usage")
            .or_else(|| event.pointer("/response/usage")),
        Wire::Anthropic => event
            .get("usage")
            .or_else(|| event.pointer("/message/usage")),
        Wire::Google => event
            .get("usageMetadata")
            .or_else(|| event.get("usage_metadata")),
    };
    if let Some(Value::Object(object)) = found {
        for (member, value) in object {
            if merged.len() < USAGE_MEMBERS || merged.contains_key(member) {
                merged.insert(member.clone(), value.clone());
            }
        }
    }
}

/// One server-sent event frame carrying `chunk`.
fn frame(chunk: &OpenAiChatStreamChunk) -> Option<Vec<u8>> {
    let mut bytes = b"data: ".to_vec();
    serde_json::to_writer(&mut bytes, chunk).ok()?;
    bytes.extend_from_slice(b"\n\n");
    Some(bytes)
}

#[cfg(test)]
mod tests {
    use serde_json::{Map, json};

    use wyrd_runtime::resources::ByteLedger;

    use super::{MediaCapture, StreamCapture, USAGE_MEMBERS, Wire, merge_usage};

    /// A provider sending a new usage member in every event cannot grow what
    /// the relay remembers past [`USAGE_MEMBERS`], and a remembered member
    /// still takes its latest value.
    #[test]
    fn merged_usage_keeps_a_bounded_set_of_members() {
        let mut merged = Map::new();
        for member in 0..1_000 {
            merge_usage(
                Wire::OpenAi,
                &json!({"usage": {format!("member_{member}"): member}}),
                &mut merged,
            );
        }
        merge_usage(
            Wire::OpenAi,
            &json!({"usage": {"member_0": "latest"}}),
            &mut merged,
        );
        assert_eq!(merged.len(), USAGE_MEMBERS);
        assert_eq!(merged["member_0"], "latest", "a kept member still updates");
    }

    /// A stream capture is charged for what it keeps; an event process
    /// memory cannot also hold drops the capture and returns its charge.
    #[test]
    fn a_stream_capture_is_charged_until_memory_refuses_it() {
        let ledger = ByteLedger::new(100);
        let mut capture = StreamCapture::new(None, &ledger.empty_charge());
        capture.push(Some(json!({"n": 1})), 40);
        capture.push(Some(json!({"n": 2})), 40);
        assert_eq!(ledger.used(), 80, "kept events are charged");
        capture.push(Some(json!({"n": 3})), 40);
        assert_eq!(ledger.used(), 0, "a refused event drops the capture");
        assert!(capture.finish().is_none(), "nothing is captured");
    }

    /// A media capture is charged for the bytes it keeps; bytes process
    /// memory cannot also hold drop the capture and return its charge.
    #[test]
    fn a_media_capture_is_charged_until_memory_refuses_it() {
        let ledger = ByteLedger::new(10);
        let mut capture = MediaCapture::new(None, "audio/mpeg", &ledger.empty_charge());
        capture.keep(&[0; 6]);
        assert_eq!(ledger.used(), 6, "kept bytes are charged");
        capture.keep(&[0; 6]);
        assert_eq!(ledger.used(), 0, "refused bytes drop the capture");
        assert!(capture.finish().is_none(), "nothing is captured");
    }
}
