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
    use crate::writer::CommitKey;
    use crate::writer::commit::{
        FaultPoint, Lease, append_snapshot, commit_group_to_iceberg, run_commit_with_fault,
        write_batches,
    };
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

    /// A group commit lands its Iceberg snapshot but crashes before finalize: the
    /// snapshot carries `wyrd_commit_keys` (the `{data_tenant_id, batch_id}` pair),
    /// NOT `wyrd_batch_id`. Recovery must roll the key forward by matching the pair.
    ///
    /// Red on the pre-02.3 tree: recovery matched only `wyrd_batch_id`, so it would
    /// abort a genuinely committed group snapshot.
    #[tokio::test]
    async fn group_commit_recovers_unfinalized_key() {
        let h = setup().await;
        let table = load_table(&h).await;
        let sql_catalog = load_sql_catalog(&h).await;

        let batch_id = *uuid::Uuid::now_v7().as_bytes();

        // Precommit row for {h.tenant, batch_id}, left unfinalized (NULL lease →
        // eligible for recovery) — simulates a crash before finalize.
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

        // Real group append: writes Parquet + commits a snapshot stamping
        // wyrd_commit_keys=[{h.tenant, batch_id}] and no wyrd_batch_id.
        let files = write_batches(&table, vec![stamped_batch(7, batch_id)])
            .await
            .unwrap();
        let (snapshot_id, _updated) = commit_group_to_iceberg(
            &sql_catalog,
            &table,
            files,
            &[CommitKey::new(h.tenant, batch_id)],
        )
        .await
        .unwrap();

        // Restart → recovery claims the orphaned row and rolls it forward.
        let _recovered = rebuild_catalog(&h).await;

        let (state, row_sid): (String, Option<i64>) = sqlx::query_as(
            "SELECT state, snapshot_id FROM vala.olap_commits \
             WHERE table_uid = $1 AND batch_id = $2",
        )
        .bind(h.table_uid.as_bytes().as_slice())
        .bind(batch_id.as_slice())
        .fetch_one(&*h.migrator)
        .await
        .unwrap();
        assert_eq!(
            state, "committed",
            "pair match in wyrd_commit_keys must roll the group key forward"
        );
        assert_eq!(
            row_sid,
            Some(snapshot_id),
            "recovered row carries the snapshot_id found via wyrd_commit_keys"
        );

        let found: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM vala.olap_recovery_events WHERE oracle_result = 'snapshot_found'",
        )
        .fetch_one(&*h.migrator)
        .await
        .unwrap();
        assert_eq!(found, 1, "one snapshot_found audit row");
    }

    /// M02 recovery discrimination: a snapshot stamps BOTH the legacy
    /// `wyrd_batch_id` AND a `wyrd_commit_keys` entry for a DIFFERENT data tenant
    /// that shares this row's `batch_id`. Recovery for THIS tenant's row must NOT
    /// match the other tenant's snapshot — the pair is authoritative — so it aborts.
    ///
    /// Red on the pre-02.3 tree: recovery matched `wyrd_batch_id` alone and would
    /// wrongly finalize this row committed against the other tenant's snapshot.
    #[tokio::test]
    async fn recovery_by_commitkey_not_batch_id() {
        let h = setup().await;
        let table = load_table(&h).await;
        let sql_catalog = load_sql_catalog(&h).await;

        let batch_id = *uuid::Uuid::now_v7().as_bytes();
        // A different data tenant that shares the same batch_id in the snapshot.
        let other_tenant = DataTenantId::new_v7();

        // Precommit row for THIS tenant {h.tenant, batch_id}, left unfinalized.
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

        // Real snapshot stamping BOTH the legacy wyrd_batch_id (would trip a
        // batch_id-only match) AND wyrd_commit_keys for the OTHER tenant only.
        let files = write_batches(&table, vec![stamped_batch(5, batch_id)])
            .await
            .unwrap();
        let other_hex = other_tenant.as_uuid().simple();
        let batch_hex = uuid::Uuid::from_bytes(batch_id).simple();
        let mut props = std::collections::HashMap::new();
        props.insert(
            "wyrd_commit_keys".to_string(),
            format!(r#"[{{"data_tenant_id":"{other_hex}","batch_id":"{batch_hex}"}}]"#),
        );
        props.insert("wyrd_batch_id".to_string(), batch_hex.to_string());
        let _ = append_snapshot(&sql_catalog, &table, files, props)
            .await
            .unwrap();

        // Restart → recovery. This tenant's pair is absent from wyrd_commit_keys.
        let _recovered = rebuild_catalog(&h).await;

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
            "batch_id alone must not match another tenant's commit key"
        );

        let absent: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM vala.olap_recovery_events WHERE oracle_result = 'snapshot_absent'",
        )
        .fetch_one(&*h.migrator)
        .await
        .unwrap();
        assert_eq!(absent, 1, "one snapshot_absent audit row");
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

    /// LIVE recovery sweep (`commit_recovery::tick`) — the crux of the CRITICAL
    /// fix. A stale precommit whose `{data_tenant_id, batch_id}` pair is ABSENT
    /// from every snapshot, on a table that DOES carry a later unrelated
    /// snapshot, must resolve `aborted`, never `committed`.
    ///
    /// Red before the fix: the sweep's oracle returned the table's
    /// `current_snapshot_id()` for any registered table without checking pair
    /// membership, so it wrongly finalized a crashed-before-append batch
    /// `committed` against the unrelated snapshot — a silent lost write once the
    /// client's idempotent retry dedups against the false commit.
    #[tokio::test]
    async fn live_sweep_aborts_precommit_absent_from_snapshots() {
        let h = setup().await;
        let table = load_table(&h).await;
        let sql_catalog = load_sql_catalog(&h).await;

        // Mint an UNRELATED committed snapshot carrying a DIFFERENT batch's pair,
        // so the table's current_snapshot_id() is Some(..) — the exact condition
        // that tricked the old oracle into finalizing committed.
        let other_batch = *uuid::Uuid::now_v7().as_bytes();
        let files = write_batches(&table, vec![stamped_batch(5, other_batch)])
            .await
            .unwrap();
        let tenant_hex = h.tenant.as_uuid().simple();
        let other_batch_hex = uuid::Uuid::from_bytes(other_batch).simple();
        let mut props = std::collections::HashMap::new();
        props.insert(
            "wyrd_commit_keys".to_string(),
            format!(r#"[{{"data_tenant_id":"{tenant_hex}","batch_id":"{other_batch_hex}"}}]"#),
        );
        append_snapshot(&sql_catalog, &table, files, props)
            .await
            .unwrap();

        // Orphaned precommit for a DIFFERENT batch, absent from every snapshot,
        // with a NULL lease so the sweep claims it.
        let orphan_batch = *uuid::Uuid::now_v7().as_bytes();
        let mut conn = vala_sql::TenantConn::acquire(&h.pool, h.tenant)
            .await
            .unwrap();
        vala_sql::queries::olap_catalog::precommit(
            &mut conn,
            h.table_uid.as_bytes(),
            &orphan_batch,
            "system",
            "system",
        )
        .await
        .unwrap();
        conn.commit().await.unwrap();

        // Oracle catalog with NO recovery pool → its startup scan is skipped, so
        // the ONLY thing that resolves the orphan is the live sweep called below.
        let catalog = WyrdCatalog::new(&h.catalog_uri, &h.backend, h.pool.clone(), None)
            .await
            .unwrap();
        let vala = vala_sql::postgres::ValaPostgres::from_pools(
            (*h.pool).clone(),
            Some((*h.recovery_pool).clone()),
        );

        let outcome = crate::serving::repair::commit_recovery::tick(&vala, &catalog, 10)
            .await
            .unwrap();

        assert_eq!(outcome.claimed, 1, "the orphan precommit is claimed");
        assert_eq!(
            outcome.resolved_committed, 0,
            "a batch absent from all snapshots must NOT be finalized committed \
             against the unrelated snapshot"
        );
        assert_eq!(
            outcome.resolved_aborted, 1,
            "pair-membership oracle must abort the never-appended batch"
        );

        let state: String = sqlx::query_scalar(
            "SELECT state FROM vala.olap_commits WHERE table_uid = $1 AND batch_id = $2",
        )
        .bind(h.table_uid.as_bytes().as_slice())
        .bind(orphan_batch.as_slice())
        .fetch_one(&*h.migrator)
        .await
        .unwrap();
        assert_eq!(
            state, "aborted",
            "durable FSM state is aborted, not committed"
        );
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
            vala_sql::queries::olap_catalog::FenceLossRecord {
                table_uid: h.table_uid.as_bytes(),
                batch_id: &batch_id,
                owner: writer_owner,
                token: writer_token,
                snapshot_id,
                rolled_back: false,
                error: Some("fence lost after fast_append; recovery already aborted the row"),
            },
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

/// Group-commit gates: N per-tenant precommit txns → ONE Iceberg `fast_append` →
/// N per-tenant finalize txns, driven through `run_group_commit`.
///
/// In the Postgres lane (`pg_tests`) — needs a real Postgres + Iceberg warehouse.
#[cfg(test)]
mod group_commit {
    use std::sync::Arc;

    use crate::batch_builder::stamp_system_columns;
    use crate::catalog::WyrdCatalog;
    use crate::catalog::iceberg_sql;
    use crate::catalog::namespaces::BifrostNamespace;
    use crate::types::{TableScope, TableUid};
    use crate::writer::BifrostWriteContext;
    use crate::writer::CommitKey;
    use crate::writer::commit::{CommitGroup, run_group_commit};
    use arrow::array::{Int64Array, StringArray};
    use arrow::datatypes::{DataType, Field, Schema};
    use arrow::record_batch::RecordBatch;
    use sqlx::PgPool;
    use tempfile::TempDir;
    use wyrd_dev_fixtures::pg::PgFixture;
    use wyrd_spec::ids::DataTenantId;
    use wyrd_storage::settings::BackendConfig;

    const NS: BifrostNamespace = BifrostNamespace::Bifrost;

    struct Harness {
        _tmp: TempDir,
        catalog_uri: String,
        backend: BackendConfig,
        pool: Arc<PgPool>,
        migrator: Arc<PgPool>,
        fixture: PgFixture,
    }

    async fn base_harness() -> Harness {
        let fixture = PgFixture::start().await.expect("fixture");
        let pool = Arc::new(fixture.app_pool().clone());
        let tmp = tempfile::tempdir().unwrap();
        let backend = BackendConfig::Local {
            root: tmp.path().to_path_buf(),
        };
        let catalog_uri = fixture.catalog_uri();
        let migrator = Arc::new(fixture.superuser_pool().await.expect("superuser pool"));
        Harness {
            _tmp: tmp,
            catalog_uri,
            backend,
            pool,
            migrator,
            fixture,
        }
    }

    async fn build_catalog(h: &Harness) -> WyrdCatalog {
        WyrdCatalog::new(&h.catalog_uri, &h.backend, h.pool.clone(), None)
            .await
            .unwrap()
    }

    async fn load_table(h: &Harness, table: &str) -> iceberg::table::Table {
        use iceberg::Catalog as _;
        let (factory, props) = crate::catalog::storage::iceberg_storage_factory(&h.backend);
        let warehouse = crate::catalog::storage::warehouse_uri(&h.backend);
        let catalog = iceberg_sql::build_catalog(&h.catalog_uri, &warehouse, factory, props)
            .await
            .unwrap();
        let ident = iceberg::TableIdent::new(NS.to_namespace_ident(), table.to_string());
        catalog.load_table(&ident).await.unwrap()
    }

    async fn load_sql_catalog(h: &Harness) -> iceberg_catalog_sql::SqlCatalog {
        let (factory, props) = crate::catalog::storage::iceberg_storage_factory(&h.backend);
        let warehouse = crate::catalog::storage::warehouse_uri(&h.backend);
        iceberg_sql::build_catalog(&h.catalog_uri, &warehouse, factory, props)
            .await
            .unwrap()
    }

    /// User batch: `val` + the three server-resolved-or-null correlation columns.
    fn user_batch(n: i64) -> RecordBatch {
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

    fn stamped(n: i64, batch_id: [u8; 16], tenant: Option<DataTenantId>) -> RecordBatch {
        let user = user_batch(n);
        let now_us = i64::try_from(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_micros()),
        )
        .unwrap_or(i64::MAX);
        stamp_system_columns(&user, now_us, batch_id, tenant).unwrap()
    }

    fn ctx(batch_id: [u8; 16]) -> BifrostWriteContext {
        let mut c = BifrostWriteContext::system();
        c.batch_id = batch_id;
        c
    }

    /// Count snapshots on the current-branch history whose summary carries
    /// `wyrd_commit_keys`, and return the current snapshot's `wyrd_commit_keys` value.
    fn commit_keys_of_current(table: &iceberg::table::Table) -> Option<String> {
        let meta = table.metadata();
        let sid = meta.current_snapshot_id()?;
        let snap = meta.snapshot_by_id(sid)?;
        snap.summary()
            .additional_properties
            .get("wyrd_commit_keys")
            .cloned()
    }

    fn snapshot_count(table: &iceberg::table::Table) -> usize {
        table.metadata().snapshots().count()
    }

    /// N≥3 distinct-`batch_id` groups (single tenant, `TenantOwned`) → ONE snapshot,
    /// all committed, all sharing the same `snapshot_id`.
    #[tokio::test]
    async fn group_commit_one_snapshot_many_batches() {
        const N: usize = 3;
        const TABLE: &str = "group_many";

        let h = base_harness().await;
        let tenant = DataTenantId::new_v7();
        h.fixture
            .seed_additional_tenant_with_uuid(tenant, &format!("t-{}", tenant.as_uuid().simple()))
            .await
            .unwrap();

        let catalog = build_catalog(&h).await;
        let table_uid: TableUid = catalog
            .create_table(
                NS,
                TABLE,
                vec![Field::new("val", DataType::Int64, false)],
                TableScope::TenantOwned,
                tenant,
                &[],
                None,
            )
            .await
            .unwrap();
        drop(catalog);

        let table = load_table(&h, TABLE).await;
        let before = snapshot_count(&table);
        let sql_catalog = load_sql_catalog(&h).await;

        let mut batch_ids: Vec<[u8; 16]> = Vec::new();
        let mut groups: Vec<CommitGroup> = Vec::new();
        for i in 0..N {
            let batch_id = *uuid::Uuid::now_v7().as_bytes();
            batch_ids.push(batch_id);
            groups.push(CommitGroup {
                key: CommitKey::new(tenant, batch_id),
                ctx: ctx(batch_id),
                batches: vec![stamped(i64::try_from(i).unwrap() + 1, batch_id, None)],
                audit: None,
            });
        }

        let outcome = run_group_commit(
            &h.pool,
            &sql_catalog,
            &table,
            &table_uid,
            TableScope::TenantOwned,
            groups,
        )
        .await
        .unwrap();

        assert!(outcome.snapshot_id.is_some(), "one snapshot committed");
        assert_eq!(outcome.committed.len(), N, "all N keys committed");
        assert!(outcome.replayed.is_empty(), "nothing replayed");
        let sid = outcome.snapshot_id.unwrap();

        // All N olap_commits rows are committed and share the same snapshot_id.
        let rows: Vec<(String, Option<i64>)> =
            sqlx::query_as("SELECT state, snapshot_id FROM vala.olap_commits WHERE table_uid = $1")
                .bind(table_uid.as_bytes().as_slice())
                .fetch_all(&*h.migrator)
                .await
                .unwrap();
        assert_eq!(rows.len(), N, "one row per batch");
        for (state, row_sid) in &rows {
            assert_eq!(state, "committed", "every row committed");
            assert_eq!(*row_sid, Some(sid), "all rows share the group snapshot_id");
        }

        // Exactly ONE new Iceberg snapshot, and it carries all N keys.
        let after = load_table(&h, TABLE).await;
        assert_eq!(
            snapshot_count(&after),
            before + 1,
            "exactly one new snapshot from the group commit"
        );
        let keys_json = commit_keys_of_current(&after).expect("wyrd_commit_keys present");
        let parsed: serde_json::Value = serde_json::from_str(&keys_json).unwrap();
        let arr = parsed.as_array().unwrap();
        assert_eq!(arr.len(), N, "snapshot lists all N commit keys");
        let listed: std::collections::HashSet<String> = arr
            .iter()
            .map(|o| o["batch_id"].as_str().unwrap().to_string())
            .collect();
        for batch_id in &batch_ids {
            let hex = uuid::Uuid::from_bytes(*batch_id).simple().to_string();
            assert!(listed.contains(&hex), "batch_id {hex} listed in summary");
        }
    }

    /// `SystemShared` table, two tenants, SAME `batch_id` → two distinct
    /// `CommitKey`s, both committed, no dedup collision. Re-run is fully idempotent.
    #[allow(clippy::too_many_lines)]
    #[tokio::test]
    async fn same_batch_id_two_tenants_no_collision() {
        const TABLE: &str = "group_shared";

        let h = base_harness().await;
        let tenant_a = DataTenantId::new_v7();
        let tenant_b = DataTenantId::new_v7();
        h.fixture
            .seed_additional_tenant_with_uuid(
                tenant_a,
                &format!("a-{}", tenant_a.as_uuid().simple()),
            )
            .await
            .unwrap();
        h.fixture
            .seed_additional_tenant_with_uuid(
                tenant_b,
                &format!("b-{}", tenant_b.as_uuid().simple()),
            )
            .await
            .unwrap();

        let catalog = build_catalog(&h).await;
        let table_uid: TableUid = catalog
            .create_table(
                NS,
                TABLE,
                vec![Field::new("val", DataType::Int64, false)],
                TableScope::SystemShared,
                DataTenantId::SYSTEM_OWNER,
                &[],
                None,
            )
            .await
            .unwrap();
        drop(catalog);

        let table = load_table(&h, TABLE).await;
        let sql_catalog = load_sql_catalog(&h).await;

        // SAME batch_id for both tenants.
        let batch_id = *uuid::Uuid::now_v7().as_bytes();
        let groups = vec![
            CommitGroup {
                key: CommitKey::new(tenant_a, batch_id),
                ctx: ctx(batch_id),
                batches: vec![stamped(2, batch_id, Some(tenant_a))],
                audit: None,
            },
            CommitGroup {
                key: CommitKey::new(tenant_b, batch_id),
                ctx: ctx(batch_id),
                batches: vec![stamped(3, batch_id, Some(tenant_b))],
                audit: None,
            },
        ];

        let outcome = run_group_commit(
            &h.pool,
            &sql_catalog,
            &table,
            &table_uid,
            TableScope::SystemShared,
            groups,
        )
        .await
        .unwrap();
        assert_eq!(outcome.committed.len(), 2, "both keys committed");
        assert!(outcome.replayed.is_empty(), "nothing replayed on first run");

        // Two committed rows, same batch_id, different data_tenant_id.
        let rows: Vec<(String, uuid::Uuid, Vec<u8>)> = sqlx::query_as(
            "SELECT state, data_tenant_id, batch_id FROM vala.olap_commits \
             WHERE table_uid = $1 ORDER BY data_tenant_id",
        )
        .bind(table_uid.as_bytes().as_slice())
        .fetch_all(&*h.migrator)
        .await
        .unwrap();
        assert_eq!(rows.len(), 2, "two distinct commit rows");
        for (state, _, row_batch) in &rows {
            assert_eq!(state, "committed");
            assert_eq!(row_batch.as_slice(), batch_id.as_slice(), "same batch_id");
        }
        let mut tenants: Vec<uuid::Uuid> = rows.iter().map(|(_, t, _)| *t).collect();
        tenants.dedup();
        assert_eq!(tenants.len(), 2, "two distinct data_tenant_id values");

        // One snapshot; wyrd_commit_keys has both {data_tenant_id, batch_id} pairs
        // with distinct data_tenant_id (the M02 discriminator).
        let after = load_table(&h, TABLE).await;
        let keys_json = commit_keys_of_current(&after).expect("wyrd_commit_keys present");
        let parsed: serde_json::Value = serde_json::from_str(&keys_json).unwrap();
        let arr = parsed.as_array().unwrap();
        assert_eq!(arr.len(), 2, "two commit keys in the snapshot");
        let binds: std::collections::HashSet<String> = arr
            .iter()
            .map(|o| o["data_tenant_id"].as_str().unwrap().to_string())
            .collect();
        assert_eq!(binds.len(), 2, "distinct data_tenant_id per tenant");
        assert!(binds.contains(&tenant_a.as_uuid().simple().to_string()));
        assert!(binds.contains(&tenant_b.as_uuid().simple().to_string()));

        // Idempotent re-run: same two groups → both replay, no new snapshot.
        let before_replay = snapshot_count(&after);
        let groups2 = vec![
            CommitGroup {
                key: CommitKey::new(tenant_a, batch_id),
                ctx: ctx(batch_id),
                batches: vec![stamped(2, batch_id, Some(tenant_a))],
                audit: None,
            },
            CommitGroup {
                key: CommitKey::new(tenant_b, batch_id),
                ctx: ctx(batch_id),
                batches: vec![stamped(3, batch_id, Some(tenant_b))],
                audit: None,
            },
        ];
        let replay = run_group_commit(
            &h.pool,
            &sql_catalog,
            &after,
            &table_uid,
            TableScope::SystemShared,
            groups2,
        )
        .await
        .unwrap();
        assert_eq!(replay.replayed.len(), 2, "both keys replay");
        assert!(replay.committed.is_empty(), "nothing freshly committed");
        assert!(replay.snapshot_id.is_none(), "no append on full replay");
        let after2 = load_table(&h, TABLE).await;
        assert_eq!(
            snapshot_count(&after2),
            before_replay,
            "idempotent re-run adds no Iceberg snapshot"
        );
    }
}

/// Coordinator gates: drive the live [`spawn_group_commit_coordinator`] handle
/// (the actor behind `WyrdCatalog::writer`) through real Postgres + Iceberg and
/// assert the group-commit contract — one `fast_append` amortizes N tenants,
/// each write keeps its own `batch_id`, chunks split at `max_commit_keys`,
/// shutdown drains the buffer, and the reply is held until the covering commit.
#[cfg(test)]
mod coordinator {
    use std::sync::Arc;
    use std::time::Duration;

    use arrow::array::{Int64Array, RecordBatch};
    use arrow::datatypes::{DataType, Field, Schema};
    use iceberg_catalog_sql::SqlCatalog;
    use sqlx::PgPool;
    use tempfile::TempDir;
    use wyrd_dev_fixtures::pg::PgFixture;
    use wyrd_spec::ids::DataTenantId;
    use wyrd_storage::settings::BackendConfig;

    use crate::catalog::WyrdCatalog;
    use crate::catalog::iceberg_sql;
    use crate::catalog::namespaces::BifrostNamespace;
    use crate::registry::Registry;
    use crate::tables::PayloadClass;
    use crate::types::{TableScope, TableUid};
    use crate::writer::BifrostWriteContext;
    use crate::writer::coordinator::{
        FlushPolicy, GroupCommitHandle, spawn_group_commit_coordinator,
    };

    const NS: BifrostNamespace = BifrostNamespace::Bifrost;

    struct Harness {
        _tmp: TempDir,
        catalog_uri: String,
        backend: BackendConfig,
        pool: Arc<PgPool>,
        migrator: Arc<PgPool>,
        fixture: PgFixture,
    }

    async fn base_harness() -> Harness {
        let fixture = PgFixture::start().await.expect("fixture");
        let pool = Arc::new(fixture.app_pool().clone());
        let tmp = tempfile::tempdir().unwrap();
        let backend = BackendConfig::Local {
            root: tmp.path().to_path_buf(),
        };
        let catalog_uri = fixture.catalog_uri();
        let migrator = Arc::new(fixture.superuser_pool().await.expect("superuser pool"));
        Harness {
            _tmp: tmp,
            catalog_uri,
            backend,
            pool,
            migrator,
            fixture,
        }
    }

    async fn build_catalog(h: &Harness) -> WyrdCatalog {
        WyrdCatalog::new(&h.catalog_uri, &h.backend, h.pool.clone(), None)
            .await
            .unwrap()
    }

    async fn load_table(h: &Harness, table: &str) -> iceberg::table::Table {
        use iceberg::Catalog as _;
        let (factory, props) = crate::catalog::storage::iceberg_storage_factory(&h.backend);
        let warehouse = crate::catalog::storage::warehouse_uri(&h.backend);
        let catalog = iceberg_sql::build_catalog(&h.catalog_uri, &warehouse, factory, props)
            .await
            .unwrap();
        let ident = iceberg::TableIdent::new(NS.to_namespace_ident(), table.to_string());
        catalog.load_table(&ident).await.unwrap()
    }

    async fn load_sql_catalog(h: &Harness) -> SqlCatalog {
        let (factory, props) = crate::catalog::storage::iceberg_storage_factory(&h.backend);
        let warehouse = crate::catalog::storage::warehouse_uri(&h.backend);
        iceberg_sql::build_catalog(&h.catalog_uri, &warehouse, factory, props)
            .await
            .unwrap()
    }

    fn user_batch(n: i64) -> RecordBatch {
        let schema = Arc::new(Schema::new(vec![Field::new("val", DataType::Int64, false)]));
        RecordBatch::try_new(
            schema,
            vec![Arc::new(Int64Array::from((0..n).collect::<Vec<_>>()))],
        )
        .unwrap()
    }

    fn ctx(batch_id: [u8; 16]) -> BifrostWriteContext {
        let mut c = BifrostWriteContext::system();
        c.batch_id = batch_id;
        c
    }

    fn snapshot_count(table: &iceberg::table::Table) -> usize {
        table.metadata().snapshots().count()
    }

    /// A no-auto-flush policy: only a dropped handle (shutdown) or an explicit
    /// timer/threshold override flushes. `max_commit_keys` overridable per test.
    fn manual_policy(max_commit_keys: usize) -> FlushPolicy {
        FlushPolicy {
            max_rows: usize::MAX,
            max_interval: Duration::from_hours(1),
            max_buffered_bytes: usize::MAX,
            max_commit_keys,
        }
    }

    async fn seed(h: &Harness, tenant: DataTenantId) {
        h.fixture
            .seed_additional_tenant_with_uuid(tenant, &format!("t-{}", tenant.as_uuid().simple()))
            .await
            .unwrap();
    }

    async fn register_table(
        h: &Harness,
        table: &str,
        scope: TableScope,
        owner: DataTenantId,
    ) -> TableUid {
        let catalog = build_catalog(h).await;
        catalog
            .create_table(
                NS,
                table,
                vec![Field::new("val", DataType::Int64, false)],
                scope,
                owner,
                &[],
                None,
            )
            .await
            .unwrap()
    }

    fn spawn(
        h: &Harness,
        table: iceberg::table::Table,
        sql_catalog: SqlCatalog,
        table_uid: TableUid,
        fqn: &str,
        scope: TableScope,
        policy: FlushPolicy,
    ) -> GroupCommitHandle {
        spawn_group_commit_coordinator(
            table,
            Arc::new(sql_catalog),
            h.pool.clone(),
            table_uid,
            fqn.to_string(),
            scope,
            Arc::new(Registry::new(h.pool.clone())),
            PayloadClass::Standard,
            &[],
            policy,
        )
    }

    #[derive(sqlx::FromRow)]
    struct CommitRow {
        state: String,
        data_tenant_id: uuid::Uuid,
        batch_id: Vec<u8>,
        snapshot_id: Option<i64>,
    }

    async fn commit_rows(h: &Harness, table_uid: TableUid) -> Vec<CommitRow> {
        sqlx::query_as::<_, CommitRow>(
            "SELECT state, data_tenant_id, batch_id, snapshot_id \
             FROM vala.olap_commits WHERE table_uid = $1 ORDER BY data_tenant_id, batch_id",
        )
        .bind(table_uid.as_bytes().as_slice())
        .fetch_all(&*h.migrator)
        .await
        .unwrap()
    }

    /// N tenants writing one physical (`SystemShared`) table, flushed together,
    /// amortize into exactly ONE Iceberg snapshot — not N. The row-count threshold
    /// fires the single covering flush; every tenant's reply carries that one
    /// snapshot id.
    #[tokio::test]
    async fn amortization_across_tenants() {
        const TABLE: &str = "coord_amort";
        let h = base_harness().await;
        let tenants = [
            DataTenantId::new_v7(),
            DataTenantId::new_v7(),
            DataTenantId::new_v7(),
        ];
        for t in &tenants {
            seed(&h, *t).await;
        }
        let table_uid = register_table(
            &h,
            TABLE,
            TableScope::SystemShared,
            DataTenantId::SYSTEM_OWNER,
        )
        .await;

        let table = load_table(&h, TABLE).await;
        let before = snapshot_count(&table);
        let sql_catalog = load_sql_catalog(&h).await;
        // Flush exactly when all three one-row writes are buffered.
        let policy = FlushPolicy {
            max_rows: 3,
            ..manual_policy(128)
        };
        let handle = spawn(
            &h,
            table,
            sql_catalog,
            table_uid,
            TABLE,
            TableScope::SystemShared,
            policy,
        );

        let mut rxs = Vec::new();
        for t in &tenants {
            let batch_id = *uuid::Uuid::now_v7().as_bytes();
            let rx = handle
                .send_write(*t, vec![user_batch(1)], ctx(batch_id))
                .expect("enqueued");
            rxs.push(rx);
        }
        let mut snaps = Vec::new();
        for rx in rxs {
            snaps.push(rx.await.expect("actor replied").expect("commit ok"));
        }
        // Every tenant acked the SAME single snapshot.
        assert!(
            snaps.iter().all(|s| *s == snaps[0]),
            "all tenants share one snapshot"
        );

        let after = load_table(&h, TABLE).await;
        assert_eq!(
            snapshot_count(&after),
            before + 1,
            "N tenants amortize into exactly ONE fast_append"
        );
        let rows = commit_rows(&h, table_uid).await;
        assert_eq!(rows.len(), 3, "one commit row per tenant");
        assert!(rows.iter().all(|r| r.state == "committed"));
        assert!(
            rows.iter().all(|r| r.snapshot_id == Some(snaps[0])),
            "all rows share the group snapshot"
        );
        let distinct: std::collections::HashSet<uuid::Uuid> =
            rows.iter().map(|r| r.data_tenant_id).collect();
        assert_eq!(distinct.len(), 3, "each row bound to its own data tenant");
        drop(handle);
    }

    /// A `SystemShared` flush covering two tenants stamps each tenant's commit row
    /// with ITS OWN `data_tenant_id` — the writes are not collapsed onto one bind.
    #[tokio::test]
    async fn per_row_tenant_stamp() {
        const TABLE: &str = "coord_stamp";
        let h = base_harness().await;
        let ta = DataTenantId::new_v7();
        let tb = DataTenantId::new_v7();
        seed(&h, ta).await;
        seed(&h, tb).await;
        let table_uid = register_table(
            &h,
            TABLE,
            TableScope::SystemShared,
            DataTenantId::SYSTEM_OWNER,
        )
        .await;

        let table = load_table(&h, TABLE).await;
        let sql_catalog = load_sql_catalog(&h).await;
        let handle = spawn(
            &h,
            table,
            sql_catalog,
            table_uid,
            TABLE,
            TableScope::SystemShared,
            manual_policy(128),
        );

        let rx_a = handle
            .send_write(
                ta,
                vec![user_batch(2)],
                ctx(*uuid::Uuid::now_v7().as_bytes()),
            )
            .expect("enqueue a");
        let rx_b = handle
            .send_write(
                tb,
                vec![user_batch(2)],
                ctx(*uuid::Uuid::now_v7().as_bytes()),
            )
            .expect("enqueue b");
        drop(handle); // shutdown → drain → one covering commit
        rx_a.await.expect("reply a").expect("commit a");
        rx_b.await.expect("reply b").expect("commit b");

        let rows = commit_rows(&h, table_uid).await;
        assert_eq!(rows.len(), 2, "two commit rows");
        let binds: std::collections::HashSet<uuid::Uuid> =
            rows.iter().map(|r| r.data_tenant_id).collect();
        assert!(
            binds.contains(&ta.as_uuid()),
            "tenant a stamped on its own row"
        );
        assert!(
            binds.contains(&tb.as_uuid()),
            "tenant b stamped on its own row"
        );
        assert_eq!(binds.len(), 2, "tenants not collapsed onto one bind");
    }

    /// Dropping the sole handle (shutdown) with a non-empty buffer drains and
    /// commits the buffered writes — no data is lost — and each reply resolves.
    #[tokio::test]
    async fn drain_on_shutdown_commits_buffer() {
        const TABLE: &str = "coord_drain";
        let h = base_harness().await;
        let tenant = DataTenantId::new_v7();
        seed(&h, tenant).await;
        let table_uid = register_table(&h, TABLE, TableScope::TenantOwned, tenant).await;

        let table = load_table(&h, TABLE).await;
        let before = snapshot_count(&table);
        let sql_catalog = load_sql_catalog(&h).await;
        let handle = spawn(
            &h,
            table,
            sql_catalog,
            table_uid,
            TABLE,
            TableScope::TenantOwned,
            manual_policy(128),
        );

        let rx1 = handle
            .send_write(
                tenant,
                vec![user_batch(1)],
                ctx(*uuid::Uuid::now_v7().as_bytes()),
            )
            .expect("enqueue 1");
        let rx2 = handle
            .send_write(
                tenant,
                vec![user_batch(1)],
                ctx(*uuid::Uuid::now_v7().as_bytes()),
            )
            .expect("enqueue 2");
        // No threshold/timer would ever fire — only shutdown drains.
        drop(handle);
        let s1 = rx1.await.expect("reply 1").expect("commit 1");
        let s2 = rx2.await.expect("reply 2").expect("commit 2");
        assert_eq!(s1, s2, "both buffered writes rode one covering commit");

        let after = load_table(&h, TABLE).await;
        assert_eq!(
            snapshot_count(&after),
            before + 1,
            "buffer committed on shutdown"
        );
        let rows = commit_rows(&h, table_uid).await;
        assert_eq!(rows.len(), 2, "both buffered writes are durable");
        assert!(rows.iter().all(|r| r.state == "committed"));
    }

    /// A single flush of more than `max_commit_keys` write units splits into
    /// multiple group commits — one Iceberg snapshot per chunk — instead of one
    /// unbounded `fast_append`.
    #[tokio::test]
    async fn flush_chunks_at_max_commit_keys() {
        const TABLE: &str = "coord_chunk";
        let h = base_harness().await;
        let tenant = DataTenantId::new_v7();
        seed(&h, tenant).await;
        let table_uid = register_table(&h, TABLE, TableScope::TenantOwned, tenant).await;

        let table = load_table(&h, TABLE).await;
        let before = snapshot_count(&table);
        let sql_catalog = load_sql_catalog(&h).await;
        // 5 keys, chunk size 2 → ceil(5/2) = 3 group commits.
        let handle = spawn(
            &h,
            table,
            sql_catalog,
            table_uid,
            TABLE,
            TableScope::TenantOwned,
            manual_policy(2),
        );

        let mut rxs = Vec::new();
        for _ in 0..5 {
            let rx = handle
                .send_write(
                    tenant,
                    vec![user_batch(1)],
                    ctx(*uuid::Uuid::now_v7().as_bytes()),
                )
                .expect("enqueue");
            rxs.push(rx);
        }
        drop(handle);
        for rx in rxs {
            rx.await.expect("reply").expect("commit");
        }

        let after = load_table(&h, TABLE).await;
        assert_eq!(
            snapshot_count(&after),
            before + 3,
            "5 keys at chunk size 2 produce 3 snapshots"
        );
        let rows = commit_rows(&h, table_uid).await;
        assert_eq!(rows.len(), 5, "all five keys committed");
        assert!(rows.iter().all(|r| r.state == "committed"));
    }

    /// A timer flush commits each buffered write as its OWN key: three writes with
    /// three distinct `batch_id`s produce three distinct commit rows — the writes
    /// are NOT collapsed under one `batch_id`.
    #[tokio::test]
    async fn timer_flush_preserves_per_batch_context() {
        const TABLE: &str = "coord_timer";
        let h = base_harness().await;
        let tenant = DataTenantId::new_v7();
        seed(&h, tenant).await;
        let table_uid = register_table(&h, TABLE, TableScope::TenantOwned, tenant).await;

        let table = load_table(&h, TABLE).await;
        let sql_catalog = load_sql_catalog(&h).await;
        // Short timer fires the flush; the await below waits for it.
        let policy = FlushPolicy {
            max_interval: Duration::from_millis(150),
            ..manual_policy(128)
        };
        let handle = spawn(
            &h,
            table,
            sql_catalog,
            table_uid,
            TABLE,
            TableScope::TenantOwned,
            policy,
        );

        let mut batch_ids = Vec::new();
        let mut rxs = Vec::new();
        for _ in 0..3 {
            let batch_id = *uuid::Uuid::now_v7().as_bytes();
            batch_ids.push(batch_id);
            rxs.push(
                handle
                    .send_write(tenant, vec![user_batch(1)], ctx(batch_id))
                    .expect("enqueue"),
            );
        }
        let mut snaps = Vec::new();
        for rx in rxs {
            snaps.push(rx.await.expect("timer reply").expect("commit"));
        }
        assert!(
            snaps.iter().all(|s| *s == snaps[0]),
            "one timer flush, one snapshot"
        );

        let rows = commit_rows(&h, table_uid).await;
        assert_eq!(
            rows.len(),
            3,
            "each write kept its own batch_id → three rows"
        );
        let distinct: std::collections::HashSet<Vec<u8>> =
            rows.iter().map(|r| r.batch_id.clone()).collect();
        assert_eq!(
            distinct.len(),
            3,
            "three distinct batch_ids, not collapsed to one"
        );
        for batch_id in &batch_ids {
            assert!(
                distinct.contains(batch_id.as_slice()),
                "batch_id {} present as its own row",
                uuid::Uuid::from_bytes(*batch_id)
            );
        }
        drop(handle);
    }

    /// The reply is held until the covering commit: before any flush the receiver
    /// has no value AND no `olap_commits` row exists; only after shutdown-drain
    /// does the reply resolve with a durable snapshot.
    #[tokio::test]
    async fn reply_held_until_covering_commit() {
        const TABLE: &str = "coord_hold";
        let h = base_harness().await;
        let tenant = DataTenantId::new_v7();
        seed(&h, tenant).await;
        let table_uid = register_table(&h, TABLE, TableScope::TenantOwned, tenant).await;

        let table = load_table(&h, TABLE).await;
        let sql_catalog = load_sql_catalog(&h).await;
        let handle = spawn(
            &h,
            table,
            sql_catalog,
            table_uid,
            TABLE,
            TableScope::TenantOwned,
            manual_policy(128),
        );

        let mut rx = handle
            .send_write(
                tenant,
                vec![user_batch(1)],
                ctx(*uuid::Uuid::now_v7().as_bytes()),
            )
            .expect("enqueue");
        // Give the actor a moment to process the buffered write (no flush trigger).
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert!(rx.try_recv().is_err(), "reply is held before any commit");
        assert!(
            commit_rows(&h, table_uid).await.is_empty(),
            "no precommit/commit row before a flush"
        );

        drop(handle); // shutdown → drain → commit
        let sid = rx.await.expect("reply after commit").expect("commit ok");
        assert!(sid > 0, "reply carries the covering snapshot id");
        let rows = commit_rows(&h, table_uid).await;
        assert_eq!(rows.len(), 1, "the held write is now durable");
        assert_eq!(rows[0].state, "committed");
    }
}
