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

    let mut index = 0;
    while index < cards.len() {
        let source_path = cards[index].source_path.clone();
        let mut discovered = Vec::new();

        ReferenceSlotVisitor::visit(&mut cards[index].spec, |entry| match entry.value {
            SlotValue::Durable(ref_slot) => {
                if let Ref::Path(path) = ref_slot {
                    match resolve_path_to_ref(
                        &source_path,
                        path,
                        sandbox,
                        &mut card_refs_by_path,
                        &mut path_cache,
                        &mut discovered,
                        &mut diagnostics,
                    ) {
                        Ok(card_ref) => *ref_slot = Ref::Ref(card_ref),
                        Err(diagnostic) => diagnostics.push(diagnostic),
                    }
                }
            }
            SlotValue::InlineablePrompt(ref_slot) => {
                if let InlineableRef::Path(path) = ref_slot {
                    match resolve_path_to_ref(
                        &source_path,
                        path,
                        sandbox,
                        &mut card_refs_by_path,
                        &mut path_cache,
                        &mut discovered,
                        &mut diagnostics,
                    ) {
                        Ok(card_ref) => *ref_slot = InlineableRef::Ref(card_ref),
                        Err(diagnostic) => diagnostics.push(diagnostic),
                    }
                }
            }
            SlotValue::InlineableAgent(ref_slot) => {
                if let InlineableRef::Path(path) = ref_slot {
                    match resolve_path_to_ref(
                        &source_path,
                        path,
                        sandbox,
                        &mut card_refs_by_path,
                        &mut path_cache,
                        &mut discovered,
                        &mut diagnostics,
                    ) {
                        Ok(card_ref) => *ref_slot = InlineableRef::Ref(card_ref),
                        Err(diagnostic) => diagnostics.push(diagnostic),
                    }
                }
            }
        });

        for discovered_card in discovered {
            if !cards
                .iter()
                .any(|card| card.source_path == discovered_card.source_path)
            {
                cards.push(discovered_card);
            }
        }
        index += 1;
    }

    Ok(diagnostics)
}

fn resolve_path_to_ref(
    base_file: &Path,
    ref_path: &Path,
    sandbox: &PathSandbox,
    known_cards: &mut HashMap<PathBuf, CardRef>,
    path_cache: &mut HashMap<PathBuf, CardRef>,
    discovered: &mut Vec<AuthoredCard>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<CardRef, Diagnostic> {
    let (resolved_path, advisory) = sandbox.resolve(base_file, ref_path)?;
    if let Some(advisory) = advisory {
        diagnostics.push(advisory);
    }

    if let Some(cached) = path_cache.get(&resolved_path) {
        return Ok(cached.clone());
    }
    if let Some(card_ref) = known_cards.get(&resolved_path) {
        path_cache.insert(resolved_path, card_ref.clone());
        return Ok(card_ref.clone());
    }

    let mut loaded_cards = parse_file(&resolved_path)?;
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

    let loaded = loaded_cards.remove(0);
    let card_ref = card_ref_for(&loaded)?;
    known_cards.insert(resolved_path.clone(), card_ref.clone());
    path_cache.insert(resolved_path, card_ref.clone());
    discovered.push(loaded);
    Ok(card_ref)
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
        let diagnostics = resolve_tree(&mut cards, &PathSandbox::new(root.to_path_buf())).unwrap();

        assert!(diagnostics.is_empty());
        assert_eq!(cards.len(), 2);
        assert!(matches!(cards[0].spec, wyrd_spec::envelope::Spec::Agent(_)));
    }
}
