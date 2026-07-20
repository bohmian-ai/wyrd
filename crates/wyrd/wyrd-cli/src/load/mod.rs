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
pub mod diagnose;
pub mod error;
pub mod order;
pub mod parse;
pub mod path;
pub mod resolve;
pub mod validate;

use std::path::Path;

pub use diagnose::{Diagnostic, Severity, SourceSpan, emit_json};
pub use error::LoadError;
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
/// parse → config → resolve → validate → order → diagnose.
///
/// # Errors
///
/// Returns `LoadError` containing every collected error-severity diagnostic.
/// A successful tree contains warning diagnostics only.
pub fn load(path: &Path) -> Result<LoadedTree, LoadError> {
    let entry_path = path
        .canonicalize()
        .map_err(|error| LoadError::single(Diagnostic::io(path.to_path_buf(), error)))?;

    // 1. Parse the entry file or directory.
    let mut cards = parse::parse_path(&entry_path)?;

    // 2. Discover workspace config
    let config = config::discover(&entry_path)?;

    // 3. Apply config defaults
    config::apply_defaults(&mut cards, &config);

    // 4. Resolve path references
    let sandbox_root = config
        .root_path
        .as_deref()
        .and_then(Path::parent)
        .unwrap_or_else(|| {
            if entry_path.is_dir() {
                entry_path.as_path()
            } else {
                entry_path.parent().unwrap_or(entry_path.as_path())
            }
        });
    let sandbox = path::PathSandbox::new(sandbox_root.to_path_buf()).map_err(LoadError::single)?;
    let mut diagnostics = resolve::resolve_tree(&mut cards, &sandbox, &config)?;

    // 5. Validate
    let validation_diagnostics = validate::validate_tree(&cards);
    diagnostics.extend(validation_diagnostics);

    // 6. Topological sort
    let order = match order::order_cards(&cards) {
        Ok(o) => o,
        Err(cycle_diagnostics) => {
            diagnostics.extend(cycle_diagnostics);
            Vec::new()
        }
    };

    if diagnostics
        .iter()
        .any(|diagnostic| diagnostic.severity == Severity::Error)
    {
        return Err(LoadError::multiple(diagnostics));
    }

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

/// Consume a loaded tree and return its wire-ready submissions.
///
/// # Errors
/// Returns an error if an invalid caller-constructed tree contains an
/// error-severity diagnostic.
pub fn build_submissions(tree: LoadedTree) -> Result<Vec<CardSubmission>, LoadError> {
    if tree
        .diagnostics
        .iter()
        .any(|diagnostic| diagnostic.severity == Severity::Error)
    {
        return Err(LoadError::multiple(tree.diagnostics));
    }
    Ok(tree.cards.into_iter().map(|card| card.submission).collect())
}
