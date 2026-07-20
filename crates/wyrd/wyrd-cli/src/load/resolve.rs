//! Reference resolution - collapse Path variants to Ref variants.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use wyrd_spec::reference::{CardRef, InlineableRef, Ref};
use wyrd_spec::refs::{ReferenceSlotVisitor, SlotValue};

use super::error::{Diagnostic, LoadError};
use super::parse::{AuthoredCard, card_ref_for, parse_file};
use super::path::PathSandbox;

/// Resolve all path references in the tree, rewriting them to `Ref` variants.
///
/// Every `Ref::Path` becomes `Ref::Ref(CardRef)` bound to a sibling
/// submission. Path-loaded cards are appended to `cards` so transitive paths
/// participate in validation and topological ordering.
pub fn resolve_tree(
    cards: &mut Vec<AuthoredCard>,
    sandbox: &PathSandbox,
    config: &wyrd_config::WyrdConfig,
) -> Result<Vec<Diagnostic>, LoadError> {
    let mut diagnostics = Vec::new();
    let mut path_cache: HashMap<PathBuf, CardRef> = HashMap::new();
    let mut card_refs_by_path: HashMap<PathBuf, CardRef> = cards
        .iter()
        .filter_map(|card| {
            card_ref_for(card)
                .ok()
                .map(|card_ref| (card.source_path.clone(), card_ref))
        })
        .collect();
    let mut depth_by_path = cards
        .iter()
        .map(|card| (card.source_path.clone(), 0usize))
        .collect::<HashMap<_, _>>();

    let mut index = 0;
    while index < cards.len() {
        let source_path = cards[index].source_path.clone();
        let source_depth = depth_by_path.get(&source_path).copied().unwrap_or(0);
        let parent_space = cards[index].metadata.space.clone();
        let mut discovered = Vec::new();
        let mut resolver = PathResolver {
            sandbox,
            known_cards: &mut card_refs_by_path,
            path_cache: &mut path_cache,
            diagnostics: &mut diagnostics,
            config,
        };

        ReferenceSlotVisitor::visit(&mut cards[index].spec, |entry| match entry.value {
            SlotValue::Durable(ref_slot) => {
                inherit_space(ref_slot.as_card_ref_mut(), parent_space.as_ref());
                if let Ref::Path(path) = ref_slot {
                    match resolver.resolve(&source_path, path, &mut discovered, source_depth) {
                        Ok(card_ref) => *ref_slot = Ref::Ref(card_ref),
                        Err(diagnostic) => resolver.diagnostics.push(diagnostic),
                    }
                }
            }
            SlotValue::InlineablePrompt(ref_slot) => {
                inherit_space(ref_slot.as_card_ref_mut(), parent_space.as_ref());
                if let InlineableRef::Path(path) = ref_slot {
                    match resolver.resolve(&source_path, path, &mut discovered, source_depth) {
                        Ok(card_ref) => *ref_slot = InlineableRef::Ref(card_ref),
                        Err(diagnostic) => resolver.diagnostics.push(diagnostic),
                    }
                }
            }
            SlotValue::InlineableAgent(ref_slot) => {
                inherit_space(ref_slot.as_card_ref_mut(), parent_space.as_ref());
                if let InlineableRef::Path(path) = ref_slot {
                    match resolver.resolve(&source_path, path, &mut discovered, source_depth) {
                        Ok(card_ref) => *ref_slot = InlineableRef::Ref(card_ref),
                        Err(diagnostic) => resolver.diagnostics.push(diagnostic),
                    }
                }
            }
        });

        for discovered_card in discovered {
            if !cards
                .iter()
                .any(|card| card.source_path == discovered_card.source_path)
            {
                depth_by_path.insert(discovered_card.source_path.clone(), source_depth + 1);
                cards.push(discovered_card);
            }
        }
        index += 1;
    }

    Ok(diagnostics)
}

struct PathResolver<'a> {
    sandbox: &'a PathSandbox,
    known_cards: &'a mut HashMap<PathBuf, CardRef>,
    path_cache: &'a mut HashMap<PathBuf, CardRef>,
    diagnostics: &'a mut Vec<Diagnostic>,
    config: &'a wyrd_config::WyrdConfig,
}

impl PathResolver<'_> {
    fn resolve(
        &mut self,
        base_file: &Path,
        ref_path: &Path,
        discovered: &mut Vec<AuthoredCard>,
        source_depth: usize,
    ) -> Result<CardRef, Diagnostic> {
        if source_depth >= 8 {
            return Err(Diagnostic::invalid_envelope(
                base_file.to_path_buf(),
                "Path reference depth exceeds the maximum of 8".to_owned(),
            ));
        }
        let (resolved_path, advisory) = self.sandbox.resolve(base_file, ref_path)?;
        if let Some(advisory) = advisory {
            self.diagnostics.push(advisory);
        }

        if let Some(cached) = self.path_cache.get(&resolved_path) {
            return Ok(cached.clone());
        }
        if let Some(card_ref) = self.known_cards.get(&resolved_path) {
            self.path_cache.insert(resolved_path, card_ref.clone());
            return Ok(card_ref.clone());
        }

        let mut loaded_cards = parse_file(&resolved_path).map_err(|error| {
            self.diagnostics.extend(error.diagnostics);
            Diagnostic::invalid_envelope(
                base_file.to_path_buf(),
                format!(
                    "referenced card failed to parse: {}",
                    resolved_path.display()
                ),
            )
        })?;
        if loaded_cards.is_empty() {
            return Err(Diagnostic::invalid_envelope(
                base_file.to_path_buf(),
                format!("Referenced file is empty: {}", ref_path.display()),
            ));
        }
        if loaded_cards.len() > 1 {
            return Err(Diagnostic::invalid_envelope(
                base_file.to_path_buf(),
                format!(
                    "Referenced file contains multiple cards: {}",
                    ref_path.display()
                ),
            ));
        }

        let mut loaded = loaded_cards.remove(0);
        super::config::apply_defaults(std::slice::from_mut(&mut loaded), self.config);
        let card_ref = card_ref_for(&loaded)?;
        self.known_cards
            .insert(resolved_path.clone(), card_ref.clone());
        self.path_cache.insert(resolved_path, card_ref.clone());
        discovered.push(loaded);
        Ok(card_ref)
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
            &PathSandbox::new(root.to_path_buf()).unwrap(),
            &wyrd_config::WyrdConfig::empty(),
        )
        .unwrap();

        assert!(diagnostics.is_empty());
        assert_eq!(cards.len(), 2);
        assert!(matches!(cards[0].spec, wyrd_spec::envelope::Spec::Agent(_)));
    }
}
