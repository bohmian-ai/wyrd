//! Lookup queries for single `wyrd.cards` rows.
#![deny(missing_docs)]

use wyrd_semver::VersionBlock;
use wyrd_spec::envelope::CardKind;
use wyrd_spec::error::WyrdError;
use wyrd_spec::ids::{CardName, CardUid, SpaceName};

use crate::row_types::cards::{CardRow, ParsedCardRow};
use crate::tenant_conn::TenantConn;

const SELECT_BY_UID: &str = r#"
    SELECT card_uid, data_tenant_id, kind, space, name, version,
           spec, spec_hash, artifact_hash, labels, annotations,
           status, created_by, created_at, updated_at, card_blob_uri
    FROM wyrd.cards
    WHERE card_uid = $1 AND status != 'deleted'
      AND data_tenant_id = wyrd.current_tenant()
"#;

const SELECT_BY_REF: &str = r#"
    SELECT card_uid, data_tenant_id, kind, space, name, version,
           spec, spec_hash, artifact_hash, labels, annotations,
           status, created_by, created_at, updated_at, card_blob_uri
    FROM wyrd.cards
    WHERE kind = $1 AND space = $2 AND name = $3 AND version = $4
      AND status != 'deleted'
      AND data_tenant_id = wyrd.current_tenant()
"#;

/// Load one card by UID within the current tenant.
///
/// # Errors
/// Returns `WYRD_REGISTRY_404_CARD_NOT_FOUND` when no row matches.
/// Returns `WYRD_REGISTRY_400_INVALID_CARD_SPEC` when stored data fails re-parse.
#[tracing::instrument(skip(conn), fields(tenant_id = %conn.data_tenant_id(), card_uid = %uid))]
pub async fn get_card_by_uid(
    conn: &mut TenantConn<'_>,
    uid: &CardUid,
) -> Result<ParsedCardRow, WyrdError> {
    let row = sqlx::query_as::<_, CardRow>(SELECT_BY_UID)
        .bind(uid.as_uuid())
        .fetch_optional(&mut **conn.transaction())
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "card registry lookup failed");
            WyrdError::registry_unavailable("card registry unavailable")
        })?
        .ok_or_else(|| WyrdError::registry_card_not_found(format!("no card with uid {uid}")))?;
    ParsedCardRow::try_from(row).map_err(|e| {
        WyrdError::registry_invalid_card_spec(format!("stored card failed to parse: {e}"))
    })
}

/// Load one card by its `(kind, space, name, version)` identity tuple.
///
/// # Errors
/// Same error surface as [`get_card_by_uid`].
#[tracing::instrument(
    skip(conn),
    fields(
        tenant_id = %conn.data_tenant_id(),
        kind = ?kind,
        space = %space,
        name = %name,
        version = %version,
    ),
)]
pub async fn get_card_by_ref(
    conn: &mut TenantConn<'_>,
    kind: CardKind,
    space: &SpaceName,
    name: &CardName,
    version: &VersionBlock,
) -> Result<ParsedCardRow, WyrdError> {
    let row = sqlx::query_as::<_, CardRow>(SELECT_BY_REF)
        .bind(kind.wire_name())
        .bind(space.as_str())
        .bind(name.as_str())
        .bind(version.as_str())
        .fetch_optional(&mut **conn.transaction())
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "card registry lookup failed");
            WyrdError::registry_unavailable("card registry unavailable")
        })?
        .ok_or_else(|| {
            WyrdError::registry_card_not_found(format!(
                "no card {}/{}/{} @ {}",
                kind.wire_name(),
                space,
                name,
                version,
            ))
        })?;
    ParsedCardRow::try_from(row).map_err(|e| {
        WyrdError::registry_invalid_card_spec(format!("stored card failed to parse: {e}"))
    })
}

/// Find a card by identity without turning an absent row into a public 404.
pub async fn find_card_by_ref(
    conn: &mut TenantConn<'_>,
    kind: CardKind,
    space: &SpaceName,
    name: &CardName,
    version: &VersionBlock,
) -> Result<Option<ParsedCardRow>, WyrdError> {
    let row = sqlx::query_as::<_, CardRow>(SELECT_BY_REF)
        .bind(kind.wire_name())
        .bind(space.as_str())
        .bind(name.as_str())
        .bind(version.as_str())
        .fetch_optional(&mut **conn.transaction())
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "card registry lookup failed");
            WyrdError::registry_unavailable("card registry unavailable")
        })?;
    row.map(ParsedCardRow::try_from).transpose().map_err(|e| {
        WyrdError::registry_invalid_card_spec(format!("stored card failed to parse: {e}"))
    })
}
