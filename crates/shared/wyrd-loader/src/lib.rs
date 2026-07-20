//! Offline card-tree loading for Wyrd clients.
//!
//! The loader owns local filesystem discovery, YAML parsing, workspace
//! defaults, reference resolution, validation, dependency ordering, and
//! diagnostics. It performs no registry, server, SQL, storage, or network
//! work.

pub mod config;
pub mod diagnose;
pub mod error;
pub mod order;
pub mod parse;
pub mod path;
pub mod resolve;
pub mod validate;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub use diagnose::{Diagnostic, Severity, SourceSpan, emit_json};
pub use error::LoadError;
pub use parse::AuthoredCard;
pub use wyrd_spec::registry::{CardSubmission, RelativeArtifactPath};

/// The local input passed from authoring surfaces to a registration engine.
///
/// `submissions` contains only wire-safe card data. `artifact_sources` is
/// local provenance for upload execution and never crosses the API boundary.
#[derive(Debug, Clone, PartialEq)]
pub struct RegistrationInput {
    /// Cards in dependency-first order.
    pub submissions: Vec<CardSubmission>,
    /// Local source files keyed by their validated manifest paths.
    pub artifact_sources: BTreeMap<RelativeArtifactPath, PathBuf>,
}

impl RegistrationInput {
    /// Project the wire-safe portion into the existing 02a registration
    /// request. Local artifact provenance remains available to the upload
    /// phase and never enters the request body.
    #[must_use]
    pub fn to_create_card_request(&self) -> wyrd_spec::registry::CreateCardRequest {
        wyrd_spec::registry::CreateCardRequest {
            submissions: self.submissions.clone(),
        }
    }
}

/// The loaded tree with source provenance retained for diagnostics and local
/// artifact upload mapping.
#[derive(Debug, Clone)]
pub struct LoadedTree {
    /// All cards in topological order (dependencies before dependents).
    pub cards: Vec<LoadedCard>,
    /// All diagnostics collected during load (warnings and errors).
    pub diagnostics: Vec<Diagnostic>,
    pub(crate) sandbox_root: PathBuf,
}

/// A single loaded card ready for submission.
#[derive(Debug, Clone)]
pub struct LoadedCard {
    /// The card submission matching the wire shape.
    pub submission: CardSubmission,
    /// The authored source file for local diagnostics and result mapping.
    pub source_path: PathBuf,
}

/// Load a card tree from the given path.
///
/// This is the loader's single public surface. It performs the complete
/// pipeline: parse → config → resolve → validate → order → diagnose.
///
/// # Errors
///
/// Returns `LoadError` containing every collected error-severity diagnostic.
/// A successful tree contains warning diagnostics only.
pub fn load(path: &Path) -> Result<LoadedTree, LoadError> {
    let entry_path = path
        .canonicalize()
        .map_err(|error| LoadError::single(Diagnostic::io(path.to_path_buf(), &error)))?;

    // 1. Discover workspace config.
    let config = config::discover(&entry_path)?;

    // 2. Establish the single filesystem boundary before parsing.
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
    let sandbox = path::PathSandbox::new(sandbox_root).map_err(LoadError::single)?;

    // 3. Parse the entry file or directory.
    let mut cards = parse::parse_path_with_sandbox(&entry_path, &sandbox)?;

    // 4. Apply config defaults.
    config::apply_defaults(&mut cards, &config);

    // 5. Resolve path references.
    let mut diagnostics = resolve::resolve_tree(&mut cards, &sandbox, &config)?;

    // 6. Validate.
    diagnostics.extend(validate::validate_tree(&cards));

    // 7. Topological sort.
    let order = match order::order_cards(&cards) {
        Ok(order) => order,
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

    // 8. Build loaded cards in topological order.
    let mut loaded_cards = Vec::with_capacity(order.len());
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
        sandbox_root: sandbox.root().to_path_buf(),
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

/// Consume a loaded tree and retain local artifact provenance for the engine.
///
/// Artifact source files are resolved relative to the authored card envelope.
/// The returned map contains no filesystem path inside any submission.
///
/// # Errors
/// Returns the collected loader diagnostics when the tree is invalid.
pub fn build_registration_input(tree: LoadedTree) -> Result<RegistrationInput, LoadError> {
    if tree
        .diagnostics
        .iter()
        .any(|diagnostic| diagnostic.severity == Severity::Error)
    {
        return Err(LoadError::multiple(tree.diagnostics));
    }

    let sandbox = path::PathSandbox::new(&tree.sandbox_root).map_err(LoadError::single)?;
    let mut artifact_sources = BTreeMap::new();
    let mut submissions = Vec::with_capacity(tree.cards.len());
    for card in tree.cards {
        for artifact in &card.submission.artifacts {
            let source = sandbox
                .resolve_regular_file(
                    &card.source_path,
                    Path::new(artifact.relative_path.as_str()),
                )
                .map_err(LoadError::single)?;
            if artifact_sources
                .insert(artifact.relative_path.clone(), source)
                .is_some()
            {
                return Err(LoadError::single(Diagnostic::invalid_envelope(
                    card.source_path,
                    format!("duplicate artifact path: {}", artifact.relative_path),
                )));
            }
        }
        submissions.push(card.submission);
    }

    Ok(RegistrationInput {
        submissions,
        artifact_sources,
    })
}

#[cfg(test)]
mod tests {
    use super::{build_registration_input, load};
    use std::path::Path;
    use tempfile::TempDir;
    use wyrd_spec::envelope::Spec;
    use wyrd_spec::reference::Ref;

    fn prompt(name: &str, version: &str) -> String {
        format!(
            "apiVersion: wyrd/v1\nkind: Prompt\nmetadata:\n  name: {name}\n  space: default\n  version: \"{version}\"\nspec:\n  provider: openai\n  model: gpt-5\n  messages: []\n"
        )
    }

    #[test]
    fn load_reads_multiple_files_in_deterministic_order() {
        let temp = TempDir::new().expect("temp directory creates");
        std::fs::write(temp.path().join("b.yaml"), prompt("card-b", "1.0.0"))
            .expect("second card writes");
        std::fs::write(temp.path().join("a.yaml"), prompt("card-a", "1.0.0"))
            .expect("first card writes");
        std::fs::write(
            temp.path().join("root.yaml"),
            "apiVersion: wyrd/v1\nkind: Service\nmetadata:\n  name: root-service\n  space: default\n  version: \"1.0.0\"\nspec:\n  service_type: api\n  components:\n    - alias: a\n      ref: a.yaml\n    - alias: b\n      ref: b.yaml\n",
        )
        .expect("root card writes");

        let tree = load(temp.path().join("root.yaml").as_path()).expect("root loads");
        let names = tree
            .cards
            .iter()
            .map(|card| card.submission.metadata.name.as_str())
            .collect::<Vec<_>>();
        assert_eq!(names, vec!["card-a", "card-b", "root-service"]);
    }

    #[test]
    fn registration_input_maps_artifact_provenance_without_serializing_paths() {
        let temp = TempDir::new().expect("temp directory creates");
        let source = temp.path().join("model.yaml");
        std::fs::write(
            &source,
            "apiVersion: wyrd/v1\nkind: Model\nmetadata:\n  name: model\n  space: default\n  version: \"1.0.0\"\nartifacts:\n  - relative_path: weights.bin\n    sha256: digest\n    size_bytes: 4\n    content_type: application/octet-stream\nspec:\n  interface:\n    kind: Sklearn\n    meta:\n      framework_version: \"1.4.0\"\n      model_subtype: GradientBoostingClassifier\n  task_type: BinaryClassification\n  signature:\n    inputs: []\n    outputs: []\n",
        )
        .expect("model writes");
        std::fs::write(temp.path().join("weights.bin"), b"data").expect("artifact writes");

        let input = build_registration_input(load(&source).expect("model loads"))
            .expect("registration input builds");
        assert_eq!(input.submissions.len(), 1);
        assert_eq!(input.artifact_sources.len(), 1);
        assert_eq!(
            input.to_create_card_request().submissions,
            input.submissions
        );
        let path = input
            .artifact_sources
            .values()
            .next()
            .expect("artifact source exists");
        assert_eq!(
            path,
            &temp
                .path()
                .canonicalize()
                .expect("temp path canonicalizes")
                .join("weights.bin")
        );
        assert!(
            !serde_json::to_string(&input.submissions[0])
                .expect("submission serializes")
                .contains(temp.path().to_string_lossy().as_ref())
        );
        assert!(Path::new(path).is_absolute());
    }

    #[test]
    fn load_rejects_duplicate_artifact_paths() {
        let temp = TempDir::new().expect("temp directory creates");
        let source = temp.path().join("model.yaml");
        std::fs::write(
            &source,
            "apiVersion: wyrd/v1\nkind: Model\nmetadata:\n  name: model\n  space: default\n  version: \"1.0.0\"\nartifacts:\n  - relative_path: weights.bin\n    sha256: digest\n    size_bytes: 4\n    content_type: application/octet-stream\n  - relative_path: weights.bin\n    sha256: digest\n    size_bytes: 4\n    content_type: application/octet-stream\nspec:\n  interface:\n    kind: Sklearn\n    meta:\n      framework_version: \"1.4.0\"\n      model_subtype: GradientBoostingClassifier\n  task_type: BinaryClassification\n  signature:\n    inputs: []\n    outputs: []\n",
        )
        .expect("model writes");

        let error = load(&source).expect_err("duplicate artifact paths must fail");
        assert!(
            error
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.message.contains("duplicate artifact path"))
        );
    }

    fn model_with_artifact(source: &std::path::Path, artifact_path: &str) {
        std::fs::write(
            source,
            format!(
                "apiVersion: wyrd/v1\nkind: Model\nmetadata:\n  name: model\n  space: default\n  version: \"1.0.0\"\nartifacts:\n  - relative_path: {artifact_path}\n    sha256: digest\n    size_bytes: 4\n    content_type: application/octet-stream\nspec:\n  interface:\n    kind: Sklearn\n    meta:\n      framework_version: \"1.4.0\"\n      model_subtype: GradientBoostingClassifier\n  task_type: BinaryClassification\n  signature:\n    inputs: []\n    outputs: []\n"
            ),
        )
        .expect("model writes");
    }

    #[test]
    fn registration_input_rejects_missing_and_nonregular_artifacts() {
        let temp = TempDir::new().expect("temp directory creates");
        let source = temp.path().join("model.yaml");
        model_with_artifact(&source, "weights.bin");

        let missing = build_registration_input(load(&source).expect("model loads"))
            .expect_err("missing artifact must fail before upload");
        assert_eq!(missing.diagnostics[0].code, "WYRD_LOADER_400_IO");

        std::fs::create_dir(temp.path().join("weights.bin")).expect("artifact directory creates");
        let nonregular = build_registration_input(load(&source).expect("model loads"))
            .expect_err("nonregular artifact must fail before upload");
        assert_eq!(
            nonregular.diagnostics[0].code,
            "WYRD_LOADER_400_INVALID_ENVELOPE"
        );
    }

    #[cfg(unix)]
    #[test]
    fn registration_input_rejects_artifact_symlink_outside_workspace() {
        use std::os::unix::fs::symlink;

        let temp = TempDir::new().expect("temp directory creates");
        let outside = TempDir::new().expect("outside directory creates");
        let outside_file = outside.path().join("weights.bin");
        std::fs::write(&outside_file, b"data").expect("outside artifact writes");
        let source = temp.path().join("model.yaml");
        model_with_artifact(&source, "weights.bin");
        symlink(&outside_file, temp.path().join("weights.bin")).expect("artifact symlink creates");

        let error = build_registration_input(load(&source).expect("model loads"))
            .expect_err("artifact symlink must remain contained");
        assert_eq!(error.diagnostics[0].code, "WYRD_LOADER_400_PATH_ESCAPE");
    }

    #[test]
    fn load_rejects_authored_reference_uid() {
        let temp = TempDir::new().expect("temp directory creates");
        std::fs::write(
            temp.path().join("service.yaml"),
            "apiVersion: wyrd/v1\nkind: Service\nmetadata:\n  name: service\n  space: default\n  version: \"1.0.0\"\nspec:\n  service_type: api\n  components:\n    - alias: prompt\n      ref:\n        kind: Prompt\n        name: prompt\n        space: default\n        version: \"1.0.0\"\n        uid: 01890f28-7c4a-7cc3-98e7-4f4a3c2d1b00\n",
        )
        .expect("service writes");

        let error = load(&temp.path().join("service.yaml"))
            .expect_err("server-managed reference uid must not be authored");
        assert!(
            error
                .diagnostics
                .iter()
                .any(|diagnostic| { diagnostic.message.contains("must omit server-managed uid") })
        );
    }

    #[test]
    fn path_and_external_refs_keep_distinct_wire_projections() {
        let temp = TempDir::new().expect("temp directory creates");
        std::fs::write(temp.path().join("prompt.yaml"), prompt("prompt", "1.0.0"))
            .expect("prompt writes");
        std::fs::write(
            temp.path().join("service.yaml"),
            "apiVersion: wyrd/v1\nkind: Service\nmetadata:\n  name: service\n  space: default\n  version: \"1.0.0\"\nspec:\n  service_type: api\n  components:\n    - alias: external\n      ref:\n        kind: Prompt\n        name: prompt\n        space: default\n        version: \"1.0.0\"\n    - alias: sibling\n      ref: prompt.yaml\n",
        )
        .expect("service writes");

        let tree = load(&temp.path().join("service.yaml")).expect("service loads");
        let service = tree
            .cards
            .iter()
            .find(|card| card.submission.metadata.name.as_str() == "service")
            .expect("service is loaded");
        let Spec::Service(service_spec) =
            Spec::from_kind_and_value(&service.submission.kind, service.submission.spec.clone())
                .expect("service spec decodes")
        else {
            panic!("service submission retains service spec");
        };

        let [external, sibling] = service_spec.components.as_slice() else {
            panic!("service has two components");
        };
        let Ref::Ref(external_ref) = &external.card_ref else {
            panic!("authored ref remains external");
        };
        let Ref::Sibling {
            sibling: sibling_ref,
        } = &sibling.card_ref
        else {
            panic!("path ref becomes a sibling projection");
        };
        assert!(external_ref.same_identity(sibling_ref));
    }
}
