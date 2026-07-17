//! Resolve non-sibling `CardRef` values before registration writes begin.

use std::collections::BTreeSet;

use wyrd_spec::error::WyrdError;
use wyrd_spec::reference::CardRef;
use wyrd_spec::registry::CardSubmission;
use wyrd_sql::TenantConn;
use wyrd_sql::queries::cards::select_card_uids_by_ref_batch;

/// Verify every external reference exists in the caller's RLS tenant.
///
/// Sibling references are resolved by the composite write and are excluded
/// from the database read. The function performs no durable writes.
pub async fn resolve_card_references(
    conn: &mut TenantConn<'_>,
    submissions: &[CardSubmission],
) -> Result<(), WyrdError> {
    let siblings = submissions
        .iter()
        .map(|submission| {
            (
                submission.kind.wire_name().to_owned(),
                submission
                    .metadata
                    .space
                    .as_ref()
                    .map_or("", |space| space.as_str())
                    .to_owned(),
                submission.metadata.name.as_str().to_owned(),
            )
        })
        .collect::<BTreeSet<_>>();
    let mut refs = Vec::new();
    for submission in submissions {
        collect_card_refs(&submission.spec, &mut refs);
    }
    refs.retain(|card_ref| {
        !siblings.contains(&(
            card_ref.kind.wire_name().to_owned(),
            card_ref.space.as_str().to_owned(),
            card_ref.name.as_str().to_owned(),
        ))
    });
    refs.sort_by_key(display_ref);
    refs.dedup_by(|left, right| display_ref(left) == display_ref(right));

    let resolved = select_card_uids_by_ref_batch(conn, &refs).await?;
    let resolved_keys = resolved
        .iter()
        .map(|(card_ref, _)| display_ref(card_ref))
        .collect::<BTreeSet<_>>();
    if let Some(missing) = refs
        .iter()
        .find(|card_ref| !resolved_keys.contains(&display_ref(card_ref)))
    {
        let identity = display_ref(missing);
        return Err(WyrdError::RegistryUnresolvedDependency {
            message: format!("card dependency {identity} was not found"),
            details: serde_json::json!({ "card_ref": identity }),
        });
    }
    Ok(())
}

/// Format a reference for deterministic comparison and actionable errors.
fn display_ref(card_ref: &CardRef) -> String {
    format!(
        "{}/{}/{}@{}",
        card_ref.kind.wire_name(),
        card_ref.space,
        card_ref.name,
        card_ref.version
    )
}

/// Recursively collect exact `CardRef` objects embedded in a JSON spec.
fn collect_card_refs(value: &serde_json::Value, output: &mut Vec<CardRef>) {
    match value {
        serde_json::Value::Array(values) => {
            for value in values {
                collect_card_refs(value, output);
            }
        }
        serde_json::Value::Object(object) => {
            if let Ok(card_ref) =
                serde_json::from_value::<CardRef>(serde_json::Value::Object(object.clone()))
            {
                output.push(card_ref);
            }
            for value in object.values() {
                collect_card_refs(value, output);
            }
        }
        serde_json::Value::Null
        | serde_json::Value::Bool(_)
        | serde_json::Value::Number(_)
        | serde_json::Value::String(_) => {}
    }
}
