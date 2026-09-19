//! Query slots for `platform.principals`.
//!
//! Platform-scope principals are the deployment's administrative identities.
//! They carry no tenant: the table has no tenant column, so a platform principal
//! cannot be given one. Every function here takes the BYPASSRLS
//! [`OperatorPool`], because `platform.*` sits outside the row-level-security
//! tenant boundary by construction.
//!
//! A principal is durable and outlives its credentials. Nothing in this module
//! reads, writes, or is affected by credential state.
// raw-query grep allowlist: platform administrative tables post-date the sqlx offline cache; run `mise run sqlx:prepare` to promote to macros.

use sqlx::types::Uuid;
use wyrd_spec::auth::PrincipalKindTag;

use sqlx::{Postgres, Transaction};

use crate::{OperatorPool, SqlError};

/// A platform-scope principal row.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct PlatformPrincipalRow {
    /// Stable principal id.
    pub id: Uuid,
    /// Principal kind label; always `global_admin` in the current type set.
    pub principal_kind: String,
    /// Operator-facing name, unique across the deployment.
    pub name: String,
    /// Lifecycle status: `active`, `suspended`, or `deleted`.
    pub status: String,
}

impl PlatformPrincipalRow {
    /// True when this principal may authenticate.
    ///
    /// A suspended or deleted principal keeps its grants and credentials but
    /// cannot establish an authenticated context.
    #[must_use]
    pub fn is_active(&self) -> bool {
        self.status == "active"
    }
}

/// Insert a platform-scope principal inside a caller-owned transaction.
///
/// Creating a principal is rarely the whole operation: it is usually followed
/// by granting its authority and issuing its first credential, and a principal
/// that exists with neither is useless *and*, because `name` is unique,
/// permanently blocks another attempt at the same name. So the transaction is
/// the caller's to commit, and every one of those statements lands or none do.
///
/// # Errors
/// Returns [`SqlError::UniqueViolation`] when `name` is already taken, and
/// [`SqlError::Query`] when the insert otherwise fails, including when `kind`
/// is not a platform-scope kind.
pub async fn insert_platform_principal_tx(
    tx: &mut Transaction<'_, Postgres>,
    id: Uuid,
    kind: PrincipalKindTag,
    name: &str,
) -> Result<(), SqlError> {
    sqlx::query(INSERT_PLATFORM_PRINCIPAL_SQL)
        .bind(id)
        .bind(kind.as_str())
        .bind(name)
        .execute(&mut **tx)
        .await
        .map_err(SqlError::from)?;
    Ok(())
}

/// Insert a platform-scope principal on its own.
///
/// For the case where the principal genuinely is the whole operation: a human
/// platform administrator is pre-registered with no credential at all, because
/// they will authenticate by federated identity. Where a principal is only
/// useful alongside a grant or a credential, use
/// [`insert_platform_principal_tx`] so a partial write cannot leave a name
/// taken by something unusable.
///
/// # Errors
/// Returns [`SqlError::UniqueViolation`] when `name` is already taken, and
/// [`SqlError::Query`] when the insert otherwise fails.
pub async fn insert_platform_principal(
    pool: &OperatorPool,
    id: Uuid,
    kind: PrincipalKindTag,
    name: &str,
) -> Result<(), SqlError> {
    sqlx::query(INSERT_PLATFORM_PRINCIPAL_SQL)
        .bind(id)
        .bind(kind.as_str())
        .bind(name)
        .execute(pool.pool())
        .await
        .map_err(SqlError::from)?;
    Ok(())
}

/// The one principal insert, shared by both entry points so they cannot drift.
const INSERT_PLATFORM_PRINCIPAL_SQL: &str =
    "INSERT INTO platform.principals (id, principal_kind, name, status)
     VALUES ($1, $2, $3, 'active')";

/// Read one platform principal by id.
///
/// # Errors
/// Returns [`SqlError::Query`] when the read fails.
pub async fn platform_principal_by_id(
    pool: &OperatorPool,
    id: Uuid,
) -> Result<Option<PlatformPrincipalRow>, SqlError> {
    sqlx::query_as::<_, PlatformPrincipalRow>(
        "SELECT id, principal_kind, name, status
           FROM platform.principals
          WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(pool.pool())
    .await
    .map_err(SqlError::from)
}

/// A platform principal with its federated identity, when it has one.
///
/// The identity is what makes a listing actionable: an operator auditing who
/// can administer the deployment needs to see the issuer and subject a
/// principal resolves from, not just its name.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct PlatformPrincipalListRow {
    /// Stable principal id.
    pub id: Uuid,
    /// Principal kind label.
    pub principal_kind: String,
    /// Operator-facing name.
    pub name: String,
    /// Lifecycle status.
    pub status: String,
    /// Claim this principal was registered against, for a human principal.
    pub match_claim: Option<String>,
    /// Subject pinned at first login, once one has happened.
    pub subject: Option<String>,
}

/// List every platform principal, newest first.
///
/// Includes suspended and deleted principals deliberately: an operator
/// reviewing who holds platform authority needs to see that a revoked
/// administrator really is revoked, which a listing of only live rows could not
/// tell them.
///
/// # Errors
/// Returns [`SqlError::Query`] when the read fails.
pub async fn list_platform_principals(
    pool: &OperatorPool,
) -> Result<Vec<PlatformPrincipalListRow>, SqlError> {
    sqlx::query_as::<_, PlatformPrincipalListRow>(
        "SELECT p.id, p.principal_kind, p.name, p.status,
                i.match_claim, i.subject
           FROM platform.principals p
           LEFT JOIN platform.principal_identities i ON i.principal_id = p.id
          ORDER BY p.id DESC",
    )
    .fetch_all(pool.pool())
    .await
    .map_err(SqlError::from)
}

/// Outcome of a guarded platform principal status change.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatusChange {
    /// The principal's status is now the requested one.
    Changed,
    /// The principal already held the requested status; nothing was written.
    Unchanged,
    /// No platform principal has that id.
    NotFound,
    /// The change would leave the deployment with no way in, and was refused.
    WouldStrandDeployment,
}

/// Lock word serializing every guarded platform status change.
///
/// Suspension is decided by counting the administrators that would remain, so
/// two concurrent suspensions must not each see the other's subject still
/// active. A transaction-scoped advisory lock is the narrowest thing that makes
/// them take turns: it needs no table, is released by commit or rollback alike,
/// and touches nothing else in the deployment.
const PLATFORM_STATUS_LOCK: i64 = 0x7779_7264_7073_7461;

/// Set a platform principal's lifecycle status without stranding the deployment.
///
/// Suspension is what makes the active-status check on every platform request a
/// live guard rather than a dormant one: nothing else in the deployment can
/// stop a platform principal from acting. That is also why it is dangerous —
/// suspending the last administrator who can actually get back in leaves no way
/// to undo it.
///
/// The guard and the write are one serialized transaction. `required` is the
/// fixed platform-administrator grant the caller owns; a remaining principal
/// counts only if it is active, holds that grant, and has some way to
/// authenticate — a live credential or a pinned federated identity. A principal
/// with a grant and no way in cannot justify suspending the one that has both.
///
/// Restoring a principal can only raise that count, so it is never guarded.
///
/// # Errors
/// Returns [`SqlError::Query`] when the lock, the count, or the update fails,
/// including when `status` is not an accepted lifecycle value.
pub async fn set_platform_principal_status(
    pool: &OperatorPool,
    id: Uuid,
    status: &str,
    required: &serde_json::Value,
) -> Result<StatusChange, SqlError> {
    let mut tx = pool.begin().await?;

    sqlx::query("SELECT pg_advisory_xact_lock($1)")
        .bind(PLATFORM_STATUS_LOCK)
        .execute(&mut *tx)
        .await
        .map_err(SqlError::from)?;

    let Some(current): Option<String> =
        sqlx::query_scalar::<_, String>("SELECT status FROM platform.principals WHERE id = $1")
            .bind(id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(SqlError::from)?
    else {
        return Ok(StatusChange::NotFound);
    };
    if current == status {
        return Ok(StatusChange::Unchanged);
    }

    if status == "suspended" {
        let (subject_usable, remaining): (bool, i64) = sqlx::query_as(USABLE_ADMINISTRATORS_SQL)
            .bind(id)
            .bind(required)
            .fetch_one(&mut *tx)
            .await
            .map_err(SqlError::from)?;
        // Only a principal that is itself a way in can be the last one. An
        // ungranted or unauthenticable row is not protection worth refusing for.
        if subject_usable && remaining == 0 {
            return Ok(StatusChange::WouldStrandDeployment);
        }
    }

    sqlx::query("UPDATE platform.principals SET status = $2, updated_at = now() WHERE id = $1")
        .bind(id)
        .bind(status)
        .execute(&mut *tx)
        .await
        .map_err(SqlError::from)?;
    tx.commit().await?;
    Ok(StatusChange::Changed)
}

/// Report whether `$1` is a way into the deployment, and how many others are.
///
/// "A way in" is the whole point: an active row is not one if it holds no
/// platform authority, and authority is not one if nothing can authenticate as
/// it. `@>` is grant containment, so a principal counts only when it holds
/// every permission in the required set.
const USABLE_ADMINISTRATORS_SQL: &str = "WITH usable AS (
        SELECT p.id
          FROM platform.principals p
          JOIN platform.principal_grants g ON g.principal_id = p.id
         WHERE p.status = 'active'
           AND g.permissions @> $2
           AND (
          EXISTS (
            SELECT 1 FROM platform.credentials c
             WHERE c.principal_id = p.id
               AND c.revoked_at IS NULL
               AND (c.expires_at IS NULL OR c.expires_at > now())
          )
          OR EXISTS (
            SELECT 1 FROM platform.principal_identities i WHERE i.principal_id = p.id
          )
        )
      )
      SELECT EXISTS (SELECT 1 FROM usable WHERE id = $1),
             (SELECT count(*) FROM usable WHERE id <> $1)";
