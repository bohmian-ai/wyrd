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
