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
        use sha2::Digest;

        let digest = sha2::Sha256::digest(
            include_bytes!("../migrations/20260802000000_vala_audit_staging.sql").as_slice(),
        );
        assert_eq!(
            format!("{digest:x}"),
            "ce869581019efb6689d9413efa77245f2381f464127e707458f4026178ca5d84",
            "20260802000000_vala_audit_staging.sql changed; applied migrations are immutable, \
             add a forward migration instead"
        );
    }

    /// The forward credential-attribution migration upgrades a database that
    /// already staged rows under the original audit-staging shape.
    ///
    /// Credential attribution arrives as an added nullable column rather than
    /// an edit to the creating migration, so an upgrade must leave the rows
    /// staged before it untouched and readable, with no credential of their
    /// own. This rebuilds the original shape, stages a row in it, then applies
    /// the exact forward migration over it.
    #[tokio::test]
    async fn staged_rows_survive_the_credential_attribution_upgrade() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let pool = fixture.superuser_pool().await.expect("superuser pool");
        sqlx::raw_sql(
            "DROP TABLE vala.audit_staging CASCADE; \
             DROP TABLE vala.audit_chain_head CASCADE; \
             DROP FUNCTION vala.audit_staging_immutable() CASCADE;",
        )
        .execute(&pool)
        .await
        .expect("current audit staging shape drops");
        sqlx::raw_sql(include_str!(
            "../migrations/20260802000000_vala_audit_staging.sql"
        ))
        .execute(&pool)
        .await
        .expect("original audit staging shape applies");

        let tenant = uuid::Uuid::from(wyrd_spec::DataTenantId::SYSTEM_OWNER);
        sqlx::query(
            "INSERT INTO vala.audit_staging \
             (data_tenant_id,seq,prev_hash,entry_hash,request_id,operation,resource, \
              principal_id,principal_kind,permission,outcome) \
             VALUES ($1,1,$2,$3,'req-1','card.register','card:demo',$4,'user','write','allowed')",
        )
        .bind(tenant)
        .bind([0u8; 32].as_slice())
        .bind([1u8; 32].as_slice())
        .bind(uuid::Uuid::now_v7())
        .execute(&pool)
        .await
        .expect("row stages under the original shape");

        sqlx::raw_sql(include_str!(
            "../migrations/20260910000027_audit_staging_credential_id.sql"
        ))
        .execute(&pool)
        .await
        .expect("forward credential attribution applies over staged rows");

        let credential: Option<uuid::Uuid> = sqlx::query_scalar(
            "SELECT credential_id FROM vala.audit_staging WHERE data_tenant_id=$1 AND seq=1",
        )
        .bind(tenant)
        .fetch_one(&pool)
        .await
        .expect("pre-upgrade row still reads");
        assert!(
            credential.is_none(),
            "a decision staged before credential attribution has no credential"
        );

        let nullable: Option<String> = sqlx::query_scalar(
            "SELECT is_nullable FROM information_schema.columns \
             WHERE table_schema='vala' AND table_name='audit_staging' \
               AND column_name='credential_id'",
        )
        .fetch_optional(&pool)
        .await
        .expect("column metadata reads");
        assert_eq!(nullable.as_deref(), Some("YES"));
    }
}
