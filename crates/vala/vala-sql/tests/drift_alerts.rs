//! SQL integration tests for `vala.drift_alerts`.
//!
//! Covers upsert deduplication, acknowledge, RLS isolation, and the
//! `series = None` vs `series = Some(...)` separation.
//! Run via `mise run test:sql`.

mod drift_alerts {
    use vala_sql::TenantConn;
    use vala_sql::queries::drift_alerts::{
        DriftAlertInsert, acknowledge_drift_alert, resolve_drift_alerts, upsert_drift_alert,
    };
    use wyrd_semver::VersionBlock;
    use wyrd_spec::envelope::CardKind;
    use wyrd_spec::ids::{CardName, DataTenantId, SpaceName};
    use wyrd_spec::reference::CardRef;

    fn card_ref() -> CardRef {
        CardRef {
            kind: CardKind::Data,
            name: CardName::new("my-model").expect("valid card name"),
            version: VersionBlock::parse("1.0.0").expect("valid version"),
            space: SpaceName::new("default").expect("valid space"),
            uid: None,
        }
    }

    fn alert_payload() -> serde_json::Value {
        serde_json::json!({ "score": 0.9, "threshold": 0.8 })
    }

    async fn setup() -> (wyrd_sql::testing::SharedDb, DataTenantId, DataTenantId) {
        let db = vala_sql::testing::shared().await.expect("shared db");
        vala_sql::testing::reset_for_test(&db).await.expect("reset");
        let a = DataTenantId::new_v7();
        let b = DataTenantId::new_v7();
        vala_sql::testing::seed_tenant(&db.platform_admin, a.as_uuid())
            .await
            .unwrap();
        vala_sql::testing::seed_tenant(&db.platform_admin, b.as_uuid())
            .await
            .unwrap();
        (db, a, b)
    }

    #[tokio::test]
    async fn upsert_creates_active_row() {
        let (db, tenant_a, _) = setup().await;
        let drift_ref = card_ref();
        let ins = DriftAlertInsert {
            drift_ref: &drift_ref,
            drift_type: "spc",
            series: None,
            alert: &alert_payload(),
        };

        let mut conn = TenantConn::acquire(&db.app, tenant_a).await.unwrap();
        upsert_drift_alert(&mut conn, &ins).await.unwrap();
        conn.commit().await.unwrap();

        let rows: Vec<(bool,)> =
            sqlx::query_as("SELECT active FROM vala.drift_alerts WHERE data_tenant_id = $1")
                .bind(tenant_a.as_uuid())
                .fetch_all(&db.migrator)
                .await
                .unwrap();

        assert_eq!(rows.len(), 1, "exactly one row after upsert");
        assert!(rows[0].0, "row must be active");
    }

    #[tokio::test]
    async fn duplicate_upsert_updates_payload_without_duplicate_row() {
        let (db, tenant_a, _) = setup().await;
        let drift_ref = card_ref();
        let payload1 = serde_json::json!({ "score": 0.9 });
        let payload2 = serde_json::json!({ "score": 0.99 });

        let mut conn = TenantConn::acquire(&db.app, tenant_a).await.unwrap();
        upsert_drift_alert(
            &mut conn,
            &DriftAlertInsert {
                drift_ref: &drift_ref,
                drift_type: "spc",
                series: None,
                alert: &payload1,
            },
        )
        .await
        .unwrap();
        upsert_drift_alert(
            &mut conn,
            &DriftAlertInsert {
                drift_ref: &drift_ref,
                drift_type: "spc",
                series: None,
                alert: &payload2,
            },
        )
        .await
        .unwrap();
        conn.commit().await.unwrap();

        let rows: Vec<(serde_json::Value,)> =
            sqlx::query_as("SELECT alert FROM vala.drift_alerts WHERE data_tenant_id = $1")
                .bind(tenant_a.as_uuid())
                .fetch_all(&db.migrator)
                .await
                .unwrap();

        assert_eq!(rows.len(), 1, "must not create duplicate row on re-upsert");
        assert_eq!(
            rows[0].0["score"], 0.99,
            "payload must be updated to latest value"
        );
    }

    #[tokio::test]
    async fn acknowledge_deactivates_alert() {
        let (db, tenant_a, _) = setup().await;
        let drift_ref = card_ref();

        let mut conn = TenantConn::acquire(&db.app, tenant_a).await.unwrap();
        upsert_drift_alert(
            &mut conn,
            &DriftAlertInsert {
                drift_ref: &drift_ref,
                drift_type: "spc",
                series: None,
                alert: &alert_payload(),
            },
        )
        .await
        .unwrap();
        acknowledge_drift_alert(&mut conn, &drift_ref, "spc", None)
            .await
            .unwrap();
        conn.commit().await.unwrap();

        let rows: Vec<(bool,)> =
            sqlx::query_as("SELECT active FROM vala.drift_alerts WHERE data_tenant_id = $1")
                .bind(tenant_a.as_uuid())
                .fetch_all(&db.migrator)
                .await
                .unwrap();

        assert_eq!(rows.len(), 1);
        assert!(!rows[0].0, "acknowledged alert must be inactive");
    }

    #[tokio::test]
    async fn rls_isolation_tenant_b_cannot_see_tenant_a_alerts() {
        let (db, tenant_a, tenant_b) = setup().await;
        let drift_ref = card_ref();

        let mut conn_a = TenantConn::acquire(&db.app, tenant_a).await.unwrap();
        upsert_drift_alert(
            &mut conn_a,
            &DriftAlertInsert {
                drift_ref: &drift_ref,
                drift_type: "spc",
                series: None,
                alert: &alert_payload(),
            },
        )
        .await
        .unwrap();
        conn_a.commit().await.unwrap();

        let mut conn_b = TenantConn::acquire(&db.app, tenant_b).await.unwrap();
        let resolved = resolve_drift_alerts(&mut conn_b, &drift_ref).await.unwrap();
        conn_b.commit().await.unwrap();

        assert!(
            resolved.is_empty(),
            "tenant B must not see tenant A's alerts (RLS isolation)"
        );
    }

    #[tokio::test]
    async fn none_and_some_series_produce_separate_rows() {
        let (db, tenant_a, _) = setup().await;
        let drift_ref = card_ref();

        let mut conn = TenantConn::acquire(&db.app, tenant_a).await.unwrap();
        upsert_drift_alert(
            &mut conn,
            &DriftAlertInsert {
                drift_ref: &drift_ref,
                drift_type: "spc",
                series: None,
                alert: &alert_payload(),
            },
        )
        .await
        .unwrap();
        upsert_drift_alert(
            &mut conn,
            &DriftAlertInsert {
                drift_ref: &drift_ref,
                drift_type: "spc",
                series: Some("feature_1"),
                alert: &alert_payload(),
            },
        )
        .await
        .unwrap();
        conn.commit().await.unwrap();

        let count: (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM vala.drift_alerts WHERE data_tenant_id = $1")
                .bind(tenant_a.as_uuid())
                .fetch_one(&db.migrator)
                .await
                .unwrap();

        assert_eq!(
            count.0, 2,
            "series=None and series=Some(\"feature_1\") must be distinct rows"
        );
    }
}
