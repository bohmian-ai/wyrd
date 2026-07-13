mod pg_tests {
    //! SQL integration test for the cross-tenant audit relay enumeration (T1a).
    //!
    //! `list_audit_tenant_ids` runs a direct cross-tenant SELECT as
    //! wyrd_platform_admin (BYPASSRLS) via OperatorPool. This proves the role
    //! actually has the vala-schema + audit_outbox grants it needs — a path the
    //! SECURITY-DEFINER `claim_unshipped_audit` tests do not cover. Run via
    //! `mise run test:sql`.

    use sqlx::types::Uuid;
    use vala_sql::OperatorPool;
    use wyrd_dev_fixtures::pg::PgFixture;
    use wyrd_spec::DataTenantId;
    use wyrd_spec::auth::{PrincipalId, PrincipalKindTag};
    use wyrd_spec::request_id::RequestId;
    use wyrd_spec::vala::api::{AuditDecision, AuditEvent, AuditResult, AuthMethod};

    fn event() -> AuditEvent {
        AuditEvent {
            request_id: RequestId::parse(&Uuid::now_v7().to_string()).unwrap(),
            trace_id: None,
            operation: "bifrost.write".to_string(),
            resource: "ns.tbl".to_string(),
            card_ref: None,
            principal_id: PrincipalId::new(Uuid::now_v7()),
            principal_kind: PrincipalKindTag::User,
            auth_method: AuthMethod::Internal,
            permission: "bifrost.write".to_string(),
            decision: AuditDecision::Allow,
            result: AuditResult::Success,
            payload_summary: "redacted".to_string(),
        }
    }

    async fn seed_shipped_row(fixture: &PgFixture, superuser: &sqlx::PgPool, tenant: DataTenantId) {
        let mut conn = vala_sql::TenantConn::acquire(fixture.app_pool(), tenant)
            .await
            .unwrap();
        let seq = vala_sql::queries::audit_outbox::append_audit(&mut conn, &event())
            .await
            .unwrap();
        conn.commit().await.unwrap();

        // Mark the row shipped directly (superuser bypasses RLS) so we exercise
        // list_audit_tenant_ids' `WHERE shipped = true` filter without running
        // the full claim/ship cycle.
        sqlx::query(
            "UPDATE vala.audit_outbox SET shipped = true WHERE data_tenant_id = $1 AND seq = $2",
        )
        .bind(tenant.as_uuid())
        .bind(seq)
        .execute(superuser)
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn list_audit_tenant_ids_enumerates_shipped_across_tenants() {
        let fixture = PgFixture::start().await.expect("fixture");
        let superuser = fixture.superuser_pool().await.expect("superuser pool");

        let tenant_a = DataTenantId::new_v7();
        let tenant_b = DataTenantId::new_v7();
        for (t, label) in [(tenant_a, "a"), (tenant_b, "b")] {
            fixture
                .seed_additional_tenant_with_uuid(
                    t,
                    &format!("relay-{label}-{}", t.as_uuid().simple()),
                )
                .await
                .unwrap();
            seed_shipped_row(&fixture, &superuser, t).await;
        }

        let op = OperatorPool::from(fixture.platform_admin_pool().clone());
        let ids = vala_sql::queries::relay::list_audit_tenant_ids(&op)
            .await
            .expect("cross-tenant enumeration must succeed under platform_admin grants");

        assert!(
            ids.contains(&tenant_a.as_uuid()),
            "tenant_a with a shipped row must be enumerated: {ids:?}"
        );
        assert!(
            ids.contains(&tenant_b.as_uuid()),
            "tenant_b with a shipped row must be enumerated: {ids:?}"
        );
    }
}
