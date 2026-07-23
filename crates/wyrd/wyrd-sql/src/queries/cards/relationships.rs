//! Tenant-scoped persistence for server-derived CardRef relationships.
#![deny(missing_docs)]

// raw-query grep allowlist: relationship resolution uses runtime CardRef
// cardinality and FOR SHARE locking, so these tenant-bound statements remain
// explicit until the SQLx offline cache supports the relationship schema.

use uuid::Uuid;
use wyrd_spec::error::WyrdError;
use wyrd_spec::ids::CardUid;
use wyrd_spec::reference::CardRef;

use crate::tenant_conn::TenantConn;

const RELATION_OUTBOUND: &str = "outbound";

/// Recheck exact external references while holding row locks until registration commits.
///
/// The preflight resolver runs in a separate transaction. This second check is the
/// write-time authority: `FOR SHARE` prevents a concurrent lifecycle update or delete
/// from invalidating the dependency between validation and the relationship insert.
pub async fn recheck_active_card_refs(
    conn: &mut TenantConn<'_>,
    refs: &[CardRef],
) -> Result<Vec<(CardRef, CardUid)>, WyrdError> {
    let mut resolved = Vec::with_capacity(refs.len());
    for card_ref in refs {
        let space = card_ref.space.as_ref().ok_or_else(|| {
            WyrdError::registry_invalid_card_spec(
                "CardRef.space is required at the registry boundary",
            )
        })?;
        let uid = sqlx::query_scalar::<_, Uuid>(
            "SELECT card_uid FROM wyrd.cards \
             WHERE data_tenant_id = wyrd.current_tenant() \
               AND kind = $1 AND space = $2 AND name = $3 AND version = $4 \
               AND status = 'active' \
             FOR SHARE",
        )
        .bind(card_ref.kind.wire_name())
        .bind(space.as_str())
        .bind(card_ref.name.as_str())
        .bind(card_ref.version.as_str())
        .fetch_optional(&mut **conn.transaction())
        .await
        .map_err(registry_db_error)?;
        let Some(uid) = uid else {
            return Err(unresolved_dependency(card_ref));
        };
        let uid = CardUid::from_uuid(uid).map_err(WyrdError::from_card_uid_error)?;
        let mut resolved_ref = card_ref.clone();
        resolved_ref.uid = Some(uid.clone());
        resolved.push((resolved_ref, uid));
    }
    Ok(resolved)
}

/// Persist exact UID-bearing outbound references in the caller's registration transaction.
pub async fn persist_outbound_relationships(
    conn: &mut TenantConn<'_>,
    card_uid: &CardUid,
    refs: &[CardRef],
) -> Result<(), WyrdError> {
    for card_ref in refs {
        let space = card_ref.space.as_ref().ok_or_else(|| {
            WyrdError::registry_invalid_card_spec(
                "CardRef.space is required at the registry boundary",
            )
        })?;
        let target_uid = card_ref
            .uid
            .as_ref()
            .ok_or_else(|| unresolved_dependency(card_ref))?;
        sqlx::query(
            "INSERT INTO wyrd.card_relationships \
                (card_uid, data_tenant_id, relation_kind, target_kind, target_space, \
                 target_name, target_version, target_uid) \
             VALUES ($1, wyrd.current_tenant(), $2, $3, $4, $5, $6, $7) \
             ON CONFLICT DO NOTHING",
        )
        .bind(card_uid.as_uuid())
        .bind(RELATION_OUTBOUND)
        .bind(card_ref.kind.wire_name())
        .bind(space.as_str())
        .bind(card_ref.name.as_str())
        .bind(card_ref.version.as_str())
        .bind(target_uid.as_uuid())
        .execute(&mut **conn.transaction())
        .await
        .map_err(registry_db_error)?;
    }
    Ok(())
}

/// Load active cards that point at `card_uid` through server-derived edges.
///
/// Deleted source cards are excluded so the returned inbound relationship
/// projection reflects the live registry graph rather than tombstone history.
pub async fn inbound_relationships(
    conn: &mut TenantConn<'_>,
    card_uid: &CardUid,
) -> Result<Vec<String>, WyrdError> {
    let rows = sqlx::query_as::<_, (String, String, String, String, uuid::Uuid)>(
        "SELECT source.kind, source.space, source.name, source.version, source.card_uid \
           FROM wyrd.card_relationships relationship \
           JOIN wyrd.cards source \
             ON source.data_tenant_id = relationship.data_tenant_id \
            AND source.card_uid = relationship.card_uid \
          WHERE relationship.data_tenant_id = wyrd.current_tenant() \
            AND relationship.target_uid = $1 \
            AND source.status <> 'deleted' \
          ORDER BY source.space, source.kind, source.name, source.version, source.card_uid",
    )
    .bind(card_uid.as_uuid())
    .fetch_all(&mut **conn.transaction())
    .await
    .map_err(registry_db_error)?;

    rows.into_iter()
        .map(|(kind, space, name, version, uid)| {
            let card_ref = CardRef {
                kind: wyrd_spec::envelope::CardKind::from_wire_name(&kind).ok_or_else(|| {
                    WyrdError::registry_invalid_card_spec(format!(
                        "stored relationship source has invalid kind {kind:?}"
                    ))
                })?,
                name: wyrd_spec::ids::CardName::new(name)
                    .map_err(|error| WyrdError::registry_invalid_card_spec(error.to_string()))?,
                version: version.parse().map_err(|error| {
                    WyrdError::registry_invalid_card_spec(format!(
                        "stored relationship source has invalid version: {error}"
                    ))
                })?,
                space: Some(
                    wyrd_spec::ids::SpaceName::new(space).map_err(|error| {
                        WyrdError::registry_invalid_card_spec(error.to_string())
                    })?,
                ),
                uid: Some(CardUid::from_uuid(uid).map_err(WyrdError::from_card_uid_error)?),
            };
            Ok(card_ref.to_string())
        })
        .collect()
}

fn unresolved_dependency(card_ref: &CardRef) -> WyrdError {
    let identity = card_ref.to_string();
    WyrdError::RegistryUnresolvedDependency {
        message: format!("card dependency {identity} was not found or is not Active"),
        details: serde_json::json!({ "card_ref": identity }),
    }
}

fn registry_db_error(error: impl std::fmt::Display) -> WyrdError {
    tracing::error!(%error, "card relationship database operation failed");
    WyrdError::registry_unavailable("card registry unavailable")
}
