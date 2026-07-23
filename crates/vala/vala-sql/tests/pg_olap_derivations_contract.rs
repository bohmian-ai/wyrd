mod pg_tests {
    //! SQL integration tests for the immutable derivation contract fingerprints
    //! (`vala.olap_derivations` columns: source_schema_fingerprint,
    //! transform_fingerprint, target_set_fingerprint).
    //!
    //! Exercises the register_derivation_with_contract query: Registered on first
    //! call, IdenticalReplay when all fingerprints match, DriftRejected (with the
    //! correct differing-column list) when any fingerprint differs. Also asserts
    //! that an IdenticalReplay does NOT advance the stored watermark.
    //!
    //! Run via `mise run test:sql`.

    use vala_sql::TenantConn;
    use vala_sql::queries::olap_catalog::upsert_table;
    use vala_sql::queries::olap_derivations::{
        ContractOutcome, DerivationContract, DerivationRegistration, advance_watermark,
        register_derivation_with_contract, select_derivation_pin,
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

    async fn register_source(conn: &mut TenantConn<'_>, source_table_uid: &[u8; 16]) {
        upsert_table(conn, source_table_uid, "spans.otlp_source", &[0u8; 32], &[])
            .await
            .expect("register source table");
    }

    fn fp(byte: u8) -> [u8; 32] {
        [byte; 32]
    }

    #[tokio::test]
    async fn register_new_derivation_returns_registered() {
        let (fixture, tenant) = setup().await;
        let derivation_uid = *uuid::Uuid::now_v7().as_bytes();
        let source_uid = *uuid::Uuid::now_v7().as_bytes();
        let target_uid = *uuid::Uuid::now_v7().as_bytes();
        let mut conn = TenantConn::acquire(fixture.app_pool(), tenant)
            .await
            .expect("acquire");
        register_source(&mut conn, &source_uid).await;

        let outcome = register_derivation_with_contract(
            &mut conn,
            DerivationRegistration {
                derivation_uid: &derivation_uid,
                source_table_uid: &source_uid,
                target_table_uid: &target_uid,
                fqn: "genai_from_spans",
                registered_watermark: None,
                contract: DerivationContract {
                    source_schema_fingerprint: &fp(0x01),
                    transform_fingerprint: &fp(0x02),
                    target_set_fingerprint: &fp(0x03),
                },
            },
        )
        .await
        .expect("register");

        assert_eq!(
            outcome,
            ContractOutcome::Registered,
            "brand-new derivation must return Registered"
        );

        conn.commit().await.expect("commit");
    }

    #[tokio::test]
    async fn identical_replay_returns_identical_replay_no_watermark_change() {
        let (fixture, tenant) = setup().await;
        let derivation_uid = *uuid::Uuid::now_v7().as_bytes();
        let source_uid = *uuid::Uuid::now_v7().as_bytes();
        let target_uid = *uuid::Uuid::now_v7().as_bytes();
        let watermark = *uuid::Uuid::now_v7().as_bytes();

        let mut conn = TenantConn::acquire(fixture.app_pool(), tenant)
            .await
            .expect("acquire");
        register_source(&mut conn, &source_uid).await;

        // First registration.
        register_derivation_with_contract(
            &mut conn,
            DerivationRegistration {
                derivation_uid: &derivation_uid,
                source_table_uid: &source_uid,
                target_table_uid: &target_uid,
                fqn: "genai_from_spans",
                registered_watermark: None,
                contract: DerivationContract {
                    source_schema_fingerprint: &fp(0x01),
                    transform_fingerprint: &fp(0x02),
                    target_set_fingerprint: &fp(0x03),
                },
            },
        )
        .await
        .expect("first register");

        // Advance the watermark so we can verify it is preserved.
        advance_watermark(&mut conn, &derivation_uid, &watermark)
            .await
            .expect("advance watermark");

        // Re-register with the SAME fingerprints.
        let outcome = register_derivation_with_contract(
            &mut conn,
            DerivationRegistration {
                derivation_uid: &derivation_uid,
                source_table_uid: &source_uid,
                target_table_uid: &target_uid,
                fqn: "genai_from_spans",
                registered_watermark: None,
                contract: DerivationContract {
                    source_schema_fingerprint: &fp(0x01),
                    transform_fingerprint: &fp(0x02),
                    target_set_fingerprint: &fp(0x03),
                },
            },
        )
        .await
        .expect("re-register");

        assert_eq!(
            outcome,
            ContractOutcome::IdenticalReplay,
            "identical fingerprints must return IdenticalReplay"
        );

        // Watermark must still be the value we advanced to — not reset.
        let pin = select_derivation_pin(&mut conn, &derivation_uid)
            .await
            .expect("pin")
            .expect("derivation exists");
        assert_eq!(
            pin.as_deref(),
            Some(watermark.as_slice()),
            "IdenticalReplay must not alter the stored watermark"
        );

        conn.commit().await.expect("commit");
    }

    #[tokio::test]
    async fn differing_transform_fingerprint_returns_drift_rejected() {
        let (fixture, tenant) = setup().await;
        let derivation_uid = *uuid::Uuid::now_v7().as_bytes();
        let source_uid = *uuid::Uuid::now_v7().as_bytes();
        let target_uid = *uuid::Uuid::now_v7().as_bytes();
        let mut conn = TenantConn::acquire(fixture.app_pool(), tenant)
            .await
            .expect("acquire");
        register_source(&mut conn, &source_uid).await;

        register_derivation_with_contract(
            &mut conn,
            DerivationRegistration {
                derivation_uid: &derivation_uid,
                source_table_uid: &source_uid,
                target_table_uid: &target_uid,
                fqn: "genai_from_spans",
                registered_watermark: None,
                contract: DerivationContract {
                    source_schema_fingerprint: &fp(0x01),
                    transform_fingerprint: &fp(0x02), // original
                    target_set_fingerprint: &fp(0x03),
                },
            },
        )
        .await
        .expect("register");

        // Re-register with a different transform_fingerprint.
        let outcome = register_derivation_with_contract(
            &mut conn,
            DerivationRegistration {
                derivation_uid: &derivation_uid,
                source_table_uid: &source_uid,
                target_table_uid: &target_uid,
                fqn: "genai_from_spans",
                registered_watermark: None,
                contract: DerivationContract {
                    source_schema_fingerprint: &fp(0x01),
                    transform_fingerprint: &fp(0xff), // CHANGED
                    target_set_fingerprint: &fp(0x03),
                },
            },
        )
        .await
        .expect("re-register");

        match outcome {
            ContractOutcome::DriftRejected { differing } => {
                assert_eq!(
                    differing,
                    vec!["transform_fingerprint"],
                    "only transform_fingerprint must be reported as differing"
                );
            }
            other => panic!("expected DriftRejected, got {other:?}"),
        }

        conn.commit().await.expect("commit");
    }

    #[tokio::test]
    async fn differing_source_schema_fingerprint_returns_drift_rejected() {
        let (fixture, tenant) = setup().await;
        let derivation_uid = *uuid::Uuid::now_v7().as_bytes();
        let source_uid = *uuid::Uuid::now_v7().as_bytes();
        let target_uid = *uuid::Uuid::now_v7().as_bytes();
        let mut conn = TenantConn::acquire(fixture.app_pool(), tenant)
            .await
            .expect("acquire");
        register_source(&mut conn, &source_uid).await;

        register_derivation_with_contract(
            &mut conn,
            DerivationRegistration {
                derivation_uid: &derivation_uid,
                source_table_uid: &source_uid,
                target_table_uid: &target_uid,
                fqn: "genai_from_spans",
                registered_watermark: None,
                contract: DerivationContract {
                    source_schema_fingerprint: &fp(0x01), // original
                    transform_fingerprint: &fp(0x02),
                    target_set_fingerprint: &fp(0x03),
                },
            },
        )
        .await
        .expect("register");

        let outcome = register_derivation_with_contract(
            &mut conn,
            DerivationRegistration {
                derivation_uid: &derivation_uid,
                source_table_uid: &source_uid,
                target_table_uid: &target_uid,
                fqn: "genai_from_spans",
                registered_watermark: None,
                contract: DerivationContract {
                    source_schema_fingerprint: &fp(0xff), // CHANGED
                    transform_fingerprint: &fp(0x02),
                    target_set_fingerprint: &fp(0x03),
                },
            },
        )
        .await
        .expect("re-register");

        match outcome {
            ContractOutcome::DriftRejected { differing } => {
                assert_eq!(
                    differing,
                    vec!["source_schema_fingerprint"],
                    "only source_schema_fingerprint must be reported as differing"
                );
            }
            other => panic!("expected DriftRejected, got {other:?}"),
        }

        conn.commit().await.expect("commit");
    }

    #[tokio::test]
    async fn differing_target_set_fingerprint_returns_drift_rejected() {
        let (fixture, tenant) = setup().await;
        let derivation_uid = *uuid::Uuid::now_v7().as_bytes();
        let source_uid = *uuid::Uuid::now_v7().as_bytes();
        let target_uid = *uuid::Uuid::now_v7().as_bytes();
        let mut conn = TenantConn::acquire(fixture.app_pool(), tenant)
            .await
            .expect("acquire");
        register_source(&mut conn, &source_uid).await;

        register_derivation_with_contract(
            &mut conn,
            DerivationRegistration {
                derivation_uid: &derivation_uid,
                source_table_uid: &source_uid,
                target_table_uid: &target_uid,
                fqn: "genai_from_spans",
                registered_watermark: None,
                contract: DerivationContract {
                    source_schema_fingerprint: &fp(0x01),
                    transform_fingerprint: &fp(0x02),
                    target_set_fingerprint: &fp(0x03), // original
                },
            },
        )
        .await
        .expect("register");

        let outcome = register_derivation_with_contract(
            &mut conn,
            DerivationRegistration {
                derivation_uid: &derivation_uid,
                source_table_uid: &source_uid,
                target_table_uid: &target_uid,
                fqn: "genai_from_spans",
                registered_watermark: None,
                contract: DerivationContract {
                    source_schema_fingerprint: &fp(0x01),
                    transform_fingerprint: &fp(0x02),
                    target_set_fingerprint: &fp(0xff), // CHANGED
                },
            },
        )
        .await
        .expect("re-register");

        match outcome {
            ContractOutcome::DriftRejected { differing } => {
                assert_eq!(
                    differing,
                    vec!["target_set_fingerprint"],
                    "only target_set_fingerprint must be reported as differing"
                );
            }
            other => panic!("expected DriftRejected, got {other:?}"),
        }

        conn.commit().await.expect("commit");
    }

    #[tokio::test]
    async fn multiple_differing_fingerprints_reports_all() {
        let (fixture, tenant) = setup().await;
        let derivation_uid = *uuid::Uuid::now_v7().as_bytes();
        let source_uid = *uuid::Uuid::now_v7().as_bytes();
        let target_uid = *uuid::Uuid::now_v7().as_bytes();
        let mut conn = TenantConn::acquire(fixture.app_pool(), tenant)
            .await
            .expect("acquire");
        register_source(&mut conn, &source_uid).await;

        register_derivation_with_contract(
            &mut conn,
            DerivationRegistration {
                derivation_uid: &derivation_uid,
                source_table_uid: &source_uid,
                target_table_uid: &target_uid,
                fqn: "genai_from_spans",
                registered_watermark: None,
                contract: DerivationContract {
                    source_schema_fingerprint: &fp(0x01),
                    transform_fingerprint: &fp(0x02),
                    target_set_fingerprint: &fp(0x03),
                },
            },
        )
        .await
        .expect("register");

        // Change source_schema and transform fingerprints; keep target the same.
        let outcome = register_derivation_with_contract(
            &mut conn,
            DerivationRegistration {
                derivation_uid: &derivation_uid,
                source_table_uid: &source_uid,
                target_table_uid: &target_uid,
                fqn: "genai_from_spans",
                registered_watermark: None,
                contract: DerivationContract {
                    source_schema_fingerprint: &fp(0xaa), // CHANGED
                    transform_fingerprint: &fp(0xbb),     // CHANGED
                    target_set_fingerprint: &fp(0x03),    // unchanged
                },
            },
        )
        .await
        .expect("re-register");

        match outcome {
            ContractOutcome::DriftRejected { differing } => {
                assert!(
                    differing.contains(&"source_schema_fingerprint"),
                    "source_schema_fingerprint must be in differing list"
                );
                assert!(
                    differing.contains(&"transform_fingerprint"),
                    "transform_fingerprint must be in differing list"
                );
                assert!(
                    !differing.contains(&"target_set_fingerprint"),
                    "target_set_fingerprint must NOT be in differing list"
                );
                assert_eq!(differing.len(), 2, "exactly two columns differ");
            }
            other => panic!("expected DriftRejected, got {other:?}"),
        }

        conn.commit().await.expect("commit");
    }
}
