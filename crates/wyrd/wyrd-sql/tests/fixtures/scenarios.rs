//! Given/When/Then helpers for the cards e2e suite.

use wyrd_spec::envelope::Card;
use wyrd_spec::error::WyrdError;
use wyrd_spec::ids::{CardUid, DataTenantId};
use wyrd_runtime::principal::Principal;
use wyrd_sql::queries::cards::{RegisterCardOutcome, RegisterCardRequest, soft_delete_card};
use wyrd_sql::queries::cards::register_card;

use super::{per_kind, TestEnv};

pub async fn register_fresh(
    env: &TestEnv,
    tenant: DataTenantId,
    actor: &Principal,
    card: &Card,
) -> Result<RegisterCardOutcome, WyrdError> {
    let mut conn = env.tenant_conn(tenant).await;
    let outcome = register_card(&mut conn, RegisterCardRequest {
        card,
        actor,
        request_id: None,
    })
    .await?;
    conn.commit().await.expect("commit failed");
    Ok(outcome)
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
    register_card(&mut conn, RegisterCardRequest {
        card,
        actor,
        request_id: None,
    })
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
