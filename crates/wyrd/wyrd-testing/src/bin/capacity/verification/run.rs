//! The Verification workload under the shared experiment lifecycle: public
//! client `verify` and `submit` against one release server, and what each
//! repetition measured.
//!
//! An L1 repetition is closed loop: N clients each send the next direct
//! `verify` once the last answers, for the window. An L2 repetition is open
//! loop: direct and background arrivals follow the [`Schedule`] through
//! capacity, overload, and recovery while the benchmark snapshot is read at each phase
//! boundary and every second of recovery, then the queue drains before the
//! next repetition, so each starts from the same empty backlog.
//!
//! Judged times are the server's `verification.direct` benchmark
//! histograms: each execution's elapsed time and its residual after the
//! union of its own Verifier blocks, in the cohort of the phase that
//! admitted it, read once every request drained. Recovery's judged cohort
//! is admitted after its grace (at most half of recovery); the grace's
//! arrivals join overload's. The
//! per-second recovery curve and the client round trips and remaining
//! histograms are diagnostic.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;
use wyrd_testing::release_server::PEAK_SAMPLE_INTERVAL_MS;

use async_trait::async_trait;
use serde_json::json;
use tokio::task::JoinSet;
use tokio::time::Instant;
use wyrd_benchmark::{BenchmarkSnapshot, Distribution, IntervalValue};
use wyrd_client::WyrdClient;
use wyrd_client::state::WyrdState;
use wyrd_client::transport::trace::with_benchmark_labels;
use wyrd_client::verify::Execution;
use wyrd_spec::card::agent::BuiltinTool;
use wyrd_spec::error::WyrdError;
use wyrd_spec::ids::ExecutionId;
use wyrd_spec::verification::VerificationVerdict;
use wyrd_spec::verification::execution::{
    RequirementOutcome, ToolRecipient, VerificationDetails, VerificationOutcome, VerificationResult,
};
use wyrd_testing::capacity::{
    Expected, NANOS_PER_MS, Population, RESPONSE_BYTE_STATS, ResourceWindow, Window,
    binary_identity, driver_cpu_seconds, elapsed, process_cpu_seconds, reaped_worker_cpu,
    release_binary,
};
use wyrd_testing::experiment::{
    ExperimentError, Level, Phase, Reading, Repetition, Role, Step, Workload,
};
use wyrd_testing::gateway_capture::{FIXTURE_ROOT, PacedUpstream, Pacing, Reply};
use wyrd_testing::profile::{ProfileMode, ProfileNote};
use wyrd_testing::release_server::{LocalServer, Metrics, PeakSampler, SERVER_URL};

use crate::gateway::run::{PHASE_SECONDS, WAIT_SECONDS, WAITS};
use crate::verification::driver::{Call, ForeignDriver, Language, unsequenced};
use crate::verification::workload::{
    AGENTS, ASSERT_VERIFIER, Application, CANONICAL_VERIFIER, Conversations, JUDGE_SLOT,
    JUDGE_VERIFIER, Tenant, canonical_context, context, failing,
};
use crate::verification::{
    ADMISSION_REFUSAL, APPLICATIONS, BACKLOG_LIMIT_SECONDS, CLIENTS, OVERHEAD_LIMIT_MS,
    OVERHEAD_TAILS, PHASE_TIMINGS, RECOVERY_LIMIT, STAGES, Schedule,
};

/// The declared server process whose CPU each repetition sums.
const PROCESS: &str = "wyrd-server";

/// How often recovery reads the benchmark snapshot again.
const POLL: Duration = Duration::from_secs(1);

/// How often a drain reads the benchmark snapshot again.
const DRAIN_POLL: Duration = Duration::from_millis(100);

/// How long a drain is watched before it is recorded as unfinished.
const DRAIN_WAIT: Duration = Duration::from_secs(90);

/// Most open-loop arrivals in flight; a scheduled arrival beyond it is never
/// sent and counts as missed, which invalidates the repetition.
const MAX_IN_FLIGHT: usize = 8_192;

/// The process outboxes a verification stages on: Scribe's and the Verifier
/// submission outbox.
const OUTBOXES: [&str; 2] = ["scribe", "verifier_submissions"];

/// The server timing boundary of one direct execution, from its start until
/// its result is ready, with the union of its Verifier blocks excluded.
const DIRECT_TIMING: &str = "verification.direct";

/// The outcome of a completed execution, whatever its verdict.
const COMPLETED: &str = "completed";

/// The benchmark case of a direct `verify`.
const DIRECT_CASE: &str = "direct-verify";

/// The benchmark case of a direct `verify` of the judge Verifier, whose
/// Agent check makes one local model call.
pub(crate) const MODEL_CASE: &str = "direct-verify-model";

/// The benchmark case of a queued `submit`.
const QUEUED_CASE: &str = "queued-verify";

/// A `submit` request's arrival until it was staged.
pub(crate) const SUBMIT: &str = "wyrd_verification_submit_seconds";

/// A queued run's wait from claimable to claimed.
pub(crate) const QUEUE_WAIT: &str = "wyrd_verification_queue_wait_seconds";

/// Scribe's acknowledgement of a deduplicated retry; a benchmarked server
/// records every other acknowledgement only as [`SCRIBE_WRITE`].
pub(crate) const SCRIBE_ACK: &str = "bifrost_scribe_ack_seconds";

/// Scribe's timing of one acknowledged append, its acknowledgement time.
pub(crate) const SCRIBE_WRITE: &str = "bifrost.scribe.write";

/// The series label of a staged submission.
pub(crate) const STAGED: (&str, &str) = ("outcome", "staged");

/// The series label of work that came from `submit`, which the server
/// records as a `manual` execution origin.
pub(crate) const SUBMITTED: (&str, &str) = ("origin", "manual");

/// The verification runtime's component histograms, each with the label that
/// names its components: queued execution phases, Verifier blocks, Agent call
/// phases, queue SQL, each outbox's write, and the Scribe queue, WAL, and lane
/// times. A direct execution's phases are its `verification.direct.*` timing
/// parts.
const BREAKDOWN: [(&str, &str); 10] = [
    ("wyrd_verification_phase_duration_seconds", "phase"),
    ("wyrd_verification_block_duration_seconds", "kind"),
    ("wyrd_verification_agent_phase_duration_seconds", "phase"),
    ("wyrd_verification_store_duration_seconds", "op"),
    ("outbox_write_seconds", "outbox"),
    ("bifrost_scribe_queue_wait_seconds", ""),
    ("bifrost_scribe_wal_append_seconds", "outcome"),
    ("bifrost_scribe_wal_fsync_seconds", "outcome"),
    ("bifrost_scribe_lane_job_seconds", "lane"),
    ("wyrd_postgres_pool_acquire_seconds", "outcome"),
];

/// One benchmark root: its boundary, whether it excludes waits, and its
/// components.
type Root = (&'static str, bool, &'static [&'static str]);

/// The driver root of a direct `verify`'s one logical execution request,
/// whose response body sizes each repetition reports.
pub const HTTP_ROOT: &str = "client.http";

/// The driver roots each direct `verify` records once, with their
/// components: the SDK flow and its preparation, its one logical execution
/// request, and that request's one attempt. The benchmark's inputs hold no
/// file, so no evidence plan or upload request precedes the execution.
pub const DRIVER_ROOTS: [(&str, bool, &[&str]); 3] = [
    ("client.verify", false, &["prepare"]),
    (
        HTTP_ROOT,
        false,
        &["serialize", "auth", "send", "body", "decode"],
    ),
    ("client.http.attempt", false, &[]),
];

/// The native binding's own root of each Python or TypeScript `verify`:
/// argument conversion through the returned language object, around the
/// shared client's `client.verify`.
pub const SDK_VERIFY: &str = "sdk.verify";

/// The roots each Python `verify` records once in its driver: the shared
/// client's, inside the binding's `sdk.verify` with its input conversion,
/// shared core, and output conversion, and that core's `sdk.verify.native`
/// split into the caller's span read, the run with the GIL released, and
/// the wait to hold the GIL again.
pub const PYTHON_ROOTS: [Root; 5] = [
    DRIVER_ROOTS[0],
    DRIVER_ROOTS[1],
    DRIVER_ROOTS[2],
    (SDK_VERIFY, false, &["input", "core", "output"]),
    ("sdk.verify.native", false, &["trace", "run", "attach"]),
];

/// The roots each TypeScript `verify` records once in its driver: the shared
/// client's, inside the addon's `sdk.verify`, which also times the wait from
/// the JavaScript call until its future first runs on the shared runtime.
pub const TYPESCRIPT_ROOTS: [Root; 4] = [
    DRIVER_ROOTS[0],
    DRIVER_ROOTS[1],
    DRIVER_ROOTS[2],
    (SDK_VERIFY, false, &["dispatch", "input", "core", "output"]),
];

/// The client roots `language`'s calls record once each, in this process
/// for Rust and in the language's driver otherwise.
pub const fn client_roots(language: Language) -> &'static [Root] {
    match language {
        Language::Rust => &DRIVER_ROOTS,
        Language::Python => &PYTHON_ROOTS,
        Language::TypeScript => &TYPESCRIPT_ROOTS,
    }
}

/// The driver root of the local provider's one answer per successful model
/// verification, with its components; the provider runs in this process.
pub const FIXTURE_ROOTS: [(&str, bool, &[&str]); 1] = [(FIXTURE_ROOT, false, &["parse", "answer"])];

/// Why an assertion step has no local provider observations.
const NO_PROVIDER: &str = "an assertion check calls no provider";

/// Why a step other than the canonical one has no per-request ledger.
const NOT_CANONICAL: &str = "only the canonical workload's Agents run code";

/// The refusal a language driver's call counts as when it reported neither
/// a stable error code nor an execution and outcome.
const INCOMPLETE_REPORT: &str = "incomplete-driver-report";

/// Acknowledged canonical executions per repetition whose details are read
/// back after the window to prove their tool records.
const DETAILS_SAMPLE: usize = 8;

/// The refusal of a details read before the execution's records are visible.
const NOT_VISIBLE: &str = "WYRD_VERIFICATION_404_EXECUTION_NOT_FOUND";

/// The server's nested Agent envelopes of a direct execution: each
/// diagnostic observation, its `verification.direct` part, and whether the
/// judge Verifier's one tool-less Agent reaches it. They nest in the
/// engine, overlap across concurrent Agents and each other, and are never
/// summed or subtracted.
pub(crate) const ENVELOPES: [(&str, &str, bool); 8] = [
    ("server.workspace", "verification.direct.workspace", false),
    ("server.tool", "verification.direct.tool", false),
    ("server.sandbox", "verification.direct.sandbox", false),
    ("server.capacity", "verification.direct.capacity", false),
    ("server.runnable", "verification.direct.runnable", false),
    ("server.model", "verification.direct.model", true),
    ("server.checkout", "verification.direct.checkout", false),
    ("server.execute", "verification.direct.execute", false),
];

/// Why a step's Verifier reaches no such envelope.
const NO_ENVELOPE: &str = "this step's Verifier runs no Agent work of this kind";

/// The Gateway phases an in-process Agent check's model call records once
/// each: the native caller authenticates and parses nothing, and a buffered
/// call relays no stream.
pub(crate) const GATEWAY_PHASES: [&str; 8] = [
    "snapshot",
    "authorize",
    "route",
    "admit",
    "credential",
    "translate",
    "account",
    "capture",
];

/// The prefix of a direct execution's phase timing parts.
const DIRECT_PARTS: &str = "verification.direct.";

/// Component observations kept per L1 repetition, largest total first.
const COMPONENT_ROWS: usize = 15;

/// Which public call an arrival makes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Op {
    /// `observe.verify` of the assertion Verifier: direct execution, judged
    /// before it answers.
    Verify,
    /// `observe.verify` of the judge Verifier: a direct execution whose one
    /// Agent check calls the local model through the Gateway.
    Judge,
    /// `observe.verify` of the canonical Verifier: a direct execution of one
    /// Assert and two Agent checks, each Agent running one `code.run`
    /// between two model calls through the Gateway.
    Canonical,
    /// `observe.submit`: staged for one queued execution.
    Submit,
}

impl Op {
    /// The benchmark case the call declares.
    const fn case(self) -> &'static str {
        match self {
            Self::Verify | Self::Canonical => DIRECT_CASE,
            Self::Judge => MODEL_CASE,
            Self::Submit => QUEUED_CASE,
        }
    }

    /// The direct call `step` declares: [`Self::Judge`] for a `model`
    /// check, [`Self::Canonical`] for a `canonical` one, else
    /// [`Self::Verify`].
    fn direct(step: &Step) -> Self {
        match step.text("check") {
            Some("model") => Self::Judge,
            Some("canonical") => Self::Canonical,
            _ => Self::Verify,
        }
    }

    /// Governed provider exchanges one acknowledged call makes.
    const fn exchanges(self) -> u64 {
        match self {
            Self::Verify | Self::Submit => 0,
            Self::Judge => 1,
            Self::Canonical => 4,
        }
    }

    /// The Verifier arrival `sequence` names and its inputs, built before
    /// the call.
    fn arrival(self, sequence: u64) -> (&'static str, serde_json::Value) {
        match self {
            Self::Verify | Self::Submit => (ASSERT_VERIFIER, context(sequence)),
            Self::Judge => (JUDGE_VERIFIER, context(sequence)),
            Self::Canonical => (CANONICAL_VERIFIER, canonical_context(sequence)),
        }
    }

    /// Whether `outcome` is what arrival `sequence` expects: the assertion's
    /// verdict, a judge's pass citing [`JUDGE_SLOT`], or
    /// [`canonical_expected`].
    fn expected(self, sequence: u64, outcome: &VerificationOutcome) -> bool {
        let verdict = if failing(sequence) {
            VerificationVerdict::Failed
        } else {
            VerificationVerdict::Passed
        };
        match self {
            Self::Verify | Self::Submit => outcome.verdict == verdict,
            Self::Judge => {
                let cited = outcome.requirements.iter().any(|requirement| {
                    requirement
                        .citations
                        .iter()
                        .any(|citation| citation.slot.as_str() == JUDGE_SLOT)
                });
                outcome.verdict == VerificationVerdict::Passed && cited
            }
            Self::Canonical => canonical_expected(sequence, outcome),
        }
    }
}

/// Whether canonical arrival `sequence` judged exactly its three
/// requirements: the Assert by the arrival's `d`, and each Agent passed,
/// citing slot `context`, with its own name and the arrival's sequence as
/// data; the verdict follows the Assert.
fn canonical_expected(sequence: u64, outcome: &VerificationOutcome) -> bool {
    let (verdict, asserted) = if failing(sequence) {
        (VerificationVerdict::Failed, RequirementOutcome::Failed)
    } else {
        (VerificationVerdict::Passed, RequirementOutcome::Passed)
    };
    let requirements = &outcome.requirements;
    let requirement = |id: &str| {
        requirements
            .iter()
            .find(|requirement| requirement.id.as_str() == id)
    };
    let agents = AGENTS.iter().all(|agent| {
        let data = json!({ "agent": agent, "sequence": sequence.to_string() });
        requirement(agent).is_some_and(|requirement| {
            requirement.outcome == RequirementOutcome::Passed
                && requirement
                    .citations
                    .iter()
                    .any(|citation| citation.slot.as_str() == "context")
                && requirement.data.as_ref() == data.as_object()
        })
    });
    requirements.len() == 3
        && outcome.verdict == verdict
        && requirement("asserted").is_some_and(|requirement| requirement.outcome == asserted)
        && agents
}

/// One acknowledged call's two client samples, microseconds from
/// immediately before the public call: until it returned its decoded
/// result, and until that result's compact fields were materialized.
/// Both hold all elapsed work in between; neither is ever reduced.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Sample {
    /// Until the call returned.
    returned_us: u64,
    /// Until its result was ready.
    ready_us: u64,
}

impl Sample {
    /// Awaits `call`, then `ready` over a successful result, timing both
    /// from immediately before `call` is first polled: work built before
    /// the call and judgment after `ready` are outside both samples. A
    /// refusal converts nothing, so its result-ready sample equals its
    /// call-return sample.
    async fn take<T, E>(
        call: impl Future<Output = Result<T, E>>,
        ready: impl AsyncFnOnce(&T),
    ) -> (Result<T, E>, Self) {
        let sent = Instant::now();
        let returned = call.await;
        let returned_us = micros(sent);
        if let Ok(result) = &returned {
            ready(result).await;
        }
        (
            returned,
            Self {
                returned_us,
                ready_us: micros(sent),
            },
        )
    }
}

/// Reads every compact field of `result` a caller judges, the verdict and
/// each requirement's outcome, citations, and data, through the decoded
/// view the Rust client returned, without copying or re-encoding it.
fn materialize(result: &VerificationResult) {
    std::hint::black_box(&result.outcome.verdict);
    for requirement in &result.outcome.requirements {
        std::hint::black_box((
            &requirement.outcome,
            &requirement.citations,
            &requirement.data,
        ));
    }
}

/// What a set of calls answered.
#[derive(Debug, Default, Clone)]
struct Measured {
    /// Call-return latency of every acknowledged call, microseconds.
    latencies_us: Vec<u64>,
    /// Result-ready latency of every acknowledged direct call, in the order
    /// of `latencies_us`; empty where the step times only call return.
    ready_us: Vec<u64>,
    /// Refused calls by stable code.
    errors: BTreeMap<String, u64>,
    /// Direct judgments whose verdict was not the expected one.
    wrong: u64,
    /// Acknowledged calls that answered at or after the window end, outside
    /// the measured CPU; still in `latencies_us`.
    late: u64,
    /// The sequence of every acknowledged canonical call, for the
    /// provider ledger's reconciliation.
    sequences: Vec<u64>,
    /// The first [`DETAILS_SAMPLE`] acknowledged canonical executions.
    sampled: Vec<ExecutionId>,
    /// In a paced window, each sent call's due slot after the window
    /// opened and its start behind that slot, microseconds.
    lateness_us: Vec<(u64, u64)>,
    /// In a paced window, how long after its due slot each caller woke,
    /// before preparing its call, microseconds, where the driver reports it.
    woke_us: Vec<u64>,
    /// The garbage collections the driver's runtime reported: start after
    /// the window opened and duration, microseconds; `None` when the driver
    /// reports none.
    collections_us: Option<Vec<(u64, u64)>>,
}

impl Measured {
    /// Records one call acknowledged after `latency_us` with a right or wrong
    /// verdict, `late` when it answered after the window end, or refused with
    /// its stable code.
    fn record(&mut self, latency_us: u64, outcome: Result<bool, &str>, late: bool) {
        match outcome {
            Ok(right) => {
                self.latencies_us.push(latency_us);
                self.wrong += u64::from(!right);
                self.late += u64::from(late);
            }
            Err(code) => *self.errors.entry(code.to_owned()).or_default() += 1,
        }
    }

    /// Records direct arrival `sequence` of `op` answered after `sample`
    /// with `returned`, judged against [`Op::expected`] after the timer
    /// stopped; an acknowledgement keeps both samples, and a canonical one
    /// also keeps its sequence and, while the sample has room, its
    /// execution.
    fn judged(
        &mut self,
        op: Op,
        sequence: u64,
        sample: Sample,
        returned: Result<VerificationResult, WyrdError>,
        late: bool,
    ) {
        match returned {
            Ok(result) => self.acknowledged(
                op,
                sequence,
                sample,
                (result.execution_id, &result.outcome),
                late,
            ),
            Err(error) => self.record(sample.returned_us, Err(error.code()), late),
        }
    }

    /// Records direct arrival `sequence` of `op` acknowledged after
    /// `sample` with `execution`'s outcome, as [`Self::judged`] does.
    fn acknowledged(
        &mut self,
        op: Op,
        sequence: u64,
        sample: Sample,
        (execution, outcome): (ExecutionId, &VerificationOutcome),
        late: bool,
    ) {
        self.ready_us.push(sample.ready_us);
        if op == Op::Canonical {
            self.sequences.push(sequence);
            if self.sampled.len() < DETAILS_SAMPLE {
                self.sampled.push(execution);
            }
        }
        self.record(sample.returned_us, Ok(op.expected(sequence, outcome)), late);
    }

    /// Records one call a language driver reported for `op`, as
    /// [`Self::judged`] records a Rust one: its language-clock samples in
    /// whole microseconds, rounded down, and its refusal code or judged
    /// outcome. A report with neither counts as an [`INCOMPLETE_REPORT`]
    /// refusal, never as an acknowledgement.
    fn reported(&mut self, op: Op, call: Call) {
        if let (Some(due), Some(lateness)) = (call.due_ns, call.lateness_ns) {
            self.lateness_us.push((due / 1_000, lateness / 1_000));
        }
        if let Some(woke) = call.woke_ns {
            self.woke_us.push(woke / 1_000);
        }
        let sample = Sample {
            returned_us: call.returned_ns / 1_000,
            ready_us: call.ready_ns / 1_000,
        };
        match (call.error, call.execution_id, call.outcome) {
            (Some(code), _, _) => self.record(sample.returned_us, Err(&code), call.late),
            (None, Some(execution), Some(outcome)) => {
                self.acknowledged(op, call.sequence, sample, (execution, &outcome), call.late);
            }
            _ => self.record(sample.returned_us, Err(INCOMPLETE_REPORT), call.late),
        }
    }

    /// Folds another client's results into this one.
    fn merge(&mut self, other: Self) {
        self.latencies_us.extend(other.latencies_us);
        self.ready_us.extend(other.ready_us);
        self.sequences.extend(other.sequences);
        self.lateness_us.extend(other.lateness_us);
        self.woke_us.extend(other.woke_us);
        let room = DETAILS_SAMPLE.saturating_sub(self.sampled.len());
        self.sampled.extend(other.sampled.into_iter().take(room));
        self.wrong += other.wrong;
        self.late += other.late;
        for (code, count) in other.errors {
            *self.errors.entry(code).or_default() += count;
        }
    }

    /// Calls that answered, acknowledged or refused.
    fn answered(&self) -> u64 {
        self.latencies_us.len() as u64 + self.errors.values().sum::<u64>()
    }

    /// Refusals other than `allowed`.
    fn unexpected(&self, allowed: Option<&str>) -> u64 {
        self.errors
            .iter()
            .filter(|(code, _)| Some(code.as_str()) != allowed)
            .map(|(_, count)| count)
            .sum()
    }

    /// Client call-return percentile `q`, milliseconds.
    fn percentile(&self, q: f64) -> Reading {
        Reading::percentile(&self.latencies_us, q, 1e3)
    }

    /// Why the direct client samples cannot be timing evidence: an
    /// acknowledged call without its result-ready sample, a sample the
    /// microsecond clock could not represent, or a result ready before its
    /// call returned. `None` when every acknowledged call has both samples.
    fn integrity(&self) -> Option<String> {
        if self.ready_us.len() != self.latencies_us.len() {
            return Some(format!(
                "{} acknowledged calls but {} result-ready samples: a total sample is missing",
                self.latencies_us.len(),
                self.ready_us.len()
            ));
        }
        let pairs = || self.latencies_us.iter().zip(&self.ready_us);
        if let Some(index) =
            pairs().position(|(returned, ready)| *returned == u64::MAX || *ready == u64::MAX)
        {
            return Some(format!(
                "client sample {index} is unrepresentable in microseconds"
            ));
        }
        pairs()
            .position(|(returned, ready)| ready < returned)
            .map(|index| format!("client sample {index} was ready before it returned"))
    }
}

/// The Verification workload: one release server, one provisioned tenant,
/// and the level's arrivals.
pub struct Bench {
    /// The level, which picks closed- or open-loop work.
    level: Level,
    /// Where the tenant's bundles and the server log land.
    logs: PathBuf,
    /// The running server, from `prepare` until `finish`.
    server: Option<LocalServer>,
    /// One application per client or rotation slot.
    states: Vec<WyrdState>,
    /// Whether the run has model steps, so it binds the judge Verifier.
    model: bool,
    /// The local provider L1's Gateway deployment calls, while prepared.
    provider: Option<Provider>,
    /// The provider's conversations and their per-arrival ledger.
    conversations: Arc<Conversations>,
    /// Each state's bundle and key, for the language drivers.
    applications: Vec<Application>,
    /// The Python and TypeScript drivers started so far, each before its
    /// language's first window and kept until `finish`.
    drivers: BTreeMap<Language, ForeignDriver>,
    /// The run's `--profile` mode, which starts every language driver in
    /// `prepare`, profiled, so each step's captures include it.
    profile: Option<ProfileMode>,
}

/// Worker threads of the local provider's own runtime: about three times
/// the ~1.3 cores the Rust c64 driver profile spends answering, so the
/// provider does not queue on its own scheduler.
const PROVIDER_WORKERS: usize = 4;

/// The local provider and the runtime only it runs on.
///
/// The provider answers on its own worker threads, not the controller's, so
/// Rust client tasks and provider answers never wait on one scheduler and
/// every language population meets the same provider. It stays in the
/// controller process, so its CPU is still driver CPU.
struct Provider {
    /// The provider, serving on `runtime`.
    upstream: PacedUpstream,
    /// The provider's runtime, until dropped.
    runtime: Option<tokio::runtime::Runtime>,
}

impl Provider {
    /// Starts the zero-delay provider answering through `reply` on a new
    /// runtime of [`PROVIDER_WORKERS`] threads.
    ///
    /// # Errors
    ///
    /// Returns the runtime build, bind, or startup failure.
    async fn start(reply: Reply) -> std::io::Result<Self> {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(PROVIDER_WORKERS)
            .thread_name("bench-provider")
            .enable_all()
            .build()?;
        let instant = Pacing {
            buffered_delay: Duration::ZERO,
            buffered_bytes: 0,
            stream_delay: Duration::ZERO,
            chunks: 0,
            chunk_interval: Duration::ZERO,
        };
        let upstream = runtime
            .spawn(PacedUpstream::replying(instant, 0, reply))
            .await
            .map_err(std::io::Error::other)??;
        Ok(Self {
            upstream,
            runtime: Some(runtime),
        })
    }
}

impl Drop for Provider {
    /// Shuts the runtime down without blocking: the controller drops the
    /// provider from async code, where a blocking runtime shutdown panics.
    fn drop(&mut self) {
        if let Some(runtime) = self.runtime.take() {
            runtime.shutdown_background();
        }
    }
}

impl Bench {
    /// A workload at `level` keeping its files under `logs`, binding the
    /// judge Verifier when `model`, its language drivers profiled under
    /// `profile`; nothing starts until [`Workload::prepare`].
    pub fn new(level: Level, logs: PathBuf, model: bool, profile: Option<ProfileMode>) -> Self {
        Self {
            level,
            logs,
            server: None,
            states: Vec::new(),
            model,
            provider: None,
            conversations: Arc::default(),
            applications: Vec::new(),
            drivers: BTreeMap::new(),
            profile,
        }
    }

    /// The running server.
    ///
    /// # Errors
    ///
    /// Returns a workload error before `prepare` or after `finish`.
    fn server(&self) -> Result<&LocalServer, ExperimentError> {
        self.server
            .as_ref()
            .ok_or_else(|| ExperimentError::workload("the server is not running"))
    }

    /// Starts the release server through the guide and provisions the
    /// tenant's applications for the level; L1 first starts the instant
    /// local provider answering every conversation through
    /// [`Conversations::reply`].
    ///
    /// # Errors
    ///
    /// Returns a workload error for a provider, server, or provisioning
    /// failure.
    async fn start(&mut self) -> Result<(), ExperimentError> {
        std::fs::create_dir_all(&self.logs).map_err(ExperimentError::workload)?;
        if self.level == Level::One {
            let conversations = Arc::clone(&self.conversations);
            self.provider = Some(
                Provider::start(Arc::new(move |body: &[u8]| conversations.reply(body)))
                    .await
                    .map_err(ExperimentError::workload)?,
            );
        }
        let binary = release_binary().map_err(ExperimentError::workload)?;
        let server = LocalServer::start(&binary, &["bench"], &[])
            .await
            .map_err(ExperimentError::workload)?;
        let [setup] = server.tenants() else {
            return Err(ExperimentError::workload("setup provisioned one tenant"));
        };
        let applications = match self.level {
            Level::One => CLIENTS.iter().copied().max().unwrap_or(0),
            Level::Two => APPLICATIONS,
        };
        let applications = usize::try_from(applications).map_err(ExperimentError::workload)?;
        let tenant = Box::pin(Tenant::provision(
            setup,
            &self.logs.join("tenants"),
            applications,
            self.provider
                .as_ref()
                .map(|provider| provider.upstream.uri())
                .as_deref(),
            self.model,
        ))
        .await
        .map_err(ExperimentError::workload)?;
        self.states = tenant.states;
        self.applications = tenant.applications;
        self.server = Some(server);
        Ok(())
    }

    /// Starts `language`'s driver before its first window, loading every
    /// provisioned application through that language's public SDK; Rust
    /// clients, and a language already started, need nothing.
    ///
    /// # Errors
    ///
    /// Returns a workload error for a language driver asked for a check
    /// other than the canonical one, or a driver that does not start.
    async fn foreign(&mut self, language: Language, op: Op) -> Result<(), ExperimentError> {
        if language == Language::Rust || self.drivers.contains_key(&language) {
            return Ok(());
        }
        if op != Op::Canonical {
            return Err(ExperimentError::workload(format!(
                "the {} driver runs only the canonical check",
                language.name()
            )));
        }
        let driver = ForeignDriver::start(
            language,
            SERVER_URL,
            CANONICAL_VERIFIER,
            [canonical_context(0), canonical_context(1)],
            &self.applications,
            self.profile,
            &self.logs,
        )
        .await?;
        self.drivers.insert(language, driver);
        Ok(())
    }

    /// One L1 repetition: `step`'s clients each run closed-loop direct
    /// `verify` of the Verifier its check names ([`Op::direct`]) for
    /// `window`, labelled with `phase`, judged by each acknowledged call's
    /// exact server timing. A step whose Agents call the provider also owes
    /// [`Op::exchanges`] local provider answers per acknowledged call when
    /// none was refused, and a canonical step reconciles the provider's
    /// per-arrival ledger and a sample of execution details as
    /// `unreconciled`. Work, seconds, and CPU stop at the window's end;
    /// calls then in flight drain into the cohort and count late.
    ///
    /// # Errors
    ///
    /// Returns a workload error for an undeclared client count or a failed
    /// scrape.
    async fn closed(
        &mut self,
        step: &Step,
        phase: Phase,
        window: Duration,
    ) -> Result<Repetition, ExperimentError> {
        let language = Language::of(step)?;
        let op = Op::direct(step);
        self.foreign(language, op).await?;
        let server = self
            .server
            .as_ref()
            .ok_or_else(|| ExperimentError::workload("the server is not running"))?;
        let clients = step
            .number("clients")
            .and_then(|clients| usize::try_from(clients).ok())
            .filter(|clients| (1..=self.states.len()).contains(clients))
            .ok_or_else(|| {
                ExperimentError::workload(format!(
                    "step `{}` declares no client count within the {} provisioned applications",
                    step.id,
                    self.states.len()
                ))
            })?;
        let before = timings(server).await?;
        let driver_before = wyrd_benchmark::snapshot();
        let first = scrape(server).await?;
        drop(self.conversations.take());
        let (opened, opening) = Window::open(server)
            .await
            .map_err(ExperimentError::workload)?;
        let mut foreign = self.drivers.get_mut(&language);
        let pid = foreign
            .as_ref()
            .map_or_else(std::process::id, |driver| driver.pid);
        let client_opened = process_cpu_seconds(pid);
        let client_memory = PeakSampler::start(pid);
        let opened_at = Instant::now();
        let stop = opened_at + window;
        let states = &self.states[..clients];
        let rate = step.number("rate");
        let pace = rate.map(|rate| Pace {
            start: opened_at,
            clients: clients as u64,
            rate,
        });
        let work = async {
            let calls = async {
                match foreign.as_mut() {
                    None => Ok((
                        drive(states, op, stop, phase.label(), pace).await,
                        None,
                        None,
                    )),
                    Some(driver) => {
                        let seconds = stop.saturating_duration_since(Instant::now());
                        let window = driver.run(clients, seconds.as_secs_f64(), rate).await?;
                        let unsequenced = unsequenced(&window.calls);
                        let mut measured = Measured {
                            collections_us: window.collections.map(|collections| {
                                collections
                                    .iter()
                                    .map(|gc| (gc.start_ns / 1_000, gc.duration_ns / 1_000))
                                    .collect()
                            }),
                            ..Measured::default()
                        };
                        for call in window.calls {
                            measured.reported(op, call);
                        }
                        let timings = window
                            .timings
                            .map(|timings| (timings.before, timings.after));
                        Ok((measured, unsequenced, timings))
                    }
                }
            };
            let at_stop = async {
                tokio::time::sleep_until(stop).await;
                process_cpu_seconds(pid)
            };
            tokio::join!(calls, at_stop)
        };
        let ((calls, client_stopped), usage, at_stop) = opened
            .measure(server, stop.into_std(), work)
            .await
            .map_err(ExperimentError::workload)?;
        let (measured, unsequenced, timings) = calls?;
        let ledger = self.conversations.take();
        let last = scrape(server).await?;
        let client_cpu = elapsed(client_opened, client_stopped);
        // A language driver generates load beside this controller, so the
        // driver budget holds both processes' CPU.
        let driver_cpu = match foreign {
            None => usage.driver_cpu_seconds,
            Some(_) => usage
                .driver_cpu_seconds
                .zip(client_cpu)
                .map(|(controller, child)| controller + child),
        };
        let mut repetition = Repetition {
            seconds: usage.seconds,
            processes: vec![PROCESS.to_owned()],
            server_cpu_seconds: usage.server_cpu_seconds,
            peak_memory_bytes: usage.peak_memory_bytes,
            opening_memory_bytes: usage.opening_memory_bytes,
            memory_interval_ms: PEAK_SAMPLE_INTERVAL_MS,
            driver_cpu_seconds: driver_cpu,
            ..Repetition::default()
        };
        set_workers(
            &mut repetition,
            usage.worker_cpu_seconds,
            usage.worker_peak_memory_bytes,
        );
        set_worker_churn(
            &mut repetition,
            &worker_events(&opening),
            &worker_events(&at_stop),
        );
        set_client_process(
            &mut repetition,
            foreign.as_deref(),
            client_cpu,
            client_memory,
        );
        if let Some(reason) = unsequenced {
            repetition.invalid.push(format!("client calls: {reason}"));
        }
        // A language driver's public SDK declares no benchmark labels, so
        // the server and the local provider record its calls unlabelled.
        let cohort = if foreign.is_some() {
            (wyrd_benchmark::UNLABELLED, wyrd_benchmark::UNLABELLED)
        } else {
            (op.case(), phase.label())
        };
        let acknowledged = measured.latencies_us.len() as u64;
        let population = Population::collect(
            server,
            &before,
            &[Expected {
                boundary: DIRECT_TIMING,
                outcome: COMPLETED,
                case: cohort.0,
                phase: cohort.1,
                count: acknowledged,
            }],
        )
        .await;
        population.validate(&mut repetition, "measured");
        let clean = measured.errors.is_empty();
        let answers = acknowledged * op.exchanges();
        let mut owed: Vec<(&[Root], u64)> = Vec::new();
        if foreign.is_none() {
            owed.push((&DRIVER_ROOTS, acknowledged));
        }
        if answers > 0 && clean {
            owed.push((&FIXTURE_ROOTS, answers));
        }
        let expected: Vec<Expected<'_>> = owed
            .iter()
            .flat_map(|(roots, count)| {
                roots.iter().map(|(boundary, _, _)| Expected {
                    boundary,
                    outcome: wyrd_benchmark::SUCCESS,
                    case: cohort.0,
                    phase: cohort.1,
                    count: *count,
                })
            })
            .collect();
        let driver = Population::collect_driver(&driver_before, &expected).await;
        driver.validate(&mut repetition, "measured");
        for (roots, count) in &owed {
            driver.diagnose_roots(&mut repetition, cohort, roots, clean.then_some(*count));
        }
        if foreign.is_some() {
            set_language_client(&mut repetition, language, timings, acknowledged, clean);
        } else {
            unreached_clients(&mut repetition, language);
            driver.response_bytes(
                &mut repetition,
                HTTP_ROOT,
                cohort,
                measured.errors.is_empty().then_some(acknowledged),
            );
        }
        if op == Op::Verify {
            for (root, _, components) in FIXTURE_ROOTS {
                let root = format!("driver.{root}");
                repetition.inapplicable(&root, NO_PROVIDER);
                for component in components {
                    repetition.inapplicable(&format!("{root}.{component}"), NO_PROVIDER);
                }
            }
        }
        set_gateway(&mut repetition, &population, op, clean);
        if op == Op::Canonical {
            let (mut unreconciled, mut problems) = ledger.unreconciled(&measured.sequences, clean);
            let client = self.states[0].client().map_err(ExperimentError::workload)?;
            let (unrecorded, missing) = recorded(client, &measured.sampled).await;
            unreconciled += unrecorded;
            problems.extend(missing);
            repetition.set("unreconciled", Reading::count(unreconciled));
            if !problems.is_empty() {
                repetition
                    .observations
                    .push(format!("unreconciled canonical work: {problems:?}"));
            }
        } else {
            repetition.set(
                "unreconciled",
                Reading::missing(format!("inapplicable: {NOT_CANONICAL}")),
            );
        }
        population.percentiles(
            &mut repetition,
            DIRECT_TIMING,
            COMPLETED,
            false,
            &[("p50_ms", 0.5), ("p95_ms", 0.95), ("p99_ms", 0.99)],
        );
        population.percentiles(
            &mut repetition,
            DIRECT_TIMING,
            COMPLETED,
            true,
            &[("overhead_p99_ms", 0.99)],
        );
        let verifier = population
            .excluded(DIRECT_TIMING, COMPLETED)
            .ok_or_else(|| "no completed direct execution has a Verifier component".to_owned());
        repetition.set(
            "overhead_mean_ms",
            mean_ms(population.residuals(DIRECT_TIMING, COMPLETED)),
        );
        repetition.set(
            "verifier_p99_ms",
            verifier.as_ref().map_or_else(Reading::missing, |verifier| {
                Reading::quantile(verifier, 0.99, NANOS_PER_MS)
            }),
        );
        repetition.set("verifier_mean_ms", mean_ms(verifier));
        set_tails(&mut repetition, &population);
        set_phases(&mut repetition, &population);
        repetition.set(
            "server_failures",
            Reading::count(population.others(DIRECT_TIMING, COMPLETED)),
        );
        set_client(&mut repetition, &measured);
        set_ready(&mut repetition, &measured);
        if let Some(pace) = pace {
            let lateness: Vec<u64> = measured.lateness_us.iter().map(|(_, late)| *late).collect();
            let scheduled = pace.scheduled(window);
            let sent = measured.answered();
            repetition.arrivals = Some(wyrd_testing::experiment::Arrivals {
                scheduled,
                sent,
                completed: acknowledged,
                missed: scheduled.saturating_sub(sent),
                lateness_p99_us: Reading::percentile(&lateness, 0.99, 1.0).value,
            });
            set_late(&mut repetition, &measured);
        }
        repetition.set(
            "server_concurrency",
            server_concurrency(&population, usage.seconds),
        );
        if let Some(reason) = measured.integrity() {
            repetition.invalid.push(format!("client timing: {reason}"));
        }
        set_envelopes(&mut repetition, &population, op);
        repetition.set(
            "verifications",
            Reading::count(acknowledged - measured.late),
        );
        repetition.set("verifications_after_window", Reading::count(measured.late));
        repetition.set("drain_seconds", Reading::exact(usage.drain_seconds));
        repetition.set("errors", Reading::count(measured.unexpected(None)));
        repetition.set("wrong", Reading::count(measured.wrong));
        repetition.set("lost_writes", lost_writes(&first, &last));
        if !observed_components(&population) {
            repetition.invalid.push(
                "the verification runtime recorded no component observation in the window"
                    .to_owned(),
            );
        }
        repetition
            .observations
            .extend(components(&population, measured.latencies_us.len()));
        if !measured.errors.is_empty() {
            repetition
                .observations
                .push(format!("refusals by code: {:?}", measured.errors));
        }
        Ok(repetition)
    }

    /// One L2 repetition (or warmup): the [`Schedule`] of `step` over
    /// `window`, open loop, then the drain that returns the queue to empty
    /// and the window's direct server timings.
    ///
    /// # Errors
    ///
    /// Returns a workload error for an undeclared rate or a failed scrape.
    async fn competing(
        &self,
        step: &Step,
        window: Duration,
    ) -> Result<Repetition, ExperimentError> {
        let server = self.server()?;
        let schedule = Schedule::of(step, window).map_err(ExperimentError::workload)?;
        let states = Arc::new(self.states.clone());
        let tallies: Arc<[[Mutex<Measured>; 2]; 3]> = Arc::default();
        let admitted: Arc<[AtomicU64; 3]> = Arc::default();
        let sequence = Arc::new(AtomicU64::new(0));
        let before = timings(server).await?;
        let first = scrape(server).await?;
        let start = Instant::now();
        let driver_before = driver_cpu_seconds();
        let resources = ResourceWindow::open(server);
        let backlog_start = backlog(&first);
        let send = |op| {
            let states = Arc::clone(&states);
            let launch = move |next: u64| {
                let state = states
                    .get(usize::try_from(next).unwrap_or(0) % states.len().max(1))?
                    .clone();
                Some(async move { call(&state, op, next).await })
            };
            arrivals(
                op,
                schedule,
                start,
                launch,
                Arc::clone(&tallies),
                Arc::clone(&admitted),
                Arc::clone(&sequence),
            )
        };
        let (sent, observed) = tokio::join!(
            async { tokio::join!(send(Op::Verify), send(Op::Submit)) },
            observe(
                server,
                &schedule,
                start,
                (&first, &before),
                (resources, driver_before)
            ),
        );
        let (direct, background) = sent;
        let (observed, resources, driver) = observed?;
        let drain = drain(server, start + window).await?;
        let used = resources.finish(
            server,
            window.as_secs_f64(),
            [reaped_worker_cpu(&first), observed.reaped],
        );

        let tallies: [[Measured; 2]; 3] = std::array::from_fn(|stage| {
            std::array::from_fn(|slot| lock(&tallies[stage][slot]).clone())
        });
        let owed: Vec<Expected<'_>> = STAGES
            .iter()
            .zip(admitted.iter())
            .map(|(phase, count)| Expected {
                boundary: DIRECT_TIMING,
                outcome: COMPLETED,
                case: DIRECT_CASE,
                phase,
                count: count.load(Ordering::Relaxed),
            })
            .collect();
        let population = Population::collect(server, &before, &owed).await;
        let ended = Ended {
            snapshots: &observed.timings,
        };
        let mut repetition = Repetition {
            seconds: window.as_secs_f64(),
            processes: vec![PROCESS.to_owned()],
            server_cpu_seconds: used.cpu_seconds,
            peak_memory_bytes: used.peak_memory,
            opening_memory_bytes: used.opening_memory,
            memory_interval_ms: PEAK_SAMPLE_INTERVAL_MS,
            driver_cpu_seconds: driver,
            ..Repetition::default()
        };
        set_workers(
            &mut repetition,
            used.worker_cpu_seconds,
            used.worker_peak_memory,
        );
        set_worker_churn(
            &mut repetition,
            &worker_events(&first),
            &observed.worker_events,
        );
        let mut lateness = direct.lateness_us.clone();
        lateness.extend(&background.lateness_us);
        let answered: u64 = tallies.iter().flatten().map(Measured::answered).sum();
        repetition.arrivals = Some(wyrd_testing::experiment::Arrivals {
            scheduled: direct.scheduled + background.scheduled,
            sent: direct.sent + background.sent,
            completed: answered,
            missed: direct.missed + background.missed,
            lateness_p99_us: Reading::percentile(&lateness, 0.99, 1.0).value,
        });
        population.validate(&mut repetition, "measured");
        if !observed_components(&observed.whole) {
            repetition.invalid.push(
                "the verification runtime recorded no component observation in the window"
                    .to_owned(),
            );
        }
        let panicked = direct.panicked + background.panicked;
        if panicked > 0 {
            repetition
                .invalid
                .push(format!("{panicked} arrivals panicked before answering"));
        }
        if backlog_start > 0.0 {
            repetition.invalid.push(format!(
                "the repetition started with {backlog_start} queued items, not the drained initial state"
            ));
        }
        let mut errors = 0;
        for ((name, [verify, submit]), telemetry) in
            STAGES.iter().zip(&tallies).zip(&observed.stages)
        {
            let allowed = (*name == "overload").then_some(ADMISSION_REFUSAL);
            let unexpected = verify.unexpected(allowed) + submit.unexpected(allowed);
            errors += unexpected;
            repetition.set(&format!("{name}.errors"), Reading::count(unexpected));
            repetition.set(&format!("{name}.wrong"), Reading::count(verify.wrong));
            repetition.set(&format!("{name}.direct_p99_ms"), verify.percentile(0.99));
            repetition.set(
                &format!("{name}.submit_mean_ms"),
                mean_ms(telemetry.histogram(SUBMIT, &[STAGED])),
            );
            repetition.set(
                &format!("{name}.queue_wait_mean_ms"),
                mean_ms(telemetry.histogram(QUEUE_WAIT, &[SUBMITTED])),
            );
            for tally in [verify, submit] {
                if !tally.errors.is_empty() {
                    repetition
                        .observations
                        .push(format!("{name} refusals by code: {:?}", tally.errors));
                }
            }
        }
        let [_, _, recovery_stage] = schedule.stages();
        for name in STAGES {
            repetition.set(
                &format!("{name}.direct_overhead_p99_ms"),
                overhead_p99(&population.only(DIRECT_CASE, name)),
            );
        }
        let mut low = Duration::ZERO;
        let mut recovered = None;
        for (since, items) in &observed.ticks {
            let second =
                ended.overhead_p99(recovery_stage.start + low, recovery_stage.start + *since);
            if second.value.is_some_and(|p99| p99 < OVERHEAD_LIMIT_MS) && *items <= observed.bound {
                recovered = Some(since.as_secs_f64());
                break;
            }
            low = *since;
        }
        repetition.set(
            "recovery.seconds",
            recovered.map_or_else(
                || Reading::above(recovery_stage.length.as_secs_f64()),
                Reading::exact,
            ),
        );
        population.only(DIRECT_CASE, STAGES[0]).percentiles(
            &mut repetition,
            DIRECT_TIMING,
            COMPLETED,
            false,
            &[("p50_ms", 0.5), ("p95_ms", 0.95), ("p99_ms", 0.99)],
        );
        for (name, items) in STAGES.iter().zip(observed.backlog) {
            repetition.set(
                &format!("{name}.backlog_seconds"),
                schedule.backlog_seconds(items),
            );
        }
        let [capacity, overload, _] = &tallies;
        set_client(&mut repetition, &capacity[0]);
        repetition.set(
            "overload.admission_refusals",
            Reading::count(
                overload
                    .iter()
                    .map(|tally| tally.errors.get(ADMISSION_REFUSAL).copied().unwrap_or(0))
                    .sum(),
            ),
        );
        repetition.set("errors", Reading::count(errors));
        let late: u64 = tallies.iter().flatten().map(|tally| tally.late).sum();
        let acknowledged: u64 = tallies
            .iter()
            .flatten()
            .map(|tally| tally.latencies_us.len() as u64)
            .sum();
        repetition.set("verifications", Reading::count(acknowledged - late));
        repetition.set("verifications_after_window", Reading::count(late));
        repetition.set(
            "capacity.backlog_growth_seconds",
            schedule.backlog_seconds(observed.backlog[0] - backlog_start),
        );
        repetition.set(
            "recovery.backlog_growth_seconds",
            schedule.backlog_seconds(observed.backlog[2] - backlog_start),
        );
        repetition.set("drain_seconds", drain);
        let acks = acknowledgements(&observed.whole);
        repetition.set(
            "evidence_acks",
            acks.as_ref()
                .map_or_else(Reading::missing, |acks| Reading::count(acks.len())),
        );
        repetition.set("evidence_ack_mean_ms", mean_ms(acks));
        Ok(repetition)
    }
}

#[async_trait]
impl Workload for Bench {
    /// Starts the release server through the guide and provisions the
    /// tenant's applications: one per L1 client at the largest declared
    /// count, or [`APPLICATIONS`] for the L2 rotation.
    ///
    /// # Errors
    ///
    /// Returns a workload error for a start or provisioning failure. A
    /// started server is kept even when provisioning fails, so
    /// [`Workload::finish`] stops it; nothing is retried.
    async fn prepare(&mut self) -> Result<(), ExperimentError> {
        self.start().await?;
        if self.level == Level::One && self.profile.is_some() {
            for language in [Language::Python, Language::TypeScript] {
                self.foreign(language, Op::Canonical).await?;
            }
        }
        Ok(())
    }

    /// The server's PID, once it runs.
    fn processes(&self) -> Vec<u32> {
        self.server
            .as_ref()
            .and_then(LocalServer::pid)
            .into_iter()
            .collect()
    }

    fn heap_profile_url(&self) -> Option<String> {
        self.server.as_ref().map(LocalServer::heap_profile_url)
    }

    /// Each started language driver's role and PID.
    fn clients(&self) -> Vec<(Role, u32)> {
        self.drivers
            .iter()
            .map(|(language, driver)| (language.role(), driver.pid))
            .collect()
    }

    /// Saves each language driver's heap capture of `step` as
    /// `heap-<language>-<step>` plus each file's extension when the run
    /// profiles the heap, noting a driver that cannot save one or whose
    /// native module is no `bench` build, so only its managed heap is
    /// sampled. Their CPU is the external profiler's capture of each driver
    /// process, which reads the runtime's perf map.
    async fn capture_clients(&mut self, step: &str, directory: &Path) -> ProfileNote {
        let mut note = ProfileNote::default();
        if !self.profile.is_some_and(ProfileMode::heap) || self.drivers.is_empty() {
            return note;
        }
        if let Err(error) = std::fs::create_dir_all(directory) {
            note.notes.push(format!(
                "client captures after {step} failed: cannot create {}: {error}",
                directory.display()
            ));
            return note;
        }
        for (language, driver) in &mut self.drivers {
            let name = format!("heap-{}-{step}", language.name());
            match driver.capture(&directory.join(&name)).await {
                Ok(captured) => {
                    if !captured.paths.iter().any(|path| path.ends_with(".pb.gz")) {
                        note.notes.push(format!(
                            "{name} holds the managed heap only: the {} native module is not a `bench` build, so nothing samples its allocations",
                            language.name()
                        ));
                    }
                    note.captures.extend(captured.paths);
                }
                Err(error) => note.notes.push(format!("{name} failed: {error}")),
            }
        }
        note
    }

    /// Runs `step` for `window`: closed loop at L1, the whole open-loop
    /// sequence and its drain at L2. A warmup runs the same work.
    ///
    /// # Errors
    ///
    /// Returns a workload error for an unprepared workload, an undeclared
    /// step, or a failed scrape. A cancelled window aborts its client and
    /// arrival tasks with their requests; work the server already queued
    /// stays queued until [`Workload::finish`] stops it.
    async fn measure(
        &mut self,
        step: &Step,
        phase: Phase,
        window: Duration,
    ) -> Result<Repetition, ExperimentError> {
        match self.level {
            Level::One => self.closed(step, phase, window).await,
            Level::Two => Box::pin(self.competing(step, window)).await,
        }
    }

    /// Drops the application states, then stops the server and keeps its
    /// log beside the tenant's files. The stop runs inside
    /// this future: `SIGTERM`, a bounded wait that yields while the server
    /// drains, and the log copy.
    ///
    /// # Errors
    ///
    /// Returns a workload error when the server exits unsuccessfully or
    /// outlives its stop grace (it is then killed), or its log cannot be
    /// copied. An unstarted workload finishes without error.
    async fn finish(&mut self) -> Result<(), ExperimentError> {
        let mut stopped = Ok(());
        for driver in std::mem::take(&mut self.drivers).into_values() {
            stopped = stopped.and(driver.stop().await);
        }
        self.states.clear();
        self.provider = None;
        let Some(server) = self.server.take() else {
            return stopped;
        };
        server
            .stop(&self.logs.join("server.log"))
            .await
            .map(|_| ())
            .map_err(ExperimentError::workload)
            .and(stopped)
    }
}

/// One L2 repetition's benchmark timings and resource observations.
struct Observed {
    /// Each phase's benchmark timing interval, in [`STAGES`] order.
    stages: [Population; 3],
    /// Queued backlog items at each phase's end.
    backlog: [f64; 3],
    /// Each recovery read: its offset from recovery's start and the queued
    /// backlog items then.
    ticks: Vec<(Duration, f64)>,
    /// The recovered backlog bound: capacity's backlog plus
    /// [`BACKLOG_LIMIT_SECONDS`] of offered work.
    bound: f64,
    /// The whole window's benchmark timing interval.
    whole: Population,
    /// The server's timing snapshot at each read, by its planned offset
    /// from the schedule's start, the start itself first.
    timings: Vec<(Duration, BenchmarkSnapshot)>,
    /// The server's reaped worker CPU in the scrape started just after the
    /// resource window froze.
    reaped: Option<f64>,
    /// The worker lifecycle event totals in that scrape ([`worker_events`]).
    worker_events: BTreeMap<String, f64>,
}

/// A window's completed direct executions, placed by when they ended: by
/// the timing snapshots taken at its phase boundaries and recovery reads.
struct Ended<'a> {
    /// Each snapshot by its offset from the schedule's start.
    snapshots: &'a [(Duration, BenchmarkSnapshot)],
}

impl Ended<'_> {
    /// The executions recorded between the snapshots at `low` and `high`
    /// after the schedule's start.
    ///
    /// # Errors
    ///
    /// Returns why there is no such interval: no snapshot was taken at
    /// either offset, or the two cannot be differenced.
    fn between(&self, low: Duration, high: Duration) -> Result<Population, String> {
        let at = |offset: Duration| {
            self.snapshots
                .iter()
                .find(|(taken, _)| *taken == offset)
                .map(|(_, snapshot)| snapshot)
                .ok_or_else(|| format!("no timing snapshot was taken at {offset:?}"))
        };
        Ok(Population::from_interval(at(high)?.since(at(low)?)?))
    }

    /// The p99, milliseconds, of the residuals of the executions that ended
    /// between `low` and `high`; missing when one has no residual or the
    /// interval is invalid.
    fn overhead_p99(&self, low: Duration, high: Duration) -> Reading {
        match self.between(low, high) {
            Ok(population) => overhead_p99(&population),
            Err(reason) => Reading::missing(reason),
        }
    }
}

/// The p99, milliseconds, of `population`'s completed direct residuals;
/// missing when one has no residual, none completed, or the population is
/// invalid.
fn overhead_p99(population: &Population) -> Reading {
    if let Some(reason) = population.invalid.first() {
        return Reading::missing(reason.clone());
    }
    population
        .residuals(DIRECT_TIMING, COMPLETED)
        .map_or_else(Reading::missing, |residual| {
            Reading::quantile(&residual, 0.99, NANOS_PER_MS)
        })
}

/// Reads the benchmark snapshot at each phase boundary of
/// `schedule` from `start`, and every [`POLL`] through recovery, keeping
/// each recovery read's backlog for the recovery judgment; `first` is the
/// pair read at the start. Freezes server CPU and returns the driver CPU
/// since `driver_before` at the window's end, before its closing scrape.
///
/// # Errors
///
/// Returns a workload error for a failed scrape.
async fn observe(
    server: &LocalServer,
    schedule: &Schedule,
    start: Instant,
    (_first, opened_timings): (&Metrics, &BenchmarkSnapshot),
    (mut resources, driver_before): (ResourceWindow, Option<f64>),
) -> Result<(Observed, ResourceWindow, Option<f64>), ExperimentError> {
    let [_, overload, recovery] = schedule.stages();
    let mut snapshots = vec![(Duration::ZERO, opened_timings.clone())];
    tokio::time::sleep_until(start + overload.start).await;
    let at_overload = scrape(server).await?;
    snapshots.push((overload.start, timings(server).await?));
    tokio::time::sleep_until(start + recovery.start).await;
    let at_recovery = scrape(server).await?;
    snapshots.push((recovery.start, timings(server).await?));
    let backlog_capacity = backlog(&at_overload);
    let bound = backlog_capacity + BACKLOG_LIMIT_SECONDS * schedule.offered();
    let mut ticks = Vec::new();
    let mut offset = recovery.start;
    let mut driver = None;
    let last_metrics = loop {
        offset = (offset + POLL).min(schedule.window);
        tokio::time::sleep_until(start + offset).await;
        if offset >= schedule.window {
            resources.freeze(server);
            driver = elapsed(driver_before, driver_cpu_seconds());
        }
        let metrics = scrape(server).await?;
        snapshots.push((offset, timings(server).await?));
        ticks.push((offset.saturating_sub(recovery.start), backlog(&metrics)));
        if offset >= schedule.window {
            break metrics;
        }
    };
    let interval = |from: usize, to: usize| {
        Population::between(&snapshots[from].1, snapshots[to].1.clone())
            .map_err(ExperimentError::workload)
    };
    let stages = [
        interval(0, 1)?,
        interval(1, 2)?,
        interval(2, snapshots.len() - 1)?,
    ];
    let whole = interval(0, snapshots.len() - 1)?;
    let observed = Observed {
        backlog: [
            backlog_capacity,
            backlog(&at_recovery),
            backlog(&last_metrics),
        ],
        ticks,
        bound,
        stages,
        whole,
        timings: snapshots,
        reaped: reaped_worker_cpu(&last_metrics),
        worker_events: worker_events(&last_metrics),
    };
    Ok((observed, resources, driver))
}

/// What one open-loop sender scheduled and sent.
#[derive(Debug, Default)]
struct Offered {
    /// Arrivals the schedule planned.
    scheduled: u64,
    /// Arrivals sent.
    sent: u64,
    /// Planned arrivals never sent because [`MAX_IN_FLIGHT`] were pending.
    missed: u64,
    /// Arrivals that panicked before answering.
    panicked: u64,
    /// Each sent arrival's lateness behind its schedule, microseconds.
    lateness_us: Vec<u64>,
}

/// Sends `op` arrivals at each phase's rate of `schedule` from `start`,
/// each the call `launch` builds for its sequence number, labelled with its
/// admission phase, recording each answer into its schedule phase's tally
/// and marking one after the window end late, counting each acknowledged
/// direct call in `admitted` by its admission phase, and waits for every
/// sent arrival to answer. An arrival whose wake comes at or after the
/// window end, or for which `launch` has no state to call, is missed:
/// nothing is admitted once measurement stopped.
async fn arrivals<Fut>(
    op: Op,
    schedule: Schedule,
    start: Instant,
    launch: impl Fn(u64) -> Option<Fut>,
    tallies: Arc<[[Mutex<Measured>; 2]; 3]>,
    admitted: Arc<[AtomicU64; 3]>,
    sequence: Arc<AtomicU64>,
) -> Offered
where
    Fut: Future<Output = Result<bool, WyrdError>> + Send + 'static,
{
    let mut offered = Offered::default();
    let mut tasks = JoinSet::new();
    let slot = usize::from(op == Op::Submit);
    let end = start + schedule.window;
    let [_, _, recovery] = schedule.stages();
    for (index, stage) in schedule.stages().into_iter().enumerate() {
        let rate = match op {
            Op::Verify | Op::Judge | Op::Canonical => stage.direct_rate,
            Op::Submit => stage.submit_rate,
        };
        for arrival in 0u32.. {
            let offset = Duration::from_secs_f64(f64::from(arrival) / rate);
            if offset >= stage.length {
                break;
            }
            let at = start + stage.start + offset;
            let phase = admission(recovery.start, recovery.length, index, stage.start + offset);
            tokio::time::sleep_until(at).await;
            offered.scheduled += 1;
            while let Some(joined) = tasks.try_join_next() {
                offered.panicked += u64::from(joined.is_err());
            }
            if tasks.len() >= MAX_IN_FLIGHT || Instant::now() >= end {
                offered.missed += 1;
                continue;
            }
            let next = sequence.fetch_add(1, Ordering::Relaxed);
            let Some(call) = launch(next) else {
                offered.missed += 1;
                continue;
            };
            offered.lateness_us.push(micros(at));
            let (tallies, admitted) = (Arc::clone(&tallies), Arc::clone(&admitted));
            tasks.spawn(async move {
                let called = Instant::now();
                let outcome = with_benchmark_labels(op.case(), STAGES[phase], call).await;
                let late = Instant::now() >= end;
                if op == Op::Verify && outcome.is_ok() {
                    admitted[phase].fetch_add(1, Ordering::Relaxed);
                }
                lock(&tallies[index][slot]).record(
                    micros(called),
                    outcome.as_ref().copied().map_err(WyrdError::code),
                    late,
                );
            });
            offered.sent += 1;
        }
    }
    while let Some(joined) = tasks.join_next().await {
        offered.panicked += u64::from(joined.is_err());
    }
    offered
}

/// The [`STAGES`] index of the admission phase of an arrival due `offset`
/// after the start in schedule phase `index`: recovery's arrivals within its
/// grace of its start at `recovery`, [`RECOVERY_LIMIT`] or half its `length`
/// when shorter, join overload's cohort, so a short smoke recovery still
/// judges a cohort.
fn admission(recovery: Duration, length: Duration, index: usize, offset: Duration) -> usize {
    if index == 2 && offset < recovery + RECOVERY_LIMIT.min(length / 2) {
        1
    } else {
        index
    }
}

/// Waits from `stopped`, when load stopped, until no queued execution is
/// pending, running, or retrying and both [`OUTBOXES`] are empty, and
/// returns the seconds it took, or a reading above [`DRAIN_WAIT`] when it did
/// not drain by then.
///
/// # Errors
///
/// Returns a workload error for a failed scrape.
async fn drain(server: &LocalServer, stopped: Instant) -> Result<Reading, ExperimentError> {
    loop {
        let metrics = scrape(server).await?;
        let waited = stopped.elapsed();
        if backlog(&metrics) <= 0.0 {
            return Ok(Reading::exact(waited.as_secs_f64()));
        }
        if waited >= DRAIN_WAIT {
            return Ok(Reading::above(DRAIN_WAIT.as_secs_f64()));
        }
        tokio::time::sleep(DRAIN_POLL).await;
    }
}

/// Queued verification items: the queue depth plus both [`OUTBOXES`].
pub(crate) fn backlog(metrics: &Metrics) -> f64 {
    metrics.sum("wyrd_verification_queue_depth", &[])
        + OUTBOXES
            .iter()
            .map(|outbox| metrics.sum("outbox_pending", &[&format!("outbox=\"{outbox}\"")]))
            .sum::<f64>()
}

/// Reads the server's cumulative benchmark timing snapshot.
///
/// # Errors
///
/// Returns the read failure as a workload error.
async fn timings(server: &LocalServer) -> Result<BenchmarkSnapshot, ExperimentError> {
    server.timings().await.map_err(ExperimentError::workload)
}

/// Scribe outbox writes lost for any reason between two scrapes, shed audit
/// decisions included; missing when a counter went backwards.
pub(crate) fn lost_writes(first: &Metrics, last: &Metrics) -> Reading {
    let labels = ["outbox=\"scribe\""];
    let lost = last.sum("outbox_events_lost_total", &labels)
        - first.sum("outbox_events_lost_total", &labels);
    if lost.is_finite() && lost >= 0.0 {
        Reading::exact(lost)
    } else {
        Reading::missing("the Scribe outbox loss counter went backwards")
    }
}

/// Reads the server's benchmark snapshot as metrics.
///
/// # Errors
///
/// Returns the read failure as a workload error.
async fn scrape(server: &LocalServer) -> Result<Metrics, ExperimentError> {
    server.metrics().await.map_err(ExperimentError::workload)
}

/// A paced window's arrival slots: `clients` callers that together offer
/// `rate` calls per second from `start`, client `i`'s `k`-th call due at
/// slot `i + k * clients`, so the slots fall `1 / rate` seconds apart.
#[derive(Debug, Clone, Copy)]
struct Pace {
    /// When slot zero is due.
    start: Instant,
    /// The callers sharing the slots.
    clients: u64,
    /// Calls per second the slots offer.
    rate: u64,
}

impl Pace {
    /// When client `index`'s `k`-th call is due.
    fn due(self, index: u64, k: u64) -> Instant {
        let slot = index + k * self.clients;
        self.start + Duration::from_secs_f64(slot as f64 / self.rate as f64)
    }

    /// The slots due before `window` ends.
    fn scheduled(self, window: Duration) -> u64 {
        u64::try_from((window.as_nanos() * u128::from(self.rate)).div_ceil(1_000_000_000))
            .unwrap_or(u64::MAX)
    }
}

/// Runs one client per state making direct `op` calls, labelled with
/// `op`'s case and `phase`, admitted until `stop`, client `i` presenting
/// application `i`'s key; a call answering at or after `stop` is late. Each
/// client calls back to back until `stop`, or under `pace` waits for each
/// call's due slot and records how late it started; every slot due before
/// `stop` is sent, however late, and a slot due at or after it never is. Each call's client time starts immediately before the public
/// `verify` and stops when its decoded result or error returns: the Run and
/// context are built before it and the result is judged after it.
async fn drive(
    states: &[WyrdState],
    op: Op,
    stop: Instant,
    phase: &'static str,
    pace: Option<Pace>,
) -> Measured {
    let sequence = Arc::new(AtomicU64::new(0));
    let mut tasks = JoinSet::new();
    for (index, state) in (0u64..).zip(states) {
        let (state, sequence) = (state.clone(), Arc::clone(&sequence));
        tasks.spawn(async move {
            let mut measured = Measured::default();
            for k in 0u64.. {
                if let Some(pace) = pace {
                    let due = pace.due(index, k);
                    if due >= stop {
                        break;
                    }
                    tokio::time::sleep_until(due).await;
                    let offset = due.saturating_duration_since(pace.start);
                    let offset = u64::try_from(offset.as_micros()).unwrap_or(u64::MAX);
                    let late = micros(due);
                    measured.lateness_us.push((offset, late));
                    measured.woke_us.push(late);
                } else if Instant::now() >= stop {
                    break;
                }
                let next = sequence.fetch_add(1, Ordering::Relaxed);
                let run = match state.run_for_card("model") {
                    Ok(run) => run,
                    Err(error) => {
                        measured.record(0, Err(error.code()), false);
                        continue;
                    }
                };
                let observe = run.observe();
                let (verifier, inputs) = op.arrival(next);
                // Boxed before the timer: building a future does no work,
                // so only its allocation stays outside the samples.
                let call = Box::pin(with_benchmark_labels(
                    op.case(),
                    phase,
                    observe.verify(verifier, inputs),
                ));
                let (returned, sample) =
                    Sample::take(call, async |result| materialize(result)).await;
                measured.judged(op, next, sample, returned, Instant::now() >= stop);
            }
            measured
        });
    }
    let mut total = Measured::default();
    while let Some(joined) = tasks.join_next().await {
        match joined {
            Ok(measured) => total.merge(measured),
            Err(_) => *total.errors.entry("panicked".to_owned()).or_default() += 1,
        }
    }
    total
}

/// Makes arrival `sequence` as `op` through `state`, the Service's
/// application lifetime, as one L2 arrival times it: Run, call, and
/// judgment together.
///
/// # Errors
///
/// Returns the client's or server's refusal; a direct verdict other than the
/// expected one ([`Op::expected`]) is `Ok(false)`, not an error.
async fn call(state: &WyrdState, op: Op, sequence: u64) -> Result<bool, WyrdError> {
    let run = state.run_for_card("model")?;
    let observe = run.observe();
    let (verifier, inputs) = op.arrival(sequence);
    match op {
        Op::Submit => observe.submit(verifier, inputs).await.map(|_| true),
        Op::Verify | Op::Judge | Op::Canonical => {
            Ok(op.expected(sequence, &observe.verify(verifier, inputs).await?.outcome))
        }
    }
}

/// Reads the details of each `sampled` canonical execution through
/// `client` once its records are visible, waiting up to [`DRAIN_WAIT`],
/// and counts each that is unreadable or whose Agent checks do not each
/// record exactly one error-free model-recipient `code.run` with output,
/// with the first problems named.
async fn recorded(client: &WyrdClient, sampled: &[ExecutionId]) -> (u64, Vec<String>) {
    let mut problems = Vec::new();
    for id in sampled {
        let deadline = Instant::now() + DRAIN_WAIT;
        let details = async {
            let execution = Execution::connect_with(*id, client).await?;
            loop {
                match execution.details().await {
                    Err(error) if error.code() == NOT_VISIBLE && Instant::now() < deadline => {
                        tokio::time::sleep(DRAIN_POLL).await;
                    }
                    read => return read,
                }
            }
        };
        match details.await {
            Ok(details) => problems.extend(unrecorded(&details).map(|agent| {
                format!("execution {id}: Agent `{agent}` lacks one clean `code.run` record")
            })),
            Err(error) => problems.push(format!("execution {id}: details unreadable: {error}")),
        }
    }
    (problems.len() as u64, problems)
}

/// The canonical Agents whose requirement in `details` does not record
/// exactly one error-free `code.run` the model received, with output.
fn unrecorded(details: &VerificationDetails) -> impl Iterator<Item = &'static str> + '_ {
    AGENTS.into_iter().filter(|agent| {
        !details.outcome.requirements.iter().any(|requirement| {
            requirement.result.id.as_str() == *agent
                && matches!(&requirement.tool_calls[..], [record]
                    if record.tool == BuiltinTool::CodeRun
                        && record.recipient == ToolRecipient::Model
                        && record.error.is_none()
                        && record.output_sha256.is_some())
        })
    })
}

/// Sets the client call-return percentiles from `measured`.
fn set_client(repetition: &mut Repetition, measured: &Measured) {
    for (id, q) in [
        ("client_p50_ms", 0.5),
        ("client_p95_ms", 0.95),
        ("client_p99_ms", 0.99),
    ] {
        repetition.set(id, measured.percentile(q));
    }
}

/// Sets the server's worker children's CPU seconds over the measured window
/// and their sampled peak summed resident memory, apart from the parent-only
/// server CPU and memory; an unavailable reading is missing with its reason,
/// never zero.
fn set_workers(repetition: &mut Repetition, cpu_seconds: Option<f64>, peak_memory: Option<u64>) {
    repetition.set(
        "worker_cpu_seconds",
        cpu_seconds.map_or_else(
            || {
                Reading::missing(
                    "worker CPU was unavailable: the server reported no reaped-worker CPU, or its live workers were unreadable, at an end of the window",
                )
            },
            Reading::exact,
        ),
    );
    repetition.set(
        "worker_peak_memory_bytes",
        peak_memory.map_or_else(
            || Reading::missing("no worker process was sampled in the window"),
            |bytes| Reading::exact(bytes as f64),
        ),
    );
}

/// Each worker lifecycle event a repetition records, as its reading id and
/// the server's `event` label.
const WORKER_EVENTS: [(&str, &str); 3] = [
    ("worker_spawns", "spawn"),
    ("worker_resets", "reset"),
    ("worker_reaps", "reap"),
];

/// The server's cumulative worker lifecycle event totals in `scrape`, by
/// `event`; an event that never happened is absent.
fn worker_events(scrape: &Metrics) -> BTreeMap<String, f64> {
    scrape.by_label(
        wyrd_server::app::metrics::VERIFICATION_PYTHON_WORKER_EVENTS_TOTAL,
        "event",
    )
}

/// Sets the worker churn between the `opened` and `closed` event totals
/// ([`worker_events`]): workers spawned, reset for reuse, and reaped inside
/// the window. An event absent at both ends never happened, so it reads zero;
/// a total that went backwards is missing.
fn set_worker_churn(
    repetition: &mut Repetition,
    opened: &BTreeMap<String, f64>,
    closed: &BTreeMap<String, f64>,
) {
    for (id, event) in WORKER_EVENTS {
        let total = |events: &BTreeMap<String, f64>| events.get(event).copied().unwrap_or(0.0);
        let count = total(closed) - total(opened);
        repetition.set(
            id,
            if count >= 0.0 {
                Reading::exact(count)
            } else {
                Reading::missing(format!("the worker `{event}` total went backwards"))
            },
        );
    }
}

/// Sets the client process's CPU seconds over the measured window and its
/// sampled peak resident memory from `memory`, and names the process: a
/// language `driver`, separate from this controller and its local provider,
/// or this process itself for Rust clients, whose readings then include the
/// controller and provider work.
fn set_client_process(
    repetition: &mut Repetition,
    driver: Option<&ForeignDriver>,
    cpu_seconds: Option<f64>,
    memory: PeakSampler,
) {
    repetition.set(
        "client_cpu_seconds",
        cpu_seconds.map_or_else(
            || Reading::missing("the client process CPU was unreadable"),
            Reading::exact,
        ),
    );
    repetition.set(
        "client_peak_memory_bytes",
        memory.finish().map_or_else(
            |error| Reading::missing(error.to_string()),
            |memory| Reading::exact(memory.peak as f64),
        ),
    );
    let observation = match driver {
        None => format!(
            "client process: rust `wyrd_client` tasks in this controller (pid {}), which also hosts the local provider, so client CPU and memory include both and equal the driver readings; binary {}",
            std::process::id(),
            std::env::current_exe()
                .ok()
                .and_then(|binary| binary_identity(&binary).ok())
                .unwrap_or_default()
        ),
        Some(driver) => format!(
            "client process: {} (pid {}), separate from this controller and its local provider, whose CPU is driver CPU; native module {}",
            driver.identity,
            driver.pid,
            driver.identity["native"]
                .as_str()
                .and_then(|native| binary_identity(Path::new(native)).ok())
                .unwrap_or_default()
        ),
    };
    repetition.observations.push(observation);
}

/// Sets a `language` step's client breakdown from the recorder window its
/// driver reported around its calls, `acknowledged` of them, `clean` when
/// none was refused: every root of [`client_roots`] and its components
/// under the language's role, and the execution response sizes. Every one is
/// required evidence, so a dropped phase or root is invalid. A driver whose
/// native module is no `bench` build reports no window: its readings stay
/// missing and its attribution unfinished, while its total samples, timed
/// on the language clock, stay complete. Every other language's client roots
/// are not reached.
fn set_language_client(
    repetition: &mut Repetition,
    language: Language,
    timings: Option<(BenchmarkSnapshot, BenchmarkSnapshot)>,
    acknowledged: u64,
    clean: bool,
) {
    unreached_clients(repetition, language);
    let roots = client_roots(language);
    let role = language.role().name();
    let Some((before, after)) = timings else {
        let reason = format!(
            "unrecorded: the {} driver reported no benchmark window; its native module is not a `bench` build",
            language.name()
        );
        for (root, _, components) in roots {
            repetition.diagnose(&format!("{role}.{root}"), Err(reason.clone()), false);
            for component in *components {
                repetition.diagnose(
                    &format!("{role}.{root}.{component}"),
                    Err(reason.clone()),
                    false,
                );
            }
        }
        for (id, _) in RESPONSE_BYTE_STATS {
            repetition.set(id, Reading::missing(reason.clone()));
        }
        repetition.observations.push(format!(
            "attribution unfinished: {reason}; client totals are complete language-clock samples"
        ));
        return;
    };
    let cohort = (wyrd_benchmark::UNLABELLED, wyrd_benchmark::UNLABELLED);
    let expected: Vec<Expected<'_>> = roots
        .iter()
        .map(|(boundary, _, _)| Expected {
            boundary,
            outcome: wyrd_benchmark::SUCCESS,
            case: cohort.0,
            phase: cohort.1,
            count: acknowledged,
        })
        .collect();
    let population = Population::reported(language.role(), &before, after, &expected);
    population.validate(repetition, "measured");
    let answered = clean.then_some(acknowledged);
    population.diagnose_roots(repetition, cohort, roots, answered);
    population.response_bytes(repetition, HTTP_ROOT, cohort, answered);
}

/// Marks the client roots of every language but `language` inapplicable:
/// a step's clients call through one SDK, so no other is reached.
fn unreached_clients(repetition: &mut Repetition, language: Language) {
    let reason = format!("this step's clients call the {} SDK", language.name());
    for other in Language::ALL.into_iter().filter(|other| *other != language) {
        let role = other.role().name();
        for (root, _, components) in client_roots(other) {
            repetition.inapplicable(&format!("{role}.{root}"), &reason);
            for component in *components {
                repetition.inapplicable(&format!("{role}.{root}.{component}"), &reason);
            }
        }
    }
}

/// The per-call materialization percentiles: each acknowledged call's
/// result-ready sample less its own call-return sample.
pub const MATERIALIZE_STATS: [(&str, f64); 2] = [
    ("client_materialize_p50_ms", 0.5),
    ("client_materialize_p99_ms", 0.99),
];

/// Sets the client result-ready percentiles from `measured`, and the
/// per-call materialization percentiles.
fn set_ready(repetition: &mut Repetition, measured: &Measured) {
    for (id, q) in [
        ("client_ready_p50_ms", 0.5),
        ("client_ready_p95_ms", 0.95),
        ("client_ready_p99_ms", 0.99),
    ] {
        repetition.set(id, Reading::percentile(&measured.ready_us, q, 1e3));
    }
    let materialized: Vec<u64> = measured
        .latencies_us
        .iter()
        .zip(&measured.ready_us)
        .map(|(returned, ready)| ready.saturating_sub(*returned))
        .collect();
    for (id, q) in MATERIALIZE_STATS {
        repetition.set(id, Reading::percentile(&materialized, q, 1e3));
    }
}

/// Sets each [`ENVELOPES`] diagnostic over the completed direct executions
/// that reached it. An envelope `op`'s Verifier never reaches is
/// inapplicable; one that covers fewer completed executions than reached
/// it leaves attribution unfinished, an observation that never makes the
/// whole-call timing invalid.
fn set_envelopes(repetition: &mut Repetition, population: &Population, op: Op) {
    let completed = population
        .elapsed(DIRECT_TIMING, COMPLETED)
        .map_or(0, |distribution| distribution.len());
    for (observation, boundary, judged) in ENVELOPES {
        if !(op == Op::Canonical || (op == Op::Judge && judged)) {
            repetition.inapplicable(observation, NO_ENVELOPE);
            continue;
        }
        let distribution = population
            .elapsed(boundary, COMPLETED)
            .ok_or_else(|| format!("no completed `{boundary}` time in the window"));
        let covered = distribution
            .as_ref()
            .map_or(0, wyrd_benchmark::Distribution::len);
        repetition.diagnose(observation, distribution, false);
        if covered != completed {
            repetition.observations.push(format!(
                "attribution unfinished: `{boundary}` covers {covered} of {completed} completed executions"
            ));
        }
    }
}

/// Sets each [`OVERHEAD_TAILS`] count of completed direct executions whose
/// residual exceeds its threshold, as the bounds the histogram allows; each
/// is missing when there is no residual.
fn set_tails(repetition: &mut Repetition, population: &Population) {
    let residuals = population.residuals(DIRECT_TIMING, COMPLETED);
    for (id, nanos) in OVERHEAD_TAILS {
        repetition.set(
            id,
            residuals
                .as_ref()
                .map_or_else(Reading::missing, |residuals| {
                    let (certain, possible) = residuals.count_above(nanos);
                    Reading::bounded(certain as f64, possible as f64, 1.0)
                }),
        );
    }
}

/// The time-averaged count of completed direct executions in progress over
/// a `seconds` window: their summed server elapsed time over the window, as
/// the bounds of the histogram's bucketed total, or why there is none.
fn server_concurrency(population: &Population, seconds: f64) -> Reading {
    match population.elapsed(DIRECT_TIMING, COMPLETED) {
        Some(distribution) if !distribution.is_empty() && seconds > 0.0 => {
            let (low, high) = distribution.total_bounds();
            Reading::bounded(low as f64, high as f64, seconds * 1e9)
                .with_samples(distribution.len())
        }
        _ => Reading::missing("no completed direct execution in the window"),
    }
}

/// Sets each [`PHASE_TIMINGS`] phase's p99 and maximum, milliseconds, over
/// the completed direct executions. A phase the window did not time is
/// missing with its reason; phases are diagnostic, so that never makes the
/// repetition invalid.
fn set_phases(repetition: &mut Repetition, population: &Population) {
    for (phase, boundary) in PHASE_TIMINGS {
        let distribution = population.elapsed(boundary, COMPLETED);
        for (statistic, q) in [("p99", 0.99), ("max", 1.0)] {
            repetition.set(
                &format!("phase_{phase}_{statistic}_ms"),
                distribution.as_ref().map_or_else(
                    || Reading::missing(format!("no completed `{boundary}` time in the window")),
                    |distribution| Reading::quantile(distribution, q, NANOS_PER_MS),
                ),
            );
        }
    }
}

/// Sets the server's `gateway.phase.<phase>` and `gateway.wait.<wait>`
/// diagnostics for each [`GATEWAY_PHASES`] phase and every controlled wait
/// a judge's in-process model call records, each over its own histogram
/// population, nested in the Verifier block and never summed. An assertion
/// step calls no provider, so each is inapplicable; on a judge step without
/// errors each is required.
fn set_gateway(repetition: &mut Repetition, population: &Population, op: Op, clean: bool) {
    let observations = GATEWAY_PHASES
        .iter()
        .map(|phase| {
            (
                format!("gateway.phase.{phase}"),
                PHASE_SECONDS,
                ("phase", *phase),
            )
        })
        .chain(WAITS.iter().map(|wait| {
            (
                format!("gateway.wait.{wait}"),
                WAIT_SECONDS,
                ("wait", *wait),
            )
        }));
    for (observation, family, label) in observations {
        if op == Op::Verify {
            repetition.inapplicable(&observation, NO_PROVIDER);
        } else {
            repetition.diagnose(&observation, population.histogram(family, &[label]), clean);
        }
    }
}

/// The mean, milliseconds, of a nanosecond `distribution` as the bounds of
/// its bucketed total, or why there is none.
pub(crate) fn mean_ms(distribution: Result<Distribution, String>) -> Reading {
    match distribution {
        Err(reason) => Reading::missing(reason),
        Ok(distribution) if distribution.is_empty() => {
            Reading::missing("no observation in the window")
        }
        Ok(distribution) => {
            let (low, high) = distribution.total_bounds();
            Reading::bounded(
                low as f64,
                high as f64,
                distribution.len() as f64 * NANOS_PER_MS,
            )
            .with_samples(distribution.len())
        }
    }
}

/// Every Scribe acknowledgement in `population`: each timed append and
/// each deduplicated retry's histogram observation.
///
/// # Errors
///
/// Returns why neither was recorded: no timed append and the retry
/// histogram's [`Population::histogram`] refusal (absent, or it refused a
/// value), or the merge's range refusal when both exist.
pub(crate) fn acknowledgements(population: &Population) -> Result<Distribution, String> {
    let retries = population.histogram(SCRIBE_ACK, &[]);
    match (population.elapsed(SCRIBE_WRITE, "success"), retries) {
        (Some(mut appends), Ok(retries)) => appends.add(&retries).map(|()| appends),
        (Some(appends), Err(_)) => Ok(appends),
        (None, retries) => retries,
    }
}

/// The runtime's component times over a window, largest total first: each
/// [`BREAKDOWN`] histogram series and each direct execution phase part,
/// keyed `family{label=value}` with the `wyrd_verification_` prefix and the
/// `_seconds` unit dropped, or by part boundary.
///
/// # Panics
///
/// Panics when two duration distributions do not merge, which every
/// nanosecond HDR histogram's shared configuration rules out.
fn component_totals(population: &Population) -> BTreeMap<String, Distribution> {
    let mut components = BTreeMap::<String, Distribution>::new();
    let mut add = |key: String, distribution: &Distribution| match components.get_mut(&key) {
        Some(total) => {
            total
                .add(distribution)
                .expect("invariant: every duration histogram shares one HDR configuration");
        }
        None => {
            components.insert(key, distribution.clone());
        }
    };
    for (family, label) in BREAKDOWN {
        let name = family
            .trim_start_matches("wyrd_verification_")
            .trim_end_matches("_seconds");
        for metric in population
            .metrics
            .iter()
            .filter(|metric| metric.name == family)
        {
            let IntervalValue::Histogram { distribution, .. } = &metric.value else {
                continue;
            };
            let key = match metric.labels.get(label) {
                Some(value) => format!("{name}{{{label}={value}}}"),
                None => name.to_owned(),
            };
            add(key, distribution);
        }
    }
    for series in &population.series {
        if series.boundary.starts_with(DIRECT_PARTS) {
            add(series.boundary.clone(), &series.elapsed);
        }
    }
    components.retain(|_, distribution| !distribution.is_empty());
    components
}

/// Whether `population` holds any verification runtime component
/// observation, as every executed `verify` records its phases and Verifier
/// blocks.
pub(crate) fn observed_components(population: &Population) -> bool {
    !component_totals(population).is_empty()
}

/// The runtime's component times over a window, largest total first: each
/// component's milliseconds per acknowledged call beside its mean per
/// observation, as the bounds of its bucketed total. Components nest and
/// background work overlaps calls, so they are read beside the client
/// latency, never summed.
pub(crate) fn components(population: &Population, acknowledged: usize) -> Vec<String> {
    let calls = acknowledged.max(1) as f64;
    let mut components: Vec<(String, (u64, u64), u64)> = component_totals(population)
        .into_iter()
        .map(|(key, distribution)| (key, distribution.total_bounds(), distribution.len()))
        .collect();
    components.sort_by_key(|(_, (_, high), _)| std::cmp::Reverse(*high));
    components
        .into_iter()
        .take(COMPONENT_ROWS)
        .map(|(component, (low, high), count)| {
            let ms = |nanos: u64, per: f64| nanos as f64 / NANOS_PER_MS / per;
            format!(
                "component {component}: {:.4}..{:.4} ms/call, {:.4}..{:.4} ms mean over {count}",
                ms(low, calls),
                ms(high, calls),
                ms(low, count as f64),
                ms(high, count as f64)
            )
        })
        .collect()
}

/// Locks a tally, recovering one a panicking arrival poisoned.
fn lock(tally: &Mutex<Measured>) -> std::sync::MutexGuard<'_, Measured> {
    tally.lock().unwrap_or_else(PoisonError::into_inner)
}

/// The start lateness a paced arrival may show before it counts as late,
/// microseconds: L1's send-lateness p99 limit.
const LATE_US: u64 = 5_000;

/// Records where a paced window's late arrivals fell and what was running
/// when they were due. `arrivals_late` counts the arrivals that started more
/// than [`LATE_US`] behind their slot, and `arrivals_late_{first,last}_ms`
/// give the due times of the first and last of them after the window opened:
/// bunched at the start, they point at the window's setup; spread across
/// it, at a recurring pause. `arrivals_wake_p99_us` is how late callers
/// woke, before preparing their calls, so start lateness well above it is
/// the caller's own preparation. Where the driver reports its runtime's
/// garbage collections, `driver_gc_collections` and `driver_gc_max_ms`
/// describe them and `arrivals_late_in_gc` counts the late arrivals whose
/// wait overlapped one. Diagnostic only: the send-lateness p99 judges.
fn set_late(repetition: &mut Repetition, measured: &Measured) {
    let late: Vec<(u64, u64)> = measured
        .lateness_us
        .iter()
        .copied()
        .filter(|(_, late)| *late > LATE_US)
        .collect();
    repetition.set(
        "arrivals_wake_p99_us",
        Reading::percentile(&measured.woke_us, 0.99, 1.0),
    );
    match &measured.collections_us {
        None => {
            for name in [
                "driver_gc_collections",
                "driver_gc_max_ms",
                "arrivals_late_in_gc",
            ] {
                repetition.set(
                    name,
                    Reading::missing("this driver reports no garbage collections"),
                );
            }
        }
        Some(collections) => {
            let overlaps = |(due, lateness): (u64, u64)| {
                collections
                    .iter()
                    .any(|(start, duration)| *start < due + lateness && due < start + duration)
            };
            repetition.set(
                "driver_gc_collections",
                Reading::count(collections.len() as u64),
            );
            repetition.set(
                "driver_gc_max_ms",
                collections
                    .iter()
                    .map(|(_, duration)| *duration)
                    .max()
                    .map_or_else(
                        || Reading::missing("no collection ran in the window"),
                        |duration| Reading::exact(duration as f64 / 1_000.0),
                    ),
            );
            repetition.set(
                "arrivals_late_in_gc",
                Reading::count(
                    late.iter()
                        .copied()
                        .filter(|arrival| overlaps(*arrival))
                        .count() as u64,
                ),
            );
        }
    }
    let late: Vec<u64> = late.into_iter().map(|(due, _)| due).collect();
    repetition.set("arrivals_late", Reading::count(late.len() as u64));
    for (name, due) in [
        ("arrivals_late_first_ms", late.iter().min()),
        ("arrivals_late_last_ms", late.iter().max()),
    ] {
        repetition.set(
            name,
            due.map_or_else(
                || Reading::missing("no arrival was late"),
                |due| Reading::exact(*due as f64 / 1_000.0),
            ),
        );
    }
}

/// Microseconds since `since`.
fn micros(since: Instant) -> u64 {
    u64::try_from(since.elapsed().as_micros()).unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests {
    /// A paced window's slots fall `1 / rate` apart across its clients, each
    /// client's calls `clients / rate` apart, and the window schedules every
    /// slot due before it ends.
    ///
    /// # Panics
    ///
    /// Panics when a slot is due at the wrong time or the count is wrong.
    #[test]
    fn paced_slots_spread_the_rate_across_clients() {
        let start = tokio::time::Instant::now();
        let pace = Pace {
            start,
            clients: 4,
            rate: 1_000,
        };
        assert_eq!(pace.due(0, 0), start);
        assert_eq!(pace.due(1, 0) - start, Duration::from_millis(1));
        assert_eq!(pace.due(3, 0) - start, Duration::from_millis(3));
        assert_eq!(pace.due(0, 1) - start, Duration::from_millis(4));
        assert_eq!(pace.due(2, 5) - start, Duration::from_millis(22));
        assert_eq!(pace.scheduled(Duration::from_secs(5)), 5_000);
        assert_eq!(pace.scheduled(Duration::from_micros(2_500)), 3);
    }

    /// A paced window reports how many arrivals started more than 5 ms late,
    /// when the first and last of them were due, and how many of them waited
    /// through a reported garbage collection; a punctual window reports none
    /// and no due times, and a driver reporting no collections leaves the
    /// overlap unmeasured.
    ///
    /// # Panics
    ///
    /// Panics when a late-arrival reading is wrong.
    #[test]
    fn late_arrivals_report_count_and_due_span() {
        // (due after the window opened, start lateness), microseconds.
        let measured = Measured {
            lateness_us: vec![
                (0, 13_000),
                (1_000, 5_001),
                (2_000, 5_000),
                (3_000, 200),
                (900_000, 7_000),
            ],
            // (start after the window opened, duration), microseconds. The
            // first collection, 0.5-2.5 ms, overlaps the waits of the late
            // arrivals due at 0 and 1 ms; the second, at 500 ms, overlaps
            // none, so the late arrival due at 900 ms waited through none.
            collections_us: Some(vec![(500, 2_000), (500_000, 300)]),
            ..Measured::default()
        };
        let mut window = Repetition::default();
        super::set_late(&mut window, &measured);
        assert_eq!(window.reading("arrivals_late").value, Some(3.0));
        assert_eq!(window.reading("arrivals_late_first_ms").value, Some(0.0));
        assert_eq!(window.reading("arrivals_late_last_ms").value, Some(900.0));
        assert_eq!(window.reading("driver_gc_collections").value, Some(2.0));
        assert_eq!(window.reading("driver_gc_max_ms").value, Some(2.0));
        assert_eq!(window.reading("arrivals_late_in_gc").value, Some(2.0));

        let punctual = Measured {
            lateness_us: vec![(0, 100), (1_000, 4_999)],
            ..Measured::default()
        };
        let mut punctual_window = Repetition::default();
        super::set_late(&mut punctual_window, &punctual);
        assert_eq!(punctual_window.reading("arrivals_late").value, Some(0.0));
        assert_eq!(
            punctual_window.reading("arrivals_late_first_ms").value,
            None
        );
        assert_eq!(punctual_window.reading("arrivals_late_last_ms").value, None);
        assert_eq!(punctual_window.reading("arrivals_late_in_gc").value, None);
    }

    use std::collections::BTreeMap;

    use std::time::Duration;

    use std::sync::Arc;
    use std::sync::atomic::{AtomicU64, Ordering};

    use wyrd_spec::error::WyrdError;

    use super::{ADMISSION_REFUSAL, Measured, Op, Pace, Repetition, Sample, admission, arrivals};
    use crate::verification::Schedule;

    /// The answer the provider gave `conversations` for one turn of canonical
    /// Agent `agent` on arrival `sequence`, with the tool result `tool` when
    /// the turn follows a `code.run`.
    ///
    /// # Panics
    ///
    /// Panics when the provider refuses the turn or answers something other
    /// than a completion.
    fn turn(
        conversations: &crate::verification::workload::Conversations,
        agent: &str,
        sequence: u64,
        tool: Option<serde_json::Value>,
    ) -> serde_json::Value {
        let mut messages = vec![
            serde_json::json!({ "role": "user", "content": format!("BENCH-AGENT {agent} {sequence}") }),
        ];
        messages.extend(tool);
        let body = serde_json::json!({ "model": "gpt-4o", "messages": messages });
        let answer = conversations
            .reply(body.to_string().as_bytes())
            .expect("the provider answers a chat request");
        serde_json::from_slice::<serde_json::Value>(&answer).expect("a JSON completion")["choices"]
            [0]["message"]
            .clone()
    }

    /// The tool message carrying `printed` as the output of the provider's
    /// `code.run` call `id`.
    fn printed(id: &str, printed: &serde_json::Value) -> serde_json::Value {
        serde_json::json!({
            "role": "tool",
            "tool_call_id": id,
            "content": serde_json::json!({ "output": format!("{printed}\n"), "value": null }).to_string(),
        })
    }

    /// A compact canonical result of arrival `sequence` judging the Assert
    /// `asserted` and each Agent of `agents` passed with `data`, citing
    /// `slot`.
    ///
    /// # Panics
    ///
    /// Panics when the shape no longer decodes as a result.
    fn canonical(
        sequence: u64,
        asserted: &str,
        agents: &[&str],
        slot: &str,
        data: impl Fn(&str) -> serde_json::Value,
    ) -> wyrd_spec::verification::execution::VerificationResult {
        let card = |kind: &str, name: &str| serde_json::json!({ "kind": kind, "name": name, "version": "1.0.0" });
        let mut requirements = vec![serde_json::json!({
            "id": "asserted", "statement": "d is yes.", "outcome": asserted,
            "reason": "assert", "citations": [],
        })];
        requirements.extend(agents.iter().map(|agent| {
            serde_json::json!({
                "id": agent, "statement": "reads", "outcome": "passed", "reason": "read",
                "citations": [{ "slot": slot, "locator": { "json_pointer": "/sequence" } }],
                "data": data(agent),
            })
        }));
        let verdict = if asserted == "passed" {
            "passed"
        } else {
            "failed"
        };
        serde_json::from_value(serde_json::json!({
            "execution_id": "0190a1b2-c3d4-7e5f-8a9b-0c1d2e3f4a5b",
            "verifier": card("Verifier", "bench-canonical"),
            "subject": card("Model", "bench-model"),
            "outcome": { "verdict": verdict, "requirements": requirements },
        }))
        .unwrap_or_else(|error| panic!("arrival {sequence}: {error}"))
    }

    /// A canonical call's client samples hold all of its elapsed work: a
    /// delay anywhere inside the public call (provider, sandbox, pool,
    /// host, or decode) stays in the call-return and result-ready totals,
    /// and deferred result conversion stays in result-ready only, while
    /// fixture preparation before the call and judgment after it stay
    /// outside both. A refusal is an error, never a sample, and a missing,
    /// unrepresentable, or inverted sample makes the timing invalid rather
    /// than being dropped or clamped.
    ///
    /// # Panics
    ///
    /// Panics when a delay leaves its total, outside work enters one, or an
    /// impossible sample passes the integrity check.
    #[tokio::test(start_paused = true)]
    async fn canonical_client_latency_includes_all_work() {
        let delayed = |ms| tokio::time::sleep(Duration::from_millis(ms));
        delayed(50).await;
        let (returned, sample) = Sample::take(
            async {
                delayed(7).await;
                Ok::<_, WyrdError>(3_u64)
            },
            async |_: &u64| delayed(3).await,
        )
        .await;
        delayed(40).await;
        assert_eq!(returned.ok(), Some(3));
        assert_eq!(
            sample,
            Sample {
                returned_us: 7_000,
                ready_us: 10_000,
            },
            "the call's own delay is in both totals, deferred conversion only in result-ready, and preparation and judgment in neither"
        );
        let (refused, after) = Sample::take(
            async {
                delayed(4).await;
                Err::<u64, _>(WyrdError::internal("refused"))
            },
            async |_: &u64| delayed(3).await,
        )
        .await;
        assert!(refused.is_err());
        assert_eq!(
            after.ready_us, after.returned_us,
            "a refusal converts nothing"
        );

        let mut measured = Measured::default();
        let result = canonical(
            0,
            "failed",
            &crate::verification::workload::AGENTS,
            "context",
            |agent| serde_json::json!({ "agent": agent, "sequence": "0" }),
        );
        measured.judged(Op::Canonical, 0, sample, Ok(result), false);
        measured.judged(
            Op::Canonical,
            1,
            after,
            Err(WyrdError::internal("refused")),
            false,
        );
        assert_eq!(measured.latencies_us, [7_000]);
        assert_eq!(measured.ready_us, [10_000]);
        assert_eq!(measured.answered(), 2);
        assert_eq!(measured.integrity(), None);
        for (broken, reason) in [
            (
                Measured {
                    ready_us: Vec::new(),
                    ..measured.clone()
                },
                "missing",
            ),
            (
                Measured {
                    latencies_us: vec![u64::MAX],
                    ready_us: vec![u64::MAX],
                    ..measured.clone()
                },
                "unrepresentable",
            ),
            (
                Measured {
                    ready_us: vec![6_000],
                    ..measured.clone()
                },
                "before it returned",
            ),
        ] {
            let found = broken
                .integrity()
                .unwrap_or_else(|| panic!("{reason} passes"));
            assert!(found.contains(reason), "{reason}: {found}");
        }
    }

    /// A profiled run's language drivers are its client roles, and each
    /// step's heap captures say what they leave out: a driver whose native
    /// module is no `bench` build saves its managed heap only, and a driver
    /// that exits before answering is a failed capture rather than a missing
    /// line.
    ///
    /// # Panics
    ///
    /// Panics when a role, capture, or note is wrong.
    #[tokio::test]
    async fn client_captures_name_their_gaps() {
        use wyrd_testing::experiment::{Level, Role, Workload};
        use wyrd_testing::profile::ProfileMode;

        use super::Bench;
        use crate::verification::driver::{ForeignDriver, Language};

        let script = |language: &str, native: bool| {
            let stem = format!("heap-{language}-verify");
            let mut paths = vec![format!("{stem}.managed")];
            if native {
                paths.push(format!("{stem}.pb.gz"));
            }
            let answer = serde_json::json!({ "heap": { "paths": paths } });
            let mut command = tokio::process::Command::new("sh");
            command.args([
                "-c",
                &format!("read config; echo '{{\"ready\":{{}}}}'; read heap; echo '{answer}'"),
            ]);
            command
        };
        let directory = tempfile::tempdir().expect("directory");
        let mut bench = Bench::new(
            Level::One,
            directory.path().join("logs"),
            false,
            Some(ProfileMode::Both),
        );
        let mut start = async |language: Language, native| {
            let driver =
                ForeignDriver::spawn(script(language.name(), native), &serde_json::json!({}), 1)
                    .await
                    .expect("the driver starts");
            let pid = driver.pid;
            bench.drivers.insert(language, driver);
            pid
        };
        let python = start(Language::Python, true).await;
        let typescript = start(Language::TypeScript, false).await;
        assert_eq!(
            bench.clients(),
            [(Role::Python, python), (Role::TypeScript, typescript)]
        );

        let note = bench.capture_clients("verify", directory.path()).await;
        assert_eq!(
            note.captures,
            [
                "heap-python-verify.managed",
                "heap-python-verify.pb.gz",
                "heap-typescript-verify.managed",
            ]
        );
        assert_eq!(
            note.notes,
            [
                "heap-typescript-verify holds the managed heap only: the typescript native module is not a `bench` build, so nothing samples its allocations",
            ]
        );

        let note = bench.capture_clients("verify", directory.path()).await;
        assert!(
            note.captures.is_empty(),
            "both drivers exited after their captures"
        );
        assert!(
            note.notes.iter().all(|n| n.contains("failed")) && note.notes.len() == 2,
            "{note:?}"
        );
    }

    /// A language driver's reported calls keep the same populations as
    /// Rust's: whole microseconds of its own clock for both samples, a
    /// refusal counted by its stable code and never sampled, a late call
    /// still sampled and counted late, an outcome judged after the window
    /// exactly as a Rust result is, and a report with neither an outcome
    /// nor a code refused rather than acknowledged; an inverted sample
    /// still fails the integrity check.
    ///
    /// # Panics
    ///
    /// Panics when a reported call lands in the wrong population or an
    /// impossible sample passes.
    #[test]
    fn language_drivers_preserve_public_boundaries_and_counts() {
        use crate::verification::workload::AGENTS;

        let own = |sequence: u64| move |agent: &str| serde_json::json!({ "agent": agent, "sequence": sequence.to_string() });
        let call = |sequence: u64,
                    returned_ns,
                    ready_ns,
                    late,
                    outcome: Option<serde_json::Value>,
                    error: Option<&str>| {
            serde_json::from_value::<super::Call>(serde_json::json!({
                "sequence": sequence, "returned_ns": returned_ns, "ready_ns": ready_ns,
                "late": late, "error": error,
                "execution_id": outcome.as_ref().map(|_| "0190a1b2-c3d4-7e5f-8a9b-0c1d2e3f4a5b"),
                "outcome": outcome,
            }))
            .expect("a driver report")
        };
        let outcome = |sequence, asserted, agents: &[&str]| {
            serde_json::to_value(
                canonical(sequence, asserted, agents, "context", own(sequence)).outcome,
            )
            .expect("an outcome")
        };
        let mut measured = Measured::default();
        measured.reported(
            Op::Canonical,
            call(
                0,
                4_999_999,
                6_000_500,
                false,
                Some(outcome(0, "passed", &AGENTS)),
                None,
            ),
        );
        measured.reported(
            Op::Canonical,
            call(
                1,
                9_000_000,
                12_000_000,
                true,
                Some(outcome(1, "failed", &AGENTS)),
                None,
            ),
        );
        measured.reported(
            Op::Canonical,
            call(
                2,
                8_000_000,
                8_000_000,
                false,
                Some(outcome(2, "passed", &AGENTS[..1])),
                None,
            ),
        );
        measured.reported(
            Op::Canonical,
            call(
                3,
                700_000,
                700_000,
                false,
                None,
                Some("WYRD_SERVER_503_SERVICE_UNAVAILABLE"),
            ),
        );
        measured.reported(Op::Canonical, call(4, 100_000, 100_000, false, None, None));

        assert_eq!(
            measured.latencies_us,
            [4_999, 9_000, 8_000],
            "whole microseconds, rounded down"
        );
        assert_eq!(measured.ready_us, [6_000, 12_000, 8_000]);
        assert_eq!(measured.sequences, [0, 1, 2]);
        assert_eq!(
            measured.wrong, 1,
            "an Agent missing from the outcome is wrong"
        );
        assert_eq!(measured.late, 1);
        assert_eq!(
            measured.errors,
            BTreeMap::from([
                ("WYRD_SERVER_503_SERVICE_UNAVAILABLE".to_owned(), 1),
                (super::INCOMPLETE_REPORT.to_owned(), 1),
            ])
        );
        assert_eq!(measured.answered(), 5);
        assert_eq!(measured.integrity(), None);

        measured.reported(
            Op::Canonical,
            call(
                5,
                3_000_000,
                2_000_000,
                false,
                Some(outcome(5, "failed", &AGENTS)),
                None,
            ),
        );
        let found = measured.integrity().expect("an inverted sample fails");
        assert!(found.contains("before it returned"), "{found}");
    }

    /// The canonical provider answers each Agent's first turn with one
    /// `code.run` of its own program and its second with a passing cited
    /// Verdict only for that conversation's own output, whatever order
    /// interleaved clients send them in; the ledger reconciles exactly one
    /// code turn and one own-output final turn per Agent per acknowledged
    /// arrival and counts a missing Agent, missing code output, a repeated
    /// turn, a mismatched conversation, and an unacknowledged one; and a
    /// result missing an Agent, its citation, or its own data is wrong.
    ///
    /// # Panics
    ///
    /// Panics when a turn, a reconciliation count, or a result judgment
    /// differs.
    #[test]
    fn canonical_workload_requires_all_three_checks() {
        use crate::verification::workload::{AGENTS, Conversations};

        let conversations = Conversations::default();
        let own = |agent: &str, sequence: u64| serde_json::json!({ "agent": agent, "sequence": sequence.to_string() });
        for sequence in [1, 2] {
            for agent in AGENTS {
                let call = turn(&conversations, agent, sequence, None);
                let [code] = call["tool_calls"]
                    .as_array()
                    .expect("one tool call")
                    .as_slice()
                else {
                    panic!("{agent} {sequence}: one call: {call}");
                };
                assert_eq!(code["function"]["name"], "code_run");
                let source: serde_json::Value = serde_json::from_str(
                    code["function"]["arguments"].as_str().expect("arguments"),
                )
                .expect("JSON arguments");
                let source = source["source"].as_str().expect("source");
                assert!(
                    source.contains("/inputs/context.json") && source.contains(agent),
                    "{source}"
                );
            }
        }
        for (agent, sequence) in [("second", 2), ("first", 1), ("second", 1), ("first", 2)] {
            let id = "call-code";
            let answered = turn(
                &conversations,
                agent,
                sequence,
                Some(printed(id, &own(agent, sequence))),
            );
            let verdict: serde_json::Value =
                serde_json::from_str(answered["content"].as_str().expect("a Verdict"))
                    .expect("JSON");
            assert_eq!(verdict["outcome"], "passed", "{agent} {sequence}");
            assert_eq!(verdict["data"], own(agent, sequence));
            assert_eq!(verdict["citations"][0]["slot"], "context");
        }
        let ledger = conversations.take();
        assert_eq!(ledger.unreconciled(&[1, 2], true), (0, Vec::new()));
        assert_eq!(
            ledger.unreconciled(&[1, 2, 3], true).0,
            1,
            "arrival 3 never conversed"
        );
        assert_eq!(
            ledger.unreconciled(&[1], true).0,
            1,
            "arrival 2 was never acknowledged"
        );
        assert_eq!(
            ledger.unreconciled(&[1], false).0,
            0,
            "a refused arrival may have conversed"
        );
        assert_eq!(
            ledger.unreconciled(&[1, 1], false).0,
            1,
            "an arrival acknowledged twice"
        );

        turn(&conversations, "first", 5, None);
        turn(&conversations, "first", 5, None);
        turn(&conversations, "second", 5, None);
        let crossed = turn(
            &conversations,
            "second",
            5,
            Some(printed("call-code", &own("second", 6))),
        );
        let bare = turn(
            &conversations,
            "first",
            5,
            Some(
                serde_json::json!({ "role": "tool", "tool_call_id": "call-code", "content": "{}" }),
            ),
        );
        for answered in [crossed, bare] {
            let verdict: serde_json::Value =
                serde_json::from_str(answered["content"].as_str().expect("a Verdict"))
                    .expect("JSON");
            assert_eq!(verdict["outcome"], "failed", "{verdict}");
        }
        let (count, problems) = conversations.take().unreconciled(&[5], true);
        assert_eq!(
            count, 3,
            "two mismatched finals and arrival 5's turns: {problems:?}"
        );
        let judge = turn(&conversations, "nobody", 7, None);
        assert!(
            judge["content"]
                .as_str()
                .is_some_and(|content| content.contains("\"answer\"")),
            "{judge}"
        );

        let canonical_op = Op::Canonical;
        assert_eq!(canonical_op.exchanges(), 4);
        assert!(canonical_op.expected(
            0,
            &canonical(0, "passed", &AGENTS, "context", |agent| own(agent, 0)).outcome
        ));
        assert!(canonical_op.expected(
            1,
            &canonical(1, "failed", &AGENTS, "context", |agent| own(agent, 1)).outcome
        ));
        for (name, wrong) in [
            (
                "missing Agent",
                canonical(0, "passed", &AGENTS[..1], "context", |agent| own(agent, 0)),
            ),
            (
                "wrong citation",
                canonical(0, "passed", &AGENTS, "answer", |agent| own(agent, 0)),
            ),
            (
                "mismatched conversation",
                canonical(0, "passed", &AGENTS, "context", |agent| own(agent, 2)),
            ),
            (
                "missing code output",
                canonical(0, "passed", &AGENTS, "context", |_| serde_json::Value::Null),
            ),
            (
                "wrong verdict",
                canonical(1, "passed", &AGENTS, "context", |agent| own(agent, 1)),
            ),
        ] {
            assert!(
                !canonical_op.expected(u64::from(name == "wrong verdict"), &wrong.outcome),
                "{name}"
            );
        }
    }

    /// Diagnostic means, acknowledgements, and components come from the
    /// benchmark recorder's snapshots: the production submission, queue-wait,
    /// and component histograms through the standard `metrics` interface,
    /// and the timed Scribe appends and direct phases through their timing
    /// parts, each observation counted once.
    ///
    /// # Panics
    /// Panics when a benchmark observation is missing, counted twice, or its
    /// bounds exclude the recorded duration.
    #[test]
    fn diagnostics_use_benchmark_distributions() {
        wyrd_benchmark::install().expect("the benchmark recorder installs");
        let before = wyrd_benchmark::snapshot();
        metrics::histogram!(super::SUBMIT, "outcome" => "staged").record(0.000_2);
        metrics::histogram!(super::SUBMIT, "outcome" => "refused").record(9.0);
        metrics::histogram!(super::QUEUE_WAIT, "origin" => "manual").record(0.000_3);
        metrics::histogram!(super::SCRIBE_ACK).record(0.000_4);
        metrics::histogram!("outbox_write_seconds", "outbox" => "scribe").record(0.000_05);
        metrics::histogram!("wyrd_verification_agent_phase_duration_seconds", "phase" => "run")
            .record(0.000_3);
        let span = tracing::Span::none();
        wyrd_benchmark::record_timing(
            &span,
            super::SCRIBE_WRITE,
            "success",
            Duration::from_micros(400),
            None,
        );
        wyrd_benchmark::record_timing_parts(
            &span,
            super::DIRECT_TIMING,
            super::COMPLETED,
            Duration::from_micros(150),
            None,
            &[("verification.direct.engine", Duration::from_micros(100))],
        );
        let population =
            wyrd_testing::capacity::Population::between(&before, wyrd_benchmark::snapshot())
                .expect("one process");
        let acks = super::acknowledgements(&population);
        assert_eq!(acks.as_ref().map(wyrd_benchmark::Distribution::len), Ok(2));
        for (distribution, expected, samples) in [
            (
                population.histogram(super::SUBMIT, &[super::STAGED]),
                0.2,
                1,
            ),
            (
                population.histogram(super::QUEUE_WAIT, &[super::SUBMITTED]),
                0.3,
                1,
            ),
            (acks, 0.4, 2),
        ] {
            let reading = super::mean_ms(distribution);
            assert!(reading.above.is_some_and(|low| low < expected));
            assert!(reading.value.is_some_and(|high| expected <= high));
            assert_eq!(reading.samples, Some(samples));
        }
        assert!(super::observed_components(&population));
        let components = super::components(&population, 1);
        assert_eq!(components.len(), 3, "{components:?}");
        assert!(components[0].starts_with("component agent_phase_duration{phase=run}:"));
        assert!(components[1].starts_with("component verification.direct.engine:"));
        assert!(components[2].starts_with("component outbox_write{outbox=scribe}:"));
        assert!(
            !super::observed_components(&wyrd_testing::capacity::Population::default()),
            "an empty window has no component"
        );
    }

    /// A judge step reports each recorded Gateway phase and wait under its
    /// own `gateway.phase.*` or `gateway.wait.*` label with its actual
    /// observation count, from the server's histograms alone; an assertion
    /// step marks every one inapplicable; and a judge window without a
    /// recorded phase is invalid rather than zero.
    ///
    /// # Panics
    /// Panics when a label, count, or inapplicable reason is wrong.
    #[test]
    fn diagnostics_include_gateway_phases_and_waits() {
        use wyrd_testing::experiment::{DIAGNOSTIC, Repetition};
        wyrd_benchmark::install().expect("the benchmark recorder installs");
        let before = wyrd_benchmark::snapshot();
        for (calls, phase) in (1_u32..).zip(super::GATEWAY_PHASES) {
            for _ in 0..calls {
                metrics::histogram!(super::PHASE_SECONDS, "operation" => "chat", "phase" => phase)
                    .record(0.000_01);
            }
        }
        for wait in super::WAITS {
            metrics::histogram!(super::WAIT_SECONDS, "operation" => "chat", "wait" => wait)
                .record(0.000_02);
        }
        let population =
            wyrd_testing::capacity::Population::between(&before, wyrd_benchmark::snapshot())
                .expect("one process");
        let mut judge = Repetition::default();
        super::set_gateway(&mut judge, &population, Op::Judge, true);
        assert!(judge.invalid.is_empty(), "{:?}", judge.invalid);
        for (calls, phase) in (1_u64..).zip(super::GATEWAY_PHASES) {
            let count = judge.reading(&format!("{DIAGNOSTIC}gateway.phase.{phase}.count"));
            assert_eq!(count.value, Some(calls as f64), "{phase}");
        }
        for wait in super::WAITS {
            let mean = judge.reading(&format!("{DIAGNOSTIC}gateway.wait.{wait}.mean_us"));
            assert!(
                mean.value.is_some_and(|high| 20.0 <= high),
                "{wait}: {mean:?}"
            );
            assert!(mean.above.is_some_and(|low| low < 20.0), "{wait}: {mean:?}");
        }
        let mut assertion = Repetition::default();
        super::set_gateway(&mut assertion, &population, Op::Verify, true);
        let reason = assertion
            .reading(&format!("{DIAGNOSTIC}gateway.phase.admit.count"))
            .reason;
        assert_eq!(
            reason.as_deref(),
            Some("inapplicable: an assertion check calls no provider")
        );
        let mut empty = Repetition::default();
        let none = wyrd_testing::capacity::Population::default();
        super::set_gateway(&mut empty, &none, Op::Judge, true);
        assert_eq!(
            empty.invalid.len(),
            super::GATEWAY_PHASES.len() + super::WAITS.len()
        );
    }

    /// A model step drives the judge under its own case while assertion
    /// steps keep theirs and their alternating expectations; the provider's
    /// answer is a passing Verdict citing the judge's slot with fixed usage;
    /// and a direct execution's Verifier block, which holds the model call,
    /// stays out of its overhead residual.
    ///
    /// # Panics
    ///
    /// Panics when a model step is driven as an assertion, the fixture's
    /// answer is not a valid cited Verdict, or the provider time reaches the
    /// residual.
    #[test]
    fn model_results_keep_provider_exclusion_and_expected_outcomes() {
        use wyrd_testing::experiment::Step;

        let model = Step::measured(
            "direct-verify-model-c1",
            serde_json::json!({ "clients": 1, "op": "verify", "check": "model" }),
        );
        let assertion = Step::measured(
            "direct-verify-c1",
            serde_json::json!({ "clients": 1, "op": "verify" }),
        );
        assert_eq!(Op::direct(&model), Op::Judge);
        assert_eq!(Op::direct(&model).case(), super::MODEL_CASE);
        assert_eq!(Op::direct(&assertion), Op::Verify);
        assert_eq!(Op::direct(&assertion).case(), super::DIRECT_CASE);
        let expected: Vec<bool> = (0..4).map(crate::verification::workload::failing).collect();
        assert_eq!(expected, [false, true, false, true]);

        let completion = wyrd_testing::gateway_capture::GatewayCapture::verdict_completion(
            crate::verification::workload::JUDGE_SLOT,
        );
        let content = completion["choices"][0]["message"]["content"]
            .as_str()
            .expect("the answer is text");
        let verdict: wyrd_spec::verification::execution::Verdict =
            serde_json::from_str(content).expect("the answer is a Verdict");
        assert_eq!(
            verdict.outcome,
            wyrd_spec::verification::VerificationVerdict::Passed
        );
        assert_eq!(
            verdict
                .citations
                .iter()
                .map(|citation| citation.slot.as_str())
                .collect::<Vec<_>>(),
            [crate::verification::workload::JUDGE_SLOT]
        );
        assert_eq!(completion["usage"]["total_tokens"], 15);

        wyrd_benchmark::install().expect("the benchmark recorder installs");
        let before = wyrd_benchmark::snapshot();
        wyrd_benchmark::record_timing_parts(
            &tracing::Span::none(),
            super::DIRECT_TIMING,
            super::COMPLETED,
            Duration::from_micros(1_000),
            Some(Duration::from_micros(900)),
            &[("verification.direct.engine", Duration::from_micros(950))],
        );
        let population =
            wyrd_testing::capacity::Population::between(&before, wyrd_benchmark::snapshot())
                .expect("one process");
        let residual = super::mean_ms(population.residuals(super::DIRECT_TIMING, super::COMPLETED));
        assert!(
            residual.value.is_some_and(|high| high < 0.9),
            "the provider's block is excluded: {residual:?}"
        );
        assert!(residual.above.is_some_and(|low| low < 0.1));
    }

    /// Once a stalled call carries the clock past the window end, every
    /// remaining due arrival is missed rather than admitted after the
    /// measurement stopped, the planned count is kept, and the earlier
    /// admission still answers into its tally.
    ///
    /// # Panics
    ///
    /// Panics when an arrival is admitted after the end or one is lost.
    #[tokio::test(start_paused = true)]
    async fn arrivals_waking_after_the_end_are_missed() {
        let schedule = Schedule {
            window: Duration::from_secs(12),
            direct_rate: 1.0,
            submit_rate: 1.0,
            overload_factor: 2.0,
        };
        let tallies: Arc<[[std::sync::Mutex<Measured>; 2]; 3]> = Arc::default();
        let admitted: Arc<[AtomicU64; 3]> = Arc::default();
        let offered = arrivals(
            Op::Verify,
            schedule,
            tokio::time::Instant::now(),
            |next| {
                Some(async move {
                    if next == 0 {
                        tokio::time::advance(Duration::from_secs(13)).await;
                    }
                    Ok(true)
                })
            },
            Arc::clone(&tallies),
            Arc::clone(&admitted),
            Arc::new(AtomicU64::new(0)),
        )
        .await;
        assert_eq!((offered.scheduled, offered.sent), (14, 1));
        assert_eq!(offered.missed, 13);
        let answered: u64 = admitted
            .iter()
            .map(|count| count.load(Ordering::SeqCst))
            .sum();
        assert_eq!(answered, 1);
    }

    /// Dropping the scheduler mid-window, as a budget overrun or an
    /// interrupt drops a repetition, admits nothing more and aborts every
    /// call it sent, so no tally changes after the stop.
    ///
    /// # Panics
    ///
    /// Panics when an arrival is admitted after the stop or a call outlives
    /// it.
    #[tokio::test(start_paused = true)]
    async fn cancelled_arrivals_admit_nothing_and_abort_their_calls() {
        let schedule = Schedule {
            window: Duration::from_secs(12),
            direct_rate: 1.0,
            submit_rate: 1.0,
            overload_factor: 2.0,
        };
        let sequence = Arc::new(AtomicU64::new(0));
        let live = Arc::new(());
        let driving = arrivals(
            Op::Verify,
            schedule,
            tokio::time::Instant::now(),
            |_| {
                let held = Arc::clone(&live);
                Some(async move {
                    std::future::pending::<()>().await;
                    drop(held);
                    Ok(true)
                })
            },
            Arc::default(),
            Arc::default(),
            Arc::clone(&sequence),
        );
        let cut = tokio::time::timeout(Duration::from_millis(2_500), driving).await;
        assert!(cut.is_err(), "the schedule outlasts the cut");
        tokio::time::sleep(Duration::from_secs(20)).await;
        assert_eq!(
            sequence.load(Ordering::SeqCst),
            3,
            "arrivals at 0, 1, and 2 s only"
        );
        assert_eq!(Arc::strong_count(&live), 1, "no call outlives the stop");
    }

    /// Arrivals keep their schedule phase's cohort except recovery's first
    /// 30 s (half of a recovery shorter than 60 s), which join overload's,
    /// so recovery is judged after its grace however late each call
    /// completes.
    ///
    /// # Panics
    ///
    /// Panics when an arrival lands in the wrong cohort.
    #[test]
    fn recovery_grace_arrivals_join_overload() {
        let recovery = Duration::from_secs(100);
        let second = Duration::from_secs(1);
        let length = second * 100;
        assert_eq!(admission(recovery, length, 0, second), 0);
        assert_eq!(admission(recovery, length, 1, second * 99), 1);
        assert_eq!(admission(recovery, length, 2, recovery + second * 29), 1);
        assert_eq!(admission(recovery, length, 2, recovery + second * 30), 2);
        let short = second * 15;
        assert_eq!(admission(recovery, short, 2, recovery + second * 7), 1);
        assert_eq!(admission(recovery, short, 2, recovery + second * 8), 2);
    }

    /// Answers count acknowledged and refused calls, only undeclared
    /// refusals are unexpected, and an acknowledgement after the window end
    /// stays answered while it is counted late; merging keeps every count.
    ///
    /// # Panics
    ///
    /// Panics when a tally miscounts.
    #[test]
    fn tallies_separate_declared_admission_refusals() {
        let mut measured = Measured {
            latencies_us: vec![100, 200],
            errors: BTreeMap::from([
                (ADMISSION_REFUSAL.to_owned(), 3),
                ("WYRD_OTHER".to_owned(), 1),
            ]),
            wrong: 1,
            late: 0,
            ..Measured::default()
        };
        assert_eq!(measured.answered(), 6);
        assert_eq!(measured.unexpected(Some(ADMISSION_REFUSAL)), 1);
        assert_eq!(measured.unexpected(None), 4);
        measured.merge(Measured {
            latencies_us: vec![300],
            errors: BTreeMap::new(),
            wrong: 2,
            late: 1,
            ..Measured::default()
        });
        measured.record(400, Ok(true), true);
        measured.record(50, Ok(true), false);
        assert_eq!(measured.wrong, 3);
        assert_eq!((measured.late, measured.answered()), (2, 9));
        assert_eq!(measured.percentile(0.5).value, Some(0.2));
        assert_eq!(measured.unexpected(None), 4);
    }

    /// Worker CPU counts every worker child's CPU inside the window, whether
    /// it exited, started, or ran throughout; a missing counter stays
    /// unavailable; and the parent-only server CPU keeps its scope in new and
    /// historical records.
    ///
    /// # Panics
    /// Panics when worker CPU is attributed outside the window, a missing
    /// counter is reported as a value, or a record loses its server CPU scope.
    #[test]
    fn worker_process_resources_preserve_measurement_scope() {
        use wyrd_testing::capacity::worker_cpu_seconds;
        use wyrd_testing::experiment::Repetition;
        use wyrd_testing::release_server::Metrics;

        // Worker `a` used 2 s before the window and 0.5 s in it, then exited
        // and was reaped; `b` started in the window and used 0.25 s; `c`
        // used 1 s before and 0.75 s in it. Earlier reaped workers used 10 s.
        let reaped = [Some(10.0), Some(10.0 + 2.5)];
        let live = [Some(2_000_000 + 1_000_000), Some(250_000 + 1_750_000)];
        assert_eq!(worker_cpu_seconds(reaped, live), Some(0.5 + 0.25 + 0.75));

        for (reaped, live) in [
            ([None, Some(12.5)], live),
            ([Some(10.0), None], live),
            (reaped, [None, Some(2_000_000)]),
            (reaped, [Some(3_000_000), None]),
            ([Some(12.5), Some(10.0)], live),
        ] {
            assert_eq!(
                worker_cpu_seconds(reaped, live),
                None,
                "{reaped:?} {live:?}"
            );
        }

        let mut measured = Repetition {
            server_cpu_seconds: Some(2.0),
            ..Repetition::default()
        };
        super::set_workers(&mut measured, Some(1.5), Some(64 << 20));
        assert_eq!(measured.server_cpu_seconds, Some(2.0));
        assert_eq!(measured.reading("worker_cpu_seconds").value, Some(1.5));
        assert_eq!(
            measured.reading("worker_peak_memory_bytes").value,
            Some(f64::from(64 << 20))
        );

        let mut unavailable = Repetition::default();
        super::set_workers(&mut unavailable, None, None);
        for id in ["worker_cpu_seconds", "worker_peak_memory_bytes"] {
            let reading = unavailable.reading(id);
            assert_eq!(reading.value, None, "{id} is never zero");
            assert!(reading.reason.is_some(), "{id} says why it is missing");
        }

        let mut historical = serde_json::to_value(Repetition {
            server_cpu_seconds: Some(2.0),
            ..Repetition::default()
        })
        .expect("a repetition serializes");
        historical["metrics"] = serde_json::json!({});
        let historical: Repetition =
            serde_json::from_value(historical).expect("a parent-only record reads");
        assert_eq!(historical.server_cpu_seconds, Some(2.0));
        assert_eq!(historical.reading("worker_cpu_seconds").value, None);

        let family = wyrd_server::app::metrics::VERIFICATION_PYTHON_WORKER_EVENTS_TOTAL;
        let opened = super::worker_events(&Metrics::parse(&format!(
            "{family}{{event=\"spawn\"}} 4\n{family}{{event=\"reset\"}} 10\n"
        )));
        let closed = super::worker_events(&Metrics::parse(&format!(
            "{family}{{event=\"spawn\"}} 6\n{family}{{event=\"reset\"}} 25\n{family}{{event=\"reap\"}} 1\n"
        )));
        super::set_worker_churn(&mut measured, &opened, &closed);
        for (id, count) in [
            ("worker_spawns", 2.0),
            ("worker_resets", 15.0),
            ("worker_reaps", 1.0),
        ] {
            assert_eq!(measured.reading(id).value, Some(count), "{id}");
        }
        super::set_worker_churn(&mut measured, &closed, &opened);
        assert_eq!(
            measured.reading("worker_resets").value,
            None,
            "a reset total is never negative churn"
        );
    }

    /// The measurement path calibrated against independent references on a
    /// real release server: the shared runner, the actual [`Window`] and
    /// resource readers, the server's timing readout and the persisted record,
    /// compared with the same workload with benchmark collection turned off.
    mod calibration {
        use std::time::Duration;

        use async_trait::async_trait;
        use tokio::time::Instant;
        use tokio_util::sync::CancellationToken;
        use wyrd_testing::capacity::{ResourceWindow, release_binary};
        use wyrd_testing::experiment::{
            ExperimentError, Level, Options, Phase, Record, Repetition, Step, Timing, Verdict,
            Workload, run,
        };
        use wyrd_testing::release_server::HostLoad;

        use crate::tests::pg_tests::burn;
        use crate::verification::declare;
        use crate::verification::run::{Bench, Op, drive};
        use crate::verification::tests::base;

        /// The step calibrated: one closed-loop client.
        const STEP: &str = "direct-verify-c1";

        /// Each measured window, the known delay the record's seconds are read
        /// against.
        const WINDOW: Duration = Duration::from_secs(5);

        /// The window's stop fires on a millisecond timer before the stop
        /// scrape starts; 50 ms covers that and scheduling on a quiet host.
        const WINDOW_SLACK_SECONDS: f64 = 0.05;

        /// The known CPU work the second repetition's driver adds, on its own
        /// thread, inside the window.
        const SPIN: Duration = Duration::from_secs(1);

        /// A thread spinning on a quiet host is charged at least this share of
        /// its wall time; the spin starts a few milliseconds before the window
        /// opens.
        const SPIN_FLOOR: f64 = 0.8;

        /// Two 10 ms CPU accounting ticks, one at each end of a reading.
        const TICKS_SECONDS: f64 = 0.02;

        /// How much the driver's own request CPU may differ between two equal
        /// windows, as a share of it.
        const DRIVER_DRIFT: f64 = 0.25;

        /// Allowed observation cost per verification, as a share of the
        /// uncollected control: recording adds one label read and two histogram
        /// records to a request costing hundreds of microseconds, and 10% covers
        /// the difference between two sequential windows on a quiet host.
        const COST_SHARE: f64 = 0.10;

        /// Allowed observation cost per verification below which a share is
        /// meaningless, milliseconds.
        const COST_FLOOR_MS: f64 = 0.01;

        /// The real L1 workload, with [`SPIN`] of driver CPU burned beside the
        /// second repetition. It starts the server inside the runner's clock
        /// and leaves it running, so the control reuses its tenant.
        struct Calibration {
            /// The Verification L1 workload.
            bench: Bench,
        }

        #[async_trait]
        impl Workload for Calibration {
            async fn prepare(&mut self) -> Result<(), ExperimentError> {
                self.bench.start().await
            }

            fn processes(&self) -> Vec<u32> {
                self.bench.processes()
            }

            fn heap_profile_url(&self) -> Option<String> {
                self.bench.heap_profile_url()
            }

            async fn measure(
                &mut self,
                step: &Step,
                phase: Phase,
                window: Duration,
            ) -> Result<Repetition, ExperimentError> {
                let spin = (phase == Phase::Repetition(1))
                    .then(|| tokio::task::spawn_blocking(|| burn(SPIN)));
                let repetition = self.bench.measure(step, phase, window).await;
                if let Some(spin) = spin {
                    spin.await.map_err(ExperimentError::workload)?;
                }
                repetition
            }

            async fn finish(&mut self) -> Result<(), ExperimentError> {
                Ok(())
            }
        }

        /// Whether `observed` lies in its reference `[low, high]`, naming
        /// `what` when it does not.
        fn within(what: &str, observed: f64, low: f64, high: f64) -> Result<(), String> {
            if (low..=high).contains(&observed) {
                Ok(())
            } else {
                Err(format!(
                    "{what} read {observed}, outside its reference [{low}, {high}]"
                ))
            }
        }

        /// One verification's wall and server CPU milliseconds over a window.
        fn per_verification(seconds: f64, cpu_seconds: f64, verifications: f64) -> [f64; 2] {
            [seconds, cpu_seconds].map(|total| total * 1e3 / verifications)
        }

        /// The release server's direct `verify` through the shared runner keeps
        /// what independent references say: each window's recorded seconds
        /// match the declared window, the driver CPU of a repetition with a
        /// known spin exceeds the plain one's by that spin, the server's p50
        /// lies under the client's own round trip and its count reconciles,
        /// and the record and its timing evidence read back. The same workload
        /// with collection off then measures what observing costs per
        /// verification in wall and server CPU, which stays within its
        /// allowance; each check rejects a materially wrong reading.
        ///
        /// Needs `mise run bench:build` and the repository Postgres wrapper on
        /// a quiet host:
        /// `RUSTFLAGS='-C force-frame-pointers=yes' CARGO_PROFILE_RELEASE_DEBUG=line-tables-only mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked --release --target-dir target/profiling -p wyrd-testing --bin capacity --run-ignored=only --no-capture -E 'test(=verification::run::tests::calibration::the_measurement_path_agrees_with_independent_references)'`.
        ///
        /// # Panics
        ///
        /// Panics when another recorder is installed, a reading falls outside
        /// its reference, the observation cost exceeds its allowance, or a
        /// check accepts a wrong reading.
        #[tokio::test]
        #[ignore = "needs the release server from `mise run bench:build`, the Postgres wrapper and a quiet host"]
        async fn the_measurement_path_agrees_with_independent_references() {
            wyrd_benchmark::install().expect("the benchmark recorder installs");
            let directory = tempfile::tempdir().expect("directory");
            let mut declaration = declare(Level::One, &[], base(Level::One)).expect("L1 declares");
            declaration.selection = Some(vec![STEP.to_owned()]);
            declaration.timing = Timing {
                warmup_seconds: 1,
                repetition_seconds: WINDOW.as_secs(),
                repetitions: 2,
                budget_seconds: 600,
                minimum_seconds: 0,
                phases: None,
            };
            let options = Options {
                artifacts: directory.path().join("artifacts"),
                profile: None,
                interrupt: CancellationToken::new(),
                setup_spent: Duration::ZERO,
            };
            let mut workload = Calibration {
                bench: Bench::new(Level::One, directory.path().join("logs"), false, None),
            };
            let record = run(
                declaration.seal(),
                &mut workload,
                HostLoad::sample,
                &options,
            )
            .await;
            let path = record.publish(directory.path()).expect("publish");
            let read = Record::read(&path).expect("read back");
            assert_eq!(read.render(), record.render());
            assert_eq!(read.verdict, Verdict::Valid, "{:?}", read.reasons);
            let step = read
                .steps
                .iter()
                .find(|step| step.id == STEP)
                .expect("calibrated step");
            let [plain, spun] = step.repetitions.as_slice() else {
                panic!("two repetitions: {:?}", step.repetitions.len());
            };
            for label in ["repetition-1", "repetition-2"] {
                let kept = options
                    .artifacts
                    .join(format!("timings/{STEP}/{label}.json"));
                assert!(kept.is_file(), "{} kept", kept.display());
            }

            let window = WINDOW.as_secs_f64();
            let seconds = |repetition: &Repetition| repetition.seconds;
            for repetition in [plain, spun] {
                within(
                    "window seconds",
                    seconds(repetition),
                    window,
                    window + WINDOW_SLACK_SECONDS,
                )
                .expect("the known delay");
            }
            let driver =
                |repetition: &Repetition| repetition.driver_cpu_seconds.expect("driver CPU");
            let spin = SPIN.as_secs_f64();
            let (spin_low, spin_high) = (
                spin * SPIN_FLOOR,
                spin + TICKS_SECONDS + DRIVER_DRIFT * driver(plain),
            );
            within(
                "spin CPU",
                driver(spun) - driver(plain),
                spin_low,
                spin_high,
            )
            .expect("the known CPU");

            let server_p50 = plain.reading("p50_ms");
            let client_p50 = plain.reading("client_p50_ms");
            let (Some(above), Some(client)) = (server_p50.above, client_p50.value) else {
                panic!("bucketed server and exact client p50: {server_p50:?} {client_p50:?}");
            };
            within("server p50 lower bound", above, f64::MIN_POSITIVE, client)
                .expect("the server's time lies inside the client's round trip");
            let verifications = plain.reading("verifications").value.expect("verifications");
            let late = plain
                .reading("verifications_after_window")
                .value
                .expect("late");
            assert!(!plain.reconciliation.is_empty(), "no cohort reconciled");
            for cohort in &plain.reconciliation {
                assert_eq!(
                    cohort.recorded as f64,
                    verifications + late,
                    "each reconciled boundary records every call once: {cohort:?}"
                );
            }

            let bench = &mut workload.bench;
            let server = bench.server.as_mut().expect("the server still runs");
            let binary = release_binary().expect("release server");
            server
                .restart(&binary, &[("WYRD_METRICS_BENCHMARK_SNAPSHOT", "0")])
                .await
                .expect("serve without collection");
            let server = &*server;
            assert!(
                server.timings().await.is_err(),
                "collection off serves no timing readout"
            );
            let states = &bench.states[..1];
            drive(
                states,
                Op::Verify,
                Instant::now() + Duration::from_secs(1),
                "warmup",
                None,
            )
            .await;
            // A server without collection serves no snapshot to scrape, so
            // the control window reads only its resources, frozen at the
            // stop before in-flight calls drain, as `Window::measure` does.
            let mut resources = ResourceWindow::open(server);
            let started = Instant::now();
            let stop = started + WINDOW;
            let (measured, seconds) = {
                let mut work =
                    std::pin::pin!(drive(states, Op::Verify, stop, "repetition-1", None));
                let early = tokio::select! {
                    measured = &mut work => Some(measured),
                    () = tokio::time::sleep_until(stop) => None,
                };
                resources.freeze(server);
                let seconds = started.elapsed().as_secs_f64();
                match early {
                    Some(measured) => (measured, seconds),
                    None => (work.await, seconds),
                }
            };
            let control_cpu = resources
                .finish(server, seconds, [None, None])
                .cpu_seconds
                .expect("control server CPU");
            assert!(measured.errors.is_empty(), "{:?}", measured.errors);
            let control = per_verification(
                seconds,
                control_cpu,
                (measured.latencies_us.len() as u64 - measured.late) as f64,
            );
            let enabled = per_verification(
                plain.seconds,
                plain.server_cpu_seconds.expect("server CPU"),
                verifications,
            );
            let mut costs = Vec::new();
            for (what, index) in [("wall", 0), ("server CPU", 1)] {
                let cost = enabled[index] - control[index];
                let allowance = (control[index] * COST_SHARE).max(COST_FLOOR_MS);
                println!(
                    "{what} per verification: {:.4} ms collected, {:.4} ms control, observation cost {cost:+.4} ms, allowance ±{allowance:.4} ms",
                    enabled[index], control[index]
                );
                costs.push((what, cost, allowance));
            }
            println!(
                "window {:.4}/{:.4} s against {window} s; spin CPU {:.4} s in [{spin_low:.4}, {spin_high:.4}]; server p50 ({above}, {:?}] ms under client p50 {client} ms; residual mean {:?} ms",
                plain.seconds,
                spun.seconds,
                driver(spun) - driver(plain),
                server_p50.value,
                plain.reading("overhead_mean_ms").value,
            );
            for (what, cost, allowance) in &costs {
                within(
                    &format!("{what} observation cost"),
                    *cost,
                    -allowance,
                    *allowance,
                )
                .expect("observing stays within its allowance");
            }

            assert!(
                within(
                    "window seconds",
                    plain.seconds * 1e3,
                    window,
                    window + WINDOW_SLACK_SECONDS
                )
                .is_err(),
                "a window read in milliseconds is rejected"
            );
            assert!(
                within("spin CPU", 0.0, spin_low, spin_high).is_err(),
                "a CPU reader that missed the spin is rejected"
            );
            assert!(
                within(
                    "server p50 lower bound",
                    client * 2.0,
                    f64::MIN_POSITIVE,
                    client
                )
                .is_err(),
                "a server time beyond the client's round trip is rejected"
            );
            let (_, _, allowance) = costs[1];
            assert!(
                within("observation cost", allowance * 2.0, -allowance, allowance).is_err(),
                "an excessive observation cost is rejected"
            );
            workload.bench.finish().await.expect("stop");
        }
    }

    /// A language step's client breakdown comes only from its driver's own
    /// reported recorder window: a complete window diagnoses every shared
    /// client and `sdk.verify` root and component with the response sizes;
    /// a window missing one component, or with fewer roots than acknowledged
    /// calls, is invalid; and a driver without a readout leaves every
    /// language reading missing, attribution unfinished, never zero.
    ///
    /// # Panics
    ///
    /// Panics when an incomplete readout renders as complete evidence.
    #[test]
    fn language_phase_breakdown_requires_complete_evidence() {
        use wyrd_benchmark::{Cohort, Observation};
        use wyrd_testing::experiment::Repetition;

        use super::{Language, client_roots, set_language_client};

        wyrd_benchmark::install().expect("the only recorder");
        let call = |skipped: Option<&str>| {
            for (root, _, components) in client_roots(Language::Python) {
                let mut observation = Observation::start(root, || Cohort::NONE);
                for component in *components {
                    if Some(*component) != skipped {
                        observation.component(component, Duration::from_micros(10));
                    }
                }
                if *root == super::HTTP_ROOT {
                    observation.bytes("response", 512);
                }
                observation.finish(wyrd_benchmark::SUCCESS);
            }
        };
        let readout = |calls: &[Option<&str>]| {
            let before = wyrd_benchmark::snapshot();
            for skipped in calls {
                call(*skipped);
            }
            (before, wyrd_benchmark::snapshot())
        };
        let id = |observation: &str| format!("diagnostic.python.{observation}.count");

        let mut complete = Repetition::default();
        set_language_client(
            &mut complete,
            Language::Python,
            Some(readout(&[None, None])),
            2,
            true,
        );
        assert!(complete.invalid.is_empty(), "{:?}", complete.invalid);
        for observation in [
            "client.verify.prepare",
            "client.http.auth",
            "client.http.attempt",
            "sdk.verify",
            "sdk.verify.input",
            "sdk.verify.core",
            "sdk.verify.output",
        ] {
            assert_eq!(
                complete.reading(&id(observation)).value,
                Some(2.0),
                "{observation}"
            );
        }
        assert_eq!(complete.reading("response_bytes_count").value, Some(2.0));
        assert!(
            complete
                .reading("diagnostic.typescript.sdk.verify.count")
                .reason
                .as_deref()
                .is_some_and(|reason| reason.starts_with("inapplicable")),
            "another language's clients are not reached"
        );

        let mut dropped = Repetition::default();
        set_language_client(
            &mut dropped,
            Language::Python,
            Some(readout(&[None, Some("core")])),
            2,
            true,
        );
        assert!(
            dropped
                .invalid
                .iter()
                .any(|reason| reason.contains("python.sdk.verify.core")),
            "a dropped phase is invalid: {:?}",
            dropped.invalid
        );

        let mut short = Repetition::default();
        set_language_client(
            &mut short,
            Language::Python,
            Some(readout(&[None])),
            2,
            true,
        );
        assert!(
            short
                .invalid
                .iter()
                .any(|reason| reason.contains("`sdk.verify`")),
            "a root recorded for fewer calls is invalid: {:?}",
            short.invalid
        );

        let mut unread = Repetition::default();
        set_language_client(&mut unread, Language::Python, None, 2, true);
        for observation in ["sdk.verify", "sdk.verify.core", "client.http"] {
            let reading = unread.reading(&id(observation));
            assert_eq!(reading.value, None, "{observation} never renders as zero");
            assert!(
                reading
                    .reason
                    .as_deref()
                    .is_some_and(|reason| !reason.starts_with("inapplicable")),
                "{observation}: {reading:?}"
            );
        }
        assert_eq!(unread.reading("response_bytes_count").value, None);
        assert!(
            unread
                .observations
                .iter()
                .any(|observation| observation.starts_with("attribution unfinished")),
            "{:?}",
            unread.observations
        );
    }
}
