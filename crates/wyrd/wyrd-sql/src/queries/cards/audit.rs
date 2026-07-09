//! Transactional audit writer for the card registry plane.
//!
//! The only callers are `register_card` and `soft_delete_card` — both inside
//! the same `TenantConn` tx as the corresponding `wyrd.cards` write.
//! Fire-and-forget audit emission is forbidden: an audit row must commit in
//! the same transaction as the write it describes.
#![deny(missing_docs)]

use wyrd_spec::error::WyrdError;

use crate::row_types::cards::{
    CardRegistrationOperation, NewAuditCardRegistrationRow, actor_kind_db_str,
};
use crate::tenant_conn::TenantConn;

/// Insert one append-only audit row for a card registration event.
///
/// MUST be called inside the same `TenantConn` tx as the corresponding
/// `wyrd.cards` write.
#[tracing::instrument(
    skip(conn, row),
    fields(
        tenant_id = %row.data_tenant_id,
        audit_id = %row.audit_id,
        operation = ?row.operation,
        kind = ?row.kind,
    )
)]
pub(crate) async fn record_card_registration_audit(
    conn: &mut TenantConn<'_>,
    row: NewAuditCardRegistrationRow<'_>,
) -> Result<(), WyrdError> {
    debug_assert!(
        match row.operation {
            CardRegistrationOperation::Register =>
                row.before_spec_hash.is_none()
                    && row.after_spec_hash.is_some()
                    && row.outcome.is_some(),
            CardRegistrationOperation::Update =>
                row.before_spec_hash.is_some()
                    && row.after_spec_hash.is_some()
                    && row.outcome.is_none(),
            CardRegistrationOperation::Delete =>
                row.before_spec_hash.is_some()
                    && row.after_spec_hash.is_none()
                    && row.outcome.is_none(),
        },
        "audit row operation/hash invariant violated"
    );

    let kind_db = row.kind.wire_name();
    let operation_db = row.operation.as_db_str();
    let outcome_db = row.outcome.map(|outcome| outcome.as_db_str());
    let actor_kind_db = actor_kind_db_str(&row.actor_kind);

    // Dynamic query is intentional: the audit table is append-only and the
    // sqlx macro adds no value here. The data_tenant_id is sourced from
    // `wyrd.current_tenant()` (set by TenantConn) so callers cannot write
    // an audit row scoped to a different tenant.
    let result = sqlx::query(
        r#"
        INSERT INTO wyrd.audit_card_registration (
            audit_id,
            data_tenant_id,
            card_uid,
            kind,
            operation,
            outcome,
            actor_principal_id,
            actor_kind,
            before_spec_hash,
            after_spec_hash,
            request_id,
            occurred_at
        )
        VALUES ($1, wyrd.current_tenant(), $2, $3, $4, $5, $6, $7, $8, $9, $10, NOW())
        "#,
    )
    .bind(row.audit_id)
    .bind(row.card_uid.as_uuid())
    .bind(kind_db)
    .bind(operation_db)
    .bind(outcome_db)
    .bind(row.actor_principal_id.as_uuid())
    .bind(actor_kind_db)
    .bind(row.before_spec_hash)
    .bind(row.after_spec_hash)
    .bind(row.request_id)
    .execute(&mut **conn.transaction())
    .await;

    match result {
        Ok(_) => Ok(()),
        Err(sqlx::Error::Database(db)) => {
            let code = db.code().map(|c| c.into_owned());
            let constraint = db.constraint().map(|c| c.to_owned());
            match (code.as_deref(), constraint.as_deref()) {
                (Some("42501"), Some("audit_card_registration_append_only")) => {
                    tracing::error!("audit_card_registration is append-only; this is a bug");
                    Err(WyrdError::internal(
                        "audit_card_registration is append-only; Rust contract guarantees no mutation",
                    ))
                }
                (Some("23514"), Some("audit_card_registration_op_hash_consistency")) => {
                    tracing::error!("audit op/hash invariant violated; this is a bug");
                    Err(WyrdError::internal(
                        "audit_card_registration op/hash invariant violated",
                    ))
                }
                (Some("23514"), Some("audit_card_registration_outcome_operation_check")) => {
                    tracing::error!("audit outcome invariant violated; this is a bug");
                    Err(WyrdError::internal(
                        "audit_card_registration outcome invariant violated",
                    ))
                }
                (Some("23514"), Some("audit_card_registration_kind_check")) => {
                    tracing::error!("audit kind literal drift; this is a bug");
                    Err(WyrdError::internal(
                        "audit_card_registration.kind literal mismatch; CardKind::wire_name drift",
                    ))
                }
                _ => Err(WyrdError::registry_unavailable(db.message().to_owned())),
            }
        }
        Err(other) => Err(WyrdError::registry_unavailable(other.to_string())),
    }
}
