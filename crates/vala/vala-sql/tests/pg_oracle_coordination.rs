//! PostgreSQL integration coverage for Oracle membership and admission.

use chrono::{Duration, Utc};
use vala_sql::{
    ValaPostgres,
    queries::{
        cluster_nodes::ClusterNodes as SqlClusterNodes,
        oracle_admission::OracleAdmissionLeases as SqlOracleAdmissionLeases,
    },
    row_types::{
        cluster_nodes::{RoleMutation, RoleRegistration},
        oracle_admission::{
            AdmissionAcquire, AdmissionReconcileScope, AdmissionRequest, LeaseMutation, RoleFence,
        },
    },
};

/// Test workflow owner that supplies one platform-tenant transaction per membership operation.
struct ClusterNodes {
    /// Tenant-aware SQL owner under test.
    inner: SqlClusterNodes,
}

impl ClusterNodes {
    /// Creates a membership test owner over the fixture's Vala runtime pool.
    fn new(postgres: ValaPostgres) -> Self {
        Self {
            inner: SqlClusterNodes::new(postgres),
        }
    }

    /// Registers one role and commits the caller-owned transaction.
    ///
    /// # Errors
    /// Returns [`vala_sql::SqlError`] when acquisition, registration, or commit fails.
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
    /// Returns [`vala_sql::SqlError`] when acquisition, mutation, or commit fails.
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
    /// Returns [`vala_sql::SqlError`] when acquisition, mutation, or commit fails.
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
    /// Returns [`vala_sql::SqlError`] when acquisition, discovery, or commit fails.
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
}

/// Test workflow owner that supplies one exact tenant transaction per admission operation.
struct OracleAdmissionLeases {
    /// Tenant-aware SQL owner under test.
    inner: SqlOracleAdmissionLeases,
    /// Tenant bound to every transaction opened by this test owner.
    tenant: DataTenantId,
}

impl OracleAdmissionLeases {
    /// Creates an admission test owner for one seeded tenant.
    fn new(postgres: ValaPostgres, tenant: DataTenantId) -> Self {
        Self {
            inner: SqlOracleAdmissionLeases::new(postgres),
            tenant,
        }
    }

    /// Acquires one lease and commits the caller-owned transaction.
    ///
    /// # Errors
    /// Returns [`vala_sql::SqlError`] when acquisition, admission, or commit fails.
    async fn acquire(
        &self,
        request: &AdmissionRequest,
    ) -> Result<AdmissionAcquire, vala_sql::SqlError> {
        let mut conn = self.inner.postgres().tenant_conn(self.tenant).await?;
        let outcome = self.inner.acquire(&mut conn, request).await?;
        conn.commit().await?;
        Ok(outcome)
    }

    /// Renews one lease and commits the caller-owned transaction.
    ///
    /// # Errors
    /// Returns [`vala_sql::SqlError`] when acquisition, renewal, or commit fails.
    async fn renew(
        &self,
        query_id: QueryId,
        leader: &RoleFence,
        expiry: chrono::DateTime<Utc>,
    ) -> Result<LeaseMutation, vala_sql::SqlError> {
        let mut conn = self.inner.postgres().tenant_conn(self.tenant).await?;
        let outcome = self
            .inner
            .renew(&mut conn, query_id, leader, expiry)
            .await?;
        conn.commit().await?;
        Ok(outcome)
    }

    /// Releases one lease and commits the caller-owned transaction.
    ///
    /// # Errors
    /// Returns [`vala_sql::SqlError`] when acquisition, release, or commit fails.
    async fn release(
        &self,
        query_id: QueryId,
        leader: &RoleFence,
    ) -> Result<LeaseMutation, vala_sql::SqlError> {
        let mut conn = self.inner.postgres().tenant_conn(self.tenant).await?;
        let outcome = self.inner.release(&mut conn, query_id, leader).await?;
        conn.commit().await?;
        Ok(outcome)
    }

    /// Expires one lease batch and commits the caller-owned transaction.
    ///
    /// # Errors
    /// Returns [`vala_sql::SqlError`] when acquisition, expiry, or commit fails.
    async fn expire_batch(
        &self,
        now: chrono::DateTime<Utc>,
        max_leases: u16,
    ) -> Result<vala_sql::row_types::oracle_admission::AdmissionExpiryReport, vala_sql::SqlError>
    {
        let mut conn = self.inner.postgres().tenant_conn(self.tenant).await?;
        let report = self.inner.expire_batch(&mut conn, now, max_leases).await?;
        conn.commit().await?;
        Ok(report)
    }

    /// Reconciles bounded scopes and commits the caller-owned transaction.
    ///
    /// # Errors
    /// Returns [`vala_sql::SqlError`] when acquisition, reconciliation, or commit fails.
    async fn reconcile_scopes(
        &self,
        scopes: &[AdmissionReconcileScope],
        now: chrono::DateTime<Utc>,
    ) -> Result<
        vala_sql::row_types::oracle_admission::AdmissionReconciliationReport,
        vala_sql::SqlError,
    > {
        let mut conn = self.inner.postgres().tenant_conn(self.tenant).await?;
        let report = self.inner.reconcile_scopes(&mut conn, scopes, now).await?;
        conn.commit().await?;
        Ok(report)
    }
}
use wyrd_dev_fixtures::pg::PgFixture;
use wyrd_spec::{
    DataTenantId,
    vala::api::{
        AdmissionScope, ClusterCapabilities, ClusterNodeKey, ClusterRole, NodeId,
        OracleAdmissionLease, QueryClass, QueryId, ScribeCapabilitiesV1,
    },
};

/// Starts an isolated database and seeds one tenant.
async fn setup() -> (PgFixture, DataTenantId) {
    let fixture = PgFixture::start().await.expect("fixture starts");
    let tenant = DataTenantId::new_v7();
    fixture
        .seed_additional_tenant_with_uuid(tenant, &format!("oracle-{}", tenant.as_uuid().simple()))
        .await
        .expect("tenant seeds");
    (fixture, tenant)
}

/// Builds a valid admission request with caller-selected timing and demand.
fn request(
    tenant: DataTenantId,
    query_id: QueryId,
    leader: NodeId,
    fence: u64,
    slots: u32,
    expires_at: chrono::DateTime<Utc>,
) -> AdmissionRequest {
    AdmissionRequest {
        lease: OracleAdmissionLease {
            query_id,
            data_tenant_id: tenant,
            query_class: QueryClass::Interactive,
            slot_units: slots,
            selected_node_ids: vec![leader],
            leader_node_id: leader,
            leader_fencing_token: fence,
            acquired_at: Utc::now(),
            expires_at,
        },
        cluster_limit: slots,
        class_limit: slots,
        tenant_limit: slots,
    }
}

/// Proves durable admission tenant columns reference the platform tenant catalog.
#[tokio::test]
async fn admission_tables_reference_platform_tenants() {
    let (fixture, _) = setup().await;
    let owner = fixture.superuser_pool().await.expect("table-owner pool");
    for table in ["oracle_admission_leases", "oracle_admission_accounting"] {
        let foreign_keys: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM pg_constraint constraint_row \
             JOIN pg_class table_row ON table_row.oid=constraint_row.conrelid \
             JOIN pg_namespace schema_row ON schema_row.oid=table_row.relnamespace \
             WHERE schema_row.nspname='vala' AND table_row.relname=$1 \
             AND constraint_row.contype='f' \
             AND constraint_row.confrelid='platform.tenants'::regclass",
        )
        .bind(table)
        .fetch_one(&owner)
        .await
        .expect("foreign-key metadata reads");
        assert_eq!(foreign_keys, 1, "{table} references platform.tenants");
    }
}

/// Proves one physical node receives independent role fences.
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
            peer_protocol_version: 1,
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
            .expect("operator role can delete matching membership"),
        RoleMutation::Applied
    );
    let mut conn = fixture
        .vala_postgres()
        .tenant_conn(DataTenantId::SYSTEM_OWNER)
        .await
        .expect("system tenant connection opens");
    let scribe_rows: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM vala.cluster_nodes \
         WHERE data_tenant_id=$1 AND node_id=$2 AND role='scribe'",
    )
    .bind(uuid::Uuid::from(DataTenantId::SYSTEM_OWNER))
    .bind(node_id.as_uuid())
    .fetch_one(&mut **conn.transaction())
    .await
    .expect("Scribe role reads");
    conn.commit().await.expect("system tenant read commits");
    assert_eq!(scribe_rows, 1);
}

/// Proves stale mutations fail and discovery excludes unready roles.
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
            .expect("stale heartbeat is represented"),
        RoleMutation::StaleFence
    );
    assert!(
        owner
            .list_live(ClusterRole::Scribe, Utc::now() - Duration::minutes(1))
            .await
            .expect("discovery succeeds")
            .is_empty()
    );
}

/// Proves concurrent admission serializes the cluster counter.
#[tokio::test]
async fn concurrent_admission_never_exceeds_scope_limits() {
    let (fixture, tenant) = setup().await;
    let leader = NodeId::new(uuid::Uuid::now_v7());
    let first = OracleAdmissionLeases::new(fixture.vala_postgres().clone(), tenant);
    let second = OracleAdmissionLeases::new(fixture.vala_postgres().clone(), tenant);
    let expiry = Utc::now() + Duration::minutes(5);
    let one = request(
        tenant,
        QueryId::new(uuid::Uuid::now_v7()),
        leader,
        1,
        1,
        expiry,
    );
    let two = request(
        tenant,
        QueryId::new(uuid::Uuid::now_v7()),
        leader,
        1,
        1,
        expiry,
    );
    let (a, b) = tokio::join!(first.acquire(&one), second.acquire(&two));
    let acquired = [a.expect("first admission"), b.expect("second admission")]
        .into_iter()
        .filter(|outcome| matches!(outcome, AdmissionAcquire::Acquired(_)))
        .count();

    assert_eq!(acquired, 1);
}

/// Proves two request tenants serialize on one deployment-wide cluster limit.
#[tokio::test]
async fn concurrent_two_tenant_admission_never_exceeds_cluster_limit() {
    let (fixture, tenant_a) = setup().await;
    let tenant_b = DataTenantId::new_v7();
    fixture
        .seed_additional_tenant_with_uuid(
            tenant_b,
            &format!("oracle-{}", tenant_b.as_uuid().simple()),
        )
        .await
        .expect("second tenant seeds");
    let leader = NodeId::new(uuid::Uuid::now_v7());
    let first = OracleAdmissionLeases::new(fixture.vala_postgres().clone(), tenant_a);
    let second = OracleAdmissionLeases::new(fixture.vala_postgres().clone(), tenant_b);
    let expiry = Utc::now() + Duration::minutes(5);
    let mut request_a = request(
        tenant_a,
        QueryId::new(uuid::Uuid::now_v7()),
        leader,
        1,
        1,
        expiry,
    );
    let mut request_b = request(
        tenant_b,
        QueryId::new(uuid::Uuid::now_v7()),
        leader,
        1,
        1,
        expiry,
    );
    request_a.class_limit = 2;
    request_a.tenant_limit = 2;
    request_b.class_limit = 2;
    request_b.tenant_limit = 2;

    let (a, b) = tokio::join!(first.acquire(&request_a), second.acquire(&request_b));
    let acquired = [
        a.expect("tenant A admission"),
        b.expect("tenant B admission"),
    ]
    .into_iter()
    .filter(|outcome| matches!(outcome, AdmissionAcquire::Acquired(_)))
    .count();
    assert_eq!(acquired, 1);

    let mut conn = fixture
        .vala_postgres()
        .tenant_conn(tenant_a)
        .await
        .expect("tenant connection opens");
    let used: i64 = sqlx::query_scalar(
        "SELECT used_slots FROM vala.oracle_admission_accounting \
         WHERE data_tenant_id=$1 AND scope_kind='cluster' AND scope_key='global' \
         AND accounting_class='all'",
    )
    .bind(uuid::Uuid::from(DataTenantId::SYSTEM_OWNER))
    .fetch_one(&mut **conn.transaction())
    .await
    .expect("global counter reads");
    let foreign_rows: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM vala.oracle_admission_accounting WHERE data_tenant_id=$1",
    )
    .bind(uuid::Uuid::from(tenant_b))
    .fetch_one(&mut **conn.transaction())
    .await
    .expect("foreign counter visibility reads");
    conn.commit().await.expect("global counter read commits");
    assert_eq!(used, 1);
    assert_eq!(foreign_rows, 0);
}

/// Proves two request tenants serialize on one deployment-wide class limit.
#[tokio::test]
async fn concurrent_two_tenant_same_class_never_exceeds_class_limit() {
    let (fixture, tenant_a) = setup().await;
    let tenant_b = DataTenantId::new_v7();
    fixture
        .seed_additional_tenant_with_uuid(
            tenant_b,
            &format!("oracle-{}", tenant_b.as_uuid().simple()),
        )
        .await
        .expect("second tenant seeds");
    let leader = NodeId::new(uuid::Uuid::now_v7());
    let first = OracleAdmissionLeases::new(fixture.vala_postgres().clone(), tenant_a);
    let second = OracleAdmissionLeases::new(fixture.vala_postgres().clone(), tenant_b);
    let expiry = Utc::now() + Duration::minutes(5);
    let mut request_a = request(
        tenant_a,
        QueryId::new(uuid::Uuid::now_v7()),
        leader,
        1,
        1,
        expiry,
    );
    let mut request_b = request(
        tenant_b,
        QueryId::new(uuid::Uuid::now_v7()),
        leader,
        1,
        1,
        expiry,
    );
    request_a.cluster_limit = 2;
    request_a.tenant_limit = 2;
    request_b.cluster_limit = 2;
    request_b.tenant_limit = 2;

    let (a, b) = tokio::join!(first.acquire(&request_a), second.acquire(&request_b));
    let outcomes = [
        a.expect("tenant A admission"),
        b.expect("tenant B admission"),
    ];
    assert_eq!(
        outcomes
            .iter()
            .filter(|outcome| matches!(outcome, AdmissionAcquire::Acquired(_)))
            .count(),
        1
    );
    assert_eq!(
        outcomes
            .iter()
            .filter(|outcome| {
                matches!(
                    outcome,
                    AdmissionAcquire::Rejected {
                        scope: AdmissionScope::Class,
                        ..
                    }
                )
            })
            .count(),
        1
    );

    let mut conn = fixture
        .vala_postgres()
        .tenant_conn(tenant_a)
        .await
        .expect("tenant connection opens");
    let used: i64 = sqlx::query_scalar(
        "SELECT used_slots FROM vala.oracle_admission_accounting \
         WHERE data_tenant_id=$1 AND scope_kind='class' AND scope_key='global' \
         AND accounting_class='interactive'",
    )
    .bind(uuid::Uuid::from(DataTenantId::SYSTEM_OWNER))
    .fetch_one(&mut **conn.transaction())
    .await
    .expect("shared class counter reads");
    let foreign_tenant_rows: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM vala.oracle_admission_accounting \
         WHERE data_tenant_id=$1 AND scope_kind='tenant'",
    )
    .bind(uuid::Uuid::from(tenant_b))
    .fetch_one(&mut **conn.transaction())
    .await
    .expect("foreign tenant-counter visibility reads");
    conn.commit().await.expect("accounting reads commit");
    assert_eq!(used, 1);
    assert_eq!(foreign_tenant_rows, 0);
}

/// Proves release is idempotent while a mismatched fence remains stale.
#[tokio::test]
async fn release_is_idempotent_and_fenced() {
    let (fixture, tenant) = setup().await;
    let owner = OracleAdmissionLeases::new(fixture.vala_postgres().clone(), tenant);
    let leader = NodeId::new(uuid::Uuid::now_v7());
    let query_id = QueryId::new(uuid::Uuid::now_v7());
    owner
        .acquire(&request(
            tenant,
            query_id,
            leader,
            7,
            1,
            Utc::now() + Duration::minutes(5),
        ))
        .await
        .expect("admission succeeds");
    assert!(matches!(
        owner
            .release(
                query_id,
                &RoleFence {
                    node_id: leader,
                    fencing_token: 6,
                },
            )
            .await
            .expect("stale release is represented"),
        LeaseMutation::StaleLeaderFence
    ));
    let fence = RoleFence {
        node_id: leader,
        fencing_token: 7,
    };
    assert!(matches!(
        owner
            .release(query_id, &fence)
            .await
            .expect("release succeeds"),
        LeaseMutation::Released
    ));
    assert!(matches!(
        owner
            .release(query_id, &fence)
            .await
            .expect("repeat release succeeds"),
        LeaseMutation::AlreadyReleased
    ));
}

/// Proves expiry reclaims one lease exactly once.
#[tokio::test]
async fn expiry_reclaims_once() {
    let (fixture, tenant) = setup().await;
    let owner = OracleAdmissionLeases::new(fixture.vala_postgres().clone(), tenant);
    owner
        .acquire(&request(
            tenant,
            QueryId::new(uuid::Uuid::now_v7()),
            NodeId::new(uuid::Uuid::now_v7()),
            1,
            2,
            Utc::now() + Duration::milliseconds(10),
        ))
        .await
        .expect("admission succeeds");
    let cutoff = Utc::now() + Duration::seconds(1);
    let first = owner
        .expire_batch(cutoff, 8)
        .await
        .expect("expiry succeeds");
    let second = owner
        .expire_batch(cutoff, 8)
        .await
        .expect("repeat expiry succeeds");

    assert_eq!((first.expired_leases, first.released_slots), (1, 2));
    assert_eq!((second.expired_leases, second.released_slots), (0, 0));
}

/// Proves reconciliation restores one corrupted request-tenant counter.
#[tokio::test]
async fn reconciliation_repairs_interrupted_counter() {
    let (fixture, tenant) = setup().await;
    let owner = OracleAdmissionLeases::new(fixture.vala_postgres().clone(), tenant);
    owner
        .acquire(&request(
            tenant,
            QueryId::new(uuid::Uuid::now_v7()),
            NodeId::new(uuid::Uuid::now_v7()),
            1,
            2,
            Utc::now() + Duration::minutes(5),
        ))
        .await
        .expect("admission succeeds");
    let mut conn = fixture
        .vala_postgres()
        .tenant_conn(tenant)
        .await
        .expect("tenant connection opens");
    sqlx::query(
        "UPDATE vala.oracle_admission_accounting SET used_slots=99 \
         WHERE data_tenant_id=$1 AND scope_kind='tenant' AND scope_key=$2 \
         AND accounting_class='interactive'",
    )
    .bind(uuid::Uuid::from(tenant))
    .bind(tenant.to_string())
    .execute(&mut **conn.transaction())
    .await
    .expect("counter corrupts");
    conn.commit().await.expect("counter corruption commits");

    let report = owner
        .reconcile_scopes(
            &[AdmissionReconcileScope::Tenant {
                data_tenant_id: tenant,
                query_class: QueryClass::Interactive,
            }],
            Utc::now(),
        )
        .await
        .expect("reconciliation succeeds");
    let mut conn = fixture
        .vala_postgres()
        .tenant_conn(tenant)
        .await
        .expect("tenant connection opens");
    let used: i64 = sqlx::query_scalar(
        "SELECT used_slots FROM vala.oracle_admission_accounting \
         WHERE data_tenant_id=$1 AND scope_kind='tenant' AND scope_key=$2 \
         AND accounting_class='interactive'",
    )
    .bind(uuid::Uuid::from(tenant))
    .bind(tenant.to_string())
    .fetch_one(&mut **conn.transaction())
    .await
    .expect("counter reads");
    conn.commit().await.expect("counter read commits");

    assert_eq!(report.repaired_scopes, 1);
    assert_eq!(used, 2);
}

/// Proves startup recovery atomically expires crash leftovers and rebuilds all shared scopes.
#[tokio::test]
async fn pg_oracle_startup_reconciles_shared_scopes() {
    let (fixture, tenant) = setup().await;
    let tenant_b = DataTenantId::new_v7();
    fixture
        .seed_additional_tenant_with_uuid(tenant_b, "oracle-recovery-second")
        .await
        .expect("second tenant seeds");
    let owner = OracleAdmissionLeases::new(fixture.vala_postgres().clone(), tenant);
    let owner_b = OracleAdmissionLeases::new(fixture.vala_postgres().clone(), tenant_b);
    let leader = NodeId::new(uuid::Uuid::now_v7());
    let now = Utc::now();
    let mut interactive = request(
        tenant,
        QueryId::new(uuid::Uuid::now_v7()),
        leader,
        1,
        2,
        now + Duration::minutes(5),
    );
    interactive.cluster_limit = 20;
    interactive.class_limit = 20;
    interactive.tenant_limit = 20;
    owner
        .acquire(&interactive)
        .await
        .expect("interactive lease");
    let mut analytical = request(
        tenant,
        QueryId::new(uuid::Uuid::now_v7()),
        leader,
        1,
        3,
        now + Duration::minutes(5),
    );
    analytical.lease.query_class = QueryClass::Analytical;
    analytical.cluster_limit = 20;
    analytical.class_limit = 20;
    analytical.tenant_limit = 20;
    owner.acquire(&analytical).await.expect("analytical lease");
    let mut expired = request(
        tenant,
        QueryId::new(uuid::Uuid::now_v7()),
        leader,
        1,
        4,
        now + Duration::seconds(1),
    );
    expired.cluster_limit = 20;
    expired.class_limit = 20;
    expired.tenant_limit = 20;
    owner.acquire(&expired).await.expect("crash-leftover lease");
    let mut second_tenant_active = request(
        tenant_b,
        QueryId::new(uuid::Uuid::now_v7()),
        leader,
        1,
        7,
        now + Duration::minutes(5),
    );
    second_tenant_active.cluster_limit = 20;
    second_tenant_active.class_limit = 20;
    second_tenant_active.tenant_limit = 20;
    owner_b
        .acquire(&second_tenant_active)
        .await
        .expect("second tenant active lease");
    let mut second_tenant_expired = request(
        tenant_b,
        QueryId::new(uuid::Uuid::now_v7()),
        leader,
        1,
        1,
        now + Duration::seconds(1),
    );
    second_tenant_expired.cluster_limit = 20;
    second_tenant_expired.class_limit = 20;
    second_tenant_expired.tenant_limit = 20;
    owner_b
        .acquire(&second_tenant_expired)
        .await
        .expect("second tenant expired lease");
    let recovery_now = now + Duration::seconds(2);

    owner
        .inner
        .recover_shared_scopes(fixture.operator_pool(), recovery_now)
        .await
        .expect("startup recovery");

    let rows: Vec<(String, String, i64)> = sqlx::query_as(
        "SELECT scope_kind, accounting_class, used_slots \
         FROM vala.oracle_admission_accounting \
         WHERE data_tenant_id=$1 AND scope_key='global' ORDER BY scope_kind, accounting_class",
    )
    .bind(uuid::Uuid::from(DataTenantId::SYSTEM_OWNER))
    .fetch_all(fixture.operator_pool().pool())
    .await
    .expect("shared counters read");
    assert!(rows.contains(&("cluster".to_owned(), "all".to_owned(), 12)));
    assert!(rows.contains(&("class".to_owned(), "interactive".to_owned(), 9)));
    assert!(rows.contains(&("class".to_owned(), "analytical".to_owned(), 3)));
    let expired_count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM vala.oracle_admission_leases WHERE expires_at <= $1",
    )
    .bind(recovery_now)
    .fetch_one(fixture.operator_pool().pool())
    .await
    .expect("expired leases count");
    assert_eq!(expired_count, 0);
    let events: Vec<(String, String, String, String, String)> = sqlx::query_as(
        "SELECT operation,resource,principal_kind,payload_summary,detail \
         FROM vala.audit_outbox WHERE data_tenant_id=$1 \
         AND operation='bifrost.oracle.admission_recovery'",
    )
    .bind(uuid::Uuid::from(DataTenantId::SYSTEM_OWNER))
    .fetch_all(&fixture.superuser_pool().await.expect("superuser pool"))
    .await
    .expect("system recovery audit reads");
    assert_eq!(events.len(), 1);
    let (operation, resource, principal_kind, summary, detail) = &events[0];
    assert_eq!(operation, "bifrost.oracle.admission_recovery");
    assert_eq!(resource, "bifrost.oracle.admission");
    assert_eq!(principal_kind, "service");
    assert_eq!(summary, "recovered Oracle admission aggregates");
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(detail).expect("recovery detail JSON"),
        serde_json::json!({
            "kind": "oracle_admission_recovery",
            "expired_lease_count": 2,
            "active_lease_count": 3,
            "interactive_slots": 9,
            "analytical_slots": 3,
            "total_slots": 12,
        })
    );
}

/// An audit append failure rolls back lease expiry, counter repair, and chain state.
#[tokio::test]
async fn pg_oracle_recovery_audit_failure_rolls_back_all_state() {
    let (fixture, tenant) = setup().await;
    let owner = OracleAdmissionLeases::new(fixture.vala_postgres().clone(), tenant);
    let now = Utc::now();
    let mut expired = request(
        tenant,
        QueryId::new(uuid::Uuid::now_v7()),
        NodeId::new(uuid::Uuid::now_v7()),
        1,
        4,
        now + Duration::seconds(1),
    );
    expired.cluster_limit = 20;
    expired.class_limit = 20;
    expired.tenant_limit = 20;
    owner.acquire(&expired).await.expect("expired lease seeds");
    let recovery_now = now + Duration::seconds(2);
    let superuser = fixture.superuser_pool().await.expect("table-owner pool");
    sqlx::query(
        "CREATE FUNCTION vala.reject_oracle_recovery_audit() RETURNS trigger \
         LANGUAGE plpgsql AS $$ BEGIN IF NEW.operation='bifrost.oracle.admission_recovery' THEN \
         RAISE EXCEPTION 'injected recovery audit failure'; END IF; RETURN NEW; END $$",
    )
    .execute(&superuser)
    .await
    .expect("audit rejection function installs");
    sqlx::query(
        "CREATE TRIGGER reject_oracle_recovery_audit BEFORE INSERT ON vala.audit_outbox \
         FOR EACH ROW EXECUTE FUNCTION vala.reject_oracle_recovery_audit()",
    )
    .execute(&superuser)
    .await
    .expect("audit rejection trigger installs");

    owner
        .inner
        .recover_shared_scopes(fixture.operator_pool(), recovery_now)
        .await
        .expect_err("recovery audit failure propagates");

    let lease_count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM vala.oracle_admission_leases WHERE data_tenant_id=$1",
    )
    .bind(uuid::Uuid::from(tenant))
    .fetch_one(fixture.operator_pool().pool())
    .await
    .expect("rolled-back lease reads");
    assert_eq!(lease_count, 1);
    let cluster_slots: i64 = sqlx::query_scalar(
        "SELECT used_slots FROM vala.oracle_admission_accounting \
         WHERE data_tenant_id=$1 AND scope_kind='cluster' AND scope_key='global' \
         AND accounting_class='all'",
    )
    .bind(uuid::Uuid::from(DataTenantId::SYSTEM_OWNER))
    .fetch_one(fixture.operator_pool().pool())
    .await
    .expect("rolled-back counter reads");
    assert_eq!(cluster_slots, 4);
    let audit_count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM vala.audit_outbox WHERE data_tenant_id=$1 \
         AND operation='bifrost.oracle.admission_recovery'",
    )
    .bind(uuid::Uuid::from(DataTenantId::SYSTEM_OWNER))
    .fetch_one(&superuser)
    .await
    .expect("rolled-back audit reads");
    assert_eq!(audit_count, 0);
}

/// The platform operator receives the fixed recovery capability plus Forge's
/// narrow append grants, while destructive audit access remains denied.
#[tokio::test]
async fn pg_oracle_recovery_audit_authority_is_execute_only() {
    let (fixture, _) = setup().await;
    let superuser = fixture.superuser_pool().await.expect("superuser pool");
    let metadata: (bool, bool, bool, bool) = sqlx::query_as(
        "SELECT p.prosecdef, \
         COALESCE(p.proconfig, ARRAY[]::text[]) @> ARRAY['search_path=\"\"'], \
         NOT EXISTS (SELECT 1 FROM aclexplode(p.proacl) acl WHERE acl.grantee=0 AND acl.privilege_type='EXECUTE'), \
         has_function_privilege('wyrd_platform_admin', \
           'vala.append_oracle_admission_recovery_audit(text,bigint,bigint,bigint,bigint,bigint)', 'EXECUTE') \
         FROM pg_proc p JOIN pg_namespace n ON n.oid=p.pronamespace \
         WHERE n.nspname='vala' AND p.proname='append_oracle_admission_recovery_audit'",
    )
    .fetch_one(&superuser)
    .await
    .expect("function authority metadata reads");
    assert_eq!(metadata, (true, true, true, true));

    for table in ["vala.audit_chain_head", "vala.audit_outbox"] {
        let privileges: (bool, bool, bool, bool) = sqlx::query_as(
            "SELECT has_table_privilege('wyrd_platform_admin',$1,'SELECT'), \
             has_table_privilege('wyrd_platform_admin',$1,'INSERT'), \
             has_table_privilege('wyrd_platform_admin',$1,'UPDATE'), \
             has_table_privilege('wyrd_platform_admin',$1,'DELETE')",
        )
        .bind(table)
        .fetch_one(&superuser)
        .await
        .expect("table privilege metadata reads");
        let expected = if table == "vala.audit_chain_head" {
            (true, true, true, false)
        } else {
            (false, true, false, false)
        };
        assert_eq!(privileges, expected, "{table}");
    }

    for statement in [
        "DELETE FROM vala.audit_chain_head",
        "SELECT * FROM vala.audit_outbox LIMIT 1",
        "UPDATE vala.audit_outbox SET payload_summary='forbidden'",
        "DELETE FROM vala.audit_outbox",
    ] {
        let error = sqlx::query(statement)
            .execute(fixture.operator_pool().pool())
            .await
            .expect_err("direct audit-table access is denied");
        assert_eq!(
            error
                .as_database_error()
                .and_then(|db| db.code())
                .as_deref(),
            Some("42501")
        );
    }

    let request_id = wyrd_spec::request_id::RequestId::now_v7();
    let negative = sqlx::query_scalar::<_, i64>(
        "SELECT vala.append_oracle_admission_recovery_audit($1,$2,$3,$4,$5,$6)",
    )
    .bind(request_id.as_str())
    .bind(-1_i64)
    .bind(0_i64)
    .bind(0_i64)
    .bind(0_i64)
    .bind(0_i64)
    .fetch_one(fixture.operator_pool().pool())
    .await
    .expect_err("negative recovery counter is rejected");
    assert_eq!(
        negative
            .as_database_error()
            .and_then(|db| db.code())
            .as_deref(),
        Some("22003")
    );
}

/// Proves reconciliation and acquisition serialize on the canonical counter.
#[tokio::test]
async fn concurrent_reconcile_and_acquire_preserve_accounting() {
    let (fixture, tenant) = setup().await;
    let owner = OracleAdmissionLeases::new(fixture.vala_postgres().clone(), tenant);
    let leader = NodeId::new(uuid::Uuid::now_v7());
    let mut first = request(
        tenant,
        QueryId::new(uuid::Uuid::now_v7()),
        leader,
        1,
        1,
        Utc::now() + Duration::minutes(5),
    );
    first.cluster_limit = 10;
    first.class_limit = 10;
    first.tenant_limit = 10;
    owner.acquire(&first).await.expect("first lease acquires");

    let acquiring = OracleAdmissionLeases::new(fixture.vala_postgres().clone(), tenant);
    let reconciling = OracleAdmissionLeases::new(fixture.vala_postgres().clone(), tenant);
    let mut second = request(
        tenant,
        QueryId::new(uuid::Uuid::now_v7()),
        leader,
        1,
        1,
        Utc::now() + Duration::minutes(5),
    );
    second.cluster_limit = 10;
    second.class_limit = 10;
    second.tenant_limit = 10;
    let (acquired, reconciled) = tokio::join!(
        acquiring.acquire(&second),
        reconciling.reconcile_scopes(&[AdmissionReconcileScope::Cluster], Utc::now())
    );
    assert!(matches!(
        acquired.expect("concurrent admission completes"),
        AdmissionAcquire::Acquired(_)
    ));
    reconciled.expect("concurrent reconciliation completes");

    let mut conn = fixture
        .vala_postgres()
        .tenant_conn(tenant)
        .await
        .expect("tenant connection opens");
    let used: i64 = sqlx::query_scalar(
        "SELECT used_slots FROM vala.oracle_admission_accounting \
         WHERE data_tenant_id=$1 AND scope_kind='cluster' AND scope_key='global' \
         AND accounting_class='all'",
    )
    .bind(uuid::Uuid::from(DataTenantId::SYSTEM_OWNER))
    .fetch_one(&mut **conn.transaction())
    .await
    .expect("counter reads");
    conn.commit().await.expect("counter read commits");
    assert_eq!(used, 2);
}

/// Shared startup recovery serializes with acquisition on canonical counters.
#[tokio::test]
async fn concurrent_recover_shared_scopes_and_acquire_preserve_accounting() {
    let (fixture, tenant) = setup().await;
    let leader = NodeId::new(uuid::Uuid::now_v7());
    let owner = OracleAdmissionLeases::new(fixture.vala_postgres().clone(), tenant);
    for expected in 1_i64..=16 {
        let barrier = std::sync::Arc::new(tokio::sync::Barrier::new(2));
        let mut next = request(
            tenant,
            QueryId::new(uuid::Uuid::now_v7()),
            leader,
            1,
            1,
            Utc::now() + Duration::minutes(5),
        );
        next.cluster_limit = 64;
        next.class_limit = 64;
        next.tenant_limit = 64;
        let acquiring = OracleAdmissionLeases::new(fixture.vala_postgres().clone(), tenant);
        let acquire_barrier = std::sync::Arc::clone(&barrier);
        let acquire = async move {
            acquire_barrier.wait().await;
            acquiring.acquire(&next).await
        };
        let recover_barrier = std::sync::Arc::clone(&barrier);
        let recover = async {
            recover_barrier.wait().await;
            owner
                .inner
                .recover_shared_scopes(fixture.operator_pool(), Utc::now())
                .await
        };
        let (acquired, recovered) = tokio::join!(acquire, recover);
        assert!(matches!(
            acquired.expect("acquire"),
            AdmissionAcquire::Acquired(_)
        ));
        recovered.expect("shared recovery");
        let used: i64 = sqlx::query_scalar(
            "SELECT used_slots FROM vala.oracle_admission_accounting \
             WHERE data_tenant_id=$1 AND scope_kind='cluster' AND scope_key='global' \
             AND accounting_class='all'",
        )
        .bind(uuid::Uuid::from(DataTenantId::SYSTEM_OWNER))
        .fetch_one(fixture.operator_pool().pool())
        .await
        .expect("counter");
        assert_eq!(used, expected);
    }
}

/// Recovery waits for an in-flight acquire lock, then snapshots its committed lease.
#[tokio::test]
async fn recover_shared_scopes_snapshots_after_acquire_commit() {
    let (fixture, tenant) = setup().await;
    let owner = SqlOracleAdmissionLeases::new(fixture.vala_postgres().clone());
    let leader = NodeId::new(uuid::Uuid::now_v7());
    let mut request = request(
        tenant,
        QueryId::new(uuid::Uuid::now_v7()),
        leader,
        1,
        1,
        Utc::now() + Duration::minutes(5),
    );
    request.cluster_limit = 64;
    request.class_limit = 64;
    request.tenant_limit = 64;
    let mut acquire_conn = fixture
        .vala_postgres()
        .tenant_conn(tenant)
        .await
        .expect("acquire connection");
    owner
        .acquire(&mut acquire_conn, &request)
        .await
        .expect("acquire holds canonical counter lock");

    let recovery_owner = SqlOracleAdmissionLeases::new(fixture.vala_postgres().clone());
    let operator = fixture.operator_pool().clone();
    let mut recovery = tokio::spawn(async move {
        recovery_owner
            .recover_shared_scopes(&operator, Utc::now())
            .await
    });
    tokio::task::yield_now().await;
    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(50), &mut recovery)
            .await
            .is_err(),
        "recovery must wait for the acquire lock"
    );
    acquire_conn.commit().await.expect("commit acquire");
    recovery
        .await
        .expect("recovery task")
        .expect("recovery after acquire commit");

    let used: i64 = sqlx::query_scalar(
        "SELECT used_slots FROM vala.oracle_admission_accounting \
         WHERE data_tenant_id=$1 AND scope_kind='cluster' AND scope_key='global' \
         AND accounting_class='all'",
    )
    .bind(uuid::Uuid::from(DataTenantId::SYSTEM_OWNER))
    .fetch_one(fixture.operator_pool().pool())
    .await
    .expect("shared counter");
    assert_eq!(used, 1);
}

/// Proves admission hints and bounded maintenance inputs remain closed.
#[tokio::test]
async fn admission_hints_and_maintenance_bounds_are_enforced() {
    let (fixture, tenant) = setup().await;
    let owner = OracleAdmissionLeases::new(fixture.vala_postgres().clone(), tenant);
    let leader = NodeId::new(uuid::Uuid::now_v7());
    let expiry = Utc::now() + Duration::minutes(5);
    owner
        .acquire(&request(
            tenant,
            QueryId::new(uuid::Uuid::now_v7()),
            leader,
            1,
            1,
            expiry,
        ))
        .await
        .expect("first admission succeeds");
    let rejection = owner
        .acquire(&request(
            tenant,
            QueryId::new(uuid::Uuid::now_v7()),
            leader,
            1,
            1,
            expiry,
        ))
        .await
        .expect("capacity rejection is represented");
    assert!(matches!(
        rejection,
        AdmissionAcquire::Rejected {
            retry_after_ms: 1_000,
            ..
        }
    ));
    assert!(owner.expire_batch(Utc::now(), 0).await.is_err());
    assert!(owner.expire_batch(Utc::now(), 129).await.is_err());
    assert!(owner.reconcile_scopes(&[], Utc::now()).await.is_err());
    assert!(
        owner
            .reconcile_scopes(
                &[
                    AdmissionReconcileScope::Cluster,
                    AdmissionReconcileScope::Cluster
                ],
                Utc::now()
            )
            .await
            .is_err()
    );
    let too_many = (0..65)
        .map(|_| AdmissionReconcileScope::Tenant {
            data_tenant_id: DataTenantId::new_v7(),
            query_class: QueryClass::Interactive,
        })
        .collect::<Vec<_>>();
    assert!(owner.reconcile_scopes(&too_many, Utc::now()).await.is_err());
}

/// Proves leader-fence comparison precedes expiry during renewal.
#[tokio::test]
async fn expired_mismatched_renewal_is_stale() {
    let (fixture, tenant) = setup().await;
    let owner = OracleAdmissionLeases::new(fixture.vala_postgres().clone(), tenant);
    let leader = NodeId::new(uuid::Uuid::now_v7());
    let query_id = QueryId::new(uuid::Uuid::now_v7());
    owner
        .acquire(&request(
            tenant,
            query_id,
            leader,
            7,
            1,
            Utc::now() + Duration::minutes(5),
        ))
        .await
        .expect("admission succeeds");
    let mut conn = fixture
        .vala_postgres()
        .tenant_conn(tenant)
        .await
        .expect("tenant connection opens");
    sqlx::query(
        "UPDATE vala.oracle_admission_leases \
         SET acquired_at=now()-interval '2 seconds',expires_at=now()-interval '1 second' \
         WHERE data_tenant_id=$1",
    )
    .bind(uuid::Uuid::from(tenant))
    .execute(&mut **conn.transaction())
    .await
    .expect("lease expires");
    conn.commit().await.expect("lease expiry commits");
    assert!(matches!(
        owner
            .renew(
                query_id,
                &RoleFence {
                    node_id: leader,
                    fencing_token: 6,
                },
                Utc::now() + Duration::minutes(5),
            )
            .await
            .expect("renewal outcome returns"),
        LeaseMutation::StaleLeaderFence
    ));
}

/// Proves ready but stale membership is excluded from discovery.
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
    let registered = owner.register(&registration).await.expect("role registers");
    owner
        .heartbeat(
            &registration.key,
            registered.lease.fencing_token,
            true,
            &registration.capabilities,
        )
        .await
        .expect("role becomes ready");
    let mut conn = fixture
        .vala_postgres()
        .tenant_conn(DataTenantId::SYSTEM_OWNER)
        .await
        .expect("system tenant connection opens");
    sqlx::query(
        "UPDATE vala.cluster_nodes SET heartbeat_at=now()-interval '10 minutes' \
         WHERE data_tenant_id=$1 AND node_id=$2 AND role='scribe'",
    )
    .bind(uuid::Uuid::from(DataTenantId::SYSTEM_OWNER))
    .bind(registration.key.node_id.as_uuid())
    .execute(&mut **conn.transaction())
    .await
    .expect("heartbeat ages");
    conn.commit().await.expect("heartbeat aging commits");
    assert!(
        owner
            .list_live(ClusterRole::Scribe, Utc::now() - Duration::minutes(1))
            .await
            .expect("discovery succeeds")
            .is_empty()
    );
}

/// Proves concurrent release and expiry reclaim one lease only once.
#[tokio::test]
async fn concurrent_release_and_expiry_do_not_double_decrement() {
    let (fixture, tenant) = setup().await;
    let leader = NodeId::new(uuid::Uuid::now_v7());
    let query_id = QueryId::new(uuid::Uuid::now_v7());
    let owner = OracleAdmissionLeases::new(fixture.vala_postgres().clone(), tenant);
    owner
        .acquire(&request(
            tenant,
            query_id,
            leader,
            3,
            1,
            Utc::now() + Duration::minutes(5),
        ))
        .await
        .expect("admission succeeds");
    let mut conn = fixture
        .vala_postgres()
        .tenant_conn(tenant)
        .await
        .expect("tenant connection opens");
    sqlx::query(
        "UPDATE vala.oracle_admission_leases \
         SET acquired_at=now()-interval '2 seconds',expires_at=now()-interval '1 second' \
         WHERE data_tenant_id=$1",
    )
    .bind(uuid::Uuid::from(tenant))
    .execute(&mut **conn.transaction())
    .await
    .expect("lease expires");
    conn.commit().await.expect("lease expiry commits");
    let releasing = OracleAdmissionLeases::new(fixture.vala_postgres().clone(), tenant);
    let expiring = OracleAdmissionLeases::new(fixture.vala_postgres().clone(), tenant);
    let fence = RoleFence {
        node_id: leader,
        fencing_token: 3,
    };
    let (released, expired) = tokio::join!(
        releasing.release(query_id, &fence),
        expiring.expire_batch(Utc::now(), 8)
    );
    released.expect("release completes");
    expired.expect("expiry completes");
    let mut conn = fixture
        .vala_postgres()
        .tenant_conn(tenant)
        .await
        .expect("tenant connection opens");
    let used: i64 = sqlx::query_scalar(
        "SELECT used_slots FROM vala.oracle_admission_accounting \
         WHERE data_tenant_id=$1 AND scope_kind='cluster' AND scope_key='global' \
         AND accounting_class='all'",
    )
    .bind(uuid::Uuid::from(DataTenantId::SYSTEM_OWNER))
    .fetch_one(&mut **conn.transaction())
    .await
    .expect("counter reads");
    conn.commit().await.expect("counter read commits");
    assert_eq!(used, 0);
}
