//! Storage HTTP e2e integration tests.
//!
//! Drives the full request chain an SDK client would walk: upload-init →
//! byte PUT → complete → download-init → byte GET → compare. Env-gated:
//!
//! ```
//! WYRD_STORAGE_E2E=1 \
//! WYRD_DATABASE_URL_MIGRATOR=postgres://wyrd_migrator:<pw>@localhost/wyrd \
//! WYRD_DATABASE_URL=postgres://wyrd_app:<pw>@localhost/wyrd \
//! cargo test -p wyrd-server --all-features --test storage_e2e -- --nocapture
//! ```
//!
//! Requires a running Postgres with applied migrations (see `mise run
//! storage:up` + `mise run check:db` for local setup).

use axum::body::to_bytes;
use axum::http::{Request, StatusCode};
use base64::Engine;
use chrono::Duration as ChronoDuration;
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tower::ServiceExt;
use wyrd_auth_issue::IssuingKey;
use wyrd_auth_verify::{
    Kid, PrincipalKindWire, TokenPrincipalRef, TokenVerifier, WyrdAuthVerifySettings,
    public_key_from_pem,
};
use wyrd_runtime::{PrincipalId, RoleRef};
use wyrd_server::auth::permission_resolver::SqlPermissionResolver;
use wyrd_server::auth::seed::seed_builtin_roles_for_tenant;
use wyrd_server::{AppState, build_router};
use wyrd_spec::DataTenantId;
use wyrd_spec::ids::CardUid;
use wyrd_spec::storage::{
    AbortResponse, DownloadInitRequest, SinglePutComplete, UploadCompleteRequest,
    UploadInitRequest, UploadPlan,
};
use wyrd_sql::SqlStore;
use wyrd_sql::TenantConn;
use wyrd_storage::{BackendConfig, StorageHandle, StorageSettings};

const FIXED_CARD_UID: &str = "018f0000-0000-7000-8000-000000000001";
const PRIVATE_KEY_PEM: &str = "-----BEGIN PRIVATE KEY-----\nMC4CAQAwBQYDK2VwBCIEID78cHNjuFihX8aWPytQRoR2iUKHVXgdh92bcTcjQTYV\n-----END PRIVATE KEY-----\n";
const PUBLIC_KEY_PEM: &[u8] = b"-----BEGIN PUBLIC KEY-----\nMCowBQYDK2VwAyEAWhCX9H41EwSjJJI1E6X3z5fTKyCZ3v2DsJluJ+DZ8Vw=\n-----END PUBLIC KEY-----\n";

fn skip_unless_e2e() -> bool {
    if std::env::var("WYRD_STORAGE_E2E").as_deref() != Ok("1") {
        eprintln!("skipping storage e2e tests; set WYRD_STORAGE_E2E=1");
        return true;
    }
    false
}

fn migrator_url() -> Option<String> {
    std::env::var("WYRD_DATABASE_URL_MIGRATOR").ok()
}

fn app_url() -> Option<String> {
    std::env::var("WYRD_DATABASE_URL").ok()
}

fn sha256_b64(bytes: &[u8]) -> String {
    base64::engine::general_purpose::STANDARD.encode(Sha256::digest(bytes))
}

async fn local_app_state(root: &std::path::Path) -> AppState {
    let migrator_url = migrator_url().expect("WYRD_DATABASE_URL_MIGRATOR must be set");
    let app_url = app_url().unwrap_or_else(|| migrator_url.clone());

    let app_pool = SqlStore::connect(&app_url, 5)
        .await
        .expect("app pool connects")
        .pool()
        .clone();

    let storage = StorageHandle::from_settings(StorageSettings {
        backend: BackendConfig::Local {
            root: root.to_path_buf(),
        },
        require_encryption: false,
        presign_ttl: Duration::from_secs(600),
        part_size_bytes: 16 * 1024 * 1024,
        public_base_url: Some("https://wyrd.test".to_owned()),
    })
    .await
    .expect("local storage handle");

    let issuing_key = Arc::new(
        IssuingKey::from_ed_pem(
            secrecy::SecretString::from(PRIVATE_KEY_PEM),
            Kid::new("k1").expect("kid is valid"),
            "wyrd",
        )
        .expect("test issuing key loads"),
    );
    let mut keys = HashMap::new();
    keys.insert(
        Kid::new("k1").expect("kid is valid"),
        Arc::new(public_key_from_pem(PUBLIC_KEY_PEM).expect("public key loads")),
    );
    let verifier = Arc::new(TokenVerifier::new(
        keys,
        "wyrd",
        Arc::new(SqlPermissionResolver::new(Arc::new(app_pool.clone()))),
        WyrdAuthVerifySettings::default(),
    ));

    AppState::new(app_pool, None, storage).with_auth_handles(issuing_key, verifier)
}

async fn setup_tenant(pool: &PgPool, tenant: DataTenantId) {
    sqlx::query(
        "INSERT INTO platform.tenants (data_tenant_id, slug, display_name, status)
         VALUES ($1, $2, $2, 'active')
         ON CONFLICT (data_tenant_id) DO NOTHING",
    )
    .bind(tenant.as_uuid())
    .bind(format!("e2e-test-{}", tenant.as_uuid()))
    .execute(pool)
    .await
    .expect("tenant row inserts");

    let mut conn = TenantConn::acquire(pool, tenant)
        .await
        .expect("tenant conn opens");
    seed_builtin_roles_for_tenant(&mut conn, tenant)
        .await
        .expect("builtin roles seed");
    conn.commit().await.expect("builtin role seed commits");
}

async fn cleanup_tenant(pool: &PgPool, tenant: DataTenantId) {
    let id = tenant.as_uuid();
    sqlx::query("DELETE FROM wyrd.storage_access_ledger WHERE data_tenant_id = $1")
        .bind(id)
        .execute(pool)
        .await
        .expect("cleanup storage_access_ledger");
    sqlx::query("DELETE FROM wyrd.storage_idempotency_keys WHERE data_tenant_id = $1")
        .bind(id)
        .execute(pool)
        .await
        .expect("cleanup storage_idempotency_keys");
    sqlx::query("DELETE FROM wyrd.storage_artifact_metadata WHERE data_tenant_id = $1")
        .bind(id)
        .execute(pool)
        .await
        .expect("cleanup storage_artifact_metadata");
    sqlx::query("DELETE FROM wyrd.storage_multipart_uploads WHERE data_tenant_id = $1")
        .bind(id)
        .execute(pool)
        .await
        .expect("cleanup storage_multipart_uploads");
    sqlx::query("DELETE FROM platform.tenants WHERE data_tenant_id = $1")
        .bind(id)
        .execute(pool)
        .await
        .expect("cleanup platform.tenants");
}

fn auth_request<B: Into<axum::body::Body>>(
    method: &str,
    uri: &str,
    token: &str,
    body: B,
    content_type: Option<&str>,
) -> Request<axum::body::Body> {
    let mut builder = Request::builder()
        .method(method)
        .uri(uri)
        .header("x-wyrd-access-token", format!("Bearer {token}"));
    if let Some(ct) = content_type {
        builder = builder.header("content-type", ct);
    }
    builder.body(body.into()).expect("request builds")
}

fn local_path(url: &str) -> &str {
    url.strip_prefix("https://wyrd.test").unwrap_or(url)
}

fn mint_test_user_jwt(state: &AppState, tenant: DataTenantId, roles: Vec<RoleRef>) -> String {
    let principal = TokenPrincipalRef {
        id: PrincipalId::new(uuid::Uuid::now_v7()),
        kind: PrincipalKindWire::User,
        tenant_id: tenant,
        card_ref: None,
    };
    state
        .issuing_key
        .as_ref()
        .expect("test state has issuing key")
        .issue_user_access_token(principal, roles, ChronoDuration::minutes(5))
        .expect("test jwt mints")
}

fn runtime_admin_role() -> RoleRef {
    RoleRef::new("runtime_admin").expect("role name is valid")
}

#[tokio::test]
async fn local_backend_upload_download_round_trip() {
    if skip_unless_e2e() {
        return;
    }
    let Some(migrator_url) = migrator_url() else {
        eprintln!("skipping: WYRD_DATABASE_URL_MIGRATOR not set");
        return;
    };

    let setup_pool = SqlStore::connect(&migrator_url, 2)
        .await
        .expect("migrator pool")
        .pool()
        .clone();

    let tenant = DataTenantId::new_v7();
    setup_tenant(&setup_pool, tenant).await;

    let root = tempfile::tempdir().expect("temp dir");
    let state = local_app_state(root.path()).await;
    let token = mint_test_user_jwt(&state, tenant, vec![runtime_admin_role()]);

    let content = b"wyrd storage e2e round-trip payload";
    let sha256 = sha256_b64(content);
    let card_uid = CardUid::new(FIXED_CARD_UID).expect("card uid");

    let init_body = serde_json::to_vec(&UploadInitRequest {
        card_uid: card_uid.clone(),
        relative_path: "model/weights.bin".to_owned(),
        expected_sha256: sha256.clone(),
        expected_size_bytes: content.len() as u64,
        content_type: None,
    })
    .expect("init body serializes");

    let init_response = build_router(state.clone())
        .oneshot(auth_request(
            "POST",
            "/v1/cards/upload/init",
            &token,
            init_body,
            Some("application/json"),
        ))
        .await
        .expect("router responds");

    assert_eq!(
        init_response.status(),
        StatusCode::OK,
        "upload/init must succeed"
    );
    let init_bytes = to_bytes(init_response.into_body(), usize::MAX)
        .await
        .expect("init body");
    let init: wyrd_spec::storage::UploadInitResponse =
        serde_json::from_slice(&init_bytes).expect("init response deserializes");

    let UploadPlan::LocalFs { put_url, .. } = &init.plan else {
        panic!(
            "expected LocalFs plan for local backend, got: {:?}",
            init.plan
        );
    };
    let upload_id = init.upload_id.clone();
    let put_path = local_path(put_url).to_owned();

    let put_response = build_router(state.clone())
        .oneshot(auth_request(
            "PUT",
            &put_path,
            &token,
            content.to_vec(),
            Some("application/octet-stream"),
        ))
        .await
        .expect("router responds");

    assert_eq!(
        put_response.status(),
        StatusCode::OK,
        "local blob PUT must succeed"
    );

    let complete_body = serde_json::to_vec(&UploadCompleteRequest::SinglePut(SinglePutComplete {}))
        .expect("complete body serializes");

    let complete_response = build_router(state.clone())
        .oneshot(auth_request(
            "POST",
            &format!("/v1/cards/upload/{upload_id}/complete"),
            &token,
            complete_body,
            Some("application/json"),
        ))
        .await
        .expect("router responds");

    assert_eq!(
        complete_response.status(),
        StatusCode::OK,
        "upload/complete must succeed"
    );

    let download_init_body = serde_json::to_vec(&DownloadInitRequest {
        card_uid: card_uid.clone(),
        relative_path: "model/weights.bin".to_owned(),
        ttl_secs: None,
    })
    .expect("download init body serializes");

    let dl_init_response = build_router(state.clone())
        .oneshot(auth_request(
            "POST",
            "/v1/cards/download/init",
            &token,
            download_init_body,
            Some("application/json"),
        ))
        .await
        .expect("router responds");

    assert_eq!(
        dl_init_response.status(),
        StatusCode::OK,
        "download/init must succeed"
    );
    let dl_init_bytes = to_bytes(dl_init_response.into_body(), usize::MAX)
        .await
        .expect("download init body");
    let dl_init: wyrd_spec::storage::DownloadInitResponse =
        serde_json::from_slice(&dl_init_bytes).expect("download init response deserializes");

    assert_eq!(dl_init.sha256, sha256, "returned sha256 must match upload");
    assert_eq!(
        dl_init.size_bytes,
        content.len() as u64,
        "returned size must match upload"
    );
    assert_eq!(
        dl_init.plan.ttl_secs, 0,
        "local backend ttl_secs sentinel is 0"
    );

    let get_path = local_path(&dl_init.plan.get_url).to_owned();
    let get_response = build_router(state.clone())
        .oneshot(auth_request(
            "GET",
            &get_path,
            &token,
            axum::body::Body::empty(),
            None,
        ))
        .await
        .expect("router responds");

    assert_eq!(
        get_response.status(),
        StatusCode::OK,
        "local blob GET must succeed"
    );
    let downloaded = to_bytes(get_response.into_body(), usize::MAX)
        .await
        .expect("download body");
    assert_eq!(
        downloaded.as_ref(),
        content,
        "round-tripped bytes must match"
    );

    cleanup_tenant(&setup_pool, tenant).await;
}

#[tokio::test]
async fn upload_without_permission_returns_403() {
    if skip_unless_e2e() {
        return;
    }
    let Some(migrator_url) = migrator_url() else {
        eprintln!("skipping: WYRD_DATABASE_URL_MIGRATOR not set");
        return;
    };

    let setup_pool = SqlStore::connect(&migrator_url, 2)
        .await
        .expect("migrator pool")
        .pool()
        .clone();

    let tenant = DataTenantId::new_v7();
    setup_tenant(&setup_pool, tenant).await;

    let root = tempfile::tempdir().expect("temp dir");
    let state = local_app_state(root.path()).await;
    let token = mint_test_user_jwt(&state, tenant, vec![]);

    let card_uid = CardUid::new(FIXED_CARD_UID).expect("card uid");
    let content = b"scope check payload";

    let init_body = serde_json::to_vec(&UploadInitRequest {
        card_uid,
        relative_path: "scope-check.bin".to_owned(),
        expected_sha256: sha256_b64(content),
        expected_size_bytes: content.len() as u64,
        content_type: None,
    })
    .expect("body serializes");

    let response = build_router(state)
        .oneshot(auth_request(
            "POST",
            "/v1/cards/upload/init",
            &token,
            init_body,
            Some("application/json"),
        ))
        .await
        .expect("router responds");

    assert_eq!(
        response.status(),
        StatusCode::FORBIDDEN,
        "upload without card:write permission must return 403"
    );

    cleanup_tenant(&setup_pool, tenant).await;
}

#[tokio::test]
async fn abort_of_already_aborted_upload_returns_aborted_false() {
    if skip_unless_e2e() {
        return;
    }
    let Some(migrator_url) = migrator_url() else {
        eprintln!("skipping: WYRD_DATABASE_URL_MIGRATOR not set");
        return;
    };

    let setup_pool = SqlStore::connect(&migrator_url, 2)
        .await
        .expect("migrator pool")
        .pool()
        .clone();

    let tenant = DataTenantId::new_v7();
    setup_tenant(&setup_pool, tenant).await;

    let root = tempfile::tempdir().expect("temp dir");
    let state = local_app_state(root.path()).await;
    let token = mint_test_user_jwt(&state, tenant, vec![runtime_admin_role()]);

    let content = b"abort-race test payload";
    let card_uid = CardUid::new(FIXED_CARD_UID).expect("card uid");

    let init_body = serde_json::to_vec(&UploadInitRequest {
        card_uid,
        relative_path: "abort-race.bin".to_owned(),
        expected_sha256: sha256_b64(content),
        expected_size_bytes: content.len() as u64,
        content_type: None,
    })
    .expect("body serializes");

    let init_response = build_router(state.clone())
        .oneshot(auth_request(
            "POST",
            "/v1/cards/upload/init",
            &token,
            init_body,
            Some("application/json"),
        ))
        .await
        .expect("router responds");

    assert_eq!(init_response.status(), StatusCode::OK);
    let init_bytes = to_bytes(init_response.into_body(), usize::MAX)
        .await
        .unwrap();
    let init: wyrd_spec::storage::UploadInitResponse = serde_json::from_slice(&init_bytes).unwrap();
    let upload_id = init.upload_id;

    let first_abort = build_router(state.clone())
        .oneshot(auth_request(
            "POST",
            &format!("/v1/cards/upload/{upload_id}/abort"),
            &token,
            axum::body::Body::empty(),
            None,
        ))
        .await
        .expect("router responds");

    assert_eq!(first_abort.status(), StatusCode::OK);
    let first_bytes = to_bytes(first_abort.into_body(), usize::MAX).await.unwrap();
    let first: AbortResponse = serde_json::from_slice(&first_bytes).unwrap();
    assert!(
        first.aborted,
        "first abort of initiating upload must succeed"
    );

    let second_abort = build_router(state)
        .oneshot(auth_request(
            "POST",
            &format!("/v1/cards/upload/{upload_id}/abort"),
            &token,
            axum::body::Body::empty(),
            None,
        ))
        .await
        .expect("router responds");

    assert_eq!(second_abort.status(), StatusCode::OK);
    let second_bytes = to_bytes(second_abort.into_body(), usize::MAX)
        .await
        .unwrap();
    let second: AbortResponse = serde_json::from_slice(&second_bytes).unwrap();
    assert!(
        !second.aborted,
        "aborting an already-aborted upload must return aborted: false"
    );

    cleanup_tenant(&setup_pool, tenant).await;
}

#[tokio::test]
async fn reinit_to_same_path_after_completion_succeeds() {
    if skip_unless_e2e() {
        return;
    }
    let Some(migrator_url) = migrator_url() else {
        eprintln!("skipping: WYRD_DATABASE_URL_MIGRATOR not set");
        return;
    };

    let setup_pool = SqlStore::connect(&migrator_url, 2)
        .await
        .expect("migrator pool")
        .pool()
        .clone();

    let tenant = DataTenantId::new_v7();
    setup_tenant(&setup_pool, tenant).await;

    let root = tempfile::tempdir().expect("temp dir");
    let state = local_app_state(root.path()).await;
    let token = mint_test_user_jwt(&state, tenant, vec![runtime_admin_role()]);

    let content = b"checkpoint payload v1";
    let card_uid = CardUid::new(FIXED_CARD_UID).expect("card uid");
    let relative_path = "model/checkpoint.bin".to_owned();

    let init_body = || {
        serde_json::to_vec(&UploadInitRequest {
            card_uid: card_uid.clone(),
            relative_path: relative_path.clone(),
            expected_sha256: sha256_b64(content),
            expected_size_bytes: content.len() as u64,
            content_type: None,
        })
        .expect("body serializes")
    };

    let first_init = build_router(state.clone())
        .oneshot(auth_request(
            "POST",
            "/v1/cards/upload/init",
            &token,
            init_body(),
            Some("application/json"),
        ))
        .await
        .expect("router responds");
    assert_eq!(first_init.status(), StatusCode::OK);
    let first_bytes = to_bytes(first_init.into_body(), usize::MAX).await.unwrap();
    let first: wyrd_spec::storage::UploadInitResponse =
        serde_json::from_slice(&first_bytes).unwrap();
    let first_upload_id = first.upload_id.clone();

    let UploadPlan::LocalFs { put_url, .. } = &first.plan else {
        panic!("expected LocalFs plan");
    };
    let put_path = local_path(put_url).to_owned();

    let put = build_router(state.clone())
        .oneshot(auth_request(
            "PUT",
            &put_path,
            &token,
            content.to_vec(),
            None,
        ))
        .await
        .expect("router responds");
    assert_eq!(put.status(), StatusCode::OK);

    let complete_body = serde_json::to_vec(&UploadCompleteRequest::SinglePut(SinglePutComplete {}))
        .expect("complete body");
    let complete = build_router(state.clone())
        .oneshot(auth_request(
            "POST",
            &format!("/v1/cards/upload/{first_upload_id}/complete"),
            &token,
            complete_body,
            Some("application/json"),
        ))
        .await
        .expect("router responds");
    assert_eq!(complete.status(), StatusCode::OK);

    let second_init = build_router(state.clone())
        .oneshot(auth_request(
            "POST",
            "/v1/cards/upload/init",
            &token,
            init_body(),
            Some("application/json"),
        ))
        .await
        .expect("router responds");
    assert_eq!(
        second_init.status(),
        StatusCode::OK,
        "re-init to a completed path must succeed, not 409"
    );
    let second_bytes = to_bytes(second_init.into_body(), usize::MAX).await.unwrap();
    let second: wyrd_spec::storage::UploadInitResponse =
        serde_json::from_slice(&second_bytes).unwrap();
    assert_ne!(
        second.upload_id, first_upload_id,
        "re-init must issue a new upload_id"
    );

    cleanup_tenant(&setup_pool, tenant).await;
}
