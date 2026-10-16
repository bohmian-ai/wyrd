//! The shared Python sandbox behind `code` blocks and the `code.run` tool.
//!
//! [`Sandbox`] owns one process-wide Monty pool of hardened
//! `wyrd-monty-worker` processes (see that crate for the in-worker
//! isolation). Each [`Sandbox::run`] admits at most `max_workers +
//! max_waiters` operations, checks out a fresh worker within the operation's
//! deadline, caps the worker's address space, feeds the source plus one
//! entrypoint call, answers every suspension (mounted file calls from the
//! call's mounts, everything else refused), and drops the checkout, which
//! kills the worker: no interpreter state or process survives into another
//! operation. A watchdog kills the largest busy worker while the combined
//! resident memory of all workers exceeds the pool ceiling.
//!
//! [`CodeExecutor`] runs `code` actions and checks on the sandbox. Values
//! cross the boundary as JSON text through the sandbox's `json` module, so
//! no host object or callback is ever exposed to Python.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use async_trait::async_trait;
use monty_pool::{
    MountSpec, Pool, PoolConfig, PoolError, ReplConfig, ResumeValue, TurnEvent, on_print_sync,
};
use monty_types::{MontyObject, ResourceLimits};
use rustix::process::{Pid, Resource, Rlimit};
use serde_json::{Map, Value};
use tokio::sync::Semaphore;
use tokio::task::JoinHandle;
use tokio::time::Instant;
use vala_eval::blocks::{
    ActionExecutor, ActionOutput, BlockCall, BlockError, CallInput, CallValue, CheckExecutor,
    CheckOutput, EvidenceReader,
};
use wyrd_spec::auth::Sha256Hex;
use wyrd_spec::card::blocks::{Action, Check, CodeCall, CodeSource};
use wyrd_spec::ids::SlotName;
use wyrd_spec::verification::execution::{CallProvenance, Verdict};

use super::HostedLimits;
use super::evidence::TenantEvidence;

/// How often the watchdog samples worker memory.
const WATCHDOG_INTERVAL: Duration = Duration::from_millis(100);

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
    /// PIDs of checked-out workers, which the watchdog may kill.
    busy: Arc<Mutex<BTreeSet<u32>>>,
    /// Limits in force.
    limits: HostedLimits,
    /// The group-memory watchdog, stopped on drop.
    watchdog: JoinHandle<()>,
}

impl Sandbox {
    /// Start the pool over the worker binary at `worker`, prewarming
    /// `monty_min_workers`, and start the memory watchdog.
    ///
    /// # Errors
    /// Returns [`PoolError::Spawn`] when a prewarmed worker cannot start.
    pub async fn start(worker: PathBuf, limits: HostedLimits) -> Result<Self, PoolError> {
        let mut config = PoolConfig::subprocess(worker);
        config.min_processes = limits.monty_min_workers;
        config.max_processes = limits.monty_max_workers;
        // One checkout per process: a worker never serves a second operation.
        config.max_checkouts_per_worker = Some(1);
        // ponytail: replacements spawn on demand, so the warm set shrinks to
        // zero under load; prewarm replacements if spawn latency matters.
        let pool = Arc::new(Pool::new(config).await?);
        let busy = Arc::new(Mutex::new(BTreeSet::new()));
        let watchdog = tokio::spawn(watch(
            Arc::clone(&pool),
            Arc::clone(&busy),
            limits.monty_pool_memory_bytes,
        ));
        Ok(Self {
            pool,
            admission: Semaphore::new(limits.monty_max_workers + limits.monty_max_waiters),
            busy,
            limits,
            watchdog,
        })
    }

    /// Run `source`, then return `entrypoint(**inputs)` as JSON, through
    /// [`Self::exec`].
    ///
    /// # Errors
    /// Returns [`BlockError::Retryable`] when admission is full, no worker
    /// frees up in time, or the pool fails, and [`BlockError::Terminal`] for
    /// an invalid entrypoint, a Python error (including memory exhaustion),
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
    /// # Errors
    /// Returns [`BlockError::Retryable`] when admission is full, no worker
    /// frees up in time, or the pool fails, and [`BlockError::Terminal`] for
    /// a Python error (including memory exhaustion), a crashed or killed
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
            .map_err(|_| retryable("the Python sandbox is at capacity".to_owned()))?;
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
            .map_err(|_| retryable("no Python worker was free before the deadline".to_owned()))?
            .map_err(|error| failed(&error))?;
        let pid = checkout.pid();
        if let Some(pid) = pid {
            self.cap(pid)?;
            self.busy
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .insert(pid);
        }
        let budget = self.limits.tool_result_max_bytes;
        let mut output = String::new();
        let mut on_print = on_print_sync(|_, text: &str| {
            if output.len() < budget {
                output.push_str(&text[..text.len().min(budget - output.len())]);
            }
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
        if let Some(pid) = pid {
            self.busy
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .remove(&pid);
        }
        drop(checkout);
        drop(on_print);
        let value = ran
            .map_err(|_| terminal("Python exceeded its time limit".to_owned()))?
            .map_err(|error| failed(&error))?;
        Ok((value, output))
    }

    /// Cap worker `pid`'s address space at the process ceiling before it
    /// runs any code.
    ///
    /// # Errors
    /// Returns [`BlockError::Retryable`] when the limit cannot be applied;
    /// the worker then never runs code.
    fn cap(&self, pid: u32) -> Result<(), BlockError> {
        let limit = Some(self.limits.monty_process_memory_bytes);
        i32::try_from(pid)
            .ok()
            .and_then(Pid::from_raw)
            .ok_or_else(|| retryable("the Python worker has no process id".to_owned()))
            .and_then(|pid| {
                rustix::process::prlimit(
                    Some(pid),
                    Resource::As,
                    Rlimit {
                        current: limit,
                        maximum: limit,
                    },
                )
                .map_err(|error| retryable(format!("the worker memory cap failed: {error}")))
            })
            .map(drop)
    }
}

impl Drop for Sandbox {
    /// Stop the watchdog; the pool kills its workers when dropped.
    fn drop(&mut self) {
        self.watchdog.abort();
    }
}

/// Kill the largest busy worker whenever all workers together hold more
/// than `ceiling` resident bytes; idle workers count toward the total.
///
/// ponytail: sampled every [`WATCHDOG_INTERVAL`], so a burst can overshoot
/// between samples; a cgroup `memory.max` on the worker group is the hard
/// upgrade path when the deployment can delegate cgroups.
async fn watch(pool: Arc<Pool>, busy: Arc<Mutex<BTreeSet<u32>>>, ceiling: u64) {
    let mut ticks = tokio::time::interval(WATCHDOG_INTERVAL);
    loop {
        ticks.tick().await;
        let busy: Vec<u32> = busy
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .iter()
            .copied()
            .collect();
        let idle: u64 = pool.idle_worker_pids().into_iter().map(resident).sum();
        let sizes: Vec<(u64, u32)> = busy.into_iter().map(|pid| (resident(pid), pid)).collect();
        let total = idle + sizes.iter().map(|(bytes, _)| bytes).sum::<u64>();
        if total > ceiling
            && let Some((_, pid)) = sizes.iter().max()
            && let Some(pid) = i32::try_from(*pid).ok().and_then(Pid::from_raw)
        {
            tracing::warn!(
                total,
                ceiling,
                "killing the largest Python worker over the pool memory ceiling"
            );
            let _ = rustix::process::kill_process(pid, rustix::process::Signal::KILL);
        }
    }
}

/// Resident bytes of process `pid`, or zero once it is gone.
fn resident(pid: u32) -> u64 {
    std::fs::read_to_string(format!("/proc/{pid}/status"))
        .ok()
        .and_then(|status| {
            status
                .lines()
                .find_map(|line| line.strip_prefix("VmRSS:"))
                .and_then(|kib| kib.trim().trim_end_matches("kB").trim().parse::<u64>().ok())
        })
        .map_or(0, |kib| kib * 1024)
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
    /// result and the provenance naming the executed source.
    ///
    /// # Errors
    /// Returns [`BlockError::Terminal`] for an Artifact source or an
    /// evidence input, plus [`Sandbox::run`] failures.
    async fn run(
        &self,
        call: &CodeCall,
        inputs: &BTreeMap<String, CallInput>,
        deadline: Instant,
    ) -> Result<(Value, CallProvenance), BlockError> {
        let CodeSource::Inline(source) = &call.source else {
            // ponytail: Artifact code sources need the Artifact file reader
            // the execution composes; until then they are refused, never run.
            return Err(terminal(
                "Artifact code sources are not readable yet".to_owned(),
            ));
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

/// Classify a pool failure: capacity and transport are retryable; anything
/// the code itself caused is terminal.
fn failed(error: &PoolError) -> BlockError {
    match error {
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
