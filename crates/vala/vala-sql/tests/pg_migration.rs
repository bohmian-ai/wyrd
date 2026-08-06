mod pg_tests {
    //! Live Postgres Vala migration integration test.
    //!
    //! Skipped automatically when Wyrd database env vars are unset so the default
    //! test suite remains credential-free.

    use secrecy::ExposeSecret;
    use wyrd_dev_fixtures::pg::PgFixture;

    /// Fresh Vala migrations apply repeatedly without schema drift.
    #[tokio::test]
    async fn vala_migrations_apply_and_are_idempotent() {
        let Some(url) = std::env::var("WYRD_DATABASE_URL").ok() else {
            return;
        };
        let _ = url;

        let dsns = wyrd_sql::dsn::resolve_external_dsns_from_env()
            .expect("dsn resolve")
            .expect("WYRD_DATABASE_URL + WYRD_DATABASE_MIGRATOR_PASSWORD set");
        let pool = wyrd_sql::pool::build_pool(
            dsns.migrator.expose_secret(),
            wyrd_sql::PoolConfig::migrator_defaults(),
        )
        .await
        .expect("migrator pool");

        wyrd_sql::migrate(&pool)
            .await
            .expect("wyrd migrate is idempotent");
        vala_sql::migrate(&pool).await.expect("first vala migrate");
        vala_sql::migrate(&pool)
            .await
            .expect("second vala migrate is idempotent");

        for schema in vala_sql::OWNED_SCHEMAS {
            let exists: (bool,) =
                sqlx::query_as("SELECT EXISTS (SELECT 1 FROM pg_namespace WHERE nspname = $1)")
                    .bind(schema)
                    .fetch_one(&pool)
                    .await
                    .expect("schema query");
            assert!(exists.0, "{schema} schema exists after vala migrate");
        }
    }

    /// Exact migration 13 upgrades a real pre-Oracle schema and Scribe row.
    #[tokio::test]
    async fn old_scribe_membership_shape_is_preserved() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let pool = fixture.superuser_pool().await.expect("superuser pool");
        sqlx::raw_sql("DROP TABLE vala.cluster_nodes;")
            .execute(&pool)
            .await
            .expect("post-11 tables drop");
        sqlx::raw_sql(include_str!(
            "../migrations/20260910000003_vala_cluster_nodes.sql"
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
            "../migrations/20260910000013_oracle_coordination.sql"
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
            "peer_protocol_version":1,
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
