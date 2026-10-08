mod pg_tests {
    //! Live Postgres Vala migration integration test.
    //!
    //! Skipped automatically when Wyrd database env vars are unset so the default
    //! test suite remains credential-free.

    use sha2::Digest;
    use wyrd_dev_fixtures::pg::PgFixture;
    use wyrd_sql::{MIGRATION_LEASE_WAIT, OperatorPool};

    /// Fresh Vala migrations apply repeatedly without schema drift and leave
    /// the retired mutable `vala.drift_alerts` table and the retired audit
    /// staging tables and immutability function absent.
    ///
    /// # Panics
    ///
    /// Panics when a migration fails, an owned schema is missing, or a
    /// retired relation or function still resolves after migrating.
    #[tokio::test]
    async fn vala_migrations_apply_and_are_idempotent() {
        let Some(owner_url) = std::env::var("WYRD_TEST_DATABASE_ADMIN_URL").ok() else {
            return;
        };
        let pool =
            wyrd_sql::pool::build_pool(&owner_url, wyrd_sql::PoolConfig::migrator_defaults())
                .await
                .expect("owner pool");

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
        let audit_staging: (bool,) = sqlx::query_as(
            "SELECT to_regclass('vala.audit_staging') IS NULL \
                AND to_regclass('vala.audit_chain_head') IS NULL \
                AND to_regclass('vala.audit_publication') IS NULL \
                AND to_regprocedure('vala.audit_staging_immutable()') IS NULL",
        )
        .fetch_one(&pool)
        .await
        .expect("retired audit staging query succeeds");
        assert!(
            audit_staging.0,
            "the retired audit staging tables and function are absent after vala migrate"
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

    /// The creating audit-staging migration still carries the bytes it shipped
    /// with.
    ///
    /// SQLx records a checksum for every migration it applies and refuses to
    /// open the pools when a recorded checksum no longer matches the file, so
    /// editing an already-shipped migration takes every migrated deployment
    /// down before it can serve. A fresh-database lane cannot see that: it has
    /// no recorded checksum to disagree with. This pins the shipped digest so
    /// an edit fails here instead of on an operator's upgrade.
    #[test]
    fn shipped_audit_staging_migration_is_immutable() {
        let digest = sha2::Sha256::digest(
            include_bytes!("../../migrations/20260802000000_vala_audit_staging.sql").as_slice(),
        );
        assert_eq!(
            format!("{digest:x}"),
            "ce869581019efb6689d9413efa77245f2381f464127e707458f4026178ca5d84",
            "20260802000000_vala_audit_staging.sql changed; applied migrations are immutable, \
             add a forward migration instead"
        );
    }
}
