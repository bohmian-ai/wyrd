//! In-memory Workflow composition over the exact Card graph.
//!
//! A Workflow hydrates from two body sources kept apart by provenance: loader
//! siblings an authored bundle supplied from disk, and registered Cards the
//! graph traversal read exactly from the registry. Skald's
//! [`Workflow::from_card_bodies`] consumes them through one lookup that serves
//! a `Sibling` slot only from siblings and a `Ref` slot only from registered
//! Cards, so a local body never satisfies an external reference with the same
//! identity. Composition writes nothing to disk and reads no artifacts.

use std::collections::HashMap;
use std::path::Path;

use skald_workflow::{Workflow, card_body_dependencies};
use wyrd_loader::LoadedTree;
use wyrd_spec::api_version::ApiVersion;
use wyrd_spec::card::workflow::WorkflowCard;
use wyrd_spec::envelope::{Card, CardKind, Relationships, Spec};
use wyrd_spec::error::WyrdError;
use wyrd_spec::reference::{CardRef, CardRefIdentity, Ref};
use wyrd_spec::registry::CardSubmission;

use crate::cards::{CardGraphHydrator, CardSelector};

use super::graph::{GraphScope, ResolvedCard, resolve_graph, resolve_refs};

/// Exact Agent and Prompt bodies one Workflow hydration consumes, by
/// provenance.
pub(crate) struct WorkflowBodies {
    /// Loader sibling bodies keyed by the identity their `Sibling` slots name.
    siblings: HashMap<CardRefIdentity, Spec>,
    /// Active registered Cards read exactly by the graph traversal.
    registered: Vec<Card>,
}

impl WorkflowBodies {
    /// Select the Workflow Card defined in `entry` and record every loaded
    /// Agent and Prompt in `tree` as a sibling body.
    ///
    /// # Errors
    /// Returns `WYRD_REGISTRY_400_INVALID_CARD_SPEC` when `entry` does not
    /// define exactly one Workflow Card, or a loaded Card is undecodable or
    /// lacks an exact space and version; and the Workflow envelope errors of
    /// [`WorkflowCard::from_envelope`].
    pub(crate) fn authored(
        tree: &LoadedTree,
        entry: &Path,
    ) -> Result<(WorkflowCard, Self), WyrdError> {
        let mut workflows = tree
            .cards
            .iter()
            .filter(|card| card.submission.kind == CardKind::Workflow && card.source_path == entry);
        let (Some(workflow), None) = (workflows.next(), workflows.next()) else {
            return Err(WyrdError::registry_invalid_card_spec(format!(
                "{} must contain exactly one Workflow Card",
                entry.display()
            )));
        };
        let workflow = WorkflowCard::from_envelope(submission_card(&workflow.submission)?)?;
        let mut siblings = HashMap::new();
        for loaded in &tree.cards {
            if matches!(loaded.submission.kind, CardKind::Agent | CardKind::Prompt) {
                let card = submission_card(&loaded.submission)?;
                siblings.insert(exact_ref(&card)?.identity_key(), card.spec);
            }
        }
        Ok((
            workflow,
            Self {
                siblings,
                registered: Vec::new(),
            },
        ))
    }

    /// Return the external references `workflow` reaches through its steps
    /// and the sibling bodies they name, deduplicated in discovery order.
    ///
    /// Sibling slots are followed into their local bodies; external slots
    /// stop here because the graph traversal reads their transitive closure
    /// from registered relationships.
    #[must_use]
    pub(crate) fn external_refs(&self, workflow: &WorkflowCard) -> Vec<CardRef> {
        let mut pending = card_body_dependencies(&Spec::Workflow(workflow.spec.clone()));
        let mut refs: Vec<CardRef> = Vec::new();
        while let Some(dependency) = pending.pop() {
            match dependency {
                Ref::Ref(card_ref) => {
                    if !refs.iter().any(|known| known.same_identity(&card_ref)) {
                        refs.push(card_ref);
                    }
                }
                Ref::Sibling { sibling } => {
                    if let Some(body) = self.siblings.get(&sibling.identity_key()) {
                        pending.extend(card_body_dependencies(body));
                    }
                }
                Ref::Path(_) => {}
            }
        }
        refs
    }

    /// Record registered Cards a traversal read, refusing any that is not
    /// active.
    ///
    /// # Errors
    /// Returns `WYRD_REGISTRY_422_UNRESOLVED_DEPENDENCY` naming the first Card
    /// whose status is not active.
    fn extend_registered(&mut self, cards: Vec<ResolvedCard>) -> Result<(), WyrdError> {
        for resolved in cards {
            let active = resolved
                .card
                .status
                .as_ref()
                .is_some_and(|status| status.phase == "active");
            if !active {
                return Err(WyrdError::RegistryUnresolvedDependency {
                    message: format!("card dependency {} is not active", resolved.card_ref),
                    details: serde_json::json!({ "card_ref": resolved.card_ref.to_string() }),
                });
            }
            self.registered.push(resolved.card);
        }
        Ok(())
    }

    /// Hydrate the runnable Skald Workflow from these bodies, binding Agent
    /// tools from the process-default tool registry.
    ///
    /// # Errors
    /// Returns the errors of [`Workflow::from_card_bodies`].
    pub(crate) fn hydrate(&self, workflow: WorkflowCard) -> Result<Workflow, WyrdError> {
        Workflow::from_card_bodies(workflow, skald_tool::default_registry(), &|dependency| {
            self.body(dependency)
        })
    }

    /// Return the body a reference names from its own provenance.
    ///
    /// A `Sibling` reads only loader siblings. An external `Ref` reads only a
    /// registered Card with the same identity and, when the reference pins
    /// one, the same UID. Paths have no body.
    fn body(&self, dependency: &Ref) -> Option<Spec> {
        match dependency {
            Ref::Sibling { sibling } => self.siblings.get(&sibling.identity_key()).cloned(),
            Ref::Ref(card_ref) => self
                .registered
                .iter()
                .find(|card| {
                    exact_ref(card).is_ok_and(|exact| {
                        exact.same_identity(card_ref)
                            && card_ref
                                .uid
                                .as_ref()
                                .is_none_or(|uid| exact.uid.as_ref() == Some(uid))
                    })
                })
                .map(|card| card.spec.clone()),
            Ref::Path(_) => None,
        }
    }
}

impl CardGraphHydrator {
    /// Read authored external references and their locked Agent and Prompt
    /// closure into `bodies`.
    ///
    /// Cancellation stops the active registry read; nothing is written.
    ///
    /// # Errors
    /// Returns the read and traversal errors of the exact graph traversal and
    /// the inactive-Card refusal of [`WorkflowBodies::extend_registered`].
    pub(crate) async fn resolve_external(
        &self,
        bodies: &mut WorkflowBodies,
        refs: &[CardRef],
    ) -> Result<(), WyrdError> {
        let cards = resolve_refs(&self.context.engine, refs, GraphScope::Runtime).await?;
        bodies.extend_registered(cards)
    }

    /// Read a registered Workflow and its locked Agent and Prompt closure,
    /// then hydrate it.
    ///
    /// Every Card is read by exact identity along UID-bearing relationships,
    /// so later versions never float in; artifact inventories are not read.
    ///
    /// Cancellation stops the active registry read; nothing is written.
    ///
    /// # Errors
    /// Returns the graph traversal errors, `WYRD_REGISTRY_422_UNRESOLVED_DEPENDENCY`
    /// for any Card in the closure that is not active, the Workflow envelope
    /// error when the root is not a Workflow, and the errors of
    /// [`WorkflowBodies::hydrate`].
    pub(crate) async fn load_workflow(
        &self,
        selector: &CardSelector,
    ) -> Result<Workflow, WyrdError> {
        let graph = resolve_graph(&self.context.engine, selector, GraphScope::Runtime).await?;
        let root = graph
            .cards
            .iter()
            .find(|card| card.card_ref == graph.root)
            .map(|card| card.card.clone())
            .ok_or_else(|| WyrdError::Internal {
                message: "resolved graph omitted its root Card".to_owned(),
                details: serde_json::json!({ "card_ref": graph.root.to_string() }),
            })?;
        let mut bodies = WorkflowBodies {
            siblings: HashMap::new(),
            registered: Vec::new(),
        };
        bodies.extend_registered(graph.cards)?;
        bodies.hydrate(WorkflowCard::from_envelope(root)?)
    }
}

/// Convert a loader submission into a Card envelope.
///
/// # Errors
/// Returns `WYRD_REGISTRY_400_INVALID_CARD_SPEC` for an undecodable spec.
fn submission_card(submission: &CardSubmission) -> Result<Card, WyrdError> {
    let spec = Spec::from_kind_and_value(&submission.kind, submission.spec.clone())
        .map_err(|error| WyrdError::registry_invalid_card_spec(error.to_string()))?;
    Ok(Card {
        api_version: ApiVersion::v1(),
        kind: submission.kind.clone(),
        metadata: submission.metadata.clone(),
        spec,
        relationships: Relationships::default(),
        status: None,
    })
}

/// Return the exact reference a Card's identity names.
///
/// # Errors
/// Returns `WYRD_REGISTRY_400_INVALID_CARD_SPEC` when the Card lacks a
/// resolved space or exact version.
fn exact_ref(card: &Card) -> Result<CardRef, WyrdError> {
    match (&card.metadata.space, card.metadata.resolved_pin()) {
        (Some(space), Some(version)) => Ok(CardRef {
            kind: card.kind.clone(),
            name: card.metadata.name.clone(),
            version: version.clone(),
            space: Some(space.clone()),
            uid: card.metadata.uid.clone(),
        }),
        _ => Err(WyrdError::registry_invalid_card_spec(format!(
            "{} {} has no exact space and version",
            card.kind.wire_name(),
            card.metadata.name
        ))),
    }
}
