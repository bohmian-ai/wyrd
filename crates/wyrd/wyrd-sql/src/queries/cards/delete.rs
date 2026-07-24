//! Soft-delete query for `wyrd.cards`.
//!
//! Dynamic query is intentional: the UPDATE statements use sqlx::query
//! (not the macro) because the WHERE predicates compose against
//! `wyrd.current_tenant()` which the macro's offline checker does not
//! resolve to a fixed tenant.
#![deny(missing_docs)]

use wyrd_runtime::principal::Principal;
use wyrd_spec::envelope::CardKind;
use wyrd_spec::error::WyrdError;
use wyrd_spec::ids::CardUid;
use wyrd_spec::reference::CardRef;
use wyrd_spec::request_id::RequestId;
use wyrd_spec::vala::audit_detail::CardRegistrationOperation;

use super::registry_db_error;
use crate::queries::cards::audit::{CardRegistrationAuditInput, record_card_registration_audit};
use crate::queries::cards::lifecycle::{CardManifestCompletionRow, manifest_completion_rows};
use crate::row_types::cards::{CardRow, CardStatus, ParsedCardRow};
use crate::tenant_conn::TenantConn;

const SELECT_FOR_DELETE: &str = r#"
    SELECT card_uid, data_tenant_id, kind, space, name, version,
           spec, spec_hash, artifact_hash, labels, annotations,
           status, created_by, created_at, updated_at, card_blob_uri
    FROM wyrd.cards
    WHERE card_uid = $1
    FOR UPDATE
"#;

/// Card state captured before a delete transition commits.
#[derive(Debug, Clone)]
pub struct CardDeleteState {
    /// Card row captured while the lifecycle row was locked.
    pub card: ParsedCardRow,
    /// Artifact and upload rows that require post-commit cleanup.
    pub manifests: Vec<CardManifestCompletionRow>,
    /// Whether this call changed the card to `deleted`.
    pub deleted: bool,
}

/// Soft-delete a card: set `status = 'deleted'` and write an audit row.
///
/// Idempotent on already-deleted cards (no audit row, no error).
///
/// # Errors
/// Returns `WYRD_REGISTRY_404_CARD_NOT_FOUND` when the uid is not present.
/// Returns `WYRD_REGISTRY_503_REGISTRY_UNAVAILABLE` on transient DB errors.
#[tracing::instrument(
    skip(conn),
    fields(tenant_id = %conn.data_tenant_id(), card_uid = %uid, actor = %actor.id),
)]
pub async fn soft_delete_card(
    conn: &mut TenantConn<'_>,
    uid: &CardUid,
    actor: &Principal,
    request_id: Option<&RequestId>,
) -> Result<(), WyrdError> {
    soft_delete_card_with_state(conn, uid, actor, request_id)
        .await
        .map(|_| ())
}

/// Soft-delete a card and return the state needed for post-commit cleanup.
///
/// The card row is locked before inbound references are checked. The caller
/// owns the transaction and must commit only after this function succeeds.
/// Repeating the operation for a deleted card is an idempotent no-op and
/// returns its retained cleanup state so reconciliation can retry safely.
pub async fn soft_delete_card_with_state(
    conn: &mut TenantConn<'_>,
    uid: &CardUid,
    actor: &Principal,
    request_id: Option<&RequestId>,
) -> Result<CardDeleteState, WyrdError> {
    soft_delete_card_with_expected_kind(conn, uid, None, actor, request_id).await
}

/// Soft-delete a card while asserting the exact kind supplied by the caller.
pub async fn soft_delete_card_with_kind(
    conn: &mut TenantConn<'_>,
    uid: &CardUid,
    expected_kind: CardKind,
    actor: &Principal,
    request_id: Option<&RequestId>,
) -> Result<CardDeleteState, WyrdError> {
    soft_delete_card_with_expected_kind(conn, uid, Some(expected_kind), actor, request_id).await
}

async fn soft_delete_card_with_expected_kind(
    conn: &mut TenantConn<'_>,
    uid: &CardUid,
    expected_kind: Option<CardKind>,
    actor: &Principal,
    request_id: Option<&RequestId>,
) -> Result<CardDeleteState, WyrdError> {
    let card = load_card_for_delete(conn, uid).await?;
    let manifests = manifest_completion_rows(conn, uid).await?;

    if expected_kind.is_some_and(|kind| card.kind != kind) {
        return Err(WyrdError::registry_card_not_found(format!(
            "no card with uid {uid}"
        )));
    }

    if card.status == CardStatus::Deleted {
        return Ok(CardDeleteState {
            card,
            manifests,
            deleted: false,
        });
    }
    if card.status != CardStatus::Active {
        return Err(WyrdError::Conflict {
            message: "only active cards can be deleted".to_owned(),
            details: serde_json::json!({ "card_uid": uid, "status": card.status }),
        });
    }

    let inbound_references = sqlx::query_scalar::<_, i64>(
        "SELECT count(*) \
           FROM wyrd.card_relationships relationship \
           JOIN wyrd.cards source \
             ON source.data_tenant_id = relationship.data_tenant_id \
            AND source.card_uid = relationship.card_uid \
          WHERE relationship.target_uid = $1 \
            AND source.status <> 'deleted'",
    )
    .bind(uid.as_uuid())
    .fetch_one(&mut **conn.transaction())
    .await
    .map_err(registry_db_error)?;
    if inbound_references > 0 {
        return Err(WyrdError::Conflict {
            message: "card is referenced by another visible card".to_owned(),
            details: serde_json::json!({
                "card_uid": uid,
                "inbound_reference_count": inbound_references,
            }),
        });
    }

    sqlx::query(
        "UPDATE wyrd.cards SET status = 'deleted', \
                reconcile_kind = 'cleanup', \
                reconcile_status = 'pending', \
                reconcile_attempts = 0, \
                reconcile_next_attempt_at = now(), \
                reconcile_lease_owner = NULL, \
                reconcile_lease_expires_at = NULL, \
                reconcile_last_error_code = NULL, \
                reconcile_last_error_message = NULL, \
                reconcile_dead_lettered_at = NULL, \
                updated_at = now() \
          WHERE card_uid = $1 \
            AND status = 'active'",
    )
    .bind(uid.as_uuid())
    .execute(&mut **conn.transaction())
    .await
    .map_err(registry_db_error)?;

    if matches!(card.kind, CardKind::Service | CardKind::Agent) {
        sqlx::query(
            "UPDATE wyrd.auth_service_accounts \
                SET status = 'deleted', updated_at = now() \
              WHERE card_uid = $1",
        )
        .bind(uid.as_uuid())
        .execute(&mut **conn.transaction())
        .await
        .map_err(registry_db_error)?;
    }

    record_card_registration_audit(
        conn,
        CardRegistrationAuditInput {
            card_uid: uid,
            card_kind: card.kind.clone(),
            operation: CardRegistrationOperation::Delete,
            outcome: None,
            actor,
            before_spec_hash: Some(card.spec_hash.as_str()),
            after_spec_hash: None,
            request_id,
        },
    )
    .await?;

    Ok(CardDeleteState {
        card,
        manifests,
        deleted: true,
    })
}

/// Soft-delete a card selected by its exact identity tuple.
pub async fn soft_delete_card_by_ref(
    conn: &mut TenantConn<'_>,
    card_ref: &CardRef,
    actor: &Principal,
    request_id: Option<&RequestId>,
) -> Result<CardDeleteState, WyrdError> {
    let space = card_ref.space.as_ref().ok_or_else(|| {
        WyrdError::registry_invalid_card_spec("CardRef.space is required for card deletion")
    })?;
    let uid = sqlx::query_scalar::<_, uuid::Uuid>(
        "SELECT card_uid FROM wyrd.cards \
          WHERE kind = $1 AND space = $2 AND name = $3 AND version = $4 \
          FOR UPDATE",
    )
    .bind(card_ref.kind.wire_name())
    .bind(space.as_str())
    .bind(card_ref.name.as_str())
    .bind(card_ref.version.as_str())
    .fetch_optional(&mut **conn.transaction())
    .await
    .map_err(registry_db_error)?
    .ok_or_else(|| WyrdError::registry_card_not_found("card reference was not found"))?;
    let uid = CardUid::from_uuid(uid).map_err(WyrdError::from_card_uid_error)?;
    soft_delete_card_with_state(conn, &uid, actor, request_id).await
}

async fn load_card_for_delete(
    conn: &mut TenantConn<'_>,
    uid: &CardUid,
) -> Result<ParsedCardRow, WyrdError> {
    let row = sqlx::query_as::<_, CardRow>(SELECT_FOR_DELETE)
        .bind(uid.as_uuid())
        .fetch_optional(&mut **conn.transaction())
        .await
        .map_err(registry_db_error)?
        .ok_or_else(|| WyrdError::registry_card_not_found(format!("no card with uid {uid}")))?;
    ParsedCardRow::try_from(row).map_err(|error| {
        WyrdError::registry_invalid_card_spec(format!("stored card failed to parse: {error}"))
    })
}
