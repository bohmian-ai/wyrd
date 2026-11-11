//! Repository Card families and their selected policy version.
//!
//! A Repository family is the Cards of kind `Repository` sharing one name;
//! its stable provider identity is `spec.repository_id`. At most one version
//! of a family is selected (`wyrd.cards.selected_at`). A declaration whose
//! root is a Repository records its target and the version selected when it
//! was written on its operation row, and [`settle_repository_selection`]
//! decides it exactly once.

use sqlx::FromRow;
use uuid::Uuid;
use wyrd_semver::VersionBlock;
use wyrd_spec::envelope::{CardKind, RepositoryStatus};
use wyrd_spec::error::WyrdError;
use wyrd_spec::ids::{CardName, CardUid};
use wyrd_spec::registry::DeclarationOperationId;

use super::registry_db_error;
use super::version_sql::lock_version_line;
use crate::TenantConn;

/// The established family name of a provider repository, if any.
///
/// # Errors
/// Returns `WYRD_REGISTRY_503_REGISTRY_UNAVAILABLE` when the read fails.
pub async fn repository_family_name(
    conn: &mut TenantConn<'_>,
    repository_id: &str,
) -> Result<Option<String>, WyrdError> {
    sqlx::query_scalar(
        "SELECT name FROM wyrd.cards
          WHERE kind = 'Repository' AND spec->>'repository_id' = $1
            AND status NOT IN ('deleted', 'failed', 'expired')
          ORDER BY created_at LIMIT 1",
    )
    .bind(repository_id)
    .fetch_optional(&mut **conn.transaction())
    .await
    .map_err(registry_db_error)
}

/// The provider repository id a Repository family name already belongs to.
///
/// # Errors
/// Returns `WYRD_REGISTRY_503_REGISTRY_UNAVAILABLE` when the read fails.
pub async fn repository_family_identity(
    conn: &mut TenantConn<'_>,
    name: &CardName,
) -> Result<Option<String>, WyrdError> {
    sqlx::query_scalar(
        "SELECT spec->>'repository_id' FROM wyrd.cards
          WHERE kind = 'Repository' AND name = $1
            AND status NOT IN ('deleted', 'failed', 'expired')
          ORDER BY created_at LIMIT 1",
    )
    .bind(name.as_str())
    .fetch_optional(&mut **conn.transaction())
    .await
    .map_err(registry_db_error)
}

/// The selected version of a Repository family, if one is selected.
///
/// # Errors
/// Returns `WYRD_REGISTRY_503_REGISTRY_UNAVAILABLE` when the read fails.
pub async fn selected_repository(
    conn: &mut TenantConn<'_>,
    name: &CardName,
) -> Result<Option<CardUid>, WyrdError> {
    let uid: Option<Uuid> = sqlx::query_scalar(
        "SELECT card_uid FROM wyrd.cards
          WHERE kind = 'Repository' AND name = $1 AND selected_at IS NOT NULL",
    )
    .bind(name.as_str())
    .fetch_optional(&mut **conn.transaction())
    .await
    .map_err(registry_db_error)?;
    uid.map(CardUid::from_uuid)
        .transpose()
        .map_err(WyrdError::from_card_uid_error)
}

/// Record a declaration's target Repository version and the version that
/// was selected when it was written, in the declaration transaction.
///
/// # Errors
/// Returns `WYRD_REGISTRY_503_REGISTRY_UNAVAILABLE` when the write fails.
pub async fn record_repository_selection(
    conn: &mut TenantConn<'_>,
    operation_id: DeclarationOperationId,
    target: &CardUid,
    expected: Option<&CardUid>,
) -> Result<(), WyrdError> {
    sqlx::query(
        "UPDATE wyrd.card_declaration_operations
            SET selection = 'pending', selection_target_uid = $2, selection_expected_uid = $3
          WHERE operation_id = $1",
    )
    .bind(operation_id.as_uuid())
    .bind(target.as_uuid())
    .bind(expected.map(CardUid::as_uuid))
    .execute(&mut **conn.transaction())
    .await
    .map_err(registry_db_error)?;
    Ok(())
}

/// The decided or pending state of a declaration's Repository selection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RepositorySelection {
    /// The declaration has no Repository root.
    None,
    /// A Card the declaration owns is not Active yet.
    Pending,
    /// The target is the family's selected version.
    Selected,
    /// Another declaration changed the selection first; `current` is the
    /// version it selected.
    Conflict {
        /// The family's selected version when this declaration settled.
        current: Option<CardUid>,
    },
}

#[derive(FromRow)]
struct SelectionRow {
    selection: Option<String>,
    selection_target_uid: Option<Uuid>,
    selection_expected_uid: Option<Uuid>,
    name: Option<String>,
    ready: bool,
}

/// Settle a declaration's Repository selection, at most once.
///
/// Locks the operation row. A settled operation returns its stored outcome
/// and changes nothing. Otherwise, once every Card the operation owns and
/// its target are Active, it locks the family line and compares the
/// family's selected version with the target and the expected prior
/// version: the target already selected is reused; the expected version
/// still selected is replaced by the target; anything else is a conflict.
/// The decision is stored on the operation and commits with the caller's
/// transaction.
///
/// # Errors
/// Returns `WYRD_REGISTRY_503_REGISTRY_UNAVAILABLE` when a read or write
/// fails or the target names no Repository family.
pub async fn settle_repository_selection(
    conn: &mut TenantConn<'_>,
    operation_id: DeclarationOperationId,
) -> Result<RepositorySelection, WyrdError> {
    let row: Option<SelectionRow> = sqlx::query_as(
        "SELECT o.selection, o.selection_target_uid, o.selection_expected_uid, t.name,
                (t.status = 'active' AND NOT EXISTS (
                    SELECT 1 FROM wyrd.cards c
                     WHERE c.declaration_operation_id = o.operation_id
                       AND c.status <> 'active')) AS ready
           FROM wyrd.card_declaration_operations o
           LEFT JOIN wyrd.cards t ON t.card_uid = o.selection_target_uid
          WHERE o.operation_id = $1
            FOR UPDATE OF o",
    )
    .bind(operation_id.as_uuid())
    .fetch_optional(&mut **conn.transaction())
    .await
    .map_err(registry_db_error)?;
    let Some(row) = row else {
        return Ok(RepositorySelection::None);
    };
    let (Some(state), Some(target)) = (row.selection.as_deref(), row.selection_target_uid) else {
        return Ok(RepositorySelection::None);
    };
    let name = row
        .name
        .as_deref()
        .map(CardName::new)
        .transpose()
        .ok()
        .flatten()
        .ok_or_else(|| registry_db_error("repository selection target has no family"))?;
    match state {
        "selected" => return Ok(RepositorySelection::Selected),
        "conflict" => {
            return Ok(RepositorySelection::Conflict {
                current: selected_repository(conn, &name).await?,
            });
        }
        _ if !row.ready => return Ok(RepositorySelection::Pending),
        _ => {}
    }
    lock_version_line(conn, CardKind::Repository, &name).await?;
    let current = selected_repository(conn, &name).await?;
    let current_uuid = current.as_ref().map(CardUid::as_uuid);
    let outcome = if current_uuid == Some(target) {
        RepositorySelection::Selected
    } else if current_uuid == row.selection_expected_uid {
        sqlx::query(
            "UPDATE wyrd.cards SET selected_at = NULL
              WHERE kind = 'Repository' AND name = $1 AND selected_at IS NOT NULL",
        )
        .bind(name.as_str())
        .execute(&mut **conn.transaction())
        .await
        .map_err(registry_db_error)?;
        sqlx::query(
            "UPDATE wyrd.cards SET selected_at = statement_timestamp() WHERE card_uid = $1",
        )
        .bind(target)
        .execute(&mut **conn.transaction())
        .await
        .map_err(registry_db_error)?;
        RepositorySelection::Selected
    } else {
        RepositorySelection::Conflict { current }
    };
    let stored = if outcome == RepositorySelection::Selected {
        "selected"
    } else {
        "conflict"
    };
    sqlx::query(
        "UPDATE wyrd.card_declaration_operations SET selection = $2 WHERE operation_id = $1",
    )
    .bind(operation_id.as_uuid())
    .bind(stored)
    .execute(&mut **conn.transaction())
    .await
    .map_err(registry_db_error)?;
    Ok(outcome)
}

#[derive(FromRow)]
struct RepositoryStatusRow {
    selected: bool,
    selected_version: Option<String>,
    full_name: Option<String>,
}

/// Read the server-managed selection view of a Card version; `None` for a
/// Card that is not a Repository.
///
/// `full_name` is what the tenant's connection last reported for the
/// Repository's id, and `None` once the connection no longer reaches it.
///
/// # Errors
/// Returns `WYRD_REGISTRY_503_REGISTRY_UNAVAILABLE` when the read fails, and
/// `WYRD_REGISTRY_400_INVALID_VERSION_BLOCK` for a stored version that no
/// longer parses.
pub async fn repository_status(
    conn: &mut TenantConn<'_>,
    card_uid: &CardUid,
) -> Result<Option<RepositoryStatus>, WyrdError> {
    let row: Option<RepositoryStatusRow> = sqlx::query_as(
        "SELECT c.selected_at IS NOT NULL AS selected, s.version AS selected_version,
                (SELECT r->>'full_name'
                   FROM wyrd.github_installations g,
                        jsonb_array_elements(g.repositories) r
                  WHERE r->>'repository_id' = c.spec->>'repository_id'
                  LIMIT 1) AS full_name
           FROM wyrd.cards c
           LEFT JOIN wyrd.cards s
             ON s.kind = 'Repository' AND s.name = c.name AND s.selected_at IS NOT NULL
          WHERE c.card_uid = $1 AND c.kind = 'Repository'",
    )
    .bind(card_uid.as_uuid())
    .fetch_optional(&mut **conn.transaction())
    .await
    .map_err(registry_db_error)?;
    row.map(|row| {
        Ok(RepositoryStatus {
            selected: row.selected,
            selected_version: row
                .selected_version
                .map(VersionBlock::parse)
                .transpose()
                .map_err(|e| WyrdError::registry_invalid_version_block(e.to_string()))?,
            full_name: row.full_name,
        })
    })
    .transpose()
}
