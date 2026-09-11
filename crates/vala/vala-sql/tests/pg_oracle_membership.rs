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

mod pg_tests {
    //! PostgreSQL integration coverage for the durable Oracle reader authority.
    //!
    //! These tests drive the real SQL owners against a live database: the
    //! per-table serialization row, the epoch lease, and the per-epoch table
    //! protection frontier. Every assertion here is about durable evidence
    //! Forge later consumes, so each one fails closed rather than tolerating a
    //! smaller protected set. Run via `mise run test:sql`.

    use chrono::Utc;
    use sqlx::PgPool;
    use uuid::Uuid;
    use vala_sql::queries::audit_outbox::append_audit;
    use vala_sql::queries::cluster_nodes::ClusterNodes;
    use vala_sql::queries::olap_catalog::upsert_table;
    use vala_sql::queries::oracle_reader_authority::{
        BIFROST_CATALOG_NAME, BifrostTableMaintenanceAuthority, OracleReaderEpochs,
        OracleTableProtections, enumerate_epoch_protection_keys_for_operator,
        list_expired_epochs_for_operator,
    };
    use vala_sql::row_types::cluster_nodes::RoleRegistration;
    use vala_sql::row_types::oracle_reader_authority::{
        OracleEpochState, ProtectionCas, ProtectionFrontier, ProtectionKey, ProtectionMember,
        TableAuthorityIdentity,
    };
    use vala_sql::{SqlError, TenantConn};
    use wyrd_dev_fixtures::pg::PgFixture;
    use wyrd_spec::DataTenantId;
    use wyrd_spec::auth::{PrincipalId, PrincipalKindTag};
    use wyrd_spec::request_id::RequestId;
    use wyrd_spec::vala::api::{
        AuditDecision, AuditDetail, AuditEvent, AuditResult, AuthMethod, ClusterCapabilities,
        ClusterNodeKey, ClusterRole, OracleCapabilitiesV1, OracleReaderEpochPhase,
        OracleTableProtectionPhase, QueryClass,
    };

    /// Namespace segment every table in this module is registered under.
    const NAMESPACE: &str = "vala.bifrost";
    /// Table segment every table in this module is registered under.
    const TABLE: &str = "events";

    /// Canonical layout JSON one registration needs; its shape is opaque here.
    fn layout() -> serde_json::Value {
        serde_json::json!({
            "partition": { "column": "wyrd_event_time", "granularity": "hour" },
            "sort_keys": [],
            "bloom_columns": []
        })
    }

    /// One live database, one seeded tenant, and one registered Bifrost table.
    ///
    /// Every test in this module needs the same durable preconditions: a tenant
    /// that exists, a table whose maintenance-authority row registration wrote,
    /// and a node that can hold an Oracle fence. Building them once keeps each
    /// test's body about the transition it is proving.
    struct ReaderAuthority {
        /// Repository Postgres fixture owning the isolated database.
        fixture: PgFixture,
        /// Data tenant owning the registered table and its protection rows.
        tenant: DataTenantId,
        /// Physical node every epoch in the test is acquired for.
        node_id: Uuid,
        /// Durable identity of the one registered table under test.
        identity: TableAuthorityIdentity,
    }

    impl ReaderAuthority {
        /// Starts the fixture, seeds one tenant, and registers one table.
        ///
        /// # Panics
        ///
        /// Panics when the fixture, tenant seed, or registration fails.
        async fn start() -> Self {
            let fixture = PgFixture::start().await.expect("fixture starts");
            let tenant = DataTenantId::new_v7();
            fixture
                .seed_additional_tenant_with_uuid(
                    tenant,
                    &format!("reader-{}", tenant.as_uuid().simple()),
                )
                .await
                .expect("tenant seeds");
            let table_uid = *Uuid::now_v7().as_bytes();
            let mut conn = TenantConn::acquire(fixture.app_pool(), tenant)
                .await
                .expect("tenant connection");
            upsert_table(
                &mut conn,
                &table_uid,
                &format!("{NAMESPACE}.{TABLE}"),
                &[9_u8; 32],
                &layout(),
            )
            .await
            .expect("table registers");
            conn.commit().await.expect("registration commits");
            Self {
                fixture,
                tenant,
                node_id: Uuid::now_v7(),
                identity: TableAuthorityIdentity {
                    tenant,
                    table_uid,
                    catalog_name: BIFROST_CATALOG_NAME.to_owned(),
                    namespace_name: NAMESPACE.to_owned(),
                    table_name: TABLE.to_owned(),
                },
            }
        }

        /// Registers this node's Oracle role and returns its new exact fence.
        ///
        /// # Panics
        ///
        /// Panics when membership registration fails.
        async fn register_oracle_role(&self) -> i64 {
            let nodes = ClusterNodes::new(self.fixture.vala_postgres().clone());
            let mut conn = self
                .fixture
                .vala_postgres()
                .tenant_conn(DataTenantId::SYSTEM_OWNER)
                .await
                .expect("system connection");
            let row = nodes
                .register(
                    &mut conn,
                    &RoleRegistration {
                        key: ClusterNodeKey {
                            node_id: wyrd_spec::vala::api::NodeId::new(self.node_id),
                            role: ClusterRole::Oracle,
                        },
                        address: "http://oracle:5002".into(),
                        capabilities: ClusterCapabilities::OracleV1(OracleCapabilitiesV1 {
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
                        started_at: Utc::now(),
                    },
                )
                .await
                .expect("oracle role registers");
            conn.commit().await.expect("registration commits");
            i64::try_from(row.lease.fencing_token).expect("fence fits an i64")
        }

        /// Acquires this node's durable epoch row at one exact fence.
        ///
        /// A protection header references its owning epoch, so every test that
        /// publishes protection needs the epoch row to exist first. This is the
        /// same acquisition the authority performs at startup.
        ///
        /// # Panics
        ///
        /// Panics when acquisition or its transaction fails.
        async fn acquire_epoch(&self, fence: i64) {
            let mut conn = self.system_conn().await;
            OracleReaderEpochs::new(&mut conn)
                .expect("epochs are system owned")
                .acquire(self.node_id, fence, std::time::Duration::from_secs(30))
                .await
                .expect("epoch acquires");
            conn.commit().await.expect("epoch acquisition commits");
        }

        /// Opens one `SYSTEM_OWNER` transaction for epoch statements.
        ///
        /// # Panics
        ///
        /// Panics when the connection cannot be acquired.
        async fn system_conn(&self) -> TenantConn<'_> {
            TenantConn::acquire(self.fixture.app_pool(), DataTenantId::SYSTEM_OWNER)
                .await
                .expect("system connection")
        }

        /// Opens one data-tenant transaction for protection statements.
        ///
        /// # Panics
        ///
        /// Panics when the connection cannot be acquired.
        async fn tenant_conn(&self) -> TenantConn<'_> {
            TenantConn::acquire(self.fixture.app_pool(), self.tenant)
                .await
                .expect("tenant connection")
        }

        /// Opens the fixture's superuser pool for out-of-band inspection.
        ///
        /// # Panics
        ///
        /// Panics when the superuser pool cannot be opened.
        async fn superuser(&self) -> PgPool {
            self.fixture.superuser_pool().await.expect("superuser pool")
        }

        /// Builds one single-chain frontier over the supplied ancestry.
        ///
        /// # Panics
        ///
        /// Panics when the ancestry cannot form a valid member.
        fn frontier(
            &self,
            ancestry: Vec<i64>,
            head_ms: i64,
            protected_ms: i64,
        ) -> ProtectionFrontier {
            let member = ProtectionMember::new(&self.identity, ancestry, head_ms, protected_ms)
                .expect("well-formed ancestry");
            ProtectionFrontier::new(&self.identity, vec![member]).expect("well-formed frontier")
        }

        /// Commits one protection revision with its canonical tenant audit row.
        ///
        /// # Panics
        ///
        /// Panics when the lock, commit, audit append, or transaction commit fails.
        async fn commit_protection(
            &self,
            fencing_token: i64,
            expected_revision: Option<i64>,
            frontier: &ProtectionFrontier,
            phase: OracleTableProtectionPhase,
        ) -> ProtectionCas {
            let mut conn = self.tenant_conn().await;
            BifrostTableMaintenanceAuthority::new(&mut conn)
                .lock(&self.identity)
                .await
                .expect("table authority row locks");
            let outcome = OracleTableProtections::new(&mut conn)
                .commit(
                    &self.identity,
                    self.node_id,
                    fencing_token,
                    expected_revision,
                    frontier,
                )
                .await
                .expect("protection commit runs");
            if let ProtectionCas::Committed(record) = &outcome {
                append_audit(
                    &mut conn,
                    &protection_event(self.node_id, fencing_token, phase, record.revision),
                )
                .await
                .expect("protection audit appends");
            }
            conn.commit().await.expect("protection transaction commits");
            outcome
        }
    }

    /// Builds one canonical epoch lifecycle audit event.
    fn epoch_event(
        node_id: Uuid,
        fencing_token: i64,
        phase: OracleReaderEpochPhase,
        state_revision: i64,
    ) -> AuditEvent {
        let operation = match phase {
            OracleReaderEpochPhase::Acquired => "oracle.reader_epoch.acquired",
            OracleReaderEpochPhase::Activated => "oracle.reader_epoch.activated",
            OracleReaderEpochPhase::Draining => "oracle.reader_epoch.draining",
            OracleReaderEpochPhase::Invalidated => "oracle.reader_epoch.invalidated",
            OracleReaderEpochPhase::Retired => "oracle.reader_epoch.retired",
        };
        AuditEvent {
            request_id: RequestId::now_v7(),
            trace_id: None,
            operation: operation.to_owned(),
            resource: format!("oracle/reader_epoch/{node_id}/{fencing_token}"),
            card_ref: None,
            principal_id: PrincipalId::new(Uuid::nil()),
            principal_kind: PrincipalKindTag::Service,
            auth_method: AuthMethod::Internal,
            permission: "bifrost:oracle".to_owned(),
            decision: AuditDecision::Allow,
            result: AuditResult::Success,
            payload_summary: operation.to_owned(),
            detail: Some(AuditDetail::OracleReaderEpoch {
                node_id,
                fencing_token,
                phase,
                state_revision,
            }),
        }
    }

    /// Builds one canonical table protection audit event.
    fn protection_event(
        node_id: Uuid,
        fencing_token: i64,
        phase: OracleTableProtectionPhase,
        revision: i64,
    ) -> AuditEvent {
        let operation = match phase {
            OracleTableProtectionPhase::Expanded => "oracle.table_protection.expanded",
            OracleTableProtectionPhase::Narrowed => "oracle.table_protection.narrowed",
            OracleTableProtectionPhase::Released => "oracle.table_protection.released",
        };
        AuditEvent {
            request_id: RequestId::now_v7(),
            trace_id: None,
            operation: operation.to_owned(),
            resource: format!("{NAMESPACE}.{TABLE}"),
            card_ref: None,
            principal_id: PrincipalId::new(Uuid::nil()),
            principal_kind: PrincipalKindTag::Service,
            auth_method: AuthMethod::Internal,
            permission: "bifrost:oracle".to_owned(),
            decision: AuditDecision::Allow,
            result: AuditResult::Success,
            payload_summary: operation.to_owned(),
            detail: Some(AuditDetail::OracleTableProtection {
                node_id,
                fencing_token,
                phase,
                group: format!("{NAMESPACE}.{TABLE}"),
                revision,
                protected_snapshot_ids: Vec::new(),
            }),
        }
    }

    /// Reads one tenant's audit operations in durable sequence order.
    async fn audit_operations(pool: &PgPool, tenant: DataTenantId) -> Vec<String> {
        sqlx::query_scalar(
            "SELECT operation FROM vala.audit_outbox WHERE data_tenant_id = $1 ORDER BY seq",
        )
        .bind(tenant.as_uuid())
        .fetch_all(pool)
        .await
        .expect("audit rows read")
    }

    /// Projects one table's columns as `(name, type, nullability)` in order.
    async fn columns(pool: &PgPool, table: &str) -> Vec<(String, String, String)> {
        sqlx::query_as(
            "SELECT column_name, data_type, is_nullable \
               FROM information_schema.columns \
              WHERE table_schema = 'vala' AND table_name = $1 \
              ORDER BY ordinal_position",
        )
        .bind(table)
        .fetch_all(pool)
        .await
        .expect("columns read")
    }

    /// Projects one table's constraint definitions of a single kind.
    async fn constraints(pool: &PgPool, table: &str, kind: &str) -> Vec<String> {
        sqlx::query_scalar(
            "SELECT pg_get_constraintdef(oid) FROM pg_constraint \
              WHERE conrelid = ('vala.' || $1)::regclass AND contype = $2 \
              ORDER BY pg_get_constraintdef(oid)",
        )
        .bind(table)
        .bind(kind)
        .fetch_all(pool)
        .await
        .expect("constraints read")
    }

    /// One foreign key's deferrability, delete rule, and column pairing.
    ///
    /// The tuple is `(condeferrable, condeferred, confdeltype, referencing
    /// columns, referenced columns)`, read straight from `pg_constraint` so the
    /// assertion does not depend on how `pg_get_constraintdef` spells defaults.
    type ForeignKeyShape = (bool, bool, String, Vec<String>, Vec<String>);

    /// Projects the privileges one role holds on one table, sorted.
    async fn grants(pool: &PgPool, table: &str, grantee: &str) -> Vec<String> {
        sqlx::query_scalar(
            "SELECT privilege_type FROM information_schema.role_table_grants \
              WHERE table_schema = 'vala' AND table_name = $1 AND grantee = $2 \
              ORDER BY privilege_type",
        )
        .bind(table)
        .bind(grantee)
        .fetch_all(pool)
        .await
        .expect("grants read")
    }

    /// Proves the four durable owners match their migration exactly.
    ///
    /// # Panics
    ///
    /// Panics when any column, constraint, policy, grant, or registration
    /// cardinality differs from the migration this module depends on.
    #[tokio::test]
    async fn oracle_reader_authority_schema_contract_matches_migration() {
        let harness = ReaderAuthority::start().await;
        let pool = harness.superuser().await;

        assert_eq!(
            columns(&pool, "bifrost_table_maintenance_authority").await,
            vec![
                ("data_tenant_id", "uuid", "NO"),
                ("catalog_name", "text", "NO"),
                ("namespace_name", "text", "NO"),
                ("table_name", "text", "NO"),
                ("table_uid", "bytea", "NO"),
            ]
            .into_iter()
            .map(|(name, kind, null)| (name.to_owned(), kind.to_owned(), null.to_owned()))
            .collect::<Vec<_>>()
        );
        assert_eq!(
            columns(&pool, "oracle_reader_epochs").await,
            vec![
                ("epoch_owner_tenant_id", "uuid", "NO"),
                ("node_id", "uuid", "NO"),
                ("fencing_token", "bigint", "NO"),
                ("state", "text", "NO"),
                ("state_revision", "bigint", "NO"),
                ("acquired_at", "timestamp with time zone", "NO"),
                ("activated_at", "timestamp with time zone", "YES"),
                ("renewed_at", "timestamp with time zone", "NO"),
                ("lease_expires_at", "timestamp with time zone", "NO"),
                ("invalidated_at", "timestamp with time zone", "YES"),
            ]
            .into_iter()
            .map(|(name, kind, null)| (name.to_owned(), kind.to_owned(), null.to_owned()))
            .collect::<Vec<_>>()
        );
        assert_eq!(
            columns(&pool, "oracle_table_protections").await,
            vec![
                ("data_tenant_id", "uuid", "NO"),
                ("table_uid", "bytea", "NO"),
                ("node_id", "uuid", "NO"),
                ("fencing_token", "bigint", "NO"),
                ("catalog_name", "text", "NO"),
                ("namespace_name", "text", "NO"),
                ("table_name", "text", "NO"),
                ("revision", "bigint", "NO"),
                ("frontier_encoding_version", "integer", "NO"),
                ("frontier_digest", "bytea", "NO"),
                ("updated_at", "timestamp with time zone", "NO"),
            ]
            .into_iter()
            .map(|(name, kind, null)| (name.to_owned(), kind.to_owned(), null.to_owned()))
            .collect::<Vec<_>>()
        );
        assert_eq!(
            columns(&pool, "oracle_table_protection_members").await,
            vec![
                ("data_tenant_id", "uuid", "NO"),
                ("table_uid", "bytea", "NO"),
                ("node_id", "uuid", "NO"),
                ("fencing_token", "bigint", "NO"),
                ("protected_snapshot_id", "bigint", "NO"),
                ("protected_snapshot_timestamp_ms", "bigint", "NO"),
                ("retained_head_snapshot_id", "bigint", "NO"),
                ("retained_head_timestamp_ms", "bigint", "NO"),
                ("ancestry_path", "ARRAY", "NO"),
                ("ancestry_digest_version", "integer", "NO"),
                ("ancestry_digest", "bytea", "NO"),
            ]
            .into_iter()
            .map(|(name, kind, null)| (name.to_owned(), kind.to_owned(), null.to_owned()))
            .collect::<Vec<_>>()
        );

        // No owner carries a column default: every durable value is written by
        // the statement that decided it, including the lease clock.
        let defaults: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM information_schema.columns \
              WHERE table_schema = 'vala' \
                AND table_name IN ('bifrost_table_maintenance_authority', \
                                   'oracle_reader_epochs', 'oracle_table_protections', \
                                   'oracle_table_protection_members') \
                AND column_default IS NOT NULL",
        )
        .fetch_one(&pool)
        .await
        .expect("defaults read");
        assert_eq!(
            defaults, 0,
            "reader authority owners carry no column defaults"
        );

        assert_eq!(
            constraints(&pool, "bifrost_table_maintenance_authority", "p").await,
            vec![
                "PRIMARY KEY (data_tenant_id, catalog_name, namespace_name, table_name)".to_owned()
            ]
        );
        assert_eq!(
            constraints(&pool, "bifrost_table_maintenance_authority", "u").await,
            vec!["UNIQUE (data_tenant_id, table_uid)".to_owned()]
        );
        assert_eq!(
            constraints(&pool, "oracle_reader_epochs", "p").await,
            vec!["PRIMARY KEY (epoch_owner_tenant_id, node_id, fencing_token)".to_owned()]
        );
        // The epoch key a protection header points at is exactly (node, fence):
        // the header carries no epoch-owner tenant, so this unique key is what
        // makes the referential lock reachable at all.
        assert_eq!(
            constraints(&pool, "oracle_reader_epochs", "u").await,
            vec!["UNIQUE (node_id, fencing_token)".to_owned()],
            "the epoch has exactly one unique key over (node_id, fencing_token)"
        );
        assert_eq!(
            constraints(&pool, "oracle_table_protections", "p").await,
            vec!["PRIMARY KEY (data_tenant_id, table_uid, node_id, fencing_token)".to_owned()]
        );
        assert_eq!(
            constraints(&pool, "oracle_table_protection_members", "p").await,
            vec![
                "PRIMARY KEY (data_tenant_id, table_uid, node_id, fencing_token, \
                 protected_snapshot_id)"
                    .to_owned()
            ]
        );

        let authority_fks = constraints(&pool, "bifrost_table_maintenance_authority", "f").await;
        assert!(
            authority_fks
                .iter()
                .any(|def| def.contains("REFERENCES vala.bifrost_tables")),
            "the authority row must name a registered table: {authority_fks:?}"
        );
        let member_fks = constraints(&pool, "oracle_table_protection_members", "f").await;
        assert!(
            member_fks.iter().any(
                |def| def.contains("REFERENCES vala.oracle_table_protections")
                    && def.contains("ON DELETE CASCADE")
            ),
            "members must cascade from their header: {member_fks:?}"
        );
        // Publication and retirement serialize on this foreign key. RESTRICT is
        // the whole mechanism: the deleting transaction takes a key-share lock
        // the inserting transaction blocks on, so a header can never outlive
        // the epoch that owns it. Deferring it would reopen the race, and
        // `pg_get_constraintdef` omits the default spelling, so the metadata is
        // asserted directly rather than read out of the printed definition.
        let epoch_fk: Vec<ForeignKeyShape> = sqlx::query_as(
            "SELECT c.condeferrable, c.condeferred, c.confdeltype::text, \
                    (SELECT array_agg(a.attname ORDER BY k.ord) \
                       FROM unnest(c.conkey) WITH ORDINALITY AS k(attnum, ord) \
                       JOIN pg_attribute a \
                         ON a.attrelid = c.conrelid AND a.attnum = k.attnum), \
                    (SELECT array_agg(a.attname ORDER BY k.ord) \
                       FROM unnest(c.confkey) WITH ORDINALITY AS k(attnum, ord) \
                       JOIN pg_attribute a \
                         ON a.attrelid = c.confrelid AND a.attnum = k.attnum) \
               FROM pg_constraint c \
              WHERE c.conrelid = 'vala.oracle_table_protections'::regclass \
                AND c.contype = 'f' \
                AND c.confrelid = 'vala.oracle_reader_epochs'::regclass",
        )
        .fetch_all(&pool)
        .await
        .expect("epoch foreign key metadata read");
        assert_eq!(
            epoch_fk,
            vec![(
                false,
                false,
                "r".to_owned(),
                vec!["node_id".to_owned(), "fencing_token".to_owned()],
                vec!["node_id".to_owned(), "fencing_token".to_owned()],
            )],
            "a header references its epoch by (node_id, fencing_token) under a \
             non-deferrable RESTRICT rule"
        );
        let protection_fks = constraints(&pool, "oracle_table_protections", "f").await;
        assert!(
            protection_fks
                .iter()
                .any(|def| def.contains("REFERENCES vala.bifrost_table_maintenance_authority")),
            "a header must name the table it serializes on: {protection_fks:?}"
        );

        // The Rust constant, the schema check, and the Redux catalog name are
        // one durable value; drift between them would silently orphan headers.
        let catalog_checks = constraints(&pool, "oracle_table_protections", "c").await;
        assert!(
            catalog_checks
                .iter()
                .any(|def| def.contains(&format!("'{BIFROST_CATALOG_NAME}'"))),
            "catalog check must pin the Bifrost catalog name: {catalog_checks:?}"
        );
        let member_checks = constraints(&pool, "oracle_table_protection_members", "c").await;
        for expected in [
            "ancestry_path[1] = retained_head_snapshot_id",
            "ancestry_digest_version = 1",
            "octet_length(ancestry_digest) = 32",
        ] {
            assert!(
                member_checks.iter().any(|def| def.contains(expected)),
                "member checks must contain {expected}: {member_checks:?}"
            );
        }
        let epoch_checks = constraints(&pool, "oracle_reader_epochs", "c").await;
        for expected in [
            "state_revision >= 1",
            "lease_expires_at > acquired_at",
            "fencing_token > 0",
        ] {
            assert!(
                epoch_checks.iter().any(|def| def.contains(expected)),
                "epoch checks must contain {expected}: {epoch_checks:?}"
            );
        }

        for table in [
            "bifrost_table_maintenance_authority",
            "oracle_reader_epochs",
            "oracle_table_protections",
            "oracle_table_protection_members",
        ] {
            let (enabled, forced): (bool, bool) = sqlx::query_as(
                "SELECT relrowsecurity, relforcerowsecurity FROM pg_class \
                  WHERE oid = ('vala.' || $1)::regclass",
            )
            .bind(table)
            .fetch_one(&pool)
            .await
            .expect("row security read");
            assert!(enabled && forced, "{table} must force row level security");
            let policies: Vec<String> = sqlx::query_scalar(
                "SELECT polname FROM pg_policy WHERE polrelid = ('vala.' || $1)::regclass",
            )
            .bind(table)
            .fetch_all(&pool)
            .await
            .expect("policies read");
            assert_eq!(policies, vec!["tenant_isolation".to_owned()], "{table}");
            assert_eq!(
                grants(&pool, table, "wyrd_app").await,
                vec![
                    "DELETE".to_owned(),
                    "INSERT".to_owned(),
                    "SELECT".to_owned(),
                    "UPDATE".to_owned()
                ],
                "{table}"
            );
        }

        // Recovery enumeration is read-only by grant, not by convention.
        assert_eq!(
            grants(&pool, "oracle_table_protections", "wyrd_platform_admin").await,
            vec!["SELECT".to_owned()]
        );
        assert_eq!(
            grants(&pool, "oracle_reader_epochs", "wyrd_platform_admin").await,
            vec!["SELECT".to_owned()]
        );
        // Forge's expiration lifecycle runs on the operator pool and takes this
        // row's `FOR UPDATE` lock as the table-local serialization boundary,
        // which Postgres grants only alongside UPDATE.
        assert_eq!(
            grants(
                &pool,
                "bifrost_table_maintenance_authority",
                "wyrd_platform_admin"
            )
            .await,
            vec!["SELECT".to_owned(), "UPDATE".to_owned()]
        );
        // Expiration preparation proves on the operator transaction that no
        // surviving frontier covers a selected snapshot, so the operator reads
        // the members. It never writes one.
        assert_eq!(
            grants(
                &pool,
                "oracle_table_protection_members",
                "wyrd_platform_admin"
            )
            .await,
            vec!["SELECT".to_owned()]
        );

        // Retirement's cross-tenant proof is execute-only and read-only, and
        // it is owned by the one role that may see past forced RLS.
        let (owner, volatility, security): (String, String, bool) = sqlx::query_as(
            "SELECT r.rolname, p.provolatile::text, p.prosecdef \
               FROM pg_proc p \
               JOIN pg_roles r ON r.oid = p.proowner \
              WHERE p.oid = 'vala.oracle_epoch_protection_count(uuid, bigint)'::regprocedure",
        )
        .fetch_one(&pool)
        .await
        .expect("retirement proof function exists");
        assert_eq!(
            (owner.as_str(), volatility.as_str(), security),
            ("wyrd_migrator", "s", true)
        );
        let executors: Vec<String> = sqlx::query_scalar(
            "SELECT grantee FROM information_schema.role_routine_grants \
              WHERE specific_schema = 'vala' AND routine_name = 'oracle_epoch_protection_count' \
                AND privilege_type = 'EXECUTE' AND grantee <> 'wyrd_migrator' \
              ORDER BY grantee",
        )
        .fetch_all(&pool)
        .await
        .expect("routine grants read");
        assert_eq!(
            executors,
            vec!["wyrd_app".to_owned(), "wyrd_platform_admin".to_owned()]
        );

        // Registration writes exactly one authority row, split at the last dot.
        let rows: Vec<(String, String, String, Vec<u8>)> = sqlx::query_as(
            "SELECT catalog_name, namespace_name, table_name, table_uid \
               FROM vala.bifrost_table_maintenance_authority WHERE data_tenant_id = $1",
        )
        .bind(harness.tenant.as_uuid())
        .fetch_all(&pool)
        .await
        .expect("authority rows read");
        assert_eq!(
            rows,
            vec![(
                BIFROST_CATALOG_NAME.to_owned(),
                NAMESPACE.to_owned(),
                TABLE.to_owned(),
                harness.identity.table_uid.to_vec(),
            )]
        );
    }

    /// Proves every epoch and protection transition carries canonical audit.
    ///
    /// Renewal is the deliberate exception: it advances the revision and the
    /// lease window and appends nothing, because auditing it every few seconds
    /// would bury the transitions that change what Forge may destroy.
    ///
    /// # Panics
    ///
    /// Panics when any revision, database time, exclusion rule, audit
    /// sequence, rollback coupling, or recovery scope differs from the
    /// contract.
    #[tokio::test]
    async fn oracle_reader_epoch_and_tenant_table_transitions_use_canonical_audit() {
        let harness = ReaderAuthority::start().await;
        let node = harness.node_id;
        let fence = harness.register_oracle_role().await;
        assert_eq!(fence, 1, "a first registration fences at one");
        let lease = std::time::Duration::from_secs(30);

        let mut conn = harness.system_conn().await;
        let acquired = OracleReaderEpochs::new(&mut conn)
            .expect("epochs are system owned")
            .acquire(node, fence, lease)
            .await
            .expect("epoch acquires");
        assert_eq!(acquired.state_revision, 1);
        assert!(
            acquired.lease_expires_at > acquired.database_now,
            "the database itself dates the lease window"
        );
        append_audit(
            &mut conn,
            &epoch_event(node, fence, OracleReaderEpochPhase::Acquired, 1),
        )
        .await
        .expect("acquisition audit appends");
        conn.commit().await.expect("acquisition commits");

        let mut conn = harness.system_conn().await;
        let activated = OracleReaderEpochs::new(&mut conn)
            .expect("epochs are system owned")
            .activate(node, fence, acquired.state_revision)
            .await
            .expect("epoch activates");
        assert_eq!(activated.state_revision, 2);
        assert!(activated.database_now >= acquired.database_now);
        append_audit(
            &mut conn,
            &epoch_event(node, fence, OracleReaderEpochPhase::Activated, 2),
        )
        .await
        .expect("activation audit appends");
        conn.commit().await.expect("activation commits");

        // Renewal advances the revision and the window and audits nothing.
        let mut conn = harness.system_conn().await;
        let renewed = OracleReaderEpochs::new(&mut conn)
            .expect("epochs are system owned")
            .renew(node, fence, activated.state_revision, lease)
            .await
            .expect("renewal statement runs")
            .expect("renewal at the confirmed revision applies");
        assert_eq!(renewed.state_revision, 3);
        assert!(renewed.lease_expires_at > activated.lease_expires_at);
        conn.commit().await.expect("renewal commits");

        // A renewal at a superseded revision is lease loss, not an error.
        let mut conn = harness.system_conn().await;
        assert!(
            OracleReaderEpochs::new(&mut conn)
                .expect("epochs are system owned")
                .renew(node, fence, activated.state_revision, lease)
                .await
                .expect("stale renewal statement runs")
                .is_none()
        );
        conn.commit().await.expect("stale renewal commits");

        // A protection change and its evidence roll back together.
        {
            let mut conn = harness.tenant_conn().await;
            BifrostTableMaintenanceAuthority::new(&mut conn)
                .lock(&harness.identity)
                .await
                .expect("table authority row locks");
            let outcome = OracleTableProtections::new(&mut conn)
                .commit(
                    &harness.identity,
                    node,
                    fence,
                    None,
                    &harness.frontier(vec![30, 20, 10], 300, 100),
                )
                .await
                .expect("protection commit runs");
            assert!(matches!(outcome, ProtectionCas::Committed(_)));
            append_audit(
                &mut conn,
                &protection_event(node, fence, OracleTableProtectionPhase::Expanded, 1),
            )
            .await
            .expect("protection audit appends");
        }
        let pool = harness.superuser().await;
        let uncommitted: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM vala.oracle_table_protections WHERE data_tenant_id = $1",
        )
        .bind(harness.tenant.as_uuid())
        .fetch_one(&pool)
        .await
        .expect("protection count read");
        assert_eq!(uncommitted, 0, "a dropped transaction leaves no protection");
        assert!(
            audit_operations(&pool, harness.tenant).await.is_empty(),
            "a dropped transaction leaves no evidence either"
        );

        let committed = harness
            .commit_protection(
                fence,
                None,
                &harness.frontier(vec![30, 20, 10], 300, 100),
                OracleTableProtectionPhase::Expanded,
            )
            .await;
        let ProtectionCas::Committed(record) = committed else {
            panic!("a first protection commits at revision one");
        };
        assert_eq!(record.revision, 1);

        let mut conn = harness.system_conn().await;
        let draining = OracleReaderEpochs::new(&mut conn)
            .expect("epochs are system owned")
            .transition(
                node,
                fence,
                renewed.state_revision,
                OracleEpochState::Draining,
            )
            .await
            .expect("epoch drains");
        assert_eq!(draining, 4);
        append_audit(
            &mut conn,
            &epoch_event(node, fence, OracleReaderEpochPhase::Draining, draining),
        )
        .await
        .expect("draining audit appends");
        conn.commit().await.expect("draining commits");

        let mut conn = harness.system_conn().await;
        let invalidated = OracleReaderEpochs::new(&mut conn)
            .expect("epochs are system owned")
            .transition(node, fence, draining, OracleEpochState::Invalidated)
            .await
            .expect("epoch invalidates");
        assert_eq!(invalidated, 5);
        append_audit(
            &mut conn,
            &epoch_event(
                node,
                fence,
                OracleReaderEpochPhase::Invalidated,
                invalidated,
            ),
        )
        .await
        .expect("invalidation audit appends");
        conn.commit().await.expect("invalidation commits");

        let mut conn = harness.system_conn().await;
        let row = OracleReaderEpochs::new(&mut conn)
            .expect("epochs are system owned")
            .read(node, fence)
            .await
            .expect("epoch reads")
            .expect("the epoch row still exists");
        assert_eq!(row.state, OracleEpochState::Invalidated);
        assert_eq!(row.state_revision, invalidated);
        conn.commit().await.expect("read commits");

        // A replacement registration advances the role fence, and the shared
        // row-lock order means the incumbent cannot renew even once more.
        let replacement = harness.register_oracle_role().await;
        assert_eq!(replacement, 2);
        let mut conn = harness.system_conn().await;
        let refused = OracleReaderEpochs::new(&mut conn)
            .expect("epochs are system owned")
            .renew(node, fence, invalidated, lease)
            .await;
        assert!(
            matches!(refused, Err(SqlError::InvariantViolation { .. })),
            "a replaced fence cannot renew: {refused:?}"
        );
        drop(conn);

        assert_eq!(
            audit_operations(&pool, DataTenantId::SYSTEM_OWNER).await,
            vec![
                "oracle.reader_epoch.acquired".to_owned(),
                "oracle.reader_epoch.activated".to_owned(),
                "oracle.reader_epoch.draining".to_owned(),
                "oracle.reader_epoch.invalidated".to_owned(),
            ],
            "renewal is deliberately unaudited"
        );
        assert_eq!(
            audit_operations(&pool, harness.tenant).await,
            vec!["oracle.table_protection.expanded".to_owned()],
            "protection audit is bound to the actual data tenant"
        );

        // Recovery enumeration is cross-tenant, read-only, and key-shaped.
        let operator = harness.fixture.operator_pool();
        assert_eq!(
            enumerate_epoch_protection_keys_for_operator(operator, node, fence)
                .await
                .expect("recovery enumerates keys"),
            vec![ProtectionKey {
                tenant: harness.tenant,
                table_uid: harness.identity.table_uid,
                node_id: node,
                fencing_token: fence,
            }]
        );
        assert!(
            list_expired_epochs_for_operator(operator, 64)
                .await
                .expect("expired scan runs")
                .is_empty(),
            "an unexpired lease is never listed as expired"
        );
        assert!(
            sqlx::query("UPDATE vala.oracle_table_protections SET revision = revision + 1")
                .execute(operator.pool())
                .await
                .is_err(),
            "the recovery grant cannot mutate tenant protection"
        );
    }

    /// Proves corrupt protection evidence and a lost CAS both fail closed.
    ///
    /// Every case here is one Forge would otherwise misread as "this table is
    /// less protected than it is", so none of them may degrade into an absent
    /// or smaller frontier.
    ///
    /// # Panics
    ///
    /// Panics when corrupt state reads as absent, when a lost compare-and-set
    /// mutates anything, or when protection leaks across tenants.
    #[tokio::test]
    async fn oracle_reader_authority_corruption_and_failed_narrowing_fail_closed() {
        let harness = ReaderAuthority::start().await;
        let node = harness.node_id;
        let fence = harness.register_oracle_role().await;
        harness.acquire_epoch(fence).await;
        let frontier = harness.frontier(vec![30, 20, 10], 300, 100);
        let ProtectionCas::Committed(record) = harness
            .commit_protection(fence, None, &frontier, OracleTableProtectionPhase::Expanded)
            .await
        else {
            panic!("a first protection commits");
        };
        assert_eq!(record.revision, 1);
        let pool = harness.superuser().await;

        // A header digest that no longer reproduces over its own members is
        // corruption, not a smaller protected set.
        sqlx::query(
            "UPDATE vala.oracle_table_protections SET frontier_digest = $2 \
              WHERE data_tenant_id = $1",
        )
        .bind(harness.tenant.as_uuid())
        .bind(vec![0_u8; 32])
        .execute(&pool)
        .await
        .expect("digest corruption applies");
        let mut conn = harness.tenant_conn().await;
        let poisoned = OracleTableProtections::new(&mut conn)
            .read(&harness.identity, node, fence)
            .await;
        assert!(
            matches!(poisoned, Err(SqlError::InvariantViolation { .. })),
            "a corrupt header digest must fail closed: {poisoned:?}"
        );
        // The same poison stops a narrowing commit before it writes anything.
        let refused = OracleTableProtections::new(&mut conn)
            .commit(
                &harness.identity,
                node,
                fence,
                Some(1),
                &ProtectionFrontier::default(),
            )
            .await;
        assert!(
            matches!(refused, Err(SqlError::InvariantViolation { .. })),
            "a commit over corrupt evidence must fail closed: {refused:?}"
        );
        drop(conn);
        sqlx::query(
            "UPDATE vala.oracle_table_protections SET frontier_digest = $2 \
              WHERE data_tenant_id = $1",
        )
        .bind(harness.tenant.as_uuid())
        .bind(record.frontier_digest.to_vec())
        .execute(&pool)
        .await
        .expect("digest restore applies");

        // An unknown member digest version is unreadable evidence, not an
        // older encoding to be tolerated.
        sqlx::query(
            "UPDATE vala.oracle_table_protection_members SET ancestry_digest = $2 \
              WHERE data_tenant_id = $1",
        )
        .bind(harness.tenant.as_uuid())
        .bind(vec![7_u8; 32])
        .execute(&pool)
        .await
        .expect("member corruption applies");
        let mut conn = harness.tenant_conn().await;
        let poisoned = OracleTableProtections::new(&mut conn)
            .list_table_protection(&harness.identity)
            .await;
        assert!(
            matches!(poisoned, Err(SqlError::InvariantViolation { .. })),
            "a corrupt member digest must fail closed for Forge too: {poisoned:?}"
        );
        drop(conn);
        let restored = frontier.members[0].ancestry_digest.to_vec();
        sqlx::query(
            "UPDATE vala.oracle_table_protection_members SET ancestry_digest = $2 \
              WHERE data_tenant_id = $1",
        )
        .bind(harness.tenant.as_uuid())
        .bind(restored)
        .execute(&pool)
        .await
        .expect("member restore applies");

        // A stale expectation is a conflict carrying the winner, and it commits
        // nothing: no revision advance, no member change, no partial write.
        let widened = harness.frontier(vec![40, 30, 20, 10], 400, 100);
        let conflict = harness
            .commit_protection(
                fence,
                Some(7),
                &widened,
                OracleTableProtectionPhase::Expanded,
            )
            .await;
        let ProtectionCas::Conflict(Some(winner)) = conflict else {
            panic!("a lost compare-and-set reports the winning record");
        };
        assert_eq!(winner.revision, 1);
        assert_eq!(winner.frontier, frontier);
        let (revision, members): (i64, i64) = sqlx::query_as(
            "SELECT (SELECT revision FROM vala.oracle_table_protections \
                      WHERE data_tenant_id = $1), \
                    (SELECT count(*) FROM vala.oracle_table_protection_members \
                      WHERE data_tenant_id = $1)",
        )
        .bind(harness.tenant.as_uuid())
        .fetch_one(&pool)
        .await
        .expect("post-conflict state reads");
        assert_eq!((revision, members), (1, 1), "a lost CAS writes nothing");
        assert_eq!(
            audit_operations(&pool, harness.tenant).await,
            vec!["oracle.table_protection.expanded".to_owned()],
            "a lost CAS leaves no evidence behind"
        );

        // Protection is tenant-private: another tenant sees nothing at all.
        let other = DataTenantId::new_v7();
        harness
            .fixture
            .seed_additional_tenant_with_uuid(other, &format!("other-{}", other.as_uuid().simple()))
            .await
            .expect("second tenant seeds");
        let mut conn = TenantConn::acquire(harness.fixture.app_pool(), other)
            .await
            .expect("second tenant connection");
        let foreign = TableAuthorityIdentity {
            tenant: other,
            ..harness.identity.clone()
        };
        assert!(
            OracleTableProtections::new(&mut conn)
                .read(&foreign, node, fence)
                .await
                .expect("cross-tenant read runs")
                .is_none(),
            "one tenant's protection is invisible to another"
        );
        drop(conn);
    }

    /// Proves neither a stale heartbeat nor a replacement removes protection.
    ///
    /// Liveness is not authority: only the database's own proof of lease expiry
    /// authorizes invalidation, and only the audited release sequence removes a
    /// header.
    ///
    /// # Panics
    ///
    /// Panics when protection is removed without database-proven expiry, when
    /// the release ordering is violated, or when a retired epoch leaves
    /// evidence behind.
    #[tokio::test]
    async fn stale_heartbeat_and_replacement_startup_cannot_remove_reader_protection() {
        let harness = ReaderAuthority::start().await;
        let node = harness.node_id;
        let fence = harness.register_oracle_role().await;
        let lease = std::time::Duration::from_secs(30);
        let mut conn = harness.system_conn().await;
        let acquired = OracleReaderEpochs::new(&mut conn)
            .expect("epochs are system owned")
            .acquire(node, fence, lease)
            .await
            .expect("epoch acquires");
        let activated = OracleReaderEpochs::new(&mut conn)
            .expect("epochs are system owned")
            .activate(node, fence, acquired.state_revision)
            .await
            .expect("epoch activates");
        conn.commit().await.expect("epoch startup commits");
        harness
            .commit_protection(
                fence,
                None,
                &harness.frontier(vec![30, 20, 10], 300, 100),
                OracleTableProtectionPhase::Expanded,
            )
            .await;
        let pool = harness.superuser().await;
        let before: (i64, i64) = sqlx::query_as(
            "SELECT (SELECT revision FROM vala.oracle_table_protections \
                      WHERE data_tenant_id = $1), \
                    (SELECT count(*) FROM vala.oracle_table_protection_members \
                      WHERE data_tenant_id = $1)",
        )
        .bind(harness.tenant.as_uuid())
        .fetch_one(&pool)
        .await
        .expect("protection state reads");

        // Stale the discovery heartbeat only, then start a replacement.
        sqlx::query(
            "UPDATE vala.cluster_nodes SET heartbeat_at = now() - interval '2 minutes' \
                      WHERE node_id = $1",
        )
        .bind(node)
        .execute(&pool)
        .await
        .expect("heartbeat ages");
        assert_eq!(harness.register_oracle_role().await, 2);

        let mut conn = harness.system_conn().await;
        assert!(
            OracleReaderEpochs::new(&mut conn)
                .expect("epochs are system owned")
                .invalidate_expired(node, fence)
                .await
                .expect("expiry check runs")
                .is_none(),
            "a stale heartbeat and a replacement are not proof of lease expiry"
        );
        conn.commit().await.expect("expiry check commits");
        let after: (i64, i64) = sqlx::query_as(
            "SELECT (SELECT revision FROM vala.oracle_table_protections \
                      WHERE data_tenant_id = $1), \
                    (SELECT count(*) FROM vala.oracle_table_protection_members \
                      WHERE data_tenant_id = $1)",
        )
        .bind(harness.tenant.as_uuid())
        .fetch_one(&pool)
        .await
        .expect("protection state re-reads");
        assert_eq!(after, before, "protection survives liveness alone");
        assert_eq!(
            audit_operations(&pool, harness.tenant).await,
            vec!["oracle.table_protection.expanded".to_owned()]
        );

        // Once Postgres itself proves the lease is past, invalidation and the
        // per-tenant release may proceed, in that order.
        sqlx::query(
            "UPDATE vala.oracle_reader_epochs \
                SET acquired_at = statement_timestamp() - interval '1 hour', \
                    activated_at = statement_timestamp() - interval '1 hour', \
                    renewed_at = statement_timestamp() - interval '1 hour', \
                    lease_expires_at = statement_timestamp() - interval '1 second' \
              WHERE node_id = $1 AND fencing_token = $2",
        )
        .bind(node)
        .bind(fence)
        .execute(&pool)
        .await
        .expect("lease expiry applies");
        assert_eq!(
            list_expired_epochs_for_operator(harness.fixture.operator_pool(), 64)
                .await
                .expect("expired scan runs")
                .len(),
            1
        );
        let mut conn = harness.system_conn().await;
        let invalidated = OracleReaderEpochs::new(&mut conn)
            .expect("epochs are system owned")
            .invalidate_expired(node, fence)
            .await
            .expect("expiry check runs")
            .expect("a provably expired lease invalidates");
        assert_eq!(invalidated, activated.state_revision + 1);
        append_audit(
            &mut conn,
            &epoch_event(
                node,
                fence,
                OracleReaderEpochPhase::Invalidated,
                invalidated,
            ),
        )
        .await
        .expect("invalidation audit appends");
        // Retirement before release is refused: the header is still protection.
        let premature = OracleReaderEpochs::new(&mut conn)
            .expect("epochs are system owned")
            .retire(node, fence, invalidated)
            .await;
        assert!(
            matches!(premature, Err(SqlError::InvariantViolation { .. })),
            "an epoch that still protects a table cannot retire: {premature:?}"
        );
        drop(conn);

        let mut conn = harness.system_conn().await;
        let invalidated = OracleReaderEpochs::new(&mut conn)
            .expect("epochs are system owned")
            .invalidate_expired(node, fence)
            .await
            .expect("expiry check runs")
            .expect("a provably expired lease invalidates");
        append_audit(
            &mut conn,
            &epoch_event(
                node,
                fence,
                OracleReaderEpochPhase::Invalidated,
                invalidated,
            ),
        )
        .await
        .expect("invalidation audit appends");
        conn.commit().await.expect("invalidation commits");

        let released = harness
            .commit_protection(
                fence,
                Some(1),
                &ProtectionFrontier::default(),
                OracleTableProtectionPhase::Released,
            )
            .await;
        assert!(matches!(released, ProtectionCas::Committed(_)));
        let mut conn = harness.system_conn().await;
        OracleReaderEpochs::new(&mut conn)
            .expect("epochs are system owned")
            .retire(node, fence, invalidated)
            .await
            .expect("a released epoch retires");
        append_audit(
            &mut conn,
            &epoch_event(node, fence, OracleReaderEpochPhase::Retired, invalidated),
        )
        .await
        .expect("retirement audit appends");
        conn.commit().await.expect("retirement commits");

        let remaining: (i64, i64) = sqlx::query_as(
            "SELECT (SELECT count(*) FROM vala.oracle_reader_epochs WHERE node_id = $1), \
                    (SELECT count(*) FROM vala.oracle_table_protection_members \
                      WHERE data_tenant_id = $2)",
        )
        .bind(node)
        .bind(harness.tenant.as_uuid())
        .fetch_one(&pool)
        .await
        .expect("final state reads");
        assert_eq!(remaining, (0, 0), "a safely retired epoch leaves nothing");
        assert_eq!(
            audit_operations(&pool, harness.tenant).await,
            vec![
                "oracle.table_protection.expanded".to_owned(),
                "oracle.table_protection.released".to_owned(),
            ]
        );
    }
}
