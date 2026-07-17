//! Offline spec-tree resolver for authored card YAMLs.
//!
//! The loader walks a directory of authored YAML, parses every `CardSubmission`,
//! resolves reference shapes (`inline` / `path` / `ref`), sanity-checks the
//! composite graph locally, and hands the engine one flat `Vec<CardSubmission>`
//! ready for the composite wire.
//!
//! This module is pure, client-tier, IO-scoped to the local filesystem, and
//! free of network, database, and cloud dependencies.

pub mod config;
pub mod discover_refs;
pub mod error;
pub mod order;
pub mod parse;
pub mod path;
pub mod resolve;
pub mod validate;

use std::path::Path;

pub use error::{Diagnostic, LoadError};
pub use parse::AuthoredCard;
pub use wyrd_spec::registry::CardSubmission;

/// The loaded tree ready for submission to the server.
#[derive(Debug)]
pub struct LoadedTree {
    /// All cards in topological order (dependencies before dependents).
    pub cards: Vec<LoadedCard>,
    /// All diagnostics collected during load (warnings and errors).
    pub diagnostics: Vec<Diagnostic>,
}

/// A single loaded card ready for submission.
#[derive(Debug)]
pub struct LoadedCard {
    /// The card submission matching the wire shape.
    pub submission: CardSubmission,
    /// The authored source file for local diagnostics and result mapping.
    pub source_path: std::path::PathBuf,
}

/// Load a card tree from the given path.
///
/// This is the loader's single public surface. It performs the complete pipeline:
/// parse → discover_refs → config → resolve → validate → order → diagnose.
///
/// # Errors
///
/// Returns `LoadError` when the tree cannot be loaded due to IO failure or
/// unrecoverable errors. Validation diagnostics are returned in `LoadedTree.diagnostics`.
pub fn load(path: &Path) -> Result<LoadedTree, LoadError> {
    // 1. Parse the entry file
    let mut cards = parse::parse_file(path)?;

    // 2. Discover workspace config
    let config = config::discover(path)?;

    // 3. Apply config defaults
    config::apply_defaults(&mut cards, &config);

    // 4. Resolve path references
    let sandbox = path::PathSandbox::new(path.parent().unwrap_or(path).to_path_buf());
    let mut diagnostics = resolve::resolve_tree(&mut cards, &sandbox)?;

    // 5. Validate
    let validation_diagnostics = validate::validate_tree(&cards);
    diagnostics.extend(validation_diagnostics);

    // 6. Topological sort
    let order = match order::order_cards(&cards) {
        Ok(o) => o,
        Err(cycle_diagnostics) => {
            diagnostics.extend(cycle_diagnostics);
            // Return early with cycle errors
            return Ok(LoadedTree {
                cards: Vec::new(),
                diagnostics,
            });
        }
    };

    // 7. Build loaded cards in topological order
    let mut loaded_cards = Vec::new();
    for index in order {
        let card = &cards[index];
        loaded_cards.push(LoadedCard {
            submission: order::submission_for(card).map_err(LoadError::single)?,
            source_path: card.source_path.clone(),
        });
    }

    Ok(LoadedTree {
        cards: loaded_cards,
        diagnostics,
    })
}
