//! Exact Card graph resolution for local hydration and Workflow loading.

use std::collections::{BTreeMap, BTreeSet};

use wyrd_spec::{
    envelope::{Card, CardKind},
    error::WyrdError,
    reference::CardRef,
    registry::{ArtifactInventoryResponse, GetCardResponse},
};

use super::CardGraphHydrator;
use crate::cards::{CardSelector, engine::RegistryEngine, reads};

/// One fully resolved Card and the registry metadata needed to materialize it.
#[derive(Debug)]
pub(super) struct ResolvedCard {
    /// Exact server-resolved Card identity.
    pub(super) card_ref: CardRef,
    /// Server-returned Card envelope.
    pub(super) card: Card,
    /// Server-owned artifact inventory; empty under [`GraphScope::Runtime`],
    /// which never reads inventories.
    pub(super) inventory: ArtifactInventoryResponse,
    /// Validated local aliases collected from every inbound relationship.
    pub(super) aliases: BTreeSet<String>,
}

/// Exact root identity and deterministic closure produced by graph resolution.
#[derive(Debug)]
pub(super) struct ResolvedGraph {
    /// Exact root selected before traversal begins.
    pub(super) root: CardRef,
    /// Unique Cards ordered by exact reference.
    pub(super) cards: Vec<ResolvedCard>,
}

/// Which relationships a traversal follows and what it reads per Card.
#[derive(Clone, Copy, Eq, PartialEq)]
pub(super) enum GraphScope {
    /// Every typed relationship plus each Card's artifact inventory, for
    /// publishing a complete hydration bundle.
    Bundle,
    /// Only Agent and Prompt relationships and no artifact inventories, for
    /// in-memory Workflow hydration that never touches artifacts or disk.
    Runtime,
}

impl GraphScope {
    /// Return whether a relationship to `card_ref` belongs to this scope.
    fn follows(self, card_ref: &CardRef) -> bool {
        match self {
            Self::Bundle => true,
            Self::Runtime => matches!(card_ref.kind, CardKind::Agent | CardKind::Prompt),
        }
    }
}

/// One pending depth-first traversal operation.
enum GraphVisit {
    /// Loads or revisits a Card reached through an alias.
    Enter(GraphVisitEntry),
    /// Marks a Card resolved after all descendants complete.
    Exit(String),
}

/// Context carried while entering one graph node.
struct GraphVisitEntry {
    /// Exact Card identity expected from the registry.
    card_ref: CardRef,
    /// Validated bundle alias used to reach the Card.
    alias: String,
}

/// Resolution state used to identify cycles and completed shared descendants.
#[derive(Clone, Copy, Eq, PartialEq)]
enum GraphVisitState {
    /// The Card remains on the active traversal stack.
    Visiting,
    /// The Card and its reachable descendants are complete.
    Resolved,
}

/// Card and inventory loaded for one traversal entry.
struct LoadedCard {
    /// Exact identity validated against the server response.
    card_ref: CardRef,
    /// Server-returned Card envelope.
    card: Card,
    /// Server-owned artifact inventory.
    inventory: ArtifactInventoryResponse,
    /// Alias associated with this traversal entry.
    alias: String,
}

/// Owns mutable state for one deterministic depth-first graph traversal.
struct GraphTraversal<'a> {
    /// Registry engine used only at remote read boundaries.
    engine: &'a RegistryEngine,
    /// Relationships followed and reads performed per Card.
    scope: GraphScope,
    /// Pending enter and exit operations.
    pending: Vec<GraphVisit>,
    /// Active and completed state keyed by exact Card reference.
    states: BTreeMap<String, GraphVisitState>,
    /// Alias ownership keyed by alias.
    aliases: BTreeMap<String, String>,
    /// Resolved nodes keyed by exact Card reference.
    nodes: BTreeMap<String, ResolvedCard>,
    /// Entry responses already read while converting a selector or authored
    /// reference to an exact identity, keyed by exact Card reference; each is
    /// consumed once in place of a second read.
    preloaded: BTreeMap<String, GetCardResponse>,
}

impl<'a> GraphTraversal<'a> {
    /// Creates empty traversal state; [`Self::enter_loaded`] seeds it.
    fn new(engine: &'a RegistryEngine, scope: GraphScope) -> Self {
        Self {
            engine,
            scope,
            pending: Vec::new(),
            states: BTreeMap::new(),
            aliases: BTreeMap::new(),
            nodes: BTreeMap::new(),
            preloaded: BTreeMap::new(),
        }
    }

    /// Schedules an entry Card whose exact response was already read, under
    /// `alias`, so traversal starts there without reading it again.
    fn enter_loaded(&mut self, card_ref: CardRef, alias: String, response: GetCardResponse) {
        self.preloaded.insert(card_ref.to_string(), response);
        self.pending
            .push(GraphVisit::Enter(GraphVisitEntry { card_ref, alias }));
    }

    /// Drives every scheduled operation until the closure is complete.
    ///
    /// # Errors
    ///
    /// Returns the first error of [`Self::process`]. Cancellation stops the
    /// active remote read and leaves no local state.
    async fn run(&mut self) -> Result<(), WyrdError> {
        while let Some(visit) = self.next() {
            self.process(visit).await?;
        }
        Ok(())
    }

    /// Removes the next depth-first operation from the traversal stack.
    fn next(&mut self) -> Option<GraphVisit> {
        self.pending.pop()
    }

    /// Applies one traversal operation, awaiting registry reads only for a new Card.
    ///
    /// # Errors
    ///
    /// Returns an error when an alias conflicts, a cycle is detected, a registry read fails, or
    /// the loaded Card violates exact-reference or relationship invariants.
    async fn process(&mut self, visit: GraphVisit) -> Result<(), WyrdError> {
        match visit {
            GraphVisit::Exit(key) => {
                self.states.insert(key, GraphVisitState::Resolved);
                Ok(())
            }
            GraphVisit::Enter(entry) => self.process_entry(entry).await,
        }
    }

    /// Resolves one entered node and schedules its descendants before its exit marker.
    ///
    /// # Errors
    ///
    /// Returns an error for unsafe aliases, graph cycles, inconsistent registry responses,
    /// missing UIDs, untyped relationships, or failed Card and inventory reads.
    async fn process_entry(&mut self, entry: GraphVisitEntry) -> Result<(), WyrdError> {
        let key = entry.card_ref.to_string();
        register_alias(&mut self.aliases, &entry.alias, &key)?;
        match self.states.get(&key) {
            Some(GraphVisitState::Visiting) => {
                return Err(graph_error(
                    "card relationship graph contains a cycle",
                    &entry.card_ref,
                ));
            }
            Some(GraphVisitState::Resolved) => {
                if let Some(node) = self.nodes.get_mut(&key) {
                    node.aliases.insert(entry.alias);
                }
                return Ok(());
            }
            None => {
                self.states.insert(key.clone(), GraphVisitState::Visiting);
            }
        }

        let loaded = self.load_card(entry).await?;
        validate_relationships(&loaded.card, &loaded.card_ref)?;
        self.pending.push(GraphVisit::Exit(key.clone()));
        self.schedule_relationships(&loaded.card)?;
        self.insert_node(key, loaded);
        Ok(())
    }

    /// Loads and validates one exact Card plus, in bundle scope, its artifact inventory.
    ///
    /// # Errors
    ///
    /// Returns an error when a remote read fails, the response does not match the expected exact
    /// reference, or the Card has no UID.
    async fn load_card(&mut self, entry: GraphVisitEntry) -> Result<LoadedCard, WyrdError> {
        let response = match self.preloaded.remove(&entry.card_ref.to_string()) {
            Some(response) => response,
            None => reads::get_response(
                &self.engine.client,
                &CardSelector::exact(entry.card_ref.clone()),
            )
            .await
            .map_err(WyrdError::from)?,
        };
        let resolved = reads::card_ref_from_card(&response.card).map_err(WyrdError::from)?;
        if resolved != entry.card_ref {
            return Err(graph_error(
                "related Card response did not match its exact reference",
                &entry.card_ref,
            ));
        }
        let uid = entry.card_ref.uid.as_ref().ok_or_else(|| {
            graph_error(
                "hydration requires UID-bearing relationship references",
                &entry.card_ref,
            )
        })?;
        let inventory = match self.scope {
            GraphScope::Bundle => reads::list_artifacts(&self.engine.client, uid)
                .await
                .map_err(WyrdError::from)?,
            GraphScope::Runtime => ArtifactInventoryResponse {
                artifacts: Vec::new(),
            },
        };
        Ok(LoadedCard {
            card_ref: entry.card_ref,
            card: response.card,
            inventory,
            alias: entry.alias,
        })
    }

    /// Schedules in-scope outbound relationships in stable server order for depth-first traversal.
    ///
    /// # Errors
    ///
    /// Returns an error when a child alias is unsafe, conflicts with another Card, or points to
    /// a Card currently on the active traversal stack.
    fn schedule_relationships(&mut self, card: &Card) -> Result<(), WyrdError> {
        for relationship in card.relationships.outbound_refs.iter().rev() {
            if !self.scope.follows(&relationship.card_ref) {
                continue;
            }
            let child = relationship.card_ref.clone();
            let child_alias = relationship
                .alias
                .clone()
                .unwrap_or_else(|| default_alias(&child));
            validate_alias(&child_alias)?;
            let child_key = child.to_string();
            register_alias(&mut self.aliases, &child_alias, &child_key)?;
            match self.states.get(&child_key) {
                Some(GraphVisitState::Visiting) => {
                    return Err(graph_error(
                        "card relationship graph contains a cycle",
                        &child,
                    ));
                }
                Some(GraphVisitState::Resolved) => {
                    if let Some(existing) = self.nodes.get_mut(&child_key) {
                        existing.aliases.insert(child_alias);
                    }
                }
                None => self.pending.push(GraphVisit::Enter(GraphVisitEntry {
                    card_ref: child,
                    alias: child_alias,
                })),
            }
        }
        Ok(())
    }

    /// Inserts one newly loaded node after its descendants have been scheduled.
    fn insert_node(&mut self, key: String, loaded: LoadedCard) {
        self.nodes.insert(
            key,
            ResolvedCard {
                card_ref: loaded.card_ref,
                card: loaded.card,
                inventory: loaded.inventory,
                aliases: BTreeSet::from([loaded.alias]),
            },
        );
    }

    /// Converts completed traversal state into its deterministic Card list.
    fn finish(self) -> Vec<ResolvedCard> {
        self.nodes.into_values().collect()
    }
}

impl CardGraphHydrator {
    /// Resolves a selector and every reachable in-scope typed outbound relationship.
    ///
    /// A versionless selector is first converted to an exact root. Traversal validates
    /// identities, aliases, cycles, and relationship typing while loading every Card and, in
    /// bundle scope, its artifact inventory.
    ///
    /// # Errors
    ///
    /// Returns an error when root resolution or any graph read fails, or when the graph violates
    /// identity, alias, cycle, UID, or relationship invariants.
    ///
    /// Cancellation stops the active remote read and does not create local bundle state.
    pub(super) async fn resolve_graph(
        &self,
        selector: &CardSelector,
        scope: GraphScope,
    ) -> Result<ResolvedGraph, WyrdError> {
        let (root, root_response) = self.load_root(selector).await?;
        let mut traversal = GraphTraversal::new(&self.context.engine, scope);
        traversal.enter_loaded(root.clone(), String::from("root"), root_response);
        traversal.run().await?;
        Ok(ResolvedGraph {
            root,
            cards: traversal.finish(),
        })
    }

    /// Resolves authored external references and their in-scope transitive relationships.
    ///
    /// Each reference is read through its exact selector, which asserts its identity and any UID
    /// it carries; traversal then follows the registered Cards' UID-bearing relationships
    /// exactly as [`resolve_graph`](Self::resolve_graph) does. References are read in order, and
    /// a Card reached more than once is loaded once.
    ///
    /// # Errors
    ///
    /// Returns an error when any read fails or a response does not match its reference, and the
    /// traversal errors of [`resolve_graph`](Self::resolve_graph).
    ///
    /// Cancellation stops the active remote read and leaves no local state.
    pub(super) async fn resolve_refs(
        &self,
        refs: &[CardRef],
        scope: GraphScope,
    ) -> Result<Vec<ResolvedCard>, WyrdError> {
        let engine = &self.context.engine;
        let mut traversal = GraphTraversal::new(engine, scope);
        for card_ref in refs {
            let response =
                reads::get_response(&engine.client, &CardSelector::exact(card_ref.clone()))
                    .await
                    .map_err(WyrdError::from)?;
            let exact = reads::card_ref_from_card(&response.card).map_err(WyrdError::from)?;
            let alias = default_alias(&exact);
            traversal.enter_loaded(exact, alias, response);
        }
        traversal.run().await?;
        Ok(traversal.finish())
    }

    /// Loads the selected root and returns its exact identity with the reusable response.
    ///
    /// # Errors
    ///
    /// Returns an error when the selector cannot be read or the response cannot form an exact
    /// reference.
    async fn load_root(
        &self,
        selector: &CardSelector,
    ) -> Result<(CardRef, GetCardResponse), WyrdError> {
        let exact_selector = self.resolve_root_selector(selector).await?;
        let response = reads::get_response(&self.context.engine.client, &exact_selector)
            .await
            .map_err(WyrdError::from)?;
        let card_ref = reads::card_ref_from_card(&response.card).map_err(WyrdError::from)?;
        Ok((card_ref, response))
    }

    /// Converts latest-by-name lookup into an exact selector while preserving exact and UID
    /// inputs.
    ///
    /// # Errors
    ///
    /// Returns an error when a versionless root cannot be read or converted to an exact
    /// reference.
    async fn resolve_root_selector(
        &self,
        selector: &CardSelector,
    ) -> Result<CardSelector, WyrdError> {
        match selector {
            CardSelector::Named { version: None, .. } => {
                let response = reads::get_response(&self.context.engine.client, selector)
                    .await
                    .map_err(WyrdError::from)?;
                reads::card_ref_from_card(&response.card)
                    .map(CardSelector::exact)
                    .map_err(WyrdError::from)
            }
            _ => Ok(selector.clone()),
        }
    }
}

/// Rejects legacy untyped outbound relationships that cannot produce a safe closure.
///
/// # Errors
///
/// Returns an error when outbound relationships exist without typed Card references.
fn validate_relationships(card: &Card, card_ref: &CardRef) -> Result<(), WyrdError> {
    if !card.relationships.outbound.is_empty() && card.relationships.outbound_refs.is_empty() {
        return Err(graph_error(
            "server returned untyped outbound relationships; graph closure is unsafe",
            card_ref,
        ));
    }
    Ok(())
}

/// Assigns an alias to one exact Card while preventing ambiguous local paths.
///
/// # Errors
///
/// Returns an error when the alias is unsafe or already identifies another Card.
fn register_alias(
    aliases: &mut BTreeMap<String, String>,
    alias: &str,
    card_key: &str,
) -> Result<(), WyrdError> {
    validate_alias(alias)?;
    if let Some(existing) = aliases.get(alias) {
        if existing != card_key {
            return Err(WyrdError::RegistryInvalidCardSpec {
                message: "relationship aliases identify conflicting Cards".to_owned(),
                details: serde_json::json!({ "alias": alias }),
            });
        }
    } else {
        aliases.insert(alias.to_owned(), card_key.to_owned());
    }
    Ok(())
}

/// Validates an alias as one portable bundle path component.
///
/// # Errors
///
/// Returns an error when the alias is empty, traverses directories, or contains unsupported
/// path characters.
pub(super) fn validate_alias(alias: &str) -> Result<(), WyrdError> {
    if alias.is_empty()
        || alias == "."
        || alias == ".."
        || alias.contains('/')
        || alias.contains('\\')
        || alias.bytes().any(
            |byte| !matches!(byte, b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'.' | b'_' | b'-'),
        )
    {
        return Err(WyrdError::RegistryInvalidCardSpec {
            message: "relationship alias is not a safe bundle path".to_owned(),
            details: serde_json::json!({ "alias": alias }),
        });
    }
    Ok(())
}

/// Builds the deterministic fallback alias for one exact Card identity.
fn default_alias(card_ref: &CardRef) -> String {
    format!(
        "{}-{}-{}-{}",
        card_ref
            .space
            .as_ref()
            .map_or("default", |space| space.as_str()),
        card_ref.kind.wire_name(),
        card_ref.name,
        card_ref.version
    )
}

/// Creates a graph-validation error associated with one exact Card.
pub(super) fn graph_error(message: &str, card_ref: &CardRef) -> WyrdError {
    WyrdError::RegistryInvalidCardSpec {
        message: message.to_owned(),
        details: serde_json::json!({ "card_ref": card_ref }),
    }
}

/// Hydration graph resolution.
#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use wyrd_semver::VersionBlock;
    use wyrd_spec::{
        envelope::CardKind,
        ids::{CardName, SpaceName},
        reference::CardRef,
    };

    use super::{default_alias, register_alias, validate_alias};

    /// Builds the exact Card reference shared by alias tests.
    fn test_card_ref() -> CardRef {
        CardRef {
            kind: CardKind::Prompt,
            name: CardName::new("welcome").expect("test name is valid"),
            version: VersionBlock::parse("1.0.0").expect("test version is valid"),
            space: Some(SpaceName::new("default").expect("test space is valid")),
            uid: None,
        }
    }

    /// Safe aliases remain one path component and traversal aliases are rejected.
    #[test]
    fn aliases_are_path_safe() {
        assert!(validate_alias("agent_one").is_ok());
        assert!(validate_alias("../escape").is_err());
        assert!(validate_alias("nested/name").is_err());
    }

    /// Default aliases encode exact identity without requiring a UID.
    #[test]
    fn default_alias_contains_exact_identity_without_uid() {
        assert_eq!(
            default_alias(&test_card_ref()),
            "default-Prompt-welcome-1.0.0"
        );
    }

    /// One alias cannot identify two exact Card references.
    #[test]
    fn aliases_cannot_point_at_two_exact_cards() {
        let mut aliases = BTreeMap::new();
        register_alias(&mut aliases, "shared", "first").expect("first alias registers");
        assert!(register_alias(&mut aliases, "shared", "second").is_err());
        register_alias(&mut aliases, "shared", "first").expect("same alias remains valid");
    }
}
