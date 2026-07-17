//! Resolve non-sibling `CardRef` values before registration writes begin.

use std::collections::{BTreeSet, HashMap};

use wyrd_spec::error::WyrdError;
use wyrd_spec::ids::CardUid;
use wyrd_spec::reference::CardRef;
use wyrd_spec::registry::CardSubmission;
use wyrd_sql::TenantConn;
use wyrd_sql::queries::cards::select_card_uids_by_ref_batch;

/// Identity key used to look up a resolved external reference.
///
/// Sibling refs (whose `(kind, space, name)` match a submission in the current
/// composite request) are NOT resolved here. Sibling UIDs are minted during
/// topo iteration inside the composite transaction, and the caller looks them
/// up from the running write map.
pub type ResolvedRefs = Vec<(CardRef, CardUid)>;

/// Resolve every external (non-sibling) `CardRef` to its `CardUid` under RLS.
///
/// Sibling references are excluded from the database read and left for the
/// composite tx to bind from its running write map. Missing external refs
/// surface as `WYRD_REGISTRY_422_UNRESOLVED_DEPENDENCY`, naming the offending
/// `kind/space/name/version` in the problem-json `detail`.
pub async fn resolve_card_references(
    conn: &mut TenantConn<'_>,
    submissions: &[CardSubmission],
) -> Result<ResolvedRefs, WyrdError> {
    let siblings = sibling_identities(submissions);

    let mut refs = Vec::new();
    for submission in submissions {
        collect_card_refs(&submission.spec, &mut refs);
    }

    refs.retain(|card_ref| !siblings.contains(&sibling_key(card_ref)));
    refs.sort_by_key(display_ref);
    refs.dedup_by(|left, right| display_ref(left) == display_ref(right));

    let resolved = select_card_uids_by_ref_batch(conn, &refs).await?;
    let resolved_refs = resolved;

    if let Some(missing) = refs.iter().find(|card_ref| {
        !resolved_refs
            .iter()
            .any(|(resolved, _)| resolved.same_identity(card_ref))
    }) {
        let identity = display_ref(missing);
        return Err(WyrdError::RegistryUnresolvedDependency {
            message: format!("card dependency {identity} was not found"),
            details: serde_json::json!({ "card_ref": identity }),
        });
    }

    Ok(resolved_refs)
}

/// Collect the `(kind, space, name)` identities that appear as siblings.
fn sibling_identities(submissions: &[CardSubmission]) -> BTreeSet<(String, String, String)> {
    submissions
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
        .collect()
}

/// Return the version-independent identity used only for sibling matching.
fn sibling_key(card_ref: &CardRef) -> (String, String, String) {
    (
        card_ref.kind.wire_name().to_owned(),
        card_ref.space.as_str().to_owned(),
        card_ref.name.as_str().to_owned(),
    )
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

/// Bind external and already-minted sibling UIDs into every embedded reference.
pub fn bind_card_references(
    value: &mut serde_json::Value,
    external: &ResolvedRefs,
    siblings: &HashMap<(String, String, String), CardUid>,
) -> Result<(), WyrdError> {
    match value {
        serde_json::Value::Array(values) => {
            for value in values {
                bind_card_references(value, external, siblings)?;
            }
        }
        serde_json::Value::Object(object) => {
            if let Ok(mut card_ref) =
                serde_json::from_value::<CardRef>(serde_json::Value::Object(object.clone()))
            {
                card_ref.uid = siblings
                    .get(&sibling_key(&card_ref))
                    .or_else(|| {
                        external
                            .iter()
                            .find(|(resolved, _)| resolved.same_identity(&card_ref))
                            .map(|(_, uid)| uid)
                    })
                    .cloned();
                *value =
                    serde_json::to_value(card_ref).map_err(WyrdError::from_spec_serialization)?;
                return Ok(());
            }
            for value in object.values_mut() {
                bind_card_references(value, external, siblings)?;
            }
        }
        serde_json::Value::Null
        | serde_json::Value::Bool(_)
        | serde_json::Value::Number(_)
        | serde_json::Value::String(_) => {}
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use uuid::Uuid;
    use wyrd_spec::ids::CardUid;
    use wyrd_spec::reference::CardRef;
    use wyrd_spec::registry::CardSubmission;

    use super::{bind_card_references, collect_card_refs, sibling_identities};

    /// Decode one compact submission fixture.
    fn submission(value: serde_json::Value) -> CardSubmission {
        serde_json::from_value(value).expect("submission fixture must deserialize")
    }

    /// Build an exact Prompt reference fixture.
    fn prompt_ref(name: &str) -> CardRef {
        serde_json::from_value(serde_json::json!({
            "kind": "Prompt",
            "name": name,
            "version": "1.0.0",
            "space": "default"
        }))
        .expect("reference fixture must deserialize")
    }

    /// Treat sibling identity as kind, space, and name without a redundant wrapper type.
    #[test]
    fn sibling_identity_ignores_version_and_uid() {
        let child = submission(serde_json::json!({
            "apiVersion": "wyrd/v1",
            "kind": "Prompt",
            "metadata": { "name": "child", "version": "2.0.0", "space": "default" },
            "spec": { "provider": "openai", "model": "gpt-4o", "messages": ["hello"] },
            "artifacts": []
        }));
        let identities = sibling_identities(&[child]);

        assert!(identities.contains(&(
            "Prompt".to_owned(),
            "default".to_owned(),
            "child".to_owned()
        )));
    }

    /// Collect nested card references without treating ordinary objects as references.
    #[test]
    fn collect_nested_card_refs() {
        let expected = prompt_ref("child");
        let value = serde_json::json!({
            "prompt": expected,
            "config": { "kind": "not-a-card", "name": "ignored" }
        });
        let mut refs = Vec::new();

        collect_card_refs(&value, &mut refs);

        assert_eq!(refs, vec![expected]);
    }

    /// Bind a sibling UID directly into the durable CardRef wire shape.
    #[test]
    fn bind_sibling_uid_without_card_ref_identity() {
        let card_ref = prompt_ref("child");
        let uid = CardUid::from_uuid(Uuid::now_v7()).expect("UUIDv7 is a valid card UID");
        let mut value = serde_json::to_value(&card_ref).expect("reference serializes");
        let siblings = HashMap::from([(
            (
                "Prompt".to_owned(),
                "default".to_owned(),
                "child".to_owned(),
            ),
            uid.clone(),
        )]);

        bind_card_references(&mut value, &Vec::new(), &siblings).expect("binding succeeds");
        let bound: CardRef = serde_json::from_value(value).expect("bound reference deserializes");

        assert_eq!(bound.uid, Some(uid));
    }

    /// Bind an externally resolved UID using CardRef identity comparison.
    #[test]
    fn bind_external_uid_for_exact_reference() {
        let card_ref = prompt_ref("external");
        let uid = CardUid::from_uuid(Uuid::now_v7()).expect("UUIDv7 is a valid card UID");
        let mut value = serde_json::to_value(&card_ref).expect("reference serializes");

        bind_card_references(
            &mut value,
            &vec![(card_ref.clone(), uid.clone())],
            &HashMap::new(),
        )
        .expect("binding succeeds");
        let bound: CardRef = serde_json::from_value(value).expect("bound reference deserializes");

        assert_eq!(bound.uid, Some(uid));
    }
}
