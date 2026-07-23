//! Reference resolution - collapse Path variants to Ref variants.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use wyrd_spec::reference::{CardRef, InlineableRef, Ref};
use wyrd_spec::refs::{ReferenceSlotVisitor, SlotValue};

use super::error::Diagnostic;
use super::parse::{AuthoredCard, card_ref_for, parse_file_with_sandbox};
use super::path::PathSandbox;

/// Resolve all path references in the tree, rewriting them to sibling variants.
///
/// Every `Ref::Path` becomes `Ref::Sibling { sibling: CardRef }` bound to a sibling
/// submission. Path-loaded cards are appended to `cards` so transitive paths
/// participate in validation and topological ordering.
pub fn resolve_tree(
    cards: &mut Vec<AuthoredCard>,
    sandbox: &PathSandbox,
    config: &wyrd_config::WyrdConfig,
) -> Vec<Diagnostic> {
    let mut state = ResolutionState::new(cards);

    let mut index = 0;
    while index < cards.len() {
        let source_path = cards[index].source_path.clone();
        let source_depth = state.depth_by_path.get(&source_path).copied().unwrap_or(0);
        let discovered =
            resolve_card_references(&mut cards[index], source_depth, sandbox, config, &mut state);
        merge_discovered_cards(cards, discovered, source_depth, &mut state.depth_by_path);
        index += 1;
    }

    state.diagnostics
}

/// Mutable state shared by every card visit in one resolution pass.
struct ResolutionState {
    diagnostics: Vec<Diagnostic>,
    path_cache: HashMap<PathBuf, CardRef>,
    card_refs_by_path: HashMap<PathBuf, CardRef>,
    depth_by_path: HashMap<PathBuf, usize>,
}

impl ResolutionState {
    /// Seed caches and depth information from the cards supplied by the caller.
    fn new(cards: &[AuthoredCard]) -> Self {
        Self {
            diagnostics: Vec::new(),
            path_cache: HashMap::new(),
            card_refs_by_path: cards
                .iter()
                .filter_map(|card| {
                    card_ref_for(card)
                        .ok()
                        .map(|card_ref| (card.source_path.clone(), card_ref))
                })
                .collect(),
            depth_by_path: cards
                .iter()
                .map(|card| (card.source_path.clone(), 0usize))
                .collect(),
        }
    }
}

/// Visit one card's reference slots and collect any newly loaded cards.
fn resolve_card_references(
    card: &mut AuthoredCard,
    source_depth: usize,
    sandbox: &PathSandbox,
    config: &wyrd_config::WyrdConfig,
    state: &mut ResolutionState,
) -> Vec<AuthoredCard> {
    let source_path = card.source_path.clone();
    let parent_space = card.metadata.space.clone();
    let mut discovered = Vec::new();
    let mut resolver = PathResolver {
        sandbox,
        known_cards: &mut state.card_refs_by_path,
        path_cache: &mut state.path_cache,
        diagnostics: &mut state.diagnostics,
        config,
    };

    ReferenceSlotVisitor::visit(&mut card.spec, |entry| {
        resolve_reference_slot(
            &mut resolver,
            entry.value,
            &source_path,
            parent_space.as_ref(),
            &mut discovered,
            source_depth,
        );
    });
    discovered
}

/// Rewrite one path-bearing slot and retain diagnostics for failed imports.
fn resolve_reference_slot(
    resolver: &mut PathResolver<'_>,
    value: SlotValue<'_>,
    source_path: &Path,
    parent_space: Option<&wyrd_spec::ids::SpaceName>,
    discovered: &mut Vec<AuthoredCard>,
    source_depth: usize,
) {
    match value {
        SlotValue::Durable(ref_slot) => {
            inherit_space(ref_slot.as_card_ref_mut(), parent_space);
            if let Ref::Path(path) = ref_slot {
                match resolver.resolve(source_path, path, discovered, source_depth) {
                    Ok(card_ref) => *ref_slot = Ref::Sibling { sibling: card_ref },
                    Err(diagnostic) => resolver.diagnostics.push(diagnostic),
                }
            }
        }
        SlotValue::InlineablePrompt(ref_slot) => {
            inherit_space(ref_slot.as_card_ref_mut(), parent_space);
            if let InlineableRef::Path(path) = ref_slot {
                match resolver.resolve(source_path, path, discovered, source_depth) {
                    Ok(card_ref) => *ref_slot = InlineableRef::Sibling { sibling: card_ref },
                    Err(diagnostic) => resolver.diagnostics.push(diagnostic),
                }
            }
        }
        SlotValue::InlineableAgent(ref_slot) => {
            inherit_space(ref_slot.as_card_ref_mut(), parent_space);
            if let InlineableRef::Path(path) = ref_slot {
                match resolver.resolve(source_path, path, discovered, source_depth) {
                    Ok(card_ref) => *ref_slot = InlineableRef::Sibling { sibling: card_ref },
                    Err(diagnostic) => resolver.diagnostics.push(diagnostic),
                }
            }
        }
    }
}

/// Append newly imported cards once and preserve their transitive depth.
fn merge_discovered_cards(
    cards: &mut Vec<AuthoredCard>,
    discovered: Vec<AuthoredCard>,
    source_depth: usize,
    depth_by_path: &mut HashMap<PathBuf, usize>,
) {
    for discovered_card in discovered {
        if !cards
            .iter()
            .any(|card| card.source_path == discovered_card.source_path)
        {
            depth_by_path.insert(discovered_card.source_path.clone(), source_depth + 1);
            cards.push(discovered_card);
        }
    }
}

struct PathResolver<'a> {
    sandbox: &'a PathSandbox,
    known_cards: &'a mut HashMap<PathBuf, CardRef>,
    path_cache: &'a mut HashMap<PathBuf, CardRef>,
    diagnostics: &'a mut Vec<Diagnostic>,
    config: &'a wyrd_config::WyrdConfig,
}

impl PathResolver<'_> {
    /// Resolve one authored path reference and register the discovered card.
    ///
    /// Resolution is bounded by the path-depth limit, uses the sandbox for
    /// filesystem containment, reuses cached card identities, and accepts only
    /// a single card from a referenced file.
    fn resolve(
        &mut self,
        base_file: &Path,
        ref_path: &Path,
        discovered: &mut Vec<AuthoredCard>,
        source_depth: usize,
    ) -> Result<CardRef, Diagnostic> {
        Self::check_depth(base_file, source_depth)?;
        let resolved_path = self.resolve_path(base_file, ref_path)?;
        if let Some(cached) = self.path_cache.get(&resolved_path) {
            return Ok(cached.clone());
        }
        if let Some(card_ref) = self.known_cards.get(&resolved_path) {
            self.path_cache.insert(resolved_path, card_ref.clone());
            return Ok(card_ref.clone());
        }

        let mut loaded = self.load_single_card(base_file, ref_path, &resolved_path)?;
        super::config::apply_defaults(std::slice::from_mut(&mut loaded), self.config);
        let card_ref = card_ref_for(&loaded)?;
        self.known_cards
            .insert(resolved_path.clone(), card_ref.clone());
        self.path_cache.insert(resolved_path, card_ref.clone());
        discovered.push(loaded);
        Ok(card_ref)
    }

    /// Reject references that would exceed the maximum supported import depth.
    fn check_depth(base_file: &Path, source_depth: usize) -> Result<(), Diagnostic> {
        if source_depth >= 8 {
            return Err(Diagnostic::invalid_envelope(
                base_file.to_path_buf(),
                "Path reference depth exceeds the maximum of 8".to_owned(),
            ));
        }
        Ok(())
    }

    /// Resolve a reference path through the sandbox and retain any advisory
    /// produced for an allowed absolute path.
    fn resolve_path(&mut self, base_file: &Path, ref_path: &Path) -> Result<PathBuf, Diagnostic> {
        let (resolved_path, advisory) = self.sandbox.resolve(base_file, ref_path)?;
        if let Some(advisory) = advisory {
            self.diagnostics.push(advisory);
        }
        Ok(resolved_path)
    }

    /// Parse one referenced file and enforce the single-card path-import rule.
    fn load_single_card(
        &mut self,
        base_file: &Path,
        ref_path: &Path,
        resolved_path: &Path,
    ) -> Result<AuthoredCard, Diagnostic> {
        let mut loaded_cards =
            parse_file_with_sandbox(resolved_path, self.sandbox).map_err(|error| {
                self.diagnostics.extend(error.diagnostics);
                Diagnostic::invalid_envelope(
                    base_file.to_path_buf(),
                    format!(
                        "referenced card failed to parse: {}",
                        resolved_path.display()
                    ),
                )
            })?;
        match loaded_cards.len() {
            0 => Err(Diagnostic::invalid_envelope(
                base_file.to_path_buf(),
                format!("Referenced file is empty: {}", ref_path.display()),
            )),
            1 => Ok(loaded_cards.remove(0)),
            _ => Err(Diagnostic::invalid_envelope(
                base_file.to_path_buf(),
                format!(
                    "Referenced file contains multiple cards: {}",
                    ref_path.display()
                ),
            )),
        }
    }
}

fn inherit_space(card_ref: Option<&mut CardRef>, parent_space: Option<&wyrd_spec::ids::SpaceName>) {
    if let Some(card_ref) = card_ref
        && card_ref.space.is_none()
    {
        card_ref.space = parent_space.cloned();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::parse_file;
    use tempfile::TempDir;

    #[test]
    fn resolve_path_rewrites_durable_ref_and_loads_sibling() {
        let temp = TempDir::new().unwrap();
        let root = temp.path();
        let target = root.join("prompt.yaml");
        std::fs::write(
            &target,
            "apiVersion: wyrd/v1\nkind: Prompt\nmetadata:\n  name: prompt\n  version: 1.0.0\n  space: default\nspec:\n  provider: anthropic\n  model: claude\n  messages: []\n",
        )
        .unwrap();
        let entry = root.join("agent.yaml");
        std::fs::write(
            &entry,
            "apiVersion: wyrd/v1\nkind: Agent\nmetadata:\n  name: agent\n  version: 1.0.0\n  space: default\nspec:\n  prompt: prompt.yaml\n",
        )
        .unwrap();

        let mut cards = parse_file(&entry).unwrap();
        let diagnostics = resolve_tree(
            &mut cards,
            &PathSandbox::new(root).unwrap(),
            &wyrd_config::WyrdConfig::empty(),
        );

        assert!(diagnostics.is_empty());
        assert_eq!(cards.len(), 2);
        assert!(matches!(cards[0].spec, wyrd_spec::envelope::Spec::Agent(_)));
    }

    #[test]
    fn resolve_rejects_multi_document_path_targets() {
        let temp = TempDir::new().unwrap();
        let target = temp.path().join("prompts.yaml");
        std::fs::write(
            &target,
            "apiVersion: wyrd/v1\nkind: Prompt\nmetadata:\n  name: first\n  version: 1.0.0\n  space: default\nspec:\n  provider: anthropic\n  model: claude\n  messages: []\n---\napiVersion: wyrd/v1\nkind: Prompt\nmetadata:\n  name: second\n  version: 1.0.0\n  space: default\nspec:\n  provider: anthropic\n  model: claude\n  messages: []\n",
        )
        .unwrap();
        let entry = temp.path().join("agent.yaml");
        std::fs::write(
            &entry,
            "apiVersion: wyrd/v1\nkind: Agent\nmetadata:\n  name: agent\n  version: 1.0.0\n  space: default\nspec:\n  prompt: prompts.yaml\n",
        )
        .unwrap();

        let error = crate::load(&entry).expect_err("path target is a single-card import");
        assert!(error.diagnostics.iter().any(|diagnostic| {
            diagnostic
                .message
                .contains("Referenced file contains multiple cards")
        }));
    }

    #[test]
    fn resolve_loads_transitive_path_references_in_discovery_order() {
        let temp = TempDir::new().unwrap();
        let root = temp.path();
        std::fs::write(
            root.join("prompt.yaml"),
            "apiVersion: wyrd/v1\nkind: Prompt\nmetadata:\n  name: prompt\n  version: 1.0.0\n  space: default\nspec:\n  provider: anthropic\n  model: claude\n  messages: []\n",
        )
        .unwrap();
        std::fs::write(
            root.join("agent.yaml"),
            "apiVersion: wyrd/v1\nkind: Agent\nmetadata:\n  name: agent\n  version: 1.0.0\n  space: default\nspec:\n  prompt: prompt.yaml\n",
        )
        .unwrap();
        let entry = root.join("service.yaml");
        std::fs::write(
            &entry,
            "apiVersion: wyrd/v1\nkind: Service\nmetadata:\n  name: service\n  version: 1.0.0\n  space: default\nspec:\n  service_type: api\n  components:\n    - alias: agent\n      ref: agent.yaml\n",
        )
        .unwrap();

        let mut cards = parse_file(&entry).unwrap();
        let diagnostics = resolve_tree(
            &mut cards,
            &PathSandbox::new(root).unwrap(),
            &wyrd_config::WyrdConfig::empty(),
        );

        assert!(diagnostics.is_empty());
        assert_eq!(cards.len(), 3);
        assert_eq!(cards[0].source_path, entry);
        assert!(cards[1].source_path.ends_with("agent.yaml"));
        assert!(cards[2].source_path.ends_with("prompt.yaml"));
    }

    #[test]
    fn resolve_reports_path_depth_overflow_without_loading_extra_cards() {
        let temp = TempDir::new().unwrap();
        let root = temp.path();
        for index in 0..=9 {
            let next = format!("service_{}.yaml", index + 1);
            let components = if index < 9 {
                format!("  components:\n    - alias: next\n      ref: {next}\n")
            } else {
                String::new()
            };
            std::fs::write(
                root.join(format!("service_{index}.yaml")),
                format!(
                    "apiVersion: wyrd/v1\nkind: Service\nmetadata:\n  name: service-{index}\n  version: 1.0.0\n  space: default\nspec:\n  service_type: api\n{components}"
                ),
            )
            .unwrap();
        }

        let entry = root.join("service_0.yaml");
        let mut cards = parse_file(&entry).unwrap();
        let diagnostics = resolve_tree(
            &mut cards,
            &PathSandbox::new(root).unwrap(),
            &wyrd_config::WyrdConfig::empty(),
        );

        assert_eq!(cards.len(), 9);
        assert!(diagnostics.iter().any(|diagnostic| {
            diagnostic
                .message
                .contains("Path reference depth exceeds the maximum of 8")
        }));
    }
}
