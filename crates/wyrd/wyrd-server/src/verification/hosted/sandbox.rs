//! The shared Python sandbox behind `code` blocks and the `code.run` tool.
//!
//! [`Sandbox`] owns one process-wide Monty pool of `wyrd-server
//! subprocess` worker processes (see [`super::worker`] for
//! the in-worker isolation). Each [`Sandbox::run`] admits at most `max_workers +
//! max_waiters` operations, checks out a fresh worker within the operation's
//! deadline, feeds the source plus one
//! entrypoint call, answers every suspension (mounted file calls from the
//! call's mounts, everything else refused), and drops the checkout, which
//! kills the worker: no interpreter state or process survives into another
//! operation. Memory limits hold on every platform: see [`Sandbox::start`].
//!
//! [`CodeExecutor`] runs `code` actions and checks on the sandbox. Values
//! cross the boundary as JSON text through the sandbox's `json` module, so
//! no host object or callback is ever exposed to Python.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use monty_pool::{
    MountSpec, Pool, PoolConfig, PoolError, ReplConfig, ResumeValue, TurnEvent, on_print_sync,
};
use monty_types::{MontyObject, ResourceLimits};
use serde_json::{Map, Value};
use tokio::sync::Semaphore;
use tokio::time::Instant;
use vala_verify::blocks::{
    ActionExecutor, ActionOutput, BlockCall, BlockError, CallInput, CallValue, CheckExecutor,
    CheckOutput, EvidenceReader,
};
use wyrd_spec::auth::Sha256Hex;
use wyrd_spec::card::verifier::{Action, Check, CodeCall, CodeSource};
use wyrd_spec::ids::SlotName;
use wyrd_spec::verification::execution::{CallProvenance, Verdict};

use super::HostedLimits;
use super::evidence::TenantEvidence;

/// The sandbox global holding the JSON-encoded entrypoint arguments.
const INPUTS_GLOBAL: &str = "_wyrd_inputs";

/// The bounded result of one Python operation.
#[derive(Debug, Clone, PartialEq)]
pub struct CodeRun {
    /// The entrypoint's return value.
    pub value: Value,
    /// Printed output, cut at the tool result limit.
    pub output: String,
}

/// The process-wide Python worker pool.
pub struct Sandbox {
    /// Hardened worker processes.
    pool: Arc<Pool>,
    /// Admission: one permit per running or waiting operation.
    admission: Semaphore,
    /// Limits in force.
    limits: HostedLimits,
}

impl Sandbox {
    /// Start the pool over the worker binary at `worker`, prewarming
    /// `monty_min_workers`.
    ///
    /// Memory is bounded portably: each worker's global allocator stops its
    /// whole heap at the operation budget plus Monty's headroom, which must
    /// fit the process ceiling, and live workers are capped so that their
    /// process ceilings together fit the pool ceiling. Idle workers count,
    /// because the cap is on live processes.
    ///
    /// # Errors
    /// Returns [`PoolError::Spawn`] when the limits cannot hold even one
    /// worker within its ceilings or a prewarmed worker cannot start.
    pub async fn start(worker: PathBuf, limits: HostedLimits) -> Result<Self, PoolError> {
        let workers = limits.live_workers().ok_or_else(|| {
            PoolError::Spawn(
                "the Monty memory limits cannot hold one worker within its ceilings".into(),
            )
        })?;
        let mut config = PoolConfig::subprocess(worker);
        config.min_processes = limits.monty_min_workers.min(workers);
        config.max_processes = workers;
        // One checkout per process: a worker never serves a second operation.
        config.max_checkouts_per_worker = Some(1);
        // ponytail: replacements spawn on demand, so the warm set shrinks to
        // zero under load; prewarm replacements if spawn latency matters.
        Ok(Self {
            pool: Arc::new(Pool::new(config).await?),
            admission: Semaphore::new(workers + limits.monty_max_waiters),
            limits,
        })
    }

    /// Run `source`, then return `entrypoint(**inputs)` as JSON, through
    /// [`Self::exec`].
    ///
    /// # Errors
    /// Returns [`BlockError::Busy`] when admission is full or no worker frees
    /// up in time, [`BlockError::Retryable`] when the pool fails, and
    /// [`BlockError::Terminal`] for an invalid entrypoint, a Python error (including memory exhaustion),
    /// a crashed or killed worker, a timeout while running, or a result
    /// that is not JSON.
    pub async fn run(
        &self,
        source: &str,
        entrypoint: &str,
        inputs: &Map<String, Value>,
        mounts: Vec<MountSpec>,
        deadline: Instant,
    ) -> Result<CodeRun, BlockError> {
        if entrypoint.is_empty()
            || entrypoint.starts_with(|c: char| c.is_ascii_digit())
            || !entrypoint
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_')
        {
            return Err(terminal(format!(
                "{entrypoint:?} is not a Python function name"
            )));
        }
        let code = format!(
            "{source}\n\nimport json as _wyrd_json\n_wyrd_json.dumps({entrypoint}(**_wyrd_json.loads({INPUTS_GLOBAL})))\n"
        );
        let arguments = vec![(
            INPUTS_GLOBAL.to_owned(),
            MontyObject::string(Value::Object(inputs.clone()).to_string()),
        )];
        let (value, output) = self.exec(code, arguments, mounts, deadline).await?;
        let text = String::try_from(&value)
            .map_err(|_| terminal("the entrypoint result is not JSON text".to_owned()))?;
        Ok(CodeRun {
            value: serde_json::from_str(&text)
                .map_err(|error| terminal(format!("the entrypoint result is not JSON: {error}")))?,
            output,
        })
    }

    /// Run `code` with `globals` on a fresh worker and return the value of
    /// its last expression plus its bounded printed output.
    ///
    /// The operation ends by `deadline` or the code timeout, whichever is
    /// first, including the wait for a worker. `mounts` are the only paths
    /// the code can reach; every other OS call, host function, or undefined
    /// name is refused inside the sandbox. The worker is killed afterwards.
    ///
    /// Admission refusal and a worker wait that ends unserved are
    /// [`BlockError::Busy`]: the process, not the code, lacked capacity, so
    /// the engine defers the whole execution instead of judging it.
    ///
    /// # Errors
    /// Returns [`BlockError::Busy`] when admission is full or no worker frees
    /// up in time, [`BlockError::Retryable`] when the pool fails, and
    /// [`BlockError::Terminal`] for a Python error (including memory exhaustion), a crashed or killed
    /// worker, or a timeout while running.
    pub async fn exec(
        &self,
        code: String,
        globals: Vec<(String, MontyObject)>,
        mounts: Vec<MountSpec>,
        deadline: Instant,
    ) -> Result<(MontyObject, String), BlockError> {
        let _admitted = self
            .admission
            .try_acquire()
            .map_err(|_| busy("the Python sandbox is at capacity".to_owned()))?;
        let deadline = deadline.min(Instant::now() + self.limits.code_timeout);
        let repl = ReplConfig {
            limits: Some(ResourceLimits {
                max_memory: usize::try_from(self.limits.monty_memory_bytes).ok(),
                ..ResourceLimits::default()
            }),
            ..ReplConfig::default()
        };
        let mut checkout = tokio::time::timeout_at(deadline, self.pool.checkout(&repl))
            .await
            .map_err(|_| busy("no Python worker was free before the deadline".to_owned()))?
            .map_err(|error| failed(&error))?;
        let budget = self.limits.tool_result_max_bytes;
        let mut output = String::new();
        let mut on_print = on_print_sync(|_, text: &str| {
            output.push_str(&text[..text.floor_char_boundary(budget - output.len())]);
        });
        let ran = tokio::time::timeout_at(deadline, async {
            let mut event = checkout
                .feed(code, globals, mounts, true, &mut on_print)
                .await?;
            loop {
                event = match event {
                    TurnEvent::Complete(value) => return Ok(value),
                    TurnEvent::OsCall { .. } => {
                        match checkout.resume_from_mounts(&mut on_print).await? {
                            Some(event) => event,
                            None => {
                                checkout
                                    .resume(ResumeValue::NotHandled, &mut on_print)
                                    .await?
                            }
                        }
                    }
                    TurnEvent::FunctionCall { .. } => {
                        checkout
                            .resume(ResumeValue::NotFound, &mut on_print)
                            .await?
                    }
                    TurnEvent::NameLookup { .. } => {
                        checkout
                            .resume_name_lookup(None::<MontyObject>, &mut on_print)
                            .await?
                    }
                    TurnEvent::ResolveFutures { .. } => {
                        return Err(PoolError::Protocol(
                            "host futures are unavailable to sandboxed Python".into(),
                        ));
                    }
                };
            }
        })
        .await;
        drop(checkout);
        drop(on_print);
        let value = ran
            .map_err(|_| terminal("Python exceeded its time limit".to_owned()))?
            .map_err(|error| failed(&error))?;
        Ok((value, output))
    }
}

/// Runs `code` actions and checks for one execution.
pub struct CodeExecutor {
    /// The shared sandbox.
    sandbox: Arc<Sandbox>,
    /// The execution's evidence, for citation validation.
    evidence: Arc<TenantEvidence>,
}

impl CodeExecutor {
    /// Code execution over `sandbox` citing `evidence`.
    #[must_use]
    pub const fn new(sandbox: Arc<Sandbox>, evidence: Arc<TenantEvidence>) -> Self {
        Self { sandbox, evidence }
    }

    /// Run `call` with the bound JSON `inputs` and no mounts, returning its
    /// result and the provenance naming the executed source's digest.
    ///
    /// Inline source runs as written; an Artifact source is loaded through
    /// the execution's [`TenantEvidence::artifact`] and must be UTF-8.
    ///
    /// # Errors
    /// Returns [`BlockError::Terminal`] for an unresolved Artifact path
    /// reference, non-UTF-8 Artifact source, or an evidence input, plus
    /// [`TenantEvidence::artifact`] and [`Sandbox::run`] failures.
    async fn run(
        &self,
        call: &CodeCall,
        inputs: &BTreeMap<String, CallInput>,
        deadline: Instant,
    ) -> Result<(Value, CallProvenance), BlockError> {
        let loaded;
        let source = match &call.source {
            CodeSource::Inline(source) => source.as_str(),
            CodeSource::Artifact { card, path } => {
                let card = card.as_card_ref().ok_or_else(|| {
                    terminal("the Artifact code source was never resolved".to_owned())
                })?;
                let (bytes, _) = self.evidence.artifact(card, path.as_deref()).await?;
                loaded = String::from_utf8(bytes)
                    .map_err(|_| terminal("the Artifact code source is not UTF-8".to_owned()))?;
                loaded.as_str()
            }
        };
        let mut arguments = Map::new();
        for (name, input) in inputs {
            let CallValue::Json(value) = &input.value else {
                return Err(terminal(format!(
                    "code input {name} is media; bind an extract step's output instead"
                )));
            };
            arguments.insert(name.clone(), value.clone());
        }
        let ran = self
            .sandbox
            .run(source, &call.entrypoint, &arguments, Vec::new(), deadline)
            .await?;
        Ok((
            ran.value,
            CallProvenance::Code {
                sha256: Sha256Hex::digest(source.as_bytes()),
                runtime: call.runtime,
            },
        ))
    }
}

#[async_trait]
impl ActionExecutor for CodeExecutor {
    /// Run a `code` action and return its JSON result.
    ///
    /// # Errors
    /// Returns [`BlockError::Terminal`] for a non-code action, plus
    /// [`Self::run`] failures.
    async fn execute(&self, call: BlockCall<'_, Action>) -> Result<ActionOutput, BlockError> {
        let Action::Code(code) = call.block else {
            return Err(terminal(
                "the code executor runs only code actions".to_owned(),
            ));
        };
        let (output, provenance) = self.run(code, &call.inputs, call.deadline).await?;
        Ok(ActionOutput {
            output,
            provenance: Some(provenance),
            tool_calls: Vec::new(),
        })
    }
}

#[async_trait]
impl CheckExecutor for CodeExecutor {
    /// Run a `code` check and return its Verdict, keeping only citations
    /// into evidence reachable through the call's bindings.
    ///
    /// # Errors
    /// Returns [`BlockError::Terminal`] for a non-code check or a result that
    /// is not a Verdict, plus [`Self::run`] and citation-read failures.
    async fn execute(&self, call: BlockCall<'_, Check>) -> Result<CheckOutput, BlockError> {
        let Check::Code(code) = call.block else {
            return Err(terminal(
                "the code executor runs only code checks".to_owned(),
            ));
        };
        let (value, provenance) = self.run(code, &call.inputs, call.deadline).await?;
        let mut verdict: Verdict = serde_json::from_value(value).map_err(|error| {
            terminal(format!("the code check result is not a Verdict: {error}"))
        })?;
        let reachable: BTreeSet<SlotName> = call
            .inputs
            .values()
            .flat_map(|input| input.sources.iter().cloned())
            .collect();
        verdict.citations = self
            .evidence
            .cited(&reachable, std::mem::take(&mut verdict.citations))
            .await?;
        Ok(CheckOutput {
            verdict,
            actual: None,
            detail: None,
            provenance: Some(provenance),
        })
    }
}

/// Classify a pool failure: an exhausted pool is busy, transport is
/// retryable, and anything the code itself caused is terminal.
fn failed(error: &PoolError) -> BlockError {
    match error {
        PoolError::Exhausted => busy("no Python worker was free".to_owned()),
        PoolError::Runtime(exception) => terminal(format!("Python raised {exception}")),
        PoolError::Typing(message) => terminal(format!("Python failed to compile: {message}")),
        PoolError::Crashed { .. } => terminal("the Python worker stopped".to_owned()),
        PoolError::Timeout { .. } => terminal("Python exceeded its time limit".to_owned()),
        other => retryable(format!("the Python sandbox failed: {other}")),
    }
}

/// A terminal error with `reason`.
const fn terminal(reason: String) -> BlockError {
    BlockError::Terminal { reason }
}

/// A retryable error with `reason`.
const fn retryable(reason: String) -> BlockError {
    BlockError::Retryable { reason }
}

/// A process-capacity refusal with `reason`.
const fn busy(reason: String) -> BlockError {
    BlockError::Busy { reason }
}
