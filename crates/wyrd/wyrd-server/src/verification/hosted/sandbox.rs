//! The shared Python sandbox behind `code` blocks and the `code.run` tool.
//!
//! [`Sandbox`] owns one process-wide Monty pool of `wyrd-server
//! subprocess` worker processes (see [`super::worker`] for
//! the in-worker isolation). Each [`Sandbox::run`] is admitted by the process
//! [`WorkloadManager`] as its tenant's verification work, checks out a
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

use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, Weak};
use std::time::Duration;

use async_trait::async_trait;
use futures_util::FutureExt as _;
use futures_util::StreamExt as _;
use futures_util::future::BoxFuture;
use futures_util::stream::FuturesUnordered;
use monty_pool::{
    MountSpec, Pool, PoolConfig, PoolError, ReplConfig, ResumeValue, TurnEvent, WorkerMemory,
    on_print_sync,
};
use monty_types::unstable::{self, MontyNode};
use monty_types::{
    CallArgs, ExcType, MontyException, MontyObject, NamedValues, ObjectRef, ResourceLimits,
};
use serde_json::{Map, Value};
use skald_spec::{PreparedSchemas, SharedSchema};
use skald_tool::ToolError;
use tokio::sync::Notify;
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;
use vala_verify::blocks::{
    ActionExecutor, ActionOutput, BlockCall, BlockError, CallInput, CallValue, CheckExecutor,
    CheckOutput, Delivery, ToolBridge, verdict_from,
};
use wyrd_runtime::resources::{
    ByteCharge, ByteLedger, CommitObservation, CpuLanes, Operation, ProcessResources, WorkContext,
    Workload, WorkloadManager,
};
use wyrd_spec::DataTenantId;
use wyrd_spec::auth::Sha256Hex;
use wyrd_spec::card::verifier::{Action, Check, CodeCall, CodeSource};
use wyrd_spec::error::WyrdError;
use wyrd_spec::verification::execution::CallProvenance;

use super::HostedLimits;
use super::evidence::TenantEvidence;
use super::workspace::{Workspace, WriteRefusal};
use crate::app::metrics::{
    VERIFICATION_HOSTED_BYTES, VERIFICATION_HOSTED_OPERATIONS, VERIFICATION_HOSTED_REFUSALS_TOTAL,
    VERIFICATION_HOSTED_WAIT_SECONDS, VERIFICATION_PYTHON_WORKER_EVENTS_TOTAL,
    VERIFICATION_PYTHON_WORKERS,
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

/// The smallest footprint a worker process has, used only to keep the pool's
/// own process count finite: the idle binary's touched pages, rounded up.
/// Workers are charged what they report, never this.
const WORKER_BASELINE_BYTES: u64 = 16 * 1024 * 1024;

/// How often worker charges follow their telemetry and dead processes are
/// reaped; matches the workers' reporting interval.
const REAP_INTERVAL: Duration = Duration::from_millis(100);

/// A worker whose latest telemetry is older than this stops new work: no new
/// session starts, on it or on a new worker, until it reports again.
const STALE_TELEMETRY: Duration = Duration::from_millis(500);

/// The process-wide Python worker pool.
pub struct Sandbox {
    /// Hardened worker processes.
    pool: Arc<Pool>,
    /// The process memory ledger operations and workers charge.
    memory: ByteLedger,
    /// Process admission, pressure and CPU lanes shared with every workload.
    workloads: WorkloadManager,
    /// The observed charge of every live worker process.
    workers: Arc<Workers>,
    /// Every operation admitted and not yet settled; closed at shutdown.
    operations: TaskTracker,
    /// Operations waiting for process admission now.
    waiting: AtomicUsize,
    /// Limits in force.
    limits: HostedLimits,
    /// One permit every operation takes and drops inside its admission wait,
    /// so a test holding it keeps operations counted as waiting.
    #[cfg(feature = "test-support")]
    admission_gate: Arc<tokio::sync::Semaphore>,
}

impl Sandbox {
    /// Start the pool over the worker binary at `worker`, prewarming
    /// `monty_min_workers`.
    ///
    /// Memory is bounded portably and charged as it is used: each worker's
    /// global allocator stops its heap at the operation budget plus Monty's
    /// headroom, and each worker reports its live and committed bytes every
    /// 100 ms, idle or not. A worker's charge follows its latest report until
    /// its process is gone; a reset worker keeps what it still reports, never
    /// a session envelope. An operation charges the inputs it retains; frames
    /// in flight show in the process's allocator commitment. When pressure
    /// turns critical, idle workers are evicted, dropping the heaps they
    /// grew, and the pool prewarms fresh ones back to `monty_min_workers`.
    ///
    /// # Errors
    /// Returns [`PoolError::Spawn`] when a prewarmed worker cannot start.
    pub async fn start(
        worker: PathBuf,
        limits: HostedLimits,
        process: &ProcessResources,
    ) -> Result<Self, PoolError> {
        let memory = process.memory().clone();
        let workloads = process.workloads().clone();
        let mut config = PoolConfig::subprocess(worker);
        config.min_processes = limits.monty_min_workers;
        // Pressure and telemetry bound how many workers start; this cap only
        // keeps the pool's own count finite.
        config.max_processes = usize::try_from(memory.limit() / WORKER_BASELINE_BYTES)
            .unwrap_or(usize::MAX)
            .max(1);
        config.max_checkouts_per_worker = None;
        let pool = Arc::new(Pool::new(config).await?);
        let workers = Arc::new(Workers {
            idle: workloads.begin(Workload::Verification, None, None),
            memory: memory.clone(),
            workloads: workloads.clone(),
            tracked: Mutex::default(),
            observed: Notify::new(),
        });
        workers.track_idle(&pool);
        let idle = Arc::downgrade(&pool);
        workloads.on_reclaim(move || {
            if let (Some(pool), Ok(runtime)) =
                (idle.upgrade(), tokio::runtime::Handle::try_current())
            {
                runtime.spawn(async move { pool.evict_idle().await });
            }
        });
        tokio::spawn(reap(Arc::downgrade(&workers), Arc::downgrade(&pool)));
        Ok(Self {
            pool,
            memory,
            workloads,
            workers,
            operations: TaskTracker::new(),
            waiting: AtomicUsize::new(0),
            limits,
            #[cfg(feature = "test-support")]
            admission_gate: Arc::new(tokio::sync::Semaphore::new(1)),
        })
    }

    /// Hold process memory so at most `leave` bytes stay free until the
    /// returned hold drops, raising the pressure the process samples and
    /// refusing retained inputs that do not fit in `leave`.
    ///
    /// The held bytes are verification work, as a memory-heavy run's would
    /// be, so contended pressure holds new Python operations in their
    /// admission wait until enough of the hold is released. Pressure is
    /// sampled once at once, so the next operation already sees the hold.
    ///
    /// Memory other owners release while the hold lives is absorbed into it
    /// soon after, so the free bytes return to `leave`.
    ///
    /// # Panics
    /// Panics when called outside a Tokio runtime.
    #[cfg(feature = "test-support")]
    #[must_use]
    pub fn occupy_for_test(&self, leave: u64) -> OccupiedForTest {
        let memory = self.memory.clone();
        let operation = self.workloads.begin(Workload::Verification, None, None);
        let mut held = memory.charge_infallible(memory.available().saturating_sub(leave));
        held.attribute(&operation);
        // Sample now so work arriving before the next periodic sample already
        // sees the hold; that sample restores the parent's real commitment.
        self.workloads.sample(0);
        let charge = Arc::new(std::sync::Mutex::new(Some(held)));
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
        OccupiedForTest {
            charge,
            pin,
            _operation: operation,
        }
    }

    /// Hold every Python operation inside its admission wait, counted by
    /// [`Self::waiting`], until the returned permit drops; process pressure
    /// and every other workload are unaffected.
    ///
    /// # Panics
    /// Panics when the gate is already held.
    #[cfg(feature = "test-support")]
    pub fn hold_admission_for_test(&self) -> tokio::sync::OwnedSemaphorePermit {
        super::hold_all(&self.admission_gate)
    }

    /// Report one worker committing more memory than the process ceiling
    /// until the returned observation drops, so every pressure sample is
    /// critical while the byte ledger keeps its room for the stores and
    /// queries a run needs before and after Python.
    #[cfg(feature = "test-support")]
    #[must_use]
    pub fn overcommit_for_test(&self) -> wyrd_runtime::resources::CommitObservation {
        let observation = self.workloads.observe_commitment();
        observation.set(u64::MAX);
        observation
    }

    /// CPU lanes free now: of the process's effective CPUs, those no turn or
    /// other workload holds.
    #[must_use]
    pub fn runnable_turns(&self) -> usize {
        self.workloads.lanes().free()
    }

    /// Python operations waiting for process admission now: paused by
    /// pressure or by stale worker telemetry.
    #[must_use]
    pub fn waiting(&self) -> usize {
        self.waiting.load(Ordering::Relaxed)
    }

    /// Bytes every live worker process is charged: its latest reported live
    /// bytes, until its process is gone.
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

    /// Workers whose latest telemetry is older than the staleness bound.
    #[cfg(feature = "test-support")]
    #[must_use]
    pub fn stale_workers(&self) -> usize {
        self.workers.stale()
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
        let (value, output) = self
            .exec(
                tenant,
                code,
                arguments,
                mounts,
                deadline,
                None,
                None,
                &Envelopes::default(),
            )
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
    /// The operation is admitted as `tenant`'s verification work, waiting while
    /// process pressure pauses it, then charges the code and globals it
    /// retains. No new session starts while any worker's telemetry is stale.
    /// The worker's observed bytes are attributed to the operation while it
    /// holds the worker. Python holds one CPU lane only while a turn runs; a
    /// program suspended on host calls or a `/work` writer holds none. Under
    /// critical pressure the operation may be cancelled as a victim. After a
    /// completed program or a Python error the session is finished, and the
    /// worker is reused only when that reset succeeds; every other end kills
    /// the worker, whose charge returns only once its process is gone.
    ///
    /// Pressure, memory refusal, stale telemetry, and a wait that ends
    /// unserved are [`BlockError::Busy`]: the process, not the code, lacked
    /// capacity, so the engine defers the whole execution instead of judging
    /// it.
    ///
    /// # Errors
    /// Returns [`BlockError::Busy`] when the sandbox is shutting down, process
    /// pressure or stale telemetry outlasts the deadline, process memory
    /// cannot hold the retained inputs, no worker starts in time, or pressure
    /// cancels the operation, [`BlockError::Retryable`] when the pool fails,
    /// and [`BlockError::Terminal`] for a Python error (including memory
    /// exhaustion), a crashed or killed worker, or a timeout while running.
    ///
    /// With `workspace`, each Python file write waits for
    /// [`Workspace::writer`] and is refused with `OSError` when
    /// [`Workspace::admit_write`] refuses it; the caller must not hold
    /// the writer itself. With `bridge`, the program can
    /// `await call_tool(name, arguments)`; its calls run while the program
    /// is suspended, and a stopped session ends it.
    ///
    /// The execution's state machine is boxed here, once, so no caller embeds
    /// it in its own future or stack frame.
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
        Box::pin(self.execute(
            tenant, code, globals, mounts, deadline, workspace, bridge, envelopes,
        ))
        .await
    }

    /// The body of [`Self::exec`], which boxes it.
    ///
    /// # Errors
    /// As [`Self::exec`].
    #[expect(
        clippy::too_many_arguments,
        reason = "one operation's tenant, program, inputs, mounts, deadline, and mediation"
    )]
    async fn execute(
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
        let victim = CancellationToken::new();
        let waited = Instant::now();
        let operation = envelopes
            .time(Envelope::Capacity, async {
                self.waiting.fetch_add(1, Ordering::Relaxed);
                let _waiting = Waiting(&self.waiting);
                let operation = tokio::time::timeout_at(deadline, async {
                    #[cfg(feature = "test-support")]
                    drop(
                        self.admission_gate
                            .acquire()
                            .await
                            .expect("the test admission gate is never closed"),
                    );
                    self.workloads
                        .admit(Workload::Verification, Some(tenant), Some(victim.clone()))
                        .await
                })
                .await
                .map_err(|_| {
                    refused(
                        "expired",
                        waited,
                        "process memory pressure did not ease before the deadline",
                    )
                })?;
                tokio::time::timeout_at(deadline, self.workers.fresh())
                    .await
                    .map_err(|_| {
                        refused("expired", waited, "a Python worker's telemetry is stale")
                    })?;
                Ok::<_, BlockError>(operation)
            })
            .await?;
        metrics::histogram!(VERIFICATION_HOSTED_WAIT_SECONDS, "outcome" => "granted")
            .record(waited.elapsed().as_secs_f64());
        let mut held = self.memory.try_charge(retained).map_err(|_| {
            refused(
                "memory",
                waited,
                "process memory cannot hold the Python operation now",
            )
        })?;
        held.attribute(&operation);
        let context = operation.context();
        let _active = (
            Raised::new(VERIFICATION_HOSTED_OPERATIONS, "active", 1.0),
            Raised::new(VERIFICATION_HOSTED_BYTES, "active", held.bytes() as f64),
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
        let lease = self
            .workers
            .lease(checkout.pid(), checkout.memory(), &operation);
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
            let program = async {
                let mut calls = Calls::new();
                let mut answered = Vec::new();
                let mut event = run_turn(
                    self.workloads.lanes(),
                    context,
                    envelopes,
                    Box::pin(checkout.feed(code, globals, mounts, true, &mut on_print)),
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
                                        .map_err(
                                            |error| PoolError::Protocol(error.to_string().into()),
                                        )?
                                    };
                                    if let Err(WriteRefusal::Scratch) = admitted {
                                        stopped = Some(busy(WriteRefusal::Scratch.to_string()));
                                        return Ok(None);
                                    }
                                    if let Err(refusal) = admitted {
                                        event = run_turn(
                                            self.workloads.lanes(),
                                            context,
                                            envelopes,
                                            Box::pin(checkout.resume(
                                                ResumeValue::Error(MontyException::new(
                                                    ExcType::OSError,
                                                    Some(refusal.to_string()),
                                                )),
                                                &mut on_print,
                                            )),
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
                                self.workloads.lanes(),
                                context,
                                envelopes,
                                Box::pin(checkout.resume_from_mounts(&mut on_print)),
                                &mut calls,
                                &mut answered,
                            )
                            .await?;
                            match mounted {
                                Some(event) => event,
                                None => {
                                    run_turn(
                                        self.workloads.lanes(),
                                        context,
                                        envelopes,
                                        Box::pin(
                                            checkout.resume(ResumeValue::NotHandled, &mut on_print),
                                        ),
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
                                self.workloads.lanes(),
                                context,
                                envelopes,
                                Box::pin(checkout.resume_name_lookup(
                                    MontyObject::function(
                                        CALL_TOOL,
                                        Some(CALL_TOOL_DOC.to_owned()),
                                    ),
                                    &mut on_print,
                                )),
                                &mut calls,
                                &mut answered,
                            )
                            .await?
                        }
                        TurnEvent::NameLookup { .. } => {
                            run_turn(
                                self.workloads.lanes(),
                                context,
                                envelopes,
                                Box::pin(
                                    checkout.resume_name_lookup(None::<MontyObject>, &mut on_print),
                                ),
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
                                self.workloads.lanes(),
                                context,
                                envelopes,
                                Box::pin(checkout.resume(ResumeValue::Future, &mut on_print)),
                                &mut calls,
                                &mut answered,
                            )
                            .await?
                        }
                        TurnEvent::FunctionCall { .. } => {
                            run_turn(
                                self.workloads.lanes(),
                                context,
                                envelopes,
                                Box::pin(checkout.resume(ResumeValue::NotFound, &mut on_print)),
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
                                self.workloads.lanes(),
                                context,
                                envelopes,
                                Box::pin(checkout.resume_futures(results, &mut on_print)),
                                &mut calls,
                                &mut answered,
                            )
                            .await?
                        }
                    };
                }
            };
            tokio::select! {
                ran = program => ran,
                () = victim.cancelled() => Ok(None),
            }
        })
        .await;
        drop(on_print);
        let resettable = matches!(
            ran,
            Ok(Ok(Some(_)) | Err(PoolError::Runtime(_) | PoolError::Typing(_)))
        );
        // The worker's charge stays with it, reset or discarded; return its
        // attribution before a reset hands the worker to another checkout.
        drop(lease);
        drop(session);
        if resettable && checkout.finish().await.is_ok() {
            worker_event("reset");
        }
        let value = ran
            .map_err(|_| terminal("Python exceeded its time limit".to_owned()))?
            .map_err(|error| failed(&error))?
            .ok_or_else(|| {
                stopped.unwrap_or_else(|| {
                    if victim.is_cancelled() {
                        busy("process memory pressure cancelled the Python operation".to_owned())
                    } else {
                        terminal("the tool session stopped".to_owned())
                    }
                })
            })?;
        Ok((value, output))
    }
}

/// One operation counted in [`Sandbox::waiting`] until dropped.
struct Waiting<'a>(&'a AtomicUsize);

impl Drop for Waiting<'_> {
    /// Stops counting this operation as waiting, on every exit path,
    /// cancellation included, so the count never strands a departed waiter.
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::Relaxed);
    }
}

/// The observed charges of every live worker process, by PID.
///
/// Each worker's charge follows its latest telemetry; idle, its bytes are
/// attributed to the sandbox's idle operation, and while an operation holds
/// it, to that operation. A worker that stops reporting keeps its last charge.
/// Each charge returns only when [`reap`] finds the process gone, never when
/// the pool merely frees the worker's slot.
struct Workers {
    /// The attribution of idle workers' bytes: verification work of no tenant.
    idle: Operation,
    /// The process memory ledger worker charges come from.
    memory: ByteLedger,
    /// Where each worker's allocator commitment is reported.
    workloads: WorkloadManager,
    /// Tracked workers by PID.
    tracked: Mutex<HashMap<u32, Tracked>>,
    /// Signalled after every observation, waking operations waiting for
    /// stale telemetry to recover.
    observed: Notify,
}

/// One live worker's telemetry and the charges that follow it.
struct Tracked {
    /// The worker's latest sample.
    telemetry: Arc<WorkerMemory>,
    /// Its reported live bytes, charged to process memory.
    charge: ByteCharge,
    /// Its reported allocator commitment.
    committed: CommitObservation,
}

impl Workers {
    /// The tracked workers, recovered after a panic elsewhere: every critical
    /// section leaves the map consistent.
    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<u32, Tracked>> {
        self.tracked
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Bytes every live worker is charged.
    fn bytes(&self) -> u64 {
        self.lock()
            .values()
            .map(|tracked| tracked.charge.bytes())
            .sum()
    }

    /// Workers whose latest telemetry is older than [`STALE_TELEMETRY`].
    fn stale(&self) -> usize {
        self.lock()
            .values()
            .filter(|tracked| tracked.telemetry.age() > STALE_TELEMETRY)
            .count()
    }

    /// Wait until no worker's telemetry is stale.
    async fn fresh(&self) {
        loop {
            let observed = self.observed.notified();
            tokio::pin!(observed);
            observed.as_mut().enable();
            if self.stale() == 0 {
                return;
            }
            observed.await;
        }
    }

    /// Track worker `pid` reporting through `telemetry`, counting a spawn
    /// the first time it is seen.
    fn track(&self, pid: u32, telemetry: Arc<WorkerMemory>) {
        if let std::collections::hash_map::Entry::Vacant(slot) = self.lock().entry(pid) {
            let mut charge = self.memory.empty_charge();
            charge.attribute(&self.idle);
            slot.insert(Tracked {
                telemetry,
                charge,
                committed: self.workloads.observe_commitment(),
            });
            worker_event("spawn");
        }
    }

    /// Track every idle worker in `pool`, such as prewarmed ones.
    fn track_idle(&self, pool: &Pool) {
        for (pid, telemetry) in pool.idle_worker_memory() {
            self.track(pid, telemetry);
        }
    }

    /// Attribute worker `pid`'s charge to `operation` until the returned
    /// lease drops, tracking the worker first when it is new.
    fn lease(
        &self,
        pid: Option<u32>,
        telemetry: Option<Arc<WorkerMemory>>,
        operation: &Operation,
    ) -> Lease<'_> {
        if let (Some(pid), Some(telemetry)) = (pid, telemetry) {
            self.track(pid, telemetry);
            if let Some(tracked) = self.lock().get_mut(&pid) {
                tracked.charge.attribute(operation);
            }
        }
        Lease { workers: self, pid }
    }

    /// Bring every charge to its worker's latest report and drop the
    /// charge of every worker whose process no longer exists.
    ///
    /// Live bytes a worker reports are observed, not requested, so they are
    /// charged even past the ledger's limit; pressure then answers them.
    fn observe(&self) {
        self.lock().retain(|pid, tracked| {
            let live = tracked.telemetry.live_bytes();
            let charged = tracked.charge.bytes();
            if live > charged {
                tracked.charge.grow_infallible(live - charged);
            } else {
                tracked.charge.shrink(charged - live);
            }
            tracked.committed.set(tracked.telemetry.committed_bytes());
            let alive = alive(*pid);
            if !alive {
                worker_event("reap");
            }
            alive
        });
        self.observed.notify_waiters();
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

/// Until the sandbox is gone, track idle workers, bring worker charges to
/// their telemetry, and return the charges of workers whose processes are
/// gone.
///
/// ponytail: polls every [`REAP_INTERVAL`]; the pool reaps its children
/// without telling its embedder, so there is no exit event to wait on.
async fn reap(workers: Weak<Workers>, pool: Weak<Pool>) {
    let mut ticks = tokio::time::interval(REAP_INTERVAL);
    ticks.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        ticks.tick().await;
        let (Some(workers), Some(pool)) = (workers.upgrade(), pool.upgrade()) else {
            return;
        };
        workers.track_idle(&pool);
        workers.observe();
    }
}

/// One operation's hold on a worker's attribution.
///
/// Dropping it, after a reset or when the operation fails or is cancelled,
/// returns the worker's charge to the idle attribution; the charge itself
/// stays with the worker until its process is gone.
struct Lease<'a> {
    /// The worker charges.
    workers: &'a Workers,
    /// The worker's PID, when the pool reported one.
    pid: Option<u32>,
}

impl Drop for Lease<'_> {
    fn drop(&mut self) {
        if let Some(pid) = self.pid
            && let Some(tracked) = self.workers.lock().get_mut(&pid)
        {
            tracked.charge.attribute(&self.workers.idle);
        }
    }
}

/// Await one Python turn while holding a CPU lane for `context`, polling the
/// outstanding `calls` through [`drive`] the whole time; the wait for the
/// lane is timed as `envelopes`' [`Envelope::Runnable`].
///
/// The worker process runs the turn, so the lane is held across the turn's
/// awaits rather than per poll; host calls polled meanwhile hold none. The
/// turn arrives boxed: a Monty checkout turn is too large to embed in every
/// caller's future.
async fn run_turn<T>(
    lanes: &CpuLanes,
    context: WorkContext,
    envelopes: &Envelopes,
    turn: Pin<Box<impl Future<Output = T>>>,
    calls: &mut Calls<'_>,
    answered: &mut Vec<Answered>,
) -> T {
    drive(
        async {
            let _running = envelopes
                .time(Envelope::Runnable, lanes.acquire(context))
                .await;
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
    /// The verification operation the held bytes are attributed to.
    _operation: Operation,
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

/// The busy refusal of an operation the process could not admit, counted
/// under `reason` (`memory` refused at once, `expired` past its deadline); an
/// expired wait since `waited` is also recorded.
fn refused(reason: &'static str, waited: Instant, message: &str) -> BlockError {
    metrics::counter!(VERIFICATION_HOSTED_REFUSALS_TOTAL, "reason" => reason).increment(1);
    if reason == "expired" {
        metrics::histogram!(VERIFICATION_HOSTED_WAIT_SECONDS, "outcome" => "expired")
            .record(waited.elapsed().as_secs_f64());
    }
    busy(message.to_owned())
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
    /// The execution's evidence, for Artifact sources and tenancy.
    evidence: Arc<TenantEvidence>,
}

impl CodeExecutor {
    /// Code execution over `sandbox` reading `evidence`.
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
    /// Run a `code` check and return its Verdict as the code returned it;
    /// the engine validates its citations.
    ///
    /// # Errors
    /// Returns [`BlockError::Terminal`] for a non-code check or a result
    /// [`verdict_from`] refuses against the check's declared `verdict`
    /// format, plus [`Self::run`] failures.
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
        let verdict = verdict_from(&PreparedSchemas::default(), &schema, value)?;
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
