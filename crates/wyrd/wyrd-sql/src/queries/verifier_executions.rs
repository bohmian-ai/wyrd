//! Verifier execution queue: acceptance, scheduling, claims, settlement, and status.
//!
//! `wyrd.verifier_executions` is the only Verifier execution queue and
//! `wyrd.operator_dispatches` the durable handoff to the Operator worker.
//! [`VerifierExecutionQueue`] owns the retry policy and activity window every
//! transition applies, and each tenant operation runs on the caller's
//! [`TenantConn`] under forced RLS without committing, so the batched
//! submission writer accepts a whole tenant batch in one transaction and the
//! scheduler can create an execution and advance its cursor in one
//! transaction. Cross-tenant work discovery and queue-depth telemetry are
//! explicit [`OperatorPool`] reads.
//!
//! Lifecycle: `pending` → (claim) `running` → `completed` | `retrying` →
//! `running` … | `errored` | `timed_out` | `cancelled`. A claim mints a fresh
//! lease token; every settlement is fenced on it, so a worker whose lease was
//! reclaimed affects zero rows and observes [`Settlement::StaleLease`].
// raw-query grep allowlist: verifier execution tables post-date the sqlx offline cache; run `mise run sqlx:prepare` to promote to macros.

use std::collections::BTreeMap;
use std::error::Error as StdError;

use chrono::{DateTime, Duration, Utc};
use serde_json::Value;
use sha2::{Digest, Sha256};
use sqlx::Error as SqlxError;
use sqlx::types::{Json, Uuid};
use wyrd_runtime::AcceptedAuthority;
use wyrd_runtime::principal::PrincipalId;
use wyrd_spec::DataTenantId;
use wyrd_spec::auth::Attribution;
use wyrd_spec::card::agent::ExecutionRoute;
use wyrd_spec::card::operator::{DriftCounts, MAX_SUMMARY_CHARS};
use wyrd_spec::card::verifier::ScheduledInput;
use wyrd_spec::error::WyrdError;
use wyrd_spec::ids::{
    BindingId, CardUid, ExecutionId, OperatorDispatchId, SlotName, VerificationResultId,
};
use wyrd_spec::vala::ids::{RunId, SpanId, TraceId};
use wyrd_spec::verification::TaskId;
use wyrd_spec::verification::execution::{
    InputDeclaration, VerificationExecutionState, VerificationRequest,
};
use wyrd_spec::verification::{
    DriftWindow, FrozenTarget, OperatorDispatchState, VerificationBindingStatus, VerificationError,
    VerificationVerdict, VerifierReadiness,
};

use crate::queries::verification::{BindingSchedule, InactivityTimeout, binding_activity};
use crate::{OperatorPool, TenantConn};

/// Resolve a binding target's frozen identities and readiness.
const RESOLVE_BINDING_SQL: &str = r"
    SELECT b.verifier_card_uid, v.version AS verifier_version,
           wyrd.verifier_readiness(b.verifier_card_uid) AS readiness,
           b.card_uid,
           EXISTS (SELECT 1 FROM wyrd.cards s
                    WHERE s.card_uid = b.card_uid
                      AND s.status IN ('active', 'deprecated')) AS subject_available,
           b.owner_card_uid, b.binding_id, b.trigger_uid, b.trigger_digest, b.operators
      FROM wyrd.verification_bindings b
      LEFT JOIN wyrd.cards v ON v.card_uid = b.verifier_card_uid
     WHERE b.binding_id = $1
";

/// Resolve a direct target: no owner, binding, Trigger, or Operators.
const RESOLVE_DIRECT_SQL: &str = r"
    SELECT p.verifier_card_uid, v.version AS verifier_version,
           wyrd.verifier_readiness(p.verifier_card_uid) AS readiness,
           $2::uuid AS card_uid,
           EXISTS (SELECT 1 FROM wyrd.cards s
                    WHERE s.card_uid = $2
                      AND s.status IN ('active', 'deprecated')) AS subject_available,
           NULL::uuid AS owner_card_uid, NULL::uuid AS binding_id,
           NULL::uuid AS trigger_uid, NULL::text AS trigger_digest,
           '[]'::jsonb AS operators
      FROM (SELECT $1::uuid AS verifier_card_uid) p
      LEFT JOIN wyrd.cards v ON v.card_uid = p.verifier_card_uid
";

/// Insert one scheduled execution, due at PostgreSQL's statement time; a
/// duplicate occurrence conflicts on its partial unique index and inserts
/// nothing.
///
/// `$12`..`$14` are the binding owner as subject, the SYSTEM actor, and the
/// owner's accepted authority, frozen from the [`ExecutionAuthority`]. The
/// execution becomes claimable on the same clock the claim predicate reads,
/// so no scheduling process can write work into the queue's future or past.
const INSERT_SCHEDULED_SQL: &str = r"
    INSERT INTO wyrd.verifier_executions (
        execution_id, data_tenant_id, verifier_card_uid, verifier_version, card_uid,
        origin, owner_card_uid, binding_id, trigger_uid, trigger_digest, operators,
        window_start, window_end, sub_principal_id, act_principal_id, authority,
        max_attempts, next_attempt_at, created_at, updated_at
    ) VALUES ($1, wyrd.current_tenant(), $2, $3, $4, 'schedule', $5, $6, $7, $8, $9,
              $10, $11, $12, $13, $14, $15,
              statement_timestamp(), statement_timestamp(), statement_timestamp())
    ON CONFLICT DO NOTHING
    RETURNING execution_id
";

/// Find the execution that already holds a binding's scheduled occurrence.
const EXISTING_OCCURRENCE_SQL: &str = r"
    SELECT execution_id
      FROM wyrd.verifier_executions
     WHERE binding_id = $1
       AND origin = 'schedule'
       AND window_end = $2
";

/// Insert one submitted execution, either `pending` and due at PostgreSQL's
/// statement time or, when `$15` carries an error, already settled `errored`.
///
/// The exact Verifier version and subject come from their Card rows, and a
/// Card the tenant does not hold selects no row, so the statement inserts
/// nothing rather than failing the batch on a foreign key. An existing
/// execution ID inserts nothing, which absorbs a retried batch.
const INSERT_SUBMISSION_SQL: &str = r"
    INSERT INTO wyrd.verifier_executions (
        execution_id, data_tenant_id, verifier_card_uid, verifier_version, card_uid,
        origin, run_id, inputs, route_overrides,
        sub_principal_id, act_principal_id, authority, cause_trace_id, cause_span_id,
        idempotency_key, request_sha256, status, error, max_attempts,
        next_attempt_at, settled_at, created_at, updated_at, cause_sampled
    )
    SELECT $1, wyrd.current_tenant(), v.card_uid, v.version, s.card_uid,
           'manual', $4, $5, $6, $7, $8, $9, $10, $11, $12, $13,
           CASE WHEN $15::jsonb IS NULL THEN 'pending' ELSE 'errored' END, $15, $14,
           CASE WHEN $15::jsonb IS NULL THEN statement_timestamp() END,
           CASE WHEN $15::jsonb IS NOT NULL THEN statement_timestamp() END,
           statement_timestamp(), statement_timestamp(), $16
      FROM wyrd.cards v, wyrd.cards s
     WHERE v.card_uid = $2 AND s.card_uid = $3
    ON CONFLICT DO NOTHING
";

/// Whether an execution with this ID already exists in the caller's tenant.
const EXECUTION_EXISTS_SQL: &str = r"
    SELECT EXISTS (SELECT 1 FROM wyrd.verifier_executions WHERE execution_id = $1)
";

/// Serialize concurrent submissions that share one subject, actor, and key.
///
/// Transaction-scoped, so it releases on commit or rollback; the class keeps
/// these locks apart from every other advisory lock in the database.
const LOCK_REQUEST_KEY_SQL: &str = r"
    SELECT pg_advisory_xact_lock($1, hashtext($2::text || '/' || $3::text || '/' || $4))
";

/// Advisory lock class of [`LOCK_REQUEST_KEY_SQL`].
const REQUEST_KEY_LOCK_CLASS: i32 = 0x0C_A2_D0_31;

/// Find the execution one subject and actor already had accepted under one key.
const KEYED_EXECUTION_SQL: &str = r"
    SELECT execution_id, request_sha256
      FROM wyrd.verifier_executions
     WHERE sub_principal_id = $1
       AND act_principal_id = $2
       AND idempotency_key = $3
";

/// How long after a scheduled window ends its occurrence becomes claimable.
///
/// An occurrence whose window ends at `T` is claimed only once PostgreSQL's
/// statement time reaches `T` plus this delay, so observations stamped just
/// before `T` that are still being ingested land inside the window they
/// belong to. The window itself stays `[start, T)`. Bound as milliseconds
/// into [`DUE_BINDING_SQL`] and [`DUE_TENANTS_SQL`], which derive the cutoff
/// from `statement_timestamp()`. A fixed value, not configuration.
const SCHEDULE_CLAIM_DELAY: Duration = Duration::seconds(30);

/// Briefly lock the earliest due scheduled binding no other scheduler holds.
///
/// Dueness is decided against PostgreSQL's statement time: a cursor is due
/// once it is at least `$1` milliseconds ([`SCHEDULE_CLAIM_DELAY`]) in the
/// past. The same statement returns statement time minus that delay as the
/// schedule clock, so the synchronous cron calculation anchors on the database
/// clock rather than the scheduler process's, and judges missed occurrences
/// and the next cursor on the same delayed clock that decides dueness.
const DUE_BINDING_SQL: &str = r"
    SELECT binding_id, schedule_cron, schedule_tz, next_run_at,
           statement_timestamp() - ($1::bigint * INTERVAL '1 millisecond') AS schedule_clock
      FROM wyrd.verification_bindings
     WHERE activation = 'schedule'
       AND next_run_at + ($1::bigint * INTERVAL '1 millisecond') <= statement_timestamp()
     ORDER BY next_run_at, binding_id
     LIMIT 1
       FOR UPDATE SKIP LOCKED
";

/// Move a claimed binding's cursor; NULL disarms an unparseable schedule.
const ADVANCE_CURSOR_SQL: &str = r"
    UPDATE wyrd.verification_bindings
       SET next_run_at = $2
     WHERE binding_id = $1
";

/// Settle every expired lease that has no attempt left as `errored`.
///
/// Expiry is decided and stamped by PostgreSQL, so a lease written by one pod
/// is never judged against another pod's clock.
const EXHAUST_EXPIRED_SQL: &str = r"
    UPDATE wyrd.verifier_executions e
       SET status = 'errored', error = $1, lease_expires_at = NULL,
           next_attempt_at = NULL, settled_at = statement_timestamp(),
           updated_at = statement_timestamp()
     WHERE e.status = 'running'
       AND e.lease_expires_at <= statement_timestamp()
       AND e.attempts >= e.max_attempts
";

/// Claim the oldest due, retry-due, or lease-expired execution under a fresh lease.
///
/// Availability, expiry, and the new lease deadline are all PostgreSQL's
/// statement time; `$2` is only the lease length in milliseconds, so a pod
/// whose clock differs from the database can neither claim early nor hold a
/// lease the database believes already expired. The claim also returns the
/// tenant's active SYSTEM principal, the exact Verifier Card's status (`NULL`
/// when the Card is absent), the binding's scheduled input selectors, and the
/// Verifier's ready fitted Drift baselines keyed by requirement, so no later
/// phase opens a connection to read them.
const CLAIM_SQL: &str = r"
    WITH candidate AS (
        SELECT execution_id, COALESCE(next_attempt_at, lease_expires_at) AS due_at
          FROM wyrd.verifier_executions
         WHERE (status IN ('pending', 'retrying') AND next_attempt_at <= statement_timestamp())
            OR (status = 'running' AND lease_expires_at <= statement_timestamp())
         ORDER BY COALESCE(next_attempt_at, lease_expires_at), execution_id
         LIMIT 1
           FOR UPDATE SKIP LOCKED
    )
    UPDATE wyrd.verifier_executions e
       SET status = 'running', attempts = e.attempts + 1, lease_token = $1,
           lease_expires_at = statement_timestamp() + ($2::bigint * INTERVAL '1 millisecond'),
           next_attempt_at = NULL, updated_at = statement_timestamp()
      FROM candidate
     WHERE e.execution_id = candidate.execution_id
    RETURNING e.execution_id, e.verifier_card_uid, e.verifier_version, e.card_uid,
              e.origin, e.owner_card_uid, e.binding_id, e.trigger_uid,
              e.trigger_digest, e.window_start, e.window_end,
              e.run_id, e.inputs, e.route_overrides,
              e.sub_principal_id, e.act_principal_id, e.authority, e.attempts,
              e.max_attempts, e.lease_expires_at, e.cause_trace_id, e.cause_span_id,
              e.cause_sampled,
              GREATEST(0, (EXTRACT(EPOCH FROM statement_timestamp() - candidate.due_at)
                           * 1000)::bigint) AS queue_wait_ms,
              GREATEST(0, (EXTRACT(EPOCH FROM statement_timestamp() - e.created_at)
                           * 1000)::bigint) AS age_ms,
              (SELECT a.id FROM wyrd.auth_service_accounts a
                WHERE a.principal_kind = 'system' AND a.status = 'active') AS system_principal_id,
              (SELECT c.status FROM wyrd.cards c
                WHERE c.card_uid = e.verifier_card_uid) AS verifier_status,
              (SELECT b.inputs FROM wyrd.verification_bindings b
                WHERE b.binding_id = e.binding_id) AS schedule_inputs,
              (SELECT jsonb_object_agg(f.requirement_id, f.fitted)
                 FROM wyrd.drift_baselines f
                WHERE f.verifier_uid = e.verifier_card_uid
                  AND f.state = 'ready'
                  AND f.requirement_id IS NOT NULL) AS fitted_baselines
";

/// Renew every listed lease still held, on the database clock.
///
/// `$2` is the lease length and `$3` the remaining time under which a lease is
/// renewed, both in milliseconds, so a lease is extended only once a third of
/// it has passed. Every unexpired lease still held is returned, renewed or
/// not; a token absent from the result has expired, been reclaimed, or been
/// settled.
const RENEW_LEASES_SQL: &str = r"
    UPDATE wyrd.verifier_executions
       SET lease_expires_at = CASE
               WHEN lease_expires_at <= statement_timestamp() + ($3::bigint * INTERVAL '1 millisecond')
               THEN statement_timestamp() + ($2::bigint * INTERVAL '1 millisecond')
               ELSE lease_expires_at
           END,
           updated_at = statement_timestamp()
     WHERE status = 'running' AND lease_token = ANY($1)
       AND lease_expires_at > statement_timestamp()
    RETURNING lease_token
";

/// Complete a leased execution, or re-apply the same completion idempotently.
const COMPLETE_SQL: &str = r"
    UPDATE wyrd.verifier_executions
       SET status = 'completed', result_id = $3, error = NULL,
           lease_expires_at = NULL, next_attempt_at = NULL,
           settled_at = COALESCE(settled_at, statement_timestamp()),
           updated_at = statement_timestamp()
     WHERE execution_id = $1
       AND lease_token = $2
       AND (status = 'running' OR (status = 'completed' AND result_id = $3))
    RETURNING binding_id, operators
";

/// Insert one Operator dispatch; the (tenant, execution, Operator) key absorbs retries.
///
/// The failure context is frozen here from the settled execution and its
/// exact Verifier and subject Cards plus the runner's bounded summary (`$5`)
/// and, for a Verifier with one drift check, that check's counts (`$6`), so
/// every delivery attempt renders the same payload.
const INSERT_DISPATCH_SQL: &str = r"
    INSERT INTO wyrd.operator_dispatches (
        dispatch_id, data_tenant_id, execution_id, operator_uid, operator_digest,
        failure_context, next_attempt_at, created_at, updated_at
    )
    SELECT $1, e.data_tenant_id, e.execution_id, $3, $4,
           jsonb_build_object(
               'dispatch_id', $1::uuid,
               'execution_id', e.execution_id,
               'result_id', e.result_id,
               'binding_id', e.binding_id,
               'verifier_card_uid', e.verifier_card_uid,
               'verifier_ref', v.space || '/' || v.name || '@' || v.version,
               'subject_uid', e.card_uid,
               'subject_ref', s.space || '/' || s.name || '@' || s.version,
               'verdict', 'failed',
               'completed_at', e.settled_at,
               'summary', $5::text
           ) || CASE WHEN $6::jsonb IS NULL THEN '{}'::jsonb
                     ELSE jsonb_build_object('drift', $6::jsonb) END,
           statement_timestamp(), statement_timestamp(), statement_timestamp()
      FROM wyrd.verifier_executions e
      JOIN wyrd.cards v ON v.card_uid = e.verifier_card_uid
      JOIN wyrd.cards s ON s.card_uid = e.card_uid
     WHERE e.execution_id = $2
    ON CONFLICT DO NOTHING
";

/// Lock a leased execution's attempt budget before deciding retry or exhaustion.
const LEASED_ATTEMPTS_SQL: &str = r"
    SELECT attempts, max_attempts
      FROM wyrd.verifier_executions
     WHERE execution_id = $1 AND lease_token = $2 AND status = 'running'
       FOR UPDATE
";

/// Schedule a leased execution's next attempt with its last error.
///
/// `$4` is the backoff in milliseconds; the deadline itself is PostgreSQL's
/// statement time plus that delay, and the statement returns it so the caller
/// reports the stored instant rather than a locally derived one.
const RETRY_SQL: &str = r"
    UPDATE wyrd.verifier_executions
       SET status = 'retrying', error = $3,
           next_attempt_at = statement_timestamp() + ($4::bigint * INTERVAL '1 millisecond'),
           lease_expires_at = NULL, updated_at = statement_timestamp()
     WHERE execution_id = $1 AND lease_token = $2 AND status = 'running'
    RETURNING next_attempt_at
";

/// Settle a leased execution in a terminal non-completed status.
const TERMINATE_SQL: &str = r"
    UPDATE wyrd.verifier_executions
       SET status = $3, error = $4, lease_expires_at = NULL,
           next_attempt_at = NULL, settled_at = statement_timestamp(),
           updated_at = statement_timestamp()
     WHERE execution_id = $1 AND lease_token = $2 AND status = 'running'
";

/// Return a leased execution to the queue, refunding its attempt; `$3` is the
/// delay in milliseconds before it is due again.
const RELEASE_SQL: &str = r"
    UPDATE wyrd.verifier_executions
       SET status = CASE WHEN attempts > 1 THEN 'retrying' ELSE 'pending' END,
           attempts = attempts - 1, lease_expires_at = NULL,
           next_attempt_at = statement_timestamp() + ($3::bigint * INTERVAL '1 millisecond'),
           updated_at = statement_timestamp()
     WHERE execution_id = $1 AND lease_token = $2 AND status = 'running'
";

/// Read one execution's control-plane status.
const STATUS_SQL: &str = r"
    SELECT execution_id, status, sub_principal_id, act_principal_id,
           verifier_card_uid, card_uid, result_id, error
      FROM wyrd.verifier_executions
     WHERE execution_id = $1
";

/// Read one execution's dispatches in identity order.
const DISPATCHES_SQL: &str = r"
    SELECT dispatch_id, operator_uid, operator_digest, status, last_error
      FROM wyrd.operator_dispatches
     WHERE execution_id = $1
     ORDER BY dispatch_id
";

/// Read a binding's frozen targets, readiness, and latest admitted execution.
///
/// Admission order is creation time, then identity; a retry keeps its row,
/// so it never becomes a newer admission.
const BINDING_TARGETS_SQL: &str = r"
    SELECT b.card_uid, b.verifier_card_uid,
           wyrd.verifier_readiness(b.verifier_card_uid) AS readiness,
           (SELECT e.execution_id FROM wyrd.verifier_executions e
             WHERE e.binding_id = b.binding_id
             ORDER BY e.created_at DESC, e.execution_id DESC LIMIT 1) AS last_execution_id
      FROM wyrd.verification_bindings b
     WHERE b.binding_id = $1
";

/// List every tenant with claimable executions, longest-waiting tenant first.
const RUNNABLE_TENANTS_SQL: &str = r"
    SELECT data_tenant_id
      FROM wyrd.verifier_executions
     WHERE (status IN ('pending', 'retrying') AND next_attempt_at <= statement_timestamp())
        OR (status = 'running' AND lease_expires_at <= statement_timestamp())
     GROUP BY data_tenant_id
     ORDER BY min(COALESCE(next_attempt_at, lease_expires_at)), data_tenant_id
";

/// List tenants with due scheduled bindings, most overdue tenant first.
///
/// Uses the same dueness rule as [`DUE_BINDING_SQL`]: `$2` is
/// [`SCHEDULE_CLAIM_DELAY`] in milliseconds.
const DUE_TENANTS_SQL: &str = r"
    SELECT data_tenant_id
      FROM wyrd.verification_bindings
     WHERE activation = 'schedule'
       AND next_run_at + ($2::bigint * INTERVAL '1 millisecond') <= statement_timestamp()
     GROUP BY data_tenant_id
     ORDER BY min(next_run_at), data_tenant_id
     LIMIT $1
";

/// Count non-terminal executions and dispatches by status across every tenant.
const QUEUE_DEPTH_SQL: &str = r"
    SELECT
        (SELECT count(*) FROM wyrd.verifier_executions WHERE status = 'pending') AS executions_pending,
        (SELECT count(*) FROM wyrd.verifier_executions WHERE status = 'retrying') AS executions_retrying,
        (SELECT count(*) FROM wyrd.verifier_executions WHERE status = 'running') AS executions_running,
        (SELECT count(*) FROM wyrd.operator_dispatches WHERE status = 'pending') AS dispatches_pending,
        (SELECT count(*) FROM wyrd.operator_dispatches WHERE status = 'retrying') AS dispatches_retrying,
        (SELECT count(*) FROM wyrd.operator_dispatches WHERE status = 'running') AS dispatches_running
";

/// Why an execution was created.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutionOrigin {
    /// A due cron occurrence of a scheduled binding.
    Schedule,
    /// A caller's `POST /v1/verification/enqueue` submission.
    Manual,
}

impl ExecutionOrigin {
    /// Stored `origin` column value.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Schedule => "schedule",
            Self::Manual => "manual",
        }
    }

    /// Decode a stored `origin` value of claimable work.
    ///
    /// # Errors
    /// Returns [`SqlxError::Decode`] for a value outside the live set,
    /// including the retired `observation` origin, which no claimable row has.
    fn parse(value: &str) -> Result<Self, SqlxError> {
        match value {
            "schedule" => Ok(Self::Schedule),
            "manual" => Ok(Self::Manual),
            other => Err(SqlxError::Decode(
                format!("unknown verifier execution origin {other:?}").into(),
            )),
        }
    }
}

/// The frozen input one claimed execution judges.
#[derive(Debug, Clone, PartialEq)]
pub enum ExecutionInput {
    /// A scheduled occurrence: its fixed window and the binding's selector
    /// for each samples slot.
    Scheduled {
        /// The occurrence's comparison window.
        window: DriftWindow,
        /// Recorded measurements filling each samples slot within `window`.
        slots: BTreeMap<SlotName, ScheduledInput>,
    },
    /// A caller submission, exactly as it was accepted.
    Submitted {
        /// Application Run the execution belongs to, when the caller named one.
        run_id: Option<RunId>,
        /// One declaration per named input slot.
        inputs: BTreeMap<SlotName, InputDeclaration>,
        /// Per-task execution route overrides.
        route_overrides: BTreeMap<TaskId, ExecutionRoute>,
    },
}

/// Who one scheduled execution serves, who accepted it, and the authority it
/// executes under.
///
/// Work Wyrd starts itself serves the binding owner under the owner's
/// resolved authority with the tenant SYSTEM principal as actor; SYSTEM is
/// never the subject and grants nothing. Frozen on the execution at
/// scheduling and never changed by claim or retry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutionAuthority {
    /// Stored `sub_principal_id`/`act_principal_id`.
    attribution: Attribution,
    /// Stored server-only effective authority.
    authority: AcceptedAuthority,
}

impl ExecutionAuthority {
    /// A scheduled execution serving the binding owner, whose current
    /// authority is `owner`, executed by the tenant SYSTEM `system`.
    #[must_use]
    pub const fn background(owner: AcceptedAuthority, system: PrincipalId) -> Self {
        Self {
            attribution: Attribution::new(owner.principal.id, system),
            authority: owner,
        }
    }

    /// The execution's subject and accepting actor.
    #[must_use]
    pub const fn attribution(&self) -> Attribution {
        self.attribution
    }

    /// The effective authority the execution runs under.
    #[must_use]
    pub const fn authority(&self) -> &AcceptedAuthority {
        &self.authority
    }
}

/// Why a scheduled occurrence or submission cannot become runnable work.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnqueueRefusal {
    /// The binding does not exist in the caller's tenant.
    BindingNotFound,
    /// The Verifier cannot run: `VerifierUnavailable` means the target is not
    /// an active building-block Verifier Card; `BaselineNotReady` means a
    /// PSI or SPC drift check has no ready fitted baseline.
    NotReady(VerifierReadiness),
    /// The subject Card does not exist or is not active in the tenant.
    SubjectUnavailable,
}

impl EnqueueRefusal {
    /// The structured error a refused submission settles `errored` with.
    ///
    /// The code is the public catalog code a direct caller would have been
    /// refused with, so `status` and `wait` surface the same error.
    #[must_use]
    pub fn error(self) -> VerificationError {
        let message = match self {
            Self::BindingNotFound => "no verification binding with this ID in the caller's tenant",
            Self::NotReady(VerifierReadiness::BaselineNotReady) => {
                "a drift check of the Verifier has no ready fitted baseline"
            }
            Self::NotReady(_) => "the target is not an active Verifier Card",
            Self::SubjectUnavailable => "the subject Card is not active in the caller's tenant",
        };
        let error = match self {
            Self::BindingNotFound => WyrdError::VerificationBindingNotFound {
                message: message.to_owned(),
                details: Value::Null,
            },
            Self::NotReady(VerifierReadiness::BaselineNotReady) => {
                WyrdError::VerificationBaselineNotReady {
                    message: message.to_owned(),
                    details: Value::Null,
                }
            }
            Self::NotReady(_) | Self::SubjectUnavailable => WyrdError::VerificationInvalidTarget {
                message: message.to_owned(),
                details: Value::Null,
            },
        };
        VerificationError {
            code: error.code().to_owned(),
            message: message.to_owned(),
        }
    }
}

/// Result of scheduling one occurrence through [`VerifierExecutionQueue::enqueue_scheduled`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnqueueOutcome {
    /// A new pending execution was created.
    Enqueued(ExecutionId),
    /// The scheduled occurrence already has this execution.
    AlreadyEnqueued(ExecutionId),
    /// Nothing was created.
    Refused(EnqueueRefusal),
}

/// A caller's `Idempotency-Key` for one submission and that request's digest.
///
/// The key is scoped to the execution's tenant, subject, and actor. The digest
/// is the SHA-256 of the request's RFC 8785 canonical JSON; a retry must match
/// it to be a replay.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequestKey {
    /// Caller-supplied `Idempotency-Key` value.
    pub key: String,
    /// SHA-256 of the canonical request body.
    pub request_sha256: [u8; 32],
}

impl RequestKey {
    /// Pair `key` with the canonical digest of `request`.
    ///
    /// # Errors
    /// Returns the serialization error when `request` cannot be canonicalized.
    pub fn new(key: String, request: &VerificationRequest) -> Result<Self, serde_json::Error> {
        let canonical = serde_jcs::to_vec(request)?;
        Ok(Self {
            key,
            request_sha256: Sha256::digest(canonical).into(),
        })
    }

    /// The execution ID every identical keyed request of `attribution` in
    /// `tenant` receives.
    ///
    /// Derived, without reading Postgres, from a domain-separated SHA-256
    /// over the tenant, subject, actor, length-prefixed key, and request
    /// digest, shaped as a `UUIDv7`. A transport retry therefore learns the
    /// same ID before the queue writer accepts the submission, while a
    /// request with different content under the same key gets a different
    /// ID, so its conflict row never overwrites the original execution.
    ///
    /// # Panics
    /// Never in practice: the builder always sets the version the
    /// [`ExecutionId`] constructor requires.
    #[must_use]
    pub fn execution_id(&self, tenant: DataTenantId, attribution: Attribution) -> ExecutionId {
        let mut hasher = Sha256::new();
        hasher.update(b"wyrd.verification.execution.v1");
        hasher.update(tenant.as_uuid().as_bytes());
        hasher.update(attribution.sub_principal_id.as_uuid().as_bytes());
        hasher.update(attribution.act_principal_id.as_uuid().as_bytes());
        hasher.update((self.key.len() as u64).to_be_bytes());
        hasher.update(self.key.as_bytes());
        hasher.update(self.request_sha256);
        let digest = hasher.finalize();
        let mut bytes = [0_u8; 16];
        bytes.copy_from_slice(&digest[..16]);
        let id = uuid::Builder::from_random_bytes(bytes)
            .with_version(uuid::Version::SortRand)
            .into_uuid();
        ExecutionId::new(id).expect("invariant: the builder sets UUID version 7")
    }
}

/// The request span a manual execution was submitted under, which its
/// attempts link to without adopting as their parent.
///
/// `sampled` is the caller's own sampling decision for the span that request
/// continued, not the server's: it decides whether the linked attempts reach
/// the tenant's trace copy, while the operator samples them by its own policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cause {
    /// Trace of the submitting request.
    pub trace_id: TraceId,
    /// The submitting request's span.
    pub span_id: SpanId,
    /// Whether the caller sampled the trace the request continued.
    pub sampled: bool,
}

/// One staged caller submission, handed to the batched queue writer once
/// Scribe has acknowledged its inputs.
///
/// Carries the accepted caller authority without credentials, so the queued
/// execution runs as the caller and never as the binding owner or SYSTEM.
#[derive(Debug, Clone, PartialEq)]
pub struct Submission {
    /// The execution ID returned to the caller at submission.
    pub execution_id: ExecutionId,
    /// The submitting request's verified effective authority.
    pub authority: AcceptedAuthority,
    /// The exact Verifier, subject, optional Run, inputs, and route overrides.
    pub request: VerificationRequest,
    /// The submitting request's span and its caller's sampling decision,
    /// which the execution links to; `None` when the request had no recorded
    /// span.
    pub cause: Option<Cause>,
    /// The request's `Idempotency-Key`, when it carried one.
    pub key: Option<RequestKey>,
}

impl Submission {
    /// Build a submission and assign its execution ID.
    ///
    /// With `key`, the ID is [`RequestKey::execution_id`], so identical
    /// retries share it; without one, every submission gets a fresh `UUIDv7`.
    ///
    /// # Errors
    /// Returns the serialization error when a keyed `request` cannot be
    /// canonicalized.
    pub fn new(
        tenant: DataTenantId,
        authority: AcceptedAuthority,
        request: VerificationRequest,
        cause: Option<Cause>,
        key: Option<String>,
    ) -> Result<Self, serde_json::Error> {
        let key = key.map(|key| RequestKey::new(key, &request)).transpose()?;
        let execution_id = key.as_ref().map_or_else(ExecutionId::new_v7, |key| {
            key.execution_id(tenant, authority.attribution())
        });
        Ok(Self {
            execution_id,
            authority,
            request,
            cause,
            key,
        })
    }
}

/// What the queue writer decided for one [`Submission`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubmissionOutcome {
    /// A new pending execution was created under the submission's ID.
    Accepted,
    /// The submission's ID already exists, or this subject and actor already
    /// had this key accepted for the same request; this is that execution.
    Replayed(ExecutionId),
    /// This subject and actor already used this key for a different request.
    /// The submission settled `errored` with the idempotency-conflict code
    /// under its own ID; the original execution is unchanged.
    Conflict {
        /// The execution the key belongs to.
        original: ExecutionId,
    },
    /// The target refused the submission, which settled `errored` with
    /// [`EnqueueRefusal::error`] under its key. A Verifier or subject Card the
    /// tenant does not hold at all leaves no row, so its status stays not
    /// found.
    Refused(EnqueueRefusal),
}

/// Why a claimed due occurrence created no execution.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScheduleSkip {
    /// A later boundary had already passed; missed occurrences are not backfilled.
    Missed,
    /// The owner principal was not runtime-active at claim time.
    Inactive,
    /// The owner's authority or the tenant SYSTEM principal could not be
    /// resolved, so nothing was accepted; nothing falls back to another identity.
    AuthorityUnavailable,
    /// The scheduled enqueue refused the occurrence.
    Refused(EnqueueRefusal),
    /// The stored schedule no longer parses; the cursor was disarmed.
    InvalidSchedule,
}

/// What a claimed due occurrence calls for, decided before any authority is
/// resolved.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DueDecision {
    /// The stored schedule no longer parses; disarm the cursor.
    InvalidSchedule,
    /// Advance to `next_run_at` without an execution.
    Skip {
        /// Why no execution is created.
        skip: ScheduleSkip,
        /// The next cursor.
        next_run_at: DateTime<Utc>,
    },
    /// Execute `window` under the owner's authority, then advance to `next_run_at`.
    Run {
        /// The occurrence's fixed window.
        window: DriftWindow,
        /// The owner's Card-bound principal the execution serves.
        owner: PrincipalId,
        /// The next cursor.
        next_run_at: DateTime<Utc>,
    },
}

/// One due scheduled binding locked by [`VerifierExecutionQueue::claim_due`]
/// until the caller's transaction ends.
///
/// The occurrence, owner activity, and missed-boundary rule are already
/// decided. When the occurrence needs an execution, [`Self::owner`] names the
/// owner principal whose current authority the caller resolves, on the same
/// transaction, before [`VerifierExecutionQueue::schedule_due`] settles the
/// tick.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DueBinding {
    /// The claimed binding.
    pub binding_id: BindingId,
    /// The stored cursor that was due.
    pub due_at: DateTime<Utc>,
    /// What the occurrence calls for.
    decision: DueDecision,
}

impl DueBinding {
    /// The owner principal an execution would serve, or `None` when the
    /// occurrence creates none and needs no authority.
    #[must_use]
    pub const fn owner(&self) -> Option<PrincipalId> {
        match self.decision {
            DueDecision::Run { owner, .. } => Some(owner),
            DueDecision::InvalidSchedule | DueDecision::Skip { .. } => None,
        }
    }
}

/// What one scheduler claim of a due binding did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScheduleOutcome {
    /// An execution was created for the occurrence's window.
    Enqueued(ExecutionId),
    /// The occurrence already had this execution.
    AlreadyEnqueued(ExecutionId),
    /// No execution was created.
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
/// Only [`VerifierExecutionQueue::claim`] and
/// [`OperatorDispatchQueue::claim`](crate::queries::operator_dispatches::OperatorDispatchQueue::claim)
/// mint one, so a settlement can only be attempted by a worker that actually
/// claimed the execution or dispatch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct LeaseToken(pub(crate) Uuid);

/// The identity a runner settles a claimed execution with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExecutionLease {
    /// The claimed execution.
    pub execution_id: ExecutionId,
    /// This claim's fencing token.
    pub token: LeaseToken,
}

/// An execution claimed for running, with every frozen identity the runner needs.
#[derive(Debug, Clone, PartialEq)]
pub struct ClaimedExecution {
    /// Settlement identity.
    pub lease: ExecutionLease,
    /// When the lease expires and the execution becomes reclaimable.
    pub lease_expires_at: DateTime<Utc>,
    /// This attempt's number, starting at 1.
    pub attempt: i32,
    /// Attempt budget frozen at admission.
    pub max_attempts: i32,
    /// Exact Verifier Card UID.
    pub verifier_card_uid: CardUid,
    /// Exact Verifier Card version.
    pub verifier_version: String,
    /// Exact Card verified.
    pub card_uid: CardUid,
    /// Why the execution exists.
    pub origin: ExecutionOrigin,
    /// Binding owner; `None` for a submission.
    pub owner_card_uid: Option<CardUid>,
    /// Binding; `None` for a submission.
    pub binding_id: Option<BindingId>,
    /// Effective Trigger; `None` for a submission.
    pub trigger: Option<FrozenTarget>,
    /// Frozen input.
    pub input: ExecutionInput,
    /// Who the execution serves and who accepted it, frozen at admission.
    pub attribution: Attribution,
    /// The effective authority accepted with the execution; every Agent call
    /// runs under it, never under a later lookup of the subject.
    pub authority: AcceptedAuthority,
    /// The request that submitted the execution, which the attempt links to
    /// without adopting as its parent; `None` for scheduled work and
    /// submissions without a recorded span.
    pub cause: Option<Cause>,
    /// How long the execution had been claimable when this claim took it:
    /// the claim statement's PostgreSQL instant less its due time, or its
    /// lease expiry for a reclaim. Telemetry only; never a coordination input.
    pub queue_wait: std::time::Duration,
    /// How long before this claim the execution was admitted: the claim
    /// statement's PostgreSQL instant less `created_at`. Telemetry only.
    pub age: std::time::Duration,
    /// The tenant's active SYSTEM principal, read in the claim transaction;
    /// `None` when the tenant has none.
    pub system_principal: Option<PrincipalId>,
    /// Whether the exact Verifier Card exists and is not deleted, read in the
    /// claim transaction; a cached Verifier is used only while this holds.
    pub verifier_present: bool,
    /// The Verifier's ready fitted Drift baselines keyed by requirement ID,
    /// read in the claim transaction; empty when it has none.
    pub fitted_baselines: BTreeMap<TaskId, Value>,
}

/// Outcome of a token-fenced settlement.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Settlement {
    /// The transition was applied (or idempotently re-applied).
    Applied,
    /// The lease token no longer holds the work; nothing changed.
    StaleLease,
}

/// Outcome of reporting a retryable execution, fit, or dispatch failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RetryOutcome {
    /// The same work will be attempted again at this database time.
    Scheduled(DateTime<Utc>),
    /// The attempt budget (or a dispatch's deadline) is spent; an execution is
    /// now `errored`, a dispatch `failed`.
    Exhausted,
    /// The lease token no longer holds the work; nothing changed.
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

/// One execution's status read, with the identities the caller authorizes on.
///
/// `state.result` is always `None`: the result lives in Bifrost, and the
/// caller reads it through `result_id` after authorizing.
#[derive(Debug, Clone, PartialEq)]
pub struct ExecutionStatus {
    /// Status, last error, and dispatches; the result is not read here.
    pub state: VerificationExecutionState,
    /// Who the execution serves and who accepted it.
    pub attribution: Attribution,
    /// Exact Verifier Card UID.
    pub verifier_card_uid: CardUid,
    /// Exact Card verified.
    pub card_uid: CardUid,
    /// Result pointer of a completed execution.
    pub result_id: Option<VerificationResultId>,
}

/// Bounded retry of engine failures: attempts and exponential backoff.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RetryPolicy {
    /// Total attempts, including the first.
    pub(crate) max_attempts: i32,
    /// Delay after the first failed attempt; doubles after each later one.
    base_delay: Duration,
    /// Ceiling on any single delay.
    max_delay: Duration,
}

impl RetryPolicy {
    /// Build a policy.
    ///
    /// # Panics
    /// Panics when `max_attempts` is not positive, since work must be
    /// attempted at least once.
    #[must_use]
    pub fn new(max_attempts: i32, base_delay: Duration, max_delay: Duration) -> Self {
        assert!(
            max_attempts > 0,
            "invariant: work is attempted at least once"
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

impl Default for RetryPolicy {
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
    /// Verifier executions.
    pub executions: QueueCounts,
    /// Operator dispatches.
    pub dispatches: QueueCounts,
}

/// Owner of the durable Verifier execution queue's policies and transitions.
///
/// Holds the retry policy frozen into every new execution and the inactivity
/// window the scheduler and binding status apply. It holds no connection:
/// every tenant method runs on the caller's [`TenantConn`] and never commits.
#[derive(Debug, Clone, Copy, Default)]
pub struct VerifierExecutionQueue {
    /// Engine-failure retry policy.
    retry: RetryPolicy,
    /// How long a qualifying exchange keeps a binding owner active.
    inactivity: InactivityTimeout,
}

impl VerifierExecutionQueue {
    /// Accept one tenant batch of acknowledged submissions, in order.
    ///
    /// This is the batched queue writer's one durable step and the only
    /// place replay and conflict are decided. For each submission, in the
    /// caller's transaction: an existing row with its execution ID is
    /// [`SubmissionOutcome::Replayed`], which makes a retried batch a no-op.
    /// A keyed submission then takes a transaction-scoped advisory lock on
    /// (subject, actor, key), so concurrent writers serialize in acceptance
    /// order, and finds the execution that subject and actor already had
    /// accepted under the key: the same digest replays it, and a different
    /// digest settles this submission `errored` with the idempotency-conflict
    /// code under its own ID and no key, since the key stays the original's.
    /// Otherwise the direct target is resolved; a refusal settles the
    /// submission `errored` with [`EnqueueRefusal::error`] and keeps its key,
    /// so a retry replays the refusal and a different request under the key
    /// conflicts with it. An accepted submission stores its key, digest,
    /// optional Run, inputs, route overrides, cause, and accepted authority on
    /// a `pending` row due at PostgreSQL's statement time. Conflicting and
    /// refused rows never execute or dispatch. The caller commits.
    ///
    /// # Errors
    /// Returns the database error when a lock, read, or insert fails, or an
    /// encode or decode error when an authority, input, or stored identity is
    /// malformed; nothing of the batch survives the caller's rollback.
    #[tracing::instrument(skip(self, conn, submissions), fields(operation = "verification.executions.accept", submissions = submissions.len()))]
    pub async fn accept_submissions(
        &self,
        conn: &mut TenantConn<'_>,
        submissions: &[Submission],
    ) -> Result<Vec<SubmissionOutcome>, SqlxError> {
        let mut outcomes = Vec::with_capacity(submissions.len());
        for submission in submissions {
            outcomes.push(self.accept(conn, submission).await?);
        }
        Ok(outcomes)
    }

    /// Decide and write one submission; see [`Self::accept_submissions`].
    ///
    /// # Errors
    /// Returns the database error when a statement fails, or an encode or
    /// decode error when a value is malformed.
    async fn accept(
        &self,
        conn: &mut TenantConn<'_>,
        submission: &Submission,
    ) -> Result<SubmissionOutcome, SqlxError> {
        let exists: bool = sqlx::query_scalar(EXECUTION_EXISTS_SQL)
            .bind(submission.execution_id.as_uuid())
            .fetch_one(&mut **conn.transaction())
            .await?;
        if exists {
            return Ok(SubmissionOutcome::Replayed(submission.execution_id));
        }
        let attribution = submission.authority.attribution();
        if let Some(key) = &submission.key {
            sqlx::query(LOCK_REQUEST_KEY_SQL)
                .bind(REQUEST_KEY_LOCK_CLASS)
                .bind(attribution.sub_principal_id.as_uuid())
                .bind(attribution.act_principal_id.as_uuid())
                .bind(key.key.as_str())
                .execute(&mut **conn.transaction())
                .await?;
            let existing: Option<(Uuid, Option<Vec<u8>>)> = sqlx::query_as(KEYED_EXECUTION_SQL)
                .bind(attribution.sub_principal_id.as_uuid())
                .bind(attribution.act_principal_id.as_uuid())
                .bind(key.key.as_str())
                .fetch_optional(&mut **conn.transaction())
                .await?;
            if let Some((original, digest)) = existing {
                let original = stored(ExecutionId::new(original))?;
                if digest.as_deref() == Some(key.request_sha256.as_slice()) {
                    return Ok(SubmissionOutcome::Replayed(original));
                }
                let conflict = WyrdError::RegistryIdempotencyConflict {
                    message: String::new(),
                    details: Value::Null,
                };
                let error = VerificationError {
                    code: conflict.code().to_owned(),
                    message: format!(
                        "Idempotency-Key {:?} was already used for a different request (execution {original})",
                        key.key
                    ),
                };
                self.insert_submission(conn, submission, None, Some(&error))
                    .await?;
                return Ok(SubmissionOutcome::Conflict { original });
            }
        }
        let resolved: ResolvedTarget = sqlx::query_as(RESOLVE_DIRECT_SQL)
            .bind(submission.request.verifier_card_uid.as_uuid())
            .bind(submission.request.card_uid.as_uuid())
            .fetch_one(&mut **conn.transaction())
            .await?;
        if let Some(refusal) = resolved.refusal()? {
            self.insert_submission(
                conn,
                submission,
                submission.key.as_ref(),
                Some(&refusal.error()),
            )
            .await?;
            return Ok(SubmissionOutcome::Refused(refusal));
        }
        self.insert_submission(conn, submission, submission.key.as_ref(), None)
            .await?;
        Ok(SubmissionOutcome::Accepted)
    }

    /// Insert one submission's row under `key`: `pending` when `error` is
    /// `None`, otherwise settled `errored` with `error`.
    ///
    /// # Errors
    /// Returns the database error when the insert fails, or an encode error
    /// when the authority, inputs, or route overrides cannot be serialized.
    async fn insert_submission(
        &self,
        conn: &mut TenantConn<'_>,
        submission: &Submission,
        key: Option<&RequestKey>,
        error: Option<&VerificationError>,
    ) -> Result<(), SqlxError> {
        let attribution = submission.authority.attribution();
        let request = &submission.request;
        sqlx::query(INSERT_SUBMISSION_SQL)
            .bind(submission.execution_id.as_uuid())
            .bind(request.verifier_card_uid.as_uuid())
            .bind(request.card_uid.as_uuid())
            .bind(request.run_id.as_ref().map(RunId::as_str))
            .bind(Json(&request.inputs))
            .bind(Json(&request.execution_route_overrides))
            .bind(attribution.sub_principal_id.as_uuid())
            .bind(attribution.act_principal_id.as_uuid())
            .bind(Json(&submission.authority))
            .bind(
                submission
                    .cause
                    .as_ref()
                    .map(|cause| cause.trace_id.as_bytes().as_slice()),
            )
            .bind(
                submission
                    .cause
                    .as_ref()
                    .map(|cause| cause.span_id.as_bytes().as_slice()),
            )
            .bind(key.map(|key| key.key.as_str()))
            .bind(key.map(|key| key.request_sha256.as_slice()))
            .bind(self.retry.max_attempts)
            .bind(error.map(Json))
            .bind(submission.cause.as_ref().map(|cause| cause.sampled))
            .execute(&mut **conn.transaction())
            .await?;
        Ok(())
    }

    /// Enqueue one due occurrence of a scheduled binding.
    ///
    /// Resolves the binding in the caller's tenant and freezes its owner,
    /// subject, Verifier, Trigger, and Operators. It refuses, without
    /// writing, a missing binding, an unavailable or unready Verifier, or an
    /// unavailable subject. Otherwise it inserts a `pending` execution of
    /// `window` under `authority`, due at PostgreSQL's statement time. An
    /// occurrence that already has an execution returns it as
    /// [`EnqueueOutcome::AlreadyEnqueued`].
    ///
    /// # Errors
    /// Returns the database error when a read or the insert fails, or a
    /// decode or encode error when stored identities or the authority are
    /// malformed.
    ///
    /// # Panics
    /// Never in practice: a ready Verifier's Card row exists, so its version
    /// is present.
    #[tracing::instrument(skip(self, conn, authority), fields(operation = "verification.executions.enqueue_scheduled", %binding_id))]
    pub async fn enqueue_scheduled(
        &self,
        conn: &mut TenantConn<'_>,
        binding_id: BindingId,
        window: DriftWindow,
        authority: &ExecutionAuthority,
    ) -> Result<EnqueueOutcome, SqlxError> {
        let resolved: Option<ResolvedTarget> = sqlx::query_as(RESOLVE_BINDING_SQL)
            .bind(binding_id.as_uuid())
            .fetch_optional(&mut **conn.transaction())
            .await?;
        let Some(resolved) = resolved else {
            return Ok(EnqueueOutcome::Refused(EnqueueRefusal::BindingNotFound));
        };
        if let Some(refusal) = resolved.refusal()? {
            return Ok(EnqueueOutcome::Refused(refusal));
        }
        let attribution = authority.attribution();
        let inserted: Option<Uuid> = sqlx::query_scalar(INSERT_SCHEDULED_SQL)
            .bind(ExecutionId::new_v7().as_uuid())
            .bind(resolved.verifier_card_uid)
            .bind(
                resolved
                    .verifier_version
                    .expect("invariant: a ready Verifier has a Card version"),
            )
            .bind(resolved.card_uid)
            .bind(resolved.owner_card_uid)
            .bind(resolved.binding_id)
            .bind(resolved.trigger_uid)
            .bind(resolved.trigger_digest)
            .bind(resolved.operators)
            .bind(window.start)
            .bind(window.end)
            .bind(attribution.sub_principal_id.as_uuid())
            .bind(attribution.act_principal_id.as_uuid())
            .bind(Json(authority.authority()))
            .bind(self.retry.max_attempts)
            .fetch_optional(&mut **conn.transaction())
            .await?;
        if let Some(execution_id) = inserted {
            return Ok(EnqueueOutcome::Enqueued(stored(ExecutionId::new(
                execution_id,
            ))?));
        }
        let existing: Uuid = sqlx::query_scalar(EXISTING_OCCURRENCE_SQL)
            .bind(binding_id.as_uuid())
            .bind(window.end)
            .fetch_one(&mut **conn.transaction())
            .await?;
        Ok(EnqueueOutcome::AlreadyEnqueued(stored(ExecutionId::new(
            existing,
        ))?))
    }

    /// Lock and decide the earliest due scheduled binding of the caller's
    /// tenant.
    ///
    /// Locks one binding whose cursor is at least [`SCHEDULE_CLAIM_DELAY`]
    /// before PostgreSQL's statement time with `FOR UPDATE SKIP LOCKED`, so
    /// concurrent schedulers never claim the same occurrence. Statement time
    /// minus that delay is the schedule clock for this tick: the occurrence
    /// calls for an execution of its fixed window only when no later boundary
    /// had reached it and its owner principal is runtime-active. Writes
    /// nothing; [`Self::schedule_due`] settles the tick in the same
    /// transaction once the caller has resolved the owner's authority for a
    /// [`DueBinding::owner`]. Returns `None` when nothing is due.
    ///
    /// # Errors
    /// Returns the database error when a read fails, or a decode error when
    /// stored identities are malformed.
    #[tracing::instrument(skip(self, conn), fields(operation = "verification.schedule.claim"))]
    pub async fn claim_due(
        &self,
        conn: &mut TenantConn<'_>,
    ) -> Result<Option<DueBinding>, SqlxError> {
        let due: Option<DueBindingRow> = sqlx::query_as(DUE_BINDING_SQL)
            .bind(SCHEDULE_CLAIM_DELAY.num_milliseconds())
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
        .and_then(|schedule| schedule.occurrence(due_at, due.schedule_clock))
        {
            Ok(occurrence) => occurrence,
            Err(error) => {
                tracing::error!(%binding_id, %error, "stored binding schedule cannot advance; disarming");
                return Ok(Some(DueBinding {
                    binding_id,
                    due_at,
                    decision: DueDecision::InvalidSchedule,
                }));
            }
        };
        let next_run_at = occurrence.next_run_at;
        let skip = |skip| DueDecision::Skip { skip, next_run_at };
        let decision = match occurrence.window {
            None => skip(ScheduleSkip::Missed),
            Some(window) => match binding_activity(conn, binding_id, self.inactivity)
                .await?
                .filter(|activity| activity.active)
                .and_then(|activity| activity.principal_id)
            {
                None => skip(ScheduleSkip::Inactive),
                Some(owner) => DueDecision::Run {
                    window,
                    owner,
                    next_run_at,
                },
            },
        };
        Ok(Some(DueBinding {
            binding_id,
            due_at,
            decision,
        }))
    }

    /// Settle one tick claimed by [`Self::claim_due`].
    ///
    /// An occurrence that calls for an execution is enqueued through
    /// [`Self::enqueue_scheduled`] (which enforces Verifier readiness) under
    /// `authority`, the owner's resolved authority with the SYSTEM actor.
    /// Without it the occurrence is skipped as
    /// [`ScheduleSkip::AuthorityUnavailable`] rather than run under any other
    /// identity. In every case the cursor moves to the first boundary strictly
    /// after the schedule clock in the same transaction, so inactive, unready,
    /// unauthorized, and missed occurrences are skipped without backfill, and
    /// an unparseable schedule is disarmed. The caller commits, releasing the
    /// lock before any analysis runs, and claims again until nothing is due.
    ///
    /// # Errors
    /// Returns the database error when the enqueue or cursor write fails.
    #[tracing::instrument(skip(self, conn, authority), fields(operation = "verification.schedule.tick", binding_id = %due.binding_id))]
    pub async fn schedule_due(
        &self,
        conn: &mut TenantConn<'_>,
        due: DueBinding,
        authority: Option<ExecutionAuthority>,
    ) -> Result<ScheduleTick, SqlxError> {
        let (outcome, next_run_at) = match due.decision {
            DueDecision::InvalidSchedule => (
                ScheduleOutcome::Skipped(ScheduleSkip::InvalidSchedule),
                None,
            ),
            DueDecision::Skip { skip, next_run_at } => {
                (ScheduleOutcome::Skipped(skip), Some(next_run_at))
            }
            DueDecision::Run {
                window,
                next_run_at,
                ..
            } => {
                let outcome = match authority {
                    None => ScheduleOutcome::Skipped(ScheduleSkip::AuthorityUnavailable),
                    Some(authority) => match self
                        .enqueue_scheduled(conn, due.binding_id, window, &authority)
                        .await?
                    {
                        EnqueueOutcome::Enqueued(id) => ScheduleOutcome::Enqueued(id),
                        EnqueueOutcome::AlreadyEnqueued(id) => ScheduleOutcome::AlreadyEnqueued(id),
                        EnqueueOutcome::Refused(refusal) => {
                            ScheduleOutcome::Skipped(ScheduleSkip::Refused(refusal))
                        }
                    },
                };
                (outcome, Some(next_run_at))
            }
        };
        self.advance_cursor(conn, due.binding_id, next_run_at)
            .await?;
        Ok(ScheduleTick {
            binding_id: due.binding_id,
            due_at: due.due_at,
            outcome,
            next_run_at,
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

    /// Claim one runnable execution of the caller's tenant under a fresh lease.
    ///
    /// First settles, as `errored`, every expired lease whose attempts are
    /// exhausted, so a crash loop cannot exceed the budget. Then locks the
    /// oldest `pending` or `retrying` execution due at PostgreSQL's statement
    /// time, or `running` execution whose lease the database considers
    /// expired, with `SKIP LOCKED`; marks it `running` with a new lease token
    /// expiring `lease_for` after that same database instant; and counts the
    /// attempt. Returns `None` when nothing is runnable. The caller commits
    /// before executing. The statement opens no span of its own: the runner
    /// times it on a `claim` span it can parent to the attempt only once the
    /// claimed row names the attempt's cause.
    ///
    /// # Errors
    /// Returns the database error when a statement fails, or a decode error
    /// when the claimed row's identities or frozen input are malformed.
    pub async fn claim(
        &self,
        conn: &mut TenantConn<'_>,
        lease_for: Duration,
    ) -> Result<Option<ClaimedExecution>, SqlxError> {
        let exhausted = VerificationError {
            code: "lease_expired".to_owned(),
            message: "the execution's lease expired after its final attempt".to_owned(),
        };
        sqlx::query(EXHAUST_EXPIRED_SQL)
            .bind(Json(&exhausted))
            .execute(&mut **conn.transaction())
            .await?;
        let token = Uuid::now_v7();
        let row: Option<ClaimedRow> = sqlx::query_as(CLAIM_SQL)
            .bind(token)
            .bind(lease_for.num_milliseconds())
            .fetch_optional(&mut **conn.transaction())
            .await?;
        row.map(|row| row.into_claimed(LeaseToken(token)))
            .transpose()
    }

    /// Renew, on the database clock, every lease in `tokens` still held.
    ///
    /// A lease is extended to `lease_for` from PostgreSQL's statement time once
    /// a third of it has passed; a younger lease is left as it is. An expired
    /// lease is never revived, because another claimant may already be
    /// reclaiming it. Returns every token that still holds an unexpired lease,
    /// renewed or not, so a token missing from the result expired, was
    /// reclaimed, or was settled, and its work must stop. Renewal evaluates no
    /// permission and writes no audit.
    ///
    /// # Errors
    /// Returns the database error when the update fails.
    #[tracing::instrument(skip(self, conn, tokens), fields(operation = "verification.executions.renew", leases = tokens.len()))]
    pub async fn renew(
        &self,
        conn: &mut TenantConn<'_>,
        tokens: &[LeaseToken],
        lease_for: Duration,
    ) -> Result<Vec<LeaseToken>, SqlxError> {
        let tokens: Vec<Uuid> = tokens.iter().map(|token| token.0).collect();
        let lease = lease_for.num_milliseconds();
        let held: Vec<Uuid> = sqlx::query_scalar(RENEW_LEASES_SQL)
            .bind(tokens)
            .bind(lease)
            .bind(lease - lease / 3)
            .fetch_all(&mut **conn.transaction())
            .await?;
        Ok(held.into_iter().map(LeaseToken).collect())
    }

    /// Settle a claimed execution `completed`, pointing at its staged result.
    ///
    /// Only the current lease holder can complete; re-completing with the
    /// same token and result is an idempotent [`Settlement::Applied`]. For a
    /// binding-created execution with a `failed` verdict, the same transaction
    /// inserts one `pending` dispatch per distinct frozen Operator, due at
    /// PostgreSQL's statement time; the (tenant, execution, Operator) key
    /// makes a settlement retry or replay insert nothing new. Each dispatch
    /// freezes the bounded failure context built from the settled execution,
    /// its exact Cards, `summary`, which is clipped to [`MAX_SUMMARY_CHARS`],
    /// and `drift`, the counts of the Verifier's one drift check when it has
    /// exactly one. Passed and inconclusive verdicts and submissions create no
    /// dispatch. The verdict itself is not stored: Bifrost owns it.
    ///
    /// # Errors
    /// Returns the database error when a statement fails, or a decode error
    /// when the frozen Operators are malformed.
    #[tracing::instrument(skip(self, conn), fields(operation = "verification.executions.complete", execution_id = %lease.execution_id))]
    pub async fn complete(
        &self,
        conn: &mut TenantConn<'_>,
        lease: ExecutionLease,
        result_id: VerificationResultId,
        verdict: VerificationVerdict,
        summary: &str,
        drift: Option<DriftCounts>,
    ) -> Result<Settlement, SqlxError> {
        let settled: Option<(Option<Uuid>, Json<Vec<FrozenTarget>>)> = sqlx::query_as(COMPLETE_SQL)
            .bind(lease.execution_id.as_uuid())
            .bind(lease.token.0)
            .bind(result_id.as_uuid())
            .fetch_optional(&mut **conn.transaction())
            .await?;
        let Some((binding_id, Json(operators))) = settled else {
            return Ok(Settlement::StaleLease);
        };
        if verdict == VerificationVerdict::Failed && binding_id.is_some() {
            let summary: String = summary.chars().take(MAX_SUMMARY_CHARS).collect();
            for operator in &operators {
                let (uid, digest) = match operator {
                    FrozenTarget::Uid(uid) => (Some(uid.as_uuid()), None),
                    FrozenTarget::Digest(digest) => (None, Some(digest.as_str())),
                };
                sqlx::query(INSERT_DISPATCH_SQL)
                    .bind(OperatorDispatchId::new_v7().as_uuid())
                    .bind(lease.execution_id.as_uuid())
                    .bind(uid)
                    .bind(digest)
                    .bind(&summary)
                    .bind(drift.map(Json))
                    .execute(&mut **conn.transaction())
                    .await?;
            }
        }
        Ok(Settlement::Applied)
    }

    /// Report a retryable engine failure for a claimed execution.
    ///
    /// With attempts left, the same execution and frozen input become
    /// `retrying`, due after the policy's backoff measured from PostgreSQL's
    /// statement time, keeping `error` visible; the stored deadline is
    /// returned as the database wrote it. With the budget exhausted, the
    /// execution settles `errored` with `error`, no verdict, and no dispatch.
    ///
    /// # Errors
    /// Returns the database error when a statement fails.
    #[tracing::instrument(skip(self, conn, error), fields(operation = "verification.executions.retry", execution_id = %lease.execution_id))]
    pub async fn retry(
        &self,
        conn: &mut TenantConn<'_>,
        lease: ExecutionLease,
        error: &VerificationError,
    ) -> Result<RetryOutcome, SqlxError> {
        let budget: Option<(i32, i32)> = sqlx::query_as(LEASED_ATTEMPTS_SQL)
            .bind(lease.execution_id.as_uuid())
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
        let next_attempt_at: Option<DateTime<Utc>> = sqlx::query_scalar(RETRY_SQL)
            .bind(lease.execution_id.as_uuid())
            .bind(lease.token.0)
            .bind(Json(error))
            .bind(self.retry.delay_after(attempts).num_milliseconds())
            .fetch_optional(&mut **conn.transaction())
            .await?;
        Ok(next_attempt_at.map_or(RetryOutcome::StaleLease, RetryOutcome::Scheduled))
    }

    /// Settle a claimed execution in a terminal status with no verdict.
    ///
    /// `cancelled`, `timed_out`, and `errored` executions record `error`,
    /// point at no result, and never dispatch an Operator.
    ///
    /// # Errors
    /// Returns the database error when the update fails.
    #[tracing::instrument(skip(self, conn, error), fields(operation = "verification.executions.terminate", execution_id = %lease.execution_id))]
    pub async fn terminate(
        &self,
        conn: &mut TenantConn<'_>,
        lease: ExecutionLease,
        status: TerminalStatus,
        error: &VerificationError,
    ) -> Result<Settlement, SqlxError> {
        let result = sqlx::query(TERMINATE_SQL)
            .bind(lease.execution_id.as_uuid())
            .bind(lease.token.0)
            .bind(status.as_str())
            .bind(Json(error))
            .execute(&mut **conn.transaction())
            .await?;
        Ok(settlement(result.rows_affected()))
    }

    /// Return a claimed execution to the queue with its attempt refunded.
    ///
    /// Shutdown drain releases with a zero `delay`; admission backpressure
    /// releases with a poll delay so a saturated reader is not reclaimed in a
    /// hot loop. The execution keeps its identity and frozen input, becomes
    /// due `delay` after PostgreSQL's statement time, and does not consume its
    /// retry budget.
    ///
    /// # Errors
    /// Returns the database error when the update fails.
    #[tracing::instrument(skip(self, conn), fields(operation = "verification.executions.release", execution_id = %lease.execution_id))]
    pub async fn release(
        &self,
        conn: &mut TenantConn<'_>,
        lease: ExecutionLease,
        delay: Duration,
    ) -> Result<Settlement, SqlxError> {
        let result = sqlx::query(RELEASE_SQL)
            .bind(lease.execution_id.as_uuid())
            .bind(lease.token.0)
            .bind(delay.num_milliseconds())
            .execute(&mut **conn.transaction())
            .await?;
        Ok(settlement(result.rows_affected()))
    }

    /// Read one execution's status projection.
    ///
    /// Returns execution status, the last error, every dispatch's delivery
    /// status, the subject and accepting actor, the exact Verifier and
    /// subject, and the result pointer — never the verdict or result body. An
    /// unknown, not-yet-accepted, or other-tenant execution is `Ok(None)`.
    ///
    /// # Errors
    /// Returns the database error when a read fails, or a decode error when a
    /// stored identity, status, or error is malformed.
    pub async fn execution_status(
        &self,
        conn: &mut TenantConn<'_>,
        execution_id: ExecutionId,
    ) -> Result<Option<ExecutionStatus>, SqlxError> {
        let row: Option<StatusRow> = sqlx::query_as(STATUS_SQL)
            .bind(execution_id.as_uuid())
            .fetch_optional(&mut **conn.transaction())
            .await?;
        let Some(row) = row else {
            return Ok(None);
        };
        let dispatches: Vec<DispatchRow> = sqlx::query_as(DISPATCHES_SQL)
            .bind(execution_id.as_uuid())
            .fetch_all(&mut **conn.transaction())
            .await?;
        Ok(Some(ExecutionStatus {
            state: VerificationExecutionState {
                execution_id: stored(ExecutionId::new(row.execution_id))?,
                status: stored(row.status.parse())?,
                error: row.error.map(|Json(error)| error),
                result: None,
                dispatches: dispatches
                    .into_iter()
                    .map(DispatchRow::into_state)
                    .collect::<Result<_, _>>()?,
            },
            attribution: Attribution::new(
                PrincipalId::new(row.sub_principal_id),
                PrincipalId::new(row.act_principal_id),
            ),
            verifier_card_uid: stored(CardUid::from_uuid(row.verifier_card_uid))?,
            card_uid: stored(CardUid::from_uuid(row.card_uid))?,
            result_id: row
                .result_id
                .map(|id| stored(VerificationResultId::new(id)))
                .transpose()?,
        }))
    }

    /// Read one binding's Binding GET projection at database time.
    ///
    /// Combines the owner activity gate (the same query the scheduler
    /// applies), generic Verifier readiness, the schedule cursor, the owner's
    /// last qualifying exchange, and the latest admitted execution. An
    /// unknown or other-tenant binding is `Ok(None)`.
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
        let (subject, verifier, readiness, last_execution): (Uuid, Uuid, String, Option<Uuid>) =
            sqlx::query_as(BINDING_TARGETS_SQL)
                .bind(binding_id.as_uuid())
                .fetch_one(&mut **conn.transaction())
                .await?;
        Ok(Some(VerificationBindingStatus {
            binding_id,
            owner_card_uid: activity.owner_card_uid,
            card_uid: stored(CardUid::from_uuid(subject))?,
            verifier_card_uid: stored(CardUid::from_uuid(verifier))?,
            active: activity.active,
            readiness: stored(readiness.parse())?,
            next_run_at: activity.next_run_at,
            last_activated_at: activity.last_authenticated_at,
            last_execution_id: last_execution
                .map(|id| stored(ExecutionId::new(id)))
                .transpose()?,
        }))
    }

    /// List every tenant that has a claimable execution at database time.
    ///
    /// Longest-waiting tenant first, so a runner visiting tenants in order does
    /// not let one busy tenant starve others. There is no tenant limit: one
    /// claim round considers every tenant with work. Read-only; claiming
    /// happens per tenant through [`Self::claim`].
    ///
    /// # Errors
    /// Returns the database error when the read fails, or a decode error when
    /// a stored tenant key is malformed.
    // tenant-isolation: cross-tenant OperatorPool
    pub async fn tenants_with_runnable_executions(
        &self,
        operator: &OperatorPool,
    ) -> Result<Vec<DataTenantId>, SqlxError> {
        let rows: Vec<Uuid> = sqlx::query_scalar(RUNNABLE_TENANTS_SQL)
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
    /// through [`Self::claim_due`].
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
            .bind(SCHEDULE_CLAIM_DELAY.num_milliseconds())
            .fetch_all(operator.pool())
            .await?;
        rows.into_iter()
            .map(|tenant| stored(DataTenantId::new(tenant)))
            .collect()
    }

    /// Count non-terminal executions and dispatches by status across every tenant.
    ///
    /// Six aggregate counts — no tenant or execution labels — so telemetry
    /// stays bounded regardless of tenant count.
    ///
    /// # Errors
    /// Returns the database error when the read fails.
    // tenant-isolation: cross-tenant OperatorPool
    pub async fn queue_depth(&self, operator: &OperatorPool) -> Result<QueueDepth, SqlxError> {
        let (
            executions_pending,
            executions_retrying,
            executions_running,
            pending,
            retrying,
            running,
        ): (i64, i64, i64, i64, i64, i64) = sqlx::query_as(QUEUE_DEPTH_SQL)
            .fetch_one(operator.pool())
            .await?;
        Ok(QueueDepth {
            executions: QueueCounts {
                pending: executions_pending,
                retrying: executions_retrying,
                running: executions_running,
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
    verifier_card_uid: Uuid,
    /// Verifier Card version; absent when the Card does not exist.
    verifier_version: Option<String>,
    /// Stored [`VerifierReadiness`] name.
    readiness: String,
    /// Exact Card verified.
    card_uid: Uuid,
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
    /// Decide whether this target refuses work, in precedence order:
    /// readiness, then subject availability.
    ///
    /// # Errors
    /// Returns [`SqlxError::Decode`] when the stored readiness is unknown.
    fn refusal(&self) -> Result<Option<EnqueueRefusal>, SqlxError> {
        let readiness: VerifierReadiness = stored(self.readiness.parse())?;
        if readiness != VerifierReadiness::Ready {
            return Ok(Some(EnqueueRefusal::NotReady(readiness)));
        }
        if !self.subject_available {
            return Ok(Some(EnqueueRefusal::SubjectUnavailable));
        }
        Ok(None)
    }
}

/// A claimed execution row from [`CLAIM_SQL`].
#[derive(sqlx::FromRow)]
struct ClaimedRow {
    /// Execution identity.
    execution_id: Uuid,
    /// Verifier Card UID.
    verifier_card_uid: Uuid,
    /// Verifier Card version.
    verifier_version: String,
    /// Card verified.
    card_uid: Uuid,
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
    /// Scheduled window start.
    window_start: Option<DateTime<Utc>>,
    /// Scheduled window end.
    window_end: Option<DateTime<Utc>>,
    /// Submitted application Run.
    run_id: Option<String>,
    /// Submitted input declarations.
    inputs: Option<Json<BTreeMap<SlotName, InputDeclaration>>>,
    /// Submitted route overrides.
    route_overrides: Option<Json<BTreeMap<TaskId, ExecutionRoute>>>,
    /// Subject the execution serves.
    sub_principal_id: Uuid,
    /// Actor that accepted the execution.
    act_principal_id: Uuid,
    /// Accepted effective authority.
    authority: Json<AcceptedAuthority>,
    /// Attempt number of this claim.
    attempts: i32,
    /// Attempt budget.
    max_attempts: i32,
    /// Lease expiry.
    lease_expires_at: DateTime<Utc>,
    /// Submitting request's trace id, 16 bytes.
    cause_trace_id: Option<Vec<u8>>,
    /// Submitting request's span id, 8 bytes.
    cause_span_id: Option<Vec<u8>>,
    /// Whether the submitting request's caller sampled it.
    cause_sampled: Option<bool>,
    /// Milliseconds the execution was claimable before this claim.
    queue_wait_ms: i64,
    /// Milliseconds since the execution was admitted.
    age_ms: i64,
    /// The tenant's active SYSTEM principal.
    system_principal_id: Option<Uuid>,
    /// The exact Verifier Card's status; `None` when the Card is absent.
    verifier_status: Option<String>,
    /// The binding's scheduled input selectors; `None` for a submission.
    schedule_inputs: Option<Json<BTreeMap<SlotName, ScheduledInput>>>,
    /// The Verifier's ready fitted baselines by requirement; `None` when none.
    fitted_baselines: Option<Json<BTreeMap<TaskId, Value>>>,
}

impl ClaimedRow {
    /// Convert raw columns into a typed claim holding `token`.
    ///
    /// # Errors
    /// Returns [`SqlxError::Decode`] when an identity is not `UUIDv7`, the
    /// origin or input columns violate their closed shapes, or a duration is
    /// negative.
    fn into_claimed(self, token: LeaseToken) -> Result<ClaimedExecution, SqlxError> {
        let input = match (
            self.window_start,
            self.window_end,
            self.schedule_inputs,
            self.inputs,
            self.route_overrides,
        ) {
            (Some(start), Some(end), Some(Json(slots)), None, None) => ExecutionInput::Scheduled {
                window: DriftWindow { start, end },
                slots,
            },
            (None, None, _, Some(Json(inputs)), Some(Json(route_overrides))) => {
                ExecutionInput::Submitted {
                    run_id: self.run_id.map(RunId::from_string),
                    inputs,
                    route_overrides,
                }
            }
            _ => {
                return Err(SqlxError::Decode(
                    "verifier execution has no single frozen input".into(),
                ));
            }
        };
        let trigger = match (self.trigger_uid, self.trigger_digest) {
            (Some(uid), _) => Some(FrozenTarget::Uid(stored(CardUid::from_uuid(uid))?)),
            (None, Some(digest)) => Some(FrozenTarget::Digest(digest)),
            (None, None) => None,
        };
        Ok(ClaimedExecution {
            lease: ExecutionLease {
                execution_id: stored(ExecutionId::new(self.execution_id))?,
                token,
            },
            lease_expires_at: self.lease_expires_at,
            attempt: self.attempts,
            max_attempts: self.max_attempts,
            verifier_card_uid: stored(CardUid::from_uuid(self.verifier_card_uid))?,
            verifier_version: self.verifier_version,
            card_uid: stored(CardUid::from_uuid(self.card_uid))?,
            origin: ExecutionOrigin::parse(&self.origin)?,
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
            attribution: Attribution::new(
                PrincipalId::new(self.sub_principal_id),
                PrincipalId::new(self.act_principal_id),
            ),
            authority: self.authority.0,
            cause: match (self.cause_trace_id, self.cause_span_id, self.cause_sampled) {
                (Some(trace), Some(span), Some(sampled)) => Some(Cause {
                    trace_id: stored(TraceId::from_bytes(fixed_width(trace)?))?,
                    span_id: stored(SpanId::from_bytes(fixed_width(span)?))?,
                    sampled,
                }),
                _ => None,
            },
            queue_wait: std::time::Duration::from_millis(stored(u64::try_from(
                self.queue_wait_ms,
            ))?),
            age: std::time::Duration::from_millis(stored(u64::try_from(self.age_ms))?),
            system_principal: self.system_principal_id.map(PrincipalId::new),
            verifier_present: self
                .verifier_status
                .is_some_and(|status| status != "deleted"),
            fitted_baselines: self
                .fitted_baselines
                .map(|Json(fitted)| fitted)
                .unwrap_or_default(),
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
    /// PostgreSQL's statement instant for this tick minus
    /// [`SCHEDULE_CLAIM_DELAY`]: the instant every schedule decision of the
    /// tick is made at, so a boundary counts as passed only once its own
    /// occurrence would be claimable.
    schedule_clock: DateTime<Utc>,
}

/// An execution status row from [`STATUS_SQL`].
#[derive(sqlx::FromRow)]
struct StatusRow {
    /// Execution identity.
    execution_id: Uuid,
    /// Stored execution status.
    status: String,
    /// Subject the execution serves.
    sub_principal_id: Uuid,
    /// Actor that accepted the execution.
    act_principal_id: Uuid,
    /// Exact Verifier Card UID.
    verifier_card_uid: Uuid,
    /// Exact Card verified.
    card_uid: Uuid,
    /// Result pointer.
    result_id: Option<Uuid>,
    /// Last execution error.
    error: Option<Json<VerificationError>>,
}

/// A dispatch row from [`DISPATCHES_SQL`].
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
    /// Convert raw columns into the execution status dispatch item.
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

/// Map a fenced update's row count to its settlement outcome; shared by the
/// execution, baseline, and Operator dispatch queues.
pub(crate) fn settlement(rows_affected: u64) -> Settlement {
    if rows_affected == 0 {
        Settlement::StaleLease
    } else {
        Settlement::Applied
    }
}

/// Convert stored identifier bytes into the fixed-width array of `N` bytes.
///
/// # Errors
/// Returns [`SqlxError::Decode`] when the column holds a different width,
/// which the table's width check precludes.
fn fixed_width<const N: usize>(bytes: Vec<u8>) -> Result<[u8; N], SqlxError> {
    bytes
        .try_into()
        .map_err(|_| SqlxError::Decode(format!("a stored identifier is not {N} bytes").into()))
}

/// Decode a stored value, surfacing a malformed one as [`SqlxError::Decode`].
///
/// # Errors
/// Returns [`SqlxError::Decode`] wrapping `value`'s error.
pub(crate) fn stored<T, E>(value: Result<T, E>) -> Result<T, SqlxError>
where
    E: StdError + Send + Sync + 'static,
{
    value.map_err(|error| SqlxError::Decode(Box::new(error)))
}

#[cfg(test)]
mod tests {
    //! Pure retry-policy arithmetic, keyed execution identity, and refusal
    //! catalog codes.

    use wyrd_spec::verification::execution::VerificationRequest;

    use super::*;

    /// The default policy allows three attempts with bounded doubling delays.
    ///
    /// # Panics
    /// Panics when a delay is not doubled from 30 seconds or exceeds the cap.
    #[test]
    fn retry_delays_double_and_cap() {
        let policy = RetryPolicy::default();
        assert_eq!(policy.max_attempts, 3);
        assert_eq!(policy.delay_after(1), Duration::seconds(30));
        assert_eq!(policy.delay_after(2), Duration::seconds(60));
        assert_eq!(policy.delay_after(3), Duration::minutes(2));
        assert_eq!(policy.delay_after(i32::MAX), Duration::minutes(2));
        assert_eq!(policy.delay_after(0), Duration::seconds(30));
    }

    /// Build an input-free request for `verifier` against a fresh subject Card
    /// UID per call, so two requests differ in subject unless a test overrides
    /// it.
    ///
    /// # Panics
    /// Panics only if a freshly generated `UUIDv7` is rejected as a Card UID,
    /// which would break the `CardUid` invariant.
    fn request(verifier: CardUid) -> VerificationRequest {
        VerificationRequest {
            verifier_card_uid: verifier,
            card_uid: CardUid::from_uuid(Uuid::now_v7()).expect("v7 Card UID"),
            run_id: None,
            inputs: BTreeMap::new(),
            execution_route_overrides: BTreeMap::new(),
        }
    }

    /// A keyed execution ID is a stable `UUIDv7` of tenant, subject, actor,
    /// key, and content, and changes when any of them changes.
    ///
    /// # Panics
    /// Panics when identical inputs disagree or a changed input collides.
    #[test]
    fn keyed_execution_id_is_stable_and_content_scoped() {
        let tenant = DataTenantId::new_v7();
        let subject = PrincipalId::new(Uuid::now_v7());
        let actor = PrincipalId::new(Uuid::now_v7());
        let attribution = Attribution::new(subject, actor);
        let original = request(CardUid::from_uuid(Uuid::now_v7()).expect("v7 Card UID"));
        let key = RequestKey::new("k-1".to_owned(), &original).expect("canonical request");
        let id = key.execution_id(tenant, attribution);
        assert_eq!(id.as_uuid().get_version_num(), 7);
        assert_eq!(
            id,
            RequestKey::new("k-1".to_owned(), &original)
                .expect("canonical request")
                .execution_id(tenant, attribution)
        );
        let other = request(CardUid::from_uuid(Uuid::now_v7()).expect("v7 Card UID"));
        let conflicting = RequestKey::new("k-1".to_owned(), &other).expect("canonical request");
        assert_ne!(conflicting.request_sha256, key.request_sha256);
        assert_ne!(conflicting.execution_id(tenant, attribution), id);
        let other_key = RequestKey::new("k-2".to_owned(), &original).expect("canonical request");
        assert_ne!(other_key.execution_id(tenant, attribution), id);
        assert_ne!(key.execution_id(DataTenantId::new_v7(), attribution), id);
        assert_ne!(
            key.execution_id(tenant, Attribution::new(subject, subject)),
            id
        );
    }

    /// Each refusal settles with the public catalog code a caller is refused with.
    ///
    /// # Panics
    /// Panics when a refusal maps to another code.
    #[test]
    fn refusals_carry_catalog_codes() {
        for (refusal, code) in [
            (
                EnqueueRefusal::BindingNotFound,
                "WYRD_VERIFICATION_404_BINDING_NOT_FOUND",
            ),
            (
                EnqueueRefusal::NotReady(VerifierReadiness::BaselineNotReady),
                "WYRD_VERIFICATION_409_BASELINE_NOT_READY",
            ),
            (
                EnqueueRefusal::NotReady(VerifierReadiness::VerifierUnavailable),
                "WYRD_VERIFICATION_400_INVALID_TARGET",
            ),
            (
                EnqueueRefusal::SubjectUnavailable,
                "WYRD_VERIFICATION_400_INVALID_TARGET",
            ),
        ] {
            assert_eq!(refusal.error().code, code);
        }
    }
}
