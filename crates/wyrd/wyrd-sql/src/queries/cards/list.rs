//! Paginated list queries for `wyrd.cards`.
#![deny(missing_docs)]

use chrono::{DateTime, Utc};
use uuid::Uuid;
use wyrd_spec::envelope::CardKind;
use wyrd_spec::error::WyrdError;
use wyrd_spec::ids::{CardUid, SpaceName};

use crate::row_types::cards::{CardRow, CardStatus};
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

const LIST_BY_KIND: &str = r#"
    SELECT card_uid, data_tenant_id, kind, space, name, version,
           spec, spec_hash, artifact_hash, labels, annotations,
           status, created_by, created_at, updated_at
    FROM wyrd.cards
    WHERE kind = $1
      AND ($2::text IS NULL OR status = $2)
      AND ($3::timestamptz IS NULL OR (created_at, card_uid) > ($3, $4::uuid))
    ORDER BY created_at ASC, card_uid ASC
    LIMIT $5
"#;

const LIST_BY_SPACE: &str = r#"
    SELECT card_uid, data_tenant_id, kind, space, name, version,
           spec, spec_hash, artifact_hash, labels, annotations,
           status, created_by, created_at, updated_at
    FROM wyrd.cards
    WHERE space = $1
      AND ($2::text IS NULL OR status = $2)
      AND ($3::timestamptz IS NULL OR (created_at, card_uid) > ($3, $4::uuid))
    ORDER BY created_at ASC, card_uid ASC
    LIMIT $5
"#;

const LIST_BY_STATUS: &str = r#"
    SELECT card_uid, data_tenant_id, kind, space, name, version,
           spec, spec_hash, artifact_hash, labels, annotations,
           status, created_by, created_at, updated_at
    FROM wyrd.cards
    WHERE status = $1
      AND ($2::text IS NULL OR kind = $2)
      AND ($3::timestamptz IS NULL OR (created_at, card_uid) > ($3, $4::uuid))
    ORDER BY created_at ASC, card_uid ASC
    LIMIT $5
"#;

fn validate_cursor(cursor: &ListCursor) -> Result<(), WyrdError> {
    if cursor.limit == 0 || cursor.limit > MAX_LIST_LIMIT {
        return Err(WyrdError::registry_list_limit_out_of_range(
            cursor.limit,
            MAX_LIST_LIMIT,
        ));
    }
    Ok(())
}

/// List cards filtered by kind.
///
/// Pass `status = None` to include all lifecycle states; pass `Some(CardStatus::Active)`
/// to restrict to active cards only.
///
/// # Errors
/// Returns `WYRD_REG_400_LIST_LIMIT_OUT_OF_RANGE` when `cursor.limit` is out of range.
/// Returns `WYRD_REG_503_REGISTRY_UNAVAILABLE` on transient DB errors.
#[tracing::instrument(
    skip(conn),
    fields(tenant_id = %conn.data_tenant_id(), kind = ?kind, limit = cursor.limit),
)]
pub async fn list_cards_by_kind(
    conn: &mut TenantConn<'_>,
    kind: CardKind,
    status: Option<CardStatus>,
    cursor: ListCursor,
) -> Result<ListPage<CardRow>, WyrdError> {
    validate_cursor(&cursor)?;
    let after_created_at = cursor.after_created_at;
    let after_uid: Option<Uuid> = cursor.after_uid.as_ref().map(|u| u.as_uuid());
    let status_filter = status.as_ref().map(|s| s.as_db_str());
    let limit = cursor.limit as i64 + 1;

    let rows = sqlx::query_as::<_, CardRow>(LIST_BY_KIND)
        .bind(kind.wire_name())
        .bind(status_filter)
        .bind(after_created_at)
        .bind(after_uid)
        .bind(limit)
        .fetch_all(&mut **conn.transaction())
        .await
        .map_err(|e| WyrdError::registry_unavailable(e.to_string()))?;

    Ok(build_page(rows, cursor))
}

/// List cards filtered by space.
///
/// # Errors
/// Returns `WYRD_REG_400_LIST_LIMIT_OUT_OF_RANGE` when `cursor.limit` is out of range.
/// Returns `WYRD_REG_503_REGISTRY_UNAVAILABLE` on transient DB errors.
#[tracing::instrument(
    skip(conn),
    fields(tenant_id = %conn.data_tenant_id(), space = %space, limit = cursor.limit),
)]
pub async fn list_cards_by_space(
    conn: &mut TenantConn<'_>,
    space: &SpaceName,
    status: Option<CardStatus>,
    cursor: ListCursor,
) -> Result<ListPage<CardRow>, WyrdError> {
    validate_cursor(&cursor)?;
    let after_created_at = cursor.after_created_at;
    let after_uid: Option<Uuid> = cursor.after_uid.as_ref().map(|u| u.as_uuid());
    let status_filter = status.as_ref().map(|s| s.as_db_str());
    let limit = cursor.limit as i64 + 1;

    let rows = sqlx::query_as::<_, CardRow>(LIST_BY_SPACE)
        .bind(space.as_str())
        .bind(status_filter)
        .bind(after_created_at)
        .bind(after_uid)
        .bind(limit)
        .fetch_all(&mut **conn.transaction())
        .await
        .map_err(|e| WyrdError::registry_unavailable(e.to_string()))?;

    Ok(build_page(rows, cursor))
}

/// List cards filtered by status, optionally also by kind.
///
/// # Errors
/// Returns `WYRD_REG_400_LIST_LIMIT_OUT_OF_RANGE` when `cursor.limit` is out of range.
/// Returns `WYRD_REG_503_REGISTRY_UNAVAILABLE` on transient DB errors.
#[tracing::instrument(
    skip(conn),
    fields(tenant_id = %conn.data_tenant_id(), status = ?status, limit = cursor.limit),
)]
pub async fn list_cards_by_status(
    conn: &mut TenantConn<'_>,
    status: CardStatus,
    kind: Option<CardKind>,
    cursor: ListCursor,
) -> Result<ListPage<CardRow>, WyrdError> {
    validate_cursor(&cursor)?;
    let after_created_at = cursor.after_created_at;
    let after_uid: Option<Uuid> = cursor.after_uid.as_ref().map(|u| u.as_uuid());
    let kind_filter = kind.as_ref().map(|k| k.wire_name());
    let limit = cursor.limit as i64 + 1;

    let rows = sqlx::query_as::<_, CardRow>(LIST_BY_STATUS)
        .bind(status.as_db_str())
        .bind(kind_filter)
        .bind(after_created_at)
        .bind(after_uid)
        .bind(limit)
        .fetch_all(&mut **conn.transaction())
        .await
        .map_err(|e| WyrdError::registry_unavailable(e.to_string()))?;

    Ok(build_page(rows, cursor))
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
