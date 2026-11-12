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

use std::collections::{BTreeMap, HashMap, HashSet};
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
use wyrd_spec::card::operator::{DriftCounts, MAX_SUMMARY_CHARS};
use wyrd_spec::card::verifier::SourceText;
use wyrd_spec::error::WyrdError;
use wyrd_spec::ids::{
    BindingId, CardUid, ExecutionId, GroupId, OperatorDispatchId, SlotName, VerificationResultId,
};
use wyrd_spec::vala::ids::{RunId, SpanId, TraceId};
use wyrd_spec::verification::TaskId;
use wyrd_spec::verification::execution::{
    PullRequestActivation, VerificationExecutionState, VerificationRequest,
};
use wyrd_spec::verification::{
    DriftWindow, FrozenTarget, GroupEndSettings, OperatorDispatchState, VerificationBindingStatus,
    VerificationError, VerificationVerdict, VerifierReadiness,
};

use crate::queries::verification::{BindingSchedule, InactivityTimeout, binding_activity};
use crate::{OperatorPool, TenantConn};

mod pull_requests;

pub use pull_requests::{
    PullRequestBinding, PullRequestEnd, PullRequestExecution, PullRequestGeneration,
    PullRequestHistory, PullRequestKey, PullRequestTarget, access_lost, superseded,
};

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

/// Insert one acknowledged scheduled execution under its prepared ID, due at
/// PostgreSQL's statement time; a duplicate occurrence conflicts on its
/// partial unique index and a binding deleted since preparation joins no row,
/// so either inserts nothing.
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
    )
    SELECT $1, wyrd.current_tenant(), $2, $3, $4, 'schedule', $5, $6, $7, $8, $9,
           $10, $11, $12, $13, $14, $15,
           statement_timestamp(), statement_timestamp(), statement_timestamp()
     WHERE EXISTS (SELECT 1 FROM wyrd.verification_bindings WHERE binding_id = $6)
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

/// Insert a batch of submitted executions, column by column: each row is
/// either `pending` and due at PostgreSQL's statement time or, when its
/// `error` carries one, already settled `errored`.
///
/// The exact Verifier version and subject come from their Card rows, and a
/// Card the tenant does not hold joins no row, so the statement skips that
/// submission rather than failing the batch on a foreign key. An existing
/// execution ID inserts nothing, which absorbs a retried batch.
const INSERT_SUBMISSIONS_SQL: &str = r"
    INSERT INTO wyrd.verifier_executions (
        execution_id, data_tenant_id, verifier_card_uid, verifier_version, card_uid,
        origin, run_id,
        sub_principal_id, act_principal_id, authority, cause_trace_id, cause_span_id,
        idempotency_key, request_sha256, status, error, max_attempts,
        next_attempt_at, settled_at, created_at, updated_at, cause_sampled, group_id,
        deployment_environment, delivery_due_at
    )
    SELECT r.execution_id, wyrd.current_tenant(), v.card_uid, v.version, s.card_uid,
           'manual', r.run_id,
           r.sub_principal_id, r.act_principal_id, r.authority, r.cause_trace_id,
           r.cause_span_id, r.idempotency_key, r.request_sha256,
           CASE WHEN r.error IS NULL THEN 'pending' ELSE 'errored' END, r.error, $15,
           CASE WHEN r.error IS NULL THEN statement_timestamp() END,
           CASE WHEN r.error IS NOT NULL THEN statement_timestamp() END,
           statement_timestamp(), statement_timestamp(), r.cause_sampled, r.group_id,
           r.deployment_environment,
           CASE WHEN r.error IS NOT NULL THEN statement_timestamp() END
      FROM unnest($1::uuid[], $2::uuid[], $3::uuid[], $4::text[], $5::uuid[],
                  $6::uuid[], $7::jsonb[], $8::bytea[], $9::bytea[], $10::text[],
                  $11::bytea[], $12::jsonb[], $13::bool[], $14::text[], $16::text[])
           AS r(execution_id, verifier_card_uid, card_uid, run_id, sub_principal_id, act_principal_id, authority,
                cause_trace_id, cause_span_id, idempotency_key, request_sha256,
                error, cause_sampled, group_id, deployment_environment)
      JOIN wyrd.cards v ON v.card_uid = r.verifier_card_uid
      JOIN wyrd.cards s ON s.card_uid = r.card_uid
    ON CONFLICT DO NOTHING
";

/// The batch's execution IDs that already exist in the caller's tenant.
const EXISTING_EXECUTIONS_SQL: &str = r"
    SELECT execution_id FROM wyrd.verifier_executions WHERE execution_id = ANY($1)
";

/// Serialize concurrent submissions that share one subject, actor, and key,
/// locking every (subject, actor, key) of a batch in one statement.
///
/// Each lock is taken in ascending hash order, the one order every batch
/// uses, so two batches sharing keys never deadlock. Transaction-scoped, so
/// the locks release on commit or rollback; the class keeps them apart from
/// every other advisory lock in the database.
const LOCK_REQUEST_KEYS_SQL: &str = r"
    SELECT pg_advisory_xact_lock($1, hash)
      FROM (SELECT DISTINCT hashtext(k.subject::text || '/' || k.actor::text || '/' || k.key) AS hash
              FROM unnest($2::uuid[], $3::uuid[], $4::text[]) AS k(subject, actor, key)
             ORDER BY hash) AS keys
";

/// Advisory lock class of [`LOCK_REQUEST_KEYS_SQL`].
const REQUEST_KEY_LOCK_CLASS: i32 = 0x0C_A2_D0_31;

/// Find the executions a batch's (subject, actor, key) triples already hold,
/// each returned with the 1-based position of its triple in the arrays.
///
/// Each triple probes `verifier_runs_manual_idempotency` once through the
/// lateral subquery, which names the index's leading tenant column, so the
/// cost follows the batch's keys and never the tenant's execution count. A
/// plain join left the plan to row estimates; with stale statistics it read
/// and sorted every execution of the tenant on each batch.
const KEYED_EXECUTIONS_SQL: &str = r"
    SELECT k.position, e.execution_id, e.request_sha256
      FROM unnest($1::uuid[], $2::uuid[], $3::text[]) WITH ORDINALITY AS k(subject, actor, key, position)
     CROSS JOIN LATERAL (
           SELECT execution_id, request_sha256
             FROM wyrd.verifier_executions
            WHERE data_tenant_id = wyrd.current_tenant()
              AND sub_principal_id = k.subject
              AND act_principal_id = k.actor
              AND idempotency_key = k.key
            LIMIT 1
           ) AS e
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
/// Bindings in `$2`, whose candidates this process is already handing off,
/// are passed over.
const DUE_BINDING_SQL: &str = r"
    SELECT binding_id, verifier_card_uid, card_uid, schedule_cron, schedule_tz, next_run_at,
           statement_timestamp() - ($1::bigint * INTERVAL '1 millisecond') AS schedule_clock
      FROM wyrd.verification_bindings
     WHERE activation = 'schedule'
       AND next_run_at + ($1::bigint * INTERVAL '1 millisecond') <= statement_timestamp()
       AND binding_id <> ALL($2::uuid[])
     ORDER BY next_run_at, binding_id
     LIMIT 1
       FOR UPDATE SKIP LOCKED
";

/// Record acknowledged source activity of grouped items: every `group_end`
/// binding of the recorded owner with a source slot for the item's exact
/// subject and source Verifier gets its group due `after` past PostgreSQL's
/// statement time, moving an existing pending row's due time.
///
/// `$1`..`$4` are aligned owner, subject, source Verifier, and group arrays.
/// `after` is the exact authored duration, at most the one year Trigger
/// validation admits; it is added as elapsed seconds, so a day is always
/// 86,400 seconds whatever the session time zone's daylight time. `DISTINCT` keeps one row per binding and group so the
/// upsert never touches a row twice, and rows are upserted in key order so
/// concurrent writers of one tenant lock shared groups in the same order
/// rather than deadlocking.
const RECORD_GROUP_ACTIVITY_SQL: &str = r"
    INSERT INTO wyrd.verification_group_pending (data_tenant_id, binding_id, group_id, due_at)
    SELECT DISTINCT wyrd.current_tenant(), b.binding_id, a.group_id,
           statement_timestamp()
               + extract(epoch FROM (b.group_end ->> 'after')::interval) * INTERVAL '1 second'
      FROM unnest($1::uuid[], $2::uuid[], $3::uuid[], $4::text[])
               AS a(owner_card_uid, card_uid, verifier_card_uid, group_id)
      JOIN wyrd.verification_bindings b
        ON b.activation = 'group_end' AND b.owner_card_uid = a.owner_card_uid
     WHERE EXISTS (
           SELECT 1 FROM jsonb_each(b.group_end -> 'sources') AS s(slot, source)
            WHERE (s.source ->> 'card_uid')::uuid = a.card_uid
              AND (s.source ->> 'verifier_card_uid')::uuid = a.verifier_card_uid)
     ORDER BY b.binding_id, a.group_id
    ON CONFLICT (data_tenant_id, binding_id, group_id)
    DO UPDATE SET due_at = EXCLUDED.due_at, generation = gen_random_uuid()
";

/// Lock the earliest expired pending group no other scheduler holds; forced
/// RLS confines it to the caller's tenant. `overdue` reports, by
/// PostgreSQL's clock, whether the group is more than five minutes past due,
/// the bound on retrying an incomplete snapshot. Groups in the aligned `$1`
/// binding and `$2` group arrays, whose candidates this process is already
/// handing off, are passed over.
const DUE_GROUP_SQL: &str = r"
    SELECT binding_id, group_id, due_at,
           statement_timestamp() - due_at > INTERVAL '5 minutes' AS overdue, generation
      FROM wyrd.verification_group_pending
     WHERE due_at <= statement_timestamp()
       AND (binding_id, group_id) NOT IN (SELECT * FROM unnest($1::uuid[], $2::text[]))
     ORDER BY due_at, binding_id, group_id
     LIMIT 1
       FOR UPDATE SKIP LOCKED
";

/// Read a pending group's binding owner, frozen `group_end` settings, group
/// Verifier, and subject.
const GROUP_BINDING_SQL: &str = r"
    SELECT owner_card_uid, group_end, verifier_card_uid, card_uid
      FROM wyrd.verification_bindings
     WHERE binding_id = $1
";

/// Consume a claimed pending group.
const CONSUME_GROUP_SQL: &str = r"
    DELETE FROM wyrd.verification_group_pending
     WHERE binding_id = $1 AND group_id = $2
";

/// Consume a pending group only while it still holds generation `$3`, the
/// activity an admitted candidate was selected from; newer activity keeps it.
const CONSUME_GROUP_GENERATION_SQL: &str = r"
    DELETE FROM wyrd.verification_group_pending
     WHERE binding_id = $1 AND group_id = $2 AND generation = $3
";

/// Insert one acknowledged group execution judging a pinned snapshot under its
/// prepared ID, due at PostgreSQL's statement time; a snapshot with the same
/// content digest (`$11`) for the binding and group conflicts on its unique
/// index and a binding deleted since preparation joins no row, so either
/// inserts nothing.
const INSERT_GROUP_SQL: &str = r"
    INSERT INTO wyrd.verifier_executions (
        execution_id, data_tenant_id, verifier_card_uid, verifier_version, card_uid,
        origin, owner_card_uid, binding_id, trigger_uid, trigger_digest, operators,
        group_id, group_snapshot_sha256,
        sub_principal_id, act_principal_id, authority,
        max_attempts, next_attempt_at, created_at, updated_at
    )
    SELECT $1, wyrd.current_tenant(), $2, $3, $4, 'group', $5, $6, $7, $8, $9,
           $10, $11, $12, $13, $14, $15,
           statement_timestamp(), statement_timestamp(), statement_timestamp()
     WHERE EXISTS (SELECT 1 FROM wyrd.verification_bindings WHERE binding_id = $6)
    ON CONFLICT DO NOTHING
    RETURNING execution_id
";

/// Record one pending group refused before admission as a terminal errored
/// group execution of binding `$2` and group `$3`, frozen from the binding
/// like a runnable one but with no accepted authority, inputs, or snapshot:
/// `$4` is the stored owner principal it would have served, `$5` the SYSTEM
/// principal recording the refusal, `$6` the structured error. A missing
/// binding or Verifier Card inserts nothing.
const INSERT_GROUP_ERROR_SQL: &str = r"
    INSERT INTO wyrd.verifier_executions (
        execution_id, data_tenant_id, verifier_card_uid, verifier_version, card_uid,
        origin, owner_card_uid, binding_id, trigger_uid, trigger_digest, operators,
        group_id, sub_principal_id, act_principal_id, status, error, max_attempts,
        created_at, updated_at, settled_at, delivery_due_at
    )
    SELECT $1, wyrd.current_tenant(), b.verifier_card_uid, v.version, b.card_uid,
           'group', b.owner_card_uid, b.binding_id, b.trigger_uid, b.trigger_digest,
           b.operators, $3, $4, $5, 'errored', $6, $7,
           statement_timestamp(), statement_timestamp(), statement_timestamp(),
           statement_timestamp()
      FROM wyrd.verification_bindings b
      JOIN wyrd.cards v ON v.card_uid = b.verifier_card_uid
     WHERE b.binding_id = $2
    RETURNING execution_id
";

/// Find the group execution of one binding and group holding a snapshot digest.
const EXISTING_SNAPSHOT_SQL: &str = r"
    SELECT execution_id
      FROM wyrd.verifier_executions
     WHERE binding_id = $1 AND group_id = $2 AND origin = 'group'
       AND group_snapshot_sha256 = $3
";

/// One applied [`COMPLETE_SQL`] settlement: the binding, its frozen
/// Operators, whether the execution is a group execution, and whether the
/// settlement replayed an earlier identical one.
type SettledRow = (Option<Uuid>, Json<Vec<FrozenTarget>>, bool, bool);

/// Lock the binding of group execution `$1` so its group completions settle
/// one at a time; selects nothing for any other execution.
const LOCK_GROUP_BINDING_SQL: &str = r"
    SELECT b.binding_id
      FROM wyrd.verification_bindings b
      JOIN wyrd.verifier_executions e ON e.binding_id = b.binding_id
     WHERE e.execution_id = $1 AND e.origin = 'group'
       FOR UPDATE OF b
";

/// The latest other settled group verdict of execution `$1`'s binding and
/// group, in settlement order, which its verdict is compared with.
const PREVIOUS_GROUP_SQL: &str = r"
    SELECT p.result_id, p.group_verdict
      FROM wyrd.verifier_executions e
      JOIN wyrd.verifier_executions p
        ON p.binding_id = e.binding_id AND p.group_id = e.group_id
     WHERE e.execution_id = $1
       AND p.execution_id <> e.execution_id
       AND p.origin = 'group' AND p.group_verdict IS NOT NULL
     ORDER BY p.settled_at DESC, p.execution_id DESC
     LIMIT 1
";

/// Move a claimed binding's cursor; NULL disarms an unparseable schedule.
const ADVANCE_CURSOR_SQL: &str = r"
    UPDATE wyrd.verification_bindings
       SET next_run_at = $2
     WHERE binding_id = $1
";

/// Move a binding's cursor to `$2` only while it still holds `$3`, the cursor
/// an admitted occurrence was prepared from, so a replayed admission or an
/// edited schedule is never moved again.
const ADVANCE_EXPECTED_CURSOR_SQL: &str = r"
    UPDATE wyrd.verification_bindings
       SET next_run_at = $2
     WHERE binding_id = $1 AND next_run_at = $3
";

/// Execution record format this worker understands.
///
/// A claim skips any execution whose `worker_format` is newer, so a record
/// written for newer checkpoint semantics is left for a worker that can run
/// it instead of being misread during a rolling upgrade.
const WORKER_FORMAT: i32 = 2;

/// Claim up to `$2` of the oldest due, retry-due, or lease-expired executions,
/// each under its own fresh lease, in one statement.
///
/// An expired lease with no attempt left is claimed without counting an
/// attempt and returned as `exhausted`, so its runner delivers a terminal
/// outcome the execution already selected, or settles the exhaustion,
/// without computing; the queue cannot see Bifrost to decide that itself.
/// Every claimed row draws its own random lease token, returned as `lease_token`, so each
/// claim renews and settles independently.
///
/// Due and lease-expired rows are each read in the order of their own
/// partial index (`verifier_runs_due`, `verifier_runs_leased`) and merged, so
/// a claim reads at most twice `$3` rows however deep the queue is; one
/// predicate over both would read and sort every due row. Rows locked by
/// either scan but not claimed stay locked only until the claim commits,
/// and concurrent claims skip them.
///
/// The first claim fixes the execution deadline `$3` milliseconds after its
/// statement time and every claim returns what remains of it, so a reclaim
/// never replenishes it.
///
/// Availability, expiry, and the new lease deadline are all PostgreSQL's
/// statement time; `$1` is only the lease length in milliseconds, so a pod
/// whose clock differs from the database can neither claim early nor hold a
/// lease the database believes already expired. Only executions whose
/// `worker_format` is at most `$4`, this worker's [`WORKER_FORMAT`], are
/// claimed, so a newer record keeps its attempts and checkpoints
/// for a worker that understands it. Each claim also returns the
/// Verifier's ready fitted Drift baselines keyed by requirement, so the drift
/// engine opens no connection to read them; the Verifier, subject, and SYSTEM
/// principal are resolved by the caller through `wyrd.verification_target`.
const CLAIM_SQL: &str = r"
    WITH due AS (
        SELECT execution_id, next_attempt_at AS due_at, false AS exhausted
          FROM wyrd.verifier_executions
         WHERE status IN ('pending', 'retrying') AND next_attempt_at <= statement_timestamp()
           AND worker_format <= $4
         ORDER BY next_attempt_at, execution_id
         LIMIT $2
           FOR UPDATE SKIP LOCKED
    ),
    expired AS (
        SELECT execution_id, lease_expires_at AS due_at, attempts >= max_attempts AS exhausted
          FROM wyrd.verifier_executions
         WHERE status = 'running' AND lease_expires_at <= statement_timestamp()
           AND worker_format <= $4
         ORDER BY lease_expires_at, execution_id
         LIMIT $2
           FOR UPDATE SKIP LOCKED
    ),
    candidate AS (
        SELECT execution_id, due_at, exhausted FROM due
        UNION ALL
        SELECT execution_id, due_at, exhausted FROM expired
        ORDER BY due_at, execution_id
        LIMIT $2
    )
    UPDATE wyrd.verifier_executions e
       SET status = 'running',
           attempts = CASE WHEN candidate.exhausted THEN e.attempts ELSE e.attempts + 1 END,
           lease_token = gen_random_uuid(),
           lease_expires_at = statement_timestamp() + ($1::bigint * INTERVAL '1 millisecond'),
           deadline_at = COALESCE(e.deadline_at,
                                  statement_timestamp() + ($3::bigint * INTERVAL '1 millisecond')),
           next_attempt_at = NULL, updated_at = statement_timestamp()
      FROM candidate
     WHERE e.execution_id = candidate.execution_id
    RETURNING e.execution_id, e.lease_token, e.verifier_card_uid, e.verifier_version, e.card_uid,
              e.origin, e.owner_card_uid, e.binding_id, e.trigger_uid,
              e.trigger_digest, e.window_start, e.window_end,
              e.run_id,
              e.sub_principal_id, e.act_principal_id, e.authority, e.attempts,
              e.max_attempts, e.lease_expires_at, e.deadline_at, e.cause_trace_id,
              e.cause_span_id, e.cause_sampled, e.group_id, e.deployment_environment,
              e.pull_request, candidate.exhausted,
              GREATEST(0, (EXTRACT(EPOCH FROM statement_timestamp() - candidate.due_at)
                           * 1000)::bigint) AS queue_wait_ms,
              GREATEST(0, (EXTRACT(EPOCH FROM statement_timestamp() - e.created_at)
                           * 1000)::bigint) AS age_ms,
              GREATEST(0, (EXTRACT(EPOCH FROM e.deadline_at - statement_timestamp())
                           * 1000)::bigint) AS remaining_ms,
              (SELECT jsonb_object_agg(f.requirement_id, f.fitted)
                 FROM wyrd.drift_baselines f
                WHERE f.verifier_uid = e.verifier_card_uid
                  AND f.state = 'ready'
                  AND f.requirement_id IS NOT NULL) AS fitted_baselines
";

/// Renew every listed lease still held, on the database clock, through
/// `wyrd.renew_verifier_leases`.
///
/// `$2` is the lease length and `$3` the remaining time under which a lease is
/// renewed, both in milliseconds, so a lease is extended only once a third of
/// it has passed. Every unexpired lease still held is returned, renewed or
/// not; a token absent from the result has expired, been reclaimed, or been
/// settled. The function locks rows in `execution_id` order, the order
/// [`COMPLETE_MANY_SQL`] also locks in, so a renewal and a batched completion
/// sharing executions never deadlock.
const RENEW_LEASES_SQL: &str = r"
    SELECT * FROM wyrd.renew_verifier_leases($1, $2, $3)
";

/// Complete a leased execution, or re-apply the same completion idempotently.
///
/// A group execution also keeps its verdict (`$4`) so later group results of
/// its binding and group can be compared with it. `replayed` is whether the
/// execution had already completed before this statement, read from the
/// statement's snapshot of the row.
const COMPLETE_SQL: &str = r"
    UPDATE wyrd.verifier_executions e
       SET status = 'completed', result_id = $3, error = NULL,
           group_verdict = CASE WHEN e.origin = 'group' THEN $4 END,
           lease_expires_at = NULL, next_attempt_at = NULL,
           settled_at = COALESCE(e.settled_at, statement_timestamp()),
           updated_at = statement_timestamp()
      FROM (SELECT status = 'completed' AS replayed
              FROM wyrd.verifier_executions WHERE execution_id = $1) prior
     WHERE e.execution_id = $1
       AND e.lease_token = $2
       AND (e.status = 'running' OR (e.status = 'completed' AND e.result_id = $3))
    RETURNING e.binding_id, e.operators, e.origin = 'group' AS grouped, prior.replayed
";

/// Complete many leased non-group executions in one round trip through
/// `wyrd.complete_verifier_executions`, or re-apply the same completions
/// idempotently, exactly as [`COMPLETE_SQL`] does one: `$1` executions, `$2`
/// their lease tokens, and `$3` their results, aligned by position. Returns
/// each applied execution with its binding and frozen Operators; a group
/// execution, a lost lease, or a different result returns nothing.
///
/// The function updates one row at a time by primary key in `execution_id`
/// order, the order [`RENEW_LEASES_SQL`] also locks in, so its plan never
/// depends on row estimates and concurrent batches and renewals sharing
/// executions never deadlock.
const COMPLETE_MANY_SQL: &str = r"
    SELECT execution_id, binding_id, operators
      FROM wyrd.complete_verifier_executions($1, $2, $3)
";

/// Insert one Operator dispatch; the (tenant, execution, Operator) key absorbs retries.
///
/// The failure context is frozen here from the settled execution and its
/// exact Verifier and subject Cards plus the runner's bounded summary (`$5`),
/// for a Verifier with one drift check that check's counts (`$6`), the
/// settled verdict (`$7`) and, for a group verdict change, the preceding
/// group result and verdict (`$8`, `$9`), so every delivery attempt renders
/// the same payload.
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
               'verifier_ref', v.kind || '/' || v.name || '@' || v.version,
               'subject_uid', e.card_uid,
               'subject_ref', s.kind || '/' || s.name || '@' || s.version,
               'verdict', $7::text,
               'completed_at', e.settled_at,
               'summary', $5::text
           ) || CASE WHEN $6::jsonb IS NULL THEN '{}'::jsonb
                     ELSE jsonb_build_object('drift', $6::jsonb) END
             || CASE WHEN $9::text IS NULL THEN '{}'::jsonb
                     ELSE jsonb_build_object('group', jsonb_build_object(
                              'group_id', e.group_id,
                              'previous_result_id', $8::uuid,
                              'previous_verdict', $9::text)) END,
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

/// Settle a leased execution in a terminal non-completed status; `$5` is
/// whether its decision is still owed to its retained state, which makes its
/// delivery due at once.
const TERMINATE_SQL: &str = r"
    UPDATE wyrd.verifier_executions
       SET status = $3, error = $4, lease_expires_at = NULL,
           next_attempt_at = NULL, settled_at = statement_timestamp(),
           delivery_due_at = CASE WHEN $5 THEN statement_timestamp() END,
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

/// Cancel one unfinished execution; a settled one is left as it is.
///
/// Clearing the lease and leaving `running` fences the worker holding it: every
/// later write it attempts is fenced on `status = 'running'` and its lease.
/// Its delivery is due at once.
const CANCEL_SQL: &str = r"
    UPDATE wyrd.verifier_executions
       SET status = 'cancelled', error = $2, lease_expires_at = NULL,
           next_attempt_at = NULL, settled_at = statement_timestamp(),
           delivery_due_at = statement_timestamp(), updated_at = statement_timestamp()
     WHERE execution_id = $1 AND status IN ('pending', 'retrying', 'running')
";

/// Cancel every unfinished execution a principal submitted or accepted, each
/// delivery due at once.
const CANCEL_PRINCIPAL_SQL: &str = r"
    UPDATE wyrd.verifier_executions
       SET status = 'cancelled', error = $2, lease_expires_at = NULL,
           next_attempt_at = NULL, settled_at = statement_timestamp(),
           delivery_due_at = statement_timestamp(), updated_at = statement_timestamp()
     WHERE (sub_principal_id = $1 OR act_principal_id = $1)
       AND status IN ('pending', 'retrying', 'running')
";

/// Claim up to `$2` due terminal deliveries of `worker_format` at most `$3`,
/// each moved `$1` milliseconds ahead on the database clock. The moved
/// instant is the claim's fence, so a claimant that dies leaves the delivery
/// due again once it passes and only the latest claimant settles it.
const CLAIM_DELIVERIES_SQL: &str = r"
    UPDATE wyrd.verifier_executions e
       SET delivery_due_at = statement_timestamp() + ($1::bigint * INTERVAL '1 millisecond')
      FROM (SELECT execution_id FROM wyrd.verifier_executions
             WHERE delivery_due_at <= statement_timestamp() AND worker_format <= $3
             ORDER BY delivery_due_at, execution_id
             LIMIT $2
               FOR UPDATE SKIP LOCKED) due
     WHERE e.execution_id = due.execution_id
    RETURNING e.execution_id, e.delivery_due_at, e.status, e.error, e.verifier_card_uid,
              e.verifier_version, e.card_uid, e.origin, e.owner_card_uid, e.binding_id,
              e.trigger_uid, e.trigger_digest, e.window_start, e.window_end, e.run_id,
              e.group_id, e.deployment_environment, e.sub_principal_id, e.authority,
              e.pull_request
";

/// Settle the terminal delivery claimed until `$2`: retire it when `$3` is
/// null, else make it due again `$3` milliseconds from now.
const SETTLE_DELIVERY_SQL: &str = r"
    UPDATE wyrd.verifier_executions
       SET delivery_due_at = statement_timestamp() + ($3::bigint * INTERVAL '1 millisecond')
     WHERE execution_id = $1 AND delivery_due_at = $2
";

/// Which of principals `$1` are no longer active, read once every listed
/// principal row is share-locked in a stable order.
///
/// The share lock conflicts with the suspension a revocation writes, so it
/// waits out a revocation in flight and reads its committed status, and a
/// later revocation waits for the admitting commit and then cancels what it
/// admitted. An id with no principal row is never inactive.
const INACTIVE_PRINCIPALS_SQL: &str = r"
    WITH users AS (
        SELECT id, status FROM wyrd.auth_users WHERE id = ANY($1) ORDER BY id FOR SHARE
    ), machines AS (
        SELECT id, status FROM wyrd.auth_service_accounts
         WHERE id = ANY($1) ORDER BY id FOR SHARE
    )
    SELECT id FROM users WHERE status <> 'active'
    UNION
    SELECT id FROM machines WHERE status <> 'active'
";

/// Read one execution's control-plane status.
const STATUS_SQL: &str = r"
    SELECT execution_id, status, sub_principal_id, act_principal_id,
           verifier_card_uid, card_uid, result_id, error,
           group_id, owner_card_uid, deployment_environment, pull_request
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
             ORDER BY e.created_at DESC, e.execution_id DESC LIMIT 1) AS last_execution_id,
           b.group_end
      FROM wyrd.verification_bindings b
     WHERE b.binding_id = $1
";

/// List every tenant with claimable executions, longest-waiting tenant first.
///
/// The recursive terms walk `verifier_runs_due` and `verifier_runs_leased`
/// backward one distinct tenant at a time, so each step lands on a tenant's
/// newest entry. Claimed and settled rows leave dead entries at the oldest
/// end of each tenant's range until vacuum removes them, and a forward walk
/// would read through them on every claim round. Each tenant's oldest
/// claimable row is then probed once per index, in a materialized step so the
/// probe is not repeated for the filter and the order. A scan therefore costs
/// the tenants with queued or running work and never the rows they hold;
/// grouping the claimable rows instead read the whole backlog every round.
/// Tenants owing a due terminal delivery are walked the same way through the
/// partial index of owed deliveries, which holds no settled history.
const RUNNABLE_TENANTS_SQL: &str = r"
    WITH RECURSIVE queued(tenant) AS (
        (SELECT data_tenant_id FROM wyrd.verifier_executions
          WHERE status IN ('pending', 'retrying')
          ORDER BY data_tenant_id DESC, next_attempt_at DESC LIMIT 1)
        UNION ALL
        SELECT (SELECT e.data_tenant_id FROM wyrd.verifier_executions e
                 WHERE e.status IN ('pending', 'retrying') AND e.data_tenant_id < q.tenant
                 ORDER BY e.data_tenant_id DESC, e.next_attempt_at DESC LIMIT 1)
          FROM queued q
         WHERE q.tenant IS NOT NULL
    ), leased(tenant) AS (
        (SELECT data_tenant_id FROM wyrd.verifier_executions
          WHERE status = 'running'
          ORDER BY data_tenant_id DESC, lease_expires_at DESC LIMIT 1)
        UNION ALL
        SELECT (SELECT e.data_tenant_id FROM wyrd.verifier_executions e
                 WHERE e.status = 'running' AND e.data_tenant_id < l.tenant
                 ORDER BY e.data_tenant_id DESC, e.lease_expires_at DESC LIMIT 1)
          FROM leased l
         WHERE l.tenant IS NOT NULL
    ), waiting AS MATERIALIZED (
        SELECT t.tenant,
               LEAST(
                   (SELECT min(e.next_attempt_at) FROM wyrd.verifier_executions e
                     WHERE e.data_tenant_id = t.tenant
                       AND e.status IN ('pending', 'retrying')
                       AND e.next_attempt_at <= statement_timestamp()),
                   (SELECT min(e.lease_expires_at) FROM wyrd.verifier_executions e
                     WHERE e.data_tenant_id = t.tenant
                       AND e.status = 'running'
                       AND e.lease_expires_at <= statement_timestamp())) AS since
          FROM (SELECT tenant FROM queued WHERE tenant IS NOT NULL
                UNION
                SELECT tenant FROM leased WHERE tenant IS NOT NULL) AS t
    ), owing(tenant) AS (
        (SELECT data_tenant_id FROM wyrd.verifier_executions
          WHERE delivery_due_at IS NOT NULL
          ORDER BY data_tenant_id DESC, delivery_due_at DESC LIMIT 1)
        UNION ALL
        SELECT (SELECT e.data_tenant_id FROM wyrd.verifier_executions e
                 WHERE e.delivery_due_at IS NOT NULL AND e.data_tenant_id < o.tenant
                 ORDER BY e.data_tenant_id DESC, e.delivery_due_at DESC LIMIT 1)
          FROM owing o
         WHERE o.tenant IS NOT NULL
    ), delivering AS MATERIALIZED (
        SELECT o.tenant,
               (SELECT min(e.delivery_due_at) FROM wyrd.verifier_executions e
                 WHERE e.data_tenant_id = o.tenant
                   AND e.delivery_due_at <= statement_timestamp()) AS since
          FROM owing o
         WHERE o.tenant IS NOT NULL
    )
    SELECT tenant
      FROM (SELECT tenant, since FROM waiting
            UNION ALL
            SELECT tenant, since FROM delivering) AS due
     WHERE since IS NOT NULL
     GROUP BY tenant
     ORDER BY min(since), tenant
";

/// List tenants with due scheduled bindings, most overdue tenant first.
///
/// Uses the same dueness rule as [`DUE_BINDING_SQL`], where `$2` is
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

/// List tenants with expired pending groups, most overdue tenant first.
const DUE_GROUP_TENANTS_SQL: &str = r"
    SELECT data_tenant_id
      FROM wyrd.verification_group_pending
     WHERE due_at <= statement_timestamp()
     GROUP BY data_tenant_id
     ORDER BY min(due_at), data_tenant_id
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

/// Histogram of every verification queue and group-scheduling store
/// operation, seconds, labelled `op` (the operation, such as `claim` or
/// `group_enqueue`) and `outcome` (`ok` or `error`). It times the operation's
/// statements from first send to last row, inside the caller's transaction,
/// so pool acquisition and commit are not included.
pub const VERIFICATION_STORE_DURATION_SECONDS: &str = "wyrd_verification_store_duration_seconds";

/// Await `work` as one `op` store operation timed into
/// [`VERIFICATION_STORE_DURATION_SECONDS`]; a future dropped before
/// completion records nothing.
///
/// # Errors
/// Returns `work`'s own error unchanged.
async fn timed<T>(
    op: &'static str,
    work: impl std::future::Future<Output = Result<T, SqlxError>>,
) -> Result<T, SqlxError> {
    let started = std::time::Instant::now();
    let result = work.await;
    metrics::histogram!(
        VERIFICATION_STORE_DURATION_SECONDS,
        "op" => op,
        "outcome" => if result.is_ok() { "ok" } else { "error" }
    )
    .record(started.elapsed().as_secs_f64());
    result
}

/// Insert one `pending` dispatch per frozen Operator of the just-settled
/// `execution`, each freezing its failure context from the settled row,
/// `summary` clipped to [`MAX_SUMMARY_CHARS`], `drift`, `verdict`, and, for a
/// group verdict change, the `previous` group result and verdict.
///
/// The (tenant, execution, Operator) key makes a retried or replayed
/// settlement insert nothing new.
///
/// # Errors
/// Returns the database error when an insert fails.
async fn insert_dispatches(
    conn: &mut TenantConn<'_>,
    execution: ExecutionId,
    operators: &[FrozenTarget],
    summary: &str,
    drift: Option<DriftCounts>,
    verdict: &str,
    previous: Option<(Uuid, String)>,
) -> Result<(), SqlxError> {
    let (previous_result, previous_verdict) = previous.unzip();
    let summary: String = summary.chars().take(MAX_SUMMARY_CHARS).collect();
    for operator in operators {
        let (uid, digest) = match operator {
            FrozenTarget::Uid(uid) => (Some(uid.as_uuid()), None),
            FrozenTarget::Digest(digest) => (None, Some(digest.as_str())),
        };
        sqlx::query(INSERT_DISPATCH_SQL)
            .bind(OperatorDispatchId::new_v7().as_uuid())
            .bind(execution.as_uuid())
            .bind(uid)
            .bind(digest)
            .bind(&summary)
            .bind(drift.map(Json))
            .bind(verdict)
            .bind(previous_result)
            .bind(previous_verdict.as_deref())
            .execute(&mut **conn.transaction())
            .await?;
    }
    Ok(())
}

/// Why an execution was created.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutionOrigin {
    /// A due cron occurrence of a scheduled binding.
    Schedule,
    /// A caller's `POST /v1/verification/enqueue` submission.
    Manual,
    /// A `group_end` binding's judgment of one pinned group snapshot.
    Group,
    /// A `pull_request` binding's review of one admitted PR source.
    PullRequest,
}

impl ExecutionOrigin {
    /// Stored `origin` column value.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Schedule => "schedule",
            Self::Manual => "manual",
            Self::Group => "group",
            Self::PullRequest => "pull_request",
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
            "group" => Ok(Self::Group),
            "pull_request" => Ok(Self::PullRequest),
            other => Err(SqlxError::Decode(
                format!("unknown verifier execution origin {other:?}").into(),
            )),
        }
    }
}

/// The frozen input one claimed execution judges.
#[derive(Debug, Clone, PartialEq)]
pub enum ExecutionInput {
    /// A scheduled occurrence: its fixed window. Every samples slot of the
    /// exact Verifier is read for the subject within it.
    Scheduled {
        /// The occurrence's comparison window.
        window: DriftWindow,
    },
    /// A caller submission. Its declarations and route overrides live in
    /// its Bifrost input record, never in PostgreSQL.
    Submitted {
        /// Application Run the execution belongs to, when the caller named one.
        run_id: Option<RunId>,
    },
    /// A group snapshot. Its selector lives in the execution's Bifrost input
    /// record; PostgreSQL holds only the snapshot digest.
    Grouped,
    /// A pull request source admitted for a `pull_request` binding: the
    /// pins frozen at admission, which every attempt reviews.
    PullRequest {
        /// The admitted PR source.
        pull_request: Box<PullRequestActivation>,
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

/// What preparing one scheduled occurrence or group snapshot decided, before
/// anything is written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Prepared<T> {
    /// New work to publish and then admit once Scribe acknowledges its input.
    Candidate(T),
    /// The occurrence or snapshot already has this execution.
    Existing(ExecutionId),
    /// The target refused the work.
    Refused(EnqueueRefusal),
}

/// One prepared scheduled occurrence: its execution ID and everything frozen
/// at preparation, admitted by [`VerifierExecutionQueue::accept_scheduled`].
#[derive(Debug, Clone, PartialEq)]
pub struct ScheduledAdmission {
    /// The execution ID the input record and the queue row share.
    pub execution_id: ExecutionId,
    /// The scheduled binding.
    pub binding_id: BindingId,
    /// The occurrence's fixed window.
    pub window: DriftWindow,
    /// The binding's frozen identities, Trigger, and Operators.
    target: ResolvedTarget,
    /// The owner's accepted authority with the SYSTEM actor.
    authority: ExecutionAuthority,
    /// The expected and next cursor admission moves; `None` moves none.
    cursor: Option<(DateTime<Utc>, DateTime<Utc>)>,
    /// The attempt budget frozen at preparation.
    max_attempts: i32,
}

impl ScheduledAdmission {
    /// The owner's accepted authority with the SYSTEM actor the run serves.
    #[must_use]
    pub const fn authority(&self) -> &ExecutionAuthority {
        &self.authority
    }

    /// The binding owner's root Card the execution serves.
    #[must_use]
    pub fn owner_card_uid(&self) -> Option<CardUid> {
        self.target
            .owner_card_uid
            .and_then(|uid| CardUid::from_uuid(uid).ok())
    }

    /// The tick reporting `outcome` for this occurrence.
    #[must_use]
    pub fn tick(&self, outcome: ScheduleOutcome) -> ScheduleTick {
        ScheduleTick {
            binding_id: self.binding_id,
            due_at: self
                .cursor
                .map_or(self.window.end, |(expected, _)| expected),
            outcome,
            next_run_at: self.cursor.map(|(_, next_run_at)| next_run_at),
        }
    }
}

/// One prepared group snapshot: its execution ID and everything frozen at
/// preparation, admitted by [`VerifierExecutionQueue::accept_grouped`].
#[derive(Debug, Clone, PartialEq)]
pub struct GroupedAdmission {
    /// The execution ID the input record and the queue row share.
    pub execution_id: ExecutionId,
    /// The `group_end` binding.
    pub binding_id: BindingId,
    /// The judged group.
    pub group_id: GroupId,
    /// SHA-256 of the snapshot's canonical content.
    digest: [u8; 32],
    /// The binding's frozen identities, Trigger, and Operators.
    target: ResolvedTarget,
    /// The owner's accepted authority with the SYSTEM actor.
    authority: ExecutionAuthority,
    /// The pending activity generation admission consumes; `None` consumes
    /// none.
    generation: Option<Uuid>,
    /// The attempt budget frozen at preparation.
    max_attempts: i32,
}

/// One queued execution a Scribe-acknowledged input record admits: a
/// caller's submission or a prepared scheduled or group candidate.
#[derive(Debug, Clone, PartialEq)]
pub enum Admission {
    /// A caller's submission.
    Submitted(Submission),
    /// A scheduled occurrence.
    Scheduled(ScheduledAdmission),
    /// A group snapshot.
    Grouped(GroupedAdmission),
    /// A reserved pull request execution, activated by
    /// [`VerifierExecutionQueue::activate_pull_requests`].
    PullRequest(PullRequestAdmission),
}

/// One reserved pull request execution whose input record Scribe
/// acknowledged; everything else was frozen when it was reserved.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PullRequestAdmission {
    /// The reserved execution.
    pub execution_id: ExecutionId,
    /// Who it serves and who accepted it.
    pub attribution: Attribution,
}

impl Admission {
    /// The execution ID the input record and the queue row share.
    #[must_use]
    pub const fn execution_id(&self) -> ExecutionId {
        match self {
            Self::Submitted(submission) => submission.execution_id,
            Self::Scheduled(admission) => admission.execution_id,
            Self::Grouped(admission) => admission.execution_id,
            Self::PullRequest(admission) => admission.execution_id,
        }
    }

    /// The subject and actor the execution runs under, which its input
    /// record is attributed to.
    #[must_use]
    pub fn attribution(&self) -> Attribution {
        match self {
            Self::Submitted(submission) => submission.authority.attribution(),
            Self::Scheduled(admission) => admission.authority.attribution(),
            Self::Grouped(admission) => admission.authority.attribution(),
            Self::PullRequest(admission) => admission.attribution,
        }
    }
}

/// What [`VerifierExecutionQueue::settle_due`] decided for one claimed tick.
#[derive(Debug, Clone, PartialEq)]
pub enum DueSettlement {
    /// The tick settled without new work; its cursor moved.
    Settled(ScheduleTick),
    /// New work to publish and admit; nothing was written.
    Candidate(Box<ScheduledAdmission>),
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
#[derive(Debug, Clone, PartialEq, Eq)]
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
        /// The owner family's runtime account the execution serves.
        owner: PrincipalId,
        /// The binding's exact owner root the account runs under.
        owner_root: CardUid,
        /// The next cursor.
        next_run_at: DateTime<Utc>,
    },
}

/// One due scheduled binding locked by [`VerifierExecutionQueue::claim_due`]
/// until the caller's transaction ends.
///
/// The occurrence, owner activity, and missed-boundary rule are already
/// decided. When the occurrence needs an execution, [`Self::owner`] names the
/// family runtime account and exact root whose current authority the caller
/// resolves, on the same transaction, before
/// [`VerifierExecutionQueue::schedule_due`] settles the tick.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DueBinding {
    /// The claimed binding.
    pub binding_id: BindingId,
    /// The stored cursor that was due.
    pub due_at: DateTime<Utc>,
    /// The binding's Verifier.
    pub verifier_card_uid: CardUid,
    /// The Card the binding verifies, whose observations a run judges.
    pub card_uid: CardUid,
    /// What the occurrence calls for.
    decision: DueDecision,
}

impl DueBinding {
    /// The window an execution of this occurrence judges, or `None` when the
    /// occurrence creates none.
    #[must_use]
    pub const fn window(&self) -> Option<&DriftWindow> {
        match &self.decision {
            DueDecision::Run { window, .. } => Some(window),
            DueDecision::InvalidSchedule | DueDecision::Skip { .. } => None,
        }
    }

    /// The family runtime account and exact owner root an execution would
    /// serve, or `None` when the occurrence creates none and needs no
    /// authority.
    #[must_use]
    pub const fn owner(&self) -> Option<(PrincipalId, &CardUid)> {
        match &self.decision {
            DueDecision::Run {
                owner, owner_root, ..
            } => Some((*owner, owner_root)),
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

/// One expired pending group locked by [`VerifierExecutionQueue::claim_due_group`]
/// until the caller's transaction ends, with its binding's owner and frozen
/// settings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DueGroup {
    /// The group's `group_end` binding.
    pub binding_id: BindingId,
    /// The caller group to judge.
    pub group_id: GroupId,
    /// When the group fell due: the cutoff for its cumulative selection, so
    /// only rows admitted before it are judged.
    pub due_at: DateTime<Utc>,
    /// Binding owner whose attributed source rows form the group.
    pub owner_card_uid: CardUid,
    /// The binding's group Verifier.
    pub verifier_card_uid: CardUid,
    /// The Card the group Verifier verifies.
    pub card_uid: CardUid,
    /// The pinned Trigger duration and source bindings.
    pub settings: GroupEndSettings,
    /// The owner family's runtime account, whatever its status; `None` when
    /// the family has none, so nothing can be attributed to it.
    pub owner: Option<PrincipalId>,
    /// Whether the owner is runtime-active, so its group may be admitted.
    pub active: bool,
    /// Whether the group is more than five minutes past due by PostgreSQL's
    /// statement time, so an incomplete snapshot stops retrying and errors.
    pub overdue: bool,
    /// The generation of the row's latest acknowledged activity, which an
    /// admitted candidate consumes only while the row still holds it.
    pub generation: Uuid,
}

/// One acknowledged grouped source input or result: the recorded
/// owner/subject/source-Verifier tuple and its caller group.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct GroupActivity {
    /// Recorded owner Service Card.
    pub owner_card_uid: CardUid,
    /// Exact Card the source execution verified.
    pub card_uid: CardUid,
    /// Exact source Verifier Card version.
    pub verifier_card_uid: CardUid,
    /// The caller group.
    pub group_id: GroupId,
}

/// One group snapshot to judge: its source selections and the content
/// digest that makes re-reading unchanged evidence enqueue nothing. The
/// selections are written to the execution's Bifrost input record; only the
/// digest is stored in PostgreSQL.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PinnedGroup {
    /// The judged group.
    pub group_id: GroupId,
    /// The selection of each source slot.
    pub sources: BTreeMap<SlotName, GroupSelection>,
    /// SHA-256 of the snapshot's canonical content.
    digest: [u8; 32],
}

/// One source slot's selection while pinning a [`PinnedGroup`]: the authored
/// source and, in list order, each execution with its selected result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GroupSelection {
    /// The authored source declaration.
    pub reference: SourceText,
    /// Each source execution in list order with its selected result, `None`
    /// when no result is visible.
    pub executions: Vec<(ExecutionId, Option<VerificationResultId>)>,
}

impl PinnedGroup {
    /// Pin `sources` for `group_id` of `binding_id`.
    ///
    /// The digest is SHA-256 of the RFC 8785 canonical JSON of the binding,
    /// group, and each slot's ordered execution and selected result IDs, so a
    /// newly visible execution or result changes it even at equal times or
    /// values, while poll time, the selection cutoff, row counts, and
    /// execution status never do.
    ///
    /// # Errors
    /// Returns [`SqlxError::Encode`] when the content cannot be serialized.
    pub fn new(
        binding_id: BindingId,
        group_id: GroupId,
        sources: BTreeMap<SlotName, GroupSelection>,
    ) -> Result<Self, SqlxError> {
        let content = serde_json::json!({
            "binding_id": binding_id,
            "group_id": group_id,
            "sources": sources
                .iter()
                .map(|(slot, selection)| (slot, &selection.executions))
                .collect::<BTreeMap<_, _>>(),
        });
        let canonical =
            serde_jcs::to_vec(&content).map_err(|error| SqlxError::Encode(error.into()))?;
        Ok(Self {
            group_id,
            sources,
            digest: Sha256::digest(canonical).into(),
        })
    }

    /// The snapshot's content digest.
    #[must_use]
    pub const fn digest(&self) -> &[u8; 32] {
        &self.digest
    }
}

/// Opaque fencing token of one claim.
///
/// Only [`VerifierExecutionQueue::claim`] and
/// [`OperatorDispatchQueue::claim`](crate::queries::operator_dispatches::OperatorDispatchQueue::claim)
/// mint one, so a settlement can only be attempted by a worker that actually
/// claimed the execution or dispatch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct LeaseToken(pub(crate) Uuid);

impl LeaseToken {
    /// The token's value, which identifies this claim as the only writer of
    /// the execution's recoverable state.
    #[must_use]
    pub const fn as_uuid(&self) -> Uuid {
        self.0
    }
}

/// The identity a runner settles a claimed execution with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExecutionLease {
    /// The claimed execution.
    pub execution_id: ExecutionId,
    /// This claim's fencing token.
    pub token: LeaseToken,
}

/// One claimed execution's completion, settled with others of its tenant by
/// [`VerifierExecutionQueue::complete_many`].
#[derive(Debug, Clone, PartialEq)]
pub struct Completion {
    /// Settlement identity.
    pub lease: ExecutionLease,
    /// The staged result the execution points at.
    pub result_id: VerificationResultId,
    /// The settled verdict, which decides whether Operators are dispatched.
    pub verdict: VerificationVerdict,
    /// The runner's failure summary, frozen into each dispatch.
    pub summary: String,
    /// The counts of the Verifier's one drift check, when it has exactly one.
    pub drift: Option<DriftCounts>,
}

/// An execution claimed for running, with every frozen identity the runner needs.
#[derive(Debug, Clone, PartialEq)]
pub struct ClaimedExecution {
    /// Settlement identity.
    pub lease: ExecutionLease,
    /// When the lease expires and the execution becomes reclaimable.
    pub lease_expires_at: DateTime<Utc>,
    /// The execution deadline its first claim fixed on the PostgreSQL clock;
    /// a reclaim or renewal never moves it.
    pub deadline_at: DateTime<Utc>,
    /// This attempt's number, starting at 1.
    pub attempt: i32,
    /// Attempt budget frozen at admission.
    pub max_attempts: i32,
    /// Whether the claim took an expired lease of the final attempt, which
    /// counts no attempt: the claimant only delivers a terminal outcome the
    /// execution already selected, or settles it as exhausted, and never
    /// computes.
    pub exhausted: bool,
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
    /// Caller group the execution belongs to; `None` when ungrouped.
    pub group_id: Option<GroupId>,
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
    /// statement's PostgreSQL instant less `created_at`. Telemetry, and the
    /// runner's bound on waiting for the execution's input record to publish.
    pub age: std::time::Duration,
    /// What remains of the execution deadline its first claim fixed, as of
    /// this claim's PostgreSQL instant; zero once it has passed. A reclaim
    /// never replenishes it.
    pub remaining: std::time::Duration,
    /// The Verifier's ready fitted Drift baselines keyed by requirement ID,
    /// read in the claim transaction; empty when it has none.
    pub fitted_baselines: BTreeMap<TaskId, Value>,
    /// The deployment environment tag accepted with the execution, written
    /// unchanged to its result rows; `None` when it carried none. Never an
    /// execution input.
    pub deployment_environment: Option<String>,
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

    /// The terminal status a stored `status` column value names.
    ///
    /// # Errors
    /// Returns [`SqlxError::Decode`] for any other status.
    fn parse(stored: &str) -> Result<Self, SqlxError> {
        match stored {
            "cancelled" => Ok(Self::Cancelled),
            "timed_out" => Ok(Self::TimedOut),
            "errored" => Ok(Self::Errored),
            other => Err(SqlxError::Decode(
                format!("{other} is not a terminal status without a verdict").into(),
            )),
        }
    }
}

/// Whether a terminal decision settled without a verdict is already held by
/// its execution's retained state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TerminalDelivery {
    /// Scribe acknowledged the decision in the state; nothing more is owed.
    Acknowledged,
    /// The state may not hold it yet, so its delivery stays owed, due at once,
    /// until a [delivery claim](VerifierExecutionQueue::claim_deliveries)
    /// settles it.
    Owed,
}

/// A terminal execution whose decision its retained state may not hold yet,
/// claimed by [`VerifierExecutionQueue::claim_deliveries`].
///
/// It carries the sanitized decision and the immutable identities the queue
/// row froze at admission, which a delivery records without computing; a
/// group refused before admission carries no accepted authority or inputs.
#[derive(Debug, Clone, PartialEq)]
pub struct PendingDelivery {
    /// The execution.
    pub execution_id: ExecutionId,
    /// The instant this claim moved the delivery to; it fences the claim's
    /// settlement, and the delivery is due again once it passes.
    pub claimed_until: DateTime<Utc>,
    /// The terminal status the queue settled.
    pub status: TerminalStatus,
    /// The public error it settled with.
    pub error: VerificationError,
    /// Exact Verifier Card UID.
    pub verifier_card_uid: CardUid,
    /// Exact Verifier Card version.
    pub verifier_version: String,
    /// Exact Card verified.
    pub card_uid: CardUid,
    /// Frozen input.
    pub input: ExecutionInput,
    /// The owner Service: the binding owner, else the accepted caller's
    /// owning root; `None` when neither exists.
    pub owner_card_uid: Option<CardUid>,
    /// Binding; `None` for a submission.
    pub binding_id: Option<BindingId>,
    /// Effective Trigger; `None` for a submission.
    pub trigger: Option<FrozenTarget>,
    /// Caller group, or the group a group execution judges.
    pub group_id: Option<GroupId>,
    /// The accepted deployment environment tag.
    pub deployment_environment: Option<String>,
    /// The subject the execution serves.
    pub sub_principal_id: PrincipalId,
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
    /// The deployment environment tag the execution was accepted with;
    /// `None` when the invocation carried none.
    pub deployment_environment: Option<String>,
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
    /// place replay and conflict are decided. The caller resolves every
    /// distinct direct target first and passes the refused ones in
    /// `refusals`, keyed by (Verifier, subject) UID. The batch reads which of
    /// its execution IDs already exist, decides every submission in order,
    /// then writes every new row in one insert, so its round trips do not
    /// grow with its size. A submission whose execution ID already exists, in the table or
    /// earlier in the batch, is [`SubmissionOutcome::Replayed`], which makes
    /// a retried batch a no-op. A keyed submission takes a
    /// transaction-scoped advisory lock on (subject, actor, key) the first
    /// time the batch meets that key, so concurrent writers serialize in
    /// acceptance order, and finds the execution that subject and actor
    /// already had accepted under the key, earlier in the batch or before
    /// it: the same digest replays it, and a different digest settles this
    /// submission `errored` with the idempotency-conflict code under its own
    /// ID and no key, since the key stays the original's. Otherwise a
    /// refused target settles the submission `errored` with
    /// [`EnqueueRefusal::error`] and keeps its key, so a retry replays the
    /// refusal and a different request under the key conflicts with it. An
    /// accepted submission stores its key, digest, optional Run, inputs,
    /// route overrides, cause, and accepted authority on a `pending` row due
    /// at PostgreSQL's statement time. Conflicting and refused rows never
    /// execute or dispatch. A row whose subject or accepting actor is no
    /// longer active is cancelled before the commit, as its revocation does
    /// (see [`Self::fence_principals`]). The caller commits.
    ///
    /// # Errors
    /// Returns the database error when a lock, read, or insert fails, or an
    /// encode or decode error when an authority, input, or stored identity is
    /// malformed; nothing of the batch survives the caller's rollback.
    #[tracing::instrument(skip(self, conn, submissions, refusals), fields(operation = "verification.executions.accept", submissions = submissions.len()))]
    pub async fn accept_submissions(
        &self,
        conn: &mut TenantConn<'_>,
        submissions: &[Submission],
        refusals: &HashMap<(Uuid, Uuid), EnqueueRefusal>,
    ) -> Result<Vec<SubmissionOutcome>, SqlxError> {
        timed("accept", async {
            let ids: Vec<Uuid> = submissions
                .iter()
                .map(|submission| submission.execution_id.as_uuid())
                .collect();
            let mut decided: HashSet<Uuid> = sqlx::query_scalar(EXISTING_EXECUTIONS_SQL)
                .bind(&ids)
                .fetch_all(&mut **conn.transaction())
                .await?
                .into_iter()
                .collect();
            let keys: Vec<_> = submissions
                .iter()
                .filter(|submission| !decided.contains(&submission.execution_id.as_uuid()))
                .filter_map(holder_key)
                .collect();
            let mut holders = Self::key_holders(conn, &keys).await?;
            let mut rows = Vec::with_capacity(submissions.len());
            let mut outcomes = Vec::with_capacity(submissions.len());
            for submission in submissions {
                if !decided.insert(submission.execution_id.as_uuid()) {
                    outcomes.push(SubmissionOutcome::Replayed(submission.execution_id));
                    continue;
                }
                outcomes.push(Self::decide(submission, refusals, &mut holders, &mut rows));
            }
            self.insert_submissions(conn, &rows).await?;
            Ok(outcomes)
        })
        .await
    }

    /// Decide one new submission and queue its row, if it writes one; see
    /// [`Self::accept_submissions`].
    ///
    /// `holders` maps each (subject, actor, key) of the batch, already locked
    /// by [`Self::key_holders`], to the execution that holds it, including
    /// ones decided earlier in this batch and not yet written. Deciding reads
    /// nothing from the database.
    fn decide<'s>(
        submission: &'s Submission,
        refusals: &HashMap<(Uuid, Uuid), EnqueueRefusal>,
        holders: &mut HashMap<(Uuid, Uuid, &'s str), KeyHolder>,
        rows: &mut Vec<NewSubmission<'s>>,
    ) -> SubmissionOutcome {
        if let (Some(key), Some(holder_key)) = (&submission.key, holder_key(submission)) {
            if let Some(holder) = holders.get(&holder_key) {
                if holder.request_sha256.as_deref() == Some(key.request_sha256.as_slice()) {
                    return SubmissionOutcome::Replayed(holder.execution_id);
                }
                let original = holder.execution_id;
                let conflict = WyrdError::RegistryIdempotencyConflict {
                    message: String::new(),
                    details: Value::Null,
                };
                rows.push(NewSubmission {
                    submission,
                    key: None,
                    error: Some(VerificationError {
                        code: conflict.code().to_owned(),
                        message: format!(
                            "Idempotency-Key {:?} was already used for a different request (execution {original})",
                            key.key
                        ),
                    }),
                });
                return SubmissionOutcome::Conflict { original };
            }
            holders.insert(
                holder_key,
                KeyHolder {
                    execution_id: submission.execution_id,
                    request_sha256: Some(key.request_sha256.to_vec()),
                },
            );
        }
        let target = (
            submission.request.verifier_card_uid.as_uuid(),
            submission.request.card_uid.as_uuid(),
        );
        let refusal = refusals.get(&target).copied();
        rows.push(NewSubmission {
            submission,
            key: submission.key.as_ref(),
            error: refusal.map(EnqueueRefusal::error),
        });
        refusal.map_or(SubmissionOutcome::Accepted, SubmissionOutcome::Refused)
    }

    /// Lock every (subject, actor, key) in `keys` and read the executions
    /// that already hold them, in two statements whatever the batch size.
    ///
    /// The locks serialize concurrent batches sharing a key until the
    /// caller's transaction ends, so the holders read here stay current for
    /// the whole batch. A key held by no execution is absent from the map.
    ///
    /// # Errors
    /// Returns the database error when the lock or lookup fails, or a decode
    /// error when a stored execution ID is malformed or a returned position
    /// names no key.
    async fn key_holders<'s>(
        conn: &mut TenantConn<'_>,
        keys: &[(Uuid, Uuid, &'s str)],
    ) -> Result<HashMap<(Uuid, Uuid, &'s str), KeyHolder>, SqlxError> {
        if keys.is_empty() {
            return Ok(HashMap::new());
        }
        let subjects: Vec<Uuid> = keys.iter().map(|key| key.0).collect();
        let actors: Vec<Uuid> = keys.iter().map(|key| key.1).collect();
        let names: Vec<&str> = keys.iter().map(|key| key.2).collect();
        sqlx::query(LOCK_REQUEST_KEYS_SQL)
            .bind(REQUEST_KEY_LOCK_CLASS)
            .bind(&subjects)
            .bind(&actors)
            .bind(&names)
            .execute(&mut **conn.transaction())
            .await?;
        let existing: Vec<(i64, Uuid, Option<Vec<u8>>)> = sqlx::query_as(KEYED_EXECUTIONS_SQL)
            .bind(&subjects)
            .bind(&actors)
            .bind(&names)
            .fetch_all(&mut **conn.transaction())
            .await?;
        existing
            .into_iter()
            .map(|(position, execution_id, request_sha256)| {
                let key = usize::try_from(position - 1)
                    .ok()
                    .and_then(|index| keys.get(index))
                    .ok_or_else(|| {
                        SqlxError::Decode(format!("key position {position} names no key").into())
                    })?;
                let holder = KeyHolder {
                    execution_id: stored(ExecutionId::new(execution_id))?,
                    request_sha256,
                };
                Ok((*key, holder))
            })
            .collect()
    }

    /// Insert a batch's new rows in one statement; each is `pending` when
    /// its `error` is `None`, otherwise settled `errored` with it.
    ///
    /// # Errors
    /// Returns the database error when the insert fails, or an encode error
    /// when an authority, inputs, or route overrides cannot be serialized.
    async fn insert_submissions(
        &self,
        conn: &mut TenantConn<'_>,
        rows: &[NewSubmission<'_>],
    ) -> Result<(), SqlxError> {
        if rows.is_empty() {
            return Ok(());
        }
        let attributions: Vec<Attribution> = rows
            .iter()
            .map(|row| row.submission.authority.attribution())
            .collect();
        let causes = || rows.iter().map(|row| row.submission.cause.as_ref());
        let requests = || rows.iter().map(|row| &row.submission.request);
        sqlx::query(INSERT_SUBMISSIONS_SQL)
            .bind(
                rows.iter()
                    .map(|row| row.submission.execution_id.as_uuid())
                    .collect::<Vec<_>>(),
            )
            .bind(
                requests()
                    .map(|request| request.verifier_card_uid.as_uuid())
                    .collect::<Vec<_>>(),
            )
            .bind(
                requests()
                    .map(|request| request.card_uid.as_uuid())
                    .collect::<Vec<_>>(),
            )
            .bind(
                requests()
                    .map(|request| request.run_id.as_ref().map(RunId::as_str))
                    .collect::<Vec<_>>(),
            )
            .bind(
                attributions
                    .iter()
                    .map(|attribution| attribution.sub_principal_id.as_uuid())
                    .collect::<Vec<_>>(),
            )
            .bind(
                attributions
                    .iter()
                    .map(|attribution| attribution.act_principal_id.as_uuid())
                    .collect::<Vec<_>>(),
            )
            .bind(
                rows.iter()
                    .map(|row| Json(&row.submission.authority))
                    .collect::<Vec<_>>(),
            )
            .bind(
                causes()
                    .map(|cause| cause.map(|cause| cause.trace_id.as_bytes().as_slice()))
                    .collect::<Vec<_>>(),
            )
            .bind(
                causes()
                    .map(|cause| cause.map(|cause| cause.span_id.as_bytes().as_slice()))
                    .collect::<Vec<_>>(),
            )
            .bind(
                rows.iter()
                    .map(|row| row.key.map(|key| key.key.as_str()))
                    .collect::<Vec<_>>(),
            )
            .bind(
                rows.iter()
                    .map(|row| row.key.map(|key| key.request_sha256.as_slice()))
                    .collect::<Vec<_>>(),
            )
            .bind(
                rows.iter()
                    .map(|row| row.error.as_ref().map(Json))
                    .collect::<Vec<_>>(),
            )
            .bind(
                causes()
                    .map(|cause| cause.map(|cause| cause.sampled))
                    .collect::<Vec<_>>(),
            )
            .bind(
                requests()
                    .map(|request| request.group_id.as_ref().map(GroupId::as_str))
                    .collect::<Vec<_>>(),
            )
            .bind(self.retry.max_attempts)
            .bind(
                requests()
                    .map(|request| request.deployment_environment.as_deref())
                    .collect::<Vec<_>>(),
            )
            .execute(&mut **conn.transaction())
            .await?;
        self.fence_principals(conn, attributions.iter().copied())
            .await
    }

    /// Order admissions just written for `attributions` against principal
    /// revocation, before the caller commits.
    ///
    /// Share-locks every subject and accepting actor through
    /// [`INACTIVE_PRINCIPALS_SQL`], then cancels the unfinished executions of
    /// any no longer active, exactly as their revocation does, so a revoked
    /// principal's delayed admission never commits runnable work. Every
    /// submitted, scheduled, and group admission calls it after its insert.
    ///
    /// # Errors
    /// Returns the database error when the lock, read, or cancellation fails.
    async fn fence_principals(
        &self,
        conn: &mut TenantConn<'_>,
        attributions: impl IntoIterator<Item = Attribution>,
    ) -> Result<(), SqlxError> {
        let ids: Vec<Uuid> = attributions
            .into_iter()
            .flat_map(|attribution| {
                [
                    attribution.sub_principal_id.as_uuid(),
                    attribution.act_principal_id.as_uuid(),
                ]
            })
            .collect();
        let inactive: Vec<Uuid> = sqlx::query_scalar(INACTIVE_PRINCIPALS_SQL)
            .bind(&ids)
            .fetch_all(&mut **conn.transaction())
            .await?;
        for principal in inactive {
            self.cancel_principal(conn, PrincipalId::new(principal))
                .await?;
        }
        Ok(())
    }

    /// Prepare one occurrence of a scheduled binding for acknowledged
    /// admission, writing nothing.
    ///
    /// Resolves the binding in the caller's tenant and freezes its owner,
    /// subject, Verifier, Trigger, and Operators with `authority` under a new
    /// execution ID. It refuses a missing binding, an unavailable or unready
    /// Verifier, or an unavailable subject, and answers an occurrence that
    /// already has an execution with that execution. `cursor` is the
    /// expected and next cursor admission moves, or `None` to leave it.
    ///
    /// # Errors
    /// Returns the database error when a read fails, or a decode error when
    /// stored identities are malformed.
    pub async fn prepare_scheduled(
        &self,
        conn: &mut TenantConn<'_>,
        binding_id: BindingId,
        window: DriftWindow,
        authority: &ExecutionAuthority,
        cursor: Option<(DateTime<Utc>, DateTime<Utc>)>,
    ) -> Result<Prepared<ScheduledAdmission>, SqlxError> {
        let target = match resolve_binding(conn, binding_id).await? {
            Ok(target) => target,
            Err(refusal) => return Ok(Prepared::Refused(refusal)),
        };
        if let Some(existing) = Self::existing_occurrence(conn, binding_id, &window).await? {
            return Ok(Prepared::Existing(existing));
        }
        Ok(Prepared::Candidate(ScheduledAdmission {
            execution_id: ExecutionId::new_v7(),
            binding_id,
            window,
            target,
            authority: authority.clone(),
            cursor,
            max_attempts: self.retry.max_attempts,
        }))
    }

    /// Admit one acknowledged scheduled occurrence on the caller's
    /// transaction and move its expected cursor.
    ///
    /// Inserts a `pending` execution under the prepared ID and frozen
    /// identities, due at PostgreSQL's statement time, then fenced against
    /// revocation by [`Self::fence_principals`]. An occurrence that
    /// already has an execution, including an earlier admission of this
    /// candidate, returns it as [`EnqueueOutcome::AlreadyEnqueued`]; a binding
    /// deleted since preparation is [`EnqueueRefusal::BindingNotFound`]. The
    /// cursor moves only while it still holds the expected value, so a
    /// replay or an edited schedule is never moved again. The caller commits.
    ///
    /// # Errors
    /// Returns the database error when a statement fails, or a decode error
    /// when a stored identity is malformed.
    ///
    /// # Panics
    /// Never in practice: a ready Verifier's Card row exists, so its version
    /// is present.
    #[tracing::instrument(skip_all, fields(operation = "verification.executions.accept_scheduled", binding_id = %admission.binding_id))]
    pub async fn accept_scheduled(
        &self,
        conn: &mut TenantConn<'_>,
        admission: &ScheduledAdmission,
    ) -> Result<EnqueueOutcome, SqlxError> {
        timed("accept_scheduled", async {
            let target = &admission.target;
            let attribution = admission.authority.attribution();
            let inserted: Option<Uuid> = sqlx::query_scalar(INSERT_SCHEDULED_SQL)
                .bind(admission.execution_id.as_uuid())
                .bind(target.verifier_card_uid)
                .bind(
                    target
                        .verifier_version
                        .as_deref()
                        .expect("invariant: a ready Verifier has a Card version"),
                )
                .bind(target.card_uid)
                .bind(target.owner_card_uid)
                .bind(target.binding_id)
                .bind(target.trigger_uid)
                .bind(target.trigger_digest.as_deref())
                .bind(&target.operators)
                .bind(admission.window.start)
                .bind(admission.window.end)
                .bind(attribution.sub_principal_id.as_uuid())
                .bind(attribution.act_principal_id.as_uuid())
                .bind(Json(admission.authority.authority()))
                .bind(admission.max_attempts)
                .fetch_optional(&mut **conn.transaction())
                .await?;
            let outcome = match inserted {
                Some(execution_id) => {
                    self.fence_principals(conn, [attribution]).await?;
                    EnqueueOutcome::Enqueued(stored(ExecutionId::new(execution_id))?)
                }
                None => {
                    match Self::existing_occurrence(conn, admission.binding_id, &admission.window)
                        .await?
                    {
                        Some(existing) => EnqueueOutcome::AlreadyEnqueued(existing),
                        None => {
                            return Ok(EnqueueOutcome::Refused(EnqueueRefusal::BindingNotFound));
                        }
                    }
                }
            };
            if let Some((expected, next_run_at)) = admission.cursor {
                sqlx::query(ADVANCE_EXPECTED_CURSOR_SQL)
                    .bind(admission.binding_id.as_uuid())
                    .bind(next_run_at)
                    .bind(expected)
                    .execute(&mut **conn.transaction())
                    .await?;
            }
            Ok(outcome)
        })
        .await
    }

    /// Enqueue one occurrence of a scheduled binding at once: prepare it
    /// through [`Self::prepare_scheduled`] and admit it through
    /// [`Self::accept_scheduled`] on the caller's transaction, moving no
    /// cursor.
    ///
    /// # Errors
    /// Returns the database error when a read or the insert fails, or a
    /// decode error when stored identities are malformed.
    #[tracing::instrument(skip(self, conn, authority), fields(operation = "verification.executions.enqueue_scheduled", %binding_id))]
    pub async fn enqueue_scheduled(
        &self,
        conn: &mut TenantConn<'_>,
        binding_id: BindingId,
        window: DriftWindow,
        authority: &ExecutionAuthority,
    ) -> Result<EnqueueOutcome, SqlxError> {
        match self
            .prepare_scheduled(conn, binding_id, window, authority, None)
            .await?
        {
            Prepared::Candidate(admission) => self.accept_scheduled(conn, &admission).await,
            Prepared::Existing(existing) => Ok(EnqueueOutcome::AlreadyEnqueued(existing)),
            Prepared::Refused(refusal) => Ok(EnqueueOutcome::Refused(refusal)),
        }
    }

    /// The execution that already holds `binding_id`'s occurrence ending at
    /// `window.end`, if any.
    ///
    /// # Errors
    /// Returns the database error when the read fails, or a decode error when
    /// the stored ID is malformed.
    async fn existing_occurrence(
        conn: &mut TenantConn<'_>,
        binding_id: BindingId,
        window: &DriftWindow,
    ) -> Result<Option<ExecutionId>, SqlxError> {
        let existing: Option<Uuid> = sqlx::query_scalar(EXISTING_OCCURRENCE_SQL)
            .bind(binding_id.as_uuid())
            .bind(window.end)
            .fetch_optional(&mut **conn.transaction())
            .await?;
        existing
            .map(|existing| stored(ExecutionId::new(existing)))
            .transpose()
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
    /// [`DueBinding::owner`]. Bindings in `skip`, whose candidates this
    /// process is already handing off, are passed over. Returns `None` when
    /// nothing else is due.
    ///
    /// # Errors
    /// Returns the database error when a read fails, or a decode error when
    /// stored identities are malformed.
    #[tracing::instrument(
        skip(self, conn, skip),
        fields(operation = "verification.schedule.claim")
    )]
    pub async fn claim_due(
        &self,
        conn: &mut TenantConn<'_>,
        skip: &[BindingId],
    ) -> Result<Option<DueBinding>, SqlxError> {
        timed("schedule_claim", async {
            let due: Option<DueBindingRow> = sqlx::query_as(DUE_BINDING_SQL)
                .bind(SCHEDULE_CLAIM_DELAY.num_milliseconds())
                .bind(skip.iter().map(BindingId::as_uuid).collect::<Vec<_>>())
                .fetch_optional(&mut **conn.transaction())
                .await?;
            let Some(due) = due else {
                return Ok(None);
            };
            let due_at = due.next_run_at;
            let binding_id = stored(BindingId::new(due.binding_id))?;
            let verifier_card_uid = stored(CardUid::from_uuid(due.verifier_card_uid))?;
            let card_uid = stored(CardUid::from_uuid(due.card_uid))?;
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
                        verifier_card_uid,
                        card_uid,
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
                    .and_then(|activity| Some((activity.principal_id?, activity.owner_card_uid)))
                {
                    None => skip(ScheduleSkip::Inactive),
                    Some((owner, owner_root)) => DueDecision::Run {
                        window,
                        owner,
                        owner_root,
                        next_run_at,
                    },
                },
            };
            Ok(Some(DueBinding {
                binding_id,
                due_at,
                verifier_card_uid,
                card_uid,
                decision,
            }))
        })
        .await
    }

    /// Decide one tick claimed by [`Self::claim_due`].
    ///
    /// An occurrence that calls for an execution is prepared through
    /// [`Self::prepare_scheduled`] (which enforces Verifier readiness) under
    /// `authority`, the owner's resolved authority with the SYSTEM actor, and
    /// returned as [`DueSettlement::Candidate`] without any write: its cursor
    /// moves only when [`Self::accept_scheduled`] admits it after Scribe
    /// acknowledges its input record. Every other tick settles here: without
    /// `authority` the occurrence is skipped as
    /// [`ScheduleSkip::AuthorityUnavailable`] rather than run under any other
    /// identity, and inactive, unready, missed, and already-executed
    /// occurrences are skipped without backfill. Each such tick moves the
    /// cursor to the first boundary strictly after the schedule clock in the
    /// same transaction, and an unparseable schedule is disarmed. The caller
    /// commits, releasing the lock before any analysis runs.
    ///
    /// # Errors
    /// Returns the database error when a read or the cursor write fails.
    #[tracing::instrument(skip(self, conn, authority), fields(operation = "verification.schedule.tick", binding_id = %due.binding_id))]
    pub async fn settle_due(
        &self,
        conn: &mut TenantConn<'_>,
        due: DueBinding,
        authority: Option<ExecutionAuthority>,
    ) -> Result<DueSettlement, SqlxError> {
        timed("schedule_due", async {
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
                            .prepare_scheduled(
                                conn,
                                due.binding_id,
                                window,
                                &authority,
                                Some((due.due_at, next_run_at)),
                            )
                            .await?
                        {
                            Prepared::Candidate(admission) => {
                                return Ok(DueSettlement::Candidate(Box::new(admission)));
                            }
                            Prepared::Existing(id) => ScheduleOutcome::AlreadyEnqueued(id),
                            Prepared::Refused(refusal) => {
                                ScheduleOutcome::Skipped(ScheduleSkip::Refused(refusal))
                            }
                        },
                    };
                    (outcome, Some(next_run_at))
                }
            };
            self.advance_cursor(conn, due.binding_id, next_run_at)
                .await?;
            Ok(DueSettlement::Settled(ScheduleTick {
                binding_id: due.binding_id,
                due_at: due.due_at,
                outcome,
                next_run_at,
            }))
        })
        .await
    }

    /// Settle one tick claimed by [`Self::claim_due`] at once: decide it
    /// through [`Self::settle_due`] and admit a candidate through
    /// [`Self::accept_scheduled`] on the caller's transaction.
    ///
    /// # Errors
    /// Returns the database error when a read, the insert, or the cursor
    /// write fails.
    pub async fn schedule_due(
        &self,
        conn: &mut TenantConn<'_>,
        due: DueBinding,
        authority: Option<ExecutionAuthority>,
    ) -> Result<ScheduleTick, SqlxError> {
        let admission = match self.settle_due(conn, due, authority).await? {
            DueSettlement::Settled(tick) => return Ok(tick),
            DueSettlement::Candidate(admission) => admission,
        };
        let outcome = match self.accept_scheduled(conn, &admission).await? {
            EnqueueOutcome::Enqueued(id) => ScheduleOutcome::Enqueued(id),
            EnqueueOutcome::AlreadyEnqueued(id) => ScheduleOutcome::AlreadyEnqueued(id),
            EnqueueOutcome::Refused(refusal) => {
                ScheduleOutcome::Skipped(ScheduleSkip::Refused(refusal))
            }
        };
        Ok(admission.tick(outcome))
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

    /// Record acknowledged grouped source activity of the caller's tenant.
    ///
    /// For every item, each `group_end` binding of its recorded owner with a
    /// source slot for its exact subject and source Verifier gets a pending
    /// row for the item's group due the binding's `after` past PostgreSQL's
    /// statement time; an existing row's due time moves to that instant. A
    /// row locked by a scheduler handing its group off blocks this statement
    /// until that transaction ends. Items no binding sources record nothing.
    /// The caller commits; an empty slice issues no statement.
    ///
    /// # Errors
    /// Returns the database error when the upsert fails.
    #[tracing::instrument(skip(self, conn, items), fields(operation = "verification.group.activity", items = items.len()))]
    pub async fn record_group_activity(
        &self,
        conn: &mut TenantConn<'_>,
        items: &[GroupActivity],
    ) -> Result<(), SqlxError> {
        timed("group_activity", async {
            if items.is_empty() {
                return Ok(());
            }
            let column =
                |field: fn(&GroupActivity) -> Uuid| items.iter().map(field).collect::<Vec<_>>();
            sqlx::query(RECORD_GROUP_ACTIVITY_SQL)
                .bind(column(|item| item.owner_card_uid.as_uuid()))
                .bind(column(|item| item.card_uid.as_uuid()))
                .bind(column(|item| item.verifier_card_uid.as_uuid()))
                .bind(
                    items
                        .iter()
                        .map(|item| item.group_id.as_str())
                        .collect::<Vec<_>>(),
                )
                .execute(&mut **conn.transaction())
                .await?;
            Ok(())
        })
        .await
    }

    /// Lock the earliest expired pending group of the caller's tenant.
    ///
    /// Locks one row whose due time PostgreSQL's statement time has reached
    /// with `FOR UPDATE SKIP LOCKED`, so concurrent schedulers never take the
    /// same group, and reads its binding's frozen targets, owner activity,
    /// and whether the group is more than five minutes past due.
    /// The caller pins a snapshot and prepares it through
    /// [`Self::prepare_group`]; a candidate consumes the row only when
    /// [`Self::accept_grouped`] admits it, and every other decision consumes
    /// it with [`Self::consume_group`] before the caller commits; rolling back
    /// leaves the group pending. Groups in `skip`, whose candidates this
    /// process is already handing off, are passed over. Returns `None` when
    /// nothing else is due.
    ///
    /// # Errors
    /// Returns the database error when a statement fails, or a decode error
    /// when stored identities or settings are malformed.
    #[tracing::instrument(skip(self, conn, skip), fields(operation = "verification.group.claim"))]
    pub async fn claim_due_group(
        &self,
        conn: &mut TenantConn<'_>,
        skip: &[(BindingId, GroupId)],
    ) -> Result<Option<DueGroup>, SqlxError> {
        timed("group_claim", async {
            let due: Option<(Uuid, String, DateTime<Utc>, bool, Uuid)> =
                sqlx::query_as(DUE_GROUP_SQL)
                    .bind(
                        skip.iter()
                            .map(|(binding, _)| binding.as_uuid())
                            .collect::<Vec<_>>(),
                    )
                    .bind(
                        skip.iter()
                            .map(|(_, group)| group.as_str())
                            .collect::<Vec<_>>(),
                    )
                    .fetch_optional(&mut **conn.transaction())
                    .await?;
            let Some((binding_id, group_id, due_at, overdue, generation)) = due else {
                return Ok(None);
            };
            let (owner, Json(settings), verifier, subject): (
                Uuid,
                Json<GroupEndSettings>,
                Uuid,
                Uuid,
            ) = sqlx::query_as(GROUP_BINDING_SQL)
                .bind(binding_id)
                .fetch_one(&mut **conn.transaction())
                .await?;
            let binding_id = stored(BindingId::new(binding_id))?;
            let activity = binding_activity(conn, binding_id, self.inactivity).await?;
            Ok(Some(DueGroup {
                binding_id,
                group_id: stored(GroupId::new(group_id))?,
                due_at,
                owner_card_uid: stored(CardUid::from_uuid(owner))?,
                verifier_card_uid: stored(CardUid::from_uuid(verifier))?,
                card_uid: stored(CardUid::from_uuid(subject))?,
                settings,
                owner: activity.as_ref().and_then(|activity| activity.principal_id),
                active: activity.is_some_and(|activity| activity.active),
                overdue,
                generation,
            }))
        })
        .await
    }

    /// Delete a pending group claimed by [`Self::claim_due_group`] in the
    /// same transaction, so commit consumes exactly the activity it judged.
    ///
    /// # Errors
    /// Returns the database error when the delete fails.
    #[tracing::instrument(skip(self, conn, due), fields(operation = "verification.group.consume", binding_id = %due.binding_id, group_id = %due.group_id))]
    pub async fn consume_group(
        &self,
        conn: &mut TenantConn<'_>,
        due: &DueGroup,
    ) -> Result<(), SqlxError> {
        timed("group_consume", async {
            sqlx::query(CONSUME_GROUP_SQL)
                .bind(due.binding_id.as_uuid())
                .bind(due.group_id.as_str())
                .execute(&mut **conn.transaction())
                .await?;
            Ok(())
        })
        .await
    }

    /// Record the claimed pending group `due` as refused before admission:
    /// one terminal errored group execution carrying `error`, serving the
    /// owner's stored principal `owner` and recorded by the tenant SYSTEM
    /// principal `system`.
    ///
    /// For an owner that is inactive or whose authority was rejected. The
    /// execution asserts no accepted authority, pins no inputs or snapshot,
    /// has no result, and is never claimed, so no Verifier runs and no
    /// Operator is dispatched. The caller consumes the pending row in the
    /// same transaction and commits; rolling back records nothing and leaves
    /// the group pending.
    ///
    /// # Errors
    /// Returns the database error when the insert fails, including when the
    /// binding or its Verifier Card no longer exists, or a decode error when
    /// the returned identity is malformed.
    #[tracing::instrument(skip(self, conn, due, error), fields(operation = "verification.group.refuse", binding_id = %due.binding_id, group_id = %due.group_id, code = %error.code))]
    pub async fn refuse_group(
        &self,
        conn: &mut TenantConn<'_>,
        due: &DueGroup,
        owner: PrincipalId,
        system: PrincipalId,
        error: &VerificationError,
    ) -> Result<ExecutionId, SqlxError> {
        timed("group_refuse", async {
            let execution_id: Uuid = sqlx::query_scalar(INSERT_GROUP_ERROR_SQL)
                .bind(ExecutionId::new_v7().as_uuid())
                .bind(due.binding_id.as_uuid())
                .bind(due.group_id.as_str())
                .bind(owner.as_uuid())
                .bind(system.as_uuid())
                .bind(Json(error))
                .bind(self.retry.max_attempts)
                .fetch_one(&mut **conn.transaction())
                .await?;
            stored(ExecutionId::new(execution_id))
        })
        .await
    }

    /// Prepare one group execution of `binding_id` judging `pinned` for
    /// acknowledged admission, writing nothing.
    ///
    /// Verifier readiness and subject availability are enforced as for a
    /// scheduled occurrence. The candidate serves the binding owner under
    /// `authority`, freezes the binding's Trigger and Operators and the
    /// snapshot's digest under a new execution ID, and consumes the pending
    /// row at `generation` when admitted, or none when `None`. A snapshot
    /// whose digest the binding and group already hold, whatever that
    /// execution's status, is [`Prepared::Existing`]; distinct snapshots may
    /// be outstanding together.
    ///
    /// # Errors
    /// Returns the database error when a read fails, or a decode error when
    /// a stored identity is malformed.
    pub async fn prepare_group(
        &self,
        conn: &mut TenantConn<'_>,
        binding_id: BindingId,
        pinned: &PinnedGroup,
        authority: &ExecutionAuthority,
        generation: Option<Uuid>,
    ) -> Result<Prepared<GroupedAdmission>, SqlxError> {
        let target = match resolve_binding(conn, binding_id).await? {
            Ok(target) => target,
            Err(refusal) => return Ok(Prepared::Refused(refusal)),
        };
        if let Some(existing) =
            Self::existing_snapshot(conn, binding_id, &pinned.group_id, &pinned.digest).await?
        {
            return Ok(Prepared::Existing(existing));
        }
        Ok(Prepared::Candidate(GroupedAdmission {
            execution_id: ExecutionId::new_v7(),
            binding_id,
            group_id: pinned.group_id.clone(),
            digest: pinned.digest,
            target,
            authority: authority.clone(),
            generation,
            max_attempts: self.retry.max_attempts,
        }))
    }

    /// Admit one acknowledged group execution on the caller's transaction
    /// and consume the pending activity it was selected from.
    ///
    /// Inserts a `pending` execution under the prepared ID and frozen
    /// identities, due at PostgreSQL's statement time, then fenced against
    /// revocation by [`Self::fence_principals`]. A snapshot the binding
    /// and group already hold, including an earlier admission of this
    /// candidate, returns that execution as
    /// [`EnqueueOutcome::AlreadyEnqueued`]; a binding deleted since
    /// preparation is [`EnqueueRefusal::BindingNotFound`]. The pending row is
    /// deleted only while it still holds the candidate's generation, so
    /// newer activity, or a row recreated since, stays pending. The caller
    /// commits.
    ///
    /// # Errors
    /// Returns the database error when a statement fails, or a decode error
    /// when a stored identity is malformed.
    ///
    /// # Panics
    /// Never in practice: a ready Verifier's Card row exists, so its version
    /// is present.
    #[tracing::instrument(skip_all, fields(operation = "verification.group.accept", binding_id = %admission.binding_id, group_id = %admission.group_id))]
    pub async fn accept_grouped(
        &self,
        conn: &mut TenantConn<'_>,
        admission: &GroupedAdmission,
    ) -> Result<EnqueueOutcome, SqlxError> {
        timed("accept_grouped", async {
            let target = &admission.target;
            let attribution = admission.authority.attribution();
            let inserted: Option<Uuid> = sqlx::query_scalar(INSERT_GROUP_SQL)
                .bind(admission.execution_id.as_uuid())
                .bind(target.verifier_card_uid)
                .bind(
                    target
                        .verifier_version
                        .as_deref()
                        .expect("invariant: a ready Verifier has a Card version"),
                )
                .bind(target.card_uid)
                .bind(target.owner_card_uid)
                .bind(target.binding_id)
                .bind(target.trigger_uid)
                .bind(target.trigger_digest.as_deref())
                .bind(&target.operators)
                .bind(admission.group_id.as_str())
                .bind(admission.digest.as_slice())
                .bind(attribution.sub_principal_id.as_uuid())
                .bind(attribution.act_principal_id.as_uuid())
                .bind(Json(admission.authority.authority()))
                .bind(admission.max_attempts)
                .fetch_optional(&mut **conn.transaction())
                .await?;
            let outcome = match inserted {
                Some(execution_id) => {
                    self.fence_principals(conn, [attribution]).await?;
                    EnqueueOutcome::Enqueued(stored(ExecutionId::new(execution_id))?)
                }
                None => match Self::existing_snapshot(
                    conn,
                    admission.binding_id,
                    &admission.group_id,
                    &admission.digest,
                )
                .await?
                {
                    Some(existing) => EnqueueOutcome::AlreadyEnqueued(existing),
                    None => return Ok(EnqueueOutcome::Refused(EnqueueRefusal::BindingNotFound)),
                },
            };
            if let Some(generation) = admission.generation {
                sqlx::query(CONSUME_GROUP_GENERATION_SQL)
                    .bind(admission.binding_id.as_uuid())
                    .bind(admission.group_id.as_str())
                    .bind(generation)
                    .execute(&mut **conn.transaction())
                    .await?;
            }
            Ok(outcome)
        })
        .await
    }

    /// Enqueue one group execution of `binding_id` judging `pinned` at once:
    /// prepare it through [`Self::prepare_group`] and admit it through
    /// [`Self::accept_grouped`] on the caller's transaction, consuming no
    /// pending row.
    ///
    /// # Errors
    /// Returns the database error when a read or the insert fails, or a
    /// decode error when a stored identity is malformed.
    #[tracing::instrument(skip(self, conn, pinned, authority), fields(operation = "verification.group.enqueue", %binding_id, group_id = %pinned.group_id))]
    pub async fn enqueue_group(
        &self,
        conn: &mut TenantConn<'_>,
        binding_id: BindingId,
        pinned: &PinnedGroup,
        authority: &ExecutionAuthority,
    ) -> Result<EnqueueOutcome, SqlxError> {
        match self
            .prepare_group(conn, binding_id, pinned, authority, None)
            .await?
        {
            Prepared::Candidate(admission) => self.accept_grouped(conn, &admission).await,
            Prepared::Existing(existing) => Ok(EnqueueOutcome::AlreadyEnqueued(existing)),
            Prepared::Refused(refusal) => Ok(EnqueueOutcome::Refused(refusal)),
        }
    }

    /// The group execution of `binding_id` and `group_id` that already holds
    /// snapshot `digest`, if any.
    ///
    /// # Errors
    /// Returns the database error when the read fails, or a decode error when
    /// the stored ID is malformed.
    async fn existing_snapshot(
        conn: &mut TenantConn<'_>,
        binding_id: BindingId,
        group_id: &GroupId,
        digest: &[u8; 32],
    ) -> Result<Option<ExecutionId>, SqlxError> {
        let existing: Option<Uuid> = sqlx::query_scalar(EXISTING_SNAPSHOT_SQL)
            .bind(binding_id.as_uuid())
            .bind(group_id.as_str())
            .bind(digest.as_slice())
            .fetch_optional(&mut **conn.transaction())
            .await?;
        existing
            .map(|existing| stored(ExecutionId::new(existing)))
            .transpose()
    }

    /// Claim up to `limit` runnable executions of the caller's tenant, each
    /// under its own fresh lease, in one statement.
    ///
    /// The statement locks the oldest `pending` or `retrying` executions due at
    /// PostgreSQL's statement time, or `running` executions whose lease the
    /// database considers expired, with `SKIP LOCKED`; marks each `running`
    /// with its own lease token expiring `lease_for` after that same database
    /// instant; counts the attempt; and fixes the execution deadline
    /// `deadline_for` after its first claim, which a reclaim keeps. An
    /// expired lease with no attempt left is claimed without counting one,
    /// as [`ClaimedExecution::exhausted`], so a crash loop cannot exceed the
    /// budget. Returns
    /// nothing when nothing is runnable, and never more than `limit`; the
    /// order is unspecified. The
    /// caller commits before executing. The statement opens no span of its
    /// own: the runner records it on each attempt's `claim` span once the
    /// claimed row names the attempt's cause.
    ///
    /// # Errors
    /// Returns the database error when the statement fails, or a decode error
    /// when a claimed row's identities or frozen input are malformed.
    pub async fn claim(
        &self,
        conn: &mut TenantConn<'_>,
        lease_for: Duration,
        deadline_for: Duration,
        limit: u32,
    ) -> Result<Vec<ClaimedExecution>, SqlxError> {
        timed("claim", async {
            let rows: Vec<ClaimedRow> = sqlx::query_as(CLAIM_SQL)
                .bind(lease_for.num_milliseconds())
                .bind(i64::from(limit))
                .bind(deadline_for.num_milliseconds())
                .bind(WORKER_FORMAT)
                .fetch_all(&mut **conn.transaction())
                .await?;
            rows.into_iter().map(ClaimedRow::into_claimed).collect()
        })
        .await
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
        timed("renew", async {
            let tokens: Vec<Uuid> = tokens.iter().map(|token| token.0).collect();
            let lease = lease_for.num_milliseconds();
            let held: Vec<Uuid> = sqlx::query_scalar(RENEW_LEASES_SQL)
                .bind(tokens)
                .bind(lease)
                .bind(lease - lease / 3)
                .fetch_all(&mut **conn.transaction())
                .await?;
            Ok(held.into_iter().map(LeaseToken).collect())
        })
        .await
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
    /// A group execution first locks its binding row for the rest of the
    /// caller's transaction, so group completions of one binding settle one
    /// at a time, then stores its verdict and compares it with the latest
    /// other settled group verdict of the same binding and group in
    /// `(settled_at, execution_id)` order: the first result is the baseline
    /// and dispatches nothing, an unchanged verdict dispatches nothing, and a
    /// changed one — including to or from `inconclusive` — dispatches once per
    /// frozen `on_verdict_change` Operator with the group, previous result,
    /// and previous verdict in its context. A replay of an already applied
    /// group completion compares nothing and dispatches nothing new.
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
        timed("complete", async {
            let verdict_text: &'static str = verdict.into();
            sqlx::query(LOCK_GROUP_BINDING_SQL)
                .bind(lease.execution_id.as_uuid())
                .fetch_optional(&mut **conn.transaction())
                .await?;
            let settled: Option<SettledRow> = sqlx::query_as(COMPLETE_SQL)
                .bind(lease.execution_id.as_uuid())
                .bind(lease.token.0)
                .bind(result_id.as_uuid())
                .bind(verdict_text)
                .fetch_optional(&mut **conn.transaction())
                .await?;
            let Some((binding_id, Json(operators), grouped, replayed)) = settled else {
                return Ok(Settlement::StaleLease);
            };
            let previous = if grouped {
                if replayed {
                    return Ok(Settlement::Applied);
                }
                let previous: Option<(Uuid, String)> = sqlx::query_as(PREVIOUS_GROUP_SQL)
                    .bind(lease.execution_id.as_uuid())
                    .fetch_optional(&mut **conn.transaction())
                    .await?;
                match previous {
                    Some(previous) if previous.1 != verdict_text => Some(previous),
                    _ => return Ok(Settlement::Applied),
                }
            } else if verdict == VerificationVerdict::Failed && binding_id.is_some() {
                None
            } else {
                return Ok(Settlement::Applied);
            };
            insert_dispatches(
                conn,
                lease.execution_id,
                &operators,
                summary,
                drift,
                verdict_text,
                previous,
            )
            .await?;
            Ok(Settlement::Applied)
        })
        .await
    }

    /// Settle many claimed executions of the caller's tenant `completed` in
    /// one statement, with the same effects as calling
    /// [`complete`](Self::complete) for each in turn.
    ///
    /// Non-group completions apply through [`COMPLETE_MANY_SQL`], and a
    /// `failed` binding-created one inserts its dispatches exactly as
    /// `complete` does. A completion the statement did not apply — a group
    /// execution, which needs its binding lock and verdict comparison, or a
    /// lost lease or different result — is settled by `complete` itself in
    /// the same transaction, which reports [`Settlement::StaleLease`] for a
    /// lost lease. The returned settlements align with `completions`.
    ///
    /// # Errors
    /// Returns the database error when a statement fails, or a decode error
    /// when the frozen Operators are malformed; the caller's transaction is
    /// then unusable and nothing in it should commit.
    #[tracing::instrument(skip_all, fields(operation = "verification.executions.complete_many", completions = completions.len()))]
    pub async fn complete_many(
        &self,
        conn: &mut TenantConn<'_>,
        completions: &[Completion],
    ) -> Result<Vec<Settlement>, SqlxError> {
        let applied: Vec<(Uuid, Option<Uuid>, Json<Vec<FrozenTarget>>)> =
            timed("complete_many", async {
                let (executions, (tokens, results)): (Vec<Uuid>, (Vec<Uuid>, Vec<Uuid>)) =
                    completions
                        .iter()
                        .map(|c| {
                            (
                                c.lease.execution_id.as_uuid(),
                                (c.lease.token.0, c.result_id.as_uuid()),
                            )
                        })
                        .unzip();
                sqlx::query_as(COMPLETE_MANY_SQL)
                    .bind(executions)
                    .bind(tokens)
                    .bind(results)
                    .fetch_all(&mut **conn.transaction())
                    .await
            })
            .await?;
        let mut applied: HashMap<Uuid, (Option<Uuid>, Vec<FrozenTarget>)> = applied
            .into_iter()
            .map(|(execution, binding, Json(operators))| (execution, (binding, operators)))
            .collect();
        let mut settlements = Vec::with_capacity(completions.len());
        for c in completions {
            let settlement = match applied.remove(&c.lease.execution_id.as_uuid()) {
                Some((binding, operators)) => {
                    if c.verdict == VerificationVerdict::Failed && binding.is_some() {
                        insert_dispatches(
                            conn,
                            c.lease.execution_id,
                            &operators,
                            &c.summary,
                            c.drift,
                            c.verdict.into(),
                            None,
                        )
                        .await?;
                    }
                    Settlement::Applied
                }
                None => {
                    self.complete(conn, c.lease, c.result_id, c.verdict, &c.summary, c.drift)
                        .await?
                }
            };
            settlements.push(settlement);
        }
        Ok(settlements)
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
        timed("retry", async {
            let budget: Option<(i32, i32)> = sqlx::query_as(LEASED_ATTEMPTS_SQL)
                .bind(lease.execution_id.as_uuid())
                .bind(lease.token.0)
                .fetch_optional(&mut **conn.transaction())
                .await?;
            let Some((attempts, max_attempts)) = budget else {
                return Ok(RetryOutcome::StaleLease);
            };
            if attempts >= max_attempts {
                self.terminate(
                    conn,
                    lease,
                    TerminalStatus::Errored,
                    error,
                    TerminalDelivery::Owed,
                )
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
        })
        .await
    }

    /// Settle a claimed execution in a terminal status with no verdict.
    ///
    /// `cancelled`, `timed_out`, and `errored` executions record `error`,
    /// point at no result, and never dispatch an Operator. A decision whose
    /// `delivery` is [`TerminalDelivery::Owed`] stays due for
    /// [`Self::claim_deliveries`]; an acknowledged one owes nothing more.
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
        delivery: TerminalDelivery,
    ) -> Result<Settlement, SqlxError> {
        timed("terminate", async {
            let result = sqlx::query(TERMINATE_SQL)
                .bind(lease.execution_id.as_uuid())
                .bind(lease.token.0)
                .bind(status.as_str())
                .bind(Json(error))
                .bind(delivery == TerminalDelivery::Owed)
                .execute(&mut **conn.transaction())
                .await?;
            Ok(settlement(result.rows_affected()))
        })
        .await
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
        timed("release", async {
            let result = sqlx::query(RELEASE_SQL)
                .bind(lease.execution_id.as_uuid())
                .bind(lease.token.0)
                .bind(delay.num_milliseconds())
                .execute(&mut **conn.transaction())
                .await?;
            Ok(settlement(result.rows_affected()))
        })
        .await
    }

    /// Cancel `execution` if it is still unfinished.
    ///
    /// The execution settles `cancelled` with no result and keeps its
    /// recorded state. A worker holding its lease is fenced: its renewal and
    /// settlement report [`Settlement::StaleLease`], and a lease confirmation
    /// before each state revision stops it. Cancelling a running execution
    /// notifies `wyrd_verification_cancelled` on commit, so the holder
    /// renews and stops at once. The cancellation is owed to the
    /// retained state until a [delivery claim](Self::claim_deliveries)
    /// settles it. Cancelling a settled or unknown execution writes nothing,
    /// so repeating a cancel is harmless.
    /// Returns whether this call cancelled it. The caller authorizes first
    /// and commits.
    ///
    /// # Errors
    /// Returns the database error when the update fails.
    #[tracing::instrument(skip(self, conn), fields(operation = "verification.executions.cancel", execution_id = %execution))]
    pub async fn cancel(
        &self,
        conn: &mut TenantConn<'_>,
        execution: ExecutionId,
    ) -> Result<bool, SqlxError> {
        timed("cancel", async {
            let error = VerificationError {
                code: "cancelled".to_owned(),
                message: "the execution was cancelled".to_owned(),
            };
            let result = sqlx::query(CANCEL_SQL)
                .bind(execution.as_uuid())
                .bind(Json(&error))
                .execute(&mut **conn.transaction())
                .await?;
            Ok(result.rows_affected() > 0)
        })
        .await
    }

    /// Cancel every unfinished execution `principal` is the subject or the
    /// accepting actor of, because its authority was revoked.
    ///
    /// Runs on the revocation's own connection so the suspension and the
    /// cancellations commit together; each settles `cancelled` with the
    /// `principal_revoked` code, fences its worker, and owes its delivery as
    /// [`Self::cancel`] does. Returns how many executions were cancelled.
    ///
    /// # Errors
    /// Returns the database error when the update fails.
    #[tracing::instrument(skip(self, conn), fields(operation = "verification.executions.cancel_principal", principal_id = %principal))]
    pub async fn cancel_principal(
        &self,
        conn: &mut TenantConn<'_>,
        principal: PrincipalId,
    ) -> Result<u64, SqlxError> {
        timed("cancel_principal", async {
            let error = VerificationError {
                code: "principal_revoked".to_owned(),
                message: "the principal the execution acts for was revoked".to_owned(),
            };
            let result = sqlx::query(CANCEL_PRINCIPAL_SQL)
                .bind(principal.as_uuid())
                .bind(Json(&error))
                .execute(&mut **conn.transaction())
                .await?;
            Ok(result.rows_affected())
        })
        .await
    }

    /// Claim up to `limit` of the tenant's due terminal deliveries, each for
    /// `lease_for` on the database clock.
    ///
    /// A terminal execution without a verdict owes its decision to its
    /// retained state from the moment its status commits until Scribe
    /// acknowledges it there, unless the attempt that settled it already
    /// waited for that acknowledgement. A claim only moves the delivery one
    /// lease ahead, so a claimant that dies leaves it due again. An
    /// execution of a newer worker format is left for a worker that
    /// understands its state. Claims compute nothing, evaluate no
    /// permission, and write no audit.
    ///
    /// # Errors
    /// Returns the database error when the statement fails, or a decode error
    /// when a claimed row's identity, status, input, or error is malformed.
    pub async fn claim_deliveries(
        &self,
        conn: &mut TenantConn<'_>,
        lease_for: Duration,
        limit: u32,
    ) -> Result<Vec<PendingDelivery>, SqlxError> {
        timed("claim_deliveries", async {
            let rows: Vec<DeliveryRow> = sqlx::query_as(CLAIM_DELIVERIES_SQL)
                .bind(lease_for.num_milliseconds())
                .bind(i64::from(limit))
                .bind(WORKER_FORMAT)
                .fetch_all(&mut **conn.transaction())
                .await?;
            rows.into_iter().map(DeliveryRow::into_pending).collect()
        })
        .await
    }

    /// Retire `pending`'s delivery once its decision is acknowledged in the
    /// retained state. A claim that no longer holds the delivery is
    /// [`Settlement::StaleLease`] and changes nothing.
    ///
    /// # Errors
    /// Returns the database error when the update fails.
    #[tracing::instrument(skip(self, conn, pending), fields(operation = "verification.executions.delivered", execution_id = %pending.execution_id))]
    pub async fn delivered(
        &self,
        conn: &mut TenantConn<'_>,
        pending: &PendingDelivery,
    ) -> Result<Settlement, SqlxError> {
        timed("delivered", self.settle_delivery(conn, pending, None)).await
    }

    /// Make `pending`'s delivery due again `delay` after the database's
    /// statement time, because its decision is not acknowledged yet. A claim
    /// that no longer holds the delivery is [`Settlement::StaleLease`].
    ///
    /// # Errors
    /// Returns the database error when the update fails.
    #[tracing::instrument(skip(self, conn, pending), fields(operation = "verification.executions.redeliver", execution_id = %pending.execution_id))]
    pub async fn redeliver(
        &self,
        conn: &mut TenantConn<'_>,
        pending: &PendingDelivery,
        delay: Duration,
    ) -> Result<Settlement, SqlxError> {
        timed(
            "redeliver",
            self.settle_delivery(conn, pending, Some(delay)),
        )
        .await
    }

    /// Apply [`SETTLE_DELIVERY_SQL`] to `pending`'s claim in the caller's
    /// transaction: retire the delivery when `again` is `None`, or make it
    /// due `again` after the database's statement time. Only the claim whose
    /// `claimed_until` still matches the row's due time changes it; any
    /// other claim is [`Settlement::StaleLease`] and changes nothing. The
    /// caller commits.
    ///
    /// # Errors
    /// Returns the database error when the update fails.
    async fn settle_delivery(
        &self,
        conn: &mut TenantConn<'_>,
        pending: &PendingDelivery,
        again: Option<Duration>,
    ) -> Result<Settlement, SqlxError> {
        let result = sqlx::query(SETTLE_DELIVERY_SQL)
            .bind(pending.execution_id.as_uuid())
            .bind(pending.claimed_until)
            .bind(again.map(|delay| delay.num_milliseconds()))
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
        timed("execution_status", async {
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
                    group_id: row
                        .group_id
                        .map(|id| stored(GroupId::new(id)))
                        .transpose()?,
                    owner_card_uid: row
                        .owner_card_uid
                        .map(|uid| stored(CardUid::from_uuid(uid)))
                        .transpose()?,
                    group: None,
                    pull_request: row.pull_request.map(|Json(pull_request)| pull_request),
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
                deployment_environment: row.deployment_environment,
            }))
        })
        .await
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
        timed("binding_status", async {
            let Some(activity) = binding_activity(conn, binding_id, self.inactivity).await? else {
                return Ok(None);
            };
            let (subject, verifier, readiness, last_execution, group_end): (
                Uuid,
                Uuid,
                String,
                Option<Uuid>,
                Option<Json<GroupEndSettings>>,
            ) = sqlx::query_as(BINDING_TARGETS_SQL)
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
                group_end: group_end.map(|Json(settings)| settings),
            }))
        })
        .await
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
        timed("runnable_scan", async {
            let rows: Vec<Uuid> = sqlx::query_scalar(RUNNABLE_TENANTS_SQL)
                .fetch_all(operator.pool())
                .await?;
            rows.into_iter()
                .map(|tenant| stored(DataTenantId::new(tenant)))
                .collect()
        })
        .await
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
        timed("due_scan", async {
            let rows: Vec<Uuid> = sqlx::query_scalar(DUE_TENANTS_SQL)
                .bind(limit)
                .bind(SCHEDULE_CLAIM_DELAY.num_milliseconds())
                .fetch_all(operator.pool())
                .await?;
            rows.into_iter()
                .map(|tenant| stored(DataTenantId::new(tenant)))
                .collect()
        })
        .await
    }

    /// List up to `limit` tenants that have an expired pending group at
    /// database time.
    ///
    /// Most overdue tenant first. Read-only; claiming happens per tenant
    /// through [`Self::claim_due_group`].
    ///
    /// # Errors
    /// Returns the database error when the read fails, or a decode error when
    /// a stored tenant key is malformed.
    #[tracing::instrument(
        skip(self, operator),
        fields(operation = "verification.group.due_tenants")
    )]
    // tenant-isolation: cross-tenant OperatorPool
    pub async fn tenants_with_due_groups(
        &self,
        operator: &OperatorPool,
        limit: i64,
    ) -> Result<Vec<DataTenantId>, SqlxError> {
        timed("due_group_scan", async {
            let rows: Vec<Uuid> = sqlx::query_scalar(DUE_GROUP_TENANTS_SQL)
                .bind(limit)
                .fetch_all(operator.pool())
                .await?;
            rows.into_iter()
                .map(|tenant| stored(DataTenantId::new(tenant)))
                .collect()
        })
        .await
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
        timed("queue_depth", async {
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
        })
        .await
    }
}

/// The (subject, actor, key) a keyed submission is decided under, or `None`
/// for a submission without an `Idempotency-Key`.
fn holder_key(submission: &Submission) -> Option<(Uuid, Uuid, &str)> {
    let key = submission.key.as_ref()?;
    let attribution = submission.authority.attribution();
    Some((
        attribution.sub_principal_id.as_uuid(),
        attribution.act_principal_id.as_uuid(),
        key.key.as_str(),
    ))
}

/// One row a submission batch writes; see
/// [`VerifierExecutionQueue::accept_submissions`].
struct NewSubmission<'s> {
    /// The submission the row records.
    submission: &'s Submission,
    /// The key the row stores; a conflicting row stores none.
    key: Option<&'s RequestKey>,
    /// The error the row settles with; `None` leaves it `pending`.
    error: Option<VerificationError>,
}

/// The execution holding one (subject, actor, key) within a submission batch.
#[derive(Clone)]
struct KeyHolder {
    /// The execution the key belongs to.
    execution_id: ExecutionId,
    /// The request digest stored with the key.
    request_sha256: Option<Vec<u8>>,
}

/// A resolved target row from [`RESOLVE_BINDING_SQL`].
#[derive(Debug, Clone, PartialEq, sqlx::FromRow)]
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

/// Resolve a binding's frozen targets for background work, or the refusal
/// that keeps it from becoming runnable.
///
/// # Errors
/// Returns the database error when the read fails, or a decode error when the
/// stored readiness is unknown.
async fn resolve_binding(
    conn: &mut TenantConn<'_>,
    binding_id: BindingId,
) -> Result<Result<ResolvedTarget, EnqueueRefusal>, SqlxError> {
    let resolved: Option<ResolvedTarget> = sqlx::query_as(RESOLVE_BINDING_SQL)
        .bind(binding_id.as_uuid())
        .fetch_optional(&mut **conn.transaction())
        .await?;
    let Some(resolved) = resolved else {
        return Ok(Err(EnqueueRefusal::BindingNotFound));
    };
    Ok(match resolved.refusal()? {
        Some(refusal) => Err(refusal),
        None => Ok(resolved),
    })
}

/// A claimed execution row from [`CLAIM_SQL`].
#[derive(sqlx::FromRow)]
struct ClaimedRow {
    /// Execution identity.
    execution_id: Uuid,
    /// The lease token this claim drew.
    lease_token: Uuid,
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
    /// Whether this claim took an expired final attempt.
    exhausted: bool,
    /// Lease expiry.
    lease_expires_at: DateTime<Utc>,
    /// The execution deadline the first claim fixed.
    deadline_at: DateTime<Utc>,
    /// Submitting request's trace id, 16 bytes.
    cause_trace_id: Option<Vec<u8>>,
    /// Submitting request's span id, 8 bytes.
    cause_span_id: Option<Vec<u8>>,
    /// Whether the submitting request's caller sampled it.
    cause_sampled: Option<bool>,
    /// Caller group the execution belongs to.
    group_id: Option<String>,
    /// Accepted deployment environment tag.
    deployment_environment: Option<String>,
    /// Admitted pull request pins of a `pull_request` execution.
    pull_request: Option<Json<PullRequestActivation>>,
    /// Milliseconds the execution was claimable before this claim.
    queue_wait_ms: i64,
    /// Milliseconds since the execution was admitted.
    age_ms: i64,
    /// Milliseconds left before the execution deadline.
    remaining_ms: i64,
    /// The Verifier's ready fitted baselines by requirement; `None` when none.
    fitted_baselines: Option<Json<BTreeMap<TaskId, Value>>>,
}

/// The single frozen input of an execution of `origin` with a scheduled
/// window `start`..`end`, a submitted `run_id`, or admitted `pull_request`
/// pins.
///
/// # Errors
/// Returns [`SqlxError::Decode`] when the columns violate their closed shape.
fn frozen_input(
    origin: ExecutionOrigin,
    start: Option<DateTime<Utc>>,
    end: Option<DateTime<Utc>>,
    run_id: Option<String>,
    pull_request: Option<Json<PullRequestActivation>>,
) -> Result<ExecutionInput, SqlxError> {
    if let (ExecutionOrigin::PullRequest, Some(Json(pull_request))) = (origin, pull_request) {
        return Ok(ExecutionInput::PullRequest {
            pull_request: Box::new(pull_request),
        });
    }
    match (origin, start, end) {
        (ExecutionOrigin::Schedule, Some(start), Some(end)) => Ok(ExecutionInput::Scheduled {
            window: DriftWindow { start, end },
        }),
        (ExecutionOrigin::Group, None, None) => Ok(ExecutionInput::Grouped),
        (ExecutionOrigin::Manual, None, None) => Ok(ExecutionInput::Submitted {
            run_id: run_id.map(RunId::from_string),
        }),
        _ => Err(SqlxError::Decode(
            "verifier execution has no single frozen input".into(),
        )),
    }
}

/// The effective Trigger a stored Card UID or inline digest names.
///
/// # Errors
/// Returns [`SqlxError::Decode`] when the UID is not a Card UID.
fn frozen_trigger(
    uid: Option<Uuid>,
    digest: Option<String>,
) -> Result<Option<FrozenTarget>, SqlxError> {
    Ok(match (uid, digest) {
        (Some(uid), _) => Some(FrozenTarget::Uid(stored(CardUid::from_uuid(uid))?)),
        (None, Some(digest)) => Some(FrozenTarget::Digest(digest)),
        (None, None) => None,
    })
}

/// A claimed terminal delivery row from [`CLAIM_DELIVERIES_SQL`].
#[derive(sqlx::FromRow)]
struct DeliveryRow {
    /// Execution identity.
    execution_id: Uuid,
    /// The instant the claim moved the delivery to.
    delivery_due_at: DateTime<Utc>,
    /// Stored terminal status.
    status: String,
    /// Settled error.
    error: Option<Json<VerificationError>>,
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
    /// Caller or judged group.
    group_id: Option<String>,
    /// Accepted deployment environment tag.
    deployment_environment: Option<String>,
    /// Subject the execution serves.
    sub_principal_id: Uuid,
    /// Accepted effective authority; `None` for a refused group or pull
    /// request binding.
    authority: Option<Json<AcceptedAuthority>>,
    /// Admitted pull request pins of a `pull_request` execution.
    pull_request: Option<Json<PullRequestActivation>>,
}

impl DeliveryRow {
    /// Convert raw columns into a typed delivery.
    ///
    /// # Errors
    /// Returns [`SqlxError::Decode`] when an identity is malformed, the
    /// status carries a verdict, the input violates its shape, or no error
    /// was settled.
    fn into_pending(self) -> Result<PendingDelivery, SqlxError> {
        let Some(Json(error)) = self.error else {
            return Err(SqlxError::Decode(
                "an owed terminal delivery has no settled error".into(),
            ));
        };
        let origin = ExecutionOrigin::parse(&self.origin)?;
        let owner_card_uid = match self.owner_card_uid {
            Some(uid) => Some(stored(CardUid::from_uuid(uid))?),
            None => self
                .authority
                .and_then(|Json(authority)| authority.owning_root().cloned()),
        };
        Ok(PendingDelivery {
            execution_id: stored(ExecutionId::new(self.execution_id))?,
            claimed_until: self.delivery_due_at,
            status: TerminalStatus::parse(&self.status)?,
            error,
            verifier_card_uid: stored(CardUid::from_uuid(self.verifier_card_uid))?,
            verifier_version: self.verifier_version,
            card_uid: stored(CardUid::from_uuid(self.card_uid))?,
            input: frozen_input(
                origin,
                self.window_start,
                self.window_end,
                self.run_id,
                self.pull_request,
            )?,
            owner_card_uid,
            binding_id: self
                .binding_id
                .map(|id| stored(BindingId::new(id)))
                .transpose()?,
            trigger: frozen_trigger(self.trigger_uid, self.trigger_digest)?,
            group_id: self
                .group_id
                .map(|id| stored(GroupId::new(id)))
                .transpose()?,
            deployment_environment: self.deployment_environment,
            sub_principal_id: PrincipalId::new(self.sub_principal_id),
        })
    }
}

impl ClaimedRow {
    /// Convert raw columns into a typed claim holding the row's lease token.
    ///
    /// # Errors
    /// Returns [`SqlxError::Decode`] when an identity is not `UUIDv7`, the
    /// origin or input columns violate their closed shapes, or a duration is
    /// negative.
    fn into_claimed(self) -> Result<ClaimedExecution, SqlxError> {
        let origin = ExecutionOrigin::parse(&self.origin)?;
        let input = frozen_input(
            origin,
            self.window_start,
            self.window_end,
            self.run_id,
            self.pull_request,
        )?;
        let trigger = frozen_trigger(self.trigger_uid, self.trigger_digest)?;
        Ok(ClaimedExecution {
            lease: ExecutionLease {
                execution_id: stored(ExecutionId::new(self.execution_id))?,
                token: LeaseToken(self.lease_token),
            },
            lease_expires_at: self.lease_expires_at,
            deadline_at: self.deadline_at,
            attempt: self.attempts,
            max_attempts: self.max_attempts,
            exhausted: self.exhausted,
            verifier_card_uid: stored(CardUid::from_uuid(self.verifier_card_uid))?,
            verifier_version: self.verifier_version,
            card_uid: stored(CardUid::from_uuid(self.card_uid))?,
            origin,
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
            group_id: self
                .group_id
                .map(|id| stored(GroupId::new(id)))
                .transpose()?,
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
            remaining: std::time::Duration::from_millis(stored(u64::try_from(self.remaining_ms))?),
            fitted_baselines: self
                .fitted_baselines
                .map(|Json(fitted)| fitted)
                .unwrap_or_default(),
            deployment_environment: self.deployment_environment,
        })
    }
}

/// A locked due binding from [`DUE_BINDING_SQL`], carrying the database
/// instant that anchors the whole tick.
#[derive(sqlx::FromRow)]
struct DueBindingRow {
    /// Binding identity.
    binding_id: Uuid,
    /// The binding's Verifier.
    verifier_card_uid: Uuid,
    /// The Card the binding verifies.
    card_uid: Uuid,
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
    /// Caller group, or the group a group execution judges.
    group_id: Option<String>,
    /// Binding owner of a background execution.
    owner_card_uid: Option<Uuid>,
    /// Deployment environment tag supplied at acceptance.
    deployment_environment: Option<String>,
    /// Admitted pull request pins of a `pull_request` execution.
    pull_request: Option<Json<PullRequestActivation>>,
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

    /// Every store operation is timed under its `op` with an `ok` or `error`
    /// outcome, and its result passes through unchanged.
    ///
    /// # Panics
    /// Panics when a result changes or a labelled sample is missing.
    #[tokio::test(flavor = "current_thread")]
    async fn store_operations_are_timed_by_op_and_outcome() {
        let recorder = metrics_exporter_prometheus::PrometheusBuilder::new().build_recorder();
        let handle = recorder.handle();
        let _guard = metrics::set_default_local_recorder(&recorder);
        assert_eq!(timed("claim", async { Ok(7) }).await.ok(), Some(7));
        assert!(
            timed::<()>("claim", async { Err(SqlxError::RowNotFound) })
                .await
                .is_err()
        );
        let rendered = handle.render();
        for outcome in ["ok", "error"] {
            let sample = format!(
                "{VERIFICATION_STORE_DURATION_SECONDS}_count{{op=\"claim\",outcome=\"{outcome}\"}} 1"
            );
            assert!(
                rendered.contains(&sample),
                "{sample} missing from {rendered}"
            );
        }
    }

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
            group_id: None,
            inputs: BTreeMap::new(),
            execution_route_overrides: BTreeMap::new(),
            deployment_environment: None,
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

/// Postgres-backed plan checks of the accept statements, skipped by the fast
/// lanes.
#[cfg(test)]
mod pg_tests {
    use serde_json::Value;
    use sqlx::AssertSqlSafe;
    use sqlx::types::Json;
    use uuid::Uuid;
    use wyrd_dev_fixtures::pg::PgFixture;

    use super::{KEYED_EXECUTIONS_SQL, RUNNABLE_TENANTS_SQL};

    /// Executions seeded before a plan is read; enough that reading the whole
    /// tenant is the plan a row-estimate join picks.
    const SEEDED: i32 = 100_000;

    /// Analyze `verifier_executions` while empty, then insert [`SEEDED`] rows
    /// for the fixture's tenant through `insert`, which binds the tenant as
    /// `$1`, `principal` as `$2`, and the row count as `$3`. Statistics are
    /// left stale, as on a table that filled after its last analyze.
    ///
    /// # Panics
    /// Panics when analyzing, seeding, or committing fails.
    async fn seed_stale(fixture: &PgFixture, principal: Uuid, insert: &'static str) {
        let superuser = fixture.superuser_pool().expect("superuser pool");
        sqlx::query("ANALYZE wyrd.verifier_executions")
            .execute(&superuser)
            .await
            .expect("empty table analyzes");
        let mut seed = superuser.begin().await.expect("seed transaction");
        sqlx::query("SET LOCAL session_replication_role = replica")
            .execute(&mut *seed)
            .await
            .expect("seeding skips foreign keys");
        sqlx::query(insert)
            .bind(fixture.data_tenant_id().as_uuid())
            .bind(principal)
            .bind(SEEDED)
            .execute(&mut *seed)
            .await
            .expect("executions seed");
        seed.commit().await.expect("seed commits");
    }

    /// Every node of `plan` that reads `verifier_executions`, as its node
    /// type and index name.
    fn execution_reads(plan: &Value) -> Vec<(Value, Value)> {
        let mut reads = Vec::new();
        let mut nodes = vec![&plan[0]["Plan"]];
        while let Some(node) = nodes.pop() {
            if node["Relation Name"] == "verifier_executions" {
                reads.push((node["Node Type"].clone(), node["Index Name"].clone()));
            }
            nodes.extend(node["Plans"].as_array().into_iter().flatten());
        }
        assert!(!reads.is_empty(), "the plan reads no executions: {plan}");
        reads
    }

    /// The idempotency lookup probes `verifier_runs_manual_idempotency` once
    /// per key and reads `verifier_executions` through nothing else, even
    /// when the table was analyzed empty and then filled, so a batch's cost
    /// never follows the tenant's execution count.
    ///
    /// # Panics
    /// Panics when the fixture cannot start, seeding or `EXPLAIN` fails, or
    /// the plan reads `verifier_executions` other than through that index.
    #[tokio::test]
    async fn keyed_lookup_probes_the_idempotency_index_however_large_the_tenant() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let principal = Uuid::now_v7();
        seed_stale(
            &fixture,
            principal,
            "INSERT INTO wyrd.verifier_executions (
                 execution_id, data_tenant_id, verifier_card_uid, verifier_version, card_uid,
                 origin, sub_principal_id, act_principal_id, authority, status, error,
                 max_attempts, created_at, updated_at, settled_at, idempotency_key,
                 request_sha256)
             SELECT gen_random_uuid(), $1, gen_random_uuid(), '1.0.0', gen_random_uuid(),
                    'manual', $2, $2, '{}', 'errored', '{}', 3, now(), now(), now(),
                    'key-' || g, '\\x00'
               FROM generate_series(1, $3) AS g",
        )
        .await;

        let keys: Vec<String> = (1..=1_000).map(|key| format!("key-{}", key * 50)).collect();
        let principals = vec![principal; keys.len()];
        let mut conn = fixture.tenant_conn().await.expect("tenant connection");
        let (Json(plan),): (Json<Value>,) = sqlx::query_as(AssertSqlSafe(format!(
            "EXPLAIN (FORMAT JSON) {KEYED_EXECUTIONS_SQL}"
        )))
        .bind(&principals)
        .bind(&principals)
        .bind(&keys)
        .fetch_one(&mut **conn.transaction())
        .await
        .expect("lookup explains");

        for (node, index) in execution_reads(&plan) {
            assert!(
                node == "Index Scan" && index == "verifier_runs_manual_idempotency",
                "{node} via {index} in {plan}"
            );
        }
    }

    /// The runnable-tenant scan reads `verifier_executions` only through its
    /// claim indexes, never the table or a bitmap of the backlog, so each
    /// claim round costs the tenants with work and not the rows queued.
    ///
    /// # Panics
    /// Panics when the fixture cannot start, seeding or `EXPLAIN` fails, or
    /// the plan reads `verifier_executions` other than through
    /// `verifier_runs_due`, `verifier_runs_leased`, or
    /// `verifier_executions_delivery_due`.
    #[tokio::test]
    async fn runnable_scan_walks_the_claim_indexes_however_large_the_backlog() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        seed_stale(
            &fixture,
            Uuid::now_v7(),
            "INSERT INTO wyrd.verifier_executions (
                 execution_id, data_tenant_id, verifier_card_uid, verifier_version, card_uid,
                 origin, sub_principal_id, act_principal_id, authority, status,
                 max_attempts, next_attempt_at, created_at, updated_at)
             SELECT gen_random_uuid(), $1, gen_random_uuid(), '1.0.0', gen_random_uuid(),
                    'manual', $2, $2, '{}', 'pending', 3, now() - g * INTERVAL '1 ms',
                    now(), now()
               FROM generate_series(1, $3) AS g",
        )
        .await;

        let operator = fixture.superuser_pool().expect("superuser pool");
        let (Json(plan),): (Json<Value>,) = sqlx::query_as(AssertSqlSafe(format!(
            "EXPLAIN (FORMAT JSON) {RUNNABLE_TENANTS_SQL}"
        )))
        .fetch_one(&operator)
        .await
        .expect("scan explains");

        for (node, index) in execution_reads(&plan) {
            assert!(
                (node == "Index Only Scan" || node == "Index Scan")
                    && matches!(
                        index.as_str(),
                        Some(
                            "verifier_runs_due"
                                | "verifier_runs_leased"
                                | "verifier_executions_delivery_due"
                        )
                    ),
                "{node} via {index} in {plan}"
            );
        }
    }
}
