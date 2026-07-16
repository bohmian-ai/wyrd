//! Pre-write card-reference resolution.

use std::collections::BTreeMap;

use wyrd_spec::error::WyrdError;
use wyrd_spec::reference::{CardRef, bind_scoped_card_ref_uids, scope_child_card_refs};
use wyrd_spec::registry::CardSubmission;
use wyrd_sql::TenantConn;

use wyrd_sql::queries::cards::select_card_uids_by_ref_batch;

/// Resolve all direct child-card references before the registration write.
pub async fn resolve_submission(
    conn: &mut TenantConn<'_>,
    mut submission: CardSubmission,
) -> Result<CardSubmission, WyrdError> {
    let refs = deduplicate_refs(
        scope_child_card_refs(&submission.spec)
            .into_iter()
            .filter(|card_ref| card_ref.uid.is_none())
            .collect(),
    );
    if refs.is_empty() {
        return Ok(submission);
    }

    let resolved = select_card_uids_by_ref_batch(conn, &refs).await?;
    if resolved.len() != refs.len() {
        let found: BTreeMap<_, _> = resolved
            .iter()
            .map(|(card_ref, uid)| (identity_key(card_ref), uid))
            .collect();
        let missing = refs
            .iter()
            .filter(|card_ref| !found.contains_key(&identity_key(card_ref)))
            .map(ToString::to_string)
            .collect::<Vec<_>>();
        return Err(WyrdError::RegistryUnresolvedDependency {
            message: "one or more card references could not be resolved".to_owned(),
            details: serde_json::json!({ "references": missing }),
        });
    }

    submission.spec = bind_scoped_card_ref_uids(&submission.kind, submission.spec, &resolved)
        .map_err(|error| WyrdError::RegistryInvalidCardSpec {
            message: format!("resolved card spec failed to decode: {error}"),
            details: serde_json::json!({}),
        })?;
    Ok(submission)
}

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

fn identity_key(card_ref: &CardRef) -> String {
    format!(
        "{}/{}/{}/{}",
        card_ref.kind.wire_name(),
        card_ref.space,
        card_ref.name,
        card_ref.version
    )
}
