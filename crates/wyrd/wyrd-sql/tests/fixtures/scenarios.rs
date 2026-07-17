//! Given/When/Then helpers for the cards e2e suite.
//!
//! Each function owns one complete operation: it opens a [`TenantConn`],
//! calls the relevant `wyrd-sql` query function, commits (where appropriate),
//! and returns the result. Tests compose these helpers to build readable
//! GIVEN → WHEN → THEN sequences without SQL appearing inline.
//!
//! Naming conventions:
//! - `register` / `register_fresh` — happy-path card registration.
//! - `expect_register_error` — registration that must fail; panics on success.
//! - `register_then_*` — multi-step composed scenarios.
//! - `fetch_*` / `try_*` — raw reads or forced-failure writes used to assert
//!   database state directly (bypassing the public query API).

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
use wyrd_sql::{CardRow, ParsedCardRow};

use super::{TestEnv, per_kind};

/// Register a card in a fresh transaction and commit. Returns the outcome or
/// the first error. Use this when you expect registration to succeed; prefer
/// [`expect_register_error`] when you expect it to fail.
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
            status: wyrd_sql::CardStatus::Active,
        },
    )
    .await?;
    conn.commit().await.expect("commit failed");
    Ok(outcome)
}

/// Alias for [`register_fresh`]. Prefer this in tests that only need a single
/// registration step and do not need to distinguish "first ever" from a
/// re-apply.
pub async fn register(
    env: &TestEnv,
    tenant: DataTenantId,
    actor: &Principal,
    card: &Card,
) -> Result<RegisterCardOutcome, WyrdError> {
    register_fresh(env, tenant, actor, card).await
}

/// Register a card, then register the exact same card again. Returns both
/// outcomes. The first should be `Created`; the second should be
/// `IdempotentNoop` (pin) or `Deduplicated` (auto/scope).
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

/// Register a card at a pinned version, then mutate the spec and attempt to
/// re-register at the same pin. Returns the first (successful) outcome and
/// the error produced by the drifted re-register. The error should be
/// `WYRD_REGISTRY_409_SPEC_DRIFT`.
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

/// Register a card and assert that it fails. Panics if the registration
/// succeeds. Returns the error for assertion. Use when you want to verify that
/// an invalid card is rejected with a specific error code.
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
            status: wyrd_sql::CardStatus::Active,
        },
    )
    .await
    .expect_err("expected registration to fail")
}

/// Soft-delete the card identified by `uid` and commit. The row remains in
/// `wyrd.cards` with `status = 'deleted'` and is excluded from all active
/// reads.
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

/// Fetch a card by its exact `(kind, space, name, version)` identity.
/// Returns `WYRD_REGISTRY_404_CARD_NOT_FOUND` when no active row matches.
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

/// Fetch the latest stable card within a semver range. Pre-release rows are
/// always excluded. Returns `WYRD_REGISTRY_404_CARD_NOT_FOUND` when no stable row
/// falls within `range`.
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

/// List all registered versions in a `(kind, space, name)` line, newest
/// first. Pass `include_prerelease = true` to include pre-release rows.
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

/// Run a composable card collection query and return one page. Use
/// [`cursor`](ListCursor) to control page size and keyset position.
pub async fn query(
    env: &TestEnv,
    tenant: DataTenantId,
    q: &CardQuery,
    cursor: ListCursor,
) -> Result<ListPage<CardRow>, WyrdError> {
    let mut conn = env.tenant_conn(tenant).await;
    query_cards(&mut conn, q, cursor).await
}

/// Find an active card in a line whose spec hash matches. Returns `None` when
/// no active row in the line carries that hash. Used to verify dedup behavior.
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

/// Check whether a card uid exists in the tenant, including deleted rows.
/// Used to verify that soft-delete preserves the row rather than removing it.
pub async fn uid_exists(
    env: &TestEnv,
    tenant: DataTenantId,
    uid: &CardUid,
) -> Result<bool, WyrdError> {
    let mut conn = env.tenant_conn(tenant).await;
    check_uid_exists(&mut conn, uid).await
}

/// Return all distinct non-deleted space slugs for the tenant, sorted
/// ascending. Used to verify that space aggregation is tenant-scoped.
pub async fn unique_spaces(
    env: &TestEnv,
    tenant: DataTenantId,
) -> Result<Vec<SpaceName>, WyrdError> {
    let mut conn = env.tenant_conn(tenant).await;
    get_unique_spaces(&mut conn).await
}

/// Read the raw generated version columns `(major, minor, patch,
/// is_prerelease)` for a card uid. These are GENERATED ALWAYS columns derived
/// from the canonical `version` string; this helper verifies that the database
/// trigger parses and stores them correctly.
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

/// Attempt to UPDATE `version_major` directly. This must fail because the
/// column is GENERATED ALWAYS — the database rejects any explicit write to it.
/// Panics if the update succeeds (schema regression).
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

/// Attempt to UPDATE the canonical `version` string directly. This must fail
/// because `cards_version_immutable` prevents any post-insert change to the
/// version column. Panics if the update succeeds (schema regression).
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

/// A card-registration detail read from the Vala audit outbox.
#[derive(Debug, sqlx::FromRow)]
pub struct RegistrationAuditRow {
    /// Canonical typed detail JSON.
    pub detail: Option<String>,
}

/// Fetch card-registration events from the tenant audit outbox.
pub async fn fetch_registration_audit(
    env: &TestEnv,
    tenant: DataTenantId,
    uid: &CardUid,
) -> Vec<RegistrationAuditRow> {
    let mut conn = env.tenant_conn(tenant).await;
    sqlx::query_as(
        "SELECT detail FROM vala.audit_outbox \
         WHERE data_tenant_id = wyrd.current_tenant() \
           AND resource = $1 AND operation = 'card.registration' \
         ORDER BY seq ASC",
    )
    .bind(format!("card:{uid}"))
    .fetch_all(&mut **conn.transaction())
    .await
    .expect("registration audit fetch")
}
