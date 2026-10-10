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
    //! `PostgreSQL` integration coverage for Oracle cut reads and Forge's
    //! snapshot retention.
    //!
    //! These tests drive the real cut function and the retention statement
    //! against a live database.

    use sqlx::{PgPool, Row};
    use uuid::Uuid;
    use vala_sql::queries::olap_catalog::upsert_table;
    use vala_sql::queries::oracle_reader_authority::{
        BifrostTableMaintenanceAuthority, CutTableRef, OracleTableCuts, TableCut,
    };
    use vala_sql::row_types::oracle_reader_authority::TableAuthorityIdentity;
    use vala_sql::{SqlError, TenantConn};
    use wyrd_dev_fixtures::pg::PgFixture;
    use wyrd_spec::DataTenantId;

    /// Logical namespace every table in this module is registered under.
    const NAMESPACE: &str = "vala.bifrost";

    /// Retention window every supersession assertion runs under.
    const RETENTION: std::time::Duration = std::time::Duration::from_hours(6);

    /// Canonical layout JSON one registration needs; its shape is opaque here.
    fn layout() -> serde_json::Value {
        serde_json::json!({
            "partition": { "column": "wyrd_event_time", "granularity": "hour" },
            "sort_keys": [],
            "bloom_columns": []
        })
    }

    /// One live database with registered tables and catalog pointers.
    struct CutReads {
        /// Repository Postgres fixture owning the isolated database.
        fixture: PgFixture,
        /// Superuser pool for out-of-band arrangement and inspection.
        superuser: PgPool,
        /// Platform-catalog pool that owns `iceberg_catalog` at runtime.
        catalog: PgPool,
    }

    impl CutReads {
        /// Starts the fixture and creates the runtime catalog relation.
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
            Self {
                fixture,
                superuser,
                catalog,
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

        /// Reads one tenant's cut over `tables` as one autocommit statement on
        /// a plain request-role connection, as Oracle does.
        ///
        /// # Errors
        ///
        /// Returns the cut statement's SQL error.
        ///
        /// # Panics
        ///
        /// Panics when the connection cannot be acquired.
        async fn read(
            &self,
            tenant: DataTenantId,
            tables: &[&str],
        ) -> Result<Vec<TableCut>, SqlError> {
            let refs: Vec<CutTableRef<'_>> = tables
                .iter()
                .map(|table| CutTableRef {
                    namespace_name: NAMESPACE,
                    table_name: table,
                })
                .collect();
            let mut conn = self.fixture.app_pool().acquire().await.expect("connection");
            OracleTableCuts::new(&mut conn, tenant).read(&refs).await
        }

        /// Records `replaced` for one table under its exclusive authority, as
        /// snapshot expiry does, and returns the retained snapshots.
        ///
        /// # Panics
        ///
        /// Panics when the authority or the statement fails.
        async fn retained(
            &self,
            identity: &TableAuthorityIdentity,
            replaced: &[i64],
            retention: std::time::Duration,
        ) -> Vec<i64> {
            let mut conn = self.conn(identity.tenant).await;
            let retained = BifrostTableMaintenanceAuthority::new(&mut conn)
                .retained_supersessions(identity, replaced, retention)
                .await
                .expect("supersessions record");
            conn.commit().await.expect("supersessions commit");
            retained
        }

        /// Moves every recorded supersession of a tenant back by `hours` in
        /// database time.
        ///
        /// # Panics
        ///
        /// Panics when the update fails.
        async fn age(&self, tenant: DataTenantId, hours: i32) {
            sqlx::query(
                "UPDATE vala.forge_snapshot_supersessions \
                    SET observed_at = observed_at - make_interval(hours => $2) \
                  WHERE data_tenant_id = $1",
            )
            .bind(tenant.as_uuid())
            .bind(hours)
            .execute(&self.superuser)
            .await
            .expect("supersessions age");
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

    /// Proves a cut is one read-only, tenant-scoped statement: deduplicated,
    /// ordered, all-or-nothing, and never blocked by Forge's exclusive
    /// maintenance authority.
    ///
    /// # Panics
    ///
    /// Panics when any privilege, grouping, isolation, absence, or
    /// non-blocking assertion fails.
    #[tokio::test]
    async fn table_cut_is_read_only_tenant_scoped_and_never_blocked() {
        let reads = CutReads::start().await;
        let pool = &reads.superuser;

        // Privilege boundary: the request role reaches the catalog only
        // through the narrow definer, and the cut runs as the caller.
        let functions = sqlx::query(
            "SELECT p.proname, p.prosecdef, p.provolatile::text AS volatility, \
                    pg_get_userbyid(p.proowner) AS owner, p.proconfig, \
                    has_function_privilege('wyrd_app', p.oid, 'EXECUTE') AS app_exec, \
                    has_function_privilege('public', p.oid, 'EXECUTE') AS public_exec \
               FROM pg_proc p JOIN pg_namespace n ON n.oid = p.pronamespace \
              WHERE n.nspname = 'vala' \
                AND p.proname IN ('oracle_catalog_metadata_location', 'oracle_read_table_cut') \
              ORDER BY p.proname",
        )
        .fetch_all(pool)
        .await
        .expect("function catalog reads");
        assert_eq!(functions.len(), 2);
        let pointer = &functions[0];
        assert!(
            pointer.get::<bool, _>("prosecdef"),
            "the pointer read is the one definer"
        );
        assert_eq!(pointer.get::<String, _>("owner"), "wyrd_platform_admin");
        assert_eq!(
            pointer.get::<Option<Vec<String>>, _>("proconfig"),
            Some(vec!["search_path=\"\"".to_owned()])
        );
        let cut = &functions[1];
        assert_eq!(cut.get::<String, _>("proname"), "oracle_read_table_cut");
        assert!(
            !cut.get::<bool, _>("prosecdef"),
            "the cut runs as the caller"
        );
        assert_eq!(cut.get::<String, _>("volatility"), "s", "the cut is STABLE");
        for row in &functions {
            assert!(row.get::<bool, _>("app_exec"));
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

        // One statement: ordered, deduplicated, empty hot set retained.
        let cuts = reads
            .read(tenant_a, &["events", "quiet", "events"])
            .await
            .expect("the cut reads");
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
                .all(|row| row.data_tenant_id == tenant_a.as_uuid()
                    && row.row_count == 100
                    && row.min_event_time.is_some()
                    && row.max_event_time.is_some()),
            "hot candidates project the tenant's durable row count and bounds"
        );
        assert_eq!(cuts[1].identity.table_name, "quiet");
        assert!(
            cuts[1].hot_files.is_empty(),
            "an empty hot set is a valid cut"
        );

        // Tenant isolation: the same logical name resolves to tenant B's own
        // pointer and hot rows.
        let cut_b = reads
            .read(tenant_b, &["events"])
            .await
            .expect("tenant B reads");
        assert_eq!(cut_b[0].metadata_location, "s3://b/events/v1.metadata.json");
        assert_eq!(cut_b[0].hot_files.len(), 1);
        assert_eq!(cut_b[0].hot_files[0].file_path, "b/hot-1.parquet");

        // The cut's tenant binding is statement-local: the next statement on
        // the same connection sees no tenant. Inside a transaction already
        // bound to another tenant the cut refuses instead of rebinding.
        let mut plain = reads
            .fixture
            .app_pool()
            .acquire()
            .await
            .expect("connection");
        OracleTableCuts::new(&mut plain, tenant_b)
            .read(&[CutTableRef {
                namespace_name: NAMESPACE,
                table_name: "events",
            }])
            .await
            .expect("the cut reads");
        let after: String =
            sqlx::query_scalar("SELECT current_setting('app.current_tenant', true)")
                .fetch_one(&mut *plain)
                .await
                .expect("setting reads");
        assert_eq!(after, "", "the binding ends with the cut statement");
        drop(plain);
        let mut bound = reads.conn(tenant_a).await;
        let refused = OracleTableCuts::new(bound.transaction(), tenant_b)
            .read(&[CutTableRef {
                namespace_name: NAMESPACE,
                table_name: "events",
            }])
            .await
            .expect_err("a cut never rebinds another tenant's transaction");
        assert!(
            refused
                .to_string()
                .contains("differs from the bound tenant"),
            "{refused}"
        );
        drop(bound);

        // A missing registration or pointer fails the whole statement.
        assert!(matches!(
            reads.read(tenant_b, &["events", "quiet"]).await,
            Err(SqlError::NoRows)
        ));
        assert!(matches!(
            reads.read(tenant_a, &["quiet", "unpublished"]).await,
            Err(SqlError::NoRows)
        ));

        // A live exclusive authority never blocks a cut, which observes the
        // pointer committed before it.
        let mut forge = reads.conn(tenant_a).await;
        let exclusive = BifrostTableMaintenanceAuthority::new(&mut forge)
            .exclusive(cuts[0].identity.clone())
            .await
            .expect("exclusive authority");
        let during = tokio::time::timeout(
            std::time::Duration::from_secs(10),
            reads.read(tenant_a, &["events"]),
        )
        .await
        .expect("a cut never waits on the exclusive authority")
        .expect("the cut reads");
        assert_eq!(
            during[0].metadata_location,
            "s3://a/events/v1.metadata.json"
        );
        drop(exclusive);
        forge
            .commit()
            .await
            .expect("forge surrenders its authority");
    }

    /// Proves snapshot retention keeps a replaced snapshot for the window
    /// measured from its first observation, forgets snapshots no longer
    /// replaced, and stays tenant-scoped.
    ///
    /// # Panics
    ///
    /// Panics when a snapshot is released early or late, an observation is
    /// re-stamped, or another tenant's observation is affected.
    #[tokio::test]
    async fn retained_supersessions_hold_replaced_snapshots_for_the_window() {
        let reads = CutReads::start().await;
        assert_eq!(
            grants(&reads.superuser, "forge_snapshot_supersessions", "wyrd_app").await,
            ["DELETE", "INSERT", "SELECT"]
        );
        let tenant_a = reads.tenant().await;
        let tenant_b = reads.tenant().await;
        reads
            .register(tenant_a, "events", Some("s3://a/events/v1.metadata.json"))
            .await;
        reads
            .register(tenant_b, "events", Some("s3://b/events/v1.metadata.json"))
            .await;
        let a = reads.read(tenant_a, &["events"]).await.expect("a cut")[0]
            .identity
            .clone();
        let b = reads.read(tenant_b, &["events"]).await.expect("b cut")[0]
            .identity
            .clone();

        assert_eq!(
            reads.retained(&a, &[1, 2], std::time::Duration::ZERO).await,
            Vec::<i64>::new(),
            "a zero window retains nothing, even on first observation"
        );
        assert_eq!(
            reads.retained(&a, &[1, 2], RETENTION).await,
            [1, 2],
            "an observation inside the window retains its snapshot"
        );
        reads.retained(&b, &[1], RETENTION).await;
        reads.age(tenant_a, 5).await;
        assert_eq!(
            reads.retained(&a, &[1, 2, 3], RETENTION).await,
            [1, 2, 3],
            "five hours is inside the window; a new observation is retained"
        );
        reads.age(tenant_a, 2).await;
        assert_eq!(
            reads.retained(&a, &[1, 2, 3], RETENTION).await,
            [3],
            "the first observation, not a later one, starts the window"
        );
        assert_eq!(
            reads.retained(&a, &[3], RETENTION).await,
            [3],
            "an expired snapshot's observation is forgotten"
        );
        reads.age(tenant_a, 7).await;
        assert_eq!(
            reads.retained(&a, &[1, 3], RETENTION).await,
            [1],
            "a re-replaced snapshot is observed afresh after being forgotten"
        );
        assert_eq!(
            reads.retained(&b, &[1], RETENTION).await,
            [1],
            "another tenant's observation is untouched"
        );
    }

    /// Proves every committed `vala.file_list` change notifies the Oracle cut
    /// channel with its table, and a rolled-back change notifies nothing.
    ///
    /// # Panics
    ///
    /// Panics when the listener cannot start, a committed write is not
    /// heard, or a rolled-back write is.
    #[tokio::test]
    async fn file_list_changes_notify_the_cut_channel_on_commit() {
        let reads = CutReads::start().await;
        let tenant = reads.tenant().await;
        reads
            .register(tenant, "events", Some("s3://a/events/v1.metadata.json"))
            .await;
        let mut listener = sqlx::postgres::PgListener::connect_with(reads.fixture.app_pool())
            .await
            .expect("listener connects");
        listener.listen("bifrost_cut").await.expect("listens");

        reads
            .hot_row(tenant, "events", "a/hot-1.parquet", false, None)
            .await;

        let heard = tokio::time::timeout(std::time::Duration::from_secs(10), listener.recv())
            .await
            .expect("a committed insert is heard")
            .expect("notification");
        assert_eq!(
            heard.payload(),
            format!("{}|{NAMESPACE}|events", tenant.as_uuid()),
            "the payload names exactly the changed table"
        );
        let mut rolled_back = reads.superuser.begin().await.expect("begin");
        let deleted = sqlx::query("DELETE FROM vala.file_list WHERE data_tenant_id = $1")
            .bind(tenant.as_uuid())
            .execute(&mut *rolled_back)
            .await
            .expect("delete runs");
        assert_eq!(deleted.rows_affected(), 1, "the trigger fired once");
        rolled_back.rollback().await.expect("rollback");
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(200), listener.recv())
                .await
                .is_err(),
            "a rolled-back change is never heard"
        );
    }
}
