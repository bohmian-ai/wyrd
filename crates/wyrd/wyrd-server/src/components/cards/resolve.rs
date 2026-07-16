//! Pre-write card-reference resolution.

use std::collections::BTreeMap;

use wyrd_spec::error::WyrdError;
use wyrd_spec::reference::{CardRef, bind_scoped_card_ref_uids, scope_child_card_refs};
use wyrd_spec::registry::CardSubmission;
use wyrd_sql::TenantConn;

use wyrd_sql::queries::cards::select_card_uids_by_ref_batch;

/// Resolve all direct child-card references before the registration write.
///
/// Every child reference is looked up as one tenant-scoped batch. The loader
/// preserves cross-tree references, so the registry is the authority that
/// verifies the referenced card exists and binds its server-generated UID.
pub async fn resolve_submission(
    conn: &mut TenantConn<'_>,
    mut submission: CardSubmission,
) -> Result<CardSubmission, WyrdError> {
    let refs = unresolved_child_refs(&submission.spec);
    if refs.is_empty() {
        return Ok(submission);
    }

    let resolved = select_card_uids_by_ref_batch(conn, &refs).await?;
    if resolved.len() != refs.len() {
        return Err(unresolved_dependency_error(&refs, &resolved));
    }

    submission.spec = bind_scoped_card_ref_uids(&submission.kind, submission.spec, &resolved)
        .map_err(|error| WyrdError::RegistryInvalidCardSpec {
            message: format!("resolved card spec failed to decode: {error}"),
            details: serde_json::json!({}),
        })?;
    Ok(submission)
}

/// Collect unique child references that still need server-side UID resolution.
fn unresolved_child_refs(spec: &wyrd_spec::envelope::Spec) -> Vec<CardRef> {
    deduplicate_refs(
        scope_child_card_refs(spec)
            .into_iter()
            .map(|mut card_ref| {
                // UIDs are server-owned. Ignore any authored value and look
                // up the dependency by its stable card identity instead.
                card_ref.uid = None;
                card_ref
            })
            .collect(),
    )
}

/// Build the stable public error for one or more missing dependency cards.
fn unresolved_dependency_error(
    requested: &[CardRef],
    resolved: &[(CardRef, wyrd_spec::ids::CardUid)],
) -> WyrdError {
    let found: BTreeMap<_, _> = resolved
        .iter()
        .map(|(card_ref, uid)| (identity_key(card_ref), uid))
        .collect();
    let missing = requested
        .iter()
        .filter(|card_ref| !found.contains_key(&identity_key(card_ref)))
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    WyrdError::RegistryUnresolvedDependency {
        message: "one or more card references could not be resolved".to_owned(),
        details: serde_json::json!({ "references": missing }),
    }
}

/// Remove duplicate references while preserving the first occurrence order.
fn deduplicate_refs(refs: Vec<CardRef>) -> Vec<CardRef> {
    let mut unique = Vec::with_capacity(refs.len());
    for card_ref in refs {
        if !unique
            .iter()
            .any(|existing: &CardRef| existing.same_identity(&card_ref))
        {
            unique.push(card_ref);
        }
    }
    unique
}

/// Return the authorization identity used to compare a reference lookup.
fn identity_key(card_ref: &CardRef) -> String {
    format!(
        "{}/{}/{}/{}",
        card_ref.kind.wire_name(),
        card_ref.space,
        card_ref.name,
        card_ref.version
    )
}
