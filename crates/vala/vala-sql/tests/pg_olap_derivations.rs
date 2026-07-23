mod pg_tests {
    //! SQL integration tests for the cross-table derivation registry
    //! (`vala.olap_derivations`, Task E).
    //!
    //! Covers the durable-watermark roundtrip (insert → mark_deriving →
    //! advance_watermark → freshness) and the NULL-watermark pin (a derivation
    //! that has consumed nothing pins to its registered/earliest position).
    //! Run via `mise run test:sql`.

    use vala_sql::TenantConn;
    use vala_sql::queries::olap_catalog::upsert_table;
    use vala_sql::queries::olap_derivations::{
        advance_watermark, derivation_freshness, insert_derivation, mark_deriving,
        select_derivation_candidates, select_derivation_pin,
    };
    use wyrd_dev_fixtures::pg::PgFixture;
    use wyrd_spec::DataTenantId;

    async fn setup() -> (PgFixture, DataTenantId) {
        let fixture = PgFixture::start().await.expect("fixture");
        let tenant = DataTenantId::new_v7();
        fixture
            .seed_additional_tenant_with_uuid(
                tenant,
                &format!("test-{}", tenant.as_uuid().simple()),
            )
            .await
            .expect("seed tenant");
        (fixture, tenant)
    }

    /// Register a source bifrost table so the derivation FK resolves.
    async fn register_source(conn: &mut TenantConn<'_>, source_table_uid: &[u8; 16], fqn: &str) {
        upsert_table(conn, source_table_uid, fqn, &[0u8; 32], &[])
            .await
            .expect("register source table");
    }

    #[tokio::test]
    async fn olap_derivations_watermark_roundtrip() {
        let (fixture, tenant) = setup().await;

        let derivation_uid = *uuid::Uuid::now_v7().as_bytes();
        let source_uid = *uuid::Uuid::now_v7().as_bytes();
        let target_uid = *uuid::Uuid::now_v7().as_bytes();
        let watermark = *uuid::Uuid::now_v7().as_bytes();

        let mut conn = TenantConn::acquire(fixture.app_pool(), tenant)
            .await
            .expect("acquire");
        register_source(&mut conn, &source_uid, "spans.otlp_source").await;
        insert_derivation(
            &mut conn,
            &derivation_uid,
            &source_uid,
            &target_uid,
            "genai.from_spans",
            None,
        )
        .await
        .expect("insert derivation");

        // Idempotent re-register on the same (source, target) identity.
        insert_derivation(
            &mut conn,
            &derivation_uid,
            &source_uid,
            &target_uid,
            "genai.from_spans",
            None,
        )
        .await
        .expect("re-insert is a no-op");

        assert!(
            mark_deriving(&mut conn, &derivation_uid)
                .await
                .expect("mark deriving"),
            "mark_deriving flips a registered derivation into progress"
        );

        // Freshness before any advance: no watermark, never derived.
        let before = derivation_freshness(&mut conn, &derivation_uid)
            .await
            .expect("freshness")
            .expect("derivation exists");
        assert!(before.watermark.is_none(), "no watermark before advance");
        assert_eq!(before.derivation_state, "deriving");
        assert!(before.derived_at.is_none(), "never derived yet");
        assert!(before.lag_seconds.is_none(), "no lag before first advance");

        assert!(
            advance_watermark(&mut conn, &derivation_uid, &watermark)
                .await
                .expect("advance"),
            "advance_watermark moves a registered derivation forward"
        );

        // Freshness after advance: watermark set, state back to idle, lag present.
        let after = derivation_freshness(&mut conn, &derivation_uid)
            .await
            .expect("freshness")
            .expect("derivation exists");
        assert_eq!(
            after.watermark.as_deref(),
            Some(watermark.as_slice()),
            "freshness reflects the advanced watermark"
        );
        assert_eq!(after.derivation_state, "idle");
        assert!(after.derived_at.is_some(), "derived_at stamped on advance");
        assert!(
            after.lag_seconds.is_some(),
            "lag is present once the derivation has advanced"
        );

        // The advanced derivation is still a runnable candidate for its source.
        let candidates = select_derivation_candidates(&mut conn, &source_uid)
            .await
            .expect("candidates");
        assert_eq!(candidates.len(), 1, "one derivation for this source");
        assert_eq!(
            candidates[0].watermark.as_deref(),
            Some(watermark.as_slice()),
            "candidate row carries the advanced watermark"
        );

        // The pin follows the live watermark once advanced.
        let pin = select_derivation_pin(&mut conn, &derivation_uid)
            .await
            .expect("pin")
            .expect("derivation exists");
        assert_eq!(
            pin.as_deref(),
            Some(watermark.as_slice()),
            "pin resolves to the live watermark after advance"
        );

        conn.commit().await.expect("commit");
    }

    #[tokio::test]
    async fn derivation_pin_null_watermark_pins_registered() {
        let (fixture, tenant) = setup().await;

        let derivation_uid = *uuid::Uuid::now_v7().as_bytes();
        let source_uid = *uuid::Uuid::now_v7().as_bytes();
        let target_uid = *uuid::Uuid::now_v7().as_bytes();
        let registered = *uuid::Uuid::now_v7().as_bytes();

        let mut conn = TenantConn::acquire(fixture.app_pool(), tenant)
            .await
            .expect("acquire");
        register_source(&mut conn, &source_uid, "spans.otlp_source").await;
        insert_derivation(
            &mut conn,
            &derivation_uid,
            &source_uid,
            &target_uid,
            "genai.from_spans",
            Some(&registered),
        )
        .await
        .expect("insert derivation with registered watermark");

        // Watermark is NULL (nothing consumed) — the pin must fall back to the
        // registered/earliest position, never NULL/empty.
        let pin = select_derivation_pin(&mut conn, &derivation_uid)
            .await
            .expect("pin")
            .expect("derivation exists");
        assert_eq!(
            pin.as_deref(),
            Some(registered.as_slice()),
            "a NULL watermark pins to the registered/earliest position"
        );

        // Sanity: freshness confirms the watermark itself is still NULL.
        let fresh = derivation_freshness(&mut conn, &derivation_uid)
            .await
            .expect("freshness")
            .expect("derivation exists");
        assert!(
            fresh.watermark.is_none(),
            "the live watermark is still NULL; only the pin fell back"
        );

        conn.commit().await.expect("commit");
    }
}
