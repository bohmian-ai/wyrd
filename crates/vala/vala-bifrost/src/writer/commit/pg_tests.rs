//! Crash-recovery tests: proves the 2PC pre-commit-row protocol survives
//! writer crashes and correctly reconciles on engine restart.
//!
//! Inlined into the crate's `cfg(test)` tree so it reaches the test-only
//! fault-injection API (`run_commit_with_fault`, `FaultPoint`) without a feature
//! flag. The `pg_tests` name keeps it in the Postgres lane: the fast family lane
//! skips it via `--skip pg_tests`; `mise run test:bifrost` runs it.

// Inline `#[cfg(test)] mod` block: the tests are compile-time test-only and
// the unwrap-audit recognizes this form, so their test `.unwrap()`s are not
// scanned as production code. The `pg_tests::recovery` path still contains
// `pg_tests`, so the fast family lane keeps skipping it via `--skip pg_tests`.
#[cfg(test)]
mod recovery {
    use std::sync::Arc;

    use crate::batch_builder::stamp_system_columns;
    use crate::catalog::WyrdCatalog;
    use crate::catalog::iceberg_sql;
    use crate::catalog::namespaces::BifrostNamespace;
    use crate::types::{TableScope, TableUid};
    use crate::writer::commit::{FaultPoint, Lease, run_commit_with_fault};
    use arrow::array::{Int64Array, StringArray};
    use arrow::datatypes::{DataType, Field, Schema};
    use arrow::record_batch::RecordBatch;
    use iceberg::Catalog as _;
    use sqlx::PgPool;
    use tempfile::TempDir;
    use wyrd_dev_fixtures::pg::PgFixture;
    use wyrd_spec::ids::DataTenantId;
    use wyrd_storage::settings::BackendConfig;

    const NS: BifrostNamespace = BifrostNamespace::Bifrost;
    const TABLE: &str = "crash_test";

    struct Harness {
        _tmp: TempDir,
        catalog_uri: String,
        backend: BackendConfig,
        pool: Arc<PgPool>,
        recovery_pool: Arc<PgPool>,
        migrator: Arc<PgPool>,
        tenant: DataTenantId,
        table_uid: TableUid,
        _fixture: PgFixture,
    }

    fn test_warehouse(catalog_uri: String) -> (String, BackendConfig, TempDir) {
        let tmp = tempfile::tempdir().unwrap();
        let backend = BackendConfig::Local {
            root: tmp.path().to_path_buf(),
        };
        (catalog_uri, backend, tmp)
    }

    async fn setup() -> Harness {
        let fixture = PgFixture::start().await.expect("fixture");
        let pool = Arc::new(fixture.app_pool().clone());
        let recovery_pool = Arc::new(fixture.recovery_pool().await.expect("recovery pool"));

        let tenant = DataTenantId::new_v7();
        fixture
            .seed_additional_tenant_with_uuid(
                tenant,
                &format!("test-{}", tenant.as_uuid().simple()),
            )
            .await
            .unwrap();

        let (catalog_uri, backend, tmp) = test_warehouse(fixture.catalog_uri());

        let catalog = WyrdCatalog::new(
            &catalog_uri,
            &backend,
            pool.clone(),
            Some(recovery_pool.clone()),
        )
        .await
        .unwrap();

        let fields = vec![Field::new("val", DataType::Int64, false)];
        let table_uid = catalog
            .create_table(
                NS,
                TABLE,
                fields,
                TableScope::TenantOwned,
                tenant,
                &[],
                None,
            )
            .await
            .unwrap();

        let migrator = Arc::new(fixture.superuser_pool().await.expect("superuser pool"));

        Harness {
            _tmp: tmp,
            catalog_uri,
            backend,
            pool,
            recovery_pool,
            migrator,
            tenant,
            table_uid,
            _fixture: fixture,
        }
    }

    fn make_batch(n: i64) -> RecordBatch {
        // Pre-stamp user columns for a TenantOwned table with user_fields=[val].
        // `with_system_columns` appends run_id, card_uid, principal_id before the
        // server-owned wyrd_* columns. stamp_system_columns then appends the three
        // wyrd_* columns. The positional cast in write_batches requires the pre-stamp
        // column count and types to match the physical schema exactly.
        let schema = Arc::new(Schema::new(vec![
            Field::new("val", DataType::Int64, false),
            Field::new("run_id", DataType::Utf8, true),
            Field::new("card_uid", DataType::Utf8, true),
            Field::new("principal_id", DataType::Utf8, true),
        ]));
        let vals: Vec<i64> = (0..n).collect();
        let nrows = vals.len();
        RecordBatch::try_new(
            schema,
            vec![
                Arc::new(Int64Array::from(vals)),
                Arc::new(StringArray::from(vec![None::<&str>; nrows])),
                Arc::new(StringArray::from(vec![None::<&str>; nrows])),
                Arc::new(StringArray::from(vec![None::<&str>; nrows])),
            ],
        )
        .unwrap()
    }

    fn stamped_batch(n: i64, batch_id: [u8; 16]) -> RecordBatch {
        let user = make_batch(n);
        let now_us = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| i64::try_from(d.as_micros()).unwrap_or(i64::MAX));
        stamp_system_columns(&user, now_us, batch_id, None).unwrap()
    }

    /// Rebuild the catalog (simulates engine restart). Recovery runs on `WyrdCatalog::new`.
    async fn rebuild_catalog(h: &Harness) -> WyrdCatalog {
        WyrdCatalog::new(
            &h.catalog_uri,
            &h.backend,
            h.pool.clone(),
            Some(h.recovery_pool.clone()),
        )
        .await
        .unwrap()
    }

    /// Load the underlying iceberg Table for `commit_with_fault` calls.
    async fn load_table(h: &Harness) -> iceberg::table::Table {
        let (factory, props) = crate::catalog::storage::iceberg_storage_factory(&h.backend);
        let warehouse = crate::catalog::storage::warehouse_uri(&h.backend);
        let catalog = iceberg_sql::build_catalog(&h.catalog_uri, &warehouse, factory, props)
            .await
            .unwrap();
        let ident = iceberg::TableIdent::new(NS.to_namespace_ident(), TABLE.to_string());
        catalog.load_table(&ident).await.unwrap()
    }

    /// Load the raw `SqlCatalog` for `commit_with_fault` calls.
    async fn load_sql_catalog(h: &Harness) -> iceberg_catalog_sql::SqlCatalog {
        let (factory, props) = crate::catalog::storage::iceberg_storage_factory(&h.backend);
        let warehouse = crate::catalog::storage::warehouse_uri(&h.backend);
        iceberg_sql::build_catalog(&h.catalog_uri, &warehouse, factory, props)
            .await
            .unwrap()
    }

    // ── Tests ──────────────────────────────────────────────────────────────────────

    /// Writer crashes after inserting the precommit row (with an already-expired
    /// lease) but before writing any Parquet. Recovery on the next engine start
    /// claims the orphaned row, finds no matching snapshot (`snapshot_absent`), and
    /// finalizes it as 'aborted'.
    #[tokio::test]
    async fn precommit_row_aborts_on_restart() {
        let h = setup().await;
        let table = load_table(&h).await;
        let catalog = load_sql_catalog(&h).await;

        let batch_id = *uuid::Uuid::now_v7().as_bytes();
        let batch = stamped_batch(10, batch_id);

        // Inject fault: inserts precommit row with expired lease, then fails.
        run_commit_with_fault(
            &h.pool,
            &catalog,
            &table,
            &h.table_uid,
            vec![batch],
            batch_id,
            h.tenant,
            FaultPoint::AfterPreCommitRow {
                lease: Lease::Expired,
            },
        )
        .await
        .expect_err("fault must return an error");

        let pending: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM vala.olap_commits WHERE state = 'precommit'")
                .fetch_one(&*h.migrator)
                .await
                .unwrap();
        assert_eq!(pending, 1, "one orphaned precommit row");

        // Restart the engine — recovery runs in WyrdCatalog::new via SECURITY DEFINER.
        drop(catalog);
        let _recovered = rebuild_catalog(&h).await;

        let aborted: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM vala.olap_commits WHERE state = 'aborted'")
                .fetch_one(&*h.migrator)
                .await
                .unwrap();
        assert_eq!(aborted, 1, "recovery must abort the orphaned precommit");

        let audited: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM vala.olap_recovery_events WHERE oracle_result = 'snapshot_absent'",
        )
        .fetch_one(&*h.migrator)
        .await
        .unwrap();
        assert_eq!(audited, 1, "one snapshot_absent audit row");
    }

    /// Writer crashes AFTER the Iceberg commit but BEFORE `finalize_committed`. The
    /// snapshot is real and carries `wyrd_batch_id`. Recovery finds it (`snapshot_found`)
    /// and rolls forward: finalizes 'committed'.
    #[tokio::test]
    async fn snapshot_committed_but_not_finalized_rolls_forward() {
        let h = setup().await;
        let table = load_table(&h).await;
        let catalog = load_sql_catalog(&h).await;

        let batch_id = *uuid::Uuid::now_v7().as_bytes();
        let batch = stamped_batch(10, batch_id);

        // Inject fault: writes Parquet + commits Iceberg, then fails before SQL finalize.
        run_commit_with_fault(
            &h.pool,
            &catalog,
            &table,
            &h.table_uid,
            vec![batch],
            batch_id,
            h.tenant,
            FaultPoint::AfterIcebergCommit,
        )
        .await
        .expect_err("fault must return an error");

        // Row is still 'precommit' (finalize_committed was never called).
        let pending: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM vala.olap_commits WHERE state = 'precommit'")
                .fetch_one(&*h.migrator)
                .await
                .unwrap();
        assert_eq!(pending, 1, "precommit row persists after fault");

        // But the lease has NOT been expired by the fault, so recovery won't claim it
        // immediately (live lease). Expire it manually to let recovery in.
        sqlx::query(
            "UPDATE vala.olap_commits SET writer_lease_expires_at = now() - interval '1 second' \
             WHERE batch_id = $1",
        )
        .bind(batch_id.as_slice())
        .execute(&*h.migrator)
        .await
        .unwrap();

        // Restart the engine — recovery finds the snapshot and rolls forward.
        drop(catalog);
        let _recovered = rebuild_catalog(&h).await;

        let committed: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM vala.olap_commits WHERE state = 'committed'")
                .fetch_one(&*h.migrator)
                .await
                .unwrap();
        assert_eq!(committed, 1, "recovery must commit the roll-forward batch");

        let snapshot_id: Option<i64> = sqlx::query_scalar(
            "SELECT snapshot_id FROM vala.olap_commits WHERE state = 'committed'",
        )
        .fetch_one(&*h.migrator)
        .await
        .unwrap();
        assert!(
            snapshot_id.is_some(),
            "committed row must carry the discovered snapshot_id after roll-forward recovery"
        );

        let audited: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM vala.olap_recovery_events WHERE oracle_result = 'snapshot_found'",
        )
        .fetch_one(&*h.migrator)
        .await
        .unwrap();
        assert_eq!(audited, 1, "one snapshot_found audit row");
    }

    /// Recovery must NOT touch a precommit row whose writer lease is still in the
    /// future (live writer mid-commit). Only after the lease expires may recovery
    /// claim and reconcile the row.
    #[tokio::test]
    async fn recovery_skips_live_writer_lease() {
        let h = setup().await;

        // Insert a precommit row with a FUTURE lease manually (simulating a live writer).
        let table_uid = h.table_uid;
        let batch_id = *uuid::Uuid::now_v7().as_bytes();
        let owner = sqlx::types::Uuid::new_v4();

        let mut conn = vala_sql::TenantConn::acquire(&h.pool, h.tenant)
            .await
            .unwrap();
        vala_sql::queries::olap_catalog::precommit(
            &mut conn,
            table_uid.as_bytes(),
            &batch_id,
            "system",
            "system",
        )
        .await
        .unwrap();
        // Stamp a future-expiring lease so claim_stale_precommits skips it.
        sqlx::query(
            "UPDATE vala.olap_commits \
             SET writer_owner = $1, writer_fencing_token = 1, \
                 writer_lease_expires_at = now() + interval '60 seconds' \
             WHERE table_uid = $2 AND batch_id = $3",
        )
        .bind(owner)
        .bind(table_uid.as_bytes().as_slice())
        .bind(batch_id.as_slice())
        .execute(&mut **conn.transaction())
        .await
        .unwrap();
        conn.commit().await.unwrap();

        // Recovery runs on catalog construction.
        let _catalog = rebuild_catalog(&h).await;

        // Row must still be 'precommit' — recovery skipped the live lease.
        let state: String = sqlx::query_scalar(
            "SELECT state FROM vala.olap_commits WHERE table_uid = $1 AND batch_id = $2",
        )
        .bind(table_uid.as_bytes().as_slice())
        .bind(batch_id.as_slice())
        .fetch_one(&*h.migrator)
        .await
        .unwrap();
        assert_eq!(state, "precommit", "live lease must protect the row");

        let events: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM vala.olap_recovery_events")
            .fetch_one(&*h.migrator)
            .await
            .unwrap();
        assert_eq!(events, 0, "no audit row for skipped live lease");

        // Now expire the lease and rebuild — recovery must claim and abort.
        sqlx::query(
            "UPDATE vala.olap_commits SET writer_lease_expires_at = now() - interval '1 second' \
             WHERE batch_id = $1",
        )
        .bind(batch_id.as_slice())
        .execute(&*h.migrator)
        .await
        .unwrap();

        let _catalog2 = rebuild_catalog(&h).await;

        let aborted: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM vala.olap_commits WHERE state = 'aborted'")
                .fetch_one(&*h.migrator)
                .await
                .unwrap();
        assert_eq!(aborted, 1, "expired lease is now claimed and aborted");
    }

    /// Cold start: recovery resolves table identity purely from the
    /// `bifrost_tables` join in `claim_stale_precommits`, not from any in-memory
    /// registry state. The engine starts with an empty cache.
    #[tokio::test]
    async fn cold_start_maps_table_identity() {
        let h = setup().await;

        // Insert a bare precommit row with no lease (null = eligible for recovery).
        let batch_id = *uuid::Uuid::now_v7().as_bytes();
        let mut conn = vala_sql::TenantConn::acquire(&h.pool, h.tenant)
            .await
            .unwrap();
        vala_sql::queries::olap_catalog::precommit(
            &mut conn,
            h.table_uid.as_bytes(),
            &batch_id,
            "system",
            "system",
        )
        .await
        .unwrap();
        // Leave writer_lease_expires_at NULL so claim_stale_precommits picks it up.
        conn.commit().await.unwrap();

        // Cold-start: rebuild with a fresh catalog (empty in-memory registry).
        let _catalog = rebuild_catalog(&h).await;

        // Recovery must have resolved the table from the bifrost_tables join and
        // decided snapshot_absent (no Iceberg snapshot was ever committed).
        let aborted: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM vala.olap_commits WHERE state = 'aborted'")
                .fetch_one(&*h.migrator)
                .await
                .unwrap();
        assert_eq!(
            aborted, 1,
            "cold-start recovery must map identity from bifrost_tables join and abort"
        );
    }

    /// Same cold-start identity mapping, but for a `SystemShared` table bound to
    /// `SYSTEM_OWNER`. A scope-specific bug in the RLS bind for system tables would
    /// pass `cold_start_maps_table_identity` and fail silently here.
    #[tokio::test]
    async fn cold_start_maps_system_shared_table_identity() {
        let h = setup().await;

        // Register a system-shared table owned by SYSTEM_OWNER.
        let catalog = rebuild_catalog(&h).await;
        let fields = vec![Field::new("val", DataType::Int64, false)];
        let sys_uid = catalog
            .create_table(
                NS,
                "crash_test_sys",
                fields,
                TableScope::SystemShared,
                DataTenantId::SYSTEM_OWNER,
                &[],
                None,
            )
            .await
            .unwrap();
        drop(catalog);

        // Bare precommit row under SYSTEM_OWNER (no lease = eligible for recovery).
        let batch_id = *uuid::Uuid::now_v7().as_bytes();
        let mut conn = vala_sql::TenantConn::acquire(&h.pool, DataTenantId::SYSTEM_OWNER)
            .await
            .unwrap();
        vala_sql::queries::olap_catalog::precommit(
            &mut conn,
            sys_uid.as_bytes(),
            &batch_id,
            "system",
            "system",
        )
        .await
        .unwrap();
        conn.commit().await.unwrap();

        // Cold-start rebuild — recovery must resolve the system-shared identity and abort.
        let _catalog = rebuild_catalog(&h).await;

        let state: String = sqlx::query_scalar(
            "SELECT state FROM vala.olap_commits WHERE table_uid = $1 AND batch_id = $2",
        )
        .bind(sys_uid.as_bytes().as_slice())
        .bind(batch_id.as_slice())
        .fetch_one(&*h.migrator)
        .await
        .unwrap();
        assert_eq!(
            state, "aborted",
            "cold-start recovery must map SystemShared identity and abort"
        );
    }

    /// Recovery is disabled-by-default: with `recovery_pool = None`, `WyrdCatalog::new`
    /// must skip the startup scan entirely and leave stale precommit rows untouched.
    /// Guards the MAJOR-4 contract — a regression that ran recovery unconditionally
    /// (or on the wrong pool) would abort rows it must never touch.
    #[tokio::test]
    async fn recovery_disabled_when_no_recovery_pool() {
        let h = setup().await;

        // Bare precommit row (no lease) — would be claimed if recovery ran.
        let batch_id = *uuid::Uuid::now_v7().as_bytes();
        let mut conn = vala_sql::TenantConn::acquire(&h.pool, h.tenant)
            .await
            .unwrap();
        vala_sql::queries::olap_catalog::precommit(
            &mut conn,
            h.table_uid.as_bytes(),
            &batch_id,
            "system",
            "system",
        )
        .await
        .unwrap();
        conn.commit().await.unwrap();

        // Construct WITHOUT a recovery pool — recovery must be skipped.
        let _catalog = WyrdCatalog::new(&h.catalog_uri, &h.backend, h.pool.clone(), None)
            .await
            .unwrap();

        let state: String = sqlx::query_scalar(
            "SELECT state FROM vala.olap_commits WHERE table_uid = $1 AND batch_id = $2",
        )
        .bind(h.table_uid.as_bytes().as_slice())
        .bind(batch_id.as_slice())
        .fetch_one(&*h.migrator)
        .await
        .unwrap();
        assert_eq!(
            state, "precommit",
            "disabled recovery must leave the stale precommit row untouched"
        );

        let events: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM vala.olap_recovery_events")
            .fetch_one(&*h.migrator)
            .await
            .unwrap();
        assert_eq!(events, 0, "disabled recovery must write no audit rows");
    }

    // The `scan_failed` oracle branch — a transient catalog/object-store failure
    // must leave the row 'precommit', increment `recovery_attempts`, write a
    // `scan_failed` audit row, and stay re-claimable — is covered deterministically
    // at the SQL contract level by `scan_failed_leaves_precommit_and_allows_retry`
    // in vala-sql/tests/olap_recovery.rs, which does not need a fault-injecting
    // StorageFactory mock.

    /// After recovery claims an expired precommit row, a zombie writer that resumes
    /// and calls `renew_writer_fence()` finds `recovery_fencing_token` IS NOT NULL and
    /// is fenced out before it can `fast_append`.
    ///
    /// Simulates the recovery-claim step by directly stamping `recovery_fencing_token`
    /// (what `vala.claim_stale_precommits` does), then proves the guarded renewal
    /// returns false and leaves the FSM unchanged.
    #[tokio::test]
    async fn zombie_writer_fenced_after_recovery_claim() {
        const TOKEN: i64 = 99;
        let h = setup().await;
        let batch_id = *uuid::Uuid::now_v7().as_bytes();
        let owner = sqlx::types::Uuid::new_v4();

        // Insert precommit row with expired lease and stamp recovery_fencing_token
        // (simulating what claim_stale_precommits does atomically).
        let mut conn = vala_sql::TenantConn::acquire(&h.pool, h.tenant)
            .await
            .unwrap();
        vala_sql::queries::olap_catalog::precommit(
            &mut conn,
            h.table_uid.as_bytes(),
            &batch_id,
            "system",
            "system",
        )
        .await
        .unwrap();
        sqlx::query(
            "UPDATE vala.olap_commits \
             SET writer_owner = $1, writer_fencing_token = $2, \
                 writer_lease_expires_at = now() - interval '1 second', \
                 recovery_fencing_token = 777 \
             WHERE table_uid = $3 AND batch_id = $4",
        )
        .bind(owner)
        .bind(TOKEN)
        .bind(h.table_uid.as_bytes().as_slice())
        .bind(batch_id.as_slice())
        .execute(&mut **conn.transaction())
        .await
        .unwrap();
        conn.commit().await.unwrap();

        // Zombie writer tries to renew — recovery_fencing_token IS NOT NULL → fenced.
        let mut conn2 = vala_sql::TenantConn::acquire(&h.pool, h.tenant)
            .await
            .unwrap();
        let held = vala_sql::queries::olap_catalog::renew_writer_fence(
            &mut conn2,
            h.table_uid.as_bytes(),
            &batch_id,
            owner,
            TOKEN,
            30,
        )
        .await
        .unwrap();
        conn2.commit().await.unwrap();

        assert!(!held, "recovery claim must fence the zombie writer out");

        let state: String = sqlx::query_scalar(
            "SELECT state FROM vala.olap_commits WHERE table_uid = $1 AND batch_id = $2",
        )
        .bind(h.table_uid.as_bytes().as_slice())
        .bind(batch_id.as_slice())
        .fetch_one(&*h.migrator)
        .await
        .unwrap();
        assert_eq!(state, "precommit", "zombie must not advance the FSM");

        let recovery_token: Option<i64> = sqlx::query_scalar(
            "SELECT recovery_fencing_token FROM vala.olap_commits WHERE table_uid = $1 AND batch_id = $2",
        )
        .bind(h.table_uid.as_bytes().as_slice())
        .bind(batch_id.as_slice())
        .fetch_one(&*h.migrator)
        .await
        .unwrap();
        assert!(
            recovery_token.is_some(),
            "recovery_fencing_token must remain set"
        );
    }

    /// An expired-lease writer is fenced by the guarded renewal even when recovery
    /// has NOT yet claimed the row — the `writer_lease_expires_at` > `now()` predicate
    /// fails closed before any recovery involvement.
    #[tokio::test]
    async fn expired_writer_fenced_before_recovery_claim() {
        const TOKEN: i64 = 42;
        let h = setup().await;
        let batch_id = *uuid::Uuid::now_v7().as_bytes();
        let owner = sqlx::types::Uuid::new_v4();

        // Insert precommit row with already-expired lease; recovery_fencing_token stays NULL.
        let mut conn = vala_sql::TenantConn::acquire(&h.pool, h.tenant)
            .await
            .unwrap();
        vala_sql::queries::olap_catalog::precommit(
            &mut conn,
            h.table_uid.as_bytes(),
            &batch_id,
            "system",
            "system",
        )
        .await
        .unwrap();
        sqlx::query(
            "UPDATE vala.olap_commits \
             SET writer_owner = $1, writer_fencing_token = $2, \
                 writer_lease_expires_at = now() - interval '1 second' \
             WHERE table_uid = $3 AND batch_id = $4",
        )
        .bind(owner)
        .bind(TOKEN)
        .bind(h.table_uid.as_bytes().as_slice())
        .bind(batch_id.as_slice())
        .execute(&mut **conn.transaction())
        .await
        .unwrap();
        conn.commit().await.unwrap();

        // Guarded renewal must fail closed — lease expired, no recovery claim yet.
        let mut conn2 = vala_sql::TenantConn::acquire(&h.pool, h.tenant)
            .await
            .unwrap();
        let held = vala_sql::queries::olap_catalog::renew_writer_fence(
            &mut conn2,
            h.table_uid.as_bytes(),
            &batch_id,
            owner,
            TOKEN,
            30,
        )
        .await
        .unwrap();
        conn2.commit().await.unwrap();

        assert!(
            !held,
            "expired lease must fence writer even before recovery claims"
        );

        let state: String = sqlx::query_scalar(
            "SELECT state FROM vala.olap_commits WHERE table_uid = $1 AND batch_id = $2",
        )
        .bind(h.table_uid.as_bytes().as_slice())
        .bind(batch_id.as_slice())
        .fetch_one(&*h.migrator)
        .await
        .unwrap();
        assert_eq!(
            state, "precommit",
            "row stays precommit — recovery never ran"
        );

        let events: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM vala.olap_recovery_events")
            .fetch_one(&*h.migrator)
            .await
            .unwrap();
        assert_eq!(events, 0, "no recovery events — no recovery ran");
    }

    /// A freshly renewed lease (`writer_lease_expires_at` well in the future) must
    /// prevent recovery from claiming the row, even if recovery runs between the
    /// renewal and the Iceberg append.
    #[tokio::test]
    async fn recovery_skips_after_unexpired_renewal() {
        const TOKEN: i64 = 55;
        let h = setup().await;
        let batch_id = *uuid::Uuid::now_v7().as_bytes();
        let owner = sqlx::types::Uuid::new_v4();

        // Insert precommit row with a fresh future lease — simulates a writer that
        // just renewed its lease before being paused between renewal and fast_append.
        let mut conn = vala_sql::TenantConn::acquire(&h.pool, h.tenant)
            .await
            .unwrap();
        vala_sql::queries::olap_catalog::precommit(
            &mut conn,
            h.table_uid.as_bytes(),
            &batch_id,
            "system",
            "system",
        )
        .await
        .unwrap();
        sqlx::query(
            "UPDATE vala.olap_commits \
             SET writer_owner = $1, writer_fencing_token = $2, \
                 writer_lease_expires_at = now() + interval '60 seconds' \
             WHERE table_uid = $3 AND batch_id = $4",
        )
        .bind(owner)
        .bind(TOKEN)
        .bind(h.table_uid.as_bytes().as_slice())
        .bind(batch_id.as_slice())
        .execute(&mut **conn.transaction())
        .await
        .unwrap();
        conn.commit().await.unwrap();

        // Recovery runs — must skip the live lease.
        let _catalog = rebuild_catalog(&h).await;

        let state: String = sqlx::query_scalar(
            "SELECT state FROM vala.olap_commits WHERE table_uid = $1 AND batch_id = $2",
        )
        .bind(h.table_uid.as_bytes().as_slice())
        .bind(batch_id.as_slice())
        .fetch_one(&*h.migrator)
        .await
        .unwrap();
        assert_eq!(
            state, "precommit",
            "freshly renewed lease must block recovery claim"
        );

        let events: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM vala.olap_recovery_events")
            .fetch_one(&*h.migrator)
            .await
            .unwrap();
        assert_eq!(
            events, 0,
            "no recovery events — recovery skipped the live lease"
        );

        // The fence must still be renewable — the writer can proceed to fast_append.
        let mut conn2 = vala_sql::TenantConn::acquire(&h.pool, h.tenant)
            .await
            .unwrap();
        let still_held = vala_sql::queries::olap_catalog::renew_writer_fence(
            &mut conn2,
            h.table_uid.as_bytes(),
            &batch_id,
            owner,
            TOKEN,
            30,
        )
        .await
        .unwrap();
        conn2.commit().await.unwrap();
        assert!(
            still_held,
            "fence must remain renewable after recovery skip"
        );
    }

    /// The bounded post-append residual: a writer renews its lease, stalls longer
    /// than `lease_ttl`, recovery claims and aborts the row, and only then does the
    /// writer's `fast_append` land. The writer's `finalize_committed` finds zero
    /// matching rows (the row is no longer `precommit`) and records the loss.
    ///
    /// This builds the post-abort state directly — no concurrency needed — and
    /// verifies the writer-side detection + audit contract: `finalize_committed`
    /// returns `false`, and a `fence_lost_after_append` audit row is written with
    /// `old_state = new_state = 'aborted'` and `rolled_back = false` (this
    /// iceberg-rust version cannot roll back the orphaned snapshot). The bare audit
    /// INSERT contract is covered separately by `audit_row_roundtrips_with_byte_ids`
    /// in vala-sql/tests/olap_recovery.rs.
    #[allow(clippy::too_many_lines)]
    #[tokio::test]
    async fn fence_lost_after_append_is_detected_and_audited() {
        let h = setup().await;

        let batch_id = *uuid::Uuid::now_v7().as_bytes();
        let writer_owner = sqlx::types::Uuid::new_v4();
        let writer_token: i64 = 7;
        let snapshot_id: i64 = 4242;

        // Writer inserts the precommit row with an (already-expired) lease, then stalls.
        let mut conn = vala_sql::TenantConn::acquire(&h.pool, h.tenant)
            .await
            .unwrap();
        vala_sql::queries::olap_catalog::precommit(
            &mut conn,
            h.table_uid.as_bytes(),
            &batch_id,
            "system",
            "system",
        )
        .await
        .unwrap();
        sqlx::query(
            "UPDATE vala.olap_commits \
             SET writer_owner = $1, writer_fencing_token = $2, \
                 writer_lease_expires_at = now() - interval '1 second' \
             WHERE table_uid = $3 AND batch_id = $4",
        )
        .bind(writer_owner)
        .bind(writer_token)
        .bind(h.table_uid.as_bytes().as_slice())
        .bind(batch_id.as_slice())
        .execute(&mut **conn.transaction())
        .await
        .unwrap();
        conn.commit().await.unwrap();

        // Recovery claims the expired-lease row and aborts it (snapshot_absent).
        let _catalog = rebuild_catalog(&h).await;
        let state: String = sqlx::query_scalar(
            "SELECT state FROM vala.olap_commits WHERE table_uid = $1 AND batch_id = $2",
        )
        .bind(h.table_uid.as_bytes().as_slice())
        .bind(batch_id.as_slice())
        .fetch_one(&*h.migrator)
        .await
        .unwrap();
        assert_eq!(
            state, "aborted",
            "recovery must abort the stalled precommit"
        );

        // The writer's delayed fast_append lands; finalize_committed must now fail
        // (the row is no longer 'precommit' and carries a recovery fencing token).
        let mut conn = vala_sql::TenantConn::acquire(&h.pool, h.tenant)
            .await
            .unwrap();
        let committed = vala_sql::queries::olap_catalog::finalize_committed(
            &mut conn,
            h.table_uid.as_bytes(),
            &batch_id,
            snapshot_id,
            writer_owner,
            writer_token,
        )
        .await
        .unwrap();
        assert!(
            !committed,
            "writer that lost the fence must not finalize 'committed'"
        );

        // Writer records the residual: it appended a snapshot it can no longer own.
        vala_sql::queries::olap_catalog::record_fence_loss_after_append(
            &mut conn,
            h.table_uid.as_bytes(),
            &batch_id,
            writer_owner,
            writer_token,
            snapshot_id,
            false,
            Some("fence lost after fast_append; recovery already aborted the row"),
        )
        .await
        .unwrap();
        conn.commit().await.unwrap();

        let (old_state, new_state, rolled_back): (String, String, Option<bool>) = sqlx::query_as(
            "SELECT old_state, new_state, rolled_back FROM vala.olap_recovery_events \
             WHERE table_uid = $1 AND batch_id = $2 AND event_kind = 'fence_lost_after_append'",
        )
        .bind(h.table_uid.as_bytes().as_slice())
        .bind(batch_id.as_slice())
        .fetch_one(&*h.migrator)
        .await
        .unwrap();
        assert_eq!(
            old_state, "aborted",
            "fence-loss audit captures post-abort state"
        );
        assert_eq!(
            new_state, "aborted",
            "fence loss does not change the FSM state"
        );
        assert_eq!(
            rolled_back,
            Some(false),
            "snapshot could not be rolled back"
        );

        // The recovery decision that aborted the row is also on the audit trail.
        let decisions: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM vala.olap_recovery_events \
             WHERE event_kind = 'recovery_decision' AND new_state = 'aborted'",
        )
        .fetch_one(&*h.migrator)
        .await
        .unwrap();
        assert_eq!(decisions, 1, "recovery_decision abort row must be present");
    }
}
