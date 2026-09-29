//! `JudgeInvoker` async trait and deterministic test invoker.
//!
//! The trait is owned by the engine; concrete impls live behind the opt-in
//! `orchestrator` feature or in other crates:
//!
//! - production: `vala-eval::orchestrator::SkaldJudgeInvoker` wraps
//!   `skald-agent::Agent::run_with` with structured output.
//! - tests: a deterministic mock invoker ships alongside the trait body in
//!   Commit 9 so engine tests stay skald-free.

use std::collections::VecDeque;
use std::sync::Arc;

use async_trait::async_trait;
use serde_json::Value;
use tokio::sync::Mutex;
use wyrd_spec::card::agent::AgentSpec;
use wyrd_spec::reference::InlineableRef;

use crate::tasks::MediaBindings;

/// Failures returned by one judge invocation attempt.
#[derive(Debug, thiserror::Error)]
pub enum JudgeError {
    /// Retryable provider-side failure.
    #[error("retryable provider failure: {reason}")]
    Retryable { reason: String },
    /// Retryable timeout.
    #[error("retryable timeout after {elapsed_ms} ms")]
    Timeout { elapsed_ms: u64 },
    /// Terminal malformed structured output.
    #[error("terminal invalid structured output: {reason}")]
    InvalidStructuredOutput { reason: String },
    /// Terminal provider rejection.
    #[error("terminal provider rejection: {reason}")]
    Terminal { reason: String },
}

impl JudgeError {
    /// True when the executor may spend retry budget on this error.
    #[must_use]
    pub fn is_retryable(&self) -> bool {
        matches!(self, Self::Retryable { .. } | Self::Timeout { .. })
    }
}

/// Dependency-inversion boundary for constrained Agent-backed LLM judge calls.
#[async_trait]
pub trait JudgeInvoker: Send + Sync {
    /// Invoke the judge once.
    ///
    /// `judge` is a durable or inline Agent reference. `context` is the JSON object the
    /// engine prepared from the task's dependencies and optional context path.
    /// `media` carries the record's named media descriptors; an implementation
    /// binds each as provider-native content into the judge Prompt's matching
    /// `${media:id}` placeholder and never renders a descriptor as text.
    ///
    /// # Errors
    /// Returns [`JudgeError`] when the invocation fails, media cannot be
    /// resolved or bound, or the judge returns invalid structured output.
    async fn invoke(
        &self,
        judge: &InlineableRef<AgentSpec>,
        context: Value,
        media: &MediaBindings,
    ) -> Result<Value, JudgeError>;
}

/// Deterministic in-crate invoker for executor tests.
pub struct MockJudgeInvoker {
    scripted: Mutex<VecDeque<Result<Value, JudgeError>>>,
    seen: Mutex<Vec<(InlineableRef<AgentSpec>, Value)>>,
    /// Media bindings received by each call, in invocation order.
    seen_media: Mutex<Vec<MediaBindings>>,
}

impl MockJudgeInvoker {
    /// Build a mock with the scripted attempt outcomes.
    #[must_use]
    pub fn new(scripted: impl IntoIterator<Item = Result<Value, JudgeError>>) -> Arc<Self> {
        Arc::new(Self {
            scripted: Mutex::new(scripted.into_iter().collect()),
            seen: Mutex::new(Vec::new()),
            seen_media: Mutex::new(Vec::new()),
        })
    }

    /// Captured `(judge_ref, context)` calls in invocation order.
    pub async fn calls(&self) -> Vec<(InlineableRef<AgentSpec>, Value)> {
        self.seen.lock().await.clone()
    }

    /// Media bindings passed to each call, in invocation order.
    pub async fn media_calls(&self) -> Vec<MediaBindings> {
        self.seen_media.lock().await.clone()
    }
}

#[async_trait]
impl JudgeInvoker for MockJudgeInvoker {
    async fn invoke(
        &self,
        judge: &InlineableRef<AgentSpec>,
        context: Value,
        media: &MediaBindings,
    ) -> Result<Value, JudgeError> {
        self.seen_media.lock().await.push(media.clone());
        self.seen
            .lock()
            .await
            .push((judge.clone(), context.clone()));
        let mut scripted = self.scripted.lock().await;
        match scripted.pop_front() {
            Some(result) => result,
            None => Err(JudgeError::Terminal {
                reason: "mock judge invoker has no scripted response".to_owned(),
            }),
        }
    }
}
