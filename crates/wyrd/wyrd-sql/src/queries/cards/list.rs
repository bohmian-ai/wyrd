//! Composable, keyset-paginated collection query for `wyrd.cards`.
#![deny(missing_docs)]
// raw-query grep allowlist: list/lookup uses QueryBuilder for dynamic filters and
// column projection. Every statement runs on a TenantConn (RLS) and filters
// `data_tenant_id = wyrd.current_tenant()`; run `mise run sqlx:prepare` to promote
// static shapes to macros.

use chrono::{DateTime, Utc};
use sqlx::{Postgres, QueryBuilder};
use wyrd_semver::VersionRange;
use wyrd_spec::envelope::CardKind;
use wyrd_spec::error::WyrdError;
use wyrd_spec::ids::{CardName, CardUid, SpaceName};
use wyrd_spec::query::{MetadataQuery, QueryFieldErrorDetail};

use crate::queries::cards::field_resolver::CardFieldResolver;
use crate::queries::cards::version_sql::push_bounds;
use crate::query::compile_query;
use crate::row_types::cards::{CARD_ROW_COLUMNS, CardRow, CardStatus};
use crate::tenant_conn::TenantConn;

/// Maximum page size for list queries.
pub const MAX_LIST_LIMIT: u32 = 200;

/// Pagination cursor for card list queries.
///
/// Keyset pagination on `(created_at ASC, card_uid ASC)`.
#[derive(Debug, Clone)]
pub struct ListCursor {
    /// Continue after this timestamp (`None` = start from the beginning).
    pub after_created_at: Option<DateTime<Utc>>,
    /// Continue after this card uid (paired with `after_created_at`).
    pub after_uid: Option<CardUid>,
    /// Page size (must be in `1..=MAX_LIST_LIMIT`).
    pub limit: u32,
}

/// One page of card rows.
#[derive(Debug, Clone)]
pub struct ListPage<T> {
    /// Rows in this page, ordered by `(created_at, card_uid)` ascending.
    pub items: Vec<T>,
    /// Cursor for the next page; `None` when exhausted.
    pub next: Option<ListCursor>,
}

/// Composable card collection query.
#[derive(Debug, Clone, Default)]
pub struct CardQuery {
    /// Restrict to one kind.
    pub kind: Option<CardKind>,
    /// Restrict to one space.
    pub space: Option<SpaceName>,
    /// Restrict to one card name.
    pub name: Option<CardName>,
    /// Restrict to versions within a range.
    pub version_range: Option<VersionRange>,
    /// Restrict to one lifecycle status. `None` excludes only deleted rows.
    pub status: Option<CardStatus>,
    /// Optional metadata filter over labels, annotations, and reserved columns.
    pub filter: Option<MetadataQuery>,
    /// Include pre-release versions. Default false excludes pre-releases.
    pub include_prerelease: bool,
}

fn validate_cursor(cursor: &ListCursor) -> Result<(), WyrdError> {
    if cursor.limit == 0 || cursor.limit > MAX_LIST_LIMIT {
        return Err(WyrdError::registry_list_limit_out_of_range(
            cursor.limit,
            MAX_LIST_LIMIT,
        ));
    }
    Ok(())
}

/// Run a composable, keyset-paginated card query within the caller's tenant.
///
/// # Errors
/// Returns list-limit, query-field, invalid-version, or registry-unavailable
/// errors depending on validation and database execution.
#[tracing::instrument(
    skip(conn, query),
    fields(
        tenant_id = %conn.data_tenant_id(),
        limit = cursor.limit,
        has_filter = query.filter.is_some(),
        has_kind = query.kind.is_some(),
        has_space = query.space.is_some(),
        has_name = query.name.is_some(),
        has_version_range = query.version_range.is_some(),
        has_status = query.status.is_some(),
    )
)]
pub async fn query_cards(
    conn: &mut TenantConn<'_>,
    query: &CardQuery,
    cursor: ListCursor,
) -> Result<ListPage<CardRow>, WyrdError> {
    validate_cursor(&cursor)?;

    // The metadata filter can compile to a regex predicate (SIMILAR TO / ~).
    // Postgres rejects trivially-invalid patterns but can run slowly on
    // pathological inputs from callers. 5s caps any single list query and
    // matches the server-level per-request deadline.
    sqlx::query("SET LOCAL statement_timeout = '5s'")
        .execute(&mut **conn.transaction())
        .await
        .map_err(|e| WyrdError::registry_unavailable(e.to_string()))?;

    let mut qb: QueryBuilder<Postgres> = QueryBuilder::new(format!(
        "SELECT {CARD_ROW_COLUMNS} FROM wyrd.cards WHERE data_tenant_id = wyrd.current_tenant()"
    ));
    push_card_filters(&mut qb, query)?;
    push_keyset_cursor(&mut qb, &cursor);
    qb.push(" ORDER BY created_at ASC, card_uid ASC LIMIT ");
    qb.push_bind(cursor.limit as i64 + 1);

    let rows = qb
        .build_query_as::<CardRow>()
        .fetch_all(&mut **conn.transaction())
        .await
        .map_err(map_query_db_error)?;

    Ok(build_page(rows, cursor))
}

/// Find an active card in a line whose spec hash matches.
///
/// Used during auto/scope registration to detect content-identical re-submits
/// before deciding whether to insert a new version row. Only non-deleted rows
/// are considered; the most recently registered version in the line is
/// returned when multiple rows share the same hash.
///
/// # Errors
/// Returns `WYRD_REG_503_REGISTRY_UNAVAILABLE` on database errors.
pub async fn find_card_by_spec_hash(
    conn: &mut TenantConn<'_>,
    kind: CardKind,
    space: &SpaceName,
    name: &CardName,
    spec_hash: &str,
) -> Result<Option<CardRow>, WyrdError> {
    let mut qb: QueryBuilder<Postgres> = QueryBuilder::new(format!(
        "SELECT {CARD_ROW_COLUMNS} FROM wyrd.cards \
         WHERE data_tenant_id = wyrd.current_tenant() AND status <> 'deleted' AND kind = "
    ));
    qb.push_bind(kind.wire_name());
    qb.push(" AND space = ").push_bind(space.as_str());
    qb.push(" AND name = ").push_bind(name.as_str());
    qb.push(" AND spec_hash = ").push_bind(spec_hash);
    qb.push(" ORDER BY version_major DESC, version_minor DESC, version_patch DESC LIMIT 1");

    qb.build_query_as::<CardRow>()
        .fetch_optional(&mut **conn.transaction())
        .await
        .map_err(|e| WyrdError::registry_unavailable(e.to_string()))
}

/// True when a card with this uid exists in the tenant, including deleted rows.
///
/// # Errors
/// Returns `WYRD_REG_503_REGISTRY_UNAVAILABLE` on database errors.
pub async fn check_uid_exists(conn: &mut TenantConn<'_>, uid: &CardUid) -> Result<bool, WyrdError> {
    let found: Option<(i32,)> = sqlx::query_as(
        "SELECT 1 FROM wyrd.cards \
         WHERE card_uid = $1 AND data_tenant_id = wyrd.current_tenant() LIMIT 1",
    )
    .bind(uid.as_uuid())
    .fetch_optional(&mut **conn.transaction())
    .await
    .map_err(|e| WyrdError::registry_unavailable(e.to_string()))?;
    Ok(found.is_some())
}

/// Distinct non-deleted space slugs in the tenant, sorted ascending.
///
/// # Errors
/// Returns `WYRD_REG_503_REGISTRY_UNAVAILABLE` on database errors or
/// `WYRD_REG_400_INVALID_CARD_SPEC` when a stored space is invalid.
pub async fn get_unique_spaces(conn: &mut TenantConn<'_>) -> Result<Vec<SpaceName>, WyrdError> {
    let rows: Vec<(String,)> = sqlx::query_as(
        "SELECT DISTINCT space FROM wyrd.cards \
         WHERE data_tenant_id = wyrd.current_tenant() AND status <> 'deleted' \
         ORDER BY space ASC",
    )
    .fetch_all(&mut **conn.transaction())
    .await
    .map_err(|e| WyrdError::registry_unavailable(e.to_string()))?;
    rows.into_iter()
        .map(|(space,)| {
            SpaceName::new(space).map_err(|e| WyrdError::registry_invalid_card_spec(e.to_string()))
        })
        .collect()
}

/// Append all `CardQuery` WHERE predicates to `qb`.
///
/// Does not touch ORDER BY, LIMIT, or the keyset cursor — those are the
/// caller's responsibility. Returns an error only when a version range cannot
/// be converted to SQL bounds.
fn push_card_filters(qb: &mut QueryBuilder<Postgres>, query: &CardQuery) -> Result<(), WyrdError> {
    match &query.status {
        Some(status) => {
            qb.push(" AND status = ").push_bind(status.as_db_str());
        }
        None => {
            qb.push(" AND status <> 'deleted'");
        }
    }
    if let Some(kind) = &query.kind {
        qb.push(" AND kind = ").push_bind(kind.wire_name());
    }
    if let Some(space) = &query.space {
        qb.push(" AND space = ").push_bind(space.as_str());
    }
    if let Some(name) = &query.name {
        qb.push(" AND name = ").push_bind(name.as_str());
    }
    if !query.include_prerelease {
        qb.push(" AND NOT version_is_prerelease");
    }
    if let Some(range) = &query.version_range {
        let bounds = range
            .to_bounds()
            .map_err(|e| WyrdError::registry_invalid_version_block(e.to_string()))?;
        push_bounds(qb, &bounds)?;
    }
    if let Some(filter) = &query.filter {
        qb.push(" AND ");
        compile_query(filter, &CardFieldResolver, qb)?;
    }
    Ok(())
}

/// Append the keyset continuation predicate for `(created_at, card_uid)` when
/// the cursor carries a prior-page position. No-ops when the cursor has no
/// prior position (first page).
fn push_keyset_cursor(qb: &mut QueryBuilder<Postgres>, cursor: &ListCursor) {
    if let (Some(after_ts), Some(after_uid)) =
        (cursor.after_created_at.as_ref(), cursor.after_uid.as_ref())
    {
        qb.push(" AND (created_at, card_uid) > (");
        qb.push_bind(after_ts.to_owned()).push(", ");
        qb.push_bind(after_uid.as_uuid()).push(")");
    }
}

fn map_query_db_error(e: sqlx::Error) -> WyrdError {
    if let Some(db) = e.as_database_error()
        && db.code().as_deref() == Some("2201B")
    {
        return WyrdError::query_invalid_field_detail(
            "regex rejected by the query engine",
            QueryFieldErrorDetail {
                reason: "regex_rejected",
                surface: Some("cards"),
                ..Default::default()
            },
        );
    }
    WyrdError::registry_unavailable(e.to_string())
}

fn build_page(mut rows: Vec<CardRow>, cursor: ListCursor) -> ListPage<CardRow> {
    let has_more = rows.len() > cursor.limit as usize;
    if has_more {
        rows.truncate(cursor.limit as usize);
    }
    let next = if has_more {
        rows.last().map(|last| {
            let last_uid = CardUid::from_uuid(last.card_uid)
                .expect("card_uid from DB always parses as UUIDv7");
            ListCursor {
                after_created_at: Some(last.created_at),
                after_uid: Some(last_uid),
                limit: cursor.limit,
            }
        })
    } else {
        None
    };
    ListPage { items: rows, next }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_cursor_rejects_zero_limit() {
        let cursor = ListCursor {
            after_created_at: None,
            after_uid: None,
            limit: 0,
        };
        let err = validate_cursor(&cursor).expect_err("limit 0 must be rejected");
        assert_eq!(err.code(), "WYRD_REG_400_LIST_LIMIT_OUT_OF_RANGE");
    }

    #[test]
    fn validate_cursor_rejects_overlimit() {
        let cursor = ListCursor {
            after_created_at: None,
            after_uid: None,
            limit: MAX_LIST_LIMIT + 1,
        };
        let err = validate_cursor(&cursor).expect_err("limit > MAX must be rejected");
        assert_eq!(err.code(), "WYRD_REG_400_LIST_LIMIT_OUT_OF_RANGE");
    }

    #[test]
    fn validate_cursor_accepts_boundary_values() {
        for limit in [1, MAX_LIST_LIMIT] {
            let cursor = ListCursor {
                after_created_at: None,
                after_uid: None,
                limit,
            };
            validate_cursor(&cursor).expect("boundary limit must be accepted");
        }
    }
}
