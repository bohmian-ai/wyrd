//! Soft-delete query for `wyrd.cards`.
#![deny(missing_docs)]

use ulid::Ulid;
use uuid::Uuid;

use wyrd_runtime::principal::Principal;
use wyrd_spec::error::WyrdError;
use wyrd_spec::ids::CardUid;
use wyrd_spec::request_id::RequestId;

use crate::queries::cards::audit::record_card_registration_audit;
use crate::row_types::cards::{CardRegistrationOperation, NewAuditCardRegistrationRow};
use crate::tenant_conn::TenantConn;

/// Soft-delete a card: set `status = 'deleted'` and write an audit row.
///
/// Idempotent on already-deleted cards (no audit row, no error).
///
/// # Errors
/// Returns `WYRD_REG_404_CARD_NOT_FOUND` when the uid is not present.
/// Returns `WYRD_REG_503_REGISTRY_UNAVAILABLE` on transient DB errors.
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
    let existing = sqlx::query_as::<_, (String, String)>(
        "SELECT spec_hash, kind FROM wyrd.cards WHERE card_uid = $1",
    )
    .bind(uid.as_uuid())
    .fetch_optional(&mut **conn.transaction())
    .await
    .map_err(|e| WyrdError::registry_unavailable(e.to_string()))?;

    let (spec_hash, kind_str) = existing
        .ok_or_else(|| WyrdError::registry_card_not_found(format!("no card with uid {uid}")))?;

    let rows_affected = sqlx::query(
        "UPDATE wyrd.cards SET status = 'deleted' \
         WHERE card_uid = $1 AND status = 'active'",
    )
    .bind(uid.as_uuid())
    .execute(&mut **conn.transaction())
    .await
    .map_err(|e| WyrdError::registry_unavailable(e.to_string()))?
    .rows_affected();

    if rows_affected == 0 {
        return Ok(());
    }

    if matches!(kind_str.as_str(), "Service" | "Agent") {
        sqlx::query(
            "UPDATE wyrd.auth_service_accounts \
             SET status = 'card_deleted', updated_at = now() \
             WHERE data_tenant_id = wyrd.current_tenant() AND card_uid = $1",
        )
        .bind(uid.as_uuid())
        .execute(&mut **conn.transaction())
        .await
        .map_err(|e| WyrdError::registry_unavailable(e.to_string()))?;
    }

    let card_kind = wyrd_spec::envelope::CardKind::from_wire_name(&kind_str).ok_or_else(|| {
        WyrdError::internal(format!("stored kind {kind_str:?} is not a known CardKind"))
    })?;

    record_card_registration_audit(
        conn,
        NewAuditCardRegistrationRow {
            audit_id: Uuid::from_bytes(Ulid::new().to_bytes()),
            data_tenant_id: conn.data_tenant_id().as_uuid(),
            card_uid: uid,
            kind: card_kind,
            operation: CardRegistrationOperation::Delete,
            actor_principal_id: actor.id,
            actor_kind: actor.kind.clone(),
            before_spec_hash: Some(spec_hash.as_str()),
            after_spec_hash: None,
            request_id: request_id.map(RequestId::as_str),
        },
    )
    .await
}
