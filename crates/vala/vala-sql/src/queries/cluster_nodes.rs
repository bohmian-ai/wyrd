//! Tenant-scoped role membership workflows.
//!
//! Dynamic query is intentional: membership projections decode capability JSON
//! through typed row conversions while every statement remains tenant-bound by
//! both [`TenantConn`] RLS and an explicit `data_tenant_id` predicate.

use std::time::Duration;

use chrono::{DateTime, Utc};
use wyrd_spec::vala::api::{
    ClusterCapabilities, ClusterNodeKey, ClusterRole, ClusterRoleLease, FencingToken,
};

use crate::row_types::cluster_nodes::{RegisteredRoleRow, RoleMutation, RoleRegistration};
use crate::{SqlError, TenantConn, ValaPostgres};

/// SQL owner for role-fenced cluster membership.
pub struct ClusterNodes {
    /// Vala runtime handle from which callers open tenant transactions.
    postgres: ValaPostgres,
}

impl ClusterNodes {
    /// Constructs the membership owner over the Vala runtime pool.
    #[must_use]
    pub fn new(postgres: ValaPostgres) -> Self {
        Self { postgres }
    }

    /// Borrows the runtime handle used by the outer workflow to open transactions.
    #[must_use]
    pub fn postgres(&self) -> &ValaPostgres {
        &self.postgres
    }

    /// Validates one ready, fresh fenced Oracle role identity.
    ///
    /// `liveness` is a window, not a cutoff instant: `PostgreSQL` subtracts it
    /// from its own `statement_timestamp()` so every replica observes the same
    /// liveness boundary regardless of host clock skew.
    ///
    /// # Errors
    /// Returns [`SqlError`] when the query fails or the exact row is absent.
    pub async fn validate_live_oracle(
        &self,
        conn: &mut TenantConn<'_>,
        node_id: wyrd_spec::vala::api::NodeId,
        fencing_token: FencingToken,
        liveness: Duration,
    ) -> Result<(), SqlError> {
        let found: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM vala.cluster_nodes \
             WHERE data_tenant_id=$1 AND node_id=$2 AND role='oracle' \
             AND fencing_token=$3 AND ready=true \
             AND heartbeat_at >= statement_timestamp() - ($4 * interval '1 second'))",
        )
        .bind(uuid::Uuid::from(conn.data_tenant_id()))
        .bind(node_id.as_uuid())
        .bind(i64::try_from(fencing_token).map_err(|_| invariant("fence exceeds i64"))?)
        .bind(liveness.as_secs_f64())
        .fetch_one(&mut **conn.transaction())
        .await
        .map_err(SqlError::from)?;
        if found {
            Ok(())
        } else {
            Err(invariant("Oracle peer membership is not live"))
        }
    }

    /// Registers one role and atomically advances only its composite fence.
    ///
    /// The existing role row is taken `FOR UPDATE` before the UPSERT so that
    /// registration of a replacement and renewal of the incumbent's Oracle
    /// reader lease serialize on one row, in the one lock order every epoch
    /// statement uses.
    ///
    /// # Errors
    /// Returns [`SqlError`] for invalid capabilities/address, overflow, JSON,
    /// or database failures.
    pub async fn register(
        &self,
        conn: &mut TenantConn<'_>,
        registration: &RoleRegistration,
    ) -> Result<RegisteredRoleRow, SqlError> {
        validate_role(
            registration.key.role,
            &registration.address,
            &registration.capabilities,
        )?;
        let role = role_name(registration.key.role);
        let capabilities = serde_json::to_value(&registration.capabilities)
            .map_err(|error| invariant(&error.to_string()))?;
        // Registration and Oracle lease renewal share one lock order: the role
        // row first, then the epoch. Taking the row here means a replacement
        // selecting a new fence and an incumbent renewing its lease serialize,
        // so an epoch whose fence has already been superseded cannot renew for
        // one more round. A first registration has no row to lock and the
        // UPSERT below creates it.
        sqlx::query(
            "SELECT 1 FROM vala.cluster_nodes \
             WHERE data_tenant_id=$1 AND node_id=$2 AND role=$3 FOR UPDATE",
        )
        .bind(uuid::Uuid::from(conn.data_tenant_id()))
        .bind(registration.key.node_id.as_uuid())
        .bind(role)
        .fetch_optional(&mut **conn.transaction())
        .await
        .map_err(SqlError::from)?;
        let row = sqlx::query_as::<_, ClusterNodeDbRow>(
            r#"INSERT INTO vala.cluster_nodes
               (data_tenant_id, node_id, role, advertise_addr, fencing_token, started_at, heartbeat_at,
                capability_version, capabilities, ready)
               VALUES ($1,$2,$3,$4,1,$5,statement_timestamp(),1,$6,false)
               ON CONFLICT (data_tenant_id, node_id, role) DO UPDATE SET
                 advertise_addr=EXCLUDED.advertise_addr,
                 fencing_token=vala.cluster_nodes.fencing_token + 1,
                 started_at=EXCLUDED.started_at, heartbeat_at=statement_timestamp(),
                 capability_version=1, capabilities=EXCLUDED.capabilities, ready=false
               RETURNING node_id, role, advertise_addr, fencing_token, capability_version,
                         capabilities, ready, started_at, heartbeat_at"#,
        )
        .bind(uuid::Uuid::from(conn.data_tenant_id()))
        .bind(registration.key.node_id.as_uuid())
        .bind(role)
        .bind(&registration.address)
        .bind(registration.started_at)
        .bind(capabilities)
        .fetch_one(&mut **conn.transaction())
        .await
        .map_err(SqlError::from)?;
        Ok(RegisteredRoleRow {
            lease: row.try_into()?,
        })
    }

    /// Applies a fenced heartbeat and capability/readiness update.
    ///
    /// # Errors
    /// Returns [`SqlError`] for invalid capability data or database failures.
    pub async fn heartbeat(
        &self,
        conn: &mut TenantConn<'_>,
        key: &ClusterNodeKey,
        fence: FencingToken,
        ready: bool,
        capabilities: &ClusterCapabilities,
    ) -> Result<RoleMutation, SqlError> {
        validate_role(key.role, "membership", capabilities)?;
        let capabilities =
            serde_json::to_value(capabilities).map_err(|error| invariant(&error.to_string()))?;
        let result = sqlx::query(
            "UPDATE vala.cluster_nodes SET heartbeat_at=now(), ready=$5, capabilities=$6 \
             WHERE data_tenant_id=$1 AND node_id=$2 AND role=$3 AND fencing_token=$4",
        )
        .bind(uuid::Uuid::from(conn.data_tenant_id()))
        .bind(key.node_id.as_uuid())
        .bind(role_name(key.role))
        .bind(i64::try_from(fence).map_err(|_| invariant("fence exceeds i64"))?)
        .bind(ready)
        .bind(capabilities)
        .execute(&mut **conn.transaction())
        .await
        .map_err(SqlError::from)?;
        Ok(mutation(result.rows_affected()))
    }

    /// Marks exactly one matching role fence not ready while retaining its epoch.
    ///
    /// The retained row is the durable monotonic-fence tombstone used by the
    /// next registration UPSERT. Ready/fresh discovery excludes it immediately.
    ///
    /// # Errors
    /// Returns [`SqlError`] for fence overflow or database failures.
    pub async fn unregister(
        &self,
        conn: &mut TenantConn<'_>,
        key: &ClusterNodeKey,
        fence: FencingToken,
    ) -> Result<RoleMutation, SqlError> {
        let result = sqlx::query(
            "UPDATE vala.cluster_nodes SET ready=false, heartbeat_at=now() \
             WHERE data_tenant_id=$1 AND node_id=$2 AND role=$3 AND fencing_token=$4",
        )
        .bind(uuid::Uuid::from(conn.data_tenant_id()))
        .bind(key.node_id.as_uuid())
        .bind(role_name(key.role))
        .bind(i64::try_from(fence).map_err(|_| invariant("fence exceeds i64"))?)
        .execute(&mut **conn.transaction())
        .await
        .map_err(SqlError::from)?;
        Ok(mutation(result.rows_affected()))
    }

    /// Lists ready, fresh, valid role leases.
    ///
    /// Malformed rows fail closed with an error rather than becoming capacity.
    ///
    /// # Errors
    /// Returns [`SqlError`] for malformed row/capability data or database failures.
    pub async fn list_live(
        &self,
        conn: &mut TenantConn<'_>,
        role: ClusterRole,
        liveness: Duration,
    ) -> Result<Vec<ClusterRoleLease>, SqlError> {
        let rows = sqlx::query_as::<_, ClusterNodeDbRow>(
            "SELECT node_id, role, advertise_addr, fencing_token, capability_version, \
             capabilities, ready, started_at, heartbeat_at FROM vala.cluster_nodes \
             WHERE data_tenant_id=$1 AND role=$2 AND ready=true \
             AND heartbeat_at >= statement_timestamp() - ($3 * interval '1 second') \
             ORDER BY node_id",
        )
        .bind(uuid::Uuid::from(conn.data_tenant_id()))
        .bind(role_name(role))
        .bind(liveness.as_secs_f64())
        .fetch_all(&mut **conn.transaction())
        .await
        .map_err(SqlError::from)?;
        rows.into_iter().map(TryInto::try_into).collect()
    }
}

/// Raw membership projection decoded and validated before becoming capacity.
#[derive(sqlx::FromRow)]
struct ClusterNodeDbRow {
    /// Physical node UUID persisted by membership.
    node_id: uuid::Uuid,
    /// Closed database role discriminator.
    role: String,
    /// Private service address.
    advertise_addr: String,
    /// Monotonic role-specific fence.
    fencing_token: i64,
    /// Capability document schema version.
    capability_version: i16,
    /// Typed capability JSON awaiting fail-closed decoding.
    capabilities: serde_json::Value,
    /// Admission readiness flag.
    ready: bool,
    /// Role boot timestamp.
    started_at: DateTime<Utc>,
    /// Latest fenced heartbeat timestamp.
    heartbeat_at: DateTime<Utc>,
}

impl TryFrom<ClusterNodeDbRow> for ClusterRoleLease {
    type Error = SqlError;

    /// Decodes one stored membership row and validates every capability.
    ///
    /// # Errors
    /// Returns [`SqlError`] for malformed roles, fences, versions, or JSON.
    fn try_from(row: ClusterNodeDbRow) -> Result<Self, Self::Error> {
        let role = parse_role(&row.role)?;
        let capabilities: ClusterCapabilities = serde_json::from_value(row.capabilities)
            .map_err(|error| invariant(&error.to_string()))?;
        validate_role(role, &row.advertise_addr, &capabilities)?;
        Ok(Self {
            key: ClusterNodeKey {
                node_id: row.node_id.into(),
                role,
            },
            address: row.advertise_addr,
            fencing_token: u64::try_from(row.fencing_token)
                .map_err(|_| invariant("negative membership fence"))?,
            capability_version: u16::try_from(row.capability_version)
                .map_err(|_| invariant("negative capability version"))?,
            capabilities,
            ready: row.ready,
            started_at: row.started_at,
            heartbeat_at: row.heartbeat_at,
        })
    }
}

/// Validates role/address/capability consistency before persistence or use.
///
/// # Errors
/// Returns [`SqlError`] for empty addresses, role mismatch, unsupported
/// versions, or invalid capacity.
fn validate_role(
    role: ClusterRole,
    address: &str,
    capabilities: &ClusterCapabilities,
) -> Result<(), SqlError> {
    if address.trim().is_empty() {
        return Err(invariant("cluster role address is empty"));
    }
    capabilities
        .validate_for_role(role)
        .map_err(|error| invariant(&error.to_string()))
}

/// Maps one closed role to its durable SQL discriminator.
fn role_name(role: ClusterRole) -> &'static str {
    match role {
        ClusterRole::Scribe => "scribe",
        ClusterRole::Oracle => "oracle",
    }
}

/// Parses one durable SQL role discriminator.
///
/// # Errors
/// Returns [`SqlError`] for unknown stored role text.
fn parse_role(role: &str) -> Result<ClusterRole, SqlError> {
    match role {
        "scribe" => Ok(ClusterRole::Scribe),
        "oracle" => Ok(ClusterRole::Oracle),
        _ => Err(invariant("unknown cluster role")),
    }
}

/// Maps affected-row count to the fenced mutation outcome.
fn mutation(rows: u64) -> RoleMutation {
    if rows == 0 {
        RoleMutation::StaleFence
    } else {
        RoleMutation::Applied
    }
}

/// Constructs a control-plane invariant error without leaking row data.
fn invariant(detail: &str) -> SqlError {
    SqlError::InvariantViolation {
        detail: detail.to_owned(),
    }
}
