//! Deterministic leaves-first topological sorting.

use super::{Edge, Node, TopoOrder, identity_key};
use crate::reference::CardRef;

/// Errors returned by the pure submission graph operations.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum GraphError {
    /// A cycle remains after all acyclic leaves have been removed.
    #[error("dependency cycle among submissions: {cycle:?}")]
    Cycle {
        /// Unresolved nodes that participate in or depend on the cycle.
        cycle: Vec<CardRef>,
    },
    /// More than one submission has no incoming edge.
    #[error("multiple roots detected: {candidates:?}")]
    MultipleRoots {
        /// Candidate root references.
        candidates: Vec<CardRef>,
    },
    /// No submissions were supplied.
    #[error("empty submission set")]
    Empty,
    /// A submission has no resolved space identity.
    #[error("submission metadata.space is required for graph operations")]
    MissingSpace,
    /// A submission identity was repeated within one request.
    #[error("duplicate submission identity: {candidates:?}")]
    DuplicateIdentity {
        /// Repeated submission references.
        candidates: Vec<CardRef>,
    },
    /// A Service lists a peer-only card kind as an owned component.
    #[error(
        "Service {service} component {alias} cannot reference peer-only card {component} at {field}"
    )]
    InvalidServiceComponentKind {
        /// Service declaring the invalid component.
        service: Box<CardRef>,
        /// Authored component alias.
        alias: String,
        /// Peer card incorrectly declared as a component.
        component: Box<CardRef>,
        /// Exact component field containing the invalid reference.
        field: String,
    },
    /// A Service-root bundle contains an Eval or Drift with no local publisher.
    #[error("Service-root bundle {root} contains unpublished observability peer {peer}")]
    UnpublishedObservabilityPeer {
        /// Selected Service root for the composite submission.
        root: Box<CardRef>,
        /// Eval or Drift submission with no incoming publication.
        peer: Box<CardRef>,
    },
    /// A submission spec could not be decoded for typed reference traversal.
    #[error("invalid submission spec: {message}")]
    InvalidSpec {
        /// Decode failure detail.
        message: String,
    },
}

/// Sort a graph with Kahn's algorithm, emitting leaves before their parents.
///
/// Edges are represented from parent to referenced child. The implementation
/// removes zero-outdegree leaves, which is the reverse of the usual Kahn
/// emission direction and gives the composite registration pipeline its
/// dependency-first order. Ready leaves are selected by
/// `(kind, space, name, version)`.
///
/// # Errors
/// Returns [`GraphError::Empty`] for an empty node set and
/// [`GraphError::Cycle`] when nodes remain unresolved after leaf removal.
pub fn topo_sort(nodes: &[Node], edges: &[Edge]) -> Result<TopoOrder, GraphError> {
    if nodes.is_empty() {
        return Err(GraphError::Empty);
    }

    let index_by_identity = nodes
        .iter()
        .enumerate()
        .map(|(index, node)| (identity_key(&node.card_ref), index))
        .collect::<std::collections::BTreeMap<_, _>>();
    let mut remaining_children = vec![0usize; nodes.len()];
    let mut parent_edges = vec![Vec::new(); nodes.len()];

    for edge in edges {
        let (Some(&from), Some(&to)) = (
            index_by_identity.get(&identity_key(&edge.from)),
            index_by_identity.get(&identity_key(&edge.to)),
        ) else {
            continue;
        };
        remaining_children[from] += 1;
        parent_edges[to].push(from);
    }

    let mut ready = nodes
        .iter()
        .enumerate()
        .filter_map(|(index, _)| (remaining_children[index] == 0).then_some(index))
        .collect::<Vec<_>>();
    let mut ordered = Vec::with_capacity(nodes.len());

    while let Some(index) = pop_canonical(&mut ready, nodes) {
        ordered.push(nodes[index].clone());
        for parent in parent_edges[index].drain(..) {
            remaining_children[parent] -= 1;
            if remaining_children[parent] == 0 {
                ready.push(parent);
            }
        }
    }

    if ordered.len() != nodes.len() {
        let mut cycle = nodes
            .iter()
            .enumerate()
            .filter_map(|(index, node)| {
                (remaining_children[index] != 0).then_some(node.card_ref.clone())
            })
            .collect::<Vec<_>>();
        cycle.sort_by_key(identity_key);
        return Err(GraphError::Cycle { cycle });
    }

    Ok(TopoOrder {
        nodes: ordered,
        edges: edges.to_vec(),
    })
}

fn pop_canonical(ready: &mut Vec<usize>, nodes: &[Node]) -> Option<usize> {
    let position = ready
        .iter()
        .enumerate()
        .min_by_key(|(_, index)| identity_key(&nodes[**index].card_ref))
        .map(|(position, _)| position)?;
    Some(ready.swap_remove(position))
}

#[cfg(test)]
mod tests {
    use super::{GraphError, topo_sort};
    use crate::envelope::CardKind;
    use crate::graph::{Edge, Node};
    use crate::reference::CardRef;
    use wyrd_semver::VersionBlock;

    fn card_ref(kind: CardKind, name: &str) -> CardRef {
        CardRef {
            kind,
            name: name.parse().expect("test card name is valid"),
            version: VersionBlock::parse("1.0.0").expect("test version is valid"),
            space: Some("default".parse().expect("test space is valid")),
            uid: None,
        }
    }

    fn node(kind: CardKind, name: &str) -> Node {
        Node {
            card_ref: card_ref(kind, name),
        }
    }

    fn edge(from: &Node, to: &Node) -> Edge {
        Edge {
            from: from.card_ref.clone(),
            to: to.card_ref.clone(),
        }
    }

    #[test]
    fn topo_sort_single_root_leaf_first() {
        let prompt = node(CardKind::Prompt, "prompt");
        let agent = node(CardKind::Agent, "agent");
        let service = node(CardKind::Service, "service");
        let edges = vec![edge(&service, &agent), edge(&agent, &prompt)];
        let order = topo_sort(&[service.clone(), prompt.clone(), agent.clone()], &edges)
            .expect("chain is acyclic");

        assert_eq!(
            order.nodes,
            vec![prompt.clone(), agent.clone(), service.clone()]
        );
        assert_eq!(order.edges, edges);
    }

    #[test]
    fn topo_sort_tie_break_canonical() {
        let agent = node(CardKind::Agent, "agent");
        let prompt_b = node(CardKind::Prompt, "prompt-b");
        let prompt_a = node(CardKind::Prompt, "prompt-a");
        let service = node(CardKind::Service, "service");
        let edges = vec![
            edge(&agent, &prompt_b),
            edge(&agent, &prompt_a),
            edge(&service, &agent),
        ];
        let nodes = [
            service.clone(),
            prompt_b.clone(),
            agent.clone(),
            prompt_a.clone(),
        ];

        let first = topo_sort(&nodes, &edges).expect("graph is acyclic");
        let second = topo_sort(&nodes, &edges).expect("graph is acyclic");
        assert_eq!(first, second);
        assert_eq!(first.nodes, vec![prompt_a, prompt_b, agent, service]);
    }

    #[test]
    fn topo_sort_cycle_returns_cycle_error() {
        let a = node(CardKind::Agent, "agent-a");
        let b = node(CardKind::Agent, "agent-b");
        let c = node(CardKind::Agent, "agent-c");
        let error = topo_sort(
            &[a.clone(), b.clone(), c.clone()],
            &[edge(&a, &b), edge(&b, &c), edge(&c, &a)],
        )
        .expect_err("cycle must be rejected");

        let GraphError::Cycle { cycle } = error else {
            panic!("expected cycle error");
        };
        assert_eq!(cycle.len(), 3);
        assert!(cycle.contains(&a.card_ref));
        assert!(cycle.contains(&b.card_ref));
        assert!(cycle.contains(&c.card_ref));
    }
}
