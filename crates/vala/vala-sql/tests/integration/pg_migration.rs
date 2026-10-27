mod pg_tests {
    //! Live Postgres Vala migration integration test.
    //!
    //! Skipped automatically when Wyrd database env vars are unset so the default
    //! test suite remains credential-free.

    use secrecy::ExposeSecret;
    use wyrd_dev_fixtures::pg::PgFixture;
    use wyrd_sql::dsn::ResolvedDsns;
    use wyrd_sql::{MIGRATION_LEASE_WAIT, OperatorPool};

    /// Fresh Vala migrations apply repeatedly without schema drift and leave
    /// the retired mutable `vala.drift_alerts` table absent.
    ///
    /// # Panics
    ///
    /// Panics when a migration fails, an owned schema is missing, or
    /// `vala.drift_alerts` still resolves after migrating.
    #[tokio::test]
    async fn vala_migrations_apply_and_are_idempotent() {
        let Ok(dsns) = ResolvedDsns::from_env() else {
            return;
        };
        let pool = wyrd_sql::pool::build_pool(
            dsns.platform().expose_secret(),
            wyrd_sql::PoolConfig::migrator_defaults(),
        )
        .await
        .expect("platform pool");

        let mut lease = OperatorPool::from(pool.clone())
            .migration_lease(MIGRATION_LEASE_WAIT)
            .await
            .expect("migration lease acquires");
        wyrd_sql::migrate(&mut lease)
            .await
            .expect("wyrd migrate is idempotent");
        vala_sql::migrate(&mut lease)
            .await
            .expect("first vala migrate");
        vala_sql::migrate(&mut lease)
            .await
            .expect("second vala migrate is idempotent");
        lease.release().await.expect("migration lease releases");

        for schema in vala_sql::OWNED_SCHEMAS {
            let exists: (bool,) =
                sqlx::query_as("SELECT EXISTS (SELECT 1 FROM pg_namespace WHERE nspname = $1)")
                    .bind(schema)
                    .fetch_one(&pool)
                    .await
                    .expect("schema query");
            assert!(exists.0, "{schema} schema exists after vala migrate");
        }
        let drift_alerts: (bool,) =
            sqlx::query_as("SELECT to_regclass('vala.drift_alerts') IS NULL")
                .fetch_one(&pool)
                .await
                .expect("to_regclass query succeeds");
        assert!(
            drift_alerts.0,
            "the retired vala.drift_alerts table is absent after vala migrate"
        );
        let namespaces: String = sqlx::query_scalar(
            "SELECT pg_get_constraintdef(oid) FROM pg_constraint \
             WHERE conname = 'forge_tasks_namespace_name_check'",
        )
        .fetch_one(&pool)
        .await
        .expect("Forge namespace constraint reads");
        assert!(
            !namespaces.contains("vala.") && namespaces.contains("bifrost.verification"),
            "Forge admits only current namespaces: {namespaces}"
        );
    }

    /// The forward migration retiring `vala.eval` upgrades a schema still
    /// holding Eval Forge tasks: it deletes only those tasks, keeps every
    /// other namespace's task, and then refuses a new `vala.eval` task.
    ///
    /// # Panics
    ///
    /// Panics when the old state cannot be restored, the migration fails, an
    /// Eval task survives, another task is lost, or `vala.eval` is admitted.
    #[tokio::test]
    async fn retired_verification_namespace_upgrades() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let pool = fixture.superuser_pool().expect("superuser pool");
        sqlx::raw_sql(
            "ALTER TABLE vala.forge_tasks DROP CONSTRAINT forge_tasks_namespace_name_check, \
             ADD CONSTRAINT forge_tasks_namespace_name_check CHECK (namespace_name IN \
             ('vala.system','vala.bifrost','vala.traces','vala.metrics','vala.logs','vala.eval',\
             'vala.drift','vala.verification','vala.dev','vala.datasets','vala.gateway'))",
        )
        .execute(&pool)
        .await
        .expect("the pre-retirement namespace constraint restores");
        let insert = |namespace: &'static str| {
            let task_id = uuid::Uuid::now_v7();
            let pool = pool.clone();
            async move {
                sqlx::query(
                    "INSERT INTO vala.forge_tasks (task_id,data_tenant_id,catalog_name,namespace_name,\
                     table_name,strategy,base_snapshot_id,plan,plan_hash,estimated_files,\
                     estimated_bytes,state,ready_at) VALUES ($1,$2,'wyrd-redux',$3,'t',\
                     'small_files',1,'{}'::jsonb,decode(repeat('00',32),'hex'),1,1,'ready',now())",
                )
                .bind(task_id)
                .bind(uuid::Uuid::now_v7())
                .bind(namespace)
                .execute(&pool)
                .await
                .map(|_| task_id)
            }
        };
        let retired = insert("vala.eval").await.expect("old Eval task inserts");
        let current = insert("vala.verification")
            .await
            .expect("current task inserts");

        sqlx::raw_sql(include_str!(
            "../../migrations/20261016000000_retire_eval_namespace.sql"
        ))
        .execute(&pool)
        .await
        .expect("the retirement migration applies");

        let remaining: Vec<uuid::Uuid> =
            sqlx::query_scalar("SELECT task_id FROM vala.forge_tasks WHERE task_id = ANY($1)")
                .bind(vec![retired, current])
                .fetch_all(&pool)
                .await
                .expect("tasks read");
        assert_eq!(remaining, vec![current]);
        assert!(
            insert("vala.eval").await.is_err(),
            "a new vala.eval task is refused"
        );
    }

    /// Exact migration 13 upgrades a real pre-Oracle schema and Scribe row.
    #[tokio::test]
    async fn old_scribe_membership_shape_is_preserved() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let pool = fixture.superuser_pool().expect("superuser pool");
        sqlx::raw_sql("DROP TABLE vala.cluster_nodes;")
            .execute(&pool)
            .await
            .expect("post-11 tables drop");
        sqlx::raw_sql(include_str!(
            "../../migrations/20260910000003_vala_cluster_nodes.sql"
        ))
        .execute(&pool)
        .await
        .expect("pre-11 cluster schema applies");
        let node_id = uuid::Uuid::now_v7();
        sqlx::query(
            "INSERT INTO vala.cluster_nodes \
             (node_id,role,advertise_addr,fencing_token,started_at,heartbeat_at,meta) \
             VALUES ($1,'scribe','http://scribe:5001',1,now(),now(),'{}'::jsonb)",
        )
        .bind(node_id)
        .execute(&pool)
        .await
        .expect("pre-11 Scribe row inserts");
        sqlx::raw_sql(include_str!(
            "../../migrations/20260910000013_oracle_coordination.sql"
        ))
        .execute(&pool)
        .await
        .expect("exact migration 13 applies");
        let row: (uuid::Uuid, i16, serde_json::Value, bool) = sqlx::query_as(
            "SELECT data_tenant_id,capability_version,capabilities,ready \
             FROM vala.cluster_nodes \
             WHERE node_id=$1 AND role='scribe'",
        )
        .bind(node_id)
        .fetch_one(&pool)
        .await
        .expect("preserved Scribe row reads");
        assert_eq!(
            row.0,
            uuid::Uuid::from(wyrd_spec::DataTenantId::SYSTEM_OWNER)
        );
        assert_eq!(row.1, 1);
        assert_eq!(
            row.2,
            serde_json::json!({"kind":"scribe_v1","tail_protocol_version":1})
        );
        assert!(!row.3);
        sqlx::query(
            "INSERT INTO vala.cluster_nodes \
             (data_tenant_id,node_id,role,advertise_addr,fencing_token,started_at,heartbeat_at,capabilities) \
             VALUES ($1,$2,'oracle','http://oracle:5002',1,now(),now(),$3)",
        )
        .bind(uuid::Uuid::from(wyrd_spec::DataTenantId::SYSTEM_OWNER))
        .bind(node_id)
        .bind(serde_json::json!({
            "kind":"oracle_v1",
            "storage_protocol_version":1
        }))
        .execute(&pool)
        .await
        .expect("same node Oracle role inserts under composite primary key");
        let roles: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM vala.cluster_nodes \
             WHERE data_tenant_id=$1 AND node_id=$2",
        )
        .bind(uuid::Uuid::from(wyrd_spec::DataTenantId::SYSTEM_OWNER))
        .bind(node_id)
        .fetch_one(&pool)
        .await
        .expect("role count reads");
        assert_eq!(roles, 2);
    }
}
