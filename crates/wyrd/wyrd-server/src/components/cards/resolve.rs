//! Pre-write card-reference resolution.

use std::collections::BTreeMap;

use wyrd_spec::error::WyrdError;
use wyrd_spec::reference::{CardRef, scope_child_card_refs};
use wyrd_spec::registry::CardSubmission;
use wyrd_sql::TenantConn;

use wyrd_sql::queries::cards::select_card_uids_by_ref_batch;

/// Resolve all direct child-card references before the registration write.
pub async fn resolve_submission(
    conn: &mut TenantConn<'_>,
    mut submission: CardSubmission,
) -> Result<CardSubmission, WyrdError> {
    let refs = deduplicate_refs(scope_child_card_refs(&submission.spec));
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

    let values =
        serde_json::to_value(&submission.spec).map_err(WyrdError::from_spec_serialization)?;
    let mut values = values;
    for (card_ref, uid) in resolved {
        bind_uid(&mut values, &card_ref, &uid)?;
    }
    submission.spec = wyrd_spec::envelope::Spec::from_kind_and_value(&submission.kind, values)
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

fn bind_uid(
    value: &mut serde_json::Value,
    card_ref: &CardRef,
    uid: &wyrd_spec::ids::CardUid,
) -> Result<(), WyrdError> {
    match value {
        serde_json::Value::Array(values) => {
            for value in values {
                bind_uid(value, card_ref, uid)?;
            }
        }
        serde_json::Value::Object(object) => {
            let matches = object
                .get("kind")
                .and_then(serde_json::Value::as_str)
                .is_some_and(|kind| kind == card_ref.kind.wire_name())
                && object
                    .get("name")
                    .and_then(serde_json::Value::as_str)
                    .is_some_and(|name| name == card_ref.name.as_str())
                && object
                    .get("version")
                    .and_then(serde_json::Value::as_str)
                    .is_some_and(|version| version == card_ref.version.as_str())
                && object
                    .get("space")
                    .and_then(serde_json::Value::as_str)
                    .is_some_and(|space| space == card_ref.space.as_str());
            if matches {
                object.insert(
                    "uid".to_owned(),
                    serde_json::to_value(uid).map_err(WyrdError::from_spec_serialization)?,
                );
            }
            for value in object.values_mut() {
                bind_uid(value, card_ref, uid)?;
            }
        }
        serde_json::Value::Null
        | serde_json::Value::Bool(_)
        | serde_json::Value::Number(_)
        | serde_json::Value::String(_) => {}
    }
    Ok(())
}
