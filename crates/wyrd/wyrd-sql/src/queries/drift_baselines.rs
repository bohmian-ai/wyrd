//! Fitted Drift baseline work and status: registration, claims, settlement.
//!
//! `wyrd.drift_baselines` holds one row per PSI or SPC Drift Verifier Card
//! version. It is both the fitter's durable work record and the status the
//! Verifier Card read serves. [`DriftBaselineQueue`] owns the retry policy
//! every transition applies; each tenant operation runs on the caller's
//! [`TenantConn`] under forced RLS without committing, so registration inserts
//! the pending row in the Card's own transaction. Cross-tenant work discovery
//! is an explicit [`OperatorPool`] read.
//!
//! Lifecycle: `pending` → (claim) `building` → `ready` | `failed` (retry due)
//! → `building` … | `failed` (final, no due time). A claim mints a fresh lease
//! token; every settlement is fenced on it, so a fitter whose lease was
//! reclaimed affects zero rows and observes [`Settlement::StaleLease`]. The
//! fitted profile is stored as the fitter's opaque JSON; this crate never
//! interprets it.
// raw-query grep allowlist: drift baseline table post-dates the sqlx offline cache; run `mise run sqlx:prepare` to promote to macros.

use chrono::{DateTime, Duration, Utc};
use serde_json::Value;
use sqlx::Error as SqlxError;
use sqlx::types::{Json, Uuid};
use wyrd_spec::DataTenantId;
use wyrd_spec::card::verifier::{DriftBaselineState, DriftBaselineStatus};
use wyrd_spec::ids::CardUid;
use wyrd_spec::reference::CardRef;
use wyrd_spec::verification::VerificationError;

use crate::queries::verifier_runs::{
    LeaseToken, RetryOutcome, RunRetryPolicy, Settlement, settlement, stored,
};
use crate::{OperatorPool, TenantConn};

/// Insert a registration's pending baseline, due at PostgreSQL's statement time.
const INSERT_PENDING_SQL: &str = r#"
    INSERT INTO wyrd.drift_baselines (
        data_tenant_id, verifier_uid, data_card_uid, max_attempts,
        next_attempt_at, created_at, updated_at
    ) VALUES (wyrd.current_tenant(), $1, $2, $3,
              statement_timestamp(), statement_timestamp(), statement_timestamp())
    ON CONFLICT DO NOTHING
"#;

/// Settle every expired lease whose attempts are exhausted as a final failure.
const EXHAUST_EXPIRED_SQL: &str = r#"
    UPDATE wyrd.drift_baselines
       SET state = 'failed', error = $1, lease_expires_at = NULL,
           next_attempt_at = NULL, updated_at = statement_timestamp()
     WHERE state = 'building'
       AND lease_expires_at <= statement_timestamp()
       AND attempts >= max_attempts
"#;

/// Claim the oldest due, retry-due, or lease-expired fit under a fresh lease.
///
/// `$2` is only the lease length in milliseconds; availability, expiry, and
/// the new deadline are PostgreSQL's statement time.
const CLAIM_SQL: &str = r#"
    WITH candidate AS (
        SELECT verifier_uid
          FROM wyrd.drift_baselines
         WHERE (state IN ('pending', 'failed') AND next_attempt_at <= statement_timestamp())
            OR (state = 'building' AND lease_expires_at <= statement_timestamp())
         ORDER BY COALESCE(next_attempt_at, lease_expires_at), verifier_uid
         LIMIT 1
           FOR UPDATE SKIP LOCKED
    )
    UPDATE wyrd.drift_baselines b
       SET state = 'building', attempts = b.attempts + 1, lease_token = $1,
           lease_expires_at = statement_timestamp() + ($2::bigint * INTERVAL '1 millisecond'),
           next_attempt_at = NULL, error = NULL, updated_at = statement_timestamp()
      FROM candidate
     WHERE b.verifier_uid = candidate.verifier_uid
    RETURNING b.verifier_uid, b.data_card_uid, b.attempts, b.max_attempts
"#;

/// Settle a leased fit `ready` with its fitted profile.
const COMPLETE_SQL: &str = r#"
    UPDATE wyrd.drift_baselines
       SET state = 'ready', fitted = $3, error = NULL, lease_expires_at = NULL,
           updated_at = statement_timestamp()
     WHERE verifier_uid = $1 AND lease_token = $2 AND state = 'building'
"#;

/// Settle a leased fit `failed`; `$4` is the retry delay in milliseconds, or
/// NULL for a final failure.
const FAIL_SQL: &str = r#"
    UPDATE wyrd.drift_baselines
       SET state = 'failed', error = $3, lease_expires_at = NULL,
           next_attempt_at = statement_timestamp() + ($4::bigint * INTERVAL '1 millisecond'),
           updated_at = statement_timestamp()
     WHERE verifier_uid = $1 AND lease_token = $2 AND state = 'building'
    RETURNING next_attempt_at
"#;

/// Lock a leased fit's attempt budget before deciding retry or exhaustion.
const LEASED_ATTEMPTS_SQL: &str = r#"
    SELECT attempts, max_attempts
      FROM wyrd.drift_baselines
     WHERE verifier_uid = $1 AND lease_token = $2 AND state = 'building'
       FOR UPDATE
"#;

/// Return a leased fit to `pending` immediately, refunding its attempt.
const RELEASE_SQL: &str = r#"
    UPDATE wyrd.drift_baselines
       SET state = 'pending', attempts = attempts - 1, lease_expires_at = NULL,
           next_attempt_at = statement_timestamp(), updated_at = statement_timestamp()
     WHERE verifier_uid = $1 AND lease_token = $2 AND state = 'building'
"#;

/// Read one Verifier's baseline status with its exact Data Card identity.
const STATUS_SQL: &str = r#"
    SELECT b.state, b.error,
           jsonb_build_object('kind', d.kind, 'space', d.space, 'name', d.name,
                              'version', d.version, 'uid', d.card_uid) AS data
      FROM wyrd.drift_baselines b
      JOIN wyrd.cards d ON d.card_uid = b.data_card_uid
     WHERE b.verifier_uid = $1
"#;

/// Read one Verifier's fitted profile when it is ready.
const FITTED_SQL: &str = r#"
    SELECT fitted FROM wyrd.drift_baselines
     WHERE verifier_uid = $1 AND state = 'ready'
"#;

/// List tenants with claimable fits, longest-waiting tenant first.
const DUE_TENANTS_SQL: &str = r#"
    SELECT data_tenant_id
      FROM wyrd.drift_baselines
     WHERE (state IN ('pending', 'failed') AND next_attempt_at <= statement_timestamp())
        OR (state = 'building' AND lease_expires_at <= statement_timestamp())
     GROUP BY data_tenant_id
     ORDER BY min(COALESCE(next_attempt_at, lease_expires_at)), data_tenant_id
     LIMIT $1
"#;

/// The identity a fitter settles a claimed baseline with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BaselineLease {
    /// The Verifier Card version whose baseline is being fitted.
    pub verifier_uid: CardUid,
    /// This claim's fencing token.
    pub token: LeaseToken,
}

/// A baseline claimed for fitting, with the exact identities the fitter needs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClaimedBaseline {
    /// Settlement identity.
    pub lease: BaselineLease,
    /// Exact baseline Data Card version to fit from.
    pub data_card_uid: CardUid,
    /// This attempt's number, starting at 1.
    pub attempt: i32,
    /// Attempt budget frozen at registration.
    pub max_attempts: i32,
}

/// Owner of the durable Drift baseline work and status transitions.
///
/// Holds the retry policy frozen into every new baseline. It holds no
/// connection: every tenant method runs on the caller's [`TenantConn`] and
/// never commits.
#[derive(Debug, Clone, Copy, Default)]
pub struct DriftBaselineQueue {
    /// Fit-failure retry policy; the same bounded doubling the run queue uses.
    retry: RunRetryPolicy,
}

impl DriftBaselineQueue {
    /// Insert the pending baseline of a newly registered PSI or SPC Verifier.
    ///
    /// Pins the exact Verifier and baseline Data Card versions and makes the
    /// row due at PostgreSQL's statement time. Runs in the registration
    /// transaction; an existing row for the same Verifier is kept, since the
    /// Verifier Card version and its baseline reference are immutable.
    ///
    /// # Errors
    /// Returns the database error when the insert fails, including a foreign
    /// key violation when either Card is not in the caller's tenant.
    #[tracing::instrument(skip(self, conn), fields(operation = "verification.baselines.register", verifier_uid = %verifier_uid))]
    pub async fn insert_pending(
        &self,
        conn: &mut TenantConn<'_>,
        verifier_uid: &CardUid,
        data_card_uid: &CardUid,
    ) -> Result<(), SqlxError> {
        sqlx::query(INSERT_PENDING_SQL)
            .bind(verifier_uid.as_uuid())
            .bind(data_card_uid.as_uuid())
            .bind(self.retry.max_attempts)
            .execute(&mut **conn.transaction())
            .await?;
        Ok(())
    }

    /// Claim one due fit of the caller's tenant under a fresh lease.
    ///
    /// First settles, as a final failure, every expired lease whose attempts
    /// are exhausted. Then locks the oldest `pending` or retry-due `failed`
    /// row, or `building` row whose lease the database considers expired, with
    /// `SKIP LOCKED`; marks it `building` with a new token expiring `lease_for`
    /// after PostgreSQL's statement time; and counts the attempt. Returns
    /// `None` when nothing is due. The caller commits before fitting.
    ///
    /// # Errors
    /// Returns the database error when a statement fails, or a decode error
    /// when a stored identity is malformed.
    #[tracing::instrument(skip(self, conn), fields(operation = "verification.baselines.claim"))]
    pub async fn claim(
        &self,
        conn: &mut TenantConn<'_>,
        lease_for: Duration,
    ) -> Result<Option<ClaimedBaseline>, SqlxError> {
        let exhausted = VerificationError {
            code: "lease_expired".to_owned(),
            message: "the baseline fit's lease expired after its final attempt".to_owned(),
        };
        sqlx::query(EXHAUST_EXPIRED_SQL)
            .bind(Json(&exhausted))
            .execute(&mut **conn.transaction())
            .await?;
        let token = Uuid::now_v7();
        let row: Option<(Uuid, Uuid, i32, i32)> = sqlx::query_as(CLAIM_SQL)
            .bind(token)
            .bind(lease_for.num_milliseconds())
            .fetch_optional(&mut **conn.transaction())
            .await?;
        row.map(|(verifier, data, attempt, max_attempts)| {
            Ok(ClaimedBaseline {
                lease: BaselineLease {
                    verifier_uid: stored(CardUid::from_uuid(verifier))?,
                    token: LeaseToken(token),
                },
                data_card_uid: stored(CardUid::from_uuid(data))?,
                attempt,
                max_attempts,
            })
        })
        .transpose()
    }

    /// Settle a claimed fit `ready` with the fitter's serialized profile.
    ///
    /// Only the current lease holder can complete. From this commit on,
    /// `wyrd.verifier_readiness` reports the Verifier ready.
    ///
    /// # Errors
    /// Returns the database error when the update fails.
    #[tracing::instrument(skip(self, conn, fitted), fields(operation = "verification.baselines.complete", verifier_uid = %lease.verifier_uid))]
    pub async fn complete(
        &self,
        conn: &mut TenantConn<'_>,
        lease: &BaselineLease,
        fitted: &Value,
    ) -> Result<Settlement, SqlxError> {
        let result = sqlx::query(COMPLETE_SQL)
            .bind(lease.verifier_uid.as_uuid())
            .bind(lease.token.0)
            .bind(Json(fitted))
            .execute(&mut **conn.transaction())
            .await?;
        Ok(settlement(result.rows_affected()))
    }

    /// Settle a claimed fit `failed` with a structured error.
    ///
    /// With attempts left, the row stays visibly `failed` and becomes due
    /// again after the policy's backoff from PostgreSQL's statement time; the
    /// stored deadline is returned. With the budget exhausted, the failure is
    /// final and the row is never claimed again.
    ///
    /// # Errors
    /// Returns the database error when a statement fails.
    #[tracing::instrument(skip(self, conn, error), fields(operation = "verification.baselines.fail", verifier_uid = %lease.verifier_uid))]
    pub async fn fail(
        &self,
        conn: &mut TenantConn<'_>,
        lease: &BaselineLease,
        error: &VerificationError,
    ) -> Result<RetryOutcome, SqlxError> {
        let budget: Option<(i32, i32)> = sqlx::query_as(LEASED_ATTEMPTS_SQL)
            .bind(lease.verifier_uid.as_uuid())
            .bind(lease.token.0)
            .fetch_optional(&mut **conn.transaction())
            .await?;
        let Some((attempts, max_attempts)) = budget else {
            return Ok(RetryOutcome::StaleLease);
        };
        let delay =
            (attempts < max_attempts).then(|| self.retry.delay_after(attempts).num_milliseconds());
        let next_attempt_at: Option<DateTime<Utc>> = sqlx::query_scalar(FAIL_SQL)
            .bind(lease.verifier_uid.as_uuid())
            .bind(lease.token.0)
            .bind(Json(error))
            .bind(delay)
            .fetch_one(&mut **conn.transaction())
            .await?;
        Ok(next_attempt_at.map_or(RetryOutcome::Exhausted, RetryOutcome::Scheduled))
    }

    /// Return a claimed fit to `pending` immediately, for shutdown drain.
    ///
    /// The interrupted attempt is refunded, so a drained fit does not consume
    /// its retry budget.
    ///
    /// # Errors
    /// Returns the database error when the update fails.
    #[tracing::instrument(skip(self, conn), fields(operation = "verification.baselines.release", verifier_uid = %lease.verifier_uid))]
    pub async fn release(
        &self,
        conn: &mut TenantConn<'_>,
        lease: &BaselineLease,
    ) -> Result<Settlement, SqlxError> {
        let result = sqlx::query(RELEASE_SQL)
            .bind(lease.verifier_uid.as_uuid())
            .bind(lease.token.0)
            .execute(&mut **conn.transaction())
            .await?;
        Ok(settlement(result.rows_affected()))
    }

    /// Read one Verifier's baseline status for the Card read.
    ///
    /// Returns the state, the exact UID-pinned baseline Data Card, and the
    /// last failure's structured error. A Verifier without a baseline row
    /// (Custom Drift, Eval, or any other Card) is `Ok(None)`.
    ///
    /// # Errors
    /// Returns the database error when the read fails, or a decode error when
    /// a stored state, identity, or error is malformed.
    pub async fn status(
        &self,
        conn: &mut TenantConn<'_>,
        verifier_uid: &CardUid,
    ) -> Result<Option<DriftBaselineStatus>, SqlxError> {
        let row: Option<(String, Option<Json<VerificationError>>, Json<CardRef>)> =
            sqlx::query_as(STATUS_SQL)
                .bind(verifier_uid.as_uuid())
                .fetch_optional(&mut **conn.transaction())
                .await?;
        row.map(|(state, error, Json(data))| {
            let state = DriftBaselineState::parse(&state).ok_or_else(|| {
                SqlxError::Decode(format!("unknown drift baseline state {state:?}").into())
            })?;
            Ok(DriftBaselineStatus {
                state,
                data,
                error: error.map(|Json(error)| error),
            })
        })
        .transpose()
    }

    /// Read one Verifier's fitted profile, or `None` until it is ready.
    ///
    /// # Errors
    /// Returns the database error when the read fails.
    pub async fn fitted(
        &self,
        conn: &mut TenantConn<'_>,
        verifier_uid: &CardUid,
    ) -> Result<Option<Value>, SqlxError> {
        let fitted: Option<Json<Value>> = sqlx::query_scalar(FITTED_SQL)
            .bind(verifier_uid.as_uuid())
            .fetch_optional(&mut **conn.transaction())
            .await?;
        Ok(fitted.map(|Json(fitted)| fitted))
    }

    /// List up to `limit` tenants that have a claimable fit at database time.
    ///
    /// Longest-waiting tenant first; claiming happens per tenant through
    /// [`Self::claim`].
    ///
    /// # Errors
    /// Returns the database error when the read fails, or a decode error when
    /// a stored tenant key is malformed.
    // tenant-isolation: cross-tenant OperatorPool
    pub async fn tenants_with_due_fits(
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
}
