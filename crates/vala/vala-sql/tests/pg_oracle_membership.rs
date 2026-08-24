//! PostgreSQL integration coverage for Oracle membership and role fencing.

use chrono::{Duration, Utc};
use vala_sql::{
    ValaPostgres,
    queries::cluster_nodes::ClusterNodes as SqlClusterNodes,
    row_types::cluster_nodes::{RoleMutation, RoleRegistration},
};
use wyrd_dev_fixtures::pg::PgFixture;
use wyrd_spec::{
    DataTenantId,
    vala::api::{
        ClusterCapabilities, ClusterNodeKey, ClusterRole, NodeId, QueryClass, ScribeCapabilitiesV1,
    },
};

/// Test workflow owner that supplies one platform-tenant transaction per membership operation.
struct ClusterNodes {
    /// Tenant-aware SQL owner under test.
    inner: SqlClusterNodes,
}

impl ClusterNodes {
    /// Creates a membership test owner over the fixture's Vala runtime pool.
    ///
    /// # Panics
    ///
    /// This constructor does not perform fallible work and cannot panic.
    fn new(postgres: ValaPostgres) -> Self {
        Self {
            inner: SqlClusterNodes::new(postgres),
        }
    }

    /// Registers one role and commits the caller-owned transaction.
    ///
    /// # Errors
    ///
    /// Returns the SQL error from tenant connection setup, registration, or commit.
    async fn register(
        &self,
        registration: &RoleRegistration,
    ) -> Result<vala_sql::row_types::cluster_nodes::RegisteredRoleRow, vala_sql::SqlError> {
        let mut conn = self
            .inner
            .postgres()
            .tenant_conn(DataTenantId::SYSTEM_OWNER)
            .await?;
        let row = self.inner.register(&mut conn, registration).await?;
        conn.commit().await?;
        Ok(row)
    }

    /// Applies one heartbeat and commits the caller-owned transaction.
    ///
    /// # Errors
    ///
    /// Returns the SQL error from tenant connection setup, heartbeat, or commit.
    async fn heartbeat(
        &self,
        key: &ClusterNodeKey,
        fence: u64,
        ready: bool,
        capabilities: &ClusterCapabilities,
    ) -> Result<RoleMutation, vala_sql::SqlError> {
        let mut conn = self
            .inner
            .postgres()
            .tenant_conn(DataTenantId::SYSTEM_OWNER)
            .await?;
        let mutation = self
            .inner
            .heartbeat(&mut conn, key, fence, ready, capabilities)
            .await?;
        conn.commit().await?;
        Ok(mutation)
    }

    /// Unregisters one role and commits the caller-owned transaction.
    ///
    /// # Errors
    ///
    /// Returns the SQL error from tenant connection setup, unregistration, or commit.
    async fn unregister(
        &self,
        key: &ClusterNodeKey,
        fence: u64,
    ) -> Result<RoleMutation, vala_sql::SqlError> {
        let mut conn = self
            .inner
            .postgres()
            .tenant_conn(DataTenantId::SYSTEM_OWNER)
            .await?;
        let mutation = self.inner.unregister(&mut conn, key, fence).await?;
        conn.commit().await?;
        Ok(mutation)
    }

    /// Lists live roles and commits the caller-owned read transaction.
    ///
    /// # Errors
    ///
    /// Returns the SQL error from tenant connection setup, discovery, or commit.
    async fn list_live(
        &self,
        role: ClusterRole,
        heartbeat_after: chrono::DateTime<Utc>,
    ) -> Result<Vec<wyrd_spec::vala::api::ClusterRoleLease>, vala_sql::SqlError> {
        let mut conn = self
            .inner
            .postgres()
            .tenant_conn(DataTenantId::SYSTEM_OWNER)
            .await?;
        let rows = self
            .inner
            .list_live(&mut conn, role, heartbeat_after)
            .await?;
        conn.commit().await?;
        Ok(rows)
    }

    /// Ages a registered node heartbeat to exercise the discovery cutoff.
    ///
    /// # Errors
    ///
    /// Returns the SQL error from tenant connection setup, update, or commit.
    async fn age_heartbeat(
        &self,
        node_id: NodeId,
        heartbeat_at: chrono::DateTime<Utc>,
    ) -> Result<(), vala_sql::SqlError> {
        let mut conn = self
            .inner
            .postgres()
            .tenant_conn(DataTenantId::SYSTEM_OWNER)
            .await?;
        sqlx::query(
            "UPDATE vala.cluster_nodes SET heartbeat_at=$2 \
             WHERE data_tenant_id=$1 AND node_id=$3",
        )
        .bind(uuid::Uuid::from(conn.data_tenant_id()))
        .bind(heartbeat_at)
        .bind(node_id.as_uuid())
        .execute(&mut **conn.transaction())
        .await
        .map_err(vala_sql::SqlError::from)?;
        conn.commit().await?;
        Ok(())
    }
}

/// Starts an isolated database and seeds one tenant.
///
/// # Panics
///
/// Panics when the repository Postgres fixture cannot start or the tenant cannot be seeded.
async fn setup() -> (PgFixture, DataTenantId) {
    let fixture = PgFixture::start().await.expect("fixture starts");
    let tenant = DataTenantId::new_v7();
    fixture
        .seed_additional_tenant_with_uuid(tenant, &format!("oracle-{}", tenant.as_uuid().simple()))
        .await
        .expect("tenant seeds");
    (fixture, tenant)
}

/// Proves one physical node receives independent role fences.
///
/// # Panics
///
/// Panics when the fixture or any membership operation fails its test invariant.
#[tokio::test]
async fn same_node_roles_have_independent_fences() {
    let (fixture, _) = setup().await;
    let owner = ClusterNodes::new(fixture.vala_postgres().clone());
    let node_id = NodeId::new(uuid::Uuid::now_v7());
    let now = Utc::now();
    let scribe = RoleRegistration {
        key: ClusterNodeKey {
            node_id,
            role: ClusterRole::Scribe,
        },
        address: "http://scribe:5001".into(),
        capabilities: ClusterCapabilities::ScribeV1(ScribeCapabilitiesV1 {
            tail_protocol_version: 1,
        }),
        started_at: now,
    };
    let oracle = RoleRegistration {
        key: ClusterNodeKey {
            node_id,
            role: ClusterRole::Oracle,
        },
        address: "http://oracle:5002".into(),
        capabilities: ClusterCapabilities::OracleV1(wyrd_spec::vala::api::OracleCapabilitiesV1 {
            peer_protocol_version: 2,
            storage_protocol_version: 1,
            cpu_cores: 4.0,
            memory_budget_bytes: 4096,
            cpu_cores_per_slot: 1.0,
            memory_bytes_per_slot: 1024,
            raw_slots: 4,
            usable_slots: 3,
            supported_classes: vec![QueryClass::Interactive],
            max_workers_per_query: 3,
        }),
        started_at: now,
    };
    let first_scribe = owner.register(&scribe).await.expect("scribe registers");
    let first_oracle = owner.register(&oracle).await.expect("oracle registers");
    let second_scribe = owner.register(&scribe).await.expect("scribe re-registers");
    assert_eq!(first_scribe.lease.fencing_token, 1);
    assert_eq!(first_oracle.lease.fencing_token, 1);
    assert_eq!(second_scribe.lease.fencing_token, 2);
    assert_eq!(
        owner
            .unregister(&oracle.key, first_oracle.lease.fencing_token)
            .await
            .expect("matching membership deletes"),
        RoleMutation::Applied
    );
}

/// Proves stale mutations fail and discovery excludes unready roles.
///
/// # Panics
///
/// Panics when the fixture or any membership operation fails its test invariant.
#[tokio::test]
async fn stale_role_mutation_is_rejected() {
    let (fixture, _) = setup().await;
    let owner = ClusterNodes::new(fixture.vala_postgres().clone());
    let registration = RoleRegistration {
        key: ClusterNodeKey {
            node_id: NodeId::new(uuid::Uuid::now_v7()),
            role: ClusterRole::Scribe,
        },
        address: "http://scribe:5001".into(),
        capabilities: ClusterCapabilities::ScribeV1(ScribeCapabilitiesV1 {
            tail_protocol_version: 1,
        }),
        started_at: Utc::now(),
    };
    owner.register(&registration).await.expect("role registers");
    assert_eq!(
        owner
            .heartbeat(&registration.key, 0, true, &registration.capabilities)
            .await
            .expect("stale heartbeat represented"),
        RoleMutation::StaleFence
    );
    assert!(
        owner
            .list_live(ClusterRole::Scribe, Utc::now() - Duration::seconds(30))
            .await
            .expect("unready role discovery")
            .is_empty(),
        "stale fenced heartbeat must not make the role live"
    );
}

/// Proves expired heartbeats are excluded from live membership discovery.
///
/// # Panics
///
/// Panics when the fixture or any membership operation fails its test invariant.
#[tokio::test]
async fn live_discovery_excludes_expired_heartbeat() {
    let (fixture, _) = setup().await;
    let owner = ClusterNodes::new(fixture.vala_postgres().clone());
    let registration = RoleRegistration {
        key: ClusterNodeKey {
            node_id: NodeId::new(uuid::Uuid::now_v7()),
            role: ClusterRole::Scribe,
        },
        address: "http://scribe:5001".into(),
        capabilities: ClusterCapabilities::ScribeV1(ScribeCapabilitiesV1 {
            tail_protocol_version: 1,
        }),
        started_at: Utc::now(),
    };
    owner.register(&registration).await.expect("role registers");
    owner
        .heartbeat(&registration.key, 1, true, &registration.capabilities)
        .await
        .expect("heartbeat applies");
    owner
        .age_heartbeat(
            registration.key.node_id,
            Utc::now() - Duration::seconds(120),
        )
        .await
        .expect("heartbeat ages");
    let live = owner
        .list_live(ClusterRole::Scribe, Utc::now() - Duration::seconds(30))
        .await
        .expect("live roles list");
    assert!(live.is_empty(), "expired heartbeat must not be live");
}
