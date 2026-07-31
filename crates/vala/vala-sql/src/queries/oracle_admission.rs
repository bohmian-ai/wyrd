//! Durable request-tenant Oracle admission with deployment-wide cluster accounting.
//!
//! Dynamic query is intentional: admission selects closed runtime scopes while
//! every statement remains constrained by [`TenantConn`] RLS and an explicit
//! `data_tenant_id` predicate to either request-owned rows or the bounded
//! `SYSTEM_OWNER` cluster/class counters.

use chrono::{DateTime, Utc};
use wyrd_spec::{
    DataTenantId,
    vala::api::{AdmissionScope, OracleAdmissionLease, QueryClass, QueryId},
};

use crate::row_types::oracle_admission::{
    AdmissionAcquire, AdmissionExpiryReport, AdmissionLeaseRow, AdmissionReconcileScope,
    AdmissionReconciliationReport, AdmissionRequest, LeaseMutation, RoleFence,
};
use crate::{OperatorPool, SqlError, TenantConn, ValaPostgres};

/// Stable bounded retry hint returned for admission-capacity rejection.
const RETRY_AFTER_MS: u64 = 1_000;

/// SQL owner for tenant-scoped leases and shared cluster/class counters.
pub struct OracleAdmissionLeases {
    /// Vala runtime handle from which outer workflows open tenant transactions.
    postgres: ValaPostgres,
}

impl OracleAdmissionLeases {
    /// Constructs the durable admission owner over the Vala runtime pool.
    #[must_use]
    pub fn new(postgres: ValaPostgres) -> Self {
        Self { postgres }
    }

    /// Borrows the runtime handle used by outer admission workflows.
    #[must_use]
    pub fn postgres(&self) -> &ValaPostgres {
        &self.postgres
    }

    /// Rebuilds deployment-wide counters from every unexpired lease under the
    /// operator pool, atomically removing stale leases first.
    ///
    /// # Errors
    /// Returns [`SqlError`] when any recovery statement fails; the transaction
    /// is rolled back and callers must keep readiness false.
    pub async fn recover_shared_scopes(
        &self,
        operator: &OperatorPool,
        now: DateTime<Utc>,
    ) -> Result<(), SqlError> {
        let shared_owner = uuid::Uuid::from(DataTenantId::SYSTEM_OWNER);
        let mut transaction = operator.begin().await.map_err(SqlError::from)?;
        let result = async {
            sqlx::query(
            "INSERT INTO vala.oracle_admission_accounting \
             (data_tenant_id,scope_kind,scope_key,accounting_class,used_slots) \
             VALUES ($1,'cluster','global','all',0), \
                    ($1,'class','global','interactive',0), \
                    ($1,'class','global','analytical',0) \
             ON CONFLICT DO NOTHING",
            )
            .bind(shared_owner)
            .execute(&mut *transaction)
            .await
            .map_err(SqlError::from)?;
            let locked = sqlx::query(
                "SELECT scope_kind, accounting_class \
                 FROM vala.oracle_admission_accounting \
                 WHERE data_tenant_id=$1 AND scope_key='global' \
                 ORDER BY CASE WHEN scope_kind='cluster' THEN 0 ELSE 1 END, accounting_class \
                 FOR UPDATE",
            )
            .bind(shared_owner)
            .fetch_all(&mut *transaction)
            .await
            .map_err(SqlError::from)?;
            if locked.len() != 3 {
                return Err(SqlError::InvariantViolation {
                    detail: "Oracle shared accounting rows are incomplete".to_owned(),
                });
            }
            sqlx::query(
                "DELETE FROM vala.oracle_admission_leases WHERE expires_at <= $1",
            )
            .bind(now)
            .execute(&mut *transaction)
            .await
            .map_err(SqlError::from)?;
            let active = sqlx::query_as::<_, (String, i64)>(
                "SELECT query_class::text, COALESCE(sum(slot_units), 0)::bigint \
                 FROM vala.oracle_admission_leases \
                 WHERE expires_at > $1 GROUP BY query_class",
            )
            .bind(now)
            .fetch_all(&mut *transaction)
            .await
            .map_err(SqlError::from)?;
            let interactive = active
                .iter()
                .find(|(class, _)| class == "interactive")
                .map_or(0, |(_, slots)| *slots);
            let analytical = active
                .iter()
                .find(|(class, _)| class == "analytical")
                .map_or(0, |(_, slots)| *slots);
            let total = interactive + analytical;
            for (scope_kind, accounting_class, used_slots) in [
                ("cluster", "all", total),
                ("class", "interactive", interactive),
                ("class", "analytical", analytical),
            ] {
                sqlx::query(
                    "UPDATE vala.oracle_admission_accounting SET used_slots=$1,updated_at=now() \
                     WHERE data_tenant_id=$2 AND scope_kind=$3 AND scope_key='global' AND accounting_class=$4",
                )
                .bind(used_slots)
                .bind(shared_owner)
                .bind(scope_kind)
                .bind(accounting_class)
                .execute(&mut *transaction)
                .await
                .map_err(SqlError::from)?;
            }
            Ok::<(), SqlError>(())
        }
        .await;
        match result {
            Ok(()) => transaction.commit().await.map_err(SqlError::from),
            Err(error) => {
                let _ = transaction.rollback().await;
                Err(error)
            }
        }
    }

    /// Atomically locks shared cluster/class and request-tenant counters.
    ///
    /// # Errors
    /// Returns [`SqlError`] for invalid demand/limits, duplicate leases, or
    /// database failures. Rejection commits no lease or counter increment.
    pub async fn acquire(
        &self,
        conn: &mut TenantConn<'_>,
        request: &AdmissionRequest,
    ) -> Result<AdmissionAcquire, SqlError> {
        validate_request(request)?;
        validate_tenant(conn, request.lease.data_tenant_id)?;
        let data_tenant_id = uuid::Uuid::from(conn.data_tenant_id());
        let shared_owner = uuid::Uuid::from(DataTenantId::SYSTEM_OWNER);
        let class = class_name(request.lease.query_class);
        let tenant = request.lease.data_tenant_id.to_string();
        for (owner, kind, key, accounting_class) in [
            (shared_owner, "cluster", "global", "all"),
            (shared_owner, "class", "global", class),
            (data_tenant_id, "tenant", tenant.as_str(), class),
        ] {
            sqlx::query(
                "INSERT INTO vala.oracle_admission_accounting \
                 (data_tenant_id,scope_kind,scope_key,accounting_class,used_slots) \
                 VALUES ($1,$2,$3,$4,0) \
                 ON CONFLICT DO NOTHING",
            )
            .bind(owner)
            .bind(kind)
            .bind(key)
            .bind(accounting_class)
            .execute(&mut **conn.transaction())
            .await
            .map_err(SqlError::from)?;
        }
        for (owner, kind, key, accounting_class, limit, scope) in [
            (
                shared_owner,
                "cluster",
                "global",
                "all",
                request.cluster_limit,
                AdmissionScope::Cluster,
            ),
            (
                shared_owner,
                "class",
                "global",
                class,
                request.class_limit,
                AdmissionScope::Class,
            ),
            (
                data_tenant_id,
                "tenant",
                tenant.as_str(),
                class,
                request.tenant_limit,
                AdmissionScope::Tenant,
            ),
        ] {
            let used: i64 = sqlx::query_scalar(
                "SELECT used_slots FROM vala.oracle_admission_accounting \
                 WHERE data_tenant_id=$1 AND scope_kind=$2 AND scope_key=$3 \
                 AND accounting_class=$4 FOR UPDATE",
            )
            .bind(owner)
            .bind(kind)
            .bind(key)
            .bind(accounting_class)
            .fetch_one(&mut **conn.transaction())
            .await
            .map_err(SqlError::from)?;
            if used + i64::from(request.lease.slot_units) > i64::from(limit) {
                return Ok(AdmissionAcquire::Rejected {
                    scope,
                    retry_after_ms: RETRY_AFTER_MS,
                });
            }
        }
        for (owner, kind, key, accounting_class) in [
            (shared_owner, "cluster", "global", "all"),
            (shared_owner, "class", "global", class),
            (data_tenant_id, "tenant", tenant.as_str(), class),
        ] {
            sqlx::query(
                "UPDATE vala.oracle_admission_accounting SET used_slots=used_slots+$5, \
                 updated_at=now() WHERE data_tenant_id=$1 AND scope_kind=$2 \
                 AND scope_key=$3 AND accounting_class=$4",
            )
            .bind(owner)
            .bind(kind)
            .bind(key)
            .bind(accounting_class)
            .bind(i64::from(request.lease.slot_units))
            .execute(&mut **conn.transaction())
            .await
            .map_err(SqlError::from)?;
        }
        insert_lease(conn, &request.lease).await?;
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
        conn: &mut TenantConn<'_>,
        query_id: QueryId,
        leader: &RoleFence,
        new_expiry: DateTime<Utc>,
    ) -> Result<LeaseMutation, SqlError> {
        let Some(row) = lock_lease(conn, query_id).await? else {
            return Ok(LeaseMutation::Missing);
        };
        if !leader_matches(&row, leader)? {
            return Ok(LeaseMutation::StaleLeaderFence);
        }
        if row.expires_at <= Utc::now() || new_expiry <= Utc::now() {
            return Ok(LeaseMutation::Missing);
        }
        sqlx::query(
            "UPDATE vala.oracle_admission_leases SET expires_at=$3 \
             WHERE data_tenant_id=$1 AND query_id=$2",
        )
        .bind(uuid::Uuid::from(conn.data_tenant_id()))
        .bind(query_id.as_uuid())
        .bind(new_expiry)
        .execute(&mut **conn.transaction())
        .await
        .map_err(SqlError::from)?;
        let mut lease = row_to_lease(conn, row).await?;
        lease.expires_at = new_expiry;
        Ok(LeaseMutation::Renewed(lease))
    }

    /// Idempotently releases one matching leader lease and its counters.
    ///
    /// # Errors
    /// Returns [`SqlError`] for malformed stored state or database failures.
    pub async fn release(
        &self,
        conn: &mut TenantConn<'_>,
        query_id: QueryId,
        leader: &RoleFence,
    ) -> Result<LeaseMutation, SqlError> {
        let Some(row) = lock_lease(conn, query_id).await? else {
            return Ok(LeaseMutation::AlreadyReleased);
        };
        if !leader_matches(&row, leader)? {
            return Ok(LeaseMutation::StaleLeaderFence);
        }
        decrement_and_delete(conn, &row).await?;
        Ok(LeaseMutation::Released)
    }

    /// Expires one ordered, skip-locked bounded batch.
    ///
    /// # Errors
    /// Returns [`SqlError`] when `max_leases` is outside `1..=128`, stored
    /// counters would underflow, or the transaction fails.
    pub async fn expire_batch(
        &self,
        conn: &mut TenantConn<'_>,
        now: DateTime<Utc>,
        max_leases: u16,
    ) -> Result<AdmissionExpiryReport, SqlError> {
        if !(1..=128).contains(&max_leases) {
            return Err(invariant("expiry batch must be 1..=128"));
        }
        let rows = sqlx::query_as::<_, AdmissionLeaseRow>(
            "SELECT query_id,data_tenant_id,query_class,slot_units,leader_node_id,\
             leader_fencing_token,acquired_at,expires_at FROM vala.oracle_admission_leases \
             WHERE data_tenant_id=$1 AND expires_at <= $2 \
             ORDER BY expires_at,query_id LIMIT $3 \
             FOR UPDATE SKIP LOCKED",
        )
        .bind(uuid::Uuid::from(conn.data_tenant_id()))
        .bind(now)
        .bind(i64::from(max_leases))
        .fetch_all(&mut **conn.transaction())
        .await
        .map_err(SqlError::from)?;
        let released_slots = rows.iter().try_fold(0_u64, |sum, row| {
            u64::try_from(row.slot_units)
                .map(|value| sum + value)
                .map_err(|_| invariant("negative lease slots"))
        })?;
        for row in &rows {
            decrement_and_delete(conn, row).await?;
        }
        let expired_leases =
            u16::try_from(rows.len()).map_err(|_| invariant("expiry count overflow"))?;
        Ok(AdmissionExpiryReport {
            expired_leases,
            released_slots,
        })
    }

    /// Recomputes request-tenant scopes and validates locked shared counters.
    ///
    /// Cluster and class accounting are exact transaction invariants maintained
    /// by acquire, release, and expiry. Reconciliation never scans other
    /// tenants' leases to reconstruct them.
    ///
    /// # Errors
    /// Returns [`SqlError`] for empty, duplicate, or over-64 scope input, or
    /// database failures.
    pub async fn reconcile_scopes(
        &self,
        conn: &mut TenantConn<'_>,
        scopes: &[AdmissionReconcileScope],
        now: DateTime<Utc>,
    ) -> Result<AdmissionReconciliationReport, SqlError> {
        if scopes.is_empty() || scopes.len() > 64 {
            return Err(invariant("reconciliation scopes must be 1..=64"));
        }
        for scope in scopes {
            if let AdmissionReconcileScope::Tenant { data_tenant_id, .. } = scope {
                validate_tenant(conn, *data_tenant_id)?;
            }
        }
        let mut keys = scopes.iter().map(scope_key).collect::<Vec<_>>();
        keys.sort();
        keys.dedup();
        if keys.len() != scopes.len() {
            return Err(invariant("duplicate reconciliation scope"));
        }
        let data_tenant_id = uuid::Uuid::from(conn.data_tenant_id());
        let shared_owner = uuid::Uuid::from(DataTenantId::SYSTEM_OWNER);
        let mut repaired = 0_u8;
        let mut unchanged = 0_u8;
        for (_, kind, key, accounting_class) in keys {
            let owner = if matches!(kind, "cluster" | "class") {
                shared_owner
            } else {
                data_tenant_id
            };
            if kind == "tenant" {
                sqlx::query(
                    "INSERT INTO vala.oracle_admission_accounting \
                     (data_tenant_id,scope_kind,scope_key,accounting_class,used_slots) \
                     VALUES ($1,$2,$3,$4,0) \
                     ON CONFLICT DO NOTHING",
                )
                .bind(owner)
                .bind(kind)
                .bind(&key)
                .bind(accounting_class)
                .execute(&mut **conn.transaction())
                .await
                .map_err(SqlError::from)?;
            }
            let used: i64 = sqlx::query_scalar(
                "SELECT used_slots FROM vala.oracle_admission_accounting \
                 WHERE data_tenant_id=$1 AND scope_kind=$2 AND scope_key=$3 \
                 AND accounting_class=$4 FOR UPDATE",
            )
            .bind(owner)
            .bind(kind)
            .bind(&key)
            .bind(accounting_class)
            .fetch_one(&mut **conn.transaction())
            .await
            .map_err(SqlError::from)?;
            let authoritative: i64 = match kind {
                // Deployment-wide counters are maintained exactly by every
                // lease mutation. Request tenants may lock them, but never
                // scan other tenants' leases to reconstruct them.
                "cluster" | "class" => used,
                "tenant" => sqlx::query_scalar(
                    "SELECT COALESCE(sum(slot_units),0)::bigint \
                     FROM vala.oracle_admission_leases \
                     WHERE data_tenant_id=$1 AND expires_at > $2 AND query_class=$3",
                )
                .bind(data_tenant_id)
                .bind(now)
                .bind(accounting_class)
                .fetch_one(&mut **conn.transaction())
                .await
                .map_err(SqlError::from)?,
                _ => return Err(invariant("unknown reconciliation scope")),
            };
            let changed = sqlx::query(
                "UPDATE vala.oracle_admission_accounting SET used_slots=$5,updated_at=now() \
                 WHERE data_tenant_id=$1 AND scope_kind=$2 AND scope_key=$3 \
                 AND accounting_class=$4 AND used_slots<>$5",
            )
            .bind(owner)
            .bind(kind)
            .bind(&key)
            .bind(accounting_class)
            .bind(authoritative)
            .execute(&mut **conn.transaction())
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
/// failures. Mutations remain uncommitted in `conn`.
async fn insert_lease(
    conn: &mut TenantConn<'_>,
    lease: &OracleAdmissionLease,
) -> Result<(), SqlError> {
    validate_tenant(conn, lease.data_tenant_id)?;
    let data_tenant_id = uuid::Uuid::from(conn.data_tenant_id());
    sqlx::query("INSERT INTO vala.oracle_admission_leases \
        (data_tenant_id,query_id,query_class,slot_units,leader_node_id,leader_fencing_token,acquired_at,expires_at) \
        VALUES ($1,$2,$3,$4,$5,$6,$7,$8)")
        .bind(data_tenant_id).bind(lease.query_id.as_uuid())
        .bind(class_name(lease.query_class))
        .bind(i32::try_from(lease.slot_units).map_err(|_| invariant("slot demand exceeds i32"))?)
        .bind(lease.leader_node_id.as_uuid())
        .bind(i64::try_from(lease.leader_fencing_token).map_err(|_| invariant("leader fence exceeds i64"))?)
        .bind(lease.acquired_at).bind(lease.expires_at)
        .execute(&mut **conn.transaction()).await.map_err(SqlError::from)?;
    let mut nodes = lease.selected_node_ids.clone();
    nodes.sort();
    nodes.dedup();
    if nodes.len() != lease.selected_node_ids.len() {
        return Err(invariant("duplicate selected node"));
    }
    for node in nodes {
        sqlx::query(
            "INSERT INTO vala.oracle_admission_nodes (data_tenant_id,query_id,node_id) \
             VALUES ($1,$2,$3)",
        )
        .bind(data_tenant_id)
        .bind(lease.query_id.as_uuid())
        .bind(node.as_uuid())
        .execute(&mut **conn.transaction())
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
    conn: &mut TenantConn<'_>,
    query_id: QueryId,
) -> Result<Option<AdmissionLeaseRow>, SqlError> {
    sqlx::query_as(
        "SELECT query_id,data_tenant_id,query_class,slot_units,leader_node_id,\
        leader_fencing_token,acquired_at,expires_at FROM vala.oracle_admission_leases \
        WHERE data_tenant_id=$1 AND query_id=$2 FOR UPDATE",
    )
    .bind(uuid::Uuid::from(conn.data_tenant_id()))
    .bind(query_id.as_uuid())
    .fetch_optional(&mut **conn.transaction())
    .await
    .map_err(SqlError::from)
}

/// Decrements all canonical counters before deleting one locked lease.
///
/// # Errors
/// Returns [`SqlError`] for missing/underflowing counters or database failure.
async fn decrement_and_delete(
    conn: &mut TenantConn<'_>,
    row: &AdmissionLeaseRow,
) -> Result<(), SqlError> {
    let data_tenant_id = uuid::Uuid::from(conn.data_tenant_id());
    let shared_owner = uuid::Uuid::from(DataTenantId::SYSTEM_OWNER);
    if row.data_tenant_id != data_tenant_id {
        return Err(invariant("locked lease tenant does not match transaction"));
    }
    let class = row.query_class.as_str();
    let tenant = row.data_tenant_id.to_string();
    for (owner, kind, key, accounting_class) in [
        (shared_owner, "cluster", "global", "all"),
        (shared_owner, "class", "global", class),
        (data_tenant_id, "tenant", tenant.as_str(), class),
    ] {
        let changed = sqlx::query(
            "UPDATE vala.oracle_admission_accounting \
            SET used_slots=used_slots-$5,updated_at=now() WHERE data_tenant_id=$1 \
            AND scope_kind=$2 AND scope_key=$3 AND accounting_class=$4 AND used_slots >= $5",
        )
        .bind(owner)
        .bind(kind)
        .bind(key)
        .bind(accounting_class)
        .bind(i64::from(row.slot_units))
        .execute(&mut **conn.transaction())
        .await
        .map_err(SqlError::from)?
        .rows_affected();
        if changed != 1 {
            return Err(invariant("admission counter underflow or missing scope"));
        }
    }
    sqlx::query("DELETE FROM vala.oracle_admission_leases WHERE data_tenant_id=$1 AND query_id=$2")
        .bind(data_tenant_id)
        .bind(row.query_id)
        .execute(&mut **conn.transaction())
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
    conn: &mut TenantConn<'_>,
    row: AdmissionLeaseRow,
) -> Result<OracleAdmissionLease, SqlError> {
    let nodes: Vec<uuid::Uuid> = sqlx::query_scalar(
        "SELECT node_id FROM vala.oracle_admission_nodes \
         WHERE data_tenant_id=$1 AND query_id=$2 ORDER BY node_id",
    )
    .bind(uuid::Uuid::from(conn.data_tenant_id()))
    .bind(row.query_id)
    .fetch_all(&mut **conn.transaction())
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
/// Validates lease and all three positive ceilings before durable mutation.
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

/// Verifies that a request-carried tenant matches the caller-owned transaction.
///
/// # Errors
/// Returns [`SqlError`] when the two independently supplied tenant identities differ.
fn validate_tenant(conn: &TenantConn<'_>, data_tenant_id: DataTenantId) -> Result<(), SqlError> {
    if conn.data_tenant_id() == data_tenant_id {
        Ok(())
    } else {
        Err(invariant("admission tenant does not match transaction"))
    }
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
fn scope_key(scope: &AdmissionReconcileScope) -> (u8, &'static str, String, &'static str) {
    match scope {
        AdmissionReconcileScope::Cluster => (0, "cluster", "global".into(), "all"),
        AdmissionReconcileScope::Class { query_class } => {
            (1, "class", "global".into(), class_name(*query_class))
        }
        AdmissionReconcileScope::Tenant {
            data_tenant_id,
            query_class,
        } => {
            let tenant = uuid::Uuid::from(*data_tenant_id);
            (2, "tenant", tenant.to_string(), class_name(*query_class))
        }
    }
}
/// Constructs a scrubbed control-plane invariant error.
fn invariant(detail: &str) -> SqlError {
    SqlError::InvariantViolation {
        detail: detail.to_owned(),
    }
}
