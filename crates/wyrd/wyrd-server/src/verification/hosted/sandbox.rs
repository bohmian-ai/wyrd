//! The shared Python sandbox behind `code` blocks and the `code.run` tool.
//!
//! [`Sandbox`] owns one process-wide Monty pool of `wyrd-server
//! subprocess` worker processes (see [`super::worker`] for
//! the in-worker isolation). Each [`Sandbox::run`] waits its tenant's turn in
//! [`HostedAdmission`] for the session's peak memory envelope, checks out a
//! worker within the operation's deadline, feeds the source plus one
//! entrypoint call, answers every suspension (mounted file calls from the
//! call's mounts, everything else refused), and finishes the session. A
//! healthy finish resets the worker for the next operation with fresh
//! globals, mounts, callbacks, output, and allocator budget; any other end
//! kills it. Memory limits hold on every platform: see [`Sandbox::start`].
//!
//! [`CodeExecutor`] runs `code` actions and checks on the sandbox. Values
//! cross the boundary as JSON text through the sandbox's `json` module, so
//! no host object or callback is ever exposed to Python.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, Weak};
use std::time::Duration;

use async_trait::async_trait;
use futures_util::FutureExt as _;
use futures_util::StreamExt as _;
use futures_util::future::BoxFuture;
use futures_util::stream::FuturesUnordered;
use monty_pool::{
    MountSpec, Pool, PoolConfig, PoolError, ReplConfig, ResumeValue, TurnEvent, on_print_sync,
};
use monty_types::unstable::{self, MontyNode};
use monty_types::{
    CallArgs, ExcType, MontyException, MontyObject, NamedValues, ObjectRef, ResourceLimits,
    memory_limit_with_headroom,
};
use serde_json::{Map, Value};
use skald_spec::{PreparedSchemas, SharedSchema};
use skald_tool::ToolError;
use tokio::sync::Semaphore;
use tokio::time::Instant;
use tokio_util::task::TaskTracker;
use vala_verify::blocks::{
    ActionExecutor, ActionOutput, BlockCall, BlockError, CallInput, CallValue, CheckExecutor,
    CheckOutput, Delivery, EvidenceReader, ToolBridge, verdict_from,
};
use wyrd_runtime::resources::{ByteCharge, ProcessResources};
use wyrd_spec::DataTenantId;
use wyrd_spec::auth::Sha256Hex;
use wyrd_spec::card::verifier::{Action, Check, CodeCall, CodeSource};
use wyrd_spec::error::WyrdError;
use wyrd_spec::ids::SlotName;
use wyrd_spec::verification::execution::CallProvenance;

use super::HostedLimits;
use super::admission::{AdmissionRefusal, HostedAdmission};
use super::evidence::TenantEvidence;
use super::worker::WORKER_STACK_SIZE;
use super::workspace::{Workspace, WriteRefusal};
use crate::app::metrics::{
    VERIFICATION_HOSTED_BYTES, VERIFICATION_HOSTED_OPERATIONS,
    VERIFICATION_PYTHON_WORKER_EVENTS_TOTAL, VERIFICATION_PYTHON_WORKERS,
};
use crate::verification::telemetry::{Envelope, Envelopes};

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

/// A worker's resident memory outside its counted heap: the interpreter
/// binary's touched pages and Monty's session state at rest.
///
/// Measured as the resident set of an idle `wyrd-server subprocess` worker
/// after a reset, rounded up; see the sandbox worker integration tests.
const WORKER_BASELINE_BYTES: u64 = 16 * 1024 * 1024;

/// How often worker charges are checked against the processes still alive.
const REAP_INTERVAL: Duration = Duration::from_millis(100);

/// The process-wide Python worker pool.
pub struct Sandbox {
    /// Hardened worker processes.
    pool: Arc<Pool>,
    /// Fair admission of session envelopes over process memory.
    admission: HostedAdmission,
    /// The resident charge of every live worker process.
    workers: Arc<Workers>,
    /// One permit per effective CPU, held only while Python runs a turn.
    runnable: Semaphore,
    /// Every operation admitted and not yet settled; closed at shutdown.
    operations: TaskTracker,
    /// The allocator ceiling plus the parent's copy of the largest frame a
    /// session can send, charged per session and kept by its worker until
    /// the worker's process is gone.
    session_bytes: u64,
    /// Limits in force.
    limits: HostedLimits,
}

impl Sandbox {
    /// Start the pool over the worker binary at `worker`, prewarming up to
    /// `monty_min_workers` that fit in `process` memory.
    ///
    /// Memory is bounded portably: each worker's global allocator stops its
    /// whole heap at the operation budget plus Monty's headroom. A session
    /// charges that ceiling twice, once for the worker heap and once for the
    /// parent's copy of the largest frame the worker can send, plus the
    /// worker's resident baseline and interpreter stack. A worker keeps that
    /// whole envelope charged until its process is gone, idle or not: its
    /// allocator keeps freed pages after a reset, so a reset returns no
    /// memory. The next session on the worker takes the charge over rather
    /// than adding to it, and idle workers are reaped whenever an operation
    /// waits for memory. There is no worker or waiter count: process memory,
    /// shared with every other capability, is the only bound.
    ///
    /// # Errors
    /// Returns [`PoolError::Spawn`] when process memory cannot hold one
    /// session's envelope with room for its request, or a prewarmed worker
    /// cannot start.
    pub async fn start(
        worker: PathBuf,
        limits: HostedLimits,
        process: &ProcessResources,
    ) -> Result<Self, PoolError> {
        let memory = process.memory();
        let resident = resident_bytes();
        let session_bytes = session_bytes(&limits)
            .filter(|session| session.saturating_add(resident) < memory.limit())
            .ok_or_else(|| {
                PoolError::Spawn("process memory cannot hold one Python session".into())
            })?;
        let mut config = PoolConfig::subprocess(worker);
        config.min_processes = limits
            .monty_min_workers
            .min(usize::try_from(memory.available() / resident).unwrap_or(usize::MAX));
        // Admission charges every worker before it can spawn, so this cap
        // never binds; it only keeps the pool's own count finite.
        config.max_processes = usize::try_from(memory.limit() / resident)
            .unwrap_or(usize::MAX)
            .max(1);
        config.max_checkouts_per_worker = None;
        let pool = Arc::new(Pool::new(config).await?);
        let workers = Arc::new(Workers {
            resident,
            charges: Mutex::default(),
        });
        for pid in pool.idle_worker_pids() {
            worker_event("spawn");
            workers.adopt(pid, memory.charge_infallible(resident));
        }
        let admission = HostedAdmission::new(memory.clone());
        tokio::spawn(reap(
            Arc::downgrade(&workers),
            Arc::downgrade(&pool),
            admission.clone(),
        ));
        Ok(Self {
            pool,
            admission,
            workers,
            runnable: Semaphore::new(process.effective_cpu().get()),
            operations: TaskTracker::new(),
            session_bytes,
            limits,
        })
    }

    /// The bytes one operation charges under `limits` when it spawns a
    /// worker: its session envelope plus the new worker's resident bytes.
    /// `None` when they overflow.
    #[must_use]
    pub fn envelope_bytes(limits: &HostedLimits) -> Option<u64> {
        session_bytes(limits)?.checked_add(resident_bytes())
    }

    /// Hold process memory so at most `leave` bytes stay free until the
    /// returned hold drops, so an operation whose waiting bytes or envelope
    /// do not fit in `leave` is refused or waits.
    ///
    /// Memory other owners release while the hold lives is absorbed into it
    /// soon after, so the free bytes return to `leave`. Memory an operation
    /// frees for itself, such as an idle worker it reaps, it can take first.
    ///
    /// # Panics
    /// Panics when called outside a Tokio runtime.
    #[cfg(feature = "test-support")]
    #[must_use]
    pub fn occupy_for_test(&self, leave: u64) -> OccupiedForTest {
        let memory = self.admission.memory().clone();
        let charge = Arc::new(std::sync::Mutex::new(Some(
            memory.charge_infallible(memory.available().saturating_sub(leave)),
        )));
        let held = Arc::clone(&charge);
        let pin = tokio::spawn(async move {
            loop {
                let changed = memory.changed();
                tokio::pin!(changed);
                changed.as_mut().enable();
                {
                    let mut held = held
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner);
                    let Some(charge) = held.as_mut() else {
                        return;
                    };
                    // A release another charge takes first is retried on
                    // the next change.
                    let _ = charge.try_grow(memory.available().saturating_sub(leave));
                }
                changed.await;
            }
        });
        OccupiedForTest { charge, pin }
    }

    /// Python turns that may run at once: the process's effective CPUs.
    #[must_use]
    pub fn runnable_turns(&self) -> usize {
        self.runnable.available_permits()
    }

    /// Python operations waiting for process memory now, and the bytes they
    /// ask for.
    #[must_use]
    pub fn waiting(&self) -> (usize, u64) {
        self.admission.waiting()
    }

    /// Bytes every live worker process holds charged while it lives.
    #[must_use]
    pub fn worker_bytes(&self) -> u64 {
        self.workers.bytes()
    }

    /// PIDs of the idle workers, for tests that follow one process across
    /// operations.
    #[cfg(feature = "test-support")]
    #[must_use]
    pub fn idle_worker_pids(&self) -> Vec<u32> {
        self.pool.idle_worker_pids()
    }

    /// PIDs of every worker still charged, idle, checked out, or awaiting
    /// its reap.
    #[cfg(feature = "test-support")]
    #[must_use]
    pub fn charged_worker_pids(&self) -> Vec<u32> {
        self.workers.lock().keys().copied().collect()
    }

    /// Settle every admitted operation, refuse new ones, and reap the idle
    /// workers, all by `deadline`; returns whether everything settled.
    ///
    /// Closes admission first, so a later [`Self::run`] answers busy, then
    /// waits until no admitted operation is running or waiting, which means
    /// no worker is checked out, because [`Pool::close`] reaps only idle
    /// workers. The pool then asks each idle worker to exit, killing one that
    /// does not within its grace. When the deadline passes first, idle
    /// workers are still reaped (a reap cut short kills its workers on drop),
    /// but `false` reports that operations were still running; their workers
    /// die with their checkouts. Repeating the call is safe and finds nothing
    /// to do.
    pub async fn shutdown(&self, deadline: Instant) -> bool {
        self.operations.close();
        let settled = tokio::time::timeout_at(deadline, self.operations.wait())
            .await
            .is_ok();
        let reaped = tokio::time::timeout_at(deadline, self.pool.close())
            .await
            .is_ok();
        settled && reaped
    }

    /// Run `source`, then return `entrypoint(**inputs)` as JSON, through
    /// [`Self::exec`].
    ///
    /// # Errors
    /// Returns [`BlockError::Busy`] when process memory refuses the
    /// operation or does not free in time, [`BlockError::Retryable`] when the
    /// pool fails, and [`BlockError::Terminal`] for an invalid entrypoint, a
    /// Python error (including memory exhaustion), a crashed or killed
    /// worker, a timeout while running, or a result that is not JSON.
    pub async fn run(
        &self,
        tenant: DataTenantId,
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
        let (value, output) = Box::pin(self.exec(
            tenant,
            code,
            arguments,
            mounts,
            deadline,
            None,
            None,
            &Envelopes::default(),
        ))
        .await?;
        let text = String::try_from(&value)
            .map_err(|_| terminal("the entrypoint result is not JSON text".to_owned()))?;
        Ok(CodeRun {
            value: serde_json::from_str(&text)
                .map_err(|error| terminal(format!("the entrypoint result is not JSON: {error}")))?,
            output,
        })
    }

    /// Run `code` with `globals` in a fresh session for `tenant` and return
    /// the value of its last expression plus its bounded printed output.
    ///
    /// The operation ends by `deadline` or the code timeout, whichever is
    /// first, including its wait for memory and a worker. `mounts` are the
    /// only paths the code can reach; every other OS call, host function, or
    /// undefined name is refused inside the sandbox.
    ///
    /// The code, its globals, and the session envelope are charged to
    /// process memory: the retained inputs before waiting, then the envelope
    /// through [`HostedAdmission::acquire`] in `tenant`'s turn. Idle workers
    /// are reaped first when the envelope does not fit. Python holds one
    /// runnable turn, bounded by effective CPUs, only while it runs; a
    /// program suspended on host calls or a `/work` writer holds none, but
    /// keeps its envelope. After a completed program or a Python error the
    /// session is finished, and the worker is reused only when that reset
    /// succeeds; every other end kills the worker, whose charge returns only
    /// once its process is gone.
    ///
    /// Memory refusal and a wait that ends unserved are [`BlockError::Busy`]:
    /// the process, not the code, lacked capacity, so the engine defers the
    /// whole execution instead of judging it.
    ///
    /// # Errors
    /// Returns [`BlockError::Busy`] when the sandbox is shutting down, process
    /// memory refuses the operation or does not free in time, or no worker
    /// starts in time, [`BlockError::Retryable`] when the pool fails, and
    /// [`BlockError::Terminal`] for a Python error (including memory
    /// exhaustion), a crashed or killed worker, or a timeout while running.
    ///
    /// With `workspace`, each Python file write waits for
    /// [`Workspace::writer`] and is refused with `OSError` when
    /// [`Workspace::admit_write`] refuses it; the caller must not hold
    /// the writer itself. With `bridge`, the program can
    /// `await call_tool(name, arguments)`; its calls run while the program
    /// is suspended, and a stopped session ends it.
    #[expect(
        clippy::too_many_arguments,
        reason = "one operation's tenant, program, inputs, mounts, deadline, and mediation"
    )]
    pub async fn exec(
        &self,
        tenant: DataTenantId,
        code: String,
        globals: Vec<(String, MontyObject)>,
        mounts: Vec<MountSpec>,
        deadline: Instant,
        workspace: Option<&Arc<Workspace>>,
        bridge: Option<&ToolBridge>,
        envelopes: &Envelopes,
    ) -> Result<(MontyObject, String), BlockError> {
        let _operation = self.operations.token();
        if self.operations.is_closed() {
            return Err(busy("the Python sandbox is shutting down".to_owned()));
        }
        let deadline = deadline.min(Instant::now() + self.limits.code_timeout);
        let retained = globals.iter().fold(code.len(), |bytes, (name, value)| {
            bytes + name.len() + value.as_ref().as_str().map_or(0, str::len)
        });
        let retained = u64::try_from(retained).unwrap_or(u64::MAX);
        let held = self.admission.hold(retained).map_err(admission_refused)?;
        let envelope = self.session_bytes + self.workers.resident;
        let mut envelope = envelopes
            .time(Envelope::Capacity, async {
                // ponytail: a new session charges a fresh envelope before
                // checkout tells which worker it gets, so reusing an idle
                // worker needs two envelopes free; below that the idle worker
                // is reaped. Charge reuse against the idle worker's envelope
                // if warm reuse under tight memory matters.
                if self.admission.memory().available() < envelope && self.pool.idle_workers() > 0 {
                    // ponytail: reaps every idle worker, not just enough of
                    // them; reap one at a time if warm-set churn under
                    // pressure matters.
                    self.pool.close().await;
                }
                self.admission
                    .acquire(tenant, held, envelope, deadline)
                    .await
            })
            .await
            .map_err(admission_refused)?;
        // The request's own bytes end with the operation; only the envelope
        // stays with the worker.
        let _retained = envelope.split(retained);
        let _active = (
            Raised::new(VERIFICATION_HOSTED_OPERATIONS, "active", 1.0),
            Raised::new(VERIFICATION_HOSTED_BYTES, "active", envelope.bytes() as f64),
        );
        let repl = ReplConfig {
            limits: Some(ResourceLimits {
                max_memory: usize::try_from(self.limits.monty_memory_bytes).ok(),
                ..ResourceLimits::default()
            }),
            ..ReplConfig::default()
        };
        let mut checkout = tokio::time::timeout_at(deadline, self.pool.checkout(&repl))
            .await
            .map_err(|_| busy("no Python worker was ready before the deadline".to_owned()))?
            .map_err(|error| failed(&error))?;
        let lease = self.workers.lease(checkout.pid(), envelope);
        let session = Raised::new(VERIFICATION_PYTHON_WORKERS, "suspended", 1.0);
        let budget = self.limits.tool_result_max_bytes;
        let mut output = String::new();
        let mut on_print = on_print_sync(|_, text: &str| {
            output.push_str(&text[..text.floor_char_boundary(budget - output.len())]);
        });
        // A host refusal that must stop the operation rather than reach
        // Python, such as process scratch with no room for a `/work` write.
        let mut stopped = None;
        let ran = tokio::time::timeout_at(deadline, async {
            let mut calls = Calls::new();
            let mut answered = Vec::new();
            let mut event = run_turn(
                &self.runnable,
                envelopes,
                checkout.feed(code, globals, mounts, true, &mut on_print),
                &mut calls,
                &mut answered,
            )
            .await?;
            loop {
                event = match event {
                    TurnEvent::Complete(value) => return Ok(Some(value)),
                    TurnEvent::OsCall {
                        function_name,
                        args,
                        ..
                    } => {
                        let writing = workspace.zip(written(&function_name, &args));
                        let _writer = match writing {
                            Some((workspace, bytes)) => {
                                let writer =
                                    drive(workspace.writer(), &mut calls, &mut answered).await;
                                let admitted = {
                                    let workspace = Arc::clone(workspace);
                                    let bytes = u64::try_from(bytes).unwrap_or(u64::MAX);
                                    drive(
                                        tokio::task::spawn_blocking(move || {
                                            workspace.admit_write(bytes)
                                        }),
                                        &mut calls,
                                        &mut answered,
                                    )
                                    .await
                                    .map_err(|error| {
                                        PoolError::Protocol(error.to_string().into())
                                    })?
                                };
                                if let Err(WriteRefusal::Scratch) = admitted {
                                    stopped = Some(busy(WriteRefusal::Scratch.to_string()));
                                    return Ok(None);
                                }
                                if let Err(refusal) = admitted {
                                    event = run_turn(
                                        &self.runnable,
                                        envelopes,
                                        checkout.resume(
                                            ResumeValue::Error(MontyException::new(
                                                ExcType::OSError,
                                                Some(refusal.to_string()),
                                            )),
                                            &mut on_print,
                                        ),
                                        &mut calls,
                                        &mut answered,
                                    )
                                    .await?;
                                    continue;
                                }
                                Some(writer)
                            }
                            None => None,
                        };
                        let mounted = run_turn(
                            &self.runnable,
                            envelopes,
                            checkout.resume_from_mounts(&mut on_print),
                            &mut calls,
                            &mut answered,
                        )
                        .await?;
                        match mounted {
                            Some(event) => event,
                            None => {
                                run_turn(
                                    &self.runnable,
                                    envelopes,
                                    checkout.resume(ResumeValue::NotHandled, &mut on_print),
                                    &mut calls,
                                    &mut answered,
                                )
                                .await?
                            }
                        }
                    }
                    TurnEvent::NameLookup {
                        name,
                        object_id: None,
                        ..
                    } if bridge.is_some() && name == CALL_TOOL => {
                        run_turn(
                            &self.runnable,
                            envelopes,
                            checkout.resume_name_lookup(
                                MontyObject::function(CALL_TOOL, Some(CALL_TOOL_DOC.to_owned())),
                                &mut on_print,
                            ),
                            &mut calls,
                            &mut answered,
                        )
                        .await?
                    }
                    TurnEvent::NameLookup { .. } => {
                        run_turn(
                            &self.runnable,
                            envelopes,
                            checkout.resume_name_lookup(None::<MontyObject>, &mut on_print),
                            &mut calls,
                            &mut answered,
                        )
                        .await?
                    }
                    TurnEvent::FunctionCall {
                        function_name,
                        args,
                        call_id,
                        object_id: None,
                        ..
                    } if function_name == CALL_TOOL
                        && let Some(bridge) = bridge =>
                    {
                        calls.push(request(bridge, &args, call_id));
                        run_turn(
                            &self.runnable,
                            envelopes,
                            checkout.resume(ResumeValue::Future, &mut on_print),
                            &mut calls,
                            &mut answered,
                        )
                        .await?
                    }
                    TurnEvent::FunctionCall { .. } => {
                        run_turn(
                            &self.runnable,
                            envelopes,
                            checkout.resume(ResumeValue::NotFound, &mut on_print),
                            &mut calls,
                            &mut answered,
                        )
                        .await?
                    }
                    TurnEvent::ResolveFutures { .. } => {
                        if answered.is_empty() {
                            let next = calls.next().await.ok_or_else(|| {
                                PoolError::Protocol("Python awaits no pending host call".into())
                            })?;
                            answered.push(next);
                        }
                        while let Some(Some(next)) = calls.next().now_or_never() {
                            answered.push(next);
                        }
                        let mut results = Vec::with_capacity(answered.len());
                        let mut deliveries = Vec::new();
                        for (call_id, result) in answered.drain(..) {
                            let Some((delivery, value)) = answer(bridge, result) else {
                                return Ok(None);
                            };
                            deliveries.extend(delivery);
                            results.push((call_id, value));
                        }
                        // Python receives these values with this resumption;
                        // one settled earlier but never resumed is dropped
                        // undelivered with the operation.
                        for delivery in deliveries {
                            delivery.deliver();
                        }
                        run_turn(
                            &self.runnable,
                            envelopes,
                            checkout.resume_futures(results, &mut on_print),
                            &mut calls,
                            &mut answered,
                        )
                        .await?
                    }
                };
            }
        })
        .await;
        drop(on_print);
        let resettable = matches!(
            ran,
            Ok(Ok(Some(_)) | Err(PoolError::Runtime(_) | PoolError::Typing(_)))
        );
        // The worker keeps its envelope from here on, reset or discarded.
        // Record it before a reset returns the worker to the pool, so a
        // concurrent checkout's lease replaces it rather than adding to it.
        drop(lease);
        drop(session);
        if resettable && checkout.finish().await.is_ok() {
            worker_event("reset");
        }
        let value = ran
            .map_err(|_| terminal("Python exceeded its time limit".to_owned()))?
            .map_err(|error| failed(&error))?
            .ok_or_else(|| {
                stopped.unwrap_or_else(|| terminal("the tool session stopped".to_owned()))
            })?;
        Ok((value, output))
    }
}

/// Bytes a live worker holds while it lives: its baseline plus its
/// interpreter thread's stack.
fn resident_bytes() -> u64 {
    WORKER_BASELINE_BYTES + u64::try_from(WORKER_STACK_SIZE).unwrap_or(u64::MAX)
}

/// One session's envelope beyond the worker's resident bytes: the
/// allocator ceiling of `limits`' budget for the worker heap, and the same
/// again for the parent's copy of the largest frame the worker can send.
fn session_bytes(limits: &HostedLimits) -> Option<u64> {
    let budget = usize::try_from(limits.monty_memory_bytes).ok()?;
    let ceiling = u64::try_from(memory_limit_with_headroom(Some(budget), false)?).ok()?;
    ceiling.checked_mul(2)
}

/// The charges of every live worker process, by PID.
///
/// A prewarmed worker holds its resident bytes here; a worker that has
/// served a session holds that session's whole envelope, idle, checked out,
/// killed, or failed. Each charge returns only when [`reap`] finds the
/// process gone, never when the pool merely frees the worker's slot.
struct Workers {
    /// Bytes one live worker holds while it lives.
    resident: u64,
    /// Charges by worker PID.
    charges: Mutex<HashMap<u32, ByteCharge>>,
}

impl Workers {
    /// The charges, recovered after a panic elsewhere: every critical
    /// section leaves the map consistent.
    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<u32, ByteCharge>> {
        self.charges
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Bytes every live worker holds.
    fn bytes(&self) -> u64 {
        self.lock().values().map(ByteCharge::bytes).sum()
    }

    /// Record `charge` as worker `pid`'s resident bytes, or merge it into
    /// the charge the worker already holds.
    fn adopt(&self, pid: u32, charge: ByteCharge) {
        match self.lock().entry(pid) {
            std::collections::hash_map::Entry::Occupied(mut held) => held.get_mut().merge(charge),
            std::collections::hash_map::Entry::Vacant(slot) => {
                slot.insert(charge);
            }
        }
    }

    /// Bind a session `envelope` to the worker `pid` it checked out.
    ///
    /// The envelope covers the worker's resident bytes and everything a
    /// session can leave behind, so it replaces whatever the worker held
    /// from prewarming or an earlier session rather than adding to it.
    fn lease(&self, pid: Option<u32>, mut envelope: ByteCharge) -> Lease<'_> {
        if let Some(pid) = pid
            && let Some(resident) = envelope.split(self.resident)
            && self.lock().insert(pid, resident).is_none()
        {
            worker_event("spawn");
        }
        Lease {
            workers: self,
            pid,
            envelope: Some(envelope),
        }
    }

    /// Drop the charge of every worker whose process no longer exists.
    fn sweep(&self) {
        self.lock().retain(|pid, _| {
            let alive = alive(*pid);
            if !alive {
                worker_event("reap");
            }
            alive
        });
    }
}

/// Whether process `pid` still exists, reaped or not.
///
/// A killed worker exists until the runtime reaps it, so its charge holds
/// until its memory is actually returned.
fn alive(pid: u32) -> bool {
    let Some(pid) = i32::try_from(pid)
        .ok()
        .and_then(rustix::process::Pid::from_raw)
    else {
        return false;
    };
    rustix::process::test_kill_process(pid) != Err(rustix::io::Errno::SRCH)
}

/// Until the sandbox is gone, return the charges of workers whose
/// processes are gone, and reap the idle workers while an operation waits
/// for memory they hold.
///
/// ponytail: polls every [`REAP_INTERVAL`]; the pool reaps its children
/// without telling its embedder, so there is no exit event to wait on. It
/// reaps every idle worker under pressure, not just enough of them; reap one
/// at a time if warm-set churn under pressure matters.
async fn reap(workers: Weak<Workers>, pool: Weak<Pool>, admission: HostedAdmission) {
    let mut ticks = tokio::time::interval(REAP_INTERVAL);
    ticks.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        ticks.tick().await;
        let (Some(workers), Some(pool)) = (workers.upgrade(), pool.upgrade()) else {
            return;
        };
        if admission.waiting().0 > 0 && pool.idle_workers() > 0 {
            pool.close().await;
        }
        workers.sweep();
    }
}

/// One session's envelope while it holds a worker.
///
/// Dropping it, after a reset or when the operation fails or is cancelled,
/// leaves the envelope charged to the worker until its process is gone or a
/// later session takes the charge over.
struct Lease<'a> {
    /// The worker charges.
    workers: &'a Workers,
    /// The worker's PID, when the pool reported one.
    pid: Option<u32>,
    /// The session envelope, beyond the worker's resident bytes.
    envelope: Option<ByteCharge>,
}

impl Drop for Lease<'_> {
    fn drop(&mut self) {
        if let (Some(pid), Some(envelope)) = (self.pid, self.envelope.take()) {
            self.workers.adopt(pid, envelope);
        }
    }
}

/// Await one Python turn while holding a runnable permit, polling the
/// outstanding `calls` through [`drive`] the whole time; the wait for the
/// permit is timed as `envelopes`' [`Envelope::Runnable`].
async fn run_turn<T>(
    runnable: &Semaphore,
    envelopes: &Envelopes,
    turn: impl Future<Output = T>,
    calls: &mut Calls<'_>,
    answered: &mut Vec<Answered>,
) -> T {
    drive(
        async {
            let _running = envelopes.time(Envelope::Runnable, runnable.acquire()).await;
            let _turn = (
                Raised::new(VERIFICATION_PYTHON_WORKERS, "runnable", 1.0),
                Raised::new(VERIFICATION_PYTHON_WORKERS, "suspended", -1.0),
            );
            turn.await
        },
        calls,
        answered,
    )
    .await
}

/// One `state` of a gauge `family` raised by `amount` until dropped, so
/// every transition is undone exactly once, on success, failure, or
/// cancellation alike. A negative amount lowers it for the same span.
struct Raised {
    /// The gauge family.
    family: &'static str,
    /// The bounded `state` label.
    state: &'static str,
    /// What was added.
    amount: f64,
}

impl Raised {
    /// Raise `family{state}` by `amount`.
    fn new(family: &'static str, state: &'static str, amount: f64) -> Self {
        metrics::gauge!(family, "state" => state).increment(amount);
        Self {
            family,
            state,
            amount,
        }
    }
}

impl Drop for Raised {
    fn drop(&mut self) {
        metrics::gauge!(self.family, "state" => self.state).decrement(self.amount);
    }
}

/// Count one Python worker lifecycle `event`: `spawn`, `reset`, or `reap`.
fn worker_event(event: &'static str) {
    metrics::counter!(VERIFICATION_PYTHON_WORKER_EVENTS_TOTAL, "event" => event).increment(1);
}

/// Process memory a test holds so at most a fixed number of bytes stay
/// free; see [`Sandbox::occupy_for_test`]. Dropping it returns every byte at
/// once.
#[cfg(feature = "test-support")]
pub struct OccupiedForTest {
    /// The held charge, taken on drop.
    charge: Arc<std::sync::Mutex<Option<ByteCharge>>>,
    /// The task absorbing released memory into the charge.
    pin: tokio::task::JoinHandle<()>,
}

#[cfg(feature = "test-support")]
impl Drop for OccupiedForTest {
    /// Stop absorbing and return the held bytes.
    fn drop(&mut self) {
        self.pin.abort();
        self.charge
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take();
    }
}

/// The busy refusal for an admission that did not grant memory.
fn admission_refused(refusal: AdmissionRefusal) -> BlockError {
    match refusal {
        AdmissionRefusal::Memory(_) => {
            busy("process memory cannot hold the Python operation now".to_owned())
        }
        AdmissionRefusal::Expired => {
            busy("process memory did not free before the deadline".to_owned())
        }
    }
}

/// The bytes a Python file-write OS call adds, or `None` for every other
/// call. Only these four calls carry data; `open()` and the rest add none.
fn written(function_name: &str, args: &CallArgs) -> Option<usize> {
    if !matches!(
        function_name,
        "Path.write_text" | "Path.append_text" | "Path.write_bytes" | "Path.append_bytes"
    ) {
        return None;
    }
    Some(match args.arg(1).map(unstable::node) {
        Some(MontyNode::String(text)) => text.len(),
        Some(MontyNode::Bytes(bytes)) => bytes.len(),
        _ => 0,
    })
}

/// The Python name of the nested-call bridge.
const CALL_TOOL: &str = "call_tool";

/// The bridge's Python docstring.
const CALL_TOOL_DOC: &str = "await call_tool(name, arguments): run the Agent's declared Wyrd tool \
    `name` (dotted, such as 'fs.read') with the JSON object `arguments` and return its JSON \
    result. A failed call raises RuntimeError whose message is the JSON problem with its \
    stable `code`.";

/// One nested call's settled answer for its Python future.
type Answered = (u32, Result<Delivery, ToolError>);

/// The nested calls a program is awaiting.
type Calls<'a> = FuturesUnordered<BoxFuture<'a, Answered>>;

/// Await `turn` while polling the outstanding `calls`, keeping each one that
/// settles in `answered` for the next [`TurnEvent::ResolveFutures`].
///
/// Dropping the returned future drops `turn`, which condemns the worker, and
/// leaves the calls to their owner, which drops them with the operation.
async fn drive<T>(
    turn: impl Future<Output = T>,
    calls: &mut Calls<'_>,
    answered: &mut Vec<Answered>,
) -> T {
    let mut turn = std::pin::pin!(turn);
    loop {
        tokio::select! {
            biased;
            output = &mut turn => return output,
            Some(done) = calls.next(), if !calls.is_empty() => answered.push(done),
        }
    }
}

/// The host future answering one `call_tool(name, arguments)` call.
///
/// The name is the first positional or the `name` keyword, the arguments
/// the second positional or the `arguments` keyword, converted by
/// [`json_from_python`]; anything else is refused through
/// [`ToolBridge::refuse`] without dispatching.
fn request<'a>(bridge: &'a ToolBridge, args: &CallArgs, call_id: u32) -> BoxFuture<'a, Answered> {
    let named = |index: usize, name: &str| args.arg(index).or_else(|| args.kwarg(name));
    let name = named(0, "name")
        .and_then(|name| name.as_str())
        .unwrap_or_default()
        .to_owned();
    let arguments = if args.args().len() + args.kwargs().len() != 2
        || args
            .kwargs()
            .any(|(key, _)| !matches!(key.as_str(), Some("name" | "arguments")))
    {
        Err("call_tool takes exactly a tool name and one arguments object".to_owned())
    } else {
        named(1, "arguments")
            .ok_or_else(|| "call_tool needs an arguments object".to_owned())
            .and_then(|value| json_from_python(value, bridge.max_argument_bytes()))
    };
    match arguments {
        Ok(arguments) => Box::pin(async move { (call_id, bridge.call(&name, arguments).await) }),
        Err(reason) => Box::pin(std::future::ready((
            call_id,
            Err(bridge.refuse(&name, reason)),
        ))),
    }
}

/// What resumes a settled call's Python future, with the delivery to record
/// once it resumes, or `None` once the session stopped and the program must
/// end.
///
/// A value converts through [`python_from_json`]; a failure, or a value it
/// cannot convert, raises `RuntimeError` carrying the compact problem JSON,
/// and an unconvertible value is recorded as refused. A structured failure
/// keeps its owner's own code, status, and scrubbed details, even when the
/// Wyrd catalog does not list that code.
fn answer(
    bridge: Option<&ToolBridge>,
    result: Result<Delivery, ToolError>,
) -> Option<(Option<Delivery>, ResumeValue)> {
    let result = result.and_then(|delivery| match python_from_json(delivery.value()) {
        Ok(value) => Ok((delivery, value)),
        Err(reason) => {
            let error = ToolError::OutputSerialization(reason);
            delivery.refuse(&error);
            Err(error)
        }
    });
    match result {
        Ok((delivery, value)) => Some((Some(delivery), ResumeValue::Return(value))),
        Err(_) if bridge.is_some_and(ToolBridge::halted) => None,
        Err(error) => {
            let mut problem = WyrdError::from(&error).problem();
            if let ToolError::StructuredInvocation(owned) = &error {
                problem.r#type = format!("https://wyrd.dev/problems/{}", owned.code);
                problem.code.clone_from(&owned.code);
                problem.status = owned.status;
                problem.title.clone_from(&owned.title);
                problem.detail.clone_from(&owned.detail);
                problem.remediation.clone_from(&owned.remediation);
                problem.details = owned.safe_details.clone().unwrap_or(Value::Null);
            }
            let problem = serde_json::to_string(&problem).unwrap_or(problem.code);
            Some((
                None,
                ResumeValue::Error(MontyException::new(ExcType::RuntimeError, Some(problem))),
            ))
        }
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
                // The source is copied into the operation's held bytes, so
                // its read charge ends with this conversion.
                let (read, _) = self.evidence.artifact(card, path.as_deref()).await?;
                loaded = String::from_utf8(read.bytes)
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
            arguments.insert(name.clone(), Value::clone(value));
        }
        let ran = self
            .sandbox
            .run(
                self.evidence.tenant(),
                source,
                &call.entrypoint,
                &arguments,
                Vec::new(),
                deadline,
            )
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
        })
    }
}

#[async_trait]
impl CheckExecutor for CodeExecutor {
    /// Run a `code` check and return its Verdict, keeping only citations
    /// into evidence reachable through the call's bindings.
    ///
    /// # Errors
    /// Returns [`BlockError::Terminal`] for a non-code check or a result
    /// [`verdict_from`] refuses against the check's declared `verdict`
    /// format, plus [`Self::run`] and citation-read failures.
    async fn execute(&self, call: BlockCall<'_, Check>) -> Result<CheckOutput, BlockError> {
        let Check::Code(code) = call.block else {
            return Err(terminal(
                "the code executor runs only code checks".to_owned(),
            ));
        };
        let schema = code
            .response_schema()
            .map_err(|error| terminal(format!("the code check format is invalid: {error}")))?;
        let (value, provenance) = self.run(&code.call, &call.inputs, call.deadline).await?;
        let schema = SharedSchema::new(Value::Object(schema));
        let mut verdict = verdict_from(&PreparedSchemas::default(), &schema, value)?;
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

/// Deepest container nesting a value may have to cross the bridge, as
/// `serde_json`'s own parse limit.
const BRIDGE_DEPTH: usize = 128;

/// The JSON a nested call receives for the Python `value`, at most about
/// `max` compact bytes.
///
/// Only `None`, `bool`, `int` from `i64::MIN` to `u64::MAX`, finite `float`, `str`, `list` and
/// `tuple` (as arrays), and `dict` with `str` keys convert, exactly, at most
/// [`BRIDGE_DEPTH`] containers deep. The size estimate stops the walk once
/// the value passes `max`, so a value sharing one large child many times
/// never expands past the limit; the recorded call checks the exact size.
///
/// # Errors
/// Returns a bounded, value-free reason for an unsupported type, a non-string
/// key, a non-finite float, an out-of-range integer, a value nested too
/// deeply, or one larger than `max`.
fn json_from_python(value: ObjectRef<'_>, max: usize) -> Result<Value, String> {
    Estimate { left: max, max }.json(value, BRIDGE_DEPTH)
}

/// The estimated compact size left while [`json_from_python`] walks a value.
struct Estimate {
    /// Bytes still allowed.
    left: usize,
    /// The whole limit, for the refusal.
    max: usize,
}

impl Estimate {
    /// Take `bytes` from what is left.
    ///
    /// # Errors
    /// Returns the size refusal once `bytes` do not fit.
    fn charge(&mut self, bytes: usize) -> Result<(), String> {
        self.left = self.left.checked_sub(bytes).ok_or_else(|| {
            format!(
                "the arguments exceed the {}-byte tool argument limit",
                self.max
            )
        })?;
        Ok(())
    }

    /// `value` as JSON with `depth` more containers allowed.
    ///
    /// # Errors
    /// Returns [`json_from_python`]'s refusals.
    fn json(&mut self, value: ObjectRef<'_>, depth: usize) -> Result<Value, String> {
        let unsupported = || format!("a {} value cannot cross the bridge", value.type_name());
        let json = match value.type_name() {
            "NoneType" => Value::Null,
            "bool" => Value::Bool(value.as_bool().ok_or_else(unsupported)?),
            "int" => match unstable::node(value) {
                MontyNode::Int(int) => Value::from(*int),
                MontyNode::BigInt(big) => i64::try_from(big)
                    .map(Value::from)
                    .or_else(|_| u64::try_from(big).map(Value::from))
                    .map_err(|_| {
                        "an int outside the JSON integer range cannot cross the bridge".to_owned()
                    })?,
                _ => return Err(unsupported()),
            },
            "float" => value
                .as_float()
                .and_then(serde_json::Number::from_f64)
                .map(Value::Number)
                .ok_or_else(|| "a non-finite float cannot cross the bridge".to_owned())?,
            "str" => {
                let text = value.as_str().ok_or_else(unsupported)?;
                self.charge(quoted_len(text))?;
                return Ok(Value::String(text.to_owned()));
            }
            "list" | "tuple" | "dict" => {
                let depth = depth
                    .checked_sub(1)
                    .ok_or_else(|| format!("values nest deeper than {BRIDGE_DEPTH} containers"))?;
                self.charge(2)?;
                if let Some(pairs) = value.pairs() {
                    let mut object = Map::new();
                    for (index, (key, item)) in pairs.into_iter().enumerate() {
                        let key = key.as_str().ok_or_else(|| {
                            "a dict key that is not a str cannot cross the bridge".to_owned()
                        })?;
                        self.charge(quoted_len(key) + 1 + usize::from(index > 0))?;
                        object.insert(key.to_owned(), self.json(item, depth)?);
                    }
                    return Ok(Value::Object(object));
                }
                let items = value.items().ok_or_else(unsupported)?;
                let mut array = Vec::new();
                for (index, item) in items.into_iter().enumerate() {
                    self.charge(usize::from(index > 0))?;
                    array.push(self.json(item, depth)?);
                }
                return Ok(Value::Array(array));
            }
            _ => return Err(unsupported()),
        };
        self.charge(json.to_string().len())?;
        Ok(json)
    }
}

/// The compact JSON length of the string `text`, escapes and quotes
/// included, as `serde_json` writes it.
fn quoted_len(text: &str) -> usize {
    2 + text
        .bytes()
        .map(|byte| match byte {
            b'"' | b'\\' | 0x08 | b'\t' | b'\n' | 0x0c | b'\r' => 2,
            0..=0x1f => 6,
            _ => 1,
        })
        .sum::<usize>()
}

/// The Python value a nested call's JSON `value` delivers.
///
/// Arrays become lists and objects dicts; integers keep their exact value
/// from `i64::MIN` to `u64::MAX`, and values nested past [`BRIDGE_DEPTH`]
/// are refused rather than changed.
///
/// # Errors
/// Returns a bounded, value-free reason for an unrepresentable value.
fn python_from_json(value: &Value) -> Result<MontyObject, String> {
    to_python(value, BRIDGE_DEPTH)
}

/// [`python_from_json`] of `value` with `depth` more containers allowed.
///
/// # Errors
/// Returns [`python_from_json`]'s refusals.
fn to_python(value: &Value, depth: usize) -> Result<MontyObject, String> {
    let nested = || {
        depth
            .checked_sub(1)
            .ok_or_else(|| format!("the result nests deeper than {BRIDGE_DEPTH} containers"))
    };
    Ok(match value {
        Value::Null => MontyObject::none(),
        Value::Bool(value) => MontyObject::bool(*value),
        Value::Number(number) => match (number.as_i64(), number.as_u64(), number.as_f64()) {
            (Some(int), _, _) => MontyObject::int(int),
            (None, Some(unsigned), _) => python_u64(unsigned),
            (None, None, Some(float)) if number.is_f64() => MontyObject::float(float),
            _ => return Err("the result has a number outside the JSON range".to_owned()),
        },
        Value::String(text) => MontyObject::string(text.as_str()),
        Value::Array(items) => {
            let depth = nested()?;
            MontyObject::list(
                items
                    .iter()
                    .map(|item| to_python(item, depth))
                    .collect::<Result<Vec<_>, _>>()?,
            )
        }
        Value::Object(object) => {
            let depth = nested()?;
            MontyObject::dict(
                object
                    .iter()
                    .map(|(key, item)| {
                        Ok((MontyObject::string(key.as_str()), to_python(item, depth)?))
                    })
                    .collect::<Result<Vec<_>, String>>()?,
            )
        }
    })
}

/// The Python `int` of `value`, exactly; Monty stores one above `i64::MAX`
/// as a big integer.
///
/// # Panics
/// Never: the value's root is the node just pushed into its own graph.
fn python_u64(value: u64) -> MontyObject {
    let mut values = NamedValues::new();
    let root = unstable::push_named(&mut values, "", value);
    let (graph, _) = unstable::into_named_values_parts(values);
    unstable::object_from_graph(graph, root).expect("the pushed root is in its own graph")
}

#[cfg(test)]
mod tests {
    //! Bridge value conversion.

    use monty_types::MontyObject;
    use serde_json::{Value, json};

    use super::{BRIDGE_DEPTH, json_from_python, python_from_json};

    /// The Python big integer with `sign` (-1, 0, or 1) and little-endian
    /// 32-bit `digits`, as Monty serializes one.
    ///
    /// # Panics
    /// Panics when Monty no longer reads that form.
    fn big(sign: i8, digits: &[u32]) -> MontyObject {
        serde_json::from_value(json!({
            "graph": {"nodes": [{"BigInt": [sign, digits]}]},
            "root": 0
        }))
        .expect("Monty reads its big-integer form")
    }

    /// Supported JSON crosses the bridge both ways unchanged, exactly at the
    /// size limit too, including every integer from `i64::MIN` to
    /// `u64::MAX`, and every unsupported, oversized, or overly nested value
    /// is refused with a reason that names no value.
    ///
    /// # Panics
    /// Panics when a supported value changes or an unsupported one converts.
    #[test]
    fn code_bridge_json_conversion_is_lossless_and_bounded() {
        let above = i64::MAX.cast_unsigned() + 1;
        let supported = json!({
            "null": null, "yes": true, "no": false,
            "min": i64::MIN, "max": i64::MAX, "zero": 0, "float": -1.5e-300,
            "above": above, "umax": u64::MAX,
            "text": "naïve 🦀 \"quoted\"\n", "": [],
            "nested": [[1, [2.25, {"k": [null, u64::MAX]}]], {}]
        });
        let python = python_from_json(&supported).expect("supported JSON converts");
        let umax = python_from_json(&json!(u64::MAX)).expect("u64::MAX converts");
        assert_eq!(umax.as_ref().type_name(), "int");
        assert_eq!(umax.to_string(), u64::MAX.to_string(), "exact, not rounded");
        let size = supported.to_string().len();
        assert_eq!(
            json_from_python(python.as_ref(), size).expect("converts back"),
            supported
        );
        let tuple = MontyObject::tuple([MontyObject::int(1), MontyObject::string("a")]);
        assert_eq!(
            json_from_python(tuple.as_ref(), 64).expect("a tuple is an array"),
            json!([1, "a"])
        );

        let deep = |depth: usize| {
            (0..depth).fold(MontyObject::none(), |inner, _| MontyObject::list([inner]))
        };
        assert!(json_from_python(deep(BRIDGE_DEPTH).as_ref(), 1024).is_ok());
        let deep_json = (0..=BRIDGE_DEPTH).fold(Value::Null, |inner, _| json!([inner]));
        let shared = MontyObject::list((0..64).map(|_| MontyObject::string("x".repeat(1024))));
        let refused = [
            ("set", MontyObject::set([MontyObject::int(1)])),
            ("bytes", MontyObject::bytes(b"secret".to_vec())),
            ("path", MontyObject::path("/inputs/secret")),
            ("function", MontyObject::function("secret", None)),
            ("cycle", MontyObject::cycle("[...]")),
            ("nan", MontyObject::float(f64::NAN)),
            ("infinity", MontyObject::float(f64::INFINITY)),
            (
                "int key",
                MontyObject::dict([(MontyObject::int(1), MontyObject::string("secret"))]),
            ),
            ("too deep", deep(BRIDGE_DEPTH + 1)),
            ("too large", shared),
            ("u64::MAX + 1", big(1, &[0, 0, 1])),
            ("i64::MIN - 1", big(-1, &[1, 1 << 31])),
        ];
        for (case, value) in refused {
            let reason = json_from_python(value.as_ref(), 4096).expect_err(case);
            assert!(
                !reason.is_empty() && !reason.contains("secret"),
                "{case}: {reason}"
            );
        }
        assert!(
            json_from_python(MontyObject::string("x".repeat(10)).as_ref(), 11).is_err(),
            "one byte past the limit"
        );
        assert!(
            json_from_python(MontyObject::string("x".repeat(10)).as_ref(), 12).is_ok(),
            "exactly the limit"
        );
        for (case, value, exact) in [
            ("u64::MAX", big(1, &[u32::MAX, u32::MAX]), json!(u64::MAX)),
            ("i64::MIN", big(-1, &[0, 1 << 31]), json!(i64::MIN)),
        ] {
            assert_eq!(
                json_from_python(value.as_ref(), 64).expect(case),
                exact,
                "{case}"
            );
        }
        assert!(python_from_json(&deep_json).is_err(), "too deep");
    }
}
