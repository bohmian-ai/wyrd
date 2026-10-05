//! `PostgreSQL` integration coverage for Oracle membership and role fencing.

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
        liveness: std::time::Duration,
    ) -> Result<Vec<wyrd_spec::vala::api::ClusterRoleLease>, vala_sql::SqlError> {
        let mut conn = self
            .inner
            .postgres()
            .tenant_conn(DataTenantId::SYSTEM_OWNER)
            .await?;
        let rows = self.inner.list_live(&mut conn, role, liveness).await?;
        conn.commit().await?;
        Ok(rows)
    }

    /// Ages a registered node heartbeat in database time to exercise the
    /// discovery window without trusting the test host clock.
    ///
    /// # Errors
    ///
    /// Returns the SQL error from tenant connection setup, update, or commit.
    async fn age_heartbeat(
        &self,
        node_id: NodeId,
        age_seconds: i64,
    ) -> Result<(), vala_sql::SqlError> {
        let mut conn = self
            .inner
            .postgres()
            .tenant_conn(DataTenantId::SYSTEM_OWNER)
            .await?;
        sqlx::query(
            "UPDATE vala.cluster_nodes \
                SET heartbeat_at=statement_timestamp() - ($2 * interval '1 second') \
             WHERE data_tenant_id=$1 AND node_id=$3",
        )
        .bind(uuid::Uuid::from(conn.data_tenant_id()))
        .bind(age_seconds as f64)
        .bind(node_id.as_uuid())
        .execute(&mut **conn.transaction())
        .await
        .map_err(vala_sql::SqlError::from)?;
        conn.commit().await?;
        Ok(())
    }
}

/// Reads one node's durable `started_at` and `heartbeat_at`, plus whether the
/// heartbeat was stamped by `PostgreSQL` rather than copied from `started_at`.
///
/// # Panics
///
/// Panics when the projection cannot be read.
async fn registration_times(
    fixture: &PgFixture,
    node_id: NodeId,
) -> (chrono::DateTime<Utc>, chrono::DateTime<Utc>) {
    let mut conn = fixture
        .vala_postgres()
        .tenant_conn(DataTenantId::SYSTEM_OWNER)
        .await
        .expect("system tenant connection opens");
    let row = sqlx::query_as::<_, (chrono::DateTime<Utc>, chrono::DateTime<Utc>)>(
        "SELECT started_at, heartbeat_at FROM vala.cluster_nodes WHERE node_id=$1",
    )
    .bind(node_id.as_uuid())
    .fetch_one(&mut **conn.transaction())
    .await
    .expect("registration row is readable");
    conn.commit().await.expect("read commits");
    row
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
            .list_live(ClusterRole::Scribe, std::time::Duration::from_secs(30))
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
    // A process clock that is far behind or far ahead of PostgreSQL must not
    // decide liveness: registration stamps heartbeat_at in the database.
    for skewed_start in [
        Utc::now() - Duration::seconds(3600),
        Utc::now() + Duration::seconds(3600),
    ] {
        let registration = RoleRegistration {
            key: ClusterNodeKey {
                node_id: NodeId::new(uuid::Uuid::now_v7()),
                role: ClusterRole::Scribe,
            },
            address: "http://scribe:5001".into(),
            capabilities: ClusterCapabilities::ScribeV1(ScribeCapabilitiesV1 {
                tail_protocol_version: 1,
            }),
            started_at: skewed_start,
        };
        owner.register(&registration).await.expect("role registers");
        let (started_at, heartbeat_at) =
            registration_times(&fixture, registration.key.node_id).await;
        assert!(
            (started_at - skewed_start).num_milliseconds().abs() < 1,
            "started_at remains caller-supplied process metadata"
        );
        assert!(
            (heartbeat_at - skewed_start).num_seconds().abs() > 60,
            "heartbeat_at must be database-stamped, not copied from started_at"
        );
        owner
            .heartbeat(&registration.key, 1, true, &registration.capabilities)
            .await
            .expect("heartbeat applies");
        assert!(
            owner
                .list_live(ClusterRole::Scribe, std::time::Duration::from_secs(30))
                .await
                .expect("live roles list")
                .iter()
                .any(|lease| lease.key.node_id == registration.key.node_id),
            "a freshly registered role is live regardless of process clock skew"
        );

        owner
            .age_heartbeat(registration.key.node_id, 120)
            .await
            .expect("heartbeat ages");
        assert!(
            owner
                .list_live(ClusterRole::Scribe, std::time::Duration::from_secs(30))
                .await
                .expect("live roles list")
                .iter()
                .all(|lease| lease.key.node_id != registration.key.node_id),
            "expired heartbeat must not be live"
        );
    }
}

mod pg_tests {
    //! `PostgreSQL` integration coverage for Oracle active table reads.
    //!
    //! These tests drive the real acquisition function, release, and Forge's
    //! destructive gate against a live database. Every assertion is about the
    //! durable evidence Forge consumes, so each one fails closed.

    use chrono::Utc;
    use sqlx::{PgPool, Row};
    use uuid::Uuid;
    use vala_sql::queries::cluster_nodes::ClusterNodes;
    use vala_sql::queries::olap_catalog::upsert_table;
    use vala_sql::queries::oracle_reader_authority::{
        AcquiredTableCut, ActiveReadOwner, ActiveTableRef, BifrostTableMaintenanceAuthority,
        OracleActiveTableReads,
    };
    use vala_sql::row_types::cluster_nodes::RoleRegistration;
    use vala_sql::row_types::oracle_reader_authority::TableAuthorityIdentity;
    use vala_sql::{SqlError, TenantConn};
    use wyrd_dev_fixtures::pg::PgFixture;
    use wyrd_spec::DataTenantId;
    use wyrd_spec::vala::api::{
        ClusterCapabilities, ClusterNodeKey, ClusterRole, OracleCapabilitiesV1, QueryClass,
    };

    /// Logical namespace every table in this module is registered under.
    const NAMESPACE: &str = "vala.bifrost";

    /// Remaining query time every fixture acquisition binds; long enough that
    /// no row it writes is abandoned during the test.
    const QUERY_DEADLINE: std::time::Duration = std::time::Duration::from_hours(1);

    /// Canonical layout JSON one registration needs; its shape is opaque here.
    fn layout() -> serde_json::Value {
        serde_json::json!({
            "partition": { "column": "wyrd_event_time", "granularity": "hour" },
            "sort_keys": [],
            "bloom_columns": []
        })
    }

    /// One live database with two tenants, their registered tables, catalog
    /// pointers, and one live Oracle fence.
    struct ActiveReads {
        /// Repository Postgres fixture owning the isolated database.
        fixture: PgFixture,
        /// Superuser pool for out-of-band arrangement and inspection.
        superuser: PgPool,
        /// Platform-catalog pool that owns `iceberg_catalog` at runtime.
        catalog: PgPool,
        /// Physical node holding the Oracle fence every claim names.
        node_id: Uuid,
        /// The node's current Oracle fencing token.
        fence: i64,
    }

    impl ActiveReads {
        /// Starts the fixture, creates the runtime catalog relation, and
        /// registers one Oracle role.
        ///
        /// # Panics
        ///
        /// Panics when any fixture, catalog, or membership step fails.
        async fn start() -> Self {
            let fixture = PgFixture::start().await.expect("fixture starts");
            let superuser = fixture.superuser_pool().await.expect("superuser pool");
            let catalog =
                PgPool::connect(secrecy::ExposeSecret::expose_secret(fixture.catalog_dsn()))
                    .await
                    .expect("catalog pool");
            sqlx::query(
                "CREATE TABLE IF NOT EXISTS iceberg_catalog.iceberg_tables (
                    catalog_name VARCHAR(255) NOT NULL,
                    table_namespace VARCHAR(255) NOT NULL,
                    table_name VARCHAR(255) NOT NULL,
                    metadata_location VARCHAR(1000),
                    previous_metadata_location VARCHAR(1000),
                    iceberg_type VARCHAR(5),
                    PRIMARY KEY (catalog_name, table_namespace, table_name))",
            )
            .execute(&catalog)
            .await
            .expect("runtime catalog relation exists");
            let node_id = Uuid::now_v7();
            let nodes = ClusterNodes::new(fixture.vala_postgres().clone());
            let mut conn = fixture
                .vala_postgres()
                .tenant_conn(DataTenantId::SYSTEM_OWNER)
                .await
                .expect("system connection");
            let row = nodes
                .register(
                    &mut conn,
                    &RoleRegistration {
                        key: ClusterNodeKey {
                            node_id: wyrd_spec::vala::api::NodeId::new(node_id),
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
            Self {
                fence: i64::try_from(row.lease.fencing_token).expect("fence fits an i64"),
                fixture,
                superuser,
                catalog,
                node_id,
            }
        }

        /// Seeds one tenant.
        ///
        /// # Panics
        ///
        /// Panics when the tenant cannot be seeded.
        async fn tenant(&self) -> DataTenantId {
            let tenant = DataTenantId::new_v7();
            self.fixture
                .seed_additional_tenant_with_uuid(
                    tenant,
                    &format!("reads-{}", tenant.as_uuid().simple()),
                )
                .await
                .expect("tenant seeds");
            tenant
        }

        /// Registers one table for a tenant and, when given, its catalog pointer.
        ///
        /// # Panics
        ///
        /// Panics when registration or the catalog insert fails.
        async fn register(&self, tenant: DataTenantId, table: &str, pointer: Option<&str>) {
            let mut conn = self.conn(tenant).await;
            upsert_table(
                &mut conn,
                Uuid::now_v7().as_bytes(),
                &format!("{NAMESPACE}.{table}"),
                &[9_u8; 32],
                &layout(),
            )
            .await
            .expect("table registers");
            conn.commit().await.expect("registration commits");
            if let Some(pointer) = pointer {
                self.move_pointer(tenant, table, pointer).await;
            }
        }

        /// Writes a table's catalog pointer as the runtime SQL catalog would.
        ///
        /// # Panics
        ///
        /// Panics when the catalog upsert fails.
        async fn move_pointer(&self, tenant: DataTenantId, table: &str, pointer: &str) {
            sqlx::query(
                "INSERT INTO iceberg_catalog.iceberg_tables
                     (catalog_name, table_namespace, table_name, metadata_location, iceberg_type)
                 VALUES ('wyrd-redux', $1, $2, $3, 'TABLE')
                 ON CONFLICT (catalog_name, table_namespace, table_name)
                 DO UPDATE SET metadata_location = EXCLUDED.metadata_location",
            )
            .bind(format!("vala.tenants.{tenant}.bifrost"))
            .bind(table)
            .bind(pointer)
            .execute(&self.catalog)
            .await
            .expect("catalog pointer writes");
        }

        /// Inserts one `file_list` row for a tenant's table.
        ///
        /// # Panics
        ///
        /// Panics when fixture SQL cannot arrange the row.
        async fn hot_row(
            &self,
            tenant: DataTenantId,
            table: &str,
            path: &str,
            compacted: bool,
            committed_snapshot_id: Option<i64>,
        ) {
            sqlx::query(
                r#"
                INSERT INTO vala.file_list (
                    id, data_tenant_id, namespace, table_name, file_path,
                    file_size, row_count, min_event_time, max_event_time,
                    partition_granularity, partition_start, node_id, writer_epoch,
                    wal_lsn_min, wal_lsn_max, promotion_record,
                    compacted, committed_snapshot_id
                ) VALUES (
                    $1, $2, $3, $4, $5, 1024, 100, now(), now(),
                    'day', date_trunc('day', now() AT TIME ZONE 'UTC') AT TIME ZONE 'UTC',
                    $6, 1, 100, 200, '{"fixture": "active-reads"}'::jsonb, $7, $8
                )
                "#,
            )
            .bind(Uuid::now_v7())
            .bind(tenant.as_uuid())
            .bind(NAMESPACE)
            .bind(table)
            .bind(path)
            .bind(Uuid::now_v7())
            .bind(compacted)
            .bind(committed_snapshot_id)
            .execute(&self.superuser)
            .await
            .expect("file_list row inserts");
        }

        /// Opens one tenant transaction on the request role.
        ///
        /// # Panics
        ///
        /// Panics when the connection cannot be acquired.
        async fn conn(&self, tenant: DataTenantId) -> TenantConn<'_> {
            TenantConn::acquire(self.fixture.app_pool(), tenant)
                .await
                .expect("tenant connection")
        }

        /// The exact fence a query on this node records.
        fn owner(&self, query_id: Uuid) -> ActiveReadOwner {
            ActiveReadOwner {
                query_id,
                node_id: self.node_id,
                fencing_token: self.fence,
            }
        }

        /// Acquires a cut for one query and commits it.
        ///
        /// # Errors
        ///
        /// Returns the acquisition's SQL error; the transaction then rolls back.
        async fn acquire(
            &self,
            tenant: DataTenantId,
            query_id: Uuid,
            tables: &[&str],
        ) -> Result<Vec<AcquiredTableCut>, SqlError> {
            let refs: Vec<ActiveTableRef<'_>> = tables
                .iter()
                .map(|table| ActiveTableRef {
                    namespace_name: NAMESPACE,
                    table_name: table,
                })
                .collect();
            let mut conn = self.conn(tenant).await;
            let cuts = OracleActiveTableReads::new(&mut conn)
                .acquire(self.owner(query_id), QUERY_DEADLINE, &refs)
                .await?;
            conn.commit().await?;
            Ok(cuts)
        }

        /// Counts every active read a query holds, across tenants.
        ///
        /// # Panics
        ///
        /// Panics when the inspection fails.
        async fn rows_for(&self, query_id: Uuid) -> i64 {
            sqlx::query_scalar(
                "SELECT count(*) FROM vala.oracle_active_table_reads WHERE query_id = $1",
            )
            .bind(query_id)
            .fetch_one(&self.superuser)
            .await
            .expect("active reads count")
        }

        /// Runs Forge's destructive gate for one acquired table: a tenant
        /// transaction takes the table's exclusive maintenance authority,
        /// which discards abandoned reads and exists only while no read
        /// remains, then surrenders it; committed so any discard sticks.
        ///
        /// # Errors
        ///
        /// Returns [`SqlError::Conflict`] while an active read remains, or the
        /// SQL failure.
        async fn forge_gate(&self, identity: &TableAuthorityIdentity) -> Result<(), SqlError> {
            let mut conn = TenantConn::acquire(self.fixture.app_pool(), identity.tenant).await?;
            let exclusive = BifrostTableMaintenanceAuthority::new(&mut conn)
                .exclusive(identity.clone())
                .await?;
            let granted = exclusive.is_some();
            drop(exclusive);
            conn.commit().await?;
            if !granted {
                return Err(SqlError::Conflict {
                    detail: "an Oracle query is still reading this table".to_owned(),
                });
            }
            Ok(())
        }

        /// Moves a query's abandonment time into the past or the future in
        /// database time.
        ///
        /// # Panics
        ///
        /// Panics when the update fails.
        async fn set_abandoned(&self, query_id: Uuid, past: bool) {
            sqlx::query(
                "UPDATE vala.oracle_active_table_reads \
                    SET acquired_at = statement_timestamp() - interval '2 hours', \
                        abandon_after = statement_timestamp() \
                          + CASE WHEN $2 THEN interval '-1 hour' ELSE interval '1 hour' END \
                  WHERE query_id = $1",
            )
            .bind(query_id)
            .bind(past)
            .execute(&self.superuser)
            .await
            .expect("abandonment time moves");
        }
    }

    /// Projects the privileges one role holds on one relation, sorted.
    ///
    /// # Panics
    ///
    /// Panics when the `information_schema` privilege read fails.
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

    /// Proves cut acquisition is one atomic, tenant-scoped statement whose
    /// active reads serialize with Forge and expire only once PostgreSQL time
    /// passes the query deadline bound at acquisition.
    ///
    /// # Panics
    ///
    /// Panics when any privilege, grouping, isolation, serialization,
    /// abandonment, or release assertion fails.
    #[tokio::test]
    async fn active_table_read_claim_is_atomic_tenant_scoped_and_postgres_expired() {
        let reads = ActiveReads::start().await;
        let pool = &reads.superuser;

        // Privilege boundary: the request role reaches the catalog only
        // through the narrow definer, and the acquisition runs as the caller.
        assert_eq!(
            grants(pool, "oracle_active_table_reads", "wyrd_app").await,
            ["DELETE", "INSERT", "SELECT", "UPDATE"]
        );
        assert_eq!(
            grants(pool, "oracle_active_table_reads", "wyrd_platform_admin").await,
            ["DELETE", "SELECT"]
        );
        let functions = sqlx::query(
            "SELECT p.proname, p.prosecdef, p.provolatile::text AS volatility, \
                    pg_get_userbyid(p.proowner) AS owner, p.proconfig, \
                    has_function_privilege('wyrd_app', p.oid, 'EXECUTE') AS app_exec, \
                    has_function_privilege('public', p.oid, 'EXECUTE') AS public_exec \
               FROM pg_proc p JOIN pg_namespace n ON n.oid = p.pronamespace \
              WHERE n.nspname = 'vala' \
                AND p.proname IN ('oracle_catalog_metadata_location', 'oracle_acquire_table_cut') \
              ORDER BY p.proname",
        )
        .fetch_all(pool)
        .await
        .expect("function catalog reads");
        assert_eq!(functions.len(), 2);
        let acquire = &functions[0];
        assert_eq!(
            acquire.get::<String, _>("proname"),
            "oracle_acquire_table_cut"
        );
        assert!(
            !acquire.get::<bool, _>("prosecdef"),
            "acquisition runs as the caller"
        );
        assert_eq!(acquire.get::<String, _>("volatility"), "v");
        assert!(acquire.get::<bool, _>("app_exec"));
        let pointer = &functions[1];
        assert!(
            pointer.get::<bool, _>("prosecdef"),
            "the pointer read is the one definer"
        );
        assert_eq!(pointer.get::<String, _>("owner"), "wyrd_platform_admin");
        assert_eq!(
            pointer.get::<Option<Vec<String>>, _>("proconfig"),
            Some(vec!["search_path=\"\"".to_owned()])
        );
        assert!(pointer.get::<bool, _>("app_exec"));
        for row in &functions {
            assert!(
                !row.get::<bool, _>("public_exec"),
                "PUBLIC holds no execute privilege"
            );
        }

        let tenant_a = reads.tenant().await;
        let tenant_b = reads.tenant().await;
        reads
            .register(tenant_a, "events", Some("s3://a/events/v1.metadata.json"))
            .await;
        reads
            .register(tenant_a, "quiet", Some("s3://a/quiet/v1.metadata.json"))
            .await;
        reads.register(tenant_a, "unpublished", None).await;
        reads
            .register(tenant_b, "events", Some("s3://b/events/v1.metadata.json"))
            .await;
        reads
            .hot_row(tenant_a, "events", "a/hot-1.parquet", false, None)
            .await;
        reads
            .hot_row(tenant_a, "events", "a/moving.parquet", true, None)
            .await;
        reads
            .hot_row(tenant_a, "events", "a/settled.parquet", true, Some(7))
            .await;
        reads
            .hot_row(tenant_b, "events", "b/hot-1.parquet", false, None)
            .await;

        let mut direct = reads.conn(tenant_a).await;
        let denied = sqlx::query("SELECT 1 FROM iceberg_catalog.iceberg_tables")
            .execute(&mut **direct.transaction())
            .await
            .expect_err("the request role cannot read the catalog directly");
        assert!(denied.to_string().contains("permission denied"), "{denied}");
        drop(direct);

        // One statement: ordered, deduplicated, empty hot set retained, and
        // claims invisible until the caller commits.
        let q1 = Uuid::now_v7();
        let refs = [
            ActiveTableRef {
                namespace_name: NAMESPACE,
                table_name: "events",
            },
            ActiveTableRef {
                namespace_name: NAMESPACE,
                table_name: "quiet",
            },
            ActiveTableRef {
                namespace_name: NAMESPACE,
                table_name: "events",
            },
        ];
        let mut conn = reads.conn(tenant_a).await;
        let cuts = OracleActiveTableReads::new(&mut conn)
            .acquire(reads.owner(q1), QUERY_DEADLINE, &refs)
            .await
            .expect("acquisition succeeds");
        assert_eq!(
            reads.rows_for(q1).await,
            0,
            "claims are invisible before commit"
        );
        conn.commit().await.expect("acquisition commits");
        assert_eq!(cuts.len(), 2, "a repeated table is one cut");
        assert_eq!(cuts[0].identity.table_name, "events");
        assert_eq!(cuts[0].metadata_location, "s3://a/events/v1.metadata.json");
        let mut paths: Vec<&str> = cuts[0]
            .hot_files
            .iter()
            .map(|row| row.file_path.as_str())
            .collect();
        paths.sort_unstable();
        assert_eq!(paths, ["a/hot-1.parquet", "a/moving.parquet"]);
        assert!(
            cuts[0]
                .hot_files
                .iter()
                .all(|row| row.data_tenant_id == tenant_a.as_uuid())
        );
        assert!(
            cuts[0].hot_files.iter().all(|row| row.row_count == 100
                && row.min_event_time.is_some()
                && row.max_event_time.is_some()),
            "hot candidates project the durable row count and event-time bounds"
        );
        assert_eq!(cuts[1].identity.table_name, "quiet");
        assert!(
            cuts[1].hot_files.is_empty(),
            "an empty hot set is a valid cut"
        );
        assert_eq!(reads.rows_for(q1).await, 2, "one claim per distinct table");
        let lifetime: (bool, f64) = sqlx::query_as(
            "SELECT bool_and(node_id = $2 AND fencing_token = $3), \
                    max(extract(epoch FROM abandon_after - acquired_at))::float8 \
               FROM vala.oracle_active_table_reads WHERE query_id = $1",
        )
        .bind(q1)
        .bind(reads.node_id)
        .bind(reads.fence)
        .fetch_one(pool)
        .await
        .expect("claim fence reads");
        assert_eq!(
            lifetime,
            (true, 3600.0),
            "exact fence, Postgres expiry at the bound query deadline"
        );

        // Tenant isolation: the same logical name resolves to tenant B's own
        // pointer and hot rows, and B cannot see A's claims.
        let q2 = Uuid::now_v7();
        let cut_b = reads
            .acquire(tenant_b, q2, &["events"])
            .await
            .expect("tenant B acquires");
        assert_eq!(cut_b[0].metadata_location, "s3://b/events/v1.metadata.json");
        assert_eq!(cut_b[0].hot_files.len(), 1);
        assert_eq!(cut_b[0].hot_files[0].file_path, "b/hot-1.parquet");
        let mut conn = reads.conn(tenant_b).await;
        let visible: i64 =
            sqlx::query_scalar("SELECT count(*) FROM vala.oracle_active_table_reads")
                .fetch_one(&mut **conn.transaction())
                .await
                .expect("tenant B reads its claims");
        assert_eq!(visible, 1, "RLS hides tenant A's claims");
        drop(conn);

        // A missing registration or pointer fails the whole statement.
        let q3 = Uuid::now_v7();
        assert!(matches!(
            reads.acquire(tenant_b, q3, &["events", "quiet"]).await,
            Err(SqlError::NoRows)
        ));
        assert!(matches!(
            reads.acquire(tenant_a, q3, &["quiet", "unpublished"]).await,
            Err(SqlError::NoRows)
        ));
        assert_eq!(reads.rows_for(q3).await, 0, "no partial claim set commits");

        // Serialization: a live exclusive authority finishes its effect
        // first, and the waiting acquisition observes its later pointer.
        // A dedicated table keeps the earlier readers out of this ordering.
        let events_a = cuts[0].identity.clone();
        reads
            .register(tenant_a, "serial", Some("s3://a/serial/v1.metadata.json"))
            .await;
        let probe = Uuid::now_v7();
        let serial = reads
            .acquire(tenant_a, probe, &["serial"])
            .await
            .expect("probe acquires")[0]
            .identity
            .clone();
        let mut releaser = reads.conn(tenant_a).await;
        OracleActiveTableReads::new(&mut releaser)
            .release(probe)
            .await
            .expect("probe releases");
        releaser.commit().await.expect("probe release commits");
        let mut forge = reads.conn(tenant_a).await;
        let exclusive = BifrostTableMaintenanceAuthority::new(&mut forge)
            .exclusive(serial)
            .await
            .expect("exclusive authority statement")
            .expect("no read holds the table");
        let q4 = Uuid::now_v7();
        let waiting = {
            let app = reads.fixture.app_pool().clone();
            let owner = reads.owner(q4);
            tokio::spawn(async move {
                let mut conn = TenantConn::acquire(&app, tenant_a).await?;
                let cut = OracleActiveTableReads::new(&mut conn)
                    .acquire(
                        owner,
                        QUERY_DEADLINE,
                        &[ActiveTableRef {
                            namespace_name: NAMESPACE,
                            table_name: "serial",
                        }],
                    )
                    .await?;
                conn.commit().await?;
                Ok::<_, SqlError>(cut)
            })
        };
        loop {
            let blocked: i64 = sqlx::query_scalar(
                "SELECT count(*) FROM pg_stat_activity \
                  WHERE wait_event_type = 'Lock' AND query LIKE '%oracle_acquire_table_cut%'",
            )
            .fetch_one(pool)
            .await
            .expect("lock waits read");
            if blocked == 1 {
                break;
            }
            tokio::task::yield_now().await;
        }
        // The destructive effect lands while the authority is still held.
        reads
            .move_pointer(tenant_a, "serial", "s3://a/serial/v2.metadata.json")
            .await;
        assert!(
            !waiting.is_finished(),
            "the reader cannot commit while the exclusive authority is live"
        );
        drop(exclusive);
        forge
            .commit()
            .await
            .expect("forge surrenders its authority");
        let later = waiting
            .await
            .expect("acquisition task joins")
            .expect("acquisition succeeds");
        assert_eq!(later[0].metadata_location, "s3://a/serial/v2.metadata.json");

        // The other ordering: a committed reader makes Forge refuse.
        assert!(matches!(
            reads.forge_gate(&events_a).await,
            Err(SqlError::Conflict { .. })
        ));

        // A read is abandoned only once PostgreSQL time passes its own
        // abandon_after; the owner's fence plays no part.
        let mut releaser = reads.conn(tenant_a).await;
        assert_eq!(
            OracleActiveTableReads::new(&mut releaser)
                .release(q4)
                .await
                .expect("release"),
            1
        );
        releaser.commit().await.expect("release commits");
        reads.set_abandoned(q1, false).await;
        assert!(matches!(
            reads.forge_gate(&events_a).await,
            Err(SqlError::Conflict { .. })
        ));
        assert_eq!(
            reads.rows_for(q1).await,
            2,
            "a read before its deadline is never discarded"
        );
        reads.set_abandoned(q1, true).await;
        reads
            .forge_gate(&events_a)
            .await
            .expect("a read past its deadline is discarded");
        assert_eq!(
            reads.rows_for(q1).await,
            1,
            "only the gated table's row is discarded"
        );

        // Release is idempotent.
        let mut releaser = reads.conn(tenant_a).await;
        let mut owner = OracleActiveTableReads::new(&mut releaser);
        assert_eq!(owner.release(q1).await.expect("release"), 1);
        assert_eq!(owner.release(q1).await.expect("repeat release"), 0);
        releaser.commit().await.expect("release commits");
        assert_eq!(reads.rows_for(q1).await, 0);
        assert_eq!(
            reads.rows_for(q2).await,
            1,
            "another tenant's claim is untouched"
        );
    }
}
