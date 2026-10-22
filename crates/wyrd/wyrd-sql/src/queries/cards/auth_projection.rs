//! Service and Agent principal projection for composite registration.
#![deny(missing_docs)]

use serde_json::Value as JsonValue;
use uuid::Uuid;
use wyrd_runtime::builtin_roles::DEFAULT_CARD_ROLE;
use wyrd_runtime::principal::PrincipalId;
use wyrd_semver::{VersionBlock, VersionSpec};
use wyrd_spec::auth::Attribution;
use wyrd_spec::envelope::{Card, Spec};
use wyrd_spec::error::WyrdError;
use wyrd_spec::ids::CardUid;
use wyrd_spec::reference::CardRef;

use crate::queries::auth::{grant_role_to_service_account, role_by_name};
use crate::tenant_conn::TenantConn;

const UPSERT_SQL: &str = r"
INSERT INTO wyrd.auth_service_accounts
    (id, data_tenant_id, principal_kind, card_kind, card_uid,
     name, version, card_ref, description, status,
     sub_principal_id, act_principal_id)
VALUES (gen_random_uuid(), wyrd.current_tenant(), $1, $2, $3,
        $4, $5, $6, $7, 'active', $8, $9)
ON CONFLICT (data_tenant_id, principal_kind, card_kind, card_uid)
DO UPDATE SET card_ref = EXCLUDED.card_ref,
    name = EXCLUDED.name, version = EXCLUDED.version,
    description = EXCLUDED.description, updated_at = now()
RETURNING id, (xmax = 0) AS inserted
";

/// Upsert the principal row backing a newly registered Service or Agent card.
///
/// The first projection of a Card also grants the built-in
/// [`DEFAULT_CARD_ROLE`] in the caller's transaction, so a key issued for the
/// principal can emit and verify its own evidence. A re-registration only
/// refreshes the row's Card projection: it never grants the Role again, so an
/// administrator's revocation stands, and keeps the creating `attribution`.
/// `xmax = 0` identifies the freshly inserted row of an
/// `INSERT ... ON CONFLICT DO UPDATE`.
///
/// # Errors
///
/// Returns `internal` for a non-Service/Agent Card or an unresolved version, and
/// `registry_unavailable` when Postgres rejects the upsert or grant.
#[tracing::instrument(skip(conn), fields(operation = "card.service_account.upsert"))]
pub async fn upsert_service_account_from_card(
    conn: &mut TenantConn<'_>,
    card_uid: &CardUid,
    card: &Card,
    attribution: Attribution,
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
        uid: Some(card_uid.clone()),
    };
    let card_ref_json: JsonValue =
        serde_json::to_value(card_ref).map_err(WyrdError::from_spec_serialization)?;
    let (id, inserted) = sqlx::query_as::<_, (Uuid, bool)>(UPSERT_SQL)
        .bind(principal_kind)
        .bind(card.kind.wire_name())
        .bind(card_uid.as_uuid())
        .bind(card.metadata.name.as_str())
        .bind(version.as_str())
        .bind(card_ref_json)
        .bind(description)
        .bind(attribution.sub_principal_id.as_uuid())
        .bind(attribution.act_principal_id.as_uuid())
        .fetch_one(&mut **conn.transaction())
        .await
        .map_err(|error| {
            tracing::error!(%error, "service-account projection failed");
            WyrdError::registry_unavailable("card registry unavailable")
        })?;
    if inserted {
        grant_default_role(conn, id).await?;
    }
    Ok(PrincipalId::new(id))
}

/// Grants the tenant's built-in [`DEFAULT_CARD_ROLE`] to a newly projected principal.
///
/// Every provisioned tenant carries the built-in roles (seeded at
/// provisioning); a bare tenant without them, such as
/// a persistence-level fixture, projects the principal with no Role.
///
/// # Errors
///
/// Returns `registry_unavailable` when Postgres rejects the lookup or grant.
async fn grant_default_role(conn: &mut TenantConn<'_>, principal: Uuid) -> Result<(), WyrdError> {
    let unavailable = |error: sqlx::Error| {
        tracing::error!(%error, "default role grant failed");
        WyrdError::registry_unavailable("card registry unavailable")
    };
    let Some(role) = role_by_name(conn, DEFAULT_CARD_ROLE)
        .await
        .map_err(unavailable)?
    else {
        tracing::warn!("tenant has no built-in default role; principal projected without it");
        return Ok(());
    };
    grant_role_to_service_account(conn, principal, role.id)
        .await
        .map_err(unavailable)?;
    Ok(())
}
