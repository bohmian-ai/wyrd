//! Version-axis read queries for `wyrd.cards`.
#![deny(missing_docs)]

use sqlx::{Postgres, QueryBuilder};
use wyrd_semver::{VersionBlock, VersionRange};
use wyrd_spec::envelope::CardKind;
use wyrd_spec::error::WyrdError;
use wyrd_spec::ids::{CardName, SpaceName};

use crate::queries::cards::version_sql::push_bounds;
use crate::row_types::cards::{CARD_ROW_COLUMNS, CardRow, ParsedCardRow};
use crate::tenant_conn::TenantConn;

/// Return the latest stable card whose version falls within `range`.
///
/// # Errors
/// Returns `WYRD_REGISTRY_400_INVALID_VERSION_BLOCK` for non-representable ranges or
/// bounds overflow, `WYRD_REGISTRY_404_CARD_NOT_FOUND` when no stable row matches,
/// `WYRD_REGISTRY_400_INVALID_CARD_SPEC` when a stored row fails to parse, and
/// `WYRD_REGISTRY_503_REGISTRY_UNAVAILABLE` on database errors.
#[tracing::instrument(skip(conn), fields(tenant_id = %conn.data_tenant_id(), kind = ?kind, space = %space, name = %name, range = %range))]
pub async fn get_latest_card_by_range(
    conn: &mut TenantConn<'_>,
    kind: CardKind,
    space: &SpaceName,
    name: &CardName,
    range: &VersionRange,
) -> Result<ParsedCardRow, WyrdError> {
    // No statement_timeout: these reads hit idx_cards_version_latest (partial, bounded index)
    // and carry no user-supplied regex, so unbounded scan is not a concern. Add one if
    // these queries are ever exposed directly to user-supplied range inputs without a pre-check.
    let bounds = range
        .to_bounds()
        .map_err(|e| WyrdError::registry_invalid_version_block(e.to_string()))?;

    let mut qb: QueryBuilder<Postgres> = QueryBuilder::new(format!(
        "SELECT {CARD_ROW_COLUMNS} FROM wyrd.cards \
         WHERE data_tenant_id = wyrd.current_tenant() \
           AND status = 'active' AND NOT version_is_prerelease AND kind = "
    ));
    qb.push_bind(kind.wire_name());
    qb.push(" AND space = ").push_bind(space.as_str());
    qb.push(" AND name = ").push_bind(name.as_str());
    push_bounds(&mut qb, &bounds)?;
    qb.push(" ORDER BY version_major DESC, version_minor DESC, version_patch DESC LIMIT 1");

    let row = qb
        .build_query_as::<CardRow>()
        .fetch_optional(&mut **conn.transaction())
        .await
        .map_err(|e| WyrdError::registry_unavailable(e.to_string()))?
        .ok_or_else(|| {
            WyrdError::registry_card_not_found(format!(
                "no stable {}/{}/{} matching {range}",
                kind.wire_name(),
                space,
                name
            ))
        })?;

    ParsedCardRow::try_from(row).map_err(|e| {
        WyrdError::registry_invalid_card_spec(format!("stored card failed to parse: {e}"))
    })
}

/// List versions registered in a `(kind, space, name)` line, newest first.
///
/// # Errors
/// Returns `WYRD_REGISTRY_503_REGISTRY_UNAVAILABLE` on database errors or
/// `WYRD_REGISTRY_400_INVALID_VERSION_BLOCK` when a stored version fails to parse.
#[tracing::instrument(skip(conn), fields(tenant_id = %conn.data_tenant_id(), kind = ?kind, space = %space, name = %name, include_prerelease))]
pub async fn list_versions(
    conn: &mut TenantConn<'_>,
    kind: CardKind,
    space: &SpaceName,
    name: &CardName,
    include_prerelease: bool,
) -> Result<Vec<VersionBlock>, WyrdError> {
    let mut qb: QueryBuilder<Postgres> = QueryBuilder::new(
        "SELECT version FROM wyrd.cards \
         WHERE data_tenant_id = wyrd.current_tenant() \
           AND status <> 'deleted' AND kind = ",
    );
    qb.push_bind(kind.wire_name());
    qb.push(" AND space = ").push_bind(space.as_str());
    qb.push(" AND name = ").push_bind(name.as_str());
    if !include_prerelease {
        qb.push(" AND NOT version_is_prerelease");
    }
    qb.push(
        " ORDER BY version_major DESC, version_minor DESC, version_patch DESC, \
         version_is_prerelease ASC",
    );

    let rows = qb
        .build_query_as::<(String,)>()
        .fetch_all(&mut **conn.transaction())
        .await
        .map_err(|e| WyrdError::registry_unavailable(e.to_string()))?;

    rows.into_iter()
        .map(|(version,)| {
            VersionBlock::parse(version)
                .map_err(|e| WyrdError::registry_invalid_version_block(e.to_string()))
        })
        .collect()
}
