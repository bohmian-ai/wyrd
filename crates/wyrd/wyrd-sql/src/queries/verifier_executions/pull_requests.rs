//! Pull request admission: one queued execution per frozen `pull_request`
//! binding of a Repository, for one admitted PR source generation.
//!
//! A provider PR observation is decided under a per-PR transaction lock
//! ([`VerifierExecutionQueue::lock_pull_request`]) against the PR's current
//! generation: an observation older than the latest one is obsolete, a
//! closed or draft PR deactivates it ([`VerifierExecutionQueue::deactivate_pull_request`]),
//! an unchanged source replays it, and a new source reserves a new
//! generation ([`VerifierExecutionQueue::reserve_pull_request`]) that cancels
//! every older unfinished execution of the PR in the same transaction.
//!
//! A reservation freezes each binding's Verifier, subject, Trigger,
//! Operators and accepted authority with the PR pins before any input record
//! is staged, so a retry after a crash reuses the first accepted policy and
//! identities. Its rows are `pending` but due at `infinity`, which no claim
//! reaches, until [`VerifierExecutionQueue::activate_pull_requests`] admits
//! them once Scribe acknowledged their input records. Cancellation fences
//! every later settlement exactly as [`VerifierExecutionQueue::cancel`] does,
//! so supersession, closure and access loss stop queued, running and later
//! parked work alike, while settled history keeps its pins.
// raw-query grep allowlist: verifier execution tables post-date the sqlx offline cache; run `mise run sqlx:prepare` to promote to macros.

use chrono::{DateTime, Utc};
use sqlx::Error as SqlxError;
use sqlx::types::{Json, Uuid};
use wyrd_runtime::principal::PrincipalId;
use wyrd_spec::auth::Attribution;
use wyrd_spec::ids::{BindingId, CardUid, ExecutionId, RepositoryId};
use wyrd_spec::verification::execution::PullRequestActivation;
use wyrd_spec::verification::{VerificationError, VerificationExecutionStatus};

use super::{
    EnqueueRefusal, ExecutionAuthority, VerifierExecutionQueue, resolve_binding, stored, timed,
};
use crate::TenantConn;

/// Advisory lock class of [`LOCK_PULL_REQUEST_SQL`], apart from every other
/// advisory lock in the database.
const PULL_REQUEST_LOCK_CLASS: i32 = 0x0CA2_D032;

/// Serialize every decision about one tenant's PR until the transaction ends.
const LOCK_PULL_REQUEST_SQL: &str = "SELECT pg_advisory_xact_lock($1, hashtext($2))";

/// The PR predicate every statement below shares: `$1` host, `$2`
/// repository id, `$3` number. It matches the partial
/// `verifier_executions_pull_request` index.
macro_rules! this_pull_request {
    () => {
        "origin = 'pull_request' AND pull_request->>'host' = $1 \
         AND pull_request->>'repository_id' = $2 \
         AND (pull_request->>'number')::bigint = $3"
    };
}

/// The latest observation of the PR: its newest generation or deactivation.
const LATEST_SQL: &str = concat!(
    "SELECT max(GREATEST(pr_observed_at, COALESCE(pr_deactivated_at, '-infinity')))
       FROM wyrd.verifier_executions WHERE ",
    this_pull_request!()
);

/// Every execution of the PR's newest generation, in binding order.
const CURRENT_SQL: &str = concat!(
    "SELECT execution_id, binding_id, status, next_attempt_at = 'infinity' AS reserved,
            verifier_card_uid, card_uid, owner_card_uid, sub_principal_id, act_principal_id,
            pull_request, pr_observed_at, pr_deactivated_at
       FROM wyrd.verifier_executions
      WHERE ",
    this_pull_request!(),
    " AND pr_observed_at = (SELECT max(pr_observed_at) FROM wyrd.verifier_executions WHERE ",
    this_pull_request!(),
    ") ORDER BY binding_id"
);

/// Cancel the PR's unfinished executions with error `$4`, fencing their
/// workers as [`super::CANCEL_SQL`] does, and mark every generation not yet
/// deactivated as deactivated at `$5`, or at statement time when `$5` is
/// null.
const DEACTIVATE_SQL: &str = concat!(
    "UPDATE wyrd.verifier_executions
        SET status = CASE WHEN status IN ('pending', 'retrying', 'running')
                          THEN 'cancelled' ELSE status END,
            error = CASE WHEN status IN ('pending', 'retrying', 'running') THEN $4 ELSE error END,
            lease_expires_at = CASE WHEN status IN ('pending', 'retrying', 'running')
                                    THEN NULL ELSE lease_expires_at END,
            next_attempt_at = CASE WHEN status IN ('pending', 'retrying', 'running')
                                   THEN NULL ELSE next_attempt_at END,
            settled_at = CASE WHEN status IN ('pending', 'retrying', 'running')
                              THEN statement_timestamp() ELSE settled_at END,
            delivery_due_at = CASE WHEN status IN ('pending', 'retrying', 'running')
                                   THEN statement_timestamp() ELSE delivery_due_at END,
            pr_deactivated_at = COALESCE(pr_deactivated_at, $5, statement_timestamp()),
            updated_at = statement_timestamp()
      WHERE ",
    this_pull_request!(),
    " AND (status IN ('pending', 'retrying', 'running') OR pr_deactivated_at IS NULL)
     RETURNING execution_id, status = 'cancelled' AND error = $4 AS cancelled"
);

/// Cancel the PR's unfinished executions superseded by a new generation with
/// error `$4`, fencing their workers as [`super::CANCEL_SQL`] does.
const SUPERSEDE_SQL: &str = concat!(
    "UPDATE wyrd.verifier_executions
        SET status = 'cancelled', error = $4, lease_expires_at = NULL, next_attempt_at = NULL,
            settled_at = statement_timestamp(), delivery_due_at = statement_timestamp(),
            updated_at = statement_timestamp()
      WHERE ",
    this_pull_request!(),
    " AND status IN ('pending', 'retrying', 'running')
     RETURNING execution_id"
);

/// Cancel the unfinished PR executions of host `$1` whose repository is one
/// of `$2`, or of every repository when `$2` is null, with error `$3`, and
/// deactivate their generations at statement time.
const CANCEL_REPOSITORIES_SQL: &str = "
    UPDATE wyrd.verifier_executions
       SET status = CASE WHEN status IN ('pending', 'retrying', 'running')
                         THEN 'cancelled' ELSE status END,
           error = CASE WHEN status IN ('pending', 'retrying', 'running') THEN $3 ELSE error END,
           lease_expires_at = CASE WHEN status IN ('pending', 'retrying', 'running')
                                   THEN NULL ELSE lease_expires_at END,
           next_attempt_at = CASE WHEN status IN ('pending', 'retrying', 'running')
                                  THEN NULL ELSE next_attempt_at END,
           settled_at = CASE WHEN status IN ('pending', 'retrying', 'running')
                             THEN statement_timestamp() ELSE settled_at END,
           delivery_due_at = CASE WHEN status IN ('pending', 'retrying', 'running')
                                  THEN statement_timestamp() ELSE delivery_due_at END,
           pr_deactivated_at = COALESCE(pr_deactivated_at, statement_timestamp()),
           updated_at = statement_timestamp()
     WHERE origin = 'pull_request' AND pull_request->>'host' = $1
       AND ($2::text[] IS NULL OR pull_request->>'repository_id' = ANY($2))
       AND (status IN ('pending', 'retrying', 'running') OR pr_deactivated_at IS NULL)
";

/// The `pull_request` bindings a Repository Card version owns, with each
/// bound Verifier, in binding order.
const BINDINGS_SQL: &str = "
    SELECT binding_id, verifier_card_uid
      FROM wyrd.verification_bindings
     WHERE owner_card_uid = $1 AND activation = 'pull_request'
     ORDER BY binding_id
";

/// Reserve one runnable execution of binding `$6` for PR pins `$15` observed
/// at `$16`: `pending` and due at `infinity` until its handoff activates it.
/// A binding deleted since its target was read joins no row.
const RESERVE_SQL: &str = "
    INSERT INTO wyrd.verifier_executions (
        execution_id, data_tenant_id, verifier_card_uid, verifier_version, card_uid,
        origin, owner_card_uid, binding_id, trigger_uid, trigger_digest, operators,
        sub_principal_id, act_principal_id, authority, max_attempts,
        next_attempt_at, created_at, updated_at, pull_request, pr_observed_at
    )
    SELECT $1, wyrd.current_tenant(), $2, $3, $4, 'pull_request', $5, $6, $7, $8, $9,
           $10, $11, $12, $13, 'infinity', statement_timestamp(), statement_timestamp(),
           $14, $15
     WHERE EXISTS (SELECT 1 FROM wyrd.verification_bindings WHERE binding_id = $6)
";

/// Record binding `$2` refused at admission of PR pins `$7` observed at `$8`
/// as one terminal errored execution `$1` with error `$5`, serving `$3` and
/// recorded by `$4`, its delivery due at once. A missing binding or Verifier
/// Card inserts nothing.
const REFUSE_SQL: &str = "
    INSERT INTO wyrd.verifier_executions (
        execution_id, data_tenant_id, verifier_card_uid, verifier_version, card_uid,
        origin, owner_card_uid, binding_id, trigger_uid, trigger_digest, operators,
        sub_principal_id, act_principal_id, status, error, max_attempts,
        created_at, updated_at, settled_at, delivery_due_at, pull_request, pr_observed_at
    )
    SELECT $1, wyrd.current_tenant(), b.verifier_card_uid, v.version, b.card_uid,
           'pull_request', b.owner_card_uid, b.binding_id, b.trigger_uid, b.trigger_digest,
           b.operators, $3, $4, 'errored', $5, $6,
           statement_timestamp(), statement_timestamp(), statement_timestamp(),
           statement_timestamp(), $7, $8
      FROM wyrd.verification_bindings b
      JOIN wyrd.cards v ON v.card_uid = b.verifier_card_uid
     WHERE b.binding_id = $2
";

/// Make reserved executions `$1` due at statement time; an execution already
/// activated, cancelled or settled is left as it is.
const ACTIVATE_SQL: &str = "
    UPDATE wyrd.verifier_executions
       SET next_attempt_at = statement_timestamp(), created_at = statement_timestamp(),
           updated_at = statement_timestamp()
     WHERE execution_id = ANY($1) AND origin = 'pull_request'
       AND status = 'pending' AND next_attempt_at = 'infinity'
    RETURNING execution_id
";

/// The error a superseded execution is cancelled with.
#[must_use]
pub fn superseded() -> VerificationError {
    VerificationError {
        code: "pull_request_superseded".to_owned(),
        message: "a newer source of the pull request superseded this execution".to_owned(),
    }
}

/// One pull request, by provider host, repository and number.
#[derive(Debug, Clone, Copy)]
pub struct PullRequestKey<'a> {
    /// Provider web host.
    pub host: &'a str,
    /// Stable provider repository id.
    pub repository_id: &'a RepositoryId,
    /// Pull request number.
    pub number: u64,
}

impl PullRequestKey<'_> {
    /// The number as the stored `bigint`.
    ///
    /// # Errors
    /// Returns [`SqlxError::Encode`] for a number beyond `i64`.
    fn number(&self) -> Result<i64, SqlxError> {
        i64::try_from(self.number).map_err(|error| SqlxError::Encode(Box::new(error)))
    }
}

/// Why a PR stopped being reviewable, recorded as the cancellation code of
/// its unfinished executions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PullRequestEnd {
    /// The PR was closed or merged.
    Closed,
    /// The PR was converted to a draft.
    Draft,
}

impl PullRequestEnd {
    /// The error the PR's unfinished executions are cancelled with.
    #[must_use]
    pub fn error(self) -> VerificationError {
        match self {
            Self::Closed => VerificationError {
                code: "pull_request_closed".to_owned(),
                message: "the pull request was closed".to_owned(),
            },
            Self::Draft => VerificationError {
                code: "pull_request_draft".to_owned(),
                message: "the pull request was converted to a draft".to_owned(),
            },
        }
    }
}

/// The error executions of a repository the tenant can no longer reach are
/// cancelled with.
#[must_use]
pub fn access_lost() -> VerificationError {
    VerificationError {
        code: "repository_access_lost".to_owned(),
        message: "the tenant's GitHub connection no longer reaches the repository".to_owned(),
    }
}

/// One frozen execution of a PR generation.
#[derive(Debug, Clone, PartialEq)]
pub struct PullRequestExecution {
    /// The execution.
    pub execution_id: ExecutionId,
    /// Its `pull_request` binding.
    pub binding_id: BindingId,
    /// Current status.
    pub status: VerificationExecutionStatus,
    /// Whether it still awaits its handoff: reserved and never activated.
    pub reserved: bool,
    /// Exact Verifier.
    pub verifier_card_uid: CardUid,
    /// Exact subject.
    pub card_uid: CardUid,
    /// The Repository Card version that owns the binding.
    pub owner_card_uid: CardUid,
    /// Who the execution serves and who accepted it.
    pub attribution: Attribution,
}

/// A PR's newest generation as stored.
#[derive(Debug, Clone, PartialEq)]
pub struct PullRequestGeneration {
    /// The frozen pins every execution shares.
    pub activation: PullRequestActivation,
    /// PostgreSQL time before the provider read that confirmed the source.
    pub observed_at: DateTime<Utc>,
    /// Whether the PR was closed, drafted or lost access since.
    pub deactivated: bool,
    /// Each binding's frozen execution, in binding order.
    pub executions: Vec<PullRequestExecution>,
}

impl PullRequestGeneration {
    /// Whether `source` is this generation's source: the same base branch,
    /// head commit and head repository. A moved base commit alone is not a
    /// new source.
    #[must_use]
    pub fn reviews(&self, source: &PullRequestActivation) -> bool {
        let current = &self.activation;
        current.base.ref_name == source.base.ref_name
            && current.head.sha == source.head.sha
            && current.head_repository_id == source.head_repository_id
    }
}

/// What the queue holds for one locked PR.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PullRequestHistory {
    /// The PR's latest observation, a generation or a deactivation; `None`
    /// when it has none.
    pub latest_at: Option<DateTime<Utc>>,
    /// The PR's newest generation, when it has one.
    pub current: Option<PullRequestGeneration>,
}

impl PullRequestHistory {
    /// Whether an observation begun at `observed_at` is older than what the
    /// queue already holds, so it can never become current.
    #[must_use]
    pub fn obsolete(&self, observed_at: DateTime<Utc>) -> bool {
        self.latest_at.is_some_and(|latest| observed_at <= latest)
    }
}

/// One `pull_request` binding of a Repository and the Verifier it runs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PullRequestBinding {
    /// The binding.
    pub binding_id: BindingId,
    /// Its bound Verifier, which the accepted authority must be allowed to run.
    pub verifier_card_uid: CardUid,
}

/// A binding's admission decision: the authority it runs under, or the
/// refusal its errored execution records.
#[derive(Debug, Clone, PartialEq)]
pub struct PullRequestTarget {
    /// The binding.
    pub binding_id: BindingId,
    /// The accepted owner/SYSTEM authority, or who it would have served,
    /// who recorded the refusal, and why.
    pub authority: Result<ExecutionAuthority, (Attribution, VerificationError)>,
}

/// Raw row of [`CURRENT_SQL`].
#[derive(sqlx::FromRow)]
struct GenerationRow {
    /// Execution identity.
    execution_id: Uuid,
    /// Binding.
    binding_id: Uuid,
    /// Stored status.
    status: String,
    /// Whether the row is still due at `infinity`.
    reserved: Option<bool>,
    /// Verifier.
    verifier_card_uid: Uuid,
    /// Subject.
    card_uid: Uuid,
    /// Repository owner.
    owner_card_uid: Uuid,
    /// Served subject.
    sub_principal_id: Uuid,
    /// Accepting actor.
    act_principal_id: Uuid,
    /// Frozen pins.
    pull_request: Json<PullRequestActivation>,
    /// Observation time.
    pr_observed_at: DateTime<Utc>,
    /// Deactivation time.
    pr_deactivated_at: Option<DateTime<Utc>>,
}

impl VerifierExecutionQueue {
    /// The database time a PR observation is ordered by, read before the
    /// provider's current PR state is: a later observation always reads
    /// later provider state, so [`PullRequestHistory::obsolete`] refuses an
    /// earlier one that commits after it.
    ///
    /// # Errors
    /// Returns the database error when the read fails.
    pub async fn observe_pull_request(
        &self,
        conn: &mut TenantConn<'_>,
    ) -> Result<DateTime<Utc>, SqlxError> {
        sqlx::query_scalar("SELECT statement_timestamp()")
            .fetch_one(&mut **conn.transaction())
            .await
    }

    /// Lock `key`'s PR for the rest of the caller's transaction and read its
    /// latest observation and newest generation.
    ///
    /// Every admission, deactivation and replay of the PR decides under this
    /// lock, so two deliveries of one PR never interleave.
    ///
    /// # Errors
    /// Returns the database error when a statement fails, or a decode error
    /// when a stored identity, status or pin is malformed.
    #[tracing::instrument(skip(self, conn), fields(operation = "verification.pull_request.lock"))]
    pub async fn lock_pull_request(
        &self,
        conn: &mut TenantConn<'_>,
        key: PullRequestKey<'_>,
    ) -> Result<PullRequestHistory, SqlxError> {
        timed("pull_request_lock", async {
            let number = key.number()?;
            let tenant = conn.data_tenant_id();
            sqlx::query(LOCK_PULL_REQUEST_SQL)
                .bind(PULL_REQUEST_LOCK_CLASS)
                .bind(format!(
                    "{tenant}/{}/{}/{number}",
                    key.host,
                    key.repository_id.as_str()
                ))
                .execute(&mut **conn.transaction())
                .await?;
            let latest_at: Option<DateTime<Utc>> = sqlx::query_scalar(LATEST_SQL)
                .bind(key.host)
                .bind(key.repository_id.as_str())
                .bind(number)
                .fetch_one(&mut **conn.transaction())
                .await?;
            let rows: Vec<GenerationRow> = sqlx::query_as(CURRENT_SQL)
                .bind(key.host)
                .bind(key.repository_id.as_str())
                .bind(number)
                .fetch_all(&mut **conn.transaction())
                .await?;
            let mut current: Option<PullRequestGeneration> = None;
            for row in rows {
                let execution = PullRequestExecution {
                    execution_id: stored(ExecutionId::new(row.execution_id))?,
                    binding_id: stored(BindingId::new(row.binding_id))?,
                    status: stored(row.status.parse())?,
                    reserved: row.reserved.unwrap_or(false),
                    verifier_card_uid: stored(CardUid::from_uuid(row.verifier_card_uid))?,
                    card_uid: stored(CardUid::from_uuid(row.card_uid))?,
                    owner_card_uid: stored(CardUid::from_uuid(row.owner_card_uid))?,
                    attribution: Attribution::new(
                        PrincipalId::new(row.sub_principal_id),
                        PrincipalId::new(row.act_principal_id),
                    ),
                };
                let generation = current.get_or_insert_with(|| PullRequestGeneration {
                    activation: row.pull_request.0,
                    observed_at: row.pr_observed_at,
                    deactivated: row.pr_deactivated_at.is_some(),
                    executions: Vec::new(),
                });
                generation.executions.push(execution);
            }
            Ok(PullRequestHistory { latest_at, current })
        })
        .await
    }

    /// The `pull_request` bindings `repository` owns, in binding order.
    ///
    /// # Errors
    /// Returns the database error when the read fails, or a decode error when
    /// a stored identity is malformed.
    pub async fn pull_request_bindings(
        &self,
        conn: &mut TenantConn<'_>,
        repository: &CardUid,
    ) -> Result<Vec<PullRequestBinding>, SqlxError> {
        let rows: Vec<(Uuid, Uuid)> = sqlx::query_as(BINDINGS_SQL)
            .bind(repository.as_uuid())
            .fetch_all(&mut **conn.transaction())
            .await?;
        rows.into_iter()
            .map(|(binding_id, verifier)| {
                Ok(PullRequestBinding {
                    binding_id: stored(BindingId::new(binding_id))?,
                    verifier_card_uid: stored(CardUid::from_uuid(verifier))?,
                })
            })
            .collect()
    }

    /// Cancel the locked PR's unfinished executions because it ended, and
    /// deactivate its generations as of `observed_at`. Returns the
    /// executions this call cancelled. History keeps its pins and status.
    ///
    /// # Errors
    /// Returns the database error when the update fails.
    #[tracing::instrument(
        skip(self, conn),
        fields(operation = "verification.pull_request.deactivate")
    )]
    pub async fn deactivate_pull_request(
        &self,
        conn: &mut TenantConn<'_>,
        key: PullRequestKey<'_>,
        observed_at: DateTime<Utc>,
        end: PullRequestEnd,
    ) -> Result<Vec<ExecutionId>, SqlxError> {
        timed("pull_request_deactivate", async {
            let rows: Vec<(Uuid, Option<bool>)> = sqlx::query_as(DEACTIVATE_SQL)
                .bind(key.host)
                .bind(key.repository_id.as_str())
                .bind(key.number()?)
                .bind(Json(end.error()))
                .bind(observed_at)
                .fetch_all(&mut **conn.transaction())
                .await?;
            rows.into_iter()
                .filter(|(_, cancelled)| cancelled.unwrap_or(false))
                .map(|(id, _)| stored(ExecutionId::new(id)))
                .collect()
        })
        .await
    }

    /// Cancel every unfinished PR execution of `host` whose repository is in
    /// `repositories`, or of every repository when `None`, because the
    /// tenant's connection no longer reaches it, and deactivate their
    /// generations. Runs in the access change's own transaction. Returns how
    /// many rows changed.
    ///
    /// # Errors
    /// Returns the database error when the update fails.
    #[tracing::instrument(
        skip(self, conn),
        fields(operation = "verification.pull_request.access_lost")
    )]
    pub async fn cancel_pull_request_repositories(
        &self,
        conn: &mut TenantConn<'_>,
        host: &str,
        repositories: Option<&[String]>,
    ) -> Result<u64, SqlxError> {
        timed("pull_request_access_lost", async {
            let result = sqlx::query(CANCEL_REPOSITORIES_SQL)
                .bind(host)
                .bind(repositories)
                .bind(Json(access_lost()))
                .execute(&mut **conn.transaction())
                .await?;
            Ok(result.rows_affected())
        })
        .await
    }

    /// Reserve a new generation of the locked PR for `activation` observed
    /// at `observed_at`: cancel every older unfinished execution of the PR
    /// as superseded, then freeze one execution per target.
    ///
    /// A runnable target freezes its binding's exact Verifier version,
    /// subject, Trigger and Operators with its accepted authority, reserved
    /// until [`Self::activate_pull_requests`]; a target whose binding refuses
    /// work or whose authority was refused settles `errored` at once. A
    /// binding deleted since it was listed reserves nothing. Admissions are
    /// ordered against principal revocation as every other admission is, so
    /// a revoked owner's reservation is cancelled before the commit. Returns the
    /// generation's executions in target order and the superseded ones.
    ///
    /// # Errors
    /// Returns the database error when a statement fails, or a decode error
    /// when a stored readiness is unknown.
    #[tracing::instrument(skip_all, fields(operation = "verification.pull_request.reserve", generation = %activation.generation))]
    pub async fn reserve_pull_request(
        &self,
        conn: &mut TenantConn<'_>,
        activation: &PullRequestActivation,
        observed_at: DateTime<Utc>,
        targets: &[PullRequestTarget],
    ) -> Result<(Vec<ExecutionId>, Vec<ExecutionId>), SqlxError> {
        timed("pull_request_reserve", async {
            let key = PullRequestKey {
                host: &activation.host,
                repository_id: &activation.repository_id,
                number: activation.number,
            };
            let superseded_rows: Vec<Uuid> = sqlx::query_scalar(SUPERSEDE_SQL)
                .bind(key.host)
                .bind(key.repository_id.as_str())
                .bind(key.number()?)
                .bind(Json(superseded()))
                .fetch_all(&mut **conn.transaction())
                .await?;
            let max_attempts = self.retry.max_attempts;
            let mut accepted = Vec::new();
            let mut reserved = Vec::with_capacity(targets.len());
            for target in targets {
                let execution_id = ExecutionId::new_v7();
                let refusal = match &target.authority {
                    Ok(authority) => match resolve_binding(conn, target.binding_id).await? {
                        Ok(resolved) => {
                            let inserted = sqlx::query(RESERVE_SQL)
                                .bind(execution_id.as_uuid())
                                .bind(resolved.verifier_card_uid)
                                .bind(resolved.verifier_version.as_deref().unwrap_or_default())
                                .bind(resolved.card_uid)
                                .bind(resolved.owner_card_uid)
                                .bind(target.binding_id.as_uuid())
                                .bind(resolved.trigger_uid)
                                .bind(resolved.trigger_digest.as_deref())
                                .bind(&resolved.operators)
                                .bind(authority.attribution().sub_principal_id.as_uuid())
                                .bind(authority.attribution().act_principal_id.as_uuid())
                                .bind(Json(authority.authority()))
                                .bind(max_attempts)
                                .bind(Json(activation))
                                .bind(observed_at)
                                .execute(&mut **conn.transaction())
                                .await?;
                            if inserted.rows_affected() > 0 {
                                reserved.push(execution_id);
                                accepted.push(authority.attribution());
                            }
                            continue;
                        }
                        Err(EnqueueRefusal::BindingNotFound) => continue,
                        Err(refusal) => (authority.attribution(), refusal.error()),
                    },
                    Err((attribution, error)) => (*attribution, error.clone()),
                };
                let (attribution, error) = refusal;
                let inserted = sqlx::query(REFUSE_SQL)
                    .bind(execution_id.as_uuid())
                    .bind(target.binding_id.as_uuid())
                    .bind(attribution.sub_principal_id.as_uuid())
                    .bind(attribution.act_principal_id.as_uuid())
                    .bind(Json(&error))
                    .bind(max_attempts)
                    .bind(Json(activation))
                    .bind(observed_at)
                    .execute(&mut **conn.transaction())
                    .await?;
                if inserted.rows_affected() > 0 {
                    reserved.push(execution_id);
                }
            }
            self.fence_principals(conn, accepted).await?;
            let superseded_ids = superseded_rows
                .into_iter()
                .map(|id| stored(ExecutionId::new(id)))
                .collect::<Result<_, _>>()?;
            Ok((reserved, superseded_ids))
        })
        .await
    }

    /// Admit reserved PR executions whose input records Scribe
    /// acknowledged: each still reserved becomes due at statement time.
    /// Returns the executions this call activated; one already activated,
    /// superseded or settled is left unchanged, so a replayed handoff admits
    /// nothing twice.
    ///
    /// # Errors
    /// Returns the database error when the update fails.
    pub async fn activate_pull_requests(
        &self,
        conn: &mut TenantConn<'_>,
        executions: &[ExecutionId],
    ) -> Result<Vec<ExecutionId>, SqlxError> {
        timed("pull_request_activate", async {
            let ids: Vec<Uuid> = executions.iter().map(ExecutionId::as_uuid).collect();
            let rows: Vec<Uuid> = sqlx::query_scalar(ACTIVATE_SQL)
                .bind(&ids)
                .fetch_all(&mut **conn.transaction())
                .await?;
            rows.into_iter()
                .map(|id| stored(ExecutionId::new(id)))
                .collect()
        })
        .await
    }
}
