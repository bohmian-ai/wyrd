//! Given/When/Then helpers for the cards e2e suite.

use wyrd_runtime::principal::Principal;
use wyrd_semver::{VersionBlock, VersionRange};
use wyrd_spec::envelope::Card;
use wyrd_spec::envelope::CardKind;
use wyrd_spec::error::WyrdError;
use wyrd_spec::ids::{CardName, CardUid, DataTenantId, SpaceName};
use wyrd_sql::queries::cards::{
    CardQuery, ListCursor, ListPage, RegisterCardOutcome, RegisterCardRequest, check_uid_exists,
    find_card_by_spec_hash, get_card_by_ref, get_latest_card_by_range, get_unique_spaces,
    list_versions, query_cards, register_card, soft_delete_card,
};
use wyrd_sql::{AuditCardRegistrationRow, CardRow, ParsedCardRow};

use super::{TestEnv, per_kind};

pub async fn register_fresh(
    env: &TestEnv,
    tenant: DataTenantId,
    actor: &Principal,
    card: &Card,
) -> Result<RegisterCardOutcome, WyrdError> {
    let mut conn = env.tenant_conn(tenant).await;
    let outcome = register_card(
        &mut conn,
        RegisterCardRequest {
            card,
            actor,
            request_id: None,
        },
    )
    .await?;
    conn.commit().await.expect("commit failed");
    Ok(outcome)
}

pub async fn register(
    env: &TestEnv,
    tenant: DataTenantId,
    actor: &Principal,
    card: &Card,
) -> Result<RegisterCardOutcome, WyrdError> {
    register_fresh(env, tenant, actor, card).await
}

pub async fn register_then_reapply_identical(
    env: &TestEnv,
    tenant: DataTenantId,
    actor: &Principal,
    card: &Card,
) -> Result<(RegisterCardOutcome, RegisterCardOutcome), WyrdError> {
    let first = register_fresh(env, tenant, actor, card).await?;
    let second = register_fresh(env, tenant, actor, card).await?;
    Ok((first, second))
}

pub async fn register_then_reapply_with_drift(
    env: &TestEnv,
    tenant: DataTenantId,
    actor: &Principal,
    v1: &Card,
) -> Result<(RegisterCardOutcome, WyrdError), WyrdError> {
    let first = register_fresh(env, tenant, actor, v1).await?;
    let mut drifted = v1.clone();
    per_kind::mutate_for_drift(&mut drifted);
    let err = expect_register_error(env, tenant, actor, &drifted).await;
    Ok((first, err))
}

pub async fn expect_register_error(
    env: &TestEnv,
    tenant: DataTenantId,
    actor: &Principal,
    card: &Card,
) -> WyrdError {
    let mut conn = env.tenant_conn(tenant).await;
    register_card(
        &mut conn,
        RegisterCardRequest {
            card,
            actor,
            request_id: None,
        },
    )
    .await
    .expect_err("expected registration to fail")
}

pub async fn soft_delete(
    env: &TestEnv,
    tenant: DataTenantId,
    uid: &CardUid,
    actor: &Principal,
) -> Result<(), WyrdError> {
    let mut conn = env.tenant_conn(tenant).await;
    soft_delete_card(&mut conn, uid, actor, None).await?;
    conn.commit().await.expect("commit failed");
    Ok(())
}

pub async fn get_by_ref(
    env: &TestEnv,
    tenant: DataTenantId,
    kind: CardKind,
    space: &SpaceName,
    name: &CardName,
    version: &VersionBlock,
) -> Result<ParsedCardRow, WyrdError> {
    let mut conn = env.tenant_conn(tenant).await;
    get_card_by_ref(&mut conn, kind, space, name, version).await
}

pub async fn latest_by_range(
    env: &TestEnv,
    tenant: DataTenantId,
    kind: CardKind,
    space: &SpaceName,
    name: &CardName,
    range: &VersionRange,
) -> Result<ParsedCardRow, WyrdError> {
    let mut conn = env.tenant_conn(tenant).await;
    get_latest_card_by_range(&mut conn, kind, space, name, range).await
}

pub async fn versions(
    env: &TestEnv,
    tenant: DataTenantId,
    kind: CardKind,
    space: &SpaceName,
    name: &CardName,
    include_prerelease: bool,
) -> Result<Vec<VersionBlock>, WyrdError> {
    let mut conn = env.tenant_conn(tenant).await;
    list_versions(&mut conn, kind, space, name, include_prerelease).await
}

pub async fn query(
    env: &TestEnv,
    tenant: DataTenantId,
    q: &CardQuery,
    cursor: ListCursor,
) -> Result<ListPage<CardRow>, WyrdError> {
    let mut conn = env.tenant_conn(tenant).await;
    query_cards(&mut conn, q, cursor).await
}

pub async fn find_by_hash(
    env: &TestEnv,
    tenant: DataTenantId,
    kind: CardKind,
    space: &SpaceName,
    name: &CardName,
    hash: &str,
) -> Result<Option<CardRow>, WyrdError> {
    let mut conn = env.tenant_conn(tenant).await;
    find_card_by_spec_hash(&mut conn, kind, space, name, hash).await
}

pub async fn uid_exists(
    env: &TestEnv,
    tenant: DataTenantId,
    uid: &CardUid,
) -> Result<bool, WyrdError> {
    let mut conn = env.tenant_conn(tenant).await;
    check_uid_exists(&mut conn, uid).await
}

pub async fn unique_spaces(
    env: &TestEnv,
    tenant: DataTenantId,
) -> Result<Vec<SpaceName>, WyrdError> {
    let mut conn = env.tenant_conn(tenant).await;
    get_unique_spaces(&mut conn).await
}

pub async fn fetch_version_columns(
    env: &TestEnv,
    tenant: DataTenantId,
    uid: &CardUid,
) -> (i64, i64, i64, bool) {
    let mut conn = env.tenant_conn(tenant).await;
    sqlx::query_as(
        "SELECT version_major, version_minor, version_patch, version_is_prerelease \
         FROM wyrd.cards WHERE data_tenant_id = wyrd.current_tenant() AND card_uid = $1",
    )
    .bind(uid.as_uuid())
    .fetch_one(&mut **conn.transaction())
    .await
    .expect("version columns fetch")
}

pub async fn try_update_version_major(
    env: &TestEnv,
    tenant: DataTenantId,
    uid: &CardUid,
) -> sqlx::Error {
    let mut conn = env.tenant_conn(tenant).await;
    sqlx::query(
        "UPDATE wyrd.cards SET version_major = 999 \
         WHERE data_tenant_id = wyrd.current_tenant() AND card_uid = $1",
    )
    .bind(uid.as_uuid())
    .execute(&mut **conn.transaction())
    .await
    .expect_err("generated version column update must fail")
}

pub async fn try_update_version(
    env: &TestEnv,
    tenant: DataTenantId,
    uid: &CardUid,
    version: &str,
) -> sqlx::Error {
    let mut conn = env.tenant_conn(tenant).await;
    sqlx::query(
        "UPDATE wyrd.cards SET version = $2 \
         WHERE data_tenant_id = wyrd.current_tenant() AND card_uid = $1",
    )
    .bind(uid.as_uuid())
    .bind(version)
    .execute(&mut **conn.transaction())
    .await
    .expect_err("canonical version update must fail")
}

pub async fn fetch_registration_audit(
    env: &TestEnv,
    tenant: DataTenantId,
    uid: &CardUid,
) -> Vec<AuditCardRegistrationRow> {
    let mut conn = env.tenant_conn(tenant).await;
    sqlx::query_as(
        "SELECT audit_id, data_tenant_id, card_uid, kind, operation, outcome, \
                actor_principal_id, actor_kind, before_spec_hash, after_spec_hash, \
                request_id, occurred_at \
         FROM wyrd.audit_card_registration \
         WHERE data_tenant_id = wyrd.current_tenant() AND card_uid = $1 \
         ORDER BY occurred_at ASC",
    )
    .bind(uid.as_uuid())
    .fetch_all(&mut **conn.transaction())
    .await
    .expect("registration audit fetch")
}
