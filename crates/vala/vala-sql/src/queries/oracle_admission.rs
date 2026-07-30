//! Durable, operator-owned Oracle admission transactions.

use chrono::{DateTime, Utc};
use wyrd_spec::vala::api::{AdmissionScope, OracleAdmissionLease, QueryClass, QueryId};

use crate::operator_transaction::OperatorTransaction;
use crate::row_types::oracle_admission::{
    AdmissionAcquire, AdmissionExpiryReport, AdmissionLeaseRow, AdmissionReconcileScope,
    AdmissionReconciliationReport, AdmissionRequest, LeaseMutation, RoleFence,
};
use crate::{OperatorPool, SqlError};

/// Stable bounded retry hint returned for admission-capacity rejection.
const RETRY_AFTER_MS: u64 = 1_000;

/// Operator-pool owner for admission leases and counters.
pub struct OracleAdmissionLeases {
    /// Audited operator-role pool used for cluster-global admission state.
    pool: OperatorPool,
}

impl OracleAdmissionLeases {
    /// Constructs the durable admission owner.
    #[must_use]
    pub fn new(pool: OperatorPool) -> Self {
        Self { pool }
    }

    /// Atomically locks and enforces cluster, class, and tenant counters.
    ///
    /// # Errors
    /// Returns [`SqlError`] for invalid demand/limits, duplicate leases, or
    /// database failures. Rejection commits no lease or counter increment.
    pub async fn acquire(&self, request: &AdmissionRequest) -> Result<AdmissionAcquire, SqlError> {
        validate_request(request)?;
        let mut tx = OperatorTransaction::start(&self.pool).await?;
        let class = class_name(request.lease.query_class);
        let tenant = request.lease.data_tenant_id.to_string();
        for (kind, key, accounting_class) in [
            ("cluster", "global", "all"),
            ("class", "global", class),
            ("tenant", tenant.as_str(), class),
        ] {
            sqlx::query(
                "INSERT INTO vala.oracle_admission_accounting \
                 (scope_kind,scope_key,accounting_class,used_slots) VALUES ($1,$2,$3,0) \
                 ON CONFLICT DO NOTHING",
            )
            .bind(kind)
            .bind(key)
            .bind(accounting_class)
            .execute(tx.connection())
            .await
            .map_err(SqlError::from)?;
        }
        for (kind, key, accounting_class, limit, scope) in [
            (
                "cluster",
                "global",
                "all",
                request.cluster_limit,
                AdmissionScope::Cluster,
            ),
            (
                "class",
                "global",
                class,
                request.class_limit,
                AdmissionScope::Class,
            ),
            (
                "tenant",
                tenant.as_str(),
                class,
                request.tenant_limit,
                AdmissionScope::Tenant,
            ),
        ] {
            let used: i64 = sqlx::query_scalar(
                "SELECT used_slots FROM vala.oracle_admission_accounting \
                 WHERE scope_kind=$1 AND scope_key=$2 AND accounting_class=$3 FOR UPDATE",
            )
            .bind(kind)
            .bind(key)
            .bind(accounting_class)
            .fetch_one(tx.connection())
            .await
            .map_err(SqlError::from)?;
            if used + i64::from(request.lease.slot_units) > i64::from(limit) {
                tx.cancel().await?;
                return Ok(AdmissionAcquire::Rejected {
                    scope,
                    retry_after_ms: RETRY_AFTER_MS,
                });
            }
        }
        for (kind, key, accounting_class) in [
            ("cluster", "global", "all"),
            ("class", "global", class),
            ("tenant", tenant.as_str(), class),
        ] {
            sqlx::query(
                "UPDATE vala.oracle_admission_accounting SET used_slots=used_slots+$4, \
                 updated_at=now() WHERE scope_kind=$1 AND scope_key=$2 AND accounting_class=$3",
            )
            .bind(kind)
            .bind(key)
            .bind(accounting_class)
            .bind(i64::from(request.lease.slot_units))
            .execute(tx.connection())
            .await
            .map_err(SqlError::from)?;
        }
        insert_lease(&mut tx, &request.lease).await?;
        tx.finish().await?;
        Ok(AdmissionAcquire::Acquired(request.lease.clone()))
    }

    /// Renews one matching active leader lease.
    ///
    /// Leader comparison precedes expiry evaluation.
    ///
    /// # Errors
    /// Returns [`SqlError`] for invalid expiry or database failures.
    pub async fn renew(
        &self,
        query_id: QueryId,
        leader: &RoleFence,
        new_expiry: DateTime<Utc>,
    ) -> Result<LeaseMutation, SqlError> {
        let mut tx = OperatorTransaction::start(&self.pool).await?;
        let Some(row) = lock_lease(&mut tx, query_id).await? else {
            tx.finish().await?;
            return Ok(LeaseMutation::Missing);
        };
        if !leader_matches(&row, leader)? {
            tx.cancel().await?;
            return Ok(LeaseMutation::StaleLeaderFence);
        }
        if row.expires_at <= Utc::now() || new_expiry <= Utc::now() {
            tx.finish().await?;
            return Ok(LeaseMutation::Missing);
        }
        sqlx::query("UPDATE vala.oracle_admission_leases SET expires_at=$2 WHERE query_id=$1")
            .bind(query_id.as_uuid())
            .bind(new_expiry)
            .execute(tx.connection())
            .await
            .map_err(SqlError::from)?;
        let mut lease = row_to_lease(&mut tx, row).await?;
        lease.expires_at = new_expiry;
        tx.finish().await?;
        Ok(LeaseMutation::Renewed(lease))
    }

    /// Idempotently releases one matching leader lease and its counters.
    ///
    /// # Errors
    /// Returns [`SqlError`] for malformed stored state or database failures.
    pub async fn release(
        &self,
        query_id: QueryId,
        leader: &RoleFence,
    ) -> Result<LeaseMutation, SqlError> {
        let mut tx = OperatorTransaction::start(&self.pool).await?;
        let Some(row) = lock_lease(&mut tx, query_id).await? else {
            tx.finish().await?;
            return Ok(LeaseMutation::AlreadyReleased);
        };
        if !leader_matches(&row, leader)? {
            tx.cancel().await?;
            return Ok(LeaseMutation::StaleLeaderFence);
        }
        decrement_and_delete(&mut tx, &row).await?;
        tx.finish().await?;
        Ok(LeaseMutation::Released)
    }

    /// Expires one ordered, skip-locked bounded batch.
    ///
    /// # Errors
    /// Returns [`SqlError`] when `max_leases` is outside `1..=128`, stored
    /// counters would underflow, or the transaction fails.
    pub async fn expire_batch(
        &self,
        now: DateTime<Utc>,
        max_leases: u16,
    ) -> Result<AdmissionExpiryReport, SqlError> {
        if !(1..=128).contains(&max_leases) {
            return Err(invariant("expiry batch must be 1..=128"));
        }
        let mut tx = OperatorTransaction::start(&self.pool).await?;
        let rows = sqlx::query_as::<_, AdmissionLeaseRow>(
            "SELECT query_id,data_tenant_id,query_class,slot_units,leader_node_id,\
             leader_fencing_token,acquired_at,expires_at FROM vala.oracle_admission_leases \
             WHERE expires_at <= $1 ORDER BY expires_at,query_id LIMIT $2 \
             FOR UPDATE SKIP LOCKED",
        )
        .bind(now)
        .bind(i64::from(max_leases))
        .fetch_all(tx.connection())
        .await
        .map_err(SqlError::from)?;
        let released_slots = rows.iter().try_fold(0_u64, |sum, row| {
            u64::try_from(row.slot_units)
                .map(|value| sum + value)
                .map_err(|_| invariant("negative lease slots"))
        })?;
        for row in &rows {
            decrement_and_delete(&mut tx, row).await?;
        }
        let expired_leases =
            u16::try_from(rows.len()).map_err(|_| invariant("expiry count overflow"))?;
        tx.finish().await?;
        Ok(AdmissionExpiryReport {
            expired_leases,
            released_slots,
        })
    }

    /// Recomputes only the supplied bounded, distinct accounting scopes.
    ///
    /// # Errors
    /// Returns [`SqlError`] for empty, duplicate, or over-64 scope input, or
    /// database failures.
    pub async fn reconcile_scopes(
        &self,
        scopes: &[AdmissionReconcileScope],
        now: DateTime<Utc>,
    ) -> Result<AdmissionReconciliationReport, SqlError> {
        if scopes.is_empty() || scopes.len() > 64 {
            return Err(invariant("reconciliation scopes must be 1..=64"));
        }
        let mut keys = scopes.iter().map(scope_key).collect::<Vec<_>>();
        keys.sort();
        keys.dedup();
        if keys.len() != scopes.len() {
            return Err(invariant("duplicate reconciliation scope"));
        }
        let mut tx = OperatorTransaction::start(&self.pool).await?;
        let mut repaired = 0_u8;
        let mut unchanged = 0_u8;
        for (_, kind, key, accounting_class, filter_tenant) in keys {
            sqlx::query(
                "INSERT INTO vala.oracle_admission_accounting \
                 (scope_kind,scope_key,accounting_class,used_slots) VALUES ($1,$2,$3,0) \
                 ON CONFLICT DO NOTHING",
            )
            .bind(kind)
            .bind(&key)
            .bind(accounting_class)
            .execute(tx.connection())
            .await
            .map_err(SqlError::from)?;
            let _: i64 = sqlx::query_scalar(
                "SELECT used_slots FROM vala.oracle_admission_accounting \
                 WHERE scope_kind=$1 AND scope_key=$2 AND accounting_class=$3 FOR UPDATE",
            )
            .bind(kind)
            .bind(&key)
            .bind(accounting_class)
            .fetch_one(tx.connection())
            .await
            .map_err(SqlError::from)?;
            let authoritative: i64 = match kind {
                "cluster" => sqlx::query_scalar("SELECT COALESCE(sum(slot_units),0)::bigint FROM vala.oracle_admission_leases WHERE expires_at > $1")
                    .bind(now).fetch_one(tx.connection()).await.map_err(SqlError::from)?,
                "class" => sqlx::query_scalar("SELECT COALESCE(sum(slot_units),0)::bigint FROM vala.oracle_admission_leases WHERE expires_at > $1 AND query_class=$2")
                    .bind(now).bind(accounting_class).fetch_one(tx.connection()).await.map_err(SqlError::from)?,
                _ => sqlx::query_scalar("SELECT COALESCE(sum(slot_units),0)::bigint FROM vala.oracle_admission_leases WHERE expires_at > $1 AND query_class=$2 AND data_tenant_id=$3")
                    .bind(now).bind(accounting_class).bind(filter_tenant.expect("tenant scope has tenant")).fetch_one(tx.connection()).await.map_err(SqlError::from)?,
            };
            let changed = sqlx::query(
                "UPDATE vala.oracle_admission_accounting SET used_slots=$4,updated_at=now() \
                 WHERE scope_kind=$1 AND scope_key=$2 AND accounting_class=$3 AND used_slots<>$4",
            )
            .bind(kind)
            .bind(&key)
            .bind(accounting_class)
            .bind(authoritative)
            .execute(tx.connection())
            .await
            .map_err(SqlError::from)?
            .rows_affected()
                == 1;
            if changed {
                repaired += 1;
            } else {
                unchanged += 1;
            }
        }
        tx.finish().await?;
        Ok(AdmissionReconciliationReport {
            repaired_scopes: repaired,
            unchanged_scopes: unchanged,
        })
    }
}

/// Inserts one validated lease and its unique sorted selected-node rows.
///
/// # Errors
/// Returns [`SqlError`] for numeric overflow, duplicate nodes, or database
/// failures. Mutations remain uncommitted in `tx`.
async fn insert_lease(
    tx: &mut OperatorTransaction,
    lease: &OracleAdmissionLease,
) -> Result<(), SqlError> {
    sqlx::query("INSERT INTO vala.oracle_admission_leases \
        (query_id,data_tenant_id,query_class,slot_units,leader_node_id,leader_fencing_token,acquired_at,expires_at) \
        VALUES ($1,$2,$3,$4,$5,$6,$7,$8)")
        .bind(lease.query_id.as_uuid()).bind(uuid::Uuid::from(lease.data_tenant_id))
        .bind(class_name(lease.query_class))
        .bind(i32::try_from(lease.slot_units).map_err(|_| invariant("slot demand exceeds i32"))?)
        .bind(lease.leader_node_id.as_uuid())
        .bind(i64::try_from(lease.leader_fencing_token).map_err(|_| invariant("leader fence exceeds i64"))?)
        .bind(lease.acquired_at).bind(lease.expires_at)
        .execute(tx.connection()).await.map_err(SqlError::from)?;
    let mut nodes = lease.selected_node_ids.clone();
    nodes.sort();
    nodes.dedup();
    if nodes.len() != lease.selected_node_ids.len() {
        return Err(invariant("duplicate selected node"));
    }
    for node in nodes {
        sqlx::query("INSERT INTO vala.oracle_admission_nodes (query_id,node_id) VALUES ($1,$2)")
            .bind(lease.query_id.as_uuid())
            .bind(node.as_uuid())
            .execute(tx.connection())
            .await
            .map_err(SqlError::from)?;
    }
    Ok(())
}

/// Locks one lease by query identity for fenced mutation.
///
/// # Errors
/// Returns [`SqlError`] when PostgreSQL cannot execute the locking read.
async fn lock_lease(
    tx: &mut OperatorTransaction,
    query_id: QueryId,
) -> Result<Option<AdmissionLeaseRow>, SqlError> {
    sqlx::query_as(
        "SELECT query_id,data_tenant_id,query_class,slot_units,leader_node_id,\
        leader_fencing_token,acquired_at,expires_at FROM vala.oracle_admission_leases \
        WHERE query_id=$1 FOR UPDATE",
    )
    .bind(query_id.as_uuid())
    .fetch_optional(tx.connection())
    .await
    .map_err(SqlError::from)
}

/// Decrements all canonical counters before deleting one locked lease.
///
/// # Errors
/// Returns [`SqlError`] for missing/underflowing counters or database failure.
async fn decrement_and_delete(
    tx: &mut OperatorTransaction,
    row: &AdmissionLeaseRow,
) -> Result<(), SqlError> {
    let class = row.query_class.as_str();
    let tenant = row.data_tenant_id.to_string();
    for (kind, key, accounting_class) in [
        ("cluster", "global", "all"),
        ("class", "global", class),
        ("tenant", tenant.as_str(), class),
    ] {
        let changed = sqlx::query(
            "UPDATE vala.oracle_admission_accounting \
            SET used_slots=used_slots-$4,updated_at=now() WHERE scope_kind=$1 AND scope_key=$2 \
            AND accounting_class=$3 AND used_slots >= $4",
        )
        .bind(kind)
        .bind(key)
        .bind(accounting_class)
        .bind(i64::from(row.slot_units))
        .execute(tx.connection())
        .await
        .map_err(SqlError::from)?
        .rows_affected();
        if changed != 1 {
            return Err(invariant("admission counter underflow or missing scope"));
        }
    }
    sqlx::query("DELETE FROM vala.oracle_admission_leases WHERE query_id=$1")
        .bind(row.query_id)
        .execute(tx.connection())
        .await
        .map_err(SqlError::from)?;
    Ok(())
}

/// Rehydrates one locked database row and its selected nodes.
///
/// # Errors
/// Returns [`SqlError`] for malformed stored identifiers/classes/counts or
/// database failures.
async fn row_to_lease(
    tx: &mut OperatorTransaction,
    row: AdmissionLeaseRow,
) -> Result<OracleAdmissionLease, SqlError> {
    let nodes: Vec<uuid::Uuid> = sqlx::query_scalar(
        "SELECT node_id FROM vala.oracle_admission_nodes WHERE query_id=$1 ORDER BY node_id",
    )
    .bind(row.query_id)
    .fetch_all(tx.connection())
    .await
    .map_err(SqlError::from)?;
    Ok(OracleAdmissionLease {
        query_id: row.query_id.into(),
        data_tenant_id: row
            .data_tenant_id
            .try_into()
            .map_err(|error: wyrd_spec::ids::IdError| invariant(&error.to_string()))?,
        query_class: parse_class(&row.query_class)?,
        slot_units: u32::try_from(row.slot_units).map_err(|_| invariant("negative lease slots"))?,
        selected_node_ids: nodes.into_iter().map(Into::into).collect(),
        leader_node_id: row.leader_node_id.into(),
        leader_fencing_token: u64::try_from(row.leader_fencing_token)
            .map_err(|_| invariant("negative leader fence"))?,
        acquired_at: row.acquired_at,
        expires_at: row.expires_at,
    })
}

/// Compares the complete persisted leader tuple with a requested fence.
///
/// # Errors
/// Returns [`SqlError`] when the supplied fence cannot fit PostgreSQL bigint.
fn leader_matches(row: &AdmissionLeaseRow, leader: &RoleFence) -> Result<bool, SqlError> {
    Ok(row.leader_node_id == leader.node_id.as_uuid()
        && u64::try_from(row.leader_fencing_token)
            .map_err(|_| invariant("negative leader fence"))?
            == leader.fencing_token)
}
/// Validates lease and all three positive ceilings before transaction start.
///
/// # Errors
/// Returns [`SqlError`] for invalid pure lease invariants or zero limits.
fn validate_request(request: &AdmissionRequest) -> Result<(), SqlError> {
    let lease = &request.lease;
    lease
        .validate()
        .map_err(|error| invariant(&error.to_string()))?;
    if request.cluster_limit == 0 || request.class_limit == 0 || request.tenant_limit == 0 {
        return Err(invariant("invalid admission request"));
    }
    Ok(())
}
/// Maps one closed admission class to durable SQL text.
fn class_name(class: QueryClass) -> &'static str {
    match class {
        QueryClass::Interactive => "interactive",
        QueryClass::Analytical => "analytical",
    }
}
/// Parses one durable admission-class discriminator.
///
/// # Errors
/// Returns [`SqlError`] for unknown stored class text.
fn parse_class(class: &str) -> Result<QueryClass, SqlError> {
    match class {
        "interactive" => Ok(QueryClass::Interactive),
        "analytical" => Ok(QueryClass::Analytical),
        _ => Err(invariant("unknown query class")),
    }
}
/// Produces the canonical cluster/class/tenant reconciliation sort and SQL key.
fn scope_key(
    scope: &AdmissionReconcileScope,
) -> (u8, &'static str, String, &'static str, Option<uuid::Uuid>) {
    match scope {
        AdmissionReconcileScope::Cluster => (0, "cluster", "global".into(), "all", None),
        AdmissionReconcileScope::Class { query_class } => {
            (1, "class", "global".into(), class_name(*query_class), None)
        }
        AdmissionReconcileScope::Tenant {
            data_tenant_id,
            query_class,
        } => {
            let tenant = uuid::Uuid::from(*data_tenant_id);
            (
                2,
                "tenant",
                tenant.to_string(),
                class_name(*query_class),
                Some(tenant),
            )
        }
    }
}
/// Constructs a scrubbed control-plane invariant error.
fn invariant(detail: &str) -> SqlError {
    SqlError::InvariantViolation {
        detail: detail.to_owned(),
    }
}
