//! Pure graph operations for composite Card registration.
//!
//! The graph uses authored `CardRef` identity coordinates to connect sibling
//! submissions. It does not resolve external references or perform any I/O.

use std::collections::{BTreeMap, BTreeSet};
use std::hash::{Hash, Hasher};

use crate::envelope::{ReferenceSlotVisitor, Spec};
use crate::reference::CardRef;
use crate::registry::CardSubmission;
use wyrd_semver::{VersionBlock, VersionSpec};

mod canonical;
mod root;
mod topo;

pub use canonical::canonical_order;
pub use root::pick_root;
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

fn hash_card_ref<H: Hasher>(card_ref: &CardRef, state: &mut H) {
    card_ref.kind.hash(state);
    card_ref.name.hash(state);
    card_ref.version.hash(state);
    card_ref.space.hash(state);
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
/// Graph identity is `(kind, space, name)`, but [`CardRef`] also requires a
/// concrete version. Auto and range version requests therefore receive a
/// graph-only placeholder. The authored submissions are cloned and never
/// mutated; registration keeps their original version selection.
pub fn graph_ready_submissions(
    submissions: &[CardSubmission],
) -> Result<Vec<CardSubmission>, GraphError> {
    if submissions.is_empty() {
        return Err(GraphError::Empty);
    }

    let placeholder = VersionBlock::parse("0.0.0").map_err(|error| GraphError::InvalidSpec {
        message: format!("invalid graph placeholder: {error}"),
    })?;
    submissions
        .iter()
        .cloned()
        .map(|mut submission| {
            if submission.metadata.space.is_none() {
                return Err(GraphError::MissingSpace);
            }
            if !submission
                .metadata
                .version
                .as_ref()
                .is_some_and(VersionSpec::is_pin)
            {
                submission.metadata.version = Some(VersionSpec::Pin(placeholder.clone()));
            }
            Ok(submission)
        })
        .collect()
}

/// Build the submission graph from CardRef-shaped objects in each spec.
///
/// A reference becomes an edge only when its `(kind, space, name)` identity
/// matches another submission in the same request. References to cards outside
/// the request are deliberately ignored; external resolution belongs to the
/// server registry boundary.
///
/// The register boundary supplies resolved metadata before calling this helper:
/// each submission must have a concrete version and space. The graph itself
/// compares nodes by `(kind, space, name)`, so version and UID do not affect
/// sibling matching.
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
        let mut collector = RefCollector {
            references: Vec::new(),
        };
        spec.walk_refs(&mut collector);
        for child in collector.references {
            let Some(target) = siblings.get(&identity_key(&child)) else {
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

pub(crate) fn identity_key(card_ref: &CardRef) -> (String, String, String) {
    (
        card_ref.kind.wire_name().to_owned(),
        card_ref.space.as_str().to_owned(),
        card_ref.name.as_str().to_owned(),
    )
}

fn submission_card_ref(submission: &CardSubmission) -> Option<CardRef> {
    Some(CardRef {
        kind: submission.kind.clone(),
        name: submission.metadata.name.clone(),
        version: submission.metadata.resolved_pin()?.clone(),
        space: submission.metadata.space.clone()?,
        uid: submission.metadata.uid.clone(),
    })
}

struct RefCollector {
    references: Vec<CardRef>,
}

impl ReferenceSlotVisitor for RefCollector {
    fn visit_ref(&mut self, card_ref: &CardRef) {
        self.references.push(card_ref.clone());
    }

    fn visit_ref_mut(&mut self, _card_ref: &mut CardRef) {}
}

#[cfg(test)]
mod tests {
    use super::{Edge, Node, build, graph_ready_submissions, identity_key};
    use crate::api_version::ApiVersion;
    use crate::envelope::{CardKind, Metadata};
    use crate::reference::CardRef;
    use crate::registry::CardSubmission;
    use serde_json::json;
    use wyrd_semver::{VersionBlock, VersionSpec};

    fn card_ref(kind: CardKind, name: &str) -> CardRef {
        CardRef {
            kind,
            name: name.parse().expect("test card name is valid"),
            version: VersionBlock::parse("1.0.0").expect("test version is valid"),
            space: "default".parse().expect("test space is valid"),
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
                space: Some(card_ref.space.clone()),
                uid: card_ref.uid.clone(),
                labels: Default::default(),
                annotations: Default::default(),
                spec_hash: None,
                artifact_hash: None,
                origin: None,
            },
            spec,
            artifacts: Vec::new(),
        }
    }

    #[test]
    fn build_ignores_external_refs() {
        let parent = card_ref(CardKind::Service, "service");
        let external = card_ref(CardKind::Agent, "external");
        let (nodes, edges) = build(&[submission(
            &parent,
            json!({"components": [{"alias": "external", "ref": serde_json::to_value(external).expect("ref serializes")}] }),
        )])
        .expect("resolved submission builds");

        assert_eq!(nodes, vec![Node { card_ref: parent }]);
        assert!(edges.is_empty());
    }

    #[test]
    fn build_derives_sibling_edges_from_nested_refs() {
        let parent = card_ref(CardKind::Service, "service");
        let child = card_ref(CardKind::Prompt, "prompt");
        let child_value = serde_json::to_value(&child).expect("ref serializes");
        let (nodes, edges) = build(&[
            submission(
                &parent,
                json!({"components": [{"alias": "prompt", "ref": child_value}]}),
            ),
            submission(
                &child,
                json!({"provider": "openai", "model": "gpt-4o", "messages": ["hello"]}),
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
    fn identity_key_excludes_version_and_uid() {
        let first = card_ref(CardKind::Agent, "agent");
        let mut second = first.clone();
        second.version = VersionBlock::parse("2.0.0").expect("test version is valid");
        assert_eq!(identity_key(&first), identity_key(&second));
    }

    #[test]
    fn graph_ready_submissions_preserves_authored_version_selection() {
        let card = card_ref(CardKind::Prompt, "prompt");
        let mut authored = submission(
            &card,
            json!({"provider": "openai", "model": "gpt-4o", "messages": ["hello"]}),
        );
        authored.metadata.version = None;

        let ready = graph_ready_submissions(&[authored.clone()]).expect("graph projection");

        assert!(authored.metadata.version.is_none());
        assert_eq!(
            ready[0].metadata.resolved_pin().map(ToString::to_string),
            Some("0.0.0".to_owned())
        );
    }

    #[test]
    fn duplicate_submission_identity_is_rejected() {
        let first = card_ref(CardKind::Prompt, "duplicate");
        let error = build(&[
            submission(
                &first,
                json!({"provider": "openai", "model": "gpt-4o", "messages": ["hello"]}),
            ),
            submission(
                &first,
                json!({"provider": "openai", "model": "gpt-4o", "messages": ["hello"]}),
            ),
        ])
        .expect_err("duplicate identities must be rejected");

        assert!(matches!(error, super::GraphError::DuplicateIdentity { .. }));
    }
}
