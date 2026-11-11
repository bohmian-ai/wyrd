//! Pure graph operations for composite Card registration.
//!
//! The graph uses authored `CardRef` identity coordinates to connect sibling
//! submissions. It does not resolve external references or perform any I/O.

use std::collections::{BTreeMap, BTreeSet};
use std::hash::{Hash, Hasher};

use crate::envelope::{CardKind, Spec};
use crate::ids::CardName;
use crate::reference::{CardRef, CardRefIdentity, SiblingRef};
use crate::refs::ReferenceSlotVisitor;
use crate::registry::CardSubmission;
use wyrd_semver::{VersionBlock, VersionSpec};

mod canonical;
mod composition;
mod root;
mod topo;

pub use canonical::{canonical_order, relationships_from_spec};
pub use composition::{
    BindingSite, binding_validation_errors, spec_binding_errors, validate_composition,
};
pub use root::{pick_root, root_last};
pub use topo::{GraphError, topo_sort};

/// One vertex in the submission DAG.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Node {
    /// The submitted Card identity.
    pub card_ref: CardRef,
}

impl Hash for Node {
    fn hash<H: Hasher>(&self, state: &mut H) {
        hash_card_ref(&self.card_ref, state);
    }
}

/// A directed edge from a parent submission to a referenced child submission.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Edge {
    /// The submission carrying the reference.
    pub from: CardRef,
    /// The sibling submission being referenced.
    pub to: CardRef,
}

impl Hash for Edge {
    fn hash<H: Hasher>(&self, state: &mut H) {
        hash_card_ref(&self.from, state);
        hash_card_ref(&self.to, state);
    }
}

/// Feed one graph reference into a [`Node`] or [`Edge`] hash.
///
/// It hashes every field `CardRef` equality compares: flat `kind`, `name`,
/// `version`, and the optional `uid`, keeping `Hash` consistent with `Eq`.
/// UID-free sibling matching is a separate rule, owned by
/// [`CardRef::same_identity`].
fn hash_card_ref<H: Hasher>(card_ref: &CardRef, state: &mut H) {
    card_ref.kind.hash(state);
    card_ref.name.hash(state);
    card_ref.version.hash(state);
    card_ref.uid.hash(state);
}

/// A successful topological ordering, with leaves before their parents.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TopoOrder {
    /// Ordered nodes, leaves first and the root last.
    pub nodes: Vec<Node>,
    /// Edges used to produce the ordering.
    pub edges: Vec<Edge>,
}

/// The single root selected from a successful graph.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RootPick {
    /// The node with no incoming edges from another submission.
    pub root: CardRef,
}

/// Prepare submissions for the pure graph operations.
///
/// Graph identity is `(kind, name, version)`. Auto and range version
/// requests therefore receive a graph-only placeholder, and every sibling
/// reference is rewritten to its target's graph identity using
/// [`sibling_target`], so graph construction, composition, and declaration
/// planning all match on exact identities. Matching runs against the
/// authored submissions, so the selected target is authoritative. A
/// placeholder is the lowest `0.0.N` that no submission pin and no reference
/// of that kind and name in the request spells, so it can never alias an
/// authored pin: an explicit sibling that matches no submission stays
/// unmatched, and an omitted and a pinned submission of one kind and name
/// stay distinct nodes. The authored submissions are cloned and never
/// mutated; registration keeps their original version selection, and
/// placeholders never become authored or persisted pins. A sibling with no
/// submitted target is left unchanged for the registration boundary to
/// refuse.
///
/// # Errors
/// Returns [`GraphError::Empty`] when there are no submissions,
/// [`GraphError::AmbiguousSibling`] when a versionless sibling names several
/// submissions, and [`GraphError::InvalidSpec`] when a spec cannot be decoded
/// or a placeholder cannot be built.
pub fn graph_ready_submissions(
    submissions: &[CardSubmission],
) -> Result<Vec<CardSubmission>, GraphError> {
    if submissions.is_empty() {
        return Err(GraphError::Empty);
    }

    let authored = authored_versions(submissions)?;
    let graph_refs = submissions
        .iter()
        .map(|submission| {
            let version = match submission.metadata.resolved_pin() {
                Some(version) => version.clone(),
                None => placeholder(&authored, &submission.kind, &submission.metadata.name)?,
            };
            Ok(CardRef {
                kind: submission.kind.clone(),
                name: submission.metadata.name.clone(),
                version,
                uid: submission.metadata.uid.clone(),
            })
        })
        .collect::<Result<Vec<_>, GraphError>>()?;
    submissions
        .iter()
        .cloned()
        .zip(&graph_refs)
        .map(|(mut submission, graph_ref)| {
            submission.metadata.version = Some(VersionSpec::Pin(graph_ref.version.clone()));
            pin_siblings(&mut submission, submissions, &graph_refs)?;
            Ok(submission)
        })
        .collect()
}

/// Every version the request spells for each kind and name: submission pins
/// and the versions of sibling and external references.
///
/// # Errors
/// Returns [`GraphError::InvalidSpec`] when a spec cannot be decoded.
fn authored_versions(
    submissions: &[CardSubmission],
) -> Result<BTreeSet<CardRefIdentity>, GraphError> {
    let mut versions = submissions
        .iter()
        .filter_map(submission_card_ref)
        .map(|card_ref| card_ref.identity_key())
        .collect::<BTreeSet<_>>();
    for submission in submissions {
        let mut spec = Spec::from_kind_and_value(&submission.kind, submission.spec.clone())
            .map_err(|error| GraphError::InvalidSpec {
                message: error.to_string(),
            })?;
        ReferenceSlotVisitor::visit(&mut spec, |slot| {
            versions.extend(slot.value.target().and_then(|target| target.identity_key()));
        });
    }
    Ok(versions)
}

/// The lowest `0.0.N` graph-only version `authored` does not spell for
/// `kind` and `name`; among `authored.len() + 1` candidates one is free.
///
/// # Errors
/// Returns [`GraphError::InvalidSpec`] when the candidate does not parse,
/// which a `0.0.N` version always does.
fn placeholder(
    authored: &BTreeSet<CardRefIdentity>,
    kind: &CardKind,
    name: &CardName,
) -> Result<VersionBlock, GraphError> {
    let free = (0..=authored.len())
        .map(|patch| format!("0.0.{patch}"))
        .find(|version| {
            !authored.contains(&CardRefIdentity {
                kind: kind.clone(),
                name: name.clone(),
                version: version.clone(),
            })
        })
        .unwrap_or_default();
    VersionBlock::parse(&free).map_err(|error| GraphError::InvalidSpec {
        message: format!("invalid graph placeholder: {error}"),
    })
}

/// Rewrite every sibling in one submission to its target's graph identity.
///
/// # Errors
/// Returns the decode and ambiguity errors of [`graph_ready_submissions`].
fn pin_siblings(
    submission: &mut CardSubmission,
    submissions: &[CardSubmission],
    graph_refs: &[CardRef],
) -> Result<(), GraphError> {
    let mut spec =
        Spec::from_kind_and_value(&submission.kind, submission.spec.clone()).map_err(|error| {
            GraphError::InvalidSpec {
                message: error.to_string(),
            }
        })?;
    let mut result = Ok(false);
    ReferenceSlotVisitor::visit(&mut spec, |mut slot| {
        let Ok(changed) = &mut result else {
            return;
        };
        let Some(sibling) = slot.value.as_sibling().cloned() else {
            return;
        };
        match sibling_target(&sibling, submissions) {
            Ok(Some(index)) => {
                slot.value.set_sibling(SiblingRef::from(&graph_refs[index]));
                *changed = true;
            }
            Ok(None) => {}
            Err(error) => result = Err(error),
        }
    });
    if result? {
        submission.spec = serde_json::to_value(&spec).map_err(|error| GraphError::InvalidSpec {
            message: error.to_string(),
        })?;
    }
    Ok(())
}

/// Find the submission a sibling reference names.
///
/// A sibling with a version names the submission pinned to exactly that
/// version. A sibling without one names the sole submission with its kind and
/// name, whatever that submission's version request; several such
/// submissions make the link ambiguous rather than guessed. Returns `None`
/// when no submission matches.
///
/// # Errors
/// Returns [`GraphError::AmbiguousSibling`] when a versionless sibling
/// matches more than one submission.
pub fn sibling_target(
    sibling: &SiblingRef,
    submissions: &[CardSubmission],
) -> Result<Option<usize>, GraphError> {
    let candidates = submissions
        .iter()
        .enumerate()
        .filter(|(_, submission)| {
            submission.kind == sibling.kind
                && submission.metadata.name == sibling.name
                && sibling
                    .version
                    .as_ref()
                    .is_none_or(|version| submission.metadata.resolved_pin() == Some(version))
        })
        .map(|(index, _)| index)
        .collect::<Vec<_>>();
    match candidates.as_slice() {
        [] => Ok(None),
        [index] => Ok(Some(*index)),
        _ => Err(GraphError::AmbiguousSibling {
            sibling: sibling.clone(),
            candidates: candidates
                .iter()
                .filter_map(|index| submission_card_ref(&submissions[*index]))
                .collect(),
        }),
    }
}

/// Build the submission graph from typed sibling references in each spec.
///
/// A sibling reference becomes an edge only when its `(kind, name, version)`
/// identity matches another submission in the same request;
/// [`graph_ready_submissions`] gives every matched sibling that version. External references are
/// deliberately ignored, even when they happen to have the same identity as a
/// submission; external resolution belongs to the server registry boundary.
///
/// The register boundary supplies resolved metadata before calling this helper:
/// each submission must have a concrete version. The graph itself
/// compares nodes by `(kind, name, version)`, so only the server UID
/// does not affect sibling matching.
///
/// # Errors
/// Returns [`GraphError::Empty`] when the request is empty or contains a
/// submission whose identity has not been resolved.
pub fn build(submissions: &[CardSubmission]) -> Result<(Vec<Node>, Vec<Edge>), GraphError> {
    if submissions.is_empty() {
        return Err(GraphError::Empty);
    }

    let submission_refs = submissions
        .iter()
        .map(submission_card_ref)
        .collect::<Option<Vec<_>>>()
        .ok_or(GraphError::Empty)?;

    let mut siblings = BTreeMap::new();
    let mut nodes = Vec::with_capacity(submission_refs.len());
    for card_ref in submission_refs {
        let key = identity_key(&card_ref);
        if let Some(existing) = siblings.insert(key, card_ref.clone()) {
            return Err(GraphError::DuplicateIdentity {
                candidates: vec![existing, card_ref],
            });
        }
        nodes.push(Node { card_ref });
    }

    let mut edges = Vec::new();
    let mut seen = BTreeSet::new();
    for (submission, parent) in submissions.iter().zip(nodes.iter()) {
        let spec = Spec::from_kind_and_value(&submission.kind, submission.spec.clone()).map_err(
            |error| GraphError::InvalidSpec {
                message: error.to_string(),
            },
        )?;
        let mut spec = spec;
        let mut references = Vec::new();
        ReferenceSlotVisitor::visit(&mut spec, |slot| {
            references.extend(slot.value.as_sibling().and_then(SiblingRef::identity_key));
        });
        for child in references {
            let Some(target) = siblings.get(&child) else {
                continue;
            };
            let edge_key = (identity_key(&parent.card_ref), identity_key(target));
            if seen.insert(edge_key) {
                edges.push(Edge {
                    from: parent.card_ref.clone(),
                    to: target.clone(),
                });
            }
        }
    }

    Ok((nodes, edges))
}

/// The UID-free `(kind, name, version)` key graph nodes, edges, and binding
/// targets compare by.
pub(crate) fn identity_key(card_ref: &CardRef) -> CardRefIdentity {
    card_ref.identity_key()
}

/// Build the graph reference of one submission with an exact version pin.
///
/// Returns `None` for an auto or range version, which has no exact identity
/// until [`graph_ready_submissions`] gives it a placeholder. A caller-asserted
/// `metadata.uid` is preserved so graph equality keeps it.
pub(super) fn submission_card_ref(submission: &CardSubmission) -> Option<CardRef> {
    Some(CardRef {
        kind: submission.kind.clone(),
        name: submission.metadata.name.clone(),
        version: submission.metadata.resolved_pin()?.clone(),
        uid: submission.metadata.uid.clone(),
    })
}

#[cfg(test)]
mod tests {
    use super::{Edge, build, graph_ready_submissions, identity_key};
    use crate::api_version::ApiVersion;
    use crate::envelope::{CardKind, Metadata};
    use crate::reference::CardRef;
    use crate::registry::CardSubmission;
    use serde_json::json;
    use std::collections::BTreeMap;
    use wyrd_semver::{VersionBlock, VersionSpec};

    fn card_ref(kind: CardKind, name: &str) -> CardRef {
        CardRef {
            kind,
            name: name.parse().expect("test card name is valid"),
            version: VersionBlock::parse("1.0.0").expect("test version is valid"),
            uid: None,
        }
    }

    fn submission(card_ref: &CardRef, spec: serde_json::Value) -> CardSubmission {
        CardSubmission {
            api_version: ApiVersion::v1(),
            kind: card_ref.kind.clone(),
            metadata: Metadata {
                name: card_ref.name.clone(),
                version: Some(VersionSpec::Pin(card_ref.version.clone())),
                bump: None,
                uid: card_ref.uid.clone(),
                labels: BTreeMap::default(),
                annotations: BTreeMap::default(),
                spec_hash: None,
                artifact_hash: None,
                origin: None,
            },
            spec,
            artifacts: Vec::new(),
        }
    }

    /// A durable reference to a Card outside the submission set derives no
    /// edge, though both Cards remain nodes.
    ///
    /// # Panics
    /// Panics when the graph does not build or derives an edge.
    #[test]
    fn build_ignores_external_refs() {
        let parent = card_ref(CardKind::Service, "service");
        let external = card_ref(CardKind::Prompt, "external");
        let (nodes, edges) = build(&[
            submission(
                &parent,
                json!({"components": [{"alias": "external", "ref": serde_json::to_value(external.clone()).expect("ref serializes")}] }),
            ),
            submission(
                &external,
                json!({"provider": "openai", "model": "gpt-4o", "messages": [{"role": "user", "content": "hello"}]}),
            ),
        ])
        .expect("resolved submission builds");

        assert_eq!(nodes.len(), 2);
        assert!(edges.is_empty());
    }

    /// A sibling reference nested inside a component value derives the edge
    /// from the parent to that sibling.
    ///
    /// # Panics
    /// Panics when the graph does not build or derives other edges.
    #[test]
    fn build_derives_sibling_edges_from_nested_refs() {
        let parent = card_ref(CardKind::Service, "service");
        let child = card_ref(CardKind::Prompt, "prompt");
        let child_value = json!({
            "sibling": serde_json::to_value(&child).expect("ref serializes")
        });
        let (nodes, edges) = build(&[
            submission(
                &parent,
                json!({"components": [{"alias": "prompt", "ref": child_value}]}),
            ),
            submission(
                &child,
                json!({"provider": "openai", "model": "gpt-4o", "messages": [{"role": "user", "content": "hello"}]}),
            ),
        ])
        .expect("resolved submissions build");

        assert_eq!(nodes.len(), 2);
        assert_eq!(
            edges,
            vec![Edge {
                from: parent,
                to: child
            }]
        );
    }

    #[test]
    fn identity_key_includes_version_but_excludes_uid() {
        let first = card_ref(CardKind::Agent, "agent");
        let mut second = first.clone();
        second.version = VersionBlock::parse("2.0.0").expect("test version is valid");
        assert_ne!(identity_key(&first), identity_key(&second));

        second.version = first.version.clone();
        second.uid = Some(
            crate::ids::CardUid::new("01890f28-7c4a-7cc3-98e7-4f4a3c2d1b11")
                .expect("test uid is valid"),
        );
        assert_eq!(identity_key(&first), identity_key(&second));
    }

    /// Graph projection pins an unversioned submission to `0.0.0` without
    /// changing the authored submission.
    ///
    /// # Panics
    /// Panics when projection fails, changes the authored version, or pins
    /// another version.
    #[test]
    fn graph_ready_submissions_preserves_authored_version_selection() {
        let card = card_ref(CardKind::Prompt, "prompt");
        let mut authored = submission(
            &card,
            json!({"provider": "openai", "model": "gpt-4o", "messages": [{"role": "user", "content": "hello"}]}),
        );
        authored.metadata.version = None;

        let ready = graph_ready_submissions(&[authored.clone()]).expect("graph projection");

        assert!(authored.metadata.version.is_none());
        assert_eq!(
            ready[0].metadata.resolved_pin().map(ToString::to_string),
            Some("0.0.0".to_owned())
        );
    }

    /// A minimal OpenAI Chat Prompt spec for submissions that need one.
    fn prompt_spec() -> serde_json::Value {
        json!({"provider": "openai", "model": "gpt-4o", "messages": [{"role": "user", "content": "hello"}]})
    }

    /// A Service whose one component is a sibling with the given identity.
    fn sibling_service(sibling: serde_json::Value) -> CardSubmission {
        let mut service = submission(
            &card_ref(CardKind::Service, "service"),
            json!({"components": [{"alias": "child", "ref": {"sibling": sibling}}]}),
        );
        service.metadata.version = None;
        service
    }

    /// A versionless sibling names the sole unversioned submission of its
    /// kind and name and gains only a graph-only version; the authored
    /// submissions keep their omitted versions.
    ///
    /// # Panics
    /// Panics when the graph does not build with the expected edge.
    #[test]
    fn versionless_sibling_targets_sole_unversioned_submission() {
        let mut child = submission(&card_ref(CardKind::Prompt, "prompt"), prompt_spec());
        child.metadata.version = None;
        let authored = [
            sibling_service(json!({"kind": "Prompt", "name": "prompt"})),
            child,
        ];

        let ready = graph_ready_submissions(&authored).expect("graph projection");
        let (_, edges) = build(&ready).expect("graph builds");

        assert_eq!(edges.len(), 1);
        assert_eq!(edges[0].to.name.as_str(), "prompt");
        assert!(authored.iter().all(|card| card.metadata.version.is_none()));
        assert_eq!(
            ready[0].spec["components"][0]["ref"]["sibling"]["version"],
            json!("0.0.0")
        );
    }

    /// An explicit sibling version selects among several submitted versions,
    /// while a versionless sibling naming them is ambiguous and refused.
    ///
    /// # Panics
    /// Panics when the explicit link does not select `2.0.0` or the
    /// versionless link is accepted.
    #[test]
    fn sibling_versions_select_or_refuse_among_submitted_versions() {
        let first = card_ref(CardKind::Prompt, "prompt");
        let mut second = first.clone();
        second.version = VersionBlock::parse("2.0.0").expect("test version is valid");
        let versions = [
            submission(&first, prompt_spec()),
            submission(&second, prompt_spec()),
        ];

        let explicit = [
            sibling_service(json!({"kind": "Prompt", "name": "prompt", "version": "2.0.0"})),
            versions[0].clone(),
            versions[1].clone(),
        ];
        let ready = graph_ready_submissions(&explicit).expect("explicit link resolves");
        let (_, edges) = build(&ready).expect("graph builds");
        assert_eq!(edges.len(), 1);
        assert_eq!(edges[0].to, second);

        let omitted = [
            sibling_service(json!({"kind": "Prompt", "name": "prompt"})),
            versions[0].clone(),
            versions[1].clone(),
        ];
        let error = graph_ready_submissions(&omitted).expect_err("ambiguous link is refused");
        assert!(matches!(error, super::GraphError::AmbiguousSibling { .. }));
    }

    /// Placeholders never alias an authored pin: an explicit `@0.0.0`
    /// sibling finds no omitted submission, a real `0.0.0` pin is its exact
    /// target beside an omitted submission of the same kind and name, and an
    /// omitted sibling naming both is ambiguous.
    ///
    /// # Panics
    /// Panics when a placeholder matches an authored version, the exact pin
    /// is not selected, or the ambiguous link is accepted.
    #[test]
    fn placeholders_never_alias_authored_pins() {
        let mut omitted = submission(&card_ref(CardKind::Prompt, "prompt"), prompt_spec());
        omitted.metadata.version = None;
        let explicit = json!({"kind": "Prompt", "name": "prompt", "version": "0.0.0"});

        let missing = [sibling_service(explicit.clone()), omitted.clone()];
        let ready = graph_ready_submissions(&missing).expect("graph projection");
        let (_, edges) = build(&ready).expect("graph builds");
        assert!(
            edges.is_empty(),
            "an unmatched explicit pin derives no edge"
        );
        assert_eq!(ready[0].spec["components"][0]["ref"]["sibling"], explicit);
        assert_ne!(
            ready[1].metadata.resolved_pin().map(ToString::to_string),
            Some("0.0.0".to_owned())
        );

        let mut zero = card_ref(CardKind::Prompt, "prompt");
        zero.version = VersionBlock::parse("0.0.0").expect("test version is valid");
        let pinned = submission(&zero, prompt_spec());
        let exact = [sibling_service(explicit), omitted.clone(), pinned.clone()];
        let ready = graph_ready_submissions(&exact).expect("omitted and pinned stay distinct");
        let (nodes, edges) = build(&ready).expect("graph builds");
        assert_eq!(nodes.len(), 3);
        assert_eq!(edges.len(), 1);
        assert_eq!(edges[0].to, zero);

        let ambiguous = [
            sibling_service(json!({"kind": "Prompt", "name": "prompt"})),
            omitted,
            pinned,
        ];
        let error = graph_ready_submissions(&ambiguous).expect_err("omitted link is ambiguous");
        assert!(matches!(error, super::GraphError::AmbiguousSibling { .. }));
    }

    /// Two submissions with the same identity are refused instead of merged.
    ///
    /// # Panics
    /// Panics when the graph builds or fails with another error.
    #[test]
    fn duplicate_submission_identity_is_rejected() {
        let first = card_ref(CardKind::Prompt, "duplicate");
        let error = build(&[
            submission(
                &first,
                json!({"provider": "openai", "model": "gpt-4o", "messages": [{"role": "user", "content": "hello"}]}),
            ),
            submission(
                &first,
                json!({"provider": "openai", "model": "gpt-4o", "messages": [{"role": "user", "content": "hello"}]}),
            ),
        ])
        .expect_err("duplicate identities must be rejected");

        assert!(matches!(error, super::GraphError::DuplicateIdentity { .. }));
    }
}
