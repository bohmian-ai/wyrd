// Wrapped in `mod pg_tests` (PgFixture-backed) so the fast family lane skips it
// via `--skip pg_tests`; `mise run test:sql` selects it by `--test` and runs it
// with Postgres up.
mod pg_tests {
    use std::time::Duration;

    use tokio_util::sync::CancellationToken;
    use wyrd_dev_fixtures::pg::PgFixture;
    use wyrd_storage::settings::{BackendConfig, StorageSettings};
    use wyrd_storage::sweeper::{SWEEPER_LEADER_LOCK_KEY, Sweeper, SweeperConfig};
    use wyrd_storage::{StorageHandle, tenant_path};

    #[tokio::test]
    async fn tick_sweeps_expired_uploads_and_reaps_idempotency_rows() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let operator_pool = fixture.operator_pool().clone();
        let tenant = fixture.data_tenant_id();
        let handle = local_storage_handle().await;
        let shutdown_rx = CancellationToken::new();
        let sweeper = Sweeper::new(
            handle,
            operator_pool.clone(),
            fixture.wyrd_postgres().clone(),
            SweeperConfig {
                enabled: true,
                tick: Duration::from_mins(1),
                batch_size: 10,
                init_grace: Duration::from_secs(30),
                idempotency_batch_size: 10,
            },
            shutdown_rx,
        );

        let blocked_upload = insert_upload(
            fixture.app_pool(),
            card(tenant, "blocked.bin"),
            "pending",
            -3600,
            -60,
        )
        .await;
        let mut leader_conn = operator_pool.pool().acquire().await.expect("leader conn");
        let acquired =
            wyrd_sql::queries::storage::admin::multipart_uploads::try_acquire_leader_lock(
                leader_conn.as_mut(),
                SWEEPER_LEADER_LOCK_KEY,
            )
            .await
            .expect("leader lock");
        assert!(acquired);
        leader_conn.close_on_drop();

        sweeper.tick().await.expect("non-leader tick skips");
        assert_upload_status(operator_pool.pool(), blocked_upload, "pending").await;
        drop(leader_conn);

        let expired_pending = insert_upload(
            fixture.app_pool(),
            card(tenant, "expired.bin"),
            "pending",
            -3600,
            -60,
        )
        .await;
        let orphan_initiating = insert_upload(
            fixture.app_pool(),
            card(tenant, "orphan.bin"),
            "initiating",
            3600,
            -300,
        )
        .await;
        let live_pending =
            insert_upload(fixture.app_pool(), card(tenant, "live.bin"), "pending", 3600, -60).await;
        insert_idempotency_key(fixture.app_pool(), tenant, "expired-key", -3600).await;
        insert_idempotency_key(fixture.app_pool(), tenant, "live-key", 3600).await;

        sweeper.tick().await.expect("leader tick sweeps");

        assert_upload_status(operator_pool.pool(), blocked_upload, "aborted").await;
        assert_upload_status(operator_pool.pool(), expired_pending, "aborted").await;
        assert_upload_status(operator_pool.pool(), orphan_initiating, "aborted").await;
        assert_upload_status(operator_pool.pool(), live_pending, "pending").await;
        assert_idempotency_count(operator_pool.pool(), "expired-key", 0).await;
        assert_idempotency_count(operator_pool.pool(), "live-key", 1).await;
    }

    /// A pending upload completed before the sweep is never reclaimed.
    ///
    /// The sweeper claims expired pending rows by status, so a row that turned
    /// `completed` before the tick must keep that status.
    ///
    /// # Panics
    /// Panics when the fixture cannot start or the completed upload is reclaimed.
    #[tokio::test]
    async fn sweeper_skips_reclamation_when_upload_already_completed() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let operator_pool = fixture.operator_pool().clone();
        let tenant = fixture.data_tenant_id();
        let handle = local_storage_handle().await;
        let shutdown_rx = CancellationToken::new();
        let sweeper = Sweeper::new(
            handle,
            operator_pool.clone(),
            fixture.wyrd_postgres().clone(),
            SweeperConfig {
                enabled: true,
                tick: Duration::from_mins(1),
                batch_size: 10,
                init_grace: Duration::from_secs(30),
                idempotency_batch_size: 10,
            },
            shutdown_rx,
        );

        let upload_id = insert_upload(
            fixture.app_pool(),
            card(tenant, "completed.bin"),
            "pending",
            -3600,
            -60,
        )
        .await;

        sqlx::query("UPDATE wyrd.storage_multipart_uploads SET status = 'completed' WHERE id = $1")
            .bind(upload_id)
            .execute(operator_pool.pool())
            .await
            .expect("force-complete upload");

        sweeper.tick().await.expect("tick runs");

        assert_upload_status(operator_pool.pool(), upload_id, "completed").await;
    }

    /// Abandoned File sessions are reclaimed by the shared sweeper beside
    /// Card uploads: with a batch of one, an older expired File session, then
    /// an orphaned initiating File session, then an expired Card upload are
    /// each aborted by successive ticks, and the File plans again afterwards.
    /// A row whose path disagrees with its persisted owner or tenant is never
    /// reclaimed.
    ///
    /// # Panics
    /// Panics when the fixture cannot start, a tick or replan fails, or a row
    /// ends in another status.
    #[tokio::test]
    async fn tick_reclaims_file_sessions_without_starving_card_uploads() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let operator_pool = fixture.operator_pool().clone();
        let tenant = fixture.data_tenant_id();
        let handle = local_storage_handle().await;
        let sweeper = |batch_size| {
            Sweeper::new(
                handle.clone(),
                operator_pool.clone(),
                fixture.wyrd_postgres().clone(),
                SweeperConfig {
                    enabled: true,
                    tick: Duration::from_mins(1),
                    batch_size,
                    init_grace: Duration::from_secs(30),
                    idempotency_batch_size: 10,
                },
                CancellationToken::new(),
            )
        };

        let pool = fixture.app_pool();
        let expired_file = insert_upload(pool, file(tenant, b"doc"), "pending", -7200, -7260).await;
        let orphan_file = insert_upload(pool, file(tenant, b"doc"), "initiating", 3600, -600).await;
        let expired_card = insert_upload(pool, card(tenant, "card.bin"), "pending", -60, -120).await;
        let bounded = sweeper(1);
        for _ in 0..3 {
            bounded.tick().await.expect("bounded tick sweeps");
        }
        for upload in [expired_file, orphan_file, expired_card] {
            assert_upload_status(operator_pool.pool(), upload, "aborted").await;
        }
        let caller = wyrd_storage::service::StorageCaller {
            data_tenant_id: tenant,
        };
        wyrd_storage::service::file_upload_init(
            &handle,
            fixture.wyrd_postgres(),
            &caller,
            &wyrd_spec::auth::Sha256Hex::digest(b"doc"),
            3,
        )
        .await
        .expect("the reclaimed File plans again");

        let (_, card_uid, relative_path, card_path) = card(tenant, "owned.bin");
        let (_, _, file_relative, file_path) = file(tenant, b"owned");
        let (_, _, foreign_relative, foreign_path) =
            file(wyrd_spec::DataTenantId::new_v7(), b"foreign");
        let refused = [
            (tenant, None, relative_path, card_path),
            (tenant, card_uid, file_relative, file_path),
            (tenant, None, foreign_relative, foreign_path),
        ];
        let mut refused_ids = Vec::new();
        for owner in refused {
            refused_ids.push(insert_upload(pool, owner, "pending", -60, -120).await);
        }
        sweeper(10).tick().await.expect("tick runs");
        for upload in refused_ids {
            assert_upload_status(operator_pool.pool(), upload, "pending").await;
        }
    }

    async fn local_storage_handle() -> std::sync::Arc<StorageHandle> {
        let root = tempfile::tempdir().expect("temp dir").keep();
        StorageHandle::from_settings(StorageSettings {
            backend: BackendConfig::Local { root },
            require_encryption: false,
            presign_ttl: Duration::from_mins(10),
            part_size_bytes: 16 * 1024 * 1024,
            multipart_threshold_bytes: 100 * 1024 * 1024,
        })
        .await
        .expect("storage handle")
    }

    /// The owner of one inserted upload row: its tenant, nullable owning
    /// Card, relative path, and full storage path.
    type Owner = (wyrd_spec::DataTenantId, Option<String>, String, String);

    /// A Card file upload of `relative_path` owned by a fresh Card.
    fn card(tenant: wyrd_spec::DataTenantId, relative_path: &str) -> Owner {
        let card_uid = sqlx::types::Uuid::now_v7().to_string();
        let storage_path = tenant_path::build(tenant, &card_uid, relative_path);
        (tenant, Some(card_uid), relative_path.to_owned(), storage_path)
    }

    /// A verification File upload session of `bytes`, owned by no Card.
    fn file(tenant: wyrd_spec::DataTenantId, bytes: &[u8]) -> Owner {
        let path = tenant_path::evidence(
            tenant,
            &wyrd_spec::auth::Sha256Hex::digest(bytes),
            &wyrd_spec::storage::UploadId::new(),
        );
        (tenant, None, path.relative_path, path.full)
    }

    /// Insert one local upload row of `owner` with `status`, expiring and
    /// created at the given offsets from now.
    ///
    /// # Panics
    /// Panics when the tenant connection, insert, or commit fails.
    async fn insert_upload(
        app_pool: &sqlx::PgPool,
        (tenant, card_uid, relative_path, storage_path): Owner,
        status: &str,
        expires_offset_secs: i64,
        created_offset_secs: i64,
    ) -> sqlx::types::Uuid {
        let mut conn = wyrd_sql::TenantConn::acquire(app_pool, tenant)
            .await
            .expect("tenant conn");
        let upload_id = sqlx::types::Uuid::now_v7();

        sqlx::query(
            r"
        INSERT INTO wyrd.storage_multipart_uploads (
            id,
            data_tenant_id,
            card_uid,
            relative_path,
            storage_path,
            backend,
            wire_protocol,
            expected_sha256,
            expected_size_bytes,
            part_count_planned,
            part_size_bytes,
            status,
            created_at,
            expires_at
        )
        VALUES (
            $1,
            wyrd.current_tenant(),
            $2,
            $3,
            $4,
            'local',
            'local_fs_v1',
            'AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=',
            0,
            1,
            0,
            $5,
            now() + make_interval(secs => $6),
            now() + make_interval(secs => $7)
        )
        ",
        )
        .bind(upload_id)
        .bind(card_uid)
        .bind(relative_path)
        .bind(storage_path)
        .bind(status)
        .bind(created_offset_secs)
        .bind(expires_offset_secs)
        .execute(&mut **conn.transaction())
        .await
        .expect("insert upload");
        conn.commit().await.expect("commit upload insert");

        upload_id
    }

    async fn insert_idempotency_key(
        app_pool: &sqlx::PgPool,
        tenant: wyrd_spec::DataTenantId,
        key: &str,
        expires_offset_secs: i64,
    ) {
        let mut conn = wyrd_sql::TenantConn::acquire(app_pool, tenant)
            .await
            .expect("tenant conn");
        sqlx::query(
            r"
        INSERT INTO wyrd.storage_idempotency_keys (
            data_tenant_id,
            idempotency_key,
            body_sha256,
            response_status,
            response_body,
            expires_at
        )
        VALUES (wyrd.current_tenant(), $1, $2, 200, $3::jsonb, now() + make_interval(secs => $4))
        ",
        )
        .bind(key)
        .bind(vec![0_u8; 32])
        .bind(serde_json::json!({
            "upload_id": "wyu_01890f28-7c4a-7cc3-98e7-4f4a3c2d1b00",
            "storage_path": "tenant/cards/card/file.bin",
            "backend": "local",
            "wire_protocol": "local_fs_v1"
        }))
        .bind(expires_offset_secs)
        .execute(&mut **conn.transaction())
        .await
        .expect("insert idempotency key");
        conn.commit().await.expect("commit idempotency insert");
    }

    async fn assert_upload_status(
        admin_pool: &sqlx::PgPool,
        id: sqlx::types::Uuid,
        expected: &str,
    ) {
        let status = sqlx::query_scalar::<_, String>(
            "SELECT status FROM wyrd.storage_multipart_uploads WHERE id = $1",
        )
        .bind(id)
        .fetch_one(admin_pool)
        .await
        .expect("fetch upload status");

        assert_eq!(status, expected);
    }

    async fn assert_idempotency_count(admin_pool: &sqlx::PgPool, key: &str, expected: i64) {
        let count = sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM wyrd.storage_idempotency_keys WHERE idempotency_key = $1",
        )
        .bind(key)
        .fetch_one(admin_pool)
        .await
        .expect("fetch idempotency count");

        assert_eq!(count, expected);
    }
}
