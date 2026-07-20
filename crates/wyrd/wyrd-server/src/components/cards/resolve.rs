//! Resolve non-sibling `CardRef` values before registration writes begin.

use std::collections::{BTreeSet, HashMap};

use wyrd_spec::envelope::Spec;
use wyrd_spec::error::WyrdError;
use wyrd_spec::ids::CardUid;
use wyrd_spec::reference::{CardRef, CardRefIdentity};
use wyrd_spec::refs::{ReferenceSlotVisitor, SlotValue};
use wyrd_spec::registry::CardSubmission;
use wyrd_sql::TenantConn;
use wyrd_sql::queries::cards::select_card_uids_by_ref_batch;

/// Identity key used to look up a resolved external reference.
pub type ResolvedRefs = Vec<(CardRef, CardUid)>;

/// Resolve every external (non-sibling) `CardRef` to its `CardUid` under RLS.
pub async fn resolve_card_references(
    conn: &mut TenantConn<'_>,
    submissions: &[CardSubmission],
) -> Result<ResolvedRefs, WyrdError> {
    let siblings = sibling_identities(submissions)?;
    let mut refs = Vec::new();

    for submission in submissions {
        let spec = Spec::from_kind_and_value(&submission.kind, submission.spec.clone())
            .map_err(|error| WyrdError::registry_invalid_card_spec(error.to_string()))?;
        validate_and_collect_refs(&spec, &siblings, &mut refs)?;
    }

    refs.sort_by_key(display_ref);
    refs.dedup_by(|left, right| left.same_identity(right));

    let resolved_refs = select_card_uids_by_ref_batch(conn, &refs).await?;
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

/// Collect the typed `CardRef` fields from one decoded spec.
#[cfg(test)]
fn collect_card_refs(spec: &Spec, output: &mut Vec<CardRef>) {
    let mut spec = spec.clone();
    ReferenceSlotVisitor::visit(&mut spec, |slot| match slot.value {
        SlotValue::Durable(reference) => {
            if let wyrd_spec::reference::Ref::Ref(card_ref) = reference {
                output.push(card_ref.clone());
            }
        }
        SlotValue::InlineablePrompt(reference) => {
            if let wyrd_spec::reference::InlineableRef::Ref(card_ref) = reference {
                output.push(card_ref.clone());
            }
        }
        SlotValue::InlineableAgent(reference) => {
            if let wyrd_spec::reference::InlineableRef::Ref(card_ref) = reference {
                output.push(card_ref.clone());
            }
        }
    });
}

/// Reject loader-only paths, validate typed sibling targets, and collect external refs.
fn validate_and_collect_refs(
    spec: &Spec,
    siblings: &BTreeSet<CardRefIdentity>,
    output: &mut Vec<CardRef>,
) -> Result<(), WyrdError> {
    let mut spec = spec.clone();
    let mut result = Ok(());
    ReferenceSlotVisitor::visit(&mut spec, |slot| {
        if result.is_err() {
            return;
        }
        match slot.value {
            SlotValue::Durable(reference) => match reference {
                wyrd_spec::reference::Ref::Ref(card_ref) => output.push(card_ref.clone()),
                wyrd_spec::reference::Ref::Sibling { sibling } => {
                    result = validate_sibling(sibling, siblings);
                }
                wyrd_spec::reference::Ref::Path(path) => {
                    result = Err(unresolved_path_error(path));
                }
            },
            SlotValue::InlineablePrompt(reference) => match reference {
                wyrd_spec::reference::InlineableRef::Ref(card_ref) => {
                    output.push(card_ref.clone());
                }
                wyrd_spec::reference::InlineableRef::Sibling { sibling } => {
                    result = validate_sibling(sibling, siblings);
                }
                wyrd_spec::reference::InlineableRef::Path(path) => {
                    result = Err(unresolved_path_error(path));
                }
                wyrd_spec::reference::InlineableRef::Inline(_) => {}
            },
            SlotValue::InlineableAgent(reference) => match reference {
                wyrd_spec::reference::InlineableRef::Ref(card_ref) => {
                    output.push(card_ref.clone());
                }
                wyrd_spec::reference::InlineableRef::Sibling { sibling } => {
                    result = validate_sibling(sibling, siblings);
                }
                wyrd_spec::reference::InlineableRef::Path(path) => {
                    result = Err(unresolved_path_error(path));
                }
                wyrd_spec::reference::InlineableRef::Inline(_) => {}
            },
        }
    });
    result
}

fn validate_sibling(
    sibling: &CardRef,
    siblings: &BTreeSet<CardRefIdentity>,
) -> Result<(), WyrdError> {
    if siblings.contains(&sibling_key(sibling)) {
        Ok(())
    } else {
        Err(WyrdError::RegistryUnresolvedDependency {
            message: format!(
                "sibling card dependency {} was not submitted",
                display_ref(sibling)
            ),
            details: serde_json::json!({ "card_ref": display_ref(sibling) }),
        })
    }
}

fn unresolved_path_error(path: &std::path::Path) -> WyrdError {
    WyrdError::RegistryUnresolvedPathRef {
        message: format!(
            "card reference path `{}` must be rewritten by the loader",
            path.display()
        ),
        details: serde_json::json!({ "path": path.display().to_string() }),
    }
}

/// Bind external and already-minted sibling UIDs into every embedded reference.
pub fn bind_card_references(
    spec: &mut Spec,
    external: &ResolvedRefs,
    siblings: &HashMap<CardRefIdentity, CardUid>,
) -> Result<(), WyrdError> {
    let mut result = Ok(());
    ReferenceSlotVisitor::visit(spec, |slot| {
        if result.is_err() {
            return;
        }
        result = match slot.value {
            SlotValue::Durable(reference) => bind_ref(reference, external, siblings),
            SlotValue::InlineablePrompt(reference) => {
                bind_inline_ref(reference, external, siblings)
            }
            SlotValue::InlineableAgent(reference) => bind_inline_ref(reference, external, siblings),
        };
    });
    result
}

fn bind_ref(
    reference: &mut wyrd_spec::reference::Ref,
    external: &ResolvedRefs,
    siblings: &HashMap<CardRefIdentity, CardUid>,
) -> Result<(), WyrdError> {
    match reference {
        wyrd_spec::reference::Ref::Ref(card_ref) => {
            card_ref.uid = external_uid(card_ref, external);
            require_uid(card_ref)
        }
        wyrd_spec::reference::Ref::Sibling { sibling } => {
            let uid = siblings.get(&sibling_key(sibling)).cloned();
            let mut card_ref = sibling.clone();
            card_ref.uid = uid;
            require_uid(&card_ref)?;
            *reference = wyrd_spec::reference::Ref::Ref(card_ref);
            Ok(())
        }
        wyrd_spec::reference::Ref::Path(path) => Err(unresolved_path_error(path)),
    }
}

fn bind_inline_ref<T>(
    reference: &mut wyrd_spec::reference::InlineableRef<T>,
    external: &ResolvedRefs,
    siblings: &HashMap<CardRefIdentity, CardUid>,
) -> Result<(), WyrdError> {
    match reference {
        wyrd_spec::reference::InlineableRef::Ref(card_ref) => {
            card_ref.uid = external_uid(card_ref, external);
            require_uid(card_ref)
        }
        wyrd_spec::reference::InlineableRef::Sibling { sibling } => {
            let uid = siblings.get(&sibling_key(sibling)).cloned();
            let mut card_ref = sibling.clone();
            card_ref.uid = uid;
            require_uid(&card_ref)?;
            *reference = wyrd_spec::reference::InlineableRef::Ref(card_ref);
            Ok(())
        }
        wyrd_spec::reference::InlineableRef::Inline(_) => Ok(()),
        wyrd_spec::reference::InlineableRef::Path(path) => Err(unresolved_path_error(path)),
    }
}

fn external_uid(card_ref: &CardRef, external: &ResolvedRefs) -> Option<CardUid> {
    external
        .iter()
        .find(|(resolved, _)| resolved.same_identity(card_ref))
        .map(|(_, uid)| uid.clone())
}

fn require_uid(card_ref: &CardRef) -> Result<(), WyrdError> {
    if card_ref.uid.is_some() {
        Ok(())
    } else {
        Err(WyrdError::RegistryUnresolvedDependency {
            message: format!("card dependency {} was not resolved", display_ref(card_ref)),
            details: serde_json::json!({ "card_ref": display_ref(card_ref) }),
        })
    }
}

/// Collect the exact `(kind, space, name, version)` identities that appear as siblings.
fn sibling_identities(
    submissions: &[CardSubmission],
) -> Result<BTreeSet<CardRefIdentity>, WyrdError> {
    submissions
        .iter()
        .map(|submission| {
            let space = submission.metadata.space.as_ref().ok_or_else(|| {
                WyrdError::registry_invalid_card_spec("metadata.space is required")
            })?;
            let version = submission.metadata.resolved_pin().cloned().ok_or_else(|| {
                WyrdError::registry_invalid_card_spec(
                    "metadata.version must resolve to an exact pin at the registry boundary",
                )
            })?;
            Ok(CardRef {
                kind: submission.kind.clone(),
                name: submission.metadata.name.clone(),
                version,
                space: Some(space.clone()),
                uid: None,
            }
            .identity_key())
        })
        .collect()
}

/// Return the exact identity used for sibling matching and UID binding.
fn sibling_key(card_ref: &CardRef) -> CardRefIdentity {
    card_ref.identity_key()
}

/// Format a reference for deterministic comparison and actionable errors.
fn display_ref(card_ref: &CardRef) -> String {
    format!(
        "{}/{}/{}@{}",
        card_ref.kind.wire_name(),
        card_ref
            .space
            .as_ref()
            .map_or("<missing-space>", |space| space.as_str()),
        card_ref.name,
        card_ref.version
    )
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use uuid::Uuid;
    use wyrd_semver::{VersionBlock, VersionSpec};
    use wyrd_spec::api_version::ApiVersion;
    use wyrd_spec::envelope::{CardKind, Metadata, Spec};
    use wyrd_spec::ids::CardUid;
    use wyrd_spec::reference::CardRef;
    use wyrd_spec::registry::CardSubmission;

    use super::{bind_card_references, collect_card_refs, sibling_identities};

    fn prompt_ref(name: &str) -> CardRef {
        CardRef {
            kind: CardKind::Prompt,
            name: name.parse().expect("test_setup: reference name is valid"),
            version: VersionBlock::parse("1.0.0").expect("test_setup: reference version is valid"),
            space: Some(
                "default"
                    .parse()
                    .expect("test_setup: reference space is valid"),
            ),
            uid: None,
        }
    }

    fn submission(name: &str) -> CardSubmission {
        CardSubmission {
            api_version: ApiVersion::v1(),
            kind: CardKind::Prompt,
            metadata: Metadata {
                name: name.parse().expect("test_setup: submission name is valid"),
                version: Some(VersionSpec::Pin(
                    VersionBlock::parse("1.0.0").expect("test_setup: submission version is valid"),
                )),
                bump: None,
                space: Some(
                    "default"
                        .parse()
                        .expect("test_setup: submission space is valid"),
                ),
                uid: None,
                labels: Default::default(),
                annotations: Default::default(),
                spec_hash: None,
                artifact_hash: None,
                origin: None,
            },
            spec: serde_json::json!({
                "provider": "openai",
                "model": "gpt-4o",
                "messages": ["hello"]
            }),
            artifacts: Vec::new(),
        }
    }

    fn service_spec(card_ref: CardRef) -> Spec {
        Spec::from_kind_and_value(
            &CardKind::Service,
            serde_json::json!({
                "components": [{"alias": "child", "ref": card_ref}]
            }),
        )
        .expect("test_setup: service spec is valid")
    }

    fn sibling_service_spec(card_ref: CardRef) -> Spec {
        Spec::from_kind_and_value(
            &CardKind::Service,
            serde_json::json!({
                "components": [{
                    "alias": "child",
                    "ref": {"sibling": card_ref}
                }]
            }),
        )
        .expect("test_setup: sibling service spec is valid")
    }

    #[test]
    fn sibling_identity_requires_space() {
        let mut child = submission("child");
        child.metadata.space = None;

        let error =
            sibling_identities(&[child]).expect_err("test_setup: missing space must be rejected");

        assert_eq!(error.code(), "WYRD_REGISTRY_400_INVALID_CARD_SPEC");
    }

    #[test]
    fn collect_typed_card_refs() {
        let expected = prompt_ref("child");
        let spec = service_spec(expected.clone());
        let mut refs = Vec::new();

        collect_card_refs(&spec, &mut refs);

        assert_eq!(refs, vec![expected]);
    }

    #[test]
    fn bind_sibling_uid_without_json_shape_sniffing() {
        let expected = prompt_ref("child");
        let uid = CardUid::from_uuid(Uuid::now_v7()).expect("test_setup: UUIDv7 is valid");
        let mut spec = sibling_service_spec(expected.clone());
        let siblings = HashMap::from([(expected.identity_key(), uid.clone())]);

        bind_card_references(&mut spec, &Vec::new(), &siblings)
            .expect("test_setup: binding succeeds");

        let mut refs = Vec::new();
        collect_card_refs(&spec, &mut refs);
        assert_eq!(refs[0].uid, Some(uid));
    }

    #[test]
    fn bind_external_uid_for_exact_reference() {
        let expected = prompt_ref("external");
        let uid = CardUid::from_uuid(Uuid::now_v7()).expect("test_setup: UUIDv7 is valid");
        let mut spec = service_spec(expected.clone());

        bind_card_references(
            &mut spec,
            &vec![(expected.clone(), uid.clone())],
            &HashMap::new(),
        )
        .expect("test_setup: binding succeeds");

        let mut refs = Vec::new();
        collect_card_refs(&spec, &mut refs);
        assert_eq!(refs[0].uid, Some(uid));
    }

    #[test]
    fn external_and_sibling_same_identity_bind_from_separate_sources() {
        let expected = prompt_ref("shared");
        let external_uid = CardUid::from_uuid(Uuid::now_v7()).expect("external UUIDv7 is valid");
        let sibling_uid = CardUid::from_uuid(Uuid::now_v7()).expect("sibling UUIDv7 is valid");
        let mut spec = Spec::from_kind_and_value(
            &CardKind::Service,
            serde_json::json!({
                "components": [
                    {"alias": "external", "ref": expected.clone()},
                    {"alias": "sibling", "ref": {"sibling": expected.clone()}}
                ]
            }),
        )
        .expect("test_setup: mixed service spec is valid");

        bind_card_references(
            &mut spec,
            &vec![(expected.clone(), external_uid.clone())],
            &HashMap::from([(expected.identity_key(), sibling_uid.clone())]),
        )
        .expect("test_setup: mixed binding succeeds");

        let mut refs = Vec::new();
        collect_card_refs(&spec, &mut refs);
        assert_eq!(refs[0].uid, Some(external_uid));
        assert_eq!(refs[1].uid, Some(sibling_uid));
    }

    #[test]
    fn sibling_identity_includes_version() {
        let first = submission("child");
        let mut second = submission("child");
        second.metadata.version = Some(VersionSpec::Pin(
            VersionBlock::parse("2.0.0").expect("test_setup: version is valid"),
        ));

        let identities = sibling_identities(&[first, second]).expect("identities resolve");
        assert_eq!(identities.len(), 2);
    }

    #[test]
    fn sibling_binding_does_not_match_a_different_version() {
        let expected = prompt_ref("child");
        let mut other_version = expected.clone();
        other_version.version = VersionBlock::parse("2.0.0").expect("version is valid");
        let uid = CardUid::from_uuid(Uuid::now_v7()).expect("test_setup: UUIDv7 is valid");
        let mut spec = sibling_service_spec(other_version);

        let error = bind_card_references(&mut spec, &vec![(expected, uid)], &HashMap::new())
            .expect_err("different sibling version must be rejected");
        assert_eq!(error.code(), "WYRD_REGISTRY_422_UNRESOLVED_DEPENDENCY");
    }
}
