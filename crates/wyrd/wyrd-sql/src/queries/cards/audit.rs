//! Transactional audit writer for the card registry plane.
//!
//! Card registration is a control-plane operation, so its outbox append lives
//! in the same tenant transaction as the card write. The append is deliberately
//! fail-closed: a failed chain update aborts the caller's transaction.
// raw-query grep allowlist: card audit writes post-date the SQLx offline cache;
// run `mise run sqlx:prepare` to promote them to macros.
#![deny(missing_docs)]
// raw-query grep allowlist: audit writes use a typed TenantConn transaction;
// sqlx macros cannot cover the append-only hash-chain payload shape.

use sha2::{Digest, Sha256};
use wyrd_runtime::principal::Principal;
use wyrd_spec::envelope::{CardKind, SpecHash};
use wyrd_spec::error::WyrdError;
use wyrd_spec::ids::CardUid;
use wyrd_spec::request_id::RequestId;
use wyrd_spec::vala::audit_detail::{
    AuditDetail, CardRegistrationOperation, CardRegistrationOutcome, audit_detail_canonical_json,
};

use crate::tenant_conn::TenantConn;

pub(super) struct CardRegistrationAuditInput<'a> {
    pub(super) card_uid: &'a CardUid,
    pub(super) card_kind: CardKind,
    pub(super) operation: CardRegistrationOperation,
    pub(super) outcome: Option<CardRegistrationOutcome>,
    pub(super) actor: &'a Principal,
    pub(super) before_spec_hash: Option<&'a str>,
    pub(super) after_spec_hash: Option<&'a str>,
    pub(super) request_id: Option<&'a RequestId>,
}

/// Append one typed card-registration event to the tenant audit outbox.
pub(crate) async fn record_card_registration_audit(
    conn: &mut TenantConn<'_>,
    input: CardRegistrationAuditInput<'_>,
) -> Result<(), WyrdError> {
    let detail = AuditDetail::CardRegistration {
        card_uid: input.card_uid.clone(),
        card_kind: input.card_kind,
        operation: input.operation,
        outcome: input.outcome,
        before_spec_hash: input.before_spec_hash.map(spec_hash),
        after_spec_hash: input.after_spec_hash.map(spec_hash),
    };
    let detail = audit_detail_canonical_json(&detail);
    let request_id = input.request_id.cloned().unwrap_or_else(RequestId::now_v7);
    let principal_kind = input.actor.kind.tag().as_str();
    let card_ref = input.actor.card_ref().map(ToString::to_string);
    let resource = format!("card:{}", input.card_uid);
    let operation_name = "card.registration";
    let permission = "cards:write";
    let outcome = "allowed";

    sqlx::query(
        r#"INSERT INTO vala.audit_chain_head (data_tenant_id)
           VALUES (wyrd.current_tenant())
           ON CONFLICT (data_tenant_id) DO NOTHING"#,
    )
    .execute(&mut **conn.transaction())
    .await
    .map_err(|error| audit_error(error, "initialize chain head"))?;

    let (last_seq, prev_hash): (i64, Vec<u8>) = sqlx::query_as(
        r#"SELECT last_seq, head_hash
           FROM vala.audit_chain_head
           FOR UPDATE"#,
    )
    .fetch_one(&mut **conn.transaction())
    .await
    .map_err(|error| audit_error(error, "lock chain head"))?;

    let seq = last_seq + 1;
    let entry_hash = entry_hash(
        &prev_hash,
        seq,
        request_id.as_str(),
        None,
        operation_name,
        &resource,
        card_ref.as_deref(),
        input.actor.id.as_uuid().as_bytes(),
        principal_kind,
        permission,
        outcome,
        Some(&detail),
    );

    sqlx::query(
        r#"INSERT INTO vala.audit_staging
           (data_tenant_id, seq, prev_hash, entry_hash, request_id, trace_id,
            operation, resource, card_ref, principal_id, principal_kind,
            permission, outcome, detail)
           VALUES (wyrd.current_tenant(), $1, $2, $3, $4, NULL, $5, $6, $7,
                   $8, $9, $10, $11, $12)"#,
    )
    .bind(seq)
    .bind(prev_hash.as_slice())
    .bind(entry_hash.as_slice())
    .bind(request_id.as_str())
    .bind(operation_name)
    .bind(&resource)
    .bind(card_ref.as_deref())
    .bind(input.actor.id.as_uuid())
    .bind(principal_kind)
    .bind(permission)
    .bind(outcome)
    .bind(&detail)
    .execute(&mut **conn.transaction())
    .await
    .map_err(|error| audit_error(error, "insert outbox row"))?;

    sqlx::query(
        r#"UPDATE vala.audit_chain_head
           SET last_seq = $1, head_hash = $2, updated_at = now()"#,
    )
    .bind(seq)
    .bind(entry_hash.as_slice())
    .execute(&mut **conn.transaction())
    .await
    .map_err(|error| audit_error(error, "advance chain head"))?;

    Ok(())
}

fn spec_hash(value: &str) -> SpecHash {
    value
        .parse()
        .expect("registry spec hashes are canonical BLAKE3 hex")
}

fn audit_error(error: sqlx::Error, stage: &str) -> WyrdError {
    tracing::error!(error = %error, stage, "card registration audit operation failed");
    WyrdError::registry_unavailable("card registry unavailable")
}

#[expect(
    clippy::too_many_arguments,
    reason = "matches the audit hash wire fields"
)]
fn entry_hash(
    prev_hash: &[u8],
    seq: i64,
    request_id: &str,
    trace_id: Option<&str>,
    operation: &str,
    resource: &str,
    card_ref: Option<&str>,
    principal_id: &[u8],
    principal_kind: &str,
    permission: &str,
    outcome: &str,
    detail: Option<&str>,
) -> [u8; 32] {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(prev_hash);
    bytes.extend_from_slice(&seq.to_be_bytes());
    push_str(&mut bytes, request_id);
    push_opt(&mut bytes, trace_id);
    push_str(&mut bytes, operation);
    push_str(&mut bytes, resource);
    push_opt(&mut bytes, card_ref);
    bytes.extend_from_slice(principal_id);
    push_str(&mut bytes, principal_kind);
    push_str(&mut bytes, permission);
    push_str(&mut bytes, outcome);
    push_opt(&mut bytes, detail);
    Sha256::digest(bytes).into()
}

fn push_str(bytes: &mut Vec<u8>, value: &str) {
    bytes.extend_from_slice(&(value.len() as u64).to_be_bytes());
    bytes.extend_from_slice(value.as_bytes());
}

fn push_opt(bytes: &mut Vec<u8>, value: Option<&str>) {
    match value {
        Some(value) => {
            bytes.push(1);
            push_str(bytes, value);
        }
        None => bytes.push(0),
    }
}
