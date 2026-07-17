//! Service and Agent principal projection for composite registration.
#![deny(missing_docs)]

use serde_json::Value as JsonValue;
use uuid::Uuid;
use wyrd_runtime::principal::{Principal, PrincipalId};
use wyrd_semver::{VersionBlock, VersionSpec};
use wyrd_spec::envelope::{Card, Spec};
use wyrd_spec::error::WyrdError;
use wyrd_spec::ids::CardUid;
use wyrd_spec::reference::CardRef;

use crate::tenant_conn::TenantConn;

const UPSERT_SQL: &str = r#"
INSERT INTO wyrd.auth_service_accounts
    (id, data_tenant_id, principal_kind, card_kind, card_uid,
     space, name, version, card_ref, description, status, created_by)
VALUES (gen_random_uuid(), wyrd.current_tenant(), $1, $2, $3,
        $4, $5, $6, $7, $8, 'active', $9)
ON CONFLICT (data_tenant_id, principal_kind, card_kind, card_uid)
DO UPDATE SET card_ref = EXCLUDED.card_ref, space = EXCLUDED.space,
    name = EXCLUDED.name, version = EXCLUDED.version,
    description = EXCLUDED.description, updated_at = now()
RETURNING id
"#;

/// Upsert the principal row backing a newly registered Service or Agent card.
#[tracing::instrument(skip(conn), fields(operation = "card.service_account.upsert"))]
pub async fn upsert_service_account_from_card(
    conn: &mut TenantConn<'_>,
    card_uid: &CardUid,
    card: &Card,
    actor: &Principal,
) -> Result<PrincipalId, WyrdError> {
    let (principal_kind, description) = match &card.spec {
        Spec::Service(spec) => ("service", spec.description.as_deref()),
        Spec::Agent(_) => ("agent", None),
        _ => {
            return Err(WyrdError::internal(
                "principal projection requires a Service or Agent card",
            ));
        }
    };
    let space = card
        .metadata
        .space
        .as_ref()
        .ok_or_else(|| WyrdError::registry_invalid_card_spec("metadata.space is required"))?;
    let version: &VersionBlock = match card.metadata.version.as_ref() {
        Some(VersionSpec::Pin(version)) => version,
        _ => {
            return Err(WyrdError::internal(
                "principal projection requires resolved version",
            ));
        }
    };
    let card_ref = CardRef {
        kind: card.kind.clone(),
        name: card.metadata.name.clone(),
        version: version.clone(),
        space: space.clone(),
        uid: Some(card_uid.clone()),
    };
    let card_ref_json: JsonValue =
        serde_json::to_value(card_ref).map_err(WyrdError::from_spec_serialization)?;
    let id = sqlx::query_scalar::<_, Uuid>(UPSERT_SQL)
        .bind(principal_kind)
        .bind(card.kind.wire_name())
        .bind(card_uid.as_uuid())
        .bind(space.as_str())
        .bind(card.metadata.name.as_str())
        .bind(version.as_str())
        .bind(card_ref_json)
        .bind(description)
        .bind(actor.id.as_uuid())
        .fetch_one(&mut **conn.transaction())
        .await
        .map_err(|error| {
            tracing::error!(%error, "service-account projection failed");
            WyrdError::registry_unavailable("card registry unavailable")
        })?;
    Ok(PrincipalId::new(id))
}
