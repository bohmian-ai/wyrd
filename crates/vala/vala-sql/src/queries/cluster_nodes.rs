//! Operator-owned role membership workflows.

use chrono::{DateTime, Utc};
use wyrd_spec::vala::api::{
    ClusterCapabilities, ClusterNodeKey, ClusterRole, ClusterRoleLease, FencingToken,
};

use crate::row_types::cluster_nodes::{RegisteredRoleRow, RoleMutation, RoleRegistration};
use crate::{OperatorPool, SqlError};

/// Operator-pool owner for role-fenced cluster membership.
pub struct ClusterNodes {
    /// Audited operator-role pool used for cluster-global membership state.
    pool: OperatorPool,
}

impl ClusterNodes {
    /// Constructs the membership owner.
    #[must_use]
    pub fn new(pool: OperatorPool) -> Self {
        Self { pool }
    }

    /// Registers one role and atomically advances only its composite fence.
    ///
    /// # Errors
    /// Returns [`SqlError`] for invalid capabilities/address, overflow, JSON,
    /// or database failures.
    pub async fn register(
        &self,
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
        let row = sqlx::query_as::<_, ClusterNodeDbRow>(
            r#"INSERT INTO vala.cluster_nodes
               (node_id, role, advertise_addr, fencing_token, started_at, heartbeat_at,
                capability_version, capabilities, ready)
               VALUES ($1,$2,$3,1,$4,$4,1,$5,false)
               ON CONFLICT (node_id, role) DO UPDATE SET
                 advertise_addr=EXCLUDED.advertise_addr,
                 fencing_token=vala.cluster_nodes.fencing_token + 1,
                 started_at=EXCLUDED.started_at, heartbeat_at=EXCLUDED.heartbeat_at,
                 capability_version=1, capabilities=EXCLUDED.capabilities, ready=false
               RETURNING node_id, role, advertise_addr, fencing_token, capability_version,
                         capabilities, ready, started_at, heartbeat_at"#,
        )
        .bind(registration.key.node_id.as_uuid())
        .bind(role)
        .bind(&registration.address)
        .bind(registration.started_at)
        .bind(capabilities)
        .fetch_one(self.pool.pool())
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
        key: &ClusterNodeKey,
        fence: FencingToken,
        ready: bool,
        capabilities: &ClusterCapabilities,
    ) -> Result<RoleMutation, SqlError> {
        validate_role(key.role, "membership", capabilities)?;
        let capabilities =
            serde_json::to_value(capabilities).map_err(|error| invariant(&error.to_string()))?;
        let result = sqlx::query(
            "UPDATE vala.cluster_nodes SET heartbeat_at=now(), ready=$4, capabilities=$5 \
             WHERE node_id=$1 AND role=$2 AND fencing_token=$3",
        )
        .bind(key.node_id.as_uuid())
        .bind(role_name(key.role))
        .bind(i64::try_from(fence).map_err(|_| invariant("fence exceeds i64"))?)
        .bind(ready)
        .bind(capabilities)
        .execute(self.pool.pool())
        .await
        .map_err(SqlError::from)?;
        Ok(mutation(result.rows_affected()))
    }

    /// Removes exactly one matching role fence.
    ///
    /// # Errors
    /// Returns [`SqlError`] for fence overflow or database failures.
    pub async fn unregister(
        &self,
        key: &ClusterNodeKey,
        fence: FencingToken,
    ) -> Result<RoleMutation, SqlError> {
        let result = sqlx::query(
            "DELETE FROM vala.cluster_nodes WHERE node_id=$1 AND role=$2 AND fencing_token=$3",
        )
        .bind(key.node_id.as_uuid())
        .bind(role_name(key.role))
        .bind(i64::try_from(fence).map_err(|_| invariant("fence exceeds i64"))?)
        .execute(self.pool.pool())
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
        role: ClusterRole,
        heartbeat_after: DateTime<Utc>,
    ) -> Result<Vec<ClusterRoleLease>, SqlError> {
        let rows = sqlx::query_as::<_, ClusterNodeDbRow>(
            "SELECT node_id, role, advertise_addr, fencing_token, capability_version, \
             capabilities, ready, started_at, heartbeat_at FROM vala.cluster_nodes \
             WHERE role=$1 AND ready=true AND heartbeat_at >= $2 ORDER BY node_id",
        )
        .bind(role_name(role))
        .bind(heartbeat_after)
        .fetch_all(self.pool.pool())
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
