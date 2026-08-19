//! Background allocation and renewal of role-fenced Oracle admission blocks.

use chrono::Duration;
use sqlx::{Postgres, Transaction};
use uuid::Uuid;
use wyrd_spec::vala::api::{OracleAdmissionDemand, QueryClass};

use crate::row_types::oracle_admission::OracleAdmissionBlockRow;
use crate::{OperatorPool, SqlError};

/// Durable allocator for canonical policy ceilings and delegated capacity.
pub struct OracleAdmissionBlocks<'a> {
    /// Cross-tenant operator connection used only by background work.
    pool: &'a OperatorPool,
}

impl<'a> OracleAdmissionBlocks<'a> {
    /// Creates a background allocator over the platform operator pool.
    #[must_use]
    pub const fn new(pool: &'a OperatorPool) -> Self {
        Self { pool }
    }

    /// Inserts one canonical ceiling or confirms the existing value is equal.
    ///
    /// # Errors
    ///
    /// Returns [`SqlError`] when the configured capacity is zero, conflicts
    /// with durable policy, or PostgreSQL cannot complete the transaction.
    pub async fn ensure_policy(
        &self,
        tenant_id: Option<Uuid>,
        query_class: QueryClass,
        capacity: u32,
    ) -> Result<(), SqlError> {
        let capacity = i32::try_from(capacity).map_err(|_| invariant("capacity exceeds i32"))?;
        if capacity == 0 {
            return Err(invariant("Oracle admission capacity must be positive"));
        }
        let scope = if tenant_id.is_some() {
            "tenant"
        } else {
            "global"
        };
        let row: i32 = sqlx::query_scalar(
            "INSERT INTO vala.oracle_admission_policies(scope_kind,data_tenant_id,query_class,capacity) \
             VALUES($1,$2,$3,$4) ON CONFLICT (scope_kind,data_tenant_id,query_class) \
             DO UPDATE SET capacity=vala.oracle_admission_policies.capacity RETURNING capacity",
        )
        .bind(scope)
        .bind(tenant_id)
        .bind(class_name(query_class))
        .bind(capacity)
        .fetch_one(self.pool.pool())
        .await
        .map_err(SqlError::from)?;
        if row != capacity {
            return Err(invariant(
                "Oracle admission policy conflicts with durable ceiling",
            ));
        }
        Ok(())
    }

    /// Allocates one co-located three-scope block using database time.
    ///
    /// The transaction locks global then tenant policy rows, subtracts every
    /// not-yet-reusable allocation, and inserts equal global, tenant, and
    /// principal rows for the exact requested amount bounded by availability.
    ///
    /// # Errors
    ///
    /// Returns [`SqlError`] for invalid demand, missing/conflicting policy, or
    /// any PostgreSQL failure. Zero available capacity returns an empty vector.
    pub async fn allocate(
        &self,
        demand: OracleAdmissionDemand,
        validity: Duration,
    ) -> Result<Vec<OracleAdmissionBlockRow>, SqlError> {
        if demand.requested_units == 0
            || demand.holder_fencing_token == 0
            || validity <= Duration::zero()
        {
            return Err(invariant("invalid Oracle admission allocation request"));
        }
        let requested = i32::try_from(demand.requested_units)
            .map_err(|_| invariant("requested units exceed i32"))?;
        let holder_fence = i64::try_from(demand.holder_fencing_token)
            .map_err(|_| invariant("holder fence exceeds i64"))?;
        let mut tx = self.pool.pool().begin().await.map_err(SqlError::from)?;
        let holder_live: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM vala.cluster_nodes WHERE data_tenant_id=$1 \
             AND node_id=$2 AND role='oracle' AND fencing_token=$3 AND ready=true \
             AND heartbeat_at>=statement_timestamp()-interval '15 seconds')",
        )
        .bind(Uuid::from(demand.tenant_id))
        .bind(demand.holder_node_id.as_uuid())
        .bind(holder_fence)
        .fetch_one(&mut *tx)
        .await
        .map_err(SqlError::from)?;
        if !holder_live {
            return Err(invariant("Oracle admission holder incarnation is not live"));
        }
        let global = lock_policy(&mut tx, None, demand.query_class).await?;
        let tenant =
            lock_policy(&mut tx, Some(demand.tenant_id.into()), demand.query_class).await?;
        let global_used = live_units(&mut tx, "global", None, demand.query_class).await?;
        let tenant_used = live_units(
            &mut tx,
            "tenant",
            Some(demand.tenant_id.into()),
            demand.query_class,
        )
        .await?;
        let units = requested
            .min(global - global_used)
            .min(tenant - tenant_used)
            .max(0);
        if units == 0 {
            tx.commit().await.map_err(SqlError::from)?;
            return Ok(Vec::new());
        }
        let allocation_id = Uuid::now_v7();
        let rows = sqlx::query_as::<_, OracleAdmissionBlockRow>(
            r#"WITH clock AS MATERIALIZED (SELECT statement_timestamp() AS now),
            scopes(scope_kind, principal_id) AS (
              VALUES ('global'::text, NULL::uuid), ('tenant', NULL), ('principal', $2::uuid)
            )
            INSERT INTO vala.oracle_admission_blocks
              (block_id, allocation_id, scope_kind, data_tenant_id, principal_id, query_class,
               units, holder_node_id, holder_fencing_token, valid_from, expires_at)
            SELECT gen_random_uuid(), $1, scope_kind, $3, principal_id, $4, $5, $6, $7,
                   clock.now, clock.now + ($8 * interval '1 millisecond')
            FROM scopes CROSS JOIN clock
            RETURNING block_id, allocation_id, scope_kind, data_tenant_id, principal_id,
                      query_class, units, holder_node_id, holder_fencing_token,
                      valid_from, expires_at, closed_at"#,
        )
        .bind(allocation_id)
        .bind(demand.principal_id.as_uuid())
        .bind(Uuid::from(demand.tenant_id))
        .bind(class_name(demand.query_class))
        .bind(units)
        .bind(demand.holder_node_id.as_uuid())
        .bind(holder_fence)
        .bind(validity.num_milliseconds())
        .fetch_all(&mut *tx)
        .await
        .map_err(SqlError::from)?;
        tx.commit().await.map_err(SqlError::from)?;
        Ok(rows)
    }

    /// Renews every row of a live allocation without permitting a late revival.
    ///
    /// # Errors
    ///
    /// Returns [`SqlError`] when PostgreSQL cannot perform the fenced update.
    pub async fn renew(
        &self,
        allocation_id: Uuid,
        holder_node_id: Uuid,
        holder_fencing_token: u64,
        validity: Duration,
    ) -> Result<bool, SqlError> {
        let fence = i64::try_from(holder_fencing_token)
            .map_err(|_| invariant("holder fence exceeds i64"))?;
        let result = sqlx::query(
            "UPDATE vala.oracle_admission_blocks SET expires_at=statement_timestamp()+($4*interval '1 millisecond') \
             WHERE allocation_id=$1 AND holder_node_id=$2 AND holder_fencing_token=$3 \
             AND closed_at IS NULL AND expires_at>statement_timestamp()",
        )
        .bind(allocation_id)
        .bind(holder_node_id)
        .bind(fence)
        .bind(validity.num_milliseconds())
        .execute(self.pool.pool())
        .await
        .map_err(SqlError::from)?;
        Ok(result.rows_affected() == 3)
    }

    /// Closes all blocks for one exact holder incarnation at database time.
    ///
    /// # Errors
    ///
    /// Returns [`SqlError`] when PostgreSQL cannot persist closure.
    pub async fn close_holder(&self, node_id: Uuid, fence: u64) -> Result<u64, SqlError> {
        let fence = i64::try_from(fence).map_err(|_| invariant("holder fence exceeds i64"))?;
        let result = sqlx::query(
            "UPDATE vala.oracle_admission_blocks SET closed_at=COALESCE(closed_at,statement_timestamp()) \
             WHERE holder_node_id=$1 AND holder_fencing_token=$2 AND closed_at IS NULL",
        )
        .bind(node_id)
        .bind(fence)
        .execute(self.pool.pool())
        .await
        .map_err(SqlError::from)?;
        Ok(result.rows_affected())
    }
}

/// Locks and returns one canonical policy row in deterministic caller order.
///
/// # Errors
///
/// Returns [`SqlError`] when the policy is missing or PostgreSQL cannot lock it.
async fn lock_policy(
    tx: &mut Transaction<'_, Postgres>,
    tenant_id: Option<Uuid>,
    query_class: QueryClass,
) -> Result<i32, SqlError> {
    sqlx::query_scalar(
        "SELECT capacity FROM vala.oracle_admission_policies \
         WHERE scope_kind=$1 AND data_tenant_id IS NOT DISTINCT FROM $2 AND query_class=$3 FOR UPDATE",
    )
    .bind(if tenant_id.is_some() { "tenant" } else { "global" })
    .bind(tenant_id)
    .bind(class_name(query_class))
    .fetch_optional(&mut **tx)
    .await
    .map_err(SqlError::from)?
    .ok_or_else(|| invariant("Oracle admission policy is not configured"))
}

/// Sums capacity that database time has not yet made reusable.
///
/// # Errors
///
/// Returns [`SqlError`] when PostgreSQL cannot evaluate capacity and membership.
async fn live_units(
    tx: &mut Transaction<'_, Postgres>,
    scope: &str,
    tenant_id: Option<Uuid>,
    query_class: QueryClass,
) -> Result<i32, SqlError> {
    sqlx::query_scalar(
        "SELECT COALESCE(sum(blocks.units),0)::integer FROM vala.oracle_admission_blocks blocks \
         WHERE blocks.scope_kind=$1 \
         AND blocks.data_tenant_id IS NOT DISTINCT FROM COALESCE($2,blocks.data_tenant_id) \
         AND blocks.query_class=$3 AND (blocks.expires_at>statement_timestamp() OR EXISTS ( \
           SELECT 1 FROM vala.cluster_nodes roles \
           WHERE roles.data_tenant_id=blocks.data_tenant_id \
           AND roles.node_id=blocks.holder_node_id AND roles.role='oracle' \
           AND roles.fencing_token=blocks.holder_fencing_token AND roles.ready=true \
           AND roles.heartbeat_at>=statement_timestamp()-interval '15 seconds'))",
    )
    .bind(scope)
    .bind(tenant_id)
    .bind(class_name(query_class))
    .fetch_one(&mut **tx)
    .await
    .map_err(SqlError::from)
}

/// Returns the durable discriminator for a closed query class.
const fn class_name(class: QueryClass) -> &'static str {
    match class {
        QueryClass::Interactive => "interactive",
        QueryClass::Analytical => "analytical",
    }
}

/// Builds one invariant error without duplicating catalog details.
fn invariant(detail: &str) -> SqlError {
    SqlError::InvariantViolation {
        detail: detail.to_owned(),
    }
}
