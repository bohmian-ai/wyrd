//! Root selection for a submission graph.

use super::topo::GraphError;
use super::{RootPick, TopoOrder, identity_key};

/// Select the sole node with no incoming edge from another submission.
///
/// The graph stores edges from parent to referenced child, so an incoming edge
/// is the signal that a node is a dependency of another submission. A valid
/// composite request therefore has exactly one root with no incoming edge.
///
/// # Errors
/// Returns [`GraphError::Empty`] for an empty order and
/// [`GraphError::MultipleRoots`] when more than one root candidate exists.
pub fn pick_root(order: &TopoOrder) -> Result<RootPick, GraphError> {
    if order.nodes.is_empty() {
        return Err(GraphError::Empty);
    }

    let candidates = order
        .nodes
        .iter()
        .filter(|node| {
            !order
                .edges
                .iter()
                .any(|edge| identity_key(&edge.to) == identity_key(&node.card_ref))
        })
        .map(|node| node.card_ref.clone())
        .collect::<Vec<_>>();

    match candidates.as_slice() {
        [root] => Ok(RootPick { root: root.clone() }),
        _ => {
            let mut candidates = candidates;
            candidates.sort_by_key(identity_key);
            Err(GraphError::MultipleRoots { candidates })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::pick_root;
    use crate::envelope::CardKind;
    use crate::graph::{Edge, Node, RootPick, TopoOrder};
    use crate::reference::CardRef;
    use wyrd_semver::VersionBlock;

    fn card_ref(kind: CardKind, name: &str) -> CardRef {
        CardRef {
            kind,
            name: name.parse().expect("test card name is valid"),
            version: VersionBlock::parse("1.0.0").expect("test version is valid"),
            space: "default".parse().expect("test space is valid"),
            uid: None,
        }
    }

    fn order(nodes: Vec<Node>, edges: Vec<Edge>) -> TopoOrder {
        TopoOrder { nodes, edges }
    }

    fn node(card_ref: CardRef) -> Node {
        Node { card_ref }
    }

    #[test]
    fn pick_root_single_root() {
        let prompt = card_ref(CardKind::Prompt, "prompt");
        let agent = card_ref(CardKind::Agent, "agent");
        let service = card_ref(CardKind::Service, "service");
        let result = pick_root(&order(
            vec![
                node(prompt.clone()),
                node(agent.clone()),
                node(service.clone()),
            ],
            vec![
                Edge {
                    from: service.clone(),
                    to: agent.clone(),
                },
                Edge {
                    from: agent,
                    to: prompt,
                },
            ],
        ))
        .expect("chain has one root");

        assert_eq!(result, RootPick { root: service });
    }

    #[test]
    fn pick_root_multiple_roots_returns_multiple_roots_error() {
        let left = card_ref(CardKind::Agent, "left");
        let right = card_ref(CardKind::Service, "right");
        let error = pick_root(&order(
            vec![node(right.clone()), node(left.clone())],
            Vec::new(),
        ))
        .expect_err("independent nodes have multiple roots");

        assert!(matches!(
            error,
            super::GraphError::MultipleRoots { candidates }
                if candidates.contains(&left) && candidates.contains(&right)
        ));
    }

    #[test]
    fn pick_root_empty_returns_empty_error() {
        assert_eq!(
            pick_root(&order(Vec::new(), Vec::new())),
            Err(super::GraphError::Empty)
        );
    }
}
