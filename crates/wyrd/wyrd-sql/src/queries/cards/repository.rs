//! Repository Card families and their selected policy version.
//!
//! A Repository family is the Cards of kind `Repository` sharing one name;
//! its stable provider identity is `spec.repository_id`. At most one version
//! of a family is selected (`wyrd.cards.selected_at`). A declaration whose
//! root is a Repository records its target and the version selected when it
//! was written on its operation row, and [`settle_repository_selection`]
//! decides it exactly once.

// raw-query grep allowlist: Repository selection statements post-date the sqlx
// offline cache; run `mise run sqlx:prepare` to promote them to macros.

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

/// The selected version of a Repository family and the selection change
/// that selected it.
///
/// `occurrence` is private to the selection owner: it distinguishes two
/// selections of one version (A, then B, then A again), which the version
/// alone cannot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectedRepository {
    /// The selected Card version.
    pub uid: CardUid,
    /// The selection change that selected it: the selecting declaration's
    /// operation id, or a unique backfilled value.
    pub occurrence: Uuid,
}

/// The selected version of a Repository family, if one is selected.
///
/// # Errors
/// Returns `WYRD_REGISTRY_503_REGISTRY_UNAVAILABLE` when the read fails.
pub async fn selected_repository(
    conn: &mut TenantConn<'_>,
    name: &CardName,
) -> Result<Option<SelectedRepository>, WyrdError> {
    let row: Option<(Uuid, Uuid)> = sqlx::query_as(
        "SELECT card_uid, selection_occurrence FROM wyrd.cards
          WHERE kind = 'Repository' AND name = $1 AND selected_at IS NOT NULL",
    )
    .bind(name.as_str())
    .fetch_optional(&mut **conn.transaction())
    .await
    .map_err(registry_db_error)?;
    row.map(|(uid, occurrence)| {
        Ok(SelectedRepository {
            uid: CardUid::from_uuid(uid).map_err(WyrdError::from_card_uid_error)?,
            occurrence,
        })
    })
    .transpose()
}

/// Record a declaration's target Repository version and the selection that
/// was current when it was written, in the declaration transaction.
///
/// The caller holds the family's version line, so `expected` is the
/// selection no settle can change before this transaction commits.
///
/// # Errors
/// Returns `WYRD_REGISTRY_503_REGISTRY_UNAVAILABLE` when the write fails.
pub async fn record_repository_selection(
    conn: &mut TenantConn<'_>,
    operation_id: DeclarationOperationId,
    target: &CardUid,
    expected: Option<&SelectedRepository>,
) -> Result<(), WyrdError> {
    sqlx::query(
        "UPDATE wyrd.card_declaration_operations
            SET selection = 'pending', selection_target_uid = $2, selection_expected_uid = $3,
                selection_expected_occurrence = $4
          WHERE operation_id = $1",
    )
    .bind(operation_id.as_uuid())
    .bind(target.as_uuid())
    .bind(expected.map(|expected| expected.uid.as_uuid()))
    .bind(expected.map(|expected| expected.occurrence))
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
    /// A member of the declaration's exact graph is not Active yet.
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

/// A declaration operation's selection state, read under its row lock.
#[derive(FromRow)]
struct SelectionRow {
    /// `pending`, `selected` or `conflict`; `None` without a Repository root.
    selection: Option<String>,
    /// The Repository version the declaration selects.
    selection_target_uid: Option<Uuid>,
    /// The version selected when the declaration was written.
    selection_expected_uid: Option<Uuid>,
    /// The selection change that selected `selection_expected_uid` then.
    selection_expected_occurrence: Option<Uuid>,
    /// The target's family name.
    name: Option<String>,
    /// Whether the target and every member of the exact graph are Active.
    ready: bool,
}

/// Lock the pending Repository selections whose exact graph includes
/// `card_uid`, in operation order, and return their operations.
///
/// Membership is the declaration's persisted outcome set (its replay seed),
/// which includes reused Cards another declaration created, plus the
/// operation that created the Card and any targeting it. A Card's activation
/// takes these locks after it locks its Card row, and every settle of the
/// same operation takes the operation lock first, so the activation that
/// makes the last member Active observes every other member's committed
/// state and settles in its own transaction. A declaration still writing
/// holds its reused Pending members (see [`hold_pending_member`]), so it has
/// committed, and is found here, before their activation reads.
///
/// # Errors
/// Returns `WYRD_REGISTRY_503_REGISTRY_UNAVAILABLE` when the read fails.
pub async fn lock_card_repository_selections(
    conn: &mut TenantConn<'_>,
    card_uid: &CardUid,
) -> Result<Vec<DeclarationOperationId>, WyrdError> {
    // ponytail: scans the pending selections of the tenant, which are few;
    // index the seed membership if pending declarations ever accumulate.
    let operations: Vec<Uuid> = sqlx::query_scalar(
        "SELECT o.operation_id FROM wyrd.card_declaration_operations o
          WHERE o.selection = 'pending'
            AND (o.selection_target_uid = $1
                 OR o.operation_id = (SELECT declaration_operation_id FROM wyrd.cards
                                       WHERE card_uid = $1)
                 OR o.stored_response->'outcomes'
                    @> jsonb_build_array(jsonb_build_object(
                           'card_ref', jsonb_build_object('uid', $1::text))))
          ORDER BY o.operation_id
            FOR UPDATE OF o",
    )
    .bind(card_uid.as_uuid())
    .fetch_all(&mut **conn.transaction())
    .await
    .map_err(registry_db_error)?;
    Ok(operations
        .into_iter()
        .map(DeclarationOperationId::new)
        .collect())
}

/// Hold a reused Pending Card for the rest of a declaration's write
/// transaction.
///
/// A declaration reads its members' states when it settles in its own write
/// transaction, before its operation is visible to anyone else. Holding the
/// reused Card `FOR SHARE` makes that Card's activation wait for the
/// declaration to commit and then find it (see
/// [`lock_card_repository_selections`]), so a member activated meanwhile is
/// never missed by both sides. The caller takes it before any Repository
/// family lock, so no lock holder waits on it in reverse. A Card that is no
/// longer Pending holds nothing.
///
/// # Errors
/// Returns `WYRD_REGISTRY_503_REGISTRY_UNAVAILABLE` when the lock fails.
pub async fn hold_pending_member(
    conn: &mut TenantConn<'_>,
    card_uid: &CardUid,
) -> Result<(), WyrdError> {
    sqlx::query("SELECT 1 FROM wyrd.cards WHERE card_uid = $1 AND status = 'pending' FOR SHARE")
        .bind(card_uid.as_uuid())
        .execute(&mut **conn.transaction())
        .await
        .map_err(registry_db_error)?;
    Ok(())
}

/// Settle a declaration's Repository selection, at most once.
///
/// Locks the operation row. A settled operation returns its stored outcome
/// and changes nothing. Otherwise, once its target and every member of its
/// exact graph are Active — the Cards in its persisted outcome set, reused
/// ones included, and the Cards it created — it locks the family line and
/// compares the family's selection with the target and the selection the
/// declaration was written from: the target already selected is reused
/// unchanged; the expected version still selected by the same selection
/// change is replaced by the target, whose occurrence becomes this
/// operation; anything else, including a later reselection of the expected
/// version, is a conflict. The decision is stored on the operation and
/// commits with the caller's transaction.
///
/// # Errors
/// Returns `WYRD_REGISTRY_503_REGISTRY_UNAVAILABLE` when a read or write
/// fails or the target names no Repository family.
pub async fn settle_repository_selection(
    conn: &mut TenantConn<'_>,
    operation_id: DeclarationOperationId,
) -> Result<RepositorySelection, WyrdError> {
    let row: Option<SelectionRow> = sqlx::query_as(
        "SELECT o.selection, o.selection_target_uid, o.selection_expected_uid,
                o.selection_expected_occurrence, t.name,
                COALESCE(t.status = 'active' AND NOT EXISTS (
                    SELECT 1 FROM wyrd.cards c
                     WHERE c.status <> 'active'
                       AND (c.declaration_operation_id = o.operation_id
                            OR c.card_uid IN (
                                SELECT (outcome->'card_ref'->>'uid')::uuid
                                  FROM jsonb_array_elements(
                                           COALESCE(o.stored_response->'outcomes', '[]'))
                                       AS outcome))), false) AS ready
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
                current: selected_repository(conn, &name)
                    .await?
                    .map(|current| current.uid),
            });
        }
        _ if !row.ready => return Ok(RepositorySelection::Pending),
        _ => {}
    }
    lock_version_line(conn, CardKind::Repository, &name).await?;
    let current = selected_repository(conn, &name).await?;
    let current_pair = current
        .as_ref()
        .map(|current| (current.uid.as_uuid(), current.occurrence));
    let expected_pair = row
        .selection_expected_uid
        .zip(row.selection_expected_occurrence);
    let outcome = if current_pair.is_some_and(|(uid, _)| uid == target) {
        RepositorySelection::Selected
    } else if current_pair == expected_pair {
        sqlx::query(
            "UPDATE wyrd.cards SET selected_at = NULL, selection_occurrence = NULL
              WHERE kind = 'Repository' AND name = $1 AND selected_at IS NOT NULL",
        )
        .bind(name.as_str())
        .execute(&mut **conn.transaction())
        .await
        .map_err(registry_db_error)?;
        sqlx::query(
            "UPDATE wyrd.cards SET selected_at = statement_timestamp(), selection_occurrence = $2
              WHERE card_uid = $1",
        )
        .bind(target)
        .bind(operation_id.as_uuid())
        .execute(&mut **conn.transaction())
        .await
        .map_err(registry_db_error)?;
        RepositorySelection::Selected
    } else {
        RepositorySelection::Conflict {
            current: current.map(|current| current.uid),
        }
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

/// A Card's Repository selection view as read from its row.
#[derive(FromRow)]
struct RepositoryStatusRow {
    /// Whether this version is its family's selected version.
    selected: bool,
    /// The family's selected version, if any.
    selected_version: Option<String>,
    /// The connected `owner/name` for the Repository's id, if reachable.
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
