//! Resolve non-sibling `CardRef` values before registration writes begin.

use std::collections::{BTreeSet, HashMap};

use wyrd_spec::envelope::{ReferenceSlotVisitor, Spec};
use wyrd_spec::error::WyrdError;
use wyrd_spec::ids::CardUid;
use wyrd_spec::reference::CardRef;
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
        collect_card_refs(&spec, &mut refs);
    }

    refs.retain(|card_ref| !siblings.contains(&sibling_key(card_ref)));
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
fn collect_card_refs(spec: &Spec, output: &mut Vec<CardRef>) {
    let mut collector = RefCollector { output };
    spec.walk_refs(&mut collector);
}

/// Bind external and already-minted sibling UIDs into every embedded reference.
pub fn bind_card_references(
    spec: &mut Spec,
    external: &ResolvedRefs,
    siblings: &HashMap<(String, String, String), CardUid>,
) -> Result<(), WyrdError> {
    let mut binder = RefBinder { external, siblings };
    spec.walk_refs_mut(&mut binder);
    Ok(())
}

/// Collects immutable typed reference slots.
struct RefCollector<'a> {
    output: &'a mut Vec<CardRef>,
}

impl ReferenceSlotVisitor for RefCollector<'_> {
    fn visit_ref(&mut self, card_ref: &CardRef) {
        self.output.push(card_ref.clone());
    }

    fn visit_ref_mut(&mut self, _card_ref: &mut CardRef) {}
}

/// Binds a reference slot to a sibling or external UID when one is available.
struct RefBinder<'a> {
    external: &'a ResolvedRefs,
    siblings: &'a HashMap<(String, String, String), CardUid>,
}

impl ReferenceSlotVisitor for RefBinder<'_> {
    fn visit_ref(&mut self, _card_ref: &CardRef) {}

    fn visit_ref_mut(&mut self, card_ref: &mut CardRef) {
        card_ref.uid = self
            .siblings
            .get(&sibling_key(card_ref))
            .or_else(|| {
                self.external
                    .iter()
                    .find(|(resolved, _)| resolved.same_identity(card_ref))
                    .map(|(_, uid)| uid)
            })
            .cloned();
    }
}

/// Collect the `(kind, space, name)` identities that appear as siblings.
fn sibling_identities(
    submissions: &[CardSubmission],
) -> Result<BTreeSet<(String, String, String)>, WyrdError> {
    submissions
        .iter()
        .map(|submission| {
            let space = submission.metadata.space.as_ref().ok_or_else(|| {
                WyrdError::registry_invalid_card_spec("metadata.space is required")
            })?;
            Ok((
                submission.kind.wire_name().to_owned(),
                space.as_str().to_owned(),
                submission.metadata.name.as_str().to_owned(),
            ))
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
            space: "default"
                .parse()
                .expect("test_setup: reference space is valid"),
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
        let mut spec = service_spec(expected.clone());
        let siblings = HashMap::from([(
            (
                "Prompt".to_owned(),
                "default".to_owned(),
                "child".to_owned(),
            ),
            uid.clone(),
        )]);

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
}
