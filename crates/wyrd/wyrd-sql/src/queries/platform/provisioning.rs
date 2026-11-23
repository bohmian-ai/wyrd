// raw-query grep allowlist: platform administrative tables post-date the sqlx offline cache; run `mise run sqlx:prepare` to promote to macros.
//! Tenant provisioning writes against the tenant directory.
//!
//! Provisioning spans two boundaries: the directory row lives at platform scope
//! behind the operator role, while the tenant's own principal, roles, and
//! credential live under row-level security inside the tenant. One transaction
//! cannot cover both, so the directory row is created in a non-usable state
//! first and only promoted once the tenant-scoped work has committed. A failure
//! anywhere in between leaves a tenant that is visibly incomplete rather than
//! one that looks live but has no way in.

use wyrd_spec::DataTenantId;

use crate::{OperatorPool, SqlError, TenantConn};

/// Create a tenant directory row in the provisioning state.
///
/// Takes the caller's transaction so the row is created in the same transaction
/// that recorded the authorization decision permitting it. The tenant is not
/// usable on commit: no consumer that filters on live tenants will see it.
///
/// # Errors
/// Returns [`SqlError::Query`] when the insert fails, including when `slug` is
/// already taken — which is how a concurrent creation for the same tenant
/// converges on one row rather than two.
pub async fn insert_provisioning_tenant(
    conn: &mut TenantConn<'_>,
    data_tenant_id: DataTenantId,
    slug: &str,
    display_name: &str,
) -> Result<TenantClaim, SqlError> {
    // An interrupted provisioning is resumable, so its slug is adopted rather
    // than refused. Without this a single failure would burn the slug
    // permanently: the row exists, the insert conflicts, and every retry of the
    // identical request reports the slug taken forever.
    //
    // Two kinds of row are adopted. A `failed` row said so on its way out. A
    // `provisioning` row that has not moved for fifteen minutes never got the
    // chance — it was cancelled or its process died — and is the more common
    // case, because that is exactly what an interrupted attempt leaves behind.
    // The window is generous so a slow but live attempt is never stolen from;
    // provisioning is a handful of writes and takes nothing like that long. An
    // `active` or `suspended` tenant, and a `provisioning` row that is still
    // moving, are genuine collisions and still conflict. So is any claim a
    // public signup made: only its own attempt may resume it, through
    // `claim_signup_tenant`, and its creator key marks it after the attempt
    // is purged.
    let adopted = sqlx::query_scalar::<_, uuid::Uuid>(
        "INSERT INTO platform.tenants (data_tenant_id, slug, display_name, status)
         VALUES ($1, $2, $3, 'provisioning')
         ON CONFLICT (slug) DO UPDATE
            SET data_tenant_id            = platform.tenants.data_tenant_id,
                display_name              = EXCLUDED.display_name,
                status                    = 'provisioning',
                provisioning_failed_reason = NULL,
                updated_at                = now()
          WHERE platform.tenants.signup_creator_key IS NULL
            AND (platform.tenants.status = 'failed'
                 OR (platform.tenants.status = 'provisioning'
                     AND platform.tenants.updated_at < now() - interval '15 minutes'))
         RETURNING data_tenant_id",
    )
    .bind(data_tenant_id.as_uuid())
    .bind(slug)
    .bind(display_name)
    .fetch_optional(&mut **conn.transaction())
    .await
    .map_err(SqlError::from)?;

    // `ON CONFLICT ... WHERE` that matches nothing returns no row rather than
    // raising, so an occupied slug is this absence, not an error.
    let Some(claimed) = adopted else {
        return Err(SqlError::UniqueViolation {
            constraint: "tenants_slug_key".to_owned(),
        });
    };

    Ok(if claimed == data_tenant_id.as_uuid() {
        TenantClaim::Created
    } else {
        TenantClaim::Resumed(
            DataTenantId::try_from(claimed).map_err(SqlError::InvalidDataTenantId)?,
        )
    })
}

/// Claim `slug` for the public signup `attempt` by the identity whose keyed
/// pseudonym is `creator`, or resume that attempt's own interrupted claim of
/// it. The creator key stays on the tenant after the attempt is purged.
///
/// Unlike [`insert_provisioning_tenant`], only a `failed` or stale
/// `provisioning` row this same attempt created is adopted, so no other
/// visitor, signup or operator claim can be taken over. A signup claim goes
/// stale after two minutes without progress, inside its proof's five-minute
/// completion window.
///
/// A new claim is refused, returning `None`, when the creator already owns
/// the deployment's owned-workspace cap of unretired signup tenants; failed
/// and provisioning claims count, and resuming this attempt's own claim is
/// not a new one. Claims by one creator are serialized on a transaction
/// lock, so concurrent completions on any replica cannot exceed the cap.
///
/// # Errors
/// Returns [`SqlError::UniqueViolation`] when the slug belongs to another
/// tenant or claim, and [`SqlError::Query`] when the insert fails.
pub async fn claim_signup_tenant(
    pool: &OperatorPool,
    data_tenant_id: DataTenantId,
    slug: &str,
    display_name: &str,
    attempt: uuid::Uuid,
    creator: &[u8; 32],
) -> Result<Option<TenantClaim>, SqlError> {
    let mut tx = pool.pool().begin().await.map_err(SqlError::from)?;
    sqlx::query(
        "SELECT pg_advisory_xact_lock(
                    hashtextextended('wyrd.signup_creator:' || encode($1, 'hex'), 0))",
    )
    .bind(creator.as_slice())
    .execute(&mut *tx)
    .await
    .map_err(SqlError::from)?;
    let admitted: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM platform.tenants WHERE signup_attempt_id = $2)
                OR (SELECT count(*) FROM platform.tenants
                     WHERE signup_creator_key = $1 AND status <> 'deleted')
                   < (SELECT signup_workspace_cap FROM platform.login_settings)",
    )
    .bind(creator.as_slice())
    .bind(attempt)
    .fetch_one(&mut *tx)
    .await
    .map_err(SqlError::from)?;
    if !admitted {
        return Ok(None);
    }
    let adopted = sqlx::query_scalar::<_, uuid::Uuid>(
        "INSERT INTO platform.tenants
             (data_tenant_id, slug, display_name, status, signup_attempt_id,
              signup_creator_key)
         VALUES ($1, $2, $3, 'provisioning', $4, $5)
         ON CONFLICT (slug) DO UPDATE
            SET display_name               = EXCLUDED.display_name,
                status                     = 'provisioning',
                provisioning_failed_reason = NULL,
                updated_at                 = now()
          WHERE platform.tenants.signup_attempt_id = EXCLUDED.signup_attempt_id
            AND (platform.tenants.status = 'failed'
                 OR (platform.tenants.status = 'provisioning'
                     AND platform.tenants.updated_at < now() - interval '2 minutes'))
         RETURNING data_tenant_id",
    )
    .bind(data_tenant_id.as_uuid())
    .bind(slug)
    .bind(display_name)
    .bind(attempt)
    .bind(creator.as_slice())
    .fetch_optional(&mut *tx)
    .await
    .map_err(SqlError::from)?;
    let Some(claimed) = adopted else {
        return Err(SqlError::UniqueViolation {
            constraint: "tenants_slug_key".to_owned(),
        });
    };
    tx.commit().await.map_err(SqlError::from)?;
    Ok(Some(if claimed == data_tenant_id.as_uuid() {
        TenantClaim::Created
    } else {
        TenantClaim::Resumed(
            DataTenantId::try_from(claimed).map_err(SqlError::InvalidDataTenantId)?,
        )
    }))
}

/// Which tenant row a provisioning attempt ended up owning.
///
/// Resuming keeps the original tenant id, because a failed attempt may already
/// have written tenant-scoped rows under it. Handing back a second id would
/// orphan them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TenantClaim {
    /// A new tenant row was created under the caller's proposed id.
    Created,
    /// An interrupted tenant was adopted; work continues under its id.
    Resumed(DataTenantId),
}

impl TenantClaim {
    /// The tenant id this attempt must actually provision under.
    #[must_use]
    pub fn tenant_id(self, proposed: DataTenantId) -> DataTenantId {
        match self {
            Self::Created => proposed,
            Self::Resumed(existing) => existing,
        }
    }
}

/// Promote a provisioning tenant to active.
///
/// Only a tenant still in the provisioning state is promoted, so a retry that
/// races a completed attempt promotes nothing rather than resurrecting a
/// suspended or failed tenant. Returns whether this call performed the
/// promotion.
///
/// # Errors
/// Returns [`SqlError::Query`] when the update fails.
pub async fn mark_tenant_active(
    pool: &OperatorPool,
    data_tenant_id: DataTenantId,
) -> Result<bool, SqlError> {
    let result = sqlx::query(
        "UPDATE platform.tenants
            SET status = 'active', updated_at = now()
          WHERE data_tenant_id = $1 AND status = 'provisioning'",
    )
    .bind(data_tenant_id.as_uuid())
    .execute(pool.pool())
    .await
    .map_err(SqlError::from)?;
    Ok(result.rows_affected() == 1)
}

/// Record that provisioning did not complete.
///
/// Leaves the tenant visibly incomplete with the reason attached, so an
/// operator can see why it never became usable and a retry knows it is resuming
/// rather than starting fresh.
///
/// # Errors
/// Returns [`SqlError::Query`] when the update fails.
pub async fn mark_tenant_failed(
    pool: &OperatorPool,
    data_tenant_id: DataTenantId,
    reason: &str,
) -> Result<(), SqlError> {
    sqlx::query(
        "UPDATE platform.tenants
            SET status = 'failed', provisioning_failed_reason = $2, updated_at = now()
          WHERE data_tenant_id = $1 AND status = 'provisioning'",
    )
    .bind(data_tenant_id.as_uuid())
    .bind(reason)
    .execute(pool.pool())
    .await
    .map_err(SqlError::from)?;
    Ok(())
}

/// Set a tenant's suspension state without destroying it.
///
/// Suspension freezes a tenant: its principals stop authenticating while every
/// row, grant, and credential survives, so resuming restores exactly what was
/// there. Returns whether the transition applied.
///
/// # Errors
/// Returns [`SqlError::Query`] when the update fails.
pub async fn set_tenant_suspended(
    conn: &mut TenantConn<'_>,
    data_tenant_id: DataTenantId,
    suspended: bool,
) -> Result<bool, SqlError> {
    let (next, expected) = if suspended {
        ("suspended", "active")
    } else {
        ("active", "suspended")
    };
    let result = sqlx::query(
        "UPDATE platform.tenants
            SET status = $2, updated_at = now()
          WHERE data_tenant_id = $1 AND status = $3",
    )
    .bind(data_tenant_id.as_uuid())
    .bind(next)
    .bind(expected)
    .execute(&mut **conn.transaction())
    .await
    .map_err(SqlError::from)?;
    Ok(result.rows_affected() == 1)
}
