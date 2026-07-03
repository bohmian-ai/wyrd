//! SQL integration tests for the S3.C5 transactional audit outbox.
//!
//! Covers the per-tenant gapless hash chain, the append-only trigger, per-tenant
//! isolation of chains and shipping, and the cross-tenant relay claim/ship cycle.
//! Run against a live Postgres:
//!   DATABASE_URL=... cargo test -p vala-sql --all-features audit_outbox -- --test-threads=1

mod audit_outbox {
    use sqlx::PgPool;
    use sqlx::types::Uuid;
    use wyrd_spec::DataTenantId;
    use wyrd_spec::auth::{PrincipalId, PrincipalKind};
    use wyrd_spec::request_id::RequestId;
    use wyrd_spec::vala::api::{AuditDecision, AuditEvent, AuditResult, AuthMethod};

    const ZERO_HASH: [u8; 32] = [0u8; 32];

    async fn seed(pool: &PgPool) -> DataTenantId {
        vala_sql::testing::migrate_for_test(pool).await.unwrap();
        let tenant = DataTenantId::new_v7();
        vala_sql::testing::seed_tenant(pool, tenant.as_uuid())
            .await
            .unwrap();
        tenant
    }

    fn event(operation: &str) -> AuditEvent {
        AuditEvent {
            request_id: RequestId::parse(&Uuid::now_v7().to_string()).unwrap(),
            trace_id: None,
            operation: operation.to_string(),
            resource: "ns.tbl".to_string(),
            card_ref: None,
            principal_id: PrincipalId::new(Uuid::now_v7()),
            principal_kind: PrincipalKind::User,
            auth_method: AuthMethod::Internal,
            permission: "bifrost.write".to_string(),
            decision: AuditDecision::Allow,
            result: AuditResult::Success,
            payload_summary: "redacted".to_string(),
        }
    }

    async fn append(pool: &PgPool, tenant: DataTenantId, operation: &str) -> i64 {
        let mut conn = vala_sql::TenantConn::acquire(pool, tenant).await.unwrap();
        let seq = vala_sql::queries::audit_outbox::append_audit(&mut conn, &event(operation))
            .await
            .unwrap();
        conn.commit().await.unwrap();
        seq
    }

    #[sqlx::test(migrations = false)]
    async fn gapless_seq_and_hash_chain(pool: PgPool) {
        let tenant = seed(&pool).await;

        assert_eq!(append(&pool, tenant, "op.a").await, 1);
        assert_eq!(append(&pool, tenant, "op.b").await, 2);
        assert_eq!(append(&pool, tenant, "op.c").await, 3);

        let rows: Vec<(i64, Vec<u8>, Vec<u8>)> = sqlx::query_as(
            "SELECT seq, prev_hash, entry_hash FROM vala.audit_outbox
              WHERE data_tenant_id = $1 ORDER BY seq",
        )
        .bind(tenant.as_uuid())
        .fetch_all(&pool)
        .await
        .unwrap();

        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0].0, 1);
        assert_eq!(rows[0].1, ZERO_HASH, "row 1 prev_hash is 32 zero bytes");
        assert_eq!(
            rows[1].1, rows[0].2,
            "row 2 prev_hash links row 1 entry_hash"
        );
        assert_eq!(
            rows[2].1, rows[1].2,
            "row 3 prev_hash links row 2 entry_hash"
        );

        let (last_seq, head_hash): (i64, Vec<u8>) = sqlx::query_as(
            "SELECT last_seq, head_hash FROM vala.audit_chain_head WHERE data_tenant_id = $1",
        )
        .bind(tenant.as_uuid())
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(last_seq, 3);
        assert_eq!(
            head_hash, rows[2].2,
            "head_hash tracks the latest entry_hash"
        );
    }

    #[sqlx::test(migrations = false)]
    async fn append_only_trigger_rejects_delete_and_content_update(pool: PgPool) {
        let tenant = seed(&pool).await;
        append(&pool, tenant, "op.a").await;

        let deleted =
            sqlx::query("DELETE FROM vala.audit_outbox WHERE data_tenant_id = $1 AND seq = 1")
                .bind(tenant.as_uuid())
                .execute(&pool)
                .await;
        assert!(
            deleted.is_err(),
            "DELETE must be rejected by the append-only trigger"
        );

        let tampered = sqlx::query(
            "UPDATE vala.audit_outbox SET operation = 'tampered'
              WHERE data_tenant_id = $1 AND seq = 1",
        )
        .bind(tenant.as_uuid())
        .execute(&pool)
        .await;
        assert!(tampered.is_err(), "content UPDATE must be rejected");
    }

    #[sqlx::test(migrations = false)]
    async fn shipped_row_is_immutable(pool: PgPool) {
        let tenant = seed(&pool).await;
        append(&pool, tenant, "op.a").await;

        let batch = [0xABu8; 16];
        let mut conn = vala_sql::TenantConn::acquire(&pool, tenant).await.unwrap();
        let shipped = vala_sql::queries::audit_outbox::mark_audit_shipped(&mut conn, 1, 1, &batch)
            .await
            .unwrap();
        conn.commit().await.unwrap();
        assert_eq!(shipped, 1);

        let reship = sqlx::query(
            "UPDATE vala.audit_outbox SET shipped = true
              WHERE data_tenant_id = $1 AND seq = 1",
        )
        .bind(tenant.as_uuid())
        .execute(&pool)
        .await;
        assert!(reship.is_err(), "an already-shipped row must be immutable");
    }

    #[sqlx::test(migrations = false)]
    async fn per_tenant_chains_and_shipping_are_isolated(pool: PgPool) {
        vala_sql::testing::migrate_for_test(&pool).await.unwrap();
        let tenant_a = DataTenantId::new_v7();
        let tenant_b = DataTenantId::new_v7();
        vala_sql::testing::seed_tenant(&pool, tenant_a.as_uuid())
            .await
            .unwrap();
        vala_sql::testing::seed_tenant(&pool, tenant_b.as_uuid())
            .await
            .unwrap();

        // Each tenant's seq is independent and starts at 1.
        assert_eq!(append(&pool, tenant_a, "a.1").await, 1);
        assert_eq!(append(&pool, tenant_a, "a.2").await, 2);
        assert_eq!(append(&pool, tenant_b, "b.1").await, 1);

        // Marking tenant A shipped must not touch tenant B's rows.
        let batch_a = [0x0Au8; 16];
        let mut conn = vala_sql::TenantConn::acquire(&pool, tenant_a)
            .await
            .unwrap();
        let shipped_a =
            vala_sql::queries::audit_outbox::mark_audit_shipped(&mut conn, 1, 2, &batch_a)
                .await
                .unwrap();
        conn.commit().await.unwrap();
        assert_eq!(shipped_a, 2, "both tenant-A rows ship");

        let b_unshipped: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM vala.audit_outbox
              WHERE data_tenant_id = $1 AND NOT shipped",
        )
        .bind(tenant_b.as_uuid())
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(b_unshipped, 1, "tenant B is untouched by tenant A shipping");
    }

    #[sqlx::test(migrations = false)]
    async fn relay_claims_across_tenants_then_marks_shipped(pool: PgPool) {
        vala_sql::testing::migrate_for_test(&pool).await.unwrap();
        let tenant_a = DataTenantId::new_v7();
        let tenant_b = DataTenantId::new_v7();
        vala_sql::testing::seed_tenant(&pool, tenant_a.as_uuid())
            .await
            .unwrap();
        vala_sql::testing::seed_tenant(&pool, tenant_b.as_uuid())
            .await
            .unwrap();

        append(&pool, tenant_a, "a.1").await;
        append(&pool, tenant_a, "a.2").await;
        append(&pool, tenant_b, "b.1").await;

        // Cross-tenant claim (SECURITY DEFINER) sees every unshipped row.
        let mut conn = vala_sql::TenantConn::acquire(&pool, tenant_a)
            .await
            .unwrap();
        let claimed = vala_sql::queries::audit_outbox::claim_unshipped_audit(&mut conn, 100)
            .await
            .unwrap();
        conn.commit().await.unwrap();
        assert_eq!(claimed.len(), 3, "claim spans both tenants");

        // Ship per tenant under that tenant's bind.
        let mut conn = vala_sql::TenantConn::acquire(&pool, tenant_a)
            .await
            .unwrap();
        assert_eq!(
            vala_sql::queries::audit_outbox::mark_audit_shipped(&mut conn, 1, 2, &[0x0Au8; 16])
                .await
                .unwrap(),
            2
        );
        conn.commit().await.unwrap();

        let mut conn = vala_sql::TenantConn::acquire(&pool, tenant_b)
            .await
            .unwrap();
        assert_eq!(
            vala_sql::queries::audit_outbox::mark_audit_shipped(&mut conn, 1, 1, &[0x0Bu8; 16])
                .await
                .unwrap(),
            1
        );
        conn.commit().await.unwrap();

        // Nothing left to claim.
        let mut conn = vala_sql::TenantConn::acquire(&pool, tenant_a)
            .await
            .unwrap();
        let remaining = vala_sql::queries::audit_outbox::claim_unshipped_audit(&mut conn, 100)
            .await
            .unwrap();
        conn.commit().await.unwrap();
        assert!(remaining.is_empty(), "all rows shipped");
    }
}
