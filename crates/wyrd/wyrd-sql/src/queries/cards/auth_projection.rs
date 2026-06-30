//! Apply-side principal projection for Service / Agent cards.
//!
//! Called only by `register_card`. Must run inside the same `TenantConn` tx.
#![deny(missing_docs)]

use serde_json::Value as JsonValue;
use uuid::Uuid;
use wyrd_semver::{VersionBlock, VersionSpec};

use wyrd_runtime::principal::{Principal, PrincipalId};
use wyrd_spec::envelope::{Card, Spec};
use wyrd_spec::error::WyrdError;
use wyrd_spec::ids::CardUid;
use wyrd_spec::reference::CardRef;

use crate::tenant_conn::TenantConn;

const UPSERT_SQL: &str = r#"
INSERT INTO wyrd.auth_service_accounts
    (id, data_tenant_id, principal_kind, card_kind, card_uid,
     space, name, version, card_ref, description, status, created_by)
VALUES (gen_random_uuid(), $1, $2, $3, $4,
        $5, $6, $7, $8, $9, 'active', $10)
ON CONFLICT (data_tenant_id, principal_kind, card_kind, card_uid)
DO UPDATE
    SET card_ref    = EXCLUDED.card_ref,
        space       = EXCLUDED.space,
        name        = EXCLUDED.name,
        version     = EXCLUDED.version,
        description = EXCLUDED.description,
        updated_at  = now()
RETURNING id
"#;

/// Upsert the `wyrd.auth_service_accounts` row backing a Service or Agent card.
///
/// MUST be called only when `card.spec` is `Spec::Service` or `Spec::Agent`.
/// Operates on the same `TenantConn` tx as the cards INSERT.
#[tracing::instrument(
    skip(conn, card, actor),
    fields(
        tenant_id = %conn.data_tenant_id(),
        kind = ?card.kind,
        card_uid = %card_uid,
        actor = %actor.id,
    )
)]
pub(crate) async fn upsert_service_account_from_card(
    conn: &mut TenantConn<'_>,
    card_uid: &CardUid,
    card: &Card,
    actor: &Principal,
) -> Result<PrincipalId, WyrdError> {
    let (principal_kind, description) = match &card.spec {
        Spec::Service(spec) => ("service", spec.description.clone()),
        Spec::Agent(_) => ("agent", None),
        _ => {
            tracing::error!(
                kind = ?card.kind,
                "principal projection invoked for non-Service / non-Agent spec; this is a bug"
            );
            return Err(WyrdError::internal(
                "principal projection dispatched on wrong spec variant",
            ));
        }
    };

    let space = card.metadata.space.clone().ok_or_else(|| {
        WyrdError::registry_invalid_card_spec(
            "metadata.space is required for Service / Agent registration",
        )
    })?;

    let version_block: VersionBlock = match &card.metadata.version {
        Some(VersionSpec::Pin(block)) => block.clone(),
        _ => {
            return Err(WyrdError::internal(
                "principal projection requires a pinned version; validate_boundary must run first",
            ));
        }
    };

    let card_ref = CardRef {
        kind: card.kind.clone(),
        name: card.metadata.name.clone(),
        version: version_block.clone(),
        space: space.clone(),
        uid: Some(card_uid.clone()),
    };

    let card_ref_json: JsonValue =
        serde_json::to_value(&card_ref).map_err(WyrdError::from_spec_serialization)?;

    let card_kind_str = card.kind.wire_name();

    let result = sqlx::query_scalar::<_, Uuid>(UPSERT_SQL)
        .bind(conn.data_tenant_id().as_uuid())
        .bind(principal_kind)
        .bind(card_kind_str)
        .bind(card_uid.as_uuid())
        .bind(space.as_str())
        .bind(card.metadata.name.as_str())
        .bind(version_block.as_str())
        .bind(card_ref_json)
        .bind(description.as_deref())
        .bind(actor.id.as_uuid())
        .fetch_one(&mut **conn.transaction())
        .await;

    match result {
        Ok(id) => Ok(PrincipalId::new(id)),
        Err(sqlx::Error::Database(db)) => {
            let code = db.code().map(|c| c.into_owned());
            match code.as_deref() {
                Some("23503") => Err(WyrdError::registry_unavailable(
                    "auth_service_accounts FK violation; tenant row may not exist yet",
                )),
                Some("23505") => Err(WyrdError::internal(
                    "auth_service_accounts unique violation outside the upsert key; name collision",
                )),
                _ => Err(WyrdError::registry_unavailable(db.message().to_owned())),
            }
        }
        Err(other) => Err(WyrdError::registry_unavailable(format!(
            "auth_service_accounts upsert failed: {other}"
        ))),
    }
}

/// Look up the existing `principal_id` for a Service/Agent card on re-apply.
///
/// Called on the idempotent-noop path when `register_card` detects a hash
/// match (no upsert needed, but we still want to return `principal_id`).
pub(crate) async fn lookup_existing_principal_id(
    conn: &mut TenantConn<'_>,
    card_uid: &CardUid,
) -> Result<Option<PrincipalId>, WyrdError> {
    let result = sqlx::query_scalar::<_, Uuid>(
        r#"
        SELECT id
        FROM wyrd.auth_service_accounts
        WHERE data_tenant_id = wyrd.current_tenant()
          AND card_uid = $1
        "#,
    )
    .bind(card_uid.as_uuid())
    .fetch_optional(&mut **conn.transaction())
    .await
    .map_err(|e| WyrdError::registry_unavailable(e.to_string()))?;

    Ok(result.map(PrincipalId::new))
}
