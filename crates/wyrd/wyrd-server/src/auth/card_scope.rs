//! Card-ref scope resolution for card-bound principals.

use std::collections::HashSet;
use std::str::FromStr;

use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use uuid::Uuid;
use wyrd_auth_issue::IssueError;
use wyrd_spec::DataTenantId;
use wyrd_spec::error::WyrdError;
use wyrd_spec::reference::{CardRef, CardRefScope};
use wyrd_sql::TenantConn;
use wyrd_sql::queries::auth::insert_audit_card_scope_mint;
use wyrd_sql::queries::cards::get_card_by_ref;

const MAX_SCOPE_DEPTH: usize = 16;
pub(crate) const MAX_SCOPE_CARDS: usize = 32;
const SCOPE_AUDIT_MEMBER_SUMMARY_LIMIT: usize = 16;

pub(crate) const MINT_KIND_API_KEY_EXCHANGE: &str = "api_key_exchange";
pub(crate) const MINT_KIND_REFRESH: &str = "refresh";
pub(crate) const MINT_KIND_DELEGATION: &str = "delegation";
pub(crate) const MINT_KIND_JWT_BEARER: &str = "jwt_bearer";

/// Resolve the observation-target scope for a card-bound principal.
///
/// The walk is tenant-scoped, fail-closed, cycle-guarded, and capped. Only
/// observation-target kinds are included and expanded.
pub(crate) async fn resolve_card_ref_scope(
    conn: &mut TenantConn<'_>,
    root: &CardRef,
) -> Result<CardRefScope, WyrdError> {
    let mut visited = HashSet::new();
    let mut members = Vec::new();
    let mut frontier = vec![(root.clone(), 0usize)];

    while let Some((card_ref, depth)) = frontier.pop() {
        if depth > MAX_SCOPE_DEPTH {
            return Err(too_large("depth", MAX_SCOPE_DEPTH, root));
        }
        let key = identity_key(&card_ref);
        if !visited.insert(key) {
            continue;
        }

        members.push(card_ref.clone());
        if members.len() > MAX_SCOPE_CARDS {
            return Err(too_large("cards", MAX_SCOPE_CARDS, root));
        }

        let row = get_card_by_ref(
            conn,
            card_ref.kind.clone(),
            &card_ref.space,
            &card_ref.name,
            &card_ref.version,
        )
        .await
        .map_err(|error| scope_resolution_error(error, root, &card_ref))?;

        for child in wyrd_spec::reference::scope_child_card_refs(&row.spec) {
            if child.kind.is_observation_target() {
                frontier.push((child, depth + 1));
            }
        }
    }

    Ok(CardRefScope::try_from_root_and_members(root, members))
}

/// Build the card identity key used for cycle detection and duplicate removal.
fn identity_key(card_ref: &CardRef) -> (String, String, String, String) {
    (
        card_ref.kind.wire_name().to_owned(),
        card_ref.space.to_string(),
        card_ref.name.to_string(),
        card_ref.version.to_string(),
    )
}

/// Build the stable `413` error for card-ref scope count or depth overflow.
fn too_large(limit_kind: &'static str, limit: usize, root: &CardRef) -> WyrdError {
    WyrdError::CardScopeTooLarge {
        message: format!("card_ref_scope exceeded {limit_kind} limit {limit}"),
        details: json!({
            "limit_kind": limit_kind,
            "limit": limit,
            "root": root.to_string(),
            "scope_mint_root": root.to_string(),
        }),
    }
}

/// Convert issuer scope-size failures into root-aware Wyrd errors.
pub(crate) fn issue_scope_error(error: IssueError, root: &CardRef) -> IssueErrorOrWyrd {
    match error {
        IssueError::CardScopeTooLarge { encoded_len, limit } => {
            IssueErrorOrWyrd::Wyrd(WyrdError::CardScopeTooLarge {
                message: format!(
                    "card_ref_scope encoded token length {encoded_len} exceeds {limit}"
                ),
                details: json!({
                    "encoded_len": encoded_len,
                    "limit": limit,
                    "root": root.to_string(),
                    "scope_mint_root": root.to_string(),
                }),
            })
        }
        other => IssueErrorOrWyrd::Issue(other),
    }
}

/// Result of mapping an issuer error at a card-bound mint site.
pub(crate) enum IssueErrorOrWyrd {
    Issue(IssueError),
    Wyrd(WyrdError),
}

/// Write the successful card-ref scope mint audit row on the mint transaction.
pub(crate) async fn write_scope_mint_success_audit(
    conn: &mut TenantConn<'_>,
    principal_id: Uuid,
    root: &CardRef,
    scope: &CardRefScope,
    request_id: &str,
    mint_kind: &str,
) -> Result<(), sqlx::Error> {
    let members = scope_member_strings(scope);
    let scope_hash = scope_hash(&members);
    insert_audit_card_scope_mint(
        conn,
        Uuid::new_v4(),
        Some(principal_id),
        mint_kind,
        root,
        request_id,
        "success",
        Some(
            i32::try_from(scope.len())
                .expect("MAX_SCOPE_CARDS invariant: scope count fits into i32"),
        ),
        Some(&scope_hash),
        scope_member_summary(&members),
        None,
        None,
    )
    .await
}

/// Best-effort write of a failed card-ref scope mint audit row on a fresh transaction.
pub(crate) async fn audit_scope_mint_failure_best_effort(
    pool: &PgPool,
    tenant_id: DataTenantId,
    request_id: &str,
    mint_kind: &str,
    error: &WyrdError,
) {
    let Some(root) = scope_failure_root(error) else {
        return;
    };
    match TenantConn::acquire(pool, tenant_id).await {
        Ok(mut conn) => {
            if let Err(audit_error) =
                write_scope_mint_failure_audit(&mut conn, &root, request_id, mint_kind, error).await
            {
                tracing::error!(
                    error = %audit_error,
                    root = %root,
                    reason = %error.code(),
                    "scope-mint failure audit write failed",
                );
                return;
            }
            if let Err(commit_error) = conn.commit().await {
                tracing::error!(
                    error = %commit_error,
                    root = %root,
                    reason = %error.code(),
                    "scope-mint failure audit commit failed",
                );
            }
        }
        Err(acquire_error) => {
            tracing::error!(
                error = %acquire_error,
                root = %root,
                reason = %error.code(),
                "scope-mint failure audit conn acquire failed",
            );
        }
    }
}

/// Write a failed card-ref scope mint audit row.
async fn write_scope_mint_failure_audit(
    conn: &mut TenantConn<'_>,
    root: &CardRef,
    request_id: &str,
    mint_kind: &str,
    error: &WyrdError,
) -> Result<(), sqlx::Error> {
    insert_audit_card_scope_mint(
        conn,
        Uuid::new_v4(),
        None,
        mint_kind,
        root,
        request_id,
        "failure",
        None,
        None,
        json!([]),
        Some(error.code()),
        Some(&error.to_string()),
    )
    .await
}

/// Attach scope-mint context to resolver errors that should be audited.
fn scope_resolution_error(error: WyrdError, root: &CardRef, unresolved: &CardRef) -> WyrdError {
    match error {
        WyrdError::RegistryCardNotFound { message, details } => WyrdError::RegistryCardNotFound {
            message,
            details: merge_scope_details(details, root, unresolved),
        },
        other => other,
    }
}

/// Merge root and unresolved child references into an existing error detail object.
fn merge_scope_details(details: Value, root: &CardRef, unresolved: &CardRef) -> Value {
    let mut details = match details {
        Value::Object(map) => map,
        _ => serde_json::Map::new(),
    };
    details.insert("scope_mint_root".to_owned(), json!(root.to_string()));
    details.insert(
        "unresolved_card_ref".to_owned(),
        json!(unresolved.to_string()),
    );
    Value::Object(details)
}

/// Extract the root card ref from a scope-mint failure error.
fn scope_failure_root(error: &WyrdError) -> Option<CardRef> {
    let details = match error {
        WyrdError::CardScopeTooLarge { details, .. }
        | WyrdError::RegistryCardNotFound { details, .. } => details,
        _ => return None,
    };
    details
        .get("scope_mint_root")
        .or_else(|| details.get("root"))
        .and_then(Value::as_str)
        .and_then(|value| CardRef::from_str(value).ok())
}

/// Return canonical string members for hashing and bounded audit summaries.
fn scope_member_strings(scope: &CardRefScope) -> Vec<String> {
    scope.as_slice().iter().map(ToString::to_string).collect()
}

/// Hash the scope member set in a deterministic order.
fn scope_hash(members: &[String]) -> String {
    let mut sorted = members.to_vec();
    sorted.sort();
    let mut hasher = Sha256::new();
    for member in sorted {
        hasher.update(member.as_bytes());
        hasher.update(b"\n");
    }
    format!("{:x}", hasher.finalize())
}

/// Build a bounded audit summary of the card-ref scope members.
fn scope_member_summary(members: &[String]) -> Value {
    json!({
        "members": members
            .iter()
            .take(SCOPE_AUDIT_MEMBER_SUMMARY_LIMIT)
            .collect::<Vec<_>>(),
        "truncated": members.len() > SCOPE_AUDIT_MEMBER_SUMMARY_LIMIT,
    })
}
