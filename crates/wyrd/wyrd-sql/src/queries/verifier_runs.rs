//! Verifier run queue: enqueue, scheduling, claims, settlement, and status.
//!
//! `wyrd.verifier_runs` is the only Verifier execution queue and
//! `wyrd.operator_dispatches` the durable handoff to the Operator worker.
//! [`VerifierRunQueue`] owns the retry policy and activity window every
//! transition applies, and each tenant operation runs on the caller's
//! [`TenantConn`] under forced RLS without committing, so a handler can compose
//! enqueue with its audit append and the scheduler can create a run and advance
//! its cursor in one transaction. Cross-tenant work discovery and queue-depth
//! telemetry are explicit [`OperatorPool`] reads.
//!
//! Lifecycle: `pending` → (claim) `running` → `completed` | `retrying` →
//! `running` … | `errored` | `timed_out` | `cancelled`. A claim mints a fresh
//! lease token; every settlement is fenced on it, so a worker whose lease was
//! reclaimed affects zero rows and observes [`Settlement::StaleLease`].
// raw-query grep allowlist: verifier run tables post-date the sqlx offline cache; run `mise run sqlx:prepare` to promote to macros.

use std::error::Error as StdError;

use chrono::{DateTime, Duration, Utc};
use serde_json::Value;
use sqlx::Error as SqlxError;
use sqlx::types::{Json, Uuid};
use wyrd_runtime::principal::PrincipalId;
use wyrd_spec::DataTenantId;
use wyrd_spec::ids::{
    BindingId, CardUid, OperatorDispatchId, VerificationResultId, VerificationRunId,
};
use wyrd_spec::verification::{
    DriftWindow, FrozenTarget, OperatorDispatchState, VerificationBindingStatus, VerificationError,
    VerificationRunStatus, VerificationRunTarget, VerificationVerdict, VerifierReadiness,
};

use crate::queries::verification::{BindingSchedule, InactivityTimeout, binding_activity};
use crate::{OperatorPool, TenantConn};

/// Resolve a binding target's frozen identities, readiness, and implementation.
const RESOLVE_BINDING_SQL: &str = r#"
    SELECT b.verifier_uid, v.version AS verifier_version,
           v.spec #>> '{implementation,kind}' AS implementation,
           wyrd.verifier_readiness(b.verifier_uid) AS readiness,
           b.subject_card_uid,
           EXISTS (SELECT 1 FROM wyrd.cards s
                    WHERE s.card_uid = b.subject_card_uid
                      AND s.status IN ('active', 'deprecated')) AS subject_available,
           b.owner_card_uid, b.binding_id, b.trigger_uid, b.trigger_digest, b.operators
      FROM wyrd.verification_bindings b
      LEFT JOIN wyrd.cards v ON v.card_uid = b.verifier_uid
     WHERE b.binding_id = $1
"#;

/// Resolve a direct target: no owner, binding, Trigger, or Operators.
const RESOLVE_DIRECT_SQL: &str = r#"
    SELECT p.verifier_uid, v.version AS verifier_version,
           v.spec #>> '{implementation,kind}' AS implementation,
           wyrd.verifier_readiness(p.verifier_uid) AS readiness,
           $2::uuid AS subject_card_uid,
           EXISTS (SELECT 1 FROM wyrd.cards s
                    WHERE s.card_uid = $2
                      AND s.status IN ('active', 'deprecated')) AS subject_available,
           NULL::uuid AS owner_card_uid, NULL::uuid AS binding_id,
           NULL::uuid AS trigger_uid, NULL::text AS trigger_digest,
           '[]'::jsonb AS operators
      FROM (SELECT $1::uuid AS verifier_uid) p
      LEFT JOIN wyrd.cards v ON v.card_uid = p.verifier_uid
"#;

/// Insert one run, due at PostgreSQL's statement time; a duplicate scheduled
/// occurrence or observation record conflicts on its partial unique index and
/// inserts nothing.
///
/// The run becomes claimable on the same clock the claim predicate reads, so
/// no enqueueing process can write work into the queue's future or past.
const INSERT_RUN_SQL: &str = r#"
    INSERT INTO wyrd.verifier_runs (
        run_id, data_tenant_id, verifier_uid, verifier_version, subject_card_uid,
        origin, owner_card_uid, binding_id, trigger_uid, trigger_digest, operators,
        window_start, window_end, input_record_id, input_event_time,
        requested_by_principal_id, max_attempts, next_attempt_at, created_at, updated_at,
        idempotency_key, request_sha256
    ) VALUES ($1, wyrd.current_tenant(), $2, $3, $4, $5, $6, $7, $8, $9, $10,
              $11, $12, $13, $14, $15, $16,
              statement_timestamp(), statement_timestamp(), statement_timestamp(), $17, $18)
    ON CONFLICT DO NOTHING
    RETURNING run_id
"#;

/// Find the run that already holds a scheduled occurrence or observation record.
const EXISTING_RUN_SQL: &str = r#"
    SELECT run_id
      FROM wyrd.verifier_runs
     WHERE binding_id = $1
       AND origin = $2
       AND ((origin = 'schedule' AND window_end = $3)
         OR (origin = 'observation' AND input_record_id = $4))
"#;

/// Serialize concurrent manual requests that share one requester and key.
///
/// Transaction-scoped, so it releases on commit or rollback; the class keeps
/// these locks apart from every other advisory lock in the database.
const LOCK_REQUEST_KEY_SQL: &str = r#"
    SELECT pg_advisory_xact_lock($1, hashtext($2::text || '/' || $3))
"#;

/// Advisory lock class of [`LOCK_REQUEST_KEY_SQL`].
const REQUEST_KEY_LOCK_CLASS: i32 = 0x0C_A2_D0_31;

/// Find the manual run a requester already created under one key.
const KEYED_RUN_SQL: &str = r#"
    SELECT run_id, request_sha256
      FROM wyrd.verifier_runs
     WHERE requested_by_principal_id = $1
       AND idempotency_key = $2
"#;

/// Read the subject Card a binding verifies.
const BINDING_SUBJECT_SQL: &str = r#"
    SELECT subject_card_uid
      FROM wyrd.verification_bindings
     WHERE binding_id = $1
"#;

/// Briefly lock the earliest due scheduled binding no other scheduler holds.
///
/// Dueness is decided against PostgreSQL's statement time, and the same
/// statement returns that instant so the synchronous cron calculation anchors
/// on the database clock rather than the scheduler process's.
const DUE_BINDING_SQL: &str = r#"
    SELECT binding_id, schedule_cron, schedule_tz, next_run_at, statement_timestamp() AS now
      FROM wyrd.verification_bindings
     WHERE activation = 'schedule'
       AND next_run_at <= statement_timestamp()
     ORDER BY next_run_at, binding_id
     LIMIT 1
       FOR UPDATE SKIP LOCKED
"#;

/// Move a claimed binding's cursor; NULL disarms an unparseable schedule.
const ADVANCE_CURSOR_SQL: &str = r#"
    UPDATE wyrd.verification_bindings
       SET next_run_at = $2
     WHERE binding_id = $1
"#;

/// Settle every expired lease that has no attempt left as `errored`.
///
/// Expiry is decided and stamped by PostgreSQL, so a lease written by one pod
/// is never judged against another pod's clock.
const EXHAUST_EXPIRED_SQL: &str = r#"
    UPDATE wyrd.verifier_runs
       SET status = 'errored', error = $1, lease_expires_at = NULL,
           next_attempt_at = NULL, settled_at = statement_timestamp(),
           updated_at = statement_timestamp()
     WHERE status = 'running'
       AND lease_expires_at <= statement_timestamp()
       AND attempts >= max_attempts
"#;

/// Claim the oldest due, retry-due, or lease-expired run under a fresh lease.
///
/// Availability, expiry, and the new lease deadline are all PostgreSQL's
/// statement time; `$2` is only the lease length in milliseconds, so a pod
/// whose clock differs from the database can neither claim early nor hold a
/// lease the database believes already expired.
const CLAIM_RUN_SQL: &str = r#"
    WITH candidate AS (
        SELECT run_id
          FROM wyrd.verifier_runs
         WHERE (status IN ('pending', 'retrying') AND next_attempt_at <= statement_timestamp())
            OR (status = 'running' AND lease_expires_at <= statement_timestamp())
         ORDER BY COALESCE(next_attempt_at, lease_expires_at), run_id
         LIMIT 1
           FOR UPDATE SKIP LOCKED
    )
    UPDATE wyrd.verifier_runs r
       SET status = 'running', attempts = r.attempts + 1, lease_token = $1,
           lease_expires_at = statement_timestamp() + ($2::bigint * INTERVAL '1 millisecond'),
           next_attempt_at = NULL, updated_at = statement_timestamp()
      FROM candidate
     WHERE r.run_id = candidate.run_id
    RETURNING r.run_id, r.verifier_uid, r.verifier_version, r.subject_card_uid,
              r.origin, r.owner_card_uid, r.binding_id, r.trigger_uid,
              r.trigger_digest, r.window_start, r.window_end, r.input_record_id,
              r.input_event_time, r.requested_by_principal_id, r.attempts,
              r.max_attempts, r.lease_expires_at
"#;

/// Complete a leased run, or re-apply the same completion idempotently.
const COMPLETE_RUN_SQL: &str = r#"
    UPDATE wyrd.verifier_runs
       SET status = 'completed', result_id = $3, error = NULL,
           lease_expires_at = NULL, next_attempt_at = NULL,
           settled_at = COALESCE(settled_at, statement_timestamp()),
           updated_at = statement_timestamp()
     WHERE run_id = $1
       AND lease_token = $2
       AND (status = 'running' OR (status = 'completed' AND result_id = $3))
    RETURNING binding_id, operators
"#;

/// Insert one Operator dispatch; the (tenant, run, Operator) key absorbs retries.
const INSERT_DISPATCH_SQL: &str = r#"
    INSERT INTO wyrd.operator_dispatches (
        dispatch_id, data_tenant_id, run_id, operator_uid, operator_digest,
        next_attempt_at, created_at, updated_at
    ) VALUES ($1, wyrd.current_tenant(), $2, $3, $4,
              statement_timestamp(), statement_timestamp(), statement_timestamp())
    ON CONFLICT DO NOTHING
"#;

/// Lock a leased run's attempt budget before deciding retry or exhaustion.
const LEASED_ATTEMPTS_SQL: &str = r#"
    SELECT attempts, max_attempts
      FROM wyrd.verifier_runs
     WHERE run_id = $1 AND lease_token = $2 AND status = 'running'
       FOR UPDATE
"#;

/// Schedule a leased run's next attempt with its last error.
///
/// `$4` is the backoff in milliseconds; the deadline itself is PostgreSQL's
/// statement time plus that delay, and the statement returns it so the caller
/// reports the stored instant rather than a locally derived one.
const RETRY_RUN_SQL: &str = r#"
    UPDATE wyrd.verifier_runs
       SET status = 'retrying', error = $3,
           next_attempt_at = statement_timestamp() + ($4::bigint * INTERVAL '1 millisecond'),
           lease_expires_at = NULL, updated_at = statement_timestamp()
     WHERE run_id = $1 AND lease_token = $2 AND status = 'running'
    RETURNING next_attempt_at
"#;

/// Settle a leased run in a terminal non-completed status.
const TERMINATE_RUN_SQL: &str = r#"
    UPDATE wyrd.verifier_runs
       SET status = $3, error = $4, lease_expires_at = NULL,
           next_attempt_at = NULL, settled_at = statement_timestamp(),
           updated_at = statement_timestamp()
     WHERE run_id = $1 AND lease_token = $2 AND status = 'running'
"#;

/// Return a leased run to the queue immediately, refunding its attempt.
const RELEASE_RUN_SQL: &str = r#"
    UPDATE wyrd.verifier_runs
       SET status = CASE WHEN attempts > 1 THEN 'retrying' ELSE 'pending' END,
           attempts = attempts - 1, lease_expires_at = NULL,
           next_attempt_at = statement_timestamp(), updated_at = statement_timestamp()
     WHERE run_id = $1 AND lease_token = $2 AND status = 'running'
"#;

/// Read one run's control-plane status.
const RUN_STATUS_SQL: &str = r#"
    SELECT run_id, status, requested_by_principal_id, result_id, error
      FROM wyrd.verifier_runs
     WHERE run_id = $1
"#;

/// Read one run's dispatches in identity order.
const RUN_DISPATCHES_SQL: &str = r#"
    SELECT dispatch_id, operator_uid, operator_digest, status, last_error
      FROM wyrd.operator_dispatches
     WHERE run_id = $1
     ORDER BY dispatch_id
"#;

/// Read a binding's frozen targets, readiness, and latest run.
const BINDING_TARGETS_SQL: &str = r#"
    SELECT b.subject_card_uid, b.verifier_uid,
           wyrd.verifier_readiness(b.verifier_uid) AS readiness,
           (SELECT r.run_id FROM wyrd.verifier_runs r
             WHERE r.binding_id = b.binding_id
             ORDER BY r.run_id DESC LIMIT 1) AS last_run_id
      FROM wyrd.verification_bindings b
     WHERE b.binding_id = $1
"#;

/// List tenants with claimable runs, longest-waiting tenant first.
const RUNNABLE_TENANTS_SQL: &str = r#"
    SELECT data_tenant_id
      FROM wyrd.verifier_runs
     WHERE (status IN ('pending', 'retrying') AND next_attempt_at <= statement_timestamp())
        OR (status = 'running' AND lease_expires_at <= statement_timestamp())
     GROUP BY data_tenant_id
     ORDER BY min(COALESCE(next_attempt_at, lease_expires_at)), data_tenant_id
     LIMIT $1
"#;

/// List tenants with due scheduled bindings, most overdue tenant first.
const DUE_TENANTS_SQL: &str = r#"
    SELECT data_tenant_id
      FROM wyrd.verification_bindings
     WHERE activation = 'schedule'
       AND next_run_at <= statement_timestamp()
     GROUP BY data_tenant_id
     ORDER BY min(next_run_at), data_tenant_id
     LIMIT $1
"#;

/// Count non-terminal runs and dispatches by status across every tenant.
const QUEUE_DEPTH_SQL: &str = r#"
    SELECT
        (SELECT count(*) FROM wyrd.verifier_runs WHERE status = 'pending') AS runs_pending,
        (SELECT count(*) FROM wyrd.verifier_runs WHERE status = 'retrying') AS runs_retrying,
        (SELECT count(*) FROM wyrd.verifier_runs WHERE status = 'running') AS runs_running,
        (SELECT count(*) FROM wyrd.operator_dispatches WHERE status = 'pending') AS dispatches_pending,
        (SELECT count(*) FROM wyrd.operator_dispatches WHERE status = 'retrying') AS dispatches_retrying,
        (SELECT count(*) FROM wyrd.operator_dispatches WHERE status = 'running') AS dispatches_running
"#;

/// Why a run was created.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunOrigin {
    /// A due cron occurrence of a scheduled binding.
    Schedule,
    /// An authenticated `POST /v1/verification/runs` call.
    Manual,
    /// A committed Eval observation matching an `observations_ready` binding.
    Observation,
}

impl RunOrigin {
    /// Stored `origin` column value.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Schedule => "schedule",
            Self::Manual => "manual",
            Self::Observation => "observation",
        }
    }

    /// Decode a stored `origin` value.
    ///
    /// # Errors
    /// Returns [`SqlxError::Decode`] for a value outside the closed set.
    fn parse(value: &str) -> Result<Self, SqlxError> {
        match value {
            "schedule" => Ok(Self::Schedule),
            "manual" => Ok(Self::Manual),
            "observation" => Ok(Self::Observation),
            other => Err(SqlxError::Decode(
                format!("unknown verifier run origin {other:?}").into(),
            )),
        }
    }
}

/// The frozen input a run analyzes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RunInput {
    /// A Drift comparison window.
    DriftWindow(DriftWindow),
    /// One committed Eval observation, located by its record ID and the exact
    /// server-managed `wyrd_event_time` that selects its UTC-day partition.
    EvalRecord {
        /// Logical input record ID.
        record_id: String,
        /// Committed observation's server event time.
        event_time: DateTime<Utc>,
    },
}

impl RunInput {
    /// Verifier implementation (`implementation.kind`) able to consume this input.
    const fn implementation(&self) -> &'static str {
        match self {
            Self::DriftWindow(_) => "drift",
            Self::EvalRecord { .. } => "eval",
        }
    }
}

/// A request to enqueue one run through the single shared enqueue path.
///
/// Each origin carries exactly the fields it requires: only a manual request
/// names a requester and may target a Verifier directly; scheduled and
/// observation work always comes from a binding, with a Drift window or an
/// Eval record respectively.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RunRequest {
    /// An authenticated manual Drift run of a binding or a direct Verifier.
    Manual {
        /// Authenticated caller, frozen as `requested_by_principal_id`.
        requested_by: PrincipalId,
        /// Binding or direct target.
        target: VerificationRunTarget,
        /// Validated Drift window.
        window: DriftWindow,
    },
    /// One due cron occurrence of a scheduled binding.
    Scheduled {
        /// The binding whose occurrence is due.
        binding_id: BindingId,
        /// The occurrence's fixed window.
        window: DriftWindow,
    },
    /// One committed Eval observation for an `observations_ready` binding.
    Observation {
        /// The matching binding.
        binding_id: BindingId,
        /// Logical input record ID.
        record_id: String,
        /// Committed observation's server event time.
        event_time: DateTime<Utc>,
    },
}

impl RunRequest {
    /// The origin stored for this request.
    const fn origin(&self) -> RunOrigin {
        match self {
            Self::Manual { .. } => RunOrigin::Manual,
            Self::Scheduled { .. } => RunOrigin::Schedule,
            Self::Observation { .. } => RunOrigin::Observation,
        }
    }

    /// The input this request freezes.
    fn input(&self) -> RunInput {
        match self {
            Self::Manual { window, .. } | Self::Scheduled { window, .. } => {
                RunInput::DriftWindow(*window)
            }
            Self::Observation {
                record_id,
                event_time,
                ..
            } => RunInput::EvalRecord {
                record_id: record_id.clone(),
                event_time: *event_time,
            },
        }
    }

    /// The binding this request targets, or `None` for a direct run.
    fn binding_id(&self) -> Option<BindingId> {
        match self {
            Self::Manual {
                target: VerificationRunTarget::Binding { binding_id },
                ..
            }
            | Self::Scheduled { binding_id, .. }
            | Self::Observation { binding_id, .. } => Some(*binding_id),
            Self::Manual {
                target: VerificationRunTarget::Verifier { .. },
                ..
            } => None,
        }
    }

    /// The manual requester, absent for scheduled and observation work.
    const fn requested_by(&self) -> Option<PrincipalId> {
        match self {
            Self::Manual { requested_by, .. } => Some(*requested_by),
            Self::Scheduled { .. } | Self::Observation { .. } => None,
        }
    }
}

/// Why a request was refused before any run was created.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnqueueRefusal {
    /// The binding does not exist in the caller's tenant.
    BindingNotFound,
    /// The Verifier cannot run: `VerifierUnavailable` means the target is not
    /// an active Verifier Card; `BaselineNotReady` means its fitted baseline
    /// is not ready.
    NotReady(VerifierReadiness),
    /// The subject Card does not exist or is not active in the tenant.
    SubjectUnavailable,
    /// The Verifier's implementation cannot consume the requested input,
    /// such as a Drift window for an Eval Verifier.
    InputMismatch,
}

/// Result of the shared enqueue path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnqueueOutcome {
    /// A new pending run was created.
    Enqueued(VerificationRunId),
    /// The scheduled occurrence or observation record already has this run.
    AlreadyEnqueued(VerificationRunId),
    /// Nothing was created.
    Refused(EnqueueRefusal),
}

/// A caller's `Idempotency-Key` for one manual request and that request's digest.
///
/// The key is scoped to the requesting principal. The digest is the SHA-256 of
/// the canonical request body; a retry must match it to be a replay.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RequestKey<'a> {
    /// Caller-supplied `Idempotency-Key` value.
    pub key: &'a str,
    /// SHA-256 of the canonical request body.
    pub request_sha256: &'a [u8],
}

/// Result of [`VerifierRunQueue::enqueue_manual`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ManualEnqueueOutcome {
    /// A new pending run was created.
    Enqueued(VerificationRunId),
    /// The requester already used this key for the same request; this is its run.
    Replayed(VerificationRunId),
    /// The requester already used this key for a different request; nothing
    /// was created. Carries the run the key belongs to.
    KeyReused(VerificationRunId),
    /// Nothing was created.
    Refused(EnqueueRefusal),
}

/// Why a claimed due occurrence created no run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScheduleSkip {
    /// A later boundary had already passed; missed occurrences are not backfilled.
    Missed,
    /// The owner principal was not runtime-active at claim time.
    Inactive,
    /// The shared enqueue path refused the occurrence.
    Refused(EnqueueRefusal),
    /// The stored schedule no longer parses; the cursor was disarmed.
    InvalidSchedule,
}

/// What one scheduler claim of a due binding did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScheduleOutcome {
    /// A run was created for the occurrence's window.
    Enqueued(VerificationRunId),
    /// The occurrence already had this run.
    AlreadyEnqueued(VerificationRunId),
    /// No run was created.
    Skipped(ScheduleSkip),
}

/// One processed due binding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScheduleTick {
    /// The claimed binding.
    pub binding_id: BindingId,
    /// The stored cursor that was due.
    pub due_at: DateTime<Utc>,
    /// What the claim did.
    pub outcome: ScheduleOutcome,
    /// The cursor after this tick; `None` only for a disarmed invalid schedule.
    pub next_run_at: Option<DateTime<Utc>>,
}

/// Opaque fencing token of one claim.
///
/// Only [`VerifierRunQueue::claim`] mints one, so a settlement can only be
/// attempted by a worker that actually claimed the run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LeaseToken(Uuid);

/// The identity a runner settles a claimed run with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RunLease {
    /// The claimed run.
    pub run_id: VerificationRunId,
    /// This claim's fencing token.
    pub token: LeaseToken,
}

/// A run claimed for execution, with every frozen identity the runner needs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClaimedRun {
    /// Settlement identity.
    pub lease: RunLease,
    /// When the lease expires and the run becomes reclaimable.
    pub lease_expires_at: DateTime<Utc>,
    /// This attempt's number, starting at 1.
    pub attempt: i32,
    /// Attempt budget frozen at enqueue.
    pub max_attempts: i32,
    /// Exact Verifier Card UID.
    pub verifier_uid: CardUid,
    /// Exact Verifier Card version.
    pub verifier_version: String,
    /// Exact verified subject Card.
    pub subject_card_uid: CardUid,
    /// Why the run exists.
    pub origin: RunOrigin,
    /// Binding owner; `None` for a direct run.
    pub owner_card_uid: Option<CardUid>,
    /// Binding; `None` for a direct run.
    pub binding_id: Option<BindingId>,
    /// Effective Trigger; `None` for a direct run.
    pub trigger: Option<FrozenTarget>,
    /// Frozen input.
    pub input: RunInput,
    /// Manual requester; `None` for scheduled and observation runs.
    pub requested_by: Option<PrincipalId>,
}

/// Outcome of a token-fenced settlement.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Settlement {
    /// The transition was applied (or idempotently re-applied).
    Applied,
    /// The lease token no longer holds the run; nothing changed.
    StaleLease,
}

/// Outcome of reporting a retryable engine failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RetryOutcome {
    /// The same run and input will be attempted again at this time.
    Scheduled(DateTime<Utc>),
    /// The attempt budget is exhausted; the run is now `errored`.
    Exhausted,
    /// The lease token no longer holds the run; nothing changed.
    StaleLease,
}

/// Terminal execution states that carry no verdict.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TerminalStatus {
    /// Stopped before producing a result.
    Cancelled,
    /// Exceeded its execution deadline.
    TimedOut,
    /// Failed terminally.
    Errored,
}

impl TerminalStatus {
    /// Stored `status` column value.
    const fn as_str(self) -> &'static str {
        match self {
            Self::Cancelled => "cancelled",
            Self::TimedOut => "timed_out",
            Self::Errored => "errored",
        }
    }
}

/// Bounded retry of engine failures: attempts and exponential backoff.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RunRetryPolicy {
    /// Total attempts, including the first.
    max_attempts: i32,
    /// Delay after the first failed attempt; doubles after each later one.
    base_delay: Duration,
    /// Ceiling on any single delay.
    max_delay: Duration,
}

impl RunRetryPolicy {
    /// Build a policy.
    ///
    /// # Panics
    /// Panics when `max_attempts` is not positive, since a run must be
    /// attempted at least once.
    #[must_use]
    pub fn new(max_attempts: i32, base_delay: Duration, max_delay: Duration) -> Self {
        assert!(
            max_attempts > 0,
            "invariant: a run is attempted at least once"
        );
        Self {
            max_attempts,
            base_delay,
            max_delay,
        }
    }

    /// Delay before the attempt after failed attempt number `attempt`.
    ///
    /// `base_delay × 2^(attempt − 1)`, capped at `max_delay`.
    #[must_use]
    pub fn delay_after(&self, attempt: i32) -> Duration {
        let doublings = u32::try_from(attempt.saturating_sub(1))
            .unwrap_or(0)
            .min(16);
        self.base_delay
            .checked_mul(1 << doublings)
            .map_or(self.max_delay, |delay| delay.min(self.max_delay))
    }
}

impl Default for RunRetryPolicy {
    /// Three attempts with 30-second then 60-second delays, capped at two minutes.
    fn default() -> Self {
        Self::new(3, Duration::seconds(30), Duration::minutes(2))
    }
}

/// Non-terminal counts of one queue.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct QueueCounts {
    /// Waiting for a first claim.
    pub pending: i64,
    /// Waiting for a retry.
    pub retrying: i64,
    /// Held under a lease (live or expired).
    pub running: i64,
}

/// Cross-tenant queue depth for telemetry; a fixed, bounded set of counts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct QueueDepth {
    /// Verifier runs.
    pub runs: QueueCounts,
    /// Operator dispatches.
    pub dispatches: QueueCounts,
}

/// Owner of the durable Verifier run queue's policies and transitions.
///
/// Holds the retry policy frozen into every new run and the inactivity window
/// the scheduler and binding status apply. It holds no connection: every
/// tenant method runs on the caller's [`TenantConn`] and never commits.
#[derive(Debug, Clone, Copy, Default)]
pub struct VerifierRunQueue {
    /// Engine-failure retry policy.
    retry: RunRetryPolicy,
    /// How long a qualifying exchange keeps a binding owner active.
    inactivity: InactivityTimeout,
}

impl VerifierRunQueue {
    /// Enqueue one run through the shared path used by every origin.
    ///
    /// Resolves the target in the caller's tenant: a binding target freezes
    /// the binding's owner, subject, Verifier, Trigger, and Operators; a
    /// direct target freezes only the Verifier and subject. It then refuses,
    /// without writing, a missing binding, an unavailable or unready Verifier,
    /// an unavailable subject, or an input the Verifier's implementation
    /// cannot consume. Otherwise it inserts a `pending` run due at
    /// PostgreSQL's statement time. A scheduled occurrence or observation
    /// record that already has a run returns that run as
    /// [`EnqueueOutcome::AlreadyEnqueued`].
    ///
    /// # Errors
    /// Returns the database error when a read or the insert fails, or a
    /// decode error when stored identities are malformed.
    #[tracing::instrument(skip(self, conn, request), fields(operation = "verification.runs.enqueue", origin = request.origin().as_str()))]
    pub async fn enqueue(
        &self,
        conn: &mut TenantConn<'_>,
        request: &RunRequest,
    ) -> Result<EnqueueOutcome, SqlxError> {
        self.insert(conn, request, None).await
    }

    /// Enqueue one authenticated manual Drift run, honoring its `Idempotency-Key`.
    ///
    /// Without a key this is [`Self::enqueue`] for a manual request. With a
    /// key it first takes a transaction-scoped advisory lock on (requester,
    /// key) so concurrent retries serialize, then returns the run the
    /// requester already created under that key:
    /// [`ManualEnqueueOutcome::Replayed`] when the request digest matches and
    /// [`ManualEnqueueOutcome::KeyReused`] when it does not. Only an unused
    /// key reaches the shared enqueue path, which stores the key and digest on
    /// the new run. Refusals write nothing, so a refused key stays unused. The
    /// caller commits.
    ///
    /// # Errors
    /// Returns the database error when the lock, a read, or the insert fails,
    /// or a decode error when stored identities are malformed.
    #[tracing::instrument(skip(self, conn, target, key), fields(operation = "verification.runs.enqueue_manual", keyed = key.is_some()))]
    pub async fn enqueue_manual(
        &self,
        conn: &mut TenantConn<'_>,
        requested_by: PrincipalId,
        target: &VerificationRunTarget,
        window: DriftWindow,
        key: Option<RequestKey<'_>>,
    ) -> Result<ManualEnqueueOutcome, SqlxError> {
        if let Some(key) = key {
            sqlx::query(LOCK_REQUEST_KEY_SQL)
                .bind(REQUEST_KEY_LOCK_CLASS)
                .bind(requested_by.as_uuid())
                .bind(key.key)
                .execute(&mut **conn.transaction())
                .await?;
            let existing: Option<(Uuid, Option<Vec<u8>>)> = sqlx::query_as(KEYED_RUN_SQL)
                .bind(requested_by.as_uuid())
                .bind(key.key)
                .fetch_optional(&mut **conn.transaction())
                .await?;
            if let Some((run_id, digest)) = existing {
                let run_id = stored(VerificationRunId::new(run_id))?;
                return Ok(if digest.as_deref() == Some(key.request_sha256) {
                    ManualEnqueueOutcome::Replayed(run_id)
                } else {
                    ManualEnqueueOutcome::KeyReused(run_id)
                });
            }
        }
        let request = RunRequest::Manual {
            requested_by,
            target: target.clone(),
            window,
        };
        Ok(match self.insert(conn, &request, key).await? {
            EnqueueOutcome::Enqueued(run_id) | EnqueueOutcome::AlreadyEnqueued(run_id) => {
                ManualEnqueueOutcome::Enqueued(run_id)
            }
            EnqueueOutcome::Refused(refusal) => ManualEnqueueOutcome::Refused(refusal),
        })
    }

    /// Return the subject Card a manual target would verify.
    ///
    /// A direct target names its subject; a binding target verifies its
    /// projected subject. An unknown or other-tenant binding is `Ok(None)`.
    /// Authorization reads this before enqueue to check a Card-bound caller's
    /// scope over the exact subject.
    ///
    /// # Errors
    /// Returns the database error when the binding read fails, or a decode
    /// error when the stored subject is malformed.
    pub async fn target_subject(
        &self,
        conn: &mut TenantConn<'_>,
        target: &VerificationRunTarget,
    ) -> Result<Option<CardUid>, SqlxError> {
        match target {
            VerificationRunTarget::Verifier {
                subject_card_uid, ..
            } => Ok(Some(subject_card_uid.clone())),
            VerificationRunTarget::Binding { binding_id } => {
                let subject: Option<Uuid> = sqlx::query_scalar(BINDING_SUBJECT_SQL)
                    .bind(binding_id.as_uuid())
                    .fetch_optional(&mut **conn.transaction())
                    .await?;
                subject
                    .map(|uid| stored(CardUid::from_uuid(uid)))
                    .transpose()
            }
        }
    }

    /// Resolve, check, and insert one run, storing `key` when one is given.
    ///
    /// Shared by every origin; see [`Self::enqueue`] for the resolution and
    /// refusal rules. `key` is `Some` only for a keyed manual request.
    ///
    /// # Errors
    /// Returns the database error when a read or the insert fails, or a
    /// decode error when stored identities are malformed.
    async fn insert(
        &self,
        conn: &mut TenantConn<'_>,
        request: &RunRequest,
        key: Option<RequestKey<'_>>,
    ) -> Result<EnqueueOutcome, SqlxError> {
        let resolved: Option<ResolvedTarget> = match request {
            RunRequest::Manual {
                target:
                    VerificationRunTarget::Verifier {
                        verifier_uid,
                        subject_card_uid,
                    },
                ..
            } => {
                sqlx::query_as(RESOLVE_DIRECT_SQL)
                    .bind(verifier_uid.as_uuid())
                    .bind(subject_card_uid.as_uuid())
                    .fetch_optional(&mut **conn.transaction())
                    .await?
            }
            _ => {
                let binding = request
                    .binding_id()
                    .expect("invariant: every non-direct request names a binding");
                sqlx::query_as(RESOLVE_BINDING_SQL)
                    .bind(binding.as_uuid())
                    .fetch_optional(&mut **conn.transaction())
                    .await?
            }
        };
        let Some(resolved) = resolved else {
            return Ok(EnqueueOutcome::Refused(EnqueueRefusal::BindingNotFound));
        };
        let input = request.input();
        if let Some(refusal) = resolved.refusal(&input)? {
            return Ok(EnqueueOutcome::Refused(refusal));
        }
        let verifier_version = resolved
            .verifier_version
            .expect("invariant: a ready Verifier has a Card version");
        let (window_start, window_end, record_id, event_time) = match &input {
            RunInput::DriftWindow(window) => (Some(window.start), Some(window.end), None, None),
            RunInput::EvalRecord {
                record_id,
                event_time,
            } => (None, None, Some(record_id.as_str()), Some(*event_time)),
        };
        let origin = request.origin();
        let inserted: Option<Uuid> = sqlx::query_scalar(INSERT_RUN_SQL)
            .bind(VerificationRunId::new_v7().as_uuid())
            .bind(resolved.verifier_uid)
            .bind(verifier_version)
            .bind(resolved.subject_card_uid)
            .bind(origin.as_str())
            .bind(resolved.owner_card_uid)
            .bind(resolved.binding_id)
            .bind(resolved.trigger_uid)
            .bind(resolved.trigger_digest)
            .bind(resolved.operators)
            .bind(window_start)
            .bind(window_end)
            .bind(record_id)
            .bind(event_time)
            .bind(request.requested_by().map(|principal| principal.as_uuid()))
            .bind(self.retry.max_attempts)
            .bind(key.map(|key| key.key))
            .bind(key.map(|key| key.request_sha256))
            .fetch_optional(&mut **conn.transaction())
            .await?;
        if let Some(run_id) = inserted {
            return Ok(EnqueueOutcome::Enqueued(stored(VerificationRunId::new(
                run_id,
            ))?));
        }
        let existing: Uuid = sqlx::query_scalar(EXISTING_RUN_SQL)
            .bind(resolved.binding_id)
            .bind(origin.as_str())
            .bind(window_end)
            .bind(record_id)
            .fetch_one(&mut **conn.transaction())
            .await?;
        Ok(EnqueueOutcome::AlreadyEnqueued(stored(
            VerificationRunId::new(existing),
        )?))
    }

    /// Process the earliest due scheduled binding of the caller's tenant.
    ///
    /// Locks one binding whose cursor is at or before PostgreSQL's statement
    /// time with `FOR UPDATE SKIP LOCKED`, so concurrent schedulers never
    /// claim the same occurrence. That same database instant is the anchor for
    /// every decision this tick makes: the occurrence yields one run for its
    /// fixed window only when it was not missed by then, its owner principal
    /// is runtime-active, and the shared enqueue path accepts it (which
    /// enforces Verifier readiness). In every case the cursor moves to the
    /// first boundary strictly after that anchor in the same transaction, so
    /// inactive, unready, and missed occurrences are skipped without
    /// backfill. Returns `None` when nothing is due. The
    /// caller commits, releasing the lock before any analysis runs, and calls
    /// again until `None`.
    ///
    /// # Errors
    /// Returns the database error when a read or write fails, or a decode
    /// error when stored identities are malformed.
    #[tracing::instrument(skip(self, conn), fields(operation = "verification.schedule.tick"))]
    pub async fn schedule_next_due(
        &self,
        conn: &mut TenantConn<'_>,
    ) -> Result<Option<ScheduleTick>, SqlxError> {
        let due: Option<DueBindingRow> = sqlx::query_as(DUE_BINDING_SQL)
            .fetch_optional(&mut **conn.transaction())
            .await?;
        let Some(due) = due else {
            return Ok(None);
        };
        let due_at = due.next_run_at;
        let binding_id = stored(BindingId::new(due.binding_id))?;
        let occurrence = match BindingSchedule::parse(
            &due.schedule_cron,
            due.schedule_tz.as_deref(),
        )
        .and_then(|schedule| schedule.occurrence(due_at, due.now))
        {
            Ok(occurrence) => occurrence,
            Err(error) => {
                tracing::error!(%binding_id, %error, "stored binding schedule cannot advance; disarming");
                self.advance_cursor(conn, binding_id, None).await?;
                return Ok(Some(ScheduleTick {
                    binding_id,
                    due_at,
                    outcome: ScheduleOutcome::Skipped(ScheduleSkip::InvalidSchedule),
                    next_run_at: None,
                }));
            }
        };
        let outcome = self
            .schedule_occurrence(conn, binding_id, occurrence.window)
            .await?;
        self.advance_cursor(conn, binding_id, Some(occurrence.next_run_at))
            .await?;
        Ok(Some(ScheduleTick {
            binding_id,
            due_at,
            outcome,
            next_run_at: Some(occurrence.next_run_at),
        }))
    }

    /// Decide and, when eligible, enqueue one claimed occurrence.
    ///
    /// # Errors
    /// Returns the database error when the activity read or enqueue fails.
    async fn schedule_occurrence(
        &self,
        conn: &mut TenantConn<'_>,
        binding_id: BindingId,
        window: Option<DriftWindow>,
    ) -> Result<ScheduleOutcome, SqlxError> {
        let Some(window) = window else {
            return Ok(ScheduleOutcome::Skipped(ScheduleSkip::Missed));
        };
        let active = binding_activity(conn, binding_id, self.inactivity)
            .await?
            .is_some_and(|activity| activity.active);
        if !active {
            return Ok(ScheduleOutcome::Skipped(ScheduleSkip::Inactive));
        }
        let request = RunRequest::Scheduled { binding_id, window };
        Ok(match self.enqueue(conn, &request).await? {
            EnqueueOutcome::Enqueued(run_id) => ScheduleOutcome::Enqueued(run_id),
            EnqueueOutcome::AlreadyEnqueued(run_id) => ScheduleOutcome::AlreadyEnqueued(run_id),
            EnqueueOutcome::Refused(refusal) => {
                ScheduleOutcome::Skipped(ScheduleSkip::Refused(refusal))
            }
        })
    }

    /// Store a claimed binding's next cursor.
    ///
    /// # Errors
    /// Returns the database error when the update fails.
    async fn advance_cursor(
        &self,
        conn: &mut TenantConn<'_>,
        binding_id: BindingId,
        next_run_at: Option<DateTime<Utc>>,
    ) -> Result<(), SqlxError> {
        sqlx::query(ADVANCE_CURSOR_SQL)
            .bind(binding_id.as_uuid())
            .bind(next_run_at)
            .execute(&mut **conn.transaction())
            .await?;
        Ok(())
    }

    /// Claim one runnable run of the caller's tenant under a fresh lease.
    ///
    /// First settles, as `errored`, every expired lease whose attempts are
    /// exhausted, so a crash loop cannot exceed the budget. Then locks the
    /// oldest `pending` or `retrying` run due at PostgreSQL's statement time,
    /// or `running` run whose lease the database considers expired, with `SKIP
    /// LOCKED`; marks it `running` with a new lease token expiring
    /// `lease_for` after that same database instant; and counts the attempt.
    /// Returns `None` when nothing is runnable. The caller commits before
    /// executing.
    ///
    /// # Errors
    /// Returns the database error when a statement fails, or a decode error
    /// when the claimed row's identities are malformed.
    #[tracing::instrument(skip(self, conn), fields(operation = "verification.runs.claim"))]
    pub async fn claim(
        &self,
        conn: &mut TenantConn<'_>,
        lease_for: Duration,
    ) -> Result<Option<ClaimedRun>, SqlxError> {
        let exhausted = VerificationError {
            code: "lease_expired".to_owned(),
            message: "the run's lease expired after its final attempt".to_owned(),
        };
        sqlx::query(EXHAUST_EXPIRED_SQL)
            .bind(Json(&exhausted))
            .execute(&mut **conn.transaction())
            .await?;
        let token = Uuid::now_v7();
        let row: Option<ClaimedRunRow> = sqlx::query_as(CLAIM_RUN_SQL)
            .bind(token)
            .bind(lease_for.num_milliseconds())
            .fetch_optional(&mut **conn.transaction())
            .await?;
        row.map(|row| row.into_claimed(LeaseToken(token)))
            .transpose()
    }

    /// Settle a claimed run `completed`, pointing at its acknowledged result.
    ///
    /// Only the current lease holder can complete; re-completing with the same
    /// token and result is an idempotent [`Settlement::Applied`]. For a
    /// binding-created run with a `failed` verdict, the same transaction
    /// inserts one `pending` dispatch per distinct frozen Operator, due at
    /// PostgreSQL's statement time; the (tenant, run, Operator) key makes a
    /// settlement retry insert
    /// nothing new. Passed and inconclusive verdicts and direct runs create no
    /// dispatch. The verdict itself is not stored: Bifrost owns it.
    ///
    /// # Errors
    /// Returns the database error when a statement fails, or a decode error
    /// when the frozen Operators are malformed.
    #[tracing::instrument(skip(self, conn), fields(operation = "verification.runs.complete", run_id = %lease.run_id))]
    pub async fn complete(
        &self,
        conn: &mut TenantConn<'_>,
        lease: RunLease,
        result_id: VerificationResultId,
        verdict: VerificationVerdict,
    ) -> Result<Settlement, SqlxError> {
        let settled: Option<(Option<Uuid>, Json<Vec<FrozenTarget>>)> =
            sqlx::query_as(COMPLETE_RUN_SQL)
                .bind(lease.run_id.as_uuid())
                .bind(lease.token.0)
                .bind(result_id.as_uuid())
                .fetch_optional(&mut **conn.transaction())
                .await?;
        let Some((binding_id, Json(operators))) = settled else {
            return Ok(Settlement::StaleLease);
        };
        if verdict == VerificationVerdict::Failed && binding_id.is_some() {
            for operator in &operators {
                let (uid, digest) = match operator {
                    FrozenTarget::Uid(uid) => (Some(uid.as_uuid()), None),
                    FrozenTarget::Digest(digest) => (None, Some(digest.as_str())),
                };
                sqlx::query(INSERT_DISPATCH_SQL)
                    .bind(OperatorDispatchId::new_v7().as_uuid())
                    .bind(lease.run_id.as_uuid())
                    .bind(uid)
                    .bind(digest)
                    .execute(&mut **conn.transaction())
                    .await?;
            }
        }
        Ok(Settlement::Applied)
    }

    /// Report a retryable engine failure for a claimed run.
    ///
    /// With attempts left, the same run and frozen input become `retrying`,
    /// due after the policy's backoff measured from PostgreSQL's statement
    /// time, keeping `error` visible; the stored deadline is returned as the
    /// database wrote it. With the budget exhausted, the run settles `errored`
    /// with `error`, no verdict, and no dispatch.
    ///
    /// # Errors
    /// Returns the database error when a statement fails.
    #[tracing::instrument(skip(self, conn, error), fields(operation = "verification.runs.retry", run_id = %lease.run_id))]
    pub async fn retry(
        &self,
        conn: &mut TenantConn<'_>,
        lease: RunLease,
        error: &VerificationError,
    ) -> Result<RetryOutcome, SqlxError> {
        let budget: Option<(i32, i32)> = sqlx::query_as(LEASED_ATTEMPTS_SQL)
            .bind(lease.run_id.as_uuid())
            .bind(lease.token.0)
            .fetch_optional(&mut **conn.transaction())
            .await?;
        let Some((attempts, max_attempts)) = budget else {
            return Ok(RetryOutcome::StaleLease);
        };
        if attempts >= max_attempts {
            self.terminate(conn, lease, TerminalStatus::Errored, error)
                .await?;
            return Ok(RetryOutcome::Exhausted);
        }
        let next_attempt_at: Option<DateTime<Utc>> = sqlx::query_scalar(RETRY_RUN_SQL)
            .bind(lease.run_id.as_uuid())
            .bind(lease.token.0)
            .bind(Json(error))
            .bind(self.retry.delay_after(attempts).num_milliseconds())
            .fetch_optional(&mut **conn.transaction())
            .await?;
        Ok(next_attempt_at.map_or(RetryOutcome::StaleLease, RetryOutcome::Scheduled))
    }

    /// Settle a claimed run in a terminal status with no verdict.
    ///
    /// `cancelled`, `timed_out`, and `errored` runs record `error`, point at no
    /// result, and never dispatch an Operator.
    ///
    /// # Errors
    /// Returns the database error when the update fails.
    #[tracing::instrument(skip(self, conn, error), fields(operation = "verification.runs.terminate", run_id = %lease.run_id))]
    pub async fn terminate(
        &self,
        conn: &mut TenantConn<'_>,
        lease: RunLease,
        status: TerminalStatus,
        error: &VerificationError,
    ) -> Result<Settlement, SqlxError> {
        let result = sqlx::query(TERMINATE_RUN_SQL)
            .bind(lease.run_id.as_uuid())
            .bind(lease.token.0)
            .bind(status.as_str())
            .bind(Json(error))
            .execute(&mut **conn.transaction())
            .await?;
        Ok(settlement(result.rows_affected()))
    }

    /// Return a claimed run to the queue immediately, for shutdown drain.
    ///
    /// The run keeps its identity and frozen input, becomes due at
    /// PostgreSQL's statement time, and has the interrupted attempt refunded,
    /// so a drained run does not consume its retry budget.
    ///
    /// # Errors
    /// Returns the database error when the update fails.
    #[tracing::instrument(skip(self, conn), fields(operation = "verification.runs.release", run_id = %lease.run_id))]
    pub async fn release(
        &self,
        conn: &mut TenantConn<'_>,
        lease: RunLease,
    ) -> Result<Settlement, SqlxError> {
        let result = sqlx::query(RELEASE_RUN_SQL)
            .bind(lease.run_id.as_uuid())
            .bind(lease.token.0)
            .execute(&mut **conn.transaction())
            .await?;
        Ok(settlement(result.rows_affected()))
    }

    /// Read one run's Run GET projection.
    ///
    /// Returns execution status, manual requester, result pointer, last error,
    /// and every dispatch's delivery status — never the verdict. An unknown or
    /// other-tenant run is `Ok(None)`.
    ///
    /// # Errors
    /// Returns the database error when a read fails, or a decode error when a
    /// stored identity, status, or error is malformed.
    pub async fn run_status(
        &self,
        conn: &mut TenantConn<'_>,
        run_id: VerificationRunId,
    ) -> Result<Option<VerificationRunStatus>, SqlxError> {
        let row: Option<RunStatusRow> = sqlx::query_as(RUN_STATUS_SQL)
            .bind(run_id.as_uuid())
            .fetch_optional(&mut **conn.transaction())
            .await?;
        let Some(row) = row else {
            return Ok(None);
        };
        let dispatches: Vec<DispatchRow> = sqlx::query_as(RUN_DISPATCHES_SQL)
            .bind(run_id.as_uuid())
            .fetch_all(&mut **conn.transaction())
            .await?;
        Ok(Some(VerificationRunStatus {
            run_id: stored(VerificationRunId::new(row.run_id))?,
            status: stored(row.status.parse())?,
            requested_by_principal_id: row.requested_by_principal_id.map(PrincipalId::new),
            result_id: row
                .result_id
                .map(|id| stored(VerificationResultId::new(id)))
                .transpose()?,
            error: row.error.map(|Json(error)| error),
            dispatches: dispatches
                .into_iter()
                .map(DispatchRow::into_state)
                .collect::<Result<_, _>>()?,
        }))
    }

    /// Read one binding's Binding GET projection at database time.
    ///
    /// Combines the owner activity gate (the same query the scheduler
    /// applies), generic Verifier readiness, the schedule cursor, the owner's
    /// last qualifying exchange, and the latest run. An unknown or
    /// other-tenant binding is `Ok(None)`.
    ///
    /// # Errors
    /// Returns the database error when a read fails, or a decode error when a
    /// stored identity or readiness value is malformed.
    pub async fn binding_status(
        &self,
        conn: &mut TenantConn<'_>,
        binding_id: BindingId,
    ) -> Result<Option<VerificationBindingStatus>, SqlxError> {
        let Some(activity) = binding_activity(conn, binding_id, self.inactivity).await? else {
            return Ok(None);
        };
        let (subject, verifier, readiness, last_run): (Uuid, Uuid, String, Option<Uuid>) =
            sqlx::query_as(BINDING_TARGETS_SQL)
                .bind(binding_id.as_uuid())
                .fetch_one(&mut **conn.transaction())
                .await?;
        Ok(Some(VerificationBindingStatus {
            binding_id,
            owner_card_uid: activity.owner_card_uid,
            subject_card_uid: stored(CardUid::from_uuid(subject))?,
            verifier_uid: stored(CardUid::from_uuid(verifier))?,
            active: activity.active,
            readiness: stored(readiness.parse())?,
            next_run_at: activity.next_run_at,
            last_activated_at: activity.last_authenticated_at,
            last_run_id: last_run
                .map(|id| stored(VerificationRunId::new(id)))
                .transpose()?,
        }))
    }

    /// List up to `limit` tenants that have a claimable run at database time.
    ///
    /// Longest-waiting tenant first, so a runner visiting tenants in order does
    /// not let one busy tenant starve others. Read-only; claiming happens per
    /// tenant through [`Self::claim`].
    ///
    /// # Errors
    /// Returns the database error when the read fails, or a decode error when
    /// a stored tenant key is malformed.
    // tenant-isolation: cross-tenant OperatorPool
    pub async fn tenants_with_runnable_runs(
        &self,
        operator: &OperatorPool,
        limit: i64,
    ) -> Result<Vec<DataTenantId>, SqlxError> {
        let rows: Vec<Uuid> = sqlx::query_scalar(RUNNABLE_TENANTS_SQL)
            .bind(limit)
            .fetch_all(operator.pool())
            .await?;
        rows.into_iter()
            .map(|tenant| stored(DataTenantId::new(tenant)))
            .collect()
    }

    /// List up to `limit` tenants that have a due scheduled binding at
    /// database time.
    ///
    /// Most overdue tenant first. Read-only; claiming happens per tenant
    /// through [`Self::schedule_next_due`].
    ///
    /// # Errors
    /// Returns the database error when the read fails, or a decode error when
    /// a stored tenant key is malformed.
    // tenant-isolation: cross-tenant OperatorPool
    pub async fn tenants_with_due_bindings(
        &self,
        operator: &OperatorPool,
        limit: i64,
    ) -> Result<Vec<DataTenantId>, SqlxError> {
        let rows: Vec<Uuid> = sqlx::query_scalar(DUE_TENANTS_SQL)
            .bind(limit)
            .fetch_all(operator.pool())
            .await?;
        rows.into_iter()
            .map(|tenant| stored(DataTenantId::new(tenant)))
            .collect()
    }

    /// Count non-terminal runs and dispatches by status across every tenant.
    ///
    /// Six aggregate counts — no tenant or run labels — so telemetry stays
    /// bounded regardless of tenant count.
    ///
    /// # Errors
    /// Returns the database error when the read fails.
    // tenant-isolation: cross-tenant OperatorPool
    pub async fn queue_depth(&self, operator: &OperatorPool) -> Result<QueueDepth, SqlxError> {
        let (runs_pending, runs_retrying, runs_running, pending, retrying, running): (
            i64,
            i64,
            i64,
            i64,
            i64,
            i64,
        ) = sqlx::query_as(QUEUE_DEPTH_SQL)
            .fetch_one(operator.pool())
            .await?;
        Ok(QueueDepth {
            runs: QueueCounts {
                pending: runs_pending,
                retrying: runs_retrying,
                running: runs_running,
            },
            dispatches: QueueCounts {
                pending,
                retrying,
                running,
            },
        })
    }
}

/// A resolved target row from [`RESOLVE_BINDING_SQL`] or [`RESOLVE_DIRECT_SQL`].
#[derive(sqlx::FromRow)]
struct ResolvedTarget {
    /// Exact Verifier Card UID.
    verifier_uid: Uuid,
    /// Verifier Card version; absent when the Card does not exist.
    verifier_version: Option<String>,
    /// Verifier `implementation.kind`; absent when the Card does not exist.
    implementation: Option<String>,
    /// Stored [`VerifierReadiness`] name.
    readiness: String,
    /// Exact subject Card UID.
    subject_card_uid: Uuid,
    /// Whether the subject Card exists and is active.
    subject_available: bool,
    /// Binding owner; absent for a direct target.
    owner_card_uid: Option<Uuid>,
    /// Binding; absent for a direct target.
    binding_id: Option<Uuid>,
    /// Frozen Trigger Card UID.
    trigger_uid: Option<Uuid>,
    /// Frozen inline Trigger digest.
    trigger_digest: Option<String>,
    /// Frozen Operators as stored JSON.
    operators: Json<Value>,
}

impl ResolvedTarget {
    /// Decide whether this target refuses `input`, in precedence order:
    /// readiness, subject availability, then implementation compatibility.
    ///
    /// # Errors
    /// Returns [`SqlxError::Decode`] when the stored readiness is unknown.
    fn refusal(&self, input: &RunInput) -> Result<Option<EnqueueRefusal>, SqlxError> {
        let readiness: VerifierReadiness = stored(self.readiness.parse())?;
        if readiness != VerifierReadiness::Ready {
            return Ok(Some(EnqueueRefusal::NotReady(readiness)));
        }
        if !self.subject_available {
            return Ok(Some(EnqueueRefusal::SubjectUnavailable));
        }
        if self.implementation.as_deref() != Some(input.implementation()) {
            return Ok(Some(EnqueueRefusal::InputMismatch));
        }
        Ok(None)
    }
}

/// A claimed run row from [`CLAIM_RUN_SQL`].
#[derive(sqlx::FromRow)]
struct ClaimedRunRow {
    /// Run identity.
    run_id: Uuid,
    /// Verifier Card UID.
    verifier_uid: Uuid,
    /// Verifier Card version.
    verifier_version: String,
    /// Subject Card UID.
    subject_card_uid: Uuid,
    /// Stored origin.
    origin: String,
    /// Binding owner.
    owner_card_uid: Option<Uuid>,
    /// Binding.
    binding_id: Option<Uuid>,
    /// Frozen Trigger Card UID.
    trigger_uid: Option<Uuid>,
    /// Frozen inline Trigger digest.
    trigger_digest: Option<String>,
    /// Drift window start.
    window_start: Option<DateTime<Utc>>,
    /// Drift window end.
    window_end: Option<DateTime<Utc>>,
    /// Eval record ID.
    input_record_id: Option<String>,
    /// Eval record event time.
    input_event_time: Option<DateTime<Utc>>,
    /// Manual requester.
    requested_by_principal_id: Option<Uuid>,
    /// Attempt number of this claim.
    attempts: i32,
    /// Attempt budget.
    max_attempts: i32,
    /// Lease expiry.
    lease_expires_at: DateTime<Utc>,
}

impl ClaimedRunRow {
    /// Convert raw columns into a typed claim holding `token`.
    ///
    /// # Errors
    /// Returns [`SqlxError::Decode`] when an identity is not `UUIDv7` or the
    /// origin or input columns violate their closed shapes.
    fn into_claimed(self, token: LeaseToken) -> Result<ClaimedRun, SqlxError> {
        let input = match (
            self.window_start,
            self.window_end,
            self.input_record_id,
            self.input_event_time,
        ) {
            (Some(start), Some(end), None, None) => {
                RunInput::DriftWindow(DriftWindow { start, end })
            }
            (None, None, Some(record_id), Some(event_time)) => RunInput::EvalRecord {
                record_id,
                event_time,
            },
            _ => {
                return Err(SqlxError::Decode(
                    "verifier run has no single frozen input".into(),
                ));
            }
        };
        let trigger = match (self.trigger_uid, self.trigger_digest) {
            (Some(uid), _) => Some(FrozenTarget::Uid(stored(CardUid::from_uuid(uid))?)),
            (None, Some(digest)) => Some(FrozenTarget::Digest(digest)),
            (None, None) => None,
        };
        Ok(ClaimedRun {
            lease: RunLease {
                run_id: stored(VerificationRunId::new(self.run_id))?,
                token,
            },
            lease_expires_at: self.lease_expires_at,
            attempt: self.attempts,
            max_attempts: self.max_attempts,
            verifier_uid: stored(CardUid::from_uuid(self.verifier_uid))?,
            verifier_version: self.verifier_version,
            subject_card_uid: stored(CardUid::from_uuid(self.subject_card_uid))?,
            origin: RunOrigin::parse(&self.origin)?,
            owner_card_uid: self
                .owner_card_uid
                .map(|uid| stored(CardUid::from_uuid(uid)))
                .transpose()?,
            binding_id: self
                .binding_id
                .map(|id| stored(BindingId::new(id)))
                .transpose()?,
            trigger,
            input,
            requested_by: self.requested_by_principal_id.map(PrincipalId::new),
        })
    }
}

/// A locked due binding from [`DUE_BINDING_SQL`], carrying the database
/// instant that anchors the whole tick.
#[derive(sqlx::FromRow)]
struct DueBindingRow {
    /// Binding identity.
    binding_id: Uuid,
    /// Stored cron expression.
    schedule_cron: String,
    /// Stored IANA time zone, when the schedule names one.
    schedule_tz: Option<String>,
    /// Cursor the occurrence is due at.
    next_run_at: DateTime<Utc>,
    /// PostgreSQL's statement instant for this tick.
    now: DateTime<Utc>,
}

/// A run status row from [`RUN_STATUS_SQL`].
#[derive(sqlx::FromRow)]
struct RunStatusRow {
    /// Run identity.
    run_id: Uuid,
    /// Stored execution status.
    status: String,
    /// Manual requester.
    requested_by_principal_id: Option<Uuid>,
    /// Result pointer.
    result_id: Option<Uuid>,
    /// Last execution error.
    error: Option<Json<VerificationError>>,
}

/// A dispatch row from [`RUN_DISPATCHES_SQL`].
#[derive(sqlx::FromRow)]
struct DispatchRow {
    /// Dispatch identity.
    dispatch_id: Uuid,
    /// Frozen Operator Card UID.
    operator_uid: Option<Uuid>,
    /// Frozen inline Operator digest.
    operator_digest: Option<String>,
    /// Stored delivery status.
    status: String,
    /// Last delivery error.
    last_error: Option<Json<VerificationError>>,
}

impl DispatchRow {
    /// Convert raw columns into the Run GET dispatch item.
    ///
    /// # Errors
    /// Returns [`SqlxError::Decode`] when an identity or status is malformed.
    fn into_state(self) -> Result<OperatorDispatchState, SqlxError> {
        let operator = match (self.operator_uid, self.operator_digest) {
            (Some(uid), None) => FrozenTarget::Uid(stored(CardUid::from_uuid(uid))?),
            (None, Some(digest)) => FrozenTarget::Digest(digest),
            _ => {
                return Err(SqlxError::Decode(
                    "operator dispatch has no single frozen Operator".into(),
                ));
            }
        };
        Ok(OperatorDispatchState {
            dispatch_id: stored(OperatorDispatchId::new(self.dispatch_id))?,
            operator,
            status: stored(self.status.parse())?,
            error: self.last_error.map(|Json(error)| error),
        })
    }
}

/// Map a fenced update's row count to its settlement outcome.
fn settlement(rows_affected: u64) -> Settlement {
    if rows_affected == 0 {
        Settlement::StaleLease
    } else {
        Settlement::Applied
    }
}

/// Decode a stored value, surfacing a malformed one as [`SqlxError::Decode`].
///
/// # Errors
/// Returns [`SqlxError::Decode`] wrapping `value`'s error.
fn stored<T, E>(value: Result<T, E>) -> Result<T, SqlxError>
where
    E: StdError + Send + Sync + 'static,
{
    value.map_err(|error| SqlxError::Decode(Box::new(error)))
}

#[cfg(test)]
mod tests {
    //! Pure retry-policy arithmetic.

    use super::*;

    /// The default policy allows three attempts with bounded doubling delays.
    ///
    /// # Panics
    /// Panics when a delay is not doubled from 30 seconds or exceeds the cap.
    #[test]
    fn retry_delays_double_and_cap() {
        let policy = RunRetryPolicy::default();
        assert_eq!(policy.max_attempts, 3);
        assert_eq!(policy.delay_after(1), Duration::seconds(30));
        assert_eq!(policy.delay_after(2), Duration::seconds(60));
        assert_eq!(policy.delay_after(3), Duration::minutes(2));
        assert_eq!(policy.delay_after(i32::MAX), Duration::minutes(2));
        assert_eq!(policy.delay_after(0), Duration::seconds(30));
    }
}
