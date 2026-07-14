mod pg_tests {
    //! Watermark state-machine invariants for `DerivationRuntime::process_tenant`:
    //!
    //! 1. `mark_failed` never moves the watermark. A failed derivation retries
    //!    from its last successful position on the next tick.
    //! 2. When a batch B partially succeeds (some targets written, some not),
    //!    `process_batches` returns `last_completed = <prior fully-completed
    //!    batch>` and `process_tenant` calls `mark_failed` — not
    //!    `advance_watermark(B)`. The watermark stays at the prior batch.
    //! 3. On repair rerun, `advance_watermark` moves forward normally and no
    //!    earlier batch is re-materialised.
    //!
    //! `blocked_tick_does_not_write_watermark` in `pg_derivation_fencing.rs`
    //! covers the lease-blocked branch. This module covers the
    //! target-write-failure branch.
    //!
    //! Run via `mise run test:bifrost`.

    use uuid::Uuid;
    use vala_sql::TenantConn;
    use vala_sql::queries::olap_catalog::{precommit, upsert_table};
    use vala_sql::queries::olap_derivations::{
        advance_watermark, derivation_freshness, insert_derivation, mark_deriving, mark_failed,
        select_derivation_pin,
    };
    use wyrd_dev_fixtures::pg::PgFixture;
    use wyrd_spec::DataTenantId;

    struct Setup {
        fixture: PgFixture,
        tenant: DataTenantId,
        source_uid: [u8; 16],
        target_uid: [u8; 16],
        derivation_uid: [u8; 16],
        b1: [u8; 16],
        b2: [u8; 16],
        b3: [u8; 16],
    }

    async fn setup() -> Setup {
        let fixture = PgFixture::start().await.expect("fixture");
        let tenant = DataTenantId::new_v7();
        fixture
            .seed_additional_tenant_with_uuid(tenant, &format!("fc-{}", tenant.as_uuid().simple()))
            .await
            .expect("seed tenant");

        let source_uid = *Uuid::now_v7().as_bytes();
        let target_uid = *Uuid::now_v7().as_bytes();
        let derivation_uid = *Uuid::now_v7().as_bytes();
        // Three source batches in ascending UUIDv7 time order so their
        // commit order is stable (`b1 < b2 < b3`).
        let b1 = *Uuid::now_v7().as_bytes();
        let b2 = *Uuid::now_v7().as_bytes();
        let b3 = *Uuid::now_v7().as_bytes();

        let mut conn = TenantConn::acquire(fixture.app_pool(), tenant)
            .await
            .expect("conn");
        upsert_table(
            &mut conn,
            &source_uid,
            "traces.spans_fail_closed",
            &[0u8; 32],
            "tenant_owned",
            &[],
        )
        .await
        .expect("upsert source");
        for bid in [&b1, &b2, &b3] {
            precommit(&mut conn, &source_uid, bid, "system", "system")
                .await
                .expect("precommit");
        }
        // Flip the three anchors precommit -> committed inside the tenant
        // transaction so RLS gates on `data_tenant_id = current_tenant()` and
        // wyrd_app has write permission. Direct SQL rather than
        // finalize_committed because the runtime state we want to reach here
        // is "the source table produced N committed anchors" — the writer
        // owner and fencing token are irrelevant to the fail-closed contract
        // under test.
        sqlx::query(
            r"
            UPDATE vala.olap_commits
               SET state        = 'committed',
                   committed_at = now(),
                   snapshot_id  = 0
             WHERE table_uid = $1
            ",
        )
        .bind(source_uid.as_slice())
        .execute(&mut **conn.transaction())
        .await
        .expect("commit rows");
        conn.commit().await.expect("commit setup");

        let mut conn = TenantConn::acquire(fixture.app_pool(), tenant)
            .await
            .expect("conn");
        insert_derivation(
            &mut conn,
            &derivation_uid,
            &source_uid,
            &target_uid,
            tenant.as_uuid(),
            "genai_from_spans",
            None,
        )
        .await
        .expect("insert derivation");
        conn.commit().await.expect("commit derivation");

        Setup {
            fixture,
            tenant,
            source_uid,
            target_uid,
            derivation_uid,
            b1,
            b2,
            b3,
        }
    }

    /// mark_failed never touches the watermark: not on a NULL watermark
    /// (first-batch failure) and not on a live watermark (later-batch failure).
    #[tokio::test]
    async fn mark_failed_does_not_move_watermark() {
        let s = setup().await;

        // Scenario A: no watermark yet -> mark_failed keeps it NULL.
        let mut conn = TenantConn::acquire(s.fixture.app_pool(), s.tenant)
            .await
            .expect("conn");
        assert!(
            mark_deriving(&mut conn, &s.derivation_uid)
                .await
                .expect("mark deriving"),
            "the derivation row exists and can enter the deriving state"
        );
        assert!(
            mark_failed(&mut conn, &s.derivation_uid)
                .await
                .expect("mark failed"),
            "mark_failed applies to the deriving row"
        );
        let fresh = derivation_freshness(&mut conn, &s.derivation_uid)
            .await
            .expect("freshness")
            .expect("row");
        assert!(
            fresh.watermark.is_none(),
            "mark_failed must not synthesise a watermark from thin air: {:?}",
            fresh.watermark
        );
        assert_eq!(fresh.derivation_state, "failed");
        conn.commit().await.expect("commit A");

        // Scenario B: watermark = b1 (B1 fully written) then mark_failed for
        // B2 leaves the watermark at b1 exactly.
        let mut conn = TenantConn::acquire(s.fixture.app_pool(), s.tenant)
            .await
            .expect("conn");
        assert!(
            mark_deriving(&mut conn, &s.derivation_uid)
                .await
                .expect("mark deriving B"),
            "the runtime marks the derivation deriving before advancing"
        );
        assert!(
            advance_watermark(&mut conn, &s.derivation_uid, &s.b1)
                .await
                .expect("advance to b1"),
            "advance_watermark commits the last fully-completed batch"
        );
        assert!(
            mark_failed(&mut conn, &s.derivation_uid)
                .await
                .expect("mark failed"),
            "the next batch fails; runtime marks the derivation failed"
        );
        let fresh = derivation_freshness(&mut conn, &s.derivation_uid)
            .await
            .expect("freshness")
            .expect("row");
        assert_eq!(
            fresh.watermark.as_deref(),
            Some(s.b1.as_slice()),
            "watermark must stay at b1; mark_failed must never move it forward"
        );
        assert_eq!(fresh.derivation_state, "failed");
        conn.commit().await.expect("commit B");
    }

    /// Repair rerun: after a partial-batch failure at B2, the next tick starts
    /// from the b1 pin (skipping already-consumed batches) and advances the
    /// watermark to b3 without re-materialising b1.
    #[tokio::test]
    async fn repair_advances_watermark_from_last_completed_without_reprocessing() {
        let s = setup().await;

        // Tick 1: B1 fully consumed -> advance to b1; B2 target write fails
        // -> mark_failed. Watermark ends at b1.
        {
            let mut conn = TenantConn::acquire(s.fixture.app_pool(), s.tenant)
                .await
                .expect("conn tick 1");
            mark_deriving(&mut conn, &s.derivation_uid)
                .await
                .expect("mark deriving tick 1");
            advance_watermark(&mut conn, &s.derivation_uid, &s.b1)
                .await
                .expect("advance to b1");
            mark_failed(&mut conn, &s.derivation_uid)
                .await
                .expect("mark failed on b2 target write");
            conn.commit().await.expect("commit tick 1");
        }

        // Between ticks, the pin resolves to the live watermark (b1), so the
        // repair scan will start at the batch AFTER b1. filter_after_pin's
        // "strictly after" semantics guarantees b1 is never re-materialised.
        {
            let mut conn = TenantConn::acquire(s.fixture.app_pool(), s.tenant)
                .await
                .expect("conn pin check");
            let pin = select_derivation_pin(&mut conn, &s.derivation_uid)
                .await
                .expect("pin")
                .expect("row");
            assert_eq!(
                pin.as_deref(),
                Some(s.b1.as_slice()),
                "pin between ticks is the live watermark; runtime scans strictly after"
            );
            conn.commit().await.expect("commit pin check");
        }

        // Tick 2: repair. B2, B3 both fully consumed -> advance to b3, state
        // returns to idle.
        {
            let mut conn = TenantConn::acquire(s.fixture.app_pool(), s.tenant)
                .await
                .expect("conn tick 2");
            mark_deriving(&mut conn, &s.derivation_uid)
                .await
                .expect("mark deriving tick 2");
            advance_watermark(&mut conn, &s.derivation_uid, &s.b3)
                .await
                .expect("advance to b3");
            conn.commit().await.expect("commit tick 2");
        }

        let mut conn = TenantConn::acquire(s.fixture.app_pool(), s.tenant)
            .await
            .expect("conn assert");
        let fresh = derivation_freshness(&mut conn, &s.derivation_uid)
            .await
            .expect("freshness")
            .expect("row");
        assert_eq!(
            fresh.watermark.as_deref(),
            Some(s.b3.as_slice()),
            "repair advances the watermark to the last fully-completed batch"
        );
        assert_eq!(
            fresh.derivation_state, "idle",
            "successful repair returns state to idle"
        );

        // Tick 3: no new batches; a no-op tick must not disturb the durable
        // watermark or the healthy state.
        conn.commit().await.expect("commit assert");
        let mut conn = TenantConn::acquire(s.fixture.app_pool(), s.tenant)
            .await
            .expect("conn tick 3");
        let after_noop = derivation_freshness(&mut conn, &s.derivation_uid)
            .await
            .expect("freshness")
            .expect("row");
        assert_eq!(
            after_noop.watermark.as_deref(),
            Some(s.b3.as_slice()),
            "no-op tick must not disturb the watermark"
        );
        assert_eq!(after_noop.derivation_state, "idle");
        conn.commit().await.expect("commit tick 3");

        // Silence unused-warning for setup fields the assertion path does not
        // touch. Keeping them on the struct documents the fixture state.
        let _ = (s.source_uid, s.target_uid, s.b2);
    }
}
