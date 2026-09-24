//! Operator dispatch queue: leased claims, delivery settlement, and deadlines.
//!
//! Settlement of a failed binding-created run inserts `wyrd.operator_dispatches`
//! rows (see [`crate::queries::verifier_runs`]); this module is the Operator
//! worker's side. [`OperatorDispatchQueue`] owns the server delivery ceilings —
//! attempt budget, dispatch deadline, and lease length — and every statement
//! evaluates availability, expiry, retry eligibility, and the deadline against
//! PostgreSQL's `statement_timestamp()`. Rust binds only durations and receives
//! the remaining interval, never a timestamp to compare with its own clock.
//!
//! Lifecycle: `pending` → (claim) `running` → `delivered` | `retrying` →
//! `running` … | `failed`. Every settlement is fenced on the claim's lease
//! token, so a worker whose lease was reclaimed changes nothing.
// raw-query grep allowlist: operator dispatch delivery post-dates the sqlx offline cache; run `mise run sqlx:prepare` to promote to macros.

use std::time::Duration;

use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::Error as SqlxError;
use sqlx::types::{Json, Uuid};
use wyrd_spec::DataTenantId;
use wyrd_spec::ids::{CardUid, IdError, OperatorDispatchId, VerificationRunId};
use wyrd_spec::verification::{FrozenTarget, VerificationError};

use crate::queries::verifier_runs::{LeaseToken, RetryOutcome, Settlement, settlement};
use crate::{OperatorPool, TenantConn};

/// Fail every dispatch that can no longer be attempted.
///
/// A due or lease-expired row fails when its attempt budget (`$1`) is spent or
/// its deadline (`$2` ms after creation) has passed; a live lease is left to
/// its holder.
const EXHAUST_SQL: &str = r#"
    UPDATE wyrd.operator_dispatches
       SET status = 'failed',
           last_error = CASE
               WHEN created_at + ($2::bigint * INTERVAL '1 millisecond') <= statement_timestamp()
                   THEN jsonb_build_object('code', 'deadline_exceeded',
                        'message', 'the dispatch delivery deadline passed',
                        'last', last_error)
               ELSE jsonb_build_object('code', 'attempts_exhausted',
                        'message', 'the dispatch attempt budget is spent',
                        'last', last_error)
           END,
           lease_expires_at = NULL, next_attempt_at = NULL,
           updated_at = statement_timestamp()
     WHERE ((status IN ('pending', 'retrying') AND next_attempt_at <= statement_timestamp())
            OR (status = 'running' AND lease_expires_at <= statement_timestamp()))
       AND (attempts >= $1
            OR created_at + ($2::bigint * INTERVAL '1 millisecond') <= statement_timestamp())
"#;

/// Claim the oldest due or lease-expired dispatch under a fresh lease.
///
/// Returns the remaining deadline in milliseconds as PostgreSQL computes it.
const CLAIM_SQL: &str = r#"
    WITH candidate AS (
        SELECT dispatch_id
          FROM wyrd.operator_dispatches
         WHERE ((status IN ('pending', 'retrying') AND next_attempt_at <= statement_timestamp())
                OR (status = 'running' AND lease_expires_at <= statement_timestamp()))
           AND attempts < $3
           AND created_at + ($4::bigint * INTERVAL '1 millisecond') > statement_timestamp()
         ORDER BY COALESCE(next_attempt_at, lease_expires_at), dispatch_id
         LIMIT 1
           FOR UPDATE SKIP LOCKED
    )
    UPDATE wyrd.operator_dispatches d
       SET status = 'running', attempts = d.attempts + 1, lease_token = $1,
           lease_expires_at = statement_timestamp() + ($2::bigint * INTERVAL '1 millisecond'),
           next_attempt_at = NULL, updated_at = statement_timestamp()
      FROM candidate, wyrd.verifier_runs r
     WHERE d.dispatch_id = candidate.dispatch_id
       AND r.run_id = d.run_id
    RETURNING d.dispatch_id, d.run_id, r.owner_card_uid, d.operator_uid,
              d.operator_digest, d.failure_context, d.attempts,
              GREATEST(0, floor(extract(epoch FROM
                  d.created_at + ($4::bigint * INTERVAL '1 millisecond') - statement_timestamp()
              ) * 1000))::bigint AS remaining_ms
"#;

/// Settle a leased dispatch `delivered`.
const DELIVER_SQL: &str = r#"
    UPDATE wyrd.operator_dispatches
       SET status = 'delivered', last_error = NULL, lease_expires_at = NULL,
           updated_at = statement_timestamp()
     WHERE dispatch_id = $1 AND lease_token = $2 AND status = 'running'
"#;

/// Settle a leased dispatch `failed` with its terminal error.
const FAIL_SQL: &str = r#"
    UPDATE wyrd.operator_dispatches
       SET status = 'failed', last_error = $3, lease_expires_at = NULL,
           updated_at = statement_timestamp()
     WHERE dispatch_id = $1 AND lease_token = $2 AND status = 'running'
"#;

/// Schedule a retry within the budget and deadline, or fail.
///
/// `$4` is the attempt budget, `$5` the delay and `$6` the deadline, both in
/// milliseconds. The next attempt is clipped to the deadline; a row already
/// at or past it fails.
const RETRY_SQL: &str = r#"
    UPDATE wyrd.operator_dispatches
       SET status = CASE WHEN attempts < $4
                          AND created_at + ($6::bigint * INTERVAL '1 millisecond') > statement_timestamp()
                         THEN 'retrying' ELSE 'failed' END,
           next_attempt_at = CASE WHEN attempts < $4
                          AND created_at + ($6::bigint * INTERVAL '1 millisecond') > statement_timestamp()
                         THEN LEAST(statement_timestamp() + ($5::bigint * INTERVAL '1 millisecond'),
                                    created_at + ($6::bigint * INTERVAL '1 millisecond'))
                         END,
           last_error = $3, lease_expires_at = NULL, updated_at = statement_timestamp()
     WHERE dispatch_id = $1 AND lease_token = $2 AND status = 'running'
    RETURNING status, next_attempt_at
"#;

/// Return a leased dispatch to the queue immediately, refunding its attempt.
const RELEASE_SQL: &str = r#"
    UPDATE wyrd.operator_dispatches
       SET status = CASE WHEN attempts > 1 THEN 'retrying' ELSE 'pending' END,
           attempts = attempts - 1, lease_expires_at = NULL,
           next_attempt_at = statement_timestamp(), updated_at = statement_timestamp()
     WHERE dispatch_id = $1 AND lease_token = $2 AND status = 'running'
"#;

/// List tenants with claimable or exhaustible dispatches, longest-waiting first.
const DUE_TENANTS_SQL: &str = r#"
    SELECT data_tenant_id
      FROM wyrd.operator_dispatches
     WHERE (status IN ('pending', 'retrying') AND next_attempt_at <= statement_timestamp())
        OR (status = 'running' AND lease_expires_at <= statement_timestamp())
     GROUP BY data_tenant_id
     ORDER BY min(COALESCE(next_attempt_at, lease_expires_at)), data_tenant_id
     LIMIT $1
"#;

/// Server delivery ceilings every dispatch transition applies.
///
/// A Card cannot raise them. The defaults are three attempts, a five-minute
/// deadline from dispatch creation, and a 45-second lease that outlives the
/// 30-second attempt timeout, so a live attempt is never reclaimed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OperatorDispatchQueue {
    /// Total attempts, including the first.
    max_attempts: i32,
    /// Deadline measured from dispatch creation.
    deadline: Duration,
    /// Lease length of one claim.
    lease: Duration,
}

impl Default for OperatorDispatchQueue {
    /// Three attempts within a five-minute deadline and a 45-second lease;
    /// the lease exceeds the 30-second attempt timeout so a live attempt is
    /// never reclaimed by another worker.
    fn default() -> Self {
        Self::new(3, Duration::from_secs(300), Duration::from_secs(45))
    }
}

/// The identity a worker settles a claimed dispatch with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DispatchLease {
    /// The claimed dispatch.
    pub dispatch_id: OperatorDispatchId,
    /// This claim's fencing token.
    pub token: LeaseToken,
}

/// A dispatch claimed for one delivery attempt.
#[derive(Debug, Clone, PartialEq)]
pub struct ClaimedDispatch {
    /// Settlement identity.
    pub lease: DispatchLease,
    /// The failed run.
    pub run_id: VerificationRunId,
    /// Owner Card whose binding configured the Operator.
    pub owner_card_uid: Option<CardUid>,
    /// The frozen Operator identity.
    pub operator: FrozenTarget,
    /// The frozen failure context as stored.
    pub failure_context: Value,
    /// This attempt's number, starting at 1.
    pub attempt: i32,
    /// Time left before the dispatch deadline, as PostgreSQL computed it.
    pub remaining: Duration,
}

impl OperatorDispatchQueue {
    /// Build a queue with explicit ceilings.
    ///
    /// # Panics
    /// Panics when `max_attempts` is not positive.
    #[must_use]
    pub fn new(max_attempts: i32, deadline: Duration, lease: Duration) -> Self {
        assert!(
            max_attempts > 0,
            "invariant: a dispatch is attempted at least once"
        );
        Self {
            max_attempts,
            deadline,
            lease,
        }
    }

    /// Fail unattemptable dispatches, then claim the tenant's next due one.
    ///
    /// Both statements run on the caller's transaction; the caller commits so
    /// the claim is durable before any external send.
    ///
    /// # Errors
    /// Returns the database error when a statement fails, or a decode error
    /// when stored identities are malformed.
    pub async fn claim(
        &self,
        conn: &mut TenantConn<'_>,
    ) -> Result<Option<ClaimedDispatch>, SqlxError> {
        sqlx::query(EXHAUST_SQL)
            .bind(self.max_attempts)
            .bind(millis(self.deadline))
            .execute(&mut **conn.transaction())
            .await?;
        let token = Uuid::now_v7();
        let row: Option<ClaimRow> = sqlx::query_as(CLAIM_SQL)
            .bind(token)
            .bind(millis(self.lease))
            .bind(self.max_attempts)
            .bind(millis(self.deadline))
            .fetch_optional(&mut **conn.transaction())
            .await?;
        row.map(|row| row.into_claimed(LeaseToken(token)))
            .transpose()
    }

    /// Settle a claimed dispatch `delivered`.
    ///
    /// # Errors
    /// Returns the database error when the update fails.
    pub async fn deliver(
        &self,
        conn: &mut TenantConn<'_>,
        lease: DispatchLease,
    ) -> Result<Settlement, SqlxError> {
        let done = sqlx::query(DELIVER_SQL)
            .bind(lease.dispatch_id.as_uuid())
            .bind(lease.token.0)
            .execute(&mut **conn.transaction())
            .await?;
        Ok(settlement(done.rows_affected()))
    }

    /// Settle a claimed dispatch `failed` with a terminal error.
    ///
    /// # Errors
    /// Returns the database error when the update fails.
    pub async fn fail(
        &self,
        conn: &mut TenantConn<'_>,
        lease: DispatchLease,
        error: &VerificationError,
    ) -> Result<Settlement, SqlxError> {
        let done = sqlx::query(FAIL_SQL)
            .bind(lease.dispatch_id.as_uuid())
            .bind(lease.token.0)
            .bind(Json(error))
            .execute(&mut **conn.transaction())
            .await?;
        Ok(settlement(done.rows_affected()))
    }

    /// Schedule another attempt after `delay`, clipped to the deadline, or fail.
    ///
    /// # Errors
    /// Returns the database error when the update fails.
    pub async fn retry(
        &self,
        conn: &mut TenantConn<'_>,
        lease: DispatchLease,
        error: &VerificationError,
        delay: Duration,
    ) -> Result<RetryOutcome, SqlxError> {
        let row: Option<(String, Option<DateTime<Utc>>)> = sqlx::query_as(RETRY_SQL)
            .bind(lease.dispatch_id.as_uuid())
            .bind(lease.token.0)
            .bind(Json(error))
            .bind(self.max_attempts)
            .bind(millis(delay))
            .bind(millis(self.deadline))
            .fetch_optional(&mut **conn.transaction())
            .await?;
        Ok(match row {
            None => RetryOutcome::StaleLease,
            Some((_, Some(at))) => RetryOutcome::Scheduled(at),
            Some((_, None)) => RetryOutcome::Exhausted,
        })
    }

    /// Return a claimed dispatch to the queue at once, refunding its attempt.
    ///
    /// Used when shutdown cancels an attempt that may not have sent.
    ///
    /// # Errors
    /// Returns the database error when the update fails.
    pub async fn release(
        &self,
        conn: &mut TenantConn<'_>,
        lease: DispatchLease,
    ) -> Result<Settlement, SqlxError> {
        let done = sqlx::query(RELEASE_SQL)
            .bind(lease.dispatch_id.as_uuid())
            .bind(lease.token.0)
            .execute(&mut **conn.transaction())
            .await?;
        Ok(settlement(done.rows_affected()))
    }

    /// Tenants with due, lease-expired, or exhaustible dispatches.
    ///
    /// # Errors
    /// Returns the database error when the read fails.
    // tenant-isolation: cross-tenant OperatorPool
    pub async fn due_tenants(
        &self,
        operator: &OperatorPool,
        limit: i64,
    ) -> Result<Vec<DataTenantId>, SqlxError> {
        let rows: Vec<(Uuid,)> = sqlx::query_as(DUE_TENANTS_SQL)
            .bind(limit)
            .fetch_all(operator.pool())
            .await?;
        rows.into_iter()
            .map(|(tenant,)| {
                DataTenantId::new(tenant).map_err(|error| SqlxError::Decode(Box::new(error)))
            })
            .collect()
    }
}

/// A duration as whole milliseconds for a bound SQL interval.
fn millis(duration: Duration) -> i64 {
    i64::try_from(duration.as_millis()).unwrap_or(i64::MAX)
}

/// The row [`CLAIM_SQL`] returns.
#[derive(sqlx::FromRow)]
struct ClaimRow {
    /// Dispatch identity.
    dispatch_id: Uuid,
    /// Run identity.
    run_id: Uuid,
    /// Owner Card.
    owner_card_uid: Option<Uuid>,
    /// Operator Card UID.
    operator_uid: Option<Uuid>,
    /// Inline Operator digest.
    operator_digest: Option<String>,
    /// Frozen failure context.
    failure_context: Json<Value>,
    /// Attempt number.
    attempts: i32,
    /// Remaining deadline in milliseconds.
    remaining_ms: i64,
}

impl ClaimRow {
    /// Decode into a typed claim fenced by `token`.
    ///
    /// # Errors
    /// Returns [`SqlxError::Decode`] for malformed identities.
    fn into_claimed(self, token: LeaseToken) -> Result<ClaimedDispatch, SqlxError> {
        let decode = |error: IdError| SqlxError::Decode(Box::new(error));
        let operator = match (self.operator_uid, self.operator_digest) {
            (Some(uid), None) => FrozenTarget::Uid(CardUid::from_uuid(uid).map_err(decode)?),
            (None, Some(digest)) => FrozenTarget::Digest(digest),
            _ => {
                return Err(SqlxError::Decode(
                    "dispatch Operator identity is malformed".into(),
                ));
            }
        };
        Ok(ClaimedDispatch {
            lease: DispatchLease {
                dispatch_id: OperatorDispatchId::new(self.dispatch_id).map_err(decode)?,
                token,
            },
            run_id: VerificationRunId::new(self.run_id).map_err(decode)?,
            owner_card_uid: self
                .owner_card_uid
                .map(CardUid::from_uuid)
                .transpose()
                .map_err(decode)?,
            operator,
            failure_context: self.failure_context.0,
            attempt: self.attempts,
            remaining: Duration::from_millis(u64::try_from(self.remaining_ms).unwrap_or(0)),
        })
    }
}
