//! Background allocation and renewal of role-fenced Oracle admission blocks.
//!
//! The delegated capacity ledger is cross-tenant by construction: one fenced
//! renewal reconciles the global, tenant, and principal accounting rows for the
//! whole node before any tenant request is admitted, so every statement runs
//! through [`OperatorPool`] inside a transaction this module opens and closes
//! itself. No raw pool or transaction leaves the owner.
// raw-query grep allowlist: the fixed statements target admission tables that post-date the sqlx offline cache; run `mise run sqlx:prepare` to promote them to macros.

use chrono::Duration;
use sqlx::{Postgres, Transaction};
use uuid::Uuid;
use wyrd_spec::DataTenantId;
use wyrd_spec::vala::api::{OracleAdmissionDemand, QueryClass};

use crate::row_types::oracle_admission::OracleAdmissionBlockRow;
use crate::{OperatorPool, SqlError};

/// Authoritative database-time evidence for one complete fenced renewal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OracleAdmissionRenewal {
    /// The three renewed global, tenant, and principal rows.
    pub rows: Vec<OracleAdmissionBlockRow>,
    /// Database statement time used to validate and advance their expiry.
    pub database_now: chrono::DateTime<chrono::Utc>,
}

/// Authoritative database-time evidence for one complete fenced allocation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OracleAdmissionAllocation {
    /// The three allocated global, tenant, and principal rows, or none when full.
    pub rows: Vec<OracleAdmissionBlockRow>,
    /// Database statement time used to establish the rows' validity interval.
    pub database_now: chrono::DateTime<chrono::Utc>,
}

/// Internal projection that carries a shared database clock beside each row.
#[derive(sqlx::FromRow)]
struct OracleAdmissionTimedBlockRow {
    /// Unique row identity.
    block_id: Uuid,
    /// Identity shared by the three accounting rows.
    allocation_id: Uuid,
    /// Durable accounting-level discriminator.
    scope_kind: String,
    /// Tenant charged by the allocation.
    data_tenant_id: Uuid,
    /// Principal charged only by the principal row.
    principal_id: Option<Uuid>,
    /// Durable query-class discriminator.
    query_class: String,
    /// Units delegated at every accounting level.
    units: i32,
    /// Physical holder node.
    holder_node_id: Uuid,
    /// Exact Oracle role-incarnation fence.
    holder_fencing_token: i64,
    /// Database-time beginning of uninterrupted validity.
    valid_from: chrono::DateTime<chrono::Utc>,
    /// Renewed absolute database-time validity bound.
    expires_at: chrono::DateTime<chrono::Utc>,
    /// Database-time closure marker after continuity loss.
    closed_at: Option<chrono::DateTime<chrono::Utc>>,
    /// Shared statement timestamp used by the fenced mutation.
    database_now: chrono::DateTime<chrono::Utc>,
}

impl OracleAdmissionTimedBlockRow {
    /// Splits one internal database projection into its durable row and clock.
    fn into_parts(self) -> (OracleAdmissionBlockRow, chrono::DateTime<chrono::Utc>) {
        (
            OracleAdmissionBlockRow {
                block_id: self.block_id,
                allocation_id: self.allocation_id,
                scope_kind: self.scope_kind,
                data_tenant_id: self.data_tenant_id,
                principal_id: self.principal_id,
                query_class: self.query_class,
                units: self.units,
                holder_node_id: self.holder_node_id,
                holder_fencing_token: self.holder_fencing_token,
                valid_from: self.valid_from,
                expires_at: self.expires_at,
                closed_at: self.closed_at,
            },
            self.database_now,
        )
    }
}

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

    /// Initializes and validates the four canonical admission ceilings.
    ///
    /// Startup writes one global and one tenant-default row per query class.
    /// It never discovers tenants or creates tenant-specific configuration.
    /// Existing equal rows make repeated pod startup idempotent; any conflict
    /// rolls back the complete canonical set.
    ///
    /// # Errors
    ///
    /// Returns [`SqlError`] when a capacity is invalid, an existing canonical
    /// row conflicts, unexpected policy state exists, or the transaction cannot
    /// commit.
    pub async fn ensure_canonical_policies(
        &self,
        global_interactive: u32,
        global_analytical: u32,
        tenant_interactive: u32,
        tenant_analytical: u32,
    ) -> Result<(), SqlError> {
        let capacities = [
            ("global", QueryClass::Interactive, global_interactive),
            ("global", QueryClass::Analytical, global_analytical),
            (
                "tenant_default",
                QueryClass::Interactive,
                tenant_interactive,
            ),
            ("tenant_default", QueryClass::Analytical, tenant_analytical),
        ];
        for (_, _, capacity) in capacities {
            validate_capacity(capacity)?;
        }
        let mut tx = self.pool.pool().begin().await.map_err(SqlError::from)?;
        for (scope, query_class, capacity) in capacities {
            ensure_policy_in(&mut tx, scope, query_class, capacity).await?;
        }
        let policy_count: i64 =
            sqlx::query_scalar("SELECT count(*) FROM vala.oracle_admission_policies")
                .fetch_one(&mut *tx)
                .await
                .map_err(SqlError::from)?;
        if policy_count != 4 {
            return Err(invariant(
                "Oracle admission policy set contains non-canonical rows",
            ));
        }
        tx.commit().await.map_err(SqlError::from)
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
    /// any PostgreSQL failure. Zero available capacity returns authoritative
    /// statement-time evidence with no rows.
    pub async fn allocate(
        &self,
        demand: OracleAdmissionDemand,
        validity: Duration,
    ) -> Result<OracleAdmissionAllocation, SqlError> {
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
        .bind(DataTenantId::SYSTEM_OWNER.as_uuid())
        .bind(demand.holder_node_id.as_uuid())
        .bind(holder_fence)
        .fetch_one(&mut *tx)
        .await
        .map_err(SqlError::from)?;
        if !holder_live {
            return Err(invariant("Oracle admission holder incarnation is not live"));
        }
        let global = lock_policy(&mut tx, "global", demand.query_class).await?;
        let tenant = lock_policy(&mut tx, "tenant_default", demand.query_class).await?;
        if !lock_active_tenant(&mut tx, demand.tenant_id).await? {
            return empty_allocation(tx).await;
        }
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
            return empty_allocation(tx).await;
        }
        let allocation_id = Uuid::now_v7();
        let rows = sqlx::query_as::<_, OracleAdmissionTimedBlockRow>(
            r#"WITH clock AS MATERIALIZED (SELECT statement_timestamp() AS now),
            scopes(scope_kind, principal_id) AS (
              VALUES ('global'::text, NULL::uuid), ('tenant', NULL), ('principal', $2::uuid)
            ), inserted AS (
              INSERT INTO vala.oracle_admission_blocks
              (block_id, allocation_id, scope_kind, data_tenant_id, principal_id, query_class,
               units, holder_node_id, holder_fencing_token, valid_from, expires_at)
              SELECT gen_random_uuid(), $1, scope_kind, $3, principal_id, $4, $5, $6, $7,
                     clock.now, clock.now + ($8 * interval '1 millisecond')
              FROM scopes CROSS JOIN clock
              RETURNING block_id, allocation_id, scope_kind, data_tenant_id, principal_id,
                        query_class, units, holder_node_id, holder_fencing_token,
                        valid_from, expires_at, closed_at
            )
            SELECT inserted.*, clock.now AS database_now
            FROM inserted CROSS JOIN clock"#,
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
        timed_allocation(rows)
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
    ) -> Result<Option<OracleAdmissionRenewal>, SqlError> {
        let fence = i64::try_from(holder_fencing_token)
            .map_err(|_| invariant("holder fence exceeds i64"))?;
        if validity <= Duration::zero() {
            return Err(invariant(
                "Oracle admission renewal validity must be positive",
            ));
        }
        let rows = sqlx::query_as::<_, OracleAdmissionTimedBlockRow>(
            r#"WITH clock AS MATERIALIZED (SELECT statement_timestamp() AS now),
            renewed AS (
              UPDATE vala.oracle_admission_blocks AS blocks
              SET expires_at=clock.now+($4*interval '1 millisecond')
              FROM clock
              WHERE blocks.allocation_id=$1 AND blocks.holder_node_id=$2
                AND blocks.holder_fencing_token=$3 AND blocks.closed_at IS NULL
                AND blocks.expires_at>clock.now
              RETURNING blocks.block_id, blocks.allocation_id, blocks.scope_kind,
                        blocks.data_tenant_id, blocks.principal_id, blocks.query_class,
                        blocks.units, blocks.holder_node_id, blocks.holder_fencing_token,
                        blocks.valid_from, blocks.expires_at, blocks.closed_at
            )
            SELECT renewed.*, clock.now AS database_now
            FROM renewed CROSS JOIN clock"#,
        )
        .bind(allocation_id)
        .bind(holder_node_id)
        .bind(fence)
        .bind(validity.num_milliseconds())
        .fetch_all(self.pool.pool())
        .await
        .map_err(SqlError::from)?;
        if rows.is_empty() {
            return Ok(None);
        }
        let mut database_now = None;
        let rows = rows
            .into_iter()
            .map(|row| {
                let (row, observed_now) = row.into_parts();
                if database_now
                    .replace(observed_now)
                    .is_some_and(|now| now != observed_now)
                {
                    return Err(invariant("Oracle admission renewal clock diverged"));
                }
                Ok(row)
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Some(OracleAdmissionRenewal {
            rows,
            database_now: database_now.ok_or_else(|| invariant("renewal clock is absent"))?,
        }))
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

    /// Closes one exact allocation owned by one exact holder incarnation.
    ///
    /// # Errors
    ///
    /// Returns [`SqlError`] when PostgreSQL cannot persist the fenced closure.
    pub async fn close_allocation(
        &self,
        allocation_id: Uuid,
        node_id: Uuid,
        fence: u64,
    ) -> Result<u64, SqlError> {
        let fence = i64::try_from(fence).map_err(|_| invariant("holder fence exceeds i64"))?;
        let result = sqlx::query(
            "UPDATE vala.oracle_admission_blocks SET closed_at=statement_timestamp() \
             WHERE allocation_id=$1 AND holder_node_id=$2 AND holder_fencing_token=$3 \
             AND closed_at IS NULL",
        )
        .bind(allocation_id)
        .bind(node_id)
        .bind(fence)
        .execute(self.pool.pool())
        .await
        .map_err(SqlError::from)?;
        Ok(result.rows_affected())
    }
}

/// Validates one canonical positive capacity before opening policy mutation.
///
/// # Errors
///
/// Returns [`SqlError`] when the capacity is zero or exceeds PostgreSQL `integer`.
fn validate_capacity(capacity: u32) -> Result<i32, SqlError> {
    let capacity = i32::try_from(capacity).map_err(|_| invariant("capacity exceeds i32"))?;
    if capacity == 0 {
        return Err(invariant("Oracle admission capacity must be positive"));
    }
    Ok(capacity)
}

/// Inserts or validates one policy row inside a caller-owned startup transaction.
///
/// # Errors
///
/// Returns [`SqlError`] when the row conflicts or PostgreSQL rejects the mutation.
async fn ensure_policy_in(
    tx: &mut Transaction<'_, Postgres>,
    scope: &'static str,
    query_class: QueryClass,
    capacity: u32,
) -> Result<(), SqlError> {
    let capacity = validate_capacity(capacity)?;
    let row: i32 = sqlx::query_scalar(
        "INSERT INTO vala.oracle_admission_policies(scope_kind,data_tenant_id,query_class,capacity) \
         VALUES($1,NULL,$2,$3) ON CONFLICT (scope_kind,data_tenant_id,query_class) \
         DO UPDATE SET capacity=vala.oracle_admission_policies.capacity RETURNING capacity",
    )
    .bind(scope)
    .bind(class_name(query_class))
    .bind(capacity)
    .fetch_one(&mut **tx)
    .await
    .map_err(SqlError::from)?;
    if row != capacity {
        return Err(invariant(
            "Oracle admission policy conflicts with durable ceiling",
        ));
    }
    Ok(())
}

/// Locks and returns one canonical policy row in deterministic caller order.
///
/// # Errors
///
/// Returns [`SqlError`] when the policy is missing or PostgreSQL cannot lock it.
async fn lock_policy(
    tx: &mut Transaction<'_, Postgres>,
    scope: &'static str,
    query_class: QueryClass,
) -> Result<i32, SqlError> {
    sqlx::query_scalar(
        "SELECT capacity FROM vala.oracle_admission_policies \
         WHERE scope_kind=$1 AND data_tenant_id IS NULL AND query_class=$2 FOR UPDATE",
    )
    .bind(scope)
    .bind(class_name(query_class))
    .fetch_optional(&mut **tx)
    .await
    .map_err(SqlError::from)?
    .ok_or_else(|| invariant("Oracle admission policy is not configured"))
}

/// Locks and validates the exact tenant carried by authenticated demand.
///
/// The row lock serializes allocation against concurrent suspension or deletion.
/// A missing or inactive tenant is an ordinary fail-closed refusal and does not
/// make canonical Oracle capacity unavailable to other tenants.
///
/// # Errors
///
/// Returns [`SqlError`] when PostgreSQL cannot inspect or lock the tenant row.
async fn lock_active_tenant(
    tx: &mut Transaction<'_, Postgres>,
    tenant_id: DataTenantId,
) -> Result<bool, SqlError> {
    let active: Option<bool> = sqlx::query_scalar(
        "SELECT status='active' AND deleted_at IS NULL FROM platform.tenants \
         WHERE data_tenant_id=$1 FOR UPDATE",
    )
    .bind(tenant_id.as_uuid())
    .fetch_optional(&mut **tx)
    .await
    .map_err(SqlError::from)?;
    Ok(active.unwrap_or(false))
}

/// Commits one authoritative capacity refusal at the database statement clock.
///
/// # Errors
///
/// Returns [`SqlError`] when PostgreSQL cannot read its clock or commit.
async fn empty_allocation(
    mut tx: Transaction<'_, Postgres>,
) -> Result<OracleAdmissionAllocation, SqlError> {
    let database_now = sqlx::query_scalar("SELECT statement_timestamp()")
        .fetch_one(&mut *tx)
        .await
        .map_err(SqlError::from)?;
    tx.commit().await.map_err(SqlError::from)?;
    Ok(OracleAdmissionAllocation {
        rows: Vec::new(),
        database_now,
    })
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
         AND blocks.query_class=$3 AND blocks.closed_at IS NULL \
         AND (blocks.expires_at>statement_timestamp() OR EXISTS ( \
           SELECT 1 FROM vala.cluster_nodes roles \
           WHERE roles.data_tenant_id=$4 \
           AND roles.node_id=blocks.holder_node_id AND roles.role='oracle' \
           AND roles.fencing_token=blocks.holder_fencing_token AND roles.ready=true \
           AND roles.heartbeat_at>=statement_timestamp()-interval '15 seconds'))",
    )
    .bind(scope)
    .bind(tenant_id)
    .bind(class_name(query_class))
    .bind(DataTenantId::SYSTEM_OWNER.as_uuid())
    .fetch_one(&mut **tx)
    .await
    .map_err(SqlError::from)
}

/// Splits timed allocation rows while proving one authoritative statement clock.
///
/// # Errors
///
/// Returns [`SqlError`] when the three rows do not carry one identical clock.
fn timed_allocation(
    rows: Vec<OracleAdmissionTimedBlockRow>,
) -> Result<OracleAdmissionAllocation, SqlError> {
    let mut database_now = None;
    let rows = rows
        .into_iter()
        .map(|row| {
            let (row, observed_now) = row.into_parts();
            if database_now
                .replace(observed_now)
                .is_some_and(|now| now != observed_now)
            {
                return Err(invariant("Oracle admission allocation clock diverged"));
            }
            Ok(row)
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(OracleAdmissionAllocation {
        rows,
        database_now: database_now.ok_or_else(|| invariant("allocation clock is absent"))?,
    })
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
