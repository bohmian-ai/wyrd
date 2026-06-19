use std::time::Duration;

use tokio_util::sync::CancellationToken;
use wyrd_dev_fixtures::pg::PgFixture;
use wyrd_storage::settings::{BackendConfig, StorageSettings};
use wyrd_storage::sweeper::{SWEEPER_LEADER_LOCK_KEY, Sweeper, SweeperConfig};
use wyrd_storage::{StorageHandle, tenant_path};

#[tokio::test]
async fn tick_sweeps_expired_uploads_and_reaps_idempotency_rows() {
    let fixture = PgFixture::start().await.expect("fixture starts");
    let admin_pool = fixture.platform_admin_pool().clone();
    let tenant = fixture.data_tenant_id();
    let handle = local_storage_handle().await;
    let shutdown_rx = CancellationToken::new();
    let sweeper = Sweeper::new(
        handle,
        admin_pool.clone(),
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
        tenant,
        "blocked.bin",
        "pending",
        -3600,
        -60,
    )
    .await;
    let mut leader_conn = admin_pool.acquire().await.expect("leader conn");
    let acquired = wyrd_sql::queries::storage::admin::multipart_uploads::try_acquire_leader_lock(
        leader_conn.as_mut(),
        SWEEPER_LEADER_LOCK_KEY,
    )
    .await
    .expect("leader lock");
    assert!(acquired);
    leader_conn.close_on_drop();

    sweeper.tick().await.expect("non-leader tick skips");
    assert_upload_status(&admin_pool, blocked_upload, "pending").await;
    drop(leader_conn);

    let expired_pending = insert_upload(
        fixture.app_pool(),
        tenant,
        "expired.bin",
        "pending",
        -3600,
        -60,
    )
    .await;
    let orphan_initiating = insert_upload(
        fixture.app_pool(),
        tenant,
        "orphan.bin",
        "initiating",
        3600,
        -300,
    )
    .await;
    let live_pending =
        insert_upload(fixture.app_pool(), tenant, "live.bin", "pending", 3600, -60).await;
    insert_idempotency_key(fixture.app_pool(), tenant, "expired-key", -3600).await;
    insert_idempotency_key(fixture.app_pool(), tenant, "live-key", 3600).await;

    sweeper.tick().await.expect("leader tick sweeps");

    assert_upload_status(&admin_pool, blocked_upload, "aborted").await;
    assert_upload_status(&admin_pool, expired_pending, "aborted").await;
    assert_upload_status(&admin_pool, orphan_initiating, "aborted").await;
    assert_upload_status(&admin_pool, live_pending, "pending").await;
    assert_audit_count(&admin_pool, tenant, 3).await;
    assert_idempotency_count(&admin_pool, "expired-key", 0).await;
    assert_idempotency_count(&admin_pool, "live-key", 1).await;
}

#[tokio::test]
async fn sweeper_skips_audit_when_upload_already_completed() {
    let fixture = PgFixture::start().await.expect("fixture starts");
    let admin_pool = fixture.platform_admin_pool().clone();
    let tenant = fixture.data_tenant_id();
    let handle = local_storage_handle().await;
    let shutdown_rx = CancellationToken::new();
    let sweeper = Sweeper::new(
        handle,
        admin_pool.clone(),
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
        tenant,
        "completed.bin",
        "pending",
        -3600,
        -60,
    )
    .await;

    sqlx::query("UPDATE wyrd.storage_multipart_uploads SET status = 'completed' WHERE id = $1")
        .bind(upload_id)
        .execute(&admin_pool)
        .await
        .expect("force-complete upload");

    sweeper.tick().await.expect("tick runs");

    assert_audit_count(&admin_pool, tenant, 0).await;
}

async fn local_storage_handle() -> std::sync::Arc<StorageHandle> {
    let root = tempfile::tempdir().expect("temp dir").keep();
    StorageHandle::from_settings(StorageSettings {
        backend: BackendConfig::Local { root },
        require_encryption: false,
        presign_ttl: Duration::from_mins(10),
        part_size_bytes: 16 * 1024 * 1024,
        public_base_url: Some("https://wyrd.test".to_owned()),
    })
    .await
    .expect("storage handle")
}

async fn insert_upload(
    app_pool: &sqlx::PgPool,
    tenant: wyrd_spec::DataTenantId,
    relative_path: &str,
    status: &str,
    expires_offset_secs: i64,
    created_offset_secs: i64,
) -> sqlx::types::Uuid {
    let mut conn = wyrd_sql::TenantConn::acquire(app_pool, tenant)
        .await
        .expect("tenant conn");
    let upload_id = sqlx::types::Uuid::now_v7();
    let card_uid = sqlx::types::Uuid::now_v7().to_string();
    let storage_path = tenant_path::build(tenant, &card_uid, relative_path);

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

async fn assert_upload_status(admin_pool: &sqlx::PgPool, id: sqlx::types::Uuid, expected: &str) {
    let status = sqlx::query_scalar::<_, String>(
        "SELECT status FROM wyrd.storage_multipart_uploads WHERE id = $1",
    )
    .bind(id)
    .fetch_one(admin_pool)
    .await
    .expect("fetch upload status");

    assert_eq!(status, expected);
}

async fn assert_audit_count(
    admin_pool: &sqlx::PgPool,
    tenant: wyrd_spec::DataTenantId,
    expected: i64,
) {
    let count = sqlx::query_scalar::<_, i64>(
        r"
        SELECT count(*)
        FROM wyrd.storage_access_ledger
        WHERE data_tenant_id = $1
          AND subject_id = 'storage-sweeper'
          AND operation = 'sweeper_abort'
        ",
    )
    .bind(tenant.as_uuid())
    .fetch_one(admin_pool)
    .await
    .expect("fetch audit count");

    assert_eq!(count, expected);
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
