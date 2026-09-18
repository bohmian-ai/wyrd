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

use sqlx::types::Uuid;
use wyrd_spec::auth::PrincipalKindTag;

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

/// Insert a platform-scope principal.
///
/// The caller supplies `id` so the principal can be referenced inside the same
/// transaction that grants its authority and issues its first credential.
///
/// # Errors
/// Returns [`SqlError::Query`] when the insert fails, including when `name` is
/// already taken or `kind` is not a platform-scope kind.
pub async fn insert_platform_principal(
    pool: &OperatorPool,
    id: Uuid,
    kind: PrincipalKindTag,
    name: &str,
) -> Result<(), SqlError> {
    sqlx::query(
        "INSERT INTO platform.principals (id, principal_kind, name, status)
         VALUES ($1, $2, $3, 'active')",
    )
    .bind(id)
    .bind(kind.as_str())
    .bind(name)
    .execute(pool.pool())
    .await
    .map_err(SqlError::from)?;
    Ok(())
}

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

/// Count the platform principals of one kind.
///
/// Initialization uses this to observe whether a deployment already has its
/// administrative root. It is a read, not a guard: the single-initialization
/// invariant is enforced durably, never by checking this first.
///
/// # Errors
/// Returns [`SqlError::Query`] when the read fails.
pub async fn count_platform_principals(
    pool: &OperatorPool,
    kind: PrincipalKindTag,
) -> Result<i64, SqlError> {
    sqlx::query_scalar::<_, i64>(
        "SELECT count(*) FROM platform.principals WHERE principal_kind = $1",
    )
    .bind(kind.as_str())
    .fetch_one(pool.pool())
    .await
    .map_err(SqlError::from)
}
