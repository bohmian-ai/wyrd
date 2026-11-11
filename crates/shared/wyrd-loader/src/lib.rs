//! Offline card-tree loading for Wyrd clients.
//!
//! The loader owns local filesystem discovery, YAML parsing, workspace
//! defaults, reference resolution, validation, dependency ordering, and
//! diagnostics. It performs no registry, server, SQL, storage, or network
//! work.

mod config;
mod diagnose;
mod error;
mod extract;
mod manifest;
mod order;
mod parse;
mod path;
mod repository;
mod resolve;
mod upload;
mod validate;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub use diagnose::{Diagnostic, Severity, SourceSpan, emit_json};
pub use error::LoadError;
pub use extract::{
    EXTRACT_VERSION, ExtractError, ExtractLimits, Extraction, lines, locates, units, zip_guard,
};
pub use manifest::{ArtifactManifest, build_artifact_manifest};
pub use parse::AuthoredCard;
pub use upload::ArtifactUpload;
pub use wyrd_spec::registry::{CardSubmission, RelativeArtifactPath};

/// The local input passed from authoring surfaces to a registration engine.
///
/// `submissions` contains only wire-safe card data. `artifact_sources` is
/// local provenance for upload execution and never crosses the API boundary.
#[derive(Debug, Clone, PartialEq)]
pub struct DeclarationInput {
    /// Cards in dependency-first order.
    pub submissions: Vec<CardSubmission>,
    /// Local source files keyed by their validated manifest paths.
    pub artifact_sources: BTreeMap<RelativeArtifactPath, PathBuf>,
    /// Identity hint for a `wyrd.toml` Repository root, verified server-side.
    pub repository: Option<wyrd_spec::registry::RepositoryHint>,
}

impl DeclarationInput {
    /// Project the wire-safe portion into the composite registration
    /// request. Local artifact provenance remains available to the upload
    /// phase and never enters the request body.
    #[must_use]
    pub fn to_declare_card_request(&self) -> wyrd_spec::registry::DeclareCardRequest {
        wyrd_spec::registry::DeclareCardRequest {
            submissions: self.submissions.clone(),
            repository: self.repository.clone(),
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
    /// Identity hint for a `wyrd.toml` Repository root; `None` for YAML.
    pub repository: Option<wyrd_spec::registry::RepositoryHint>,
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
/// Equivalent to [`load_with_repository`] without an explicit remote.
///
/// # Errors
///
/// Returns `LoadError` containing every collected error-severity diagnostic.
/// A successful tree contains warning diagnostics only.
pub fn load(path: &Path) -> Result<LoadedTree, LoadError> {
    load_with_repository(path, None)
}

/// Load a card tree from a YAML file, a directory, or a `wyrd.toml`.
///
/// Performs the complete pipeline: parse → config → resolve → validate →
/// order → diagnose. A `wyrd.toml` entry declares the Repository root of its
/// `[repository]` table, and only the Verifiers it lists plus their
/// transitive graph; `repository` is an explicit Git remote URL that
/// replaces the checkout's origin remote as the identity hint.
///
/// # Errors
///
/// Returns `LoadError` containing every collected error-severity diagnostic,
/// including a missing Repository identity, or when `repository` is given
/// for an entry other than `wyrd.toml`.
pub fn load_with_repository(
    path: &Path,
    repository: Option<&str>,
) -> Result<LoadedTree, LoadError> {
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

    // 3. Parse the entry file or directory, or build the `wyrd.toml` root.
    let is_toml = entry_path.is_file() && entry_path.file_name() == Some("wyrd.toml".as_ref());
    let (mut cards, hint) = if is_toml {
        let table = config.repository.as_ref().ok_or_else(|| {
            LoadError::single(Diagnostic::invalid_envelope(
                entry_path.clone(),
                "wyrd.toml has no [repository] table to declare".to_owned(),
            ))
        })?;
        let (card, hint) = repository::repository_root(&entry_path, table, repository)
            .map_err(LoadError::single)?;
        (vec![card], Some(hint))
    } else if repository.is_some() {
        return Err(LoadError::single(Diagnostic::invalid_envelope(
            entry_path,
            "--repository applies only when declaring wyrd.toml".to_owned(),
        )));
    } else {
        (parse::parse_path_with_sandbox(&entry_path, &sandbox)?, None)
    };

    // 4. Apply config defaults.
    config::apply_defaults(&mut cards, &config);

    // 5. Resolve path references.
    let mut diagnostics = resolve::resolve_tree(&mut cards, &sandbox, &config);

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
        repository: hint,
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
pub fn build_declaration_input(tree: LoadedTree) -> Result<DeclarationInput, LoadError> {
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

    Ok(DeclarationInput {
        submissions,
        artifact_sources,
        repository: tree.repository,
    })
}

#[cfg(test)]
mod tests {
    use super::{build_declaration_input, load};
    use std::path::Path;
    use tempfile::TempDir;
    use wyrd_spec::envelope::Spec;
    use wyrd_spec::reference::Ref;

    /// The YAML of a minimal portable Prompt Card named `name` at `version`,
    /// written to temporary workspaces by the loader tests.
    fn prompt(name: &str, version: &str) -> String {
        format!(
            "apiVersion: wyrd/v1\nkind: Prompt\nmetadata:\n  name: {name}\n  version: \"{version}\"\nspec:\n  provider: openai\n  model: gpt-5\n  system: Be brief.\n"
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
            "apiVersion: wyrd/v1\nkind: Service\nmetadata:\n  name: root-service\n  version: \"1.0.0\"\nspec:\n  service_type: api\n  components:\n    - alias: a\n      ref: a.yaml\n    - alias: b\n      ref: b.yaml\n",
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
    fn declaration_input_maps_artifact_provenance_without_serializing_paths() {
        let temp = TempDir::new().expect("temp directory creates");
        let source = temp.path().join("model.yaml");
        std::fs::write(
            &source,
            "apiVersion: wyrd/v1\nkind: Model\nmetadata:\n  name: model\n  version: \"1.0.0\"\nartifacts:\n  - relative_path: weights.bin\n    sha256: digest\n    size_bytes: 4\n    content_type: application/octet-stream\nspec:\n  interface:\n    kind: Sklearn\n    meta:\n      framework_version: \"1.4.0\"\n      model_subtype: GradientBoostingClassifier\n  task_type: BinaryClassification\n  signature:\n    inputs: []\n    outputs: []\n",
        )
        .expect("model writes");
        std::fs::write(temp.path().join("weights.bin"), b"data").expect("artifact writes");

        let input = build_declaration_input(load(&source).expect("model loads"))
            .expect("registration input builds");
        assert_eq!(input.submissions.len(), 1);
        assert_eq!(input.artifact_sources.len(), 1);
        assert_eq!(
            input.to_declare_card_request().submissions,
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
            "apiVersion: wyrd/v1\nkind: Model\nmetadata:\n  name: model\n  version: \"1.0.0\"\nartifacts:\n  - relative_path: weights.bin\n    sha256: digest\n    size_bytes: 4\n    content_type: application/octet-stream\n  - relative_path: weights.bin\n    sha256: digest\n    size_bytes: 4\n    content_type: application/octet-stream\nspec:\n  interface:\n    kind: Sklearn\n    meta:\n      framework_version: \"1.4.0\"\n      model_subtype: GradientBoostingClassifier\n  task_type: BinaryClassification\n  signature:\n    inputs: []\n    outputs: []\n",
        )
        .expect("model writes");

        let error = load(&source).expect_err("duplicate artifact paths must fail");
        assert!(
            error
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == "WYRD_REGISTRY_400_INVALID_ARTIFACT_PATH")
        );
    }

    fn model_with_artifact(source: &std::path::Path, artifact_path: &str) {
        std::fs::write(
            source,
            format!(
                "apiVersion: wyrd/v1\nkind: Model\nmetadata:\n  name: model\n  version: \"1.0.0\"\nartifacts:\n  - relative_path: {artifact_path}\n    sha256: digest\n    size_bytes: 4\n    content_type: application/octet-stream\nspec:\n  interface:\n    kind: Sklearn\n    meta:\n      framework_version: \"1.4.0\"\n      model_subtype: GradientBoostingClassifier\n  task_type: BinaryClassification\n  signature:\n    inputs: []\n    outputs: []\n"
            ),
        )
        .expect("model writes");
    }

    #[test]
    fn declaration_input_rejects_missing_and_nonregular_artifacts() {
        let temp = TempDir::new().expect("temp directory creates");
        let source = temp.path().join("model.yaml");
        model_with_artifact(&source, "weights.bin");

        let missing = build_declaration_input(load(&source).expect("model loads"))
            .expect_err("missing artifact must fail before upload");
        assert_eq!(missing.diagnostics[0].code, "WYRD_LOADER_400_IO");

        std::fs::create_dir(temp.path().join("weights.bin")).expect("artifact directory creates");
        let nonregular = build_declaration_input(load(&source).expect("model loads"))
            .expect_err("nonregular artifact must fail before upload");
        assert_eq!(
            nonregular.diagnostics[0].code,
            "WYRD_LOADER_400_INVALID_ENVELOPE"
        );
    }

    #[cfg(unix)]
    #[test]
    fn declaration_input_rejects_artifact_symlink_outside_workspace() {
        use std::os::unix::fs::symlink;

        let temp = TempDir::new().expect("temp directory creates");
        let outside = TempDir::new().expect("outside directory creates");
        let outside_file = outside.path().join("weights.bin");
        std::fs::write(&outside_file, b"data").expect("outside artifact writes");
        let source = temp.path().join("model.yaml");
        model_with_artifact(&source, "weights.bin");
        symlink(&outside_file, temp.path().join("weights.bin")).expect("artifact symlink creates");

        let error = build_declaration_input(load(&source).expect("model loads"))
            .expect_err("artifact symlink must remain contained");
        assert_eq!(error.diagnostics[0].code, "WYRD_LOADER_400_PATH_ESCAPE");
    }

    #[test]
    fn load_rejects_authored_reference_uid() {
        let temp = TempDir::new().expect("temp directory creates");
        std::fs::write(
            temp.path().join("service.yaml"),
            "apiVersion: wyrd/v1\nkind: Service\nmetadata:\n  name: service\n  version: \"1.0.0\"\nspec:\n  service_type: api\n  components:\n    - alias: prompt\n      ref:\n        kind: Prompt\n        name: prompt\n        version: \"1.0.0\"\n        uid: 01890f28-7c4a-7cc3-98e7-4f4a3c2d1b00\n",
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
            "apiVersion: wyrd/v1\nkind: Service\nmetadata:\n  name: service\n  version: \"1.0.0\"\nspec:\n  service_type: api\n  components:\n    - alias: external\n      ref:\n        kind: Prompt\n        name: prompt\n        version: \"1.0.0\"\n    - alias: sibling\n      ref: prompt.yaml\n",
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

    /// A Verifier Card YAML named `name` with one assert requirement.
    fn verifier(name: &str) -> String {
        format!(
            "apiVersion: wyrd/v1\nkind: Verifier\nmetadata:\n  name: {name}\n  version: \"1.0.0\"\nspec:\n  inputs:\n    pr:\n      type: json\n  requirements:\n    - id: ok\n      statement: The change is fine.\n      check:\n        kind: assert\n        path: inputs.pr.ok\n        operator: in\n        expected: [true]\n"
        )
    }

    /// A checkout at `root` whose origin is `url`, with `wyrd.toml`.
    ///
    /// # Panics
    /// Panics when the `.git` directory, its config or `wyrd.toml` cannot be
    /// written.
    fn checkout(root: &Path, url: Option<&str>, toml: &str) {
        std::fs::create_dir_all(root.join(".git")).expect("git dir creates");
        let config = url.map_or_else(String::new, |url| {
            format!("[remote \"origin\"]\n\turl = {url}\n")
        });
        std::fs::write(root.join(".git/config"), config).expect("git config writes");
        std::fs::write(root.join("wyrd.toml"), toml).expect("wyrd.toml writes");
    }

    /// The Repository root the TOML entry produced.
    ///
    /// # Panics
    /// Panics when `tree` has no Repository Card or its spec does not decode
    /// as a Repository.
    fn repository_spec(tree: &super::LoadedTree) -> wyrd_spec::card::repository::RepositorySpec {
        let root = tree
            .cards
            .iter()
            .find(|card| card.submission.kind == wyrd_spec::envelope::CardKind::Repository)
            .expect("repository root loads");
        let Spec::Repository(spec) =
            Spec::from_kind_and_value(&root.submission.kind, root.submission.spec.clone())
                .expect("repository spec decodes")
        else {
            panic!("repository root keeps its spec");
        };
        spec
    }

    /// Minimal TOML declares the inferred Repository with one binding per
    /// listed Verifier, local and registered, sharing `runs_on`; only the
    /// listed local Verifier joins it, never unrelated YAML beside it.
    ///
    /// # Panics
    /// Panics when the root, bindings, hint or selected graph differ.
    #[test]
    fn toml_declares_repository_root_with_only_selected_verifiers() {
        let temp = TempDir::new().expect("temp directory creates");
        std::fs::create_dir(temp.path().join("review")).expect("review dir creates");
        std::fs::write(
            temp.path().join("review/verifier.yaml"),
            verifier("code-review"),
        )
        .expect("verifier writes");
        std::fs::write(
            temp.path().join("unrelated.yaml"),
            prompt("unrelated", "1.0.0"),
        )
        .expect("unrelated writes");
        checkout(
            temp.path(),
            Some("git@github.com:Acme/Widget.Service.git"),
            "[repository.verified_by]\nverifier = [\"./review/verifier.yaml\", { kind = \"Verifier\", name = \"shared\", version = \"2.0.0\" }]\nruns_on = { kind = \"pull_request\" }\n",
        );

        let tree = load(&temp.path().join("wyrd.toml")).expect("wyrd.toml loads");
        let names: Vec<_> = tree
            .cards
            .iter()
            .map(|card| card.submission.metadata.name.as_str())
            .collect();
        assert_eq!(names, ["code-review", "widget-service"]);
        let hint = tree.repository.clone().expect("toml carries a hint");
        assert!(hint.infer_name);
        assert_eq!(
            hint.remote.map(|remote| (remote.host, remote.full_name)),
            Some(("github.com".to_owned(), "Acme/Widget.Service".to_owned()))
        );
        let spec = repository_spec(&tree);
        assert_eq!(spec.repository_id, None);
        let [local, registered] = spec.verified_by.as_slice() else {
            panic!("two verifiers expand into two bindings");
        };
        assert!(matches!(local.verifier, Ref::Sibling { .. }));
        assert!(matches!(registered.verifier, Ref::Ref(_)));
        assert_eq!(local.runs_on, registered.runs_on);
        assert!(local.runs_on.is_some());
        let request = build_declaration_input(tree)
            .expect("input builds")
            .to_declare_card_request();
        assert!(request.repository.is_some());
    }

    /// Minimal TOML in a checkout whose origin uses Git's quoted value or an
    /// uppercase variable name infers the Repository from that origin
    /// instead of refusing it as unidentified.
    ///
    /// # Panics
    /// Panics when either legal origin form does not reach the inferred root.
    #[test]
    fn toml_infers_repository_from_quoted_or_uppercase_origin() {
        for line in [
            "url = \"https://github.com/acme/widget.git\"",
            "URL = https://github.com/acme/widget.git",
        ] {
            let temp = TempDir::new().expect("temp directory creates");
            checkout(temp.path(), None, "[repository.verified_by]\nverifier = []\n");
            std::fs::write(
                temp.path().join(".git/config"),
                format!("[remote \"origin\"]\n\t{line}\n"),
            )
            .expect("git config writes");

            let tree = load(&temp.path().join("wyrd.toml")).expect("legal origin loads");
            assert_eq!(tree.cards[0].submission.metadata.name.as_str(), "widget");
            assert_eq!(
                tree.repository
                    .and_then(|hint| hint.remote)
                    .map(|remote| remote.full_name)
                    .as_deref(),
                Some("acme/widget"),
                "{line}"
            );
        }
    }

    /// Overrides win and an explicit remote identifies a repository outside
    /// any checkout; the hint then names no inferred name.
    ///
    /// # Panics
    /// Panics when overrides or the explicit remote are not applied.
    #[test]
    fn toml_overrides_and_explicit_remote_work_outside_a_checkout() {
        let temp = TempDir::new().expect("temp directory creates");
        std::fs::write(
            temp.path().join("wyrd.toml"),
            "[repository.metadata]\nname = \"widget\"\nversion = \"1.2.0\"\n[repository.spec]\nrepository_id = \"42\"\n[repository.verified_by]\nverifier = []\n",
        )
        .expect("wyrd.toml writes");

        let tree = super::load_with_repository(
            &temp.path().join("wyrd.toml"),
            Some("https://github.com/acme/widget"),
        )
        .expect("explicit identity loads");
        let root = &tree.cards[0].submission;
        assert_eq!(root.metadata.name.as_str(), "widget");
        assert_eq!(
            root.metadata
                .version
                .as_ref()
                .map(ToString::to_string)
                .as_deref(),
            Some("1.2.0")
        );
        let hint = tree.repository.clone().expect("hint");
        assert!(!hint.infer_name);
        assert_eq!(
            hint.remote.map(|remote| remote.full_name).as_deref(),
            Some("acme/widget")
        );
        let spec = repository_spec(&tree);
        assert_eq!(
            spec.repository_id
                .as_ref()
                .map(wyrd_spec::ids::RepositoryId::as_str),
            Some("42")
        );
        assert!(spec.verified_by.is_empty());

        let tree = load(&temp.path().join("wyrd.toml")).expect("repository_id alone identifies");
        assert!(tree.repository.expect("hint").remote.is_none());
    }

    /// Without a remote, an explicit remote or a repository id, declaring
    /// the TOML is refused with the stable missing-identity code; a bad
    /// explicit remote and `--repository` on YAML are refused too.
    ///
    /// # Panics
    /// Panics when a missing or misplaced identity loads.
    #[test]
    fn toml_without_identity_is_refused() {
        let temp = TempDir::new().expect("temp directory creates");
        checkout(
            temp.path(),
            None,
            "[repository.verified_by]\nverifier = []\n",
        );
        let toml = temp.path().join("wyrd.toml");
        let error = load(&toml).expect_err("missing identity refuses");
        assert_eq!(
            error.diagnostics[0].code,
            "WYRD_REGISTRY_400_REPOSITORY_IDENTITY_MISSING"
        );
        let error = super::load_with_repository(&toml, Some("/srv/git/widget"))
            .expect_err("a remote that names no repository refuses");
        assert_eq!(
            error.diagnostics[0].code,
            "WYRD_LOADER_400_INVALID_ENVELOPE"
        );
        std::fs::write(temp.path().join("prompt.yaml"), prompt("prompt", "1.0.0"))
            .expect("prompt writes");
        let error = super::load_with_repository(
            &temp.path().join("prompt.yaml"),
            Some("https://github.com/acme/widget"),
        )
        .expect_err("a remote without a wyrd.toml repository refuses");
        assert_eq!(
            error.diagnostics[0].code,
            "WYRD_LOADER_400_INVALID_ENVELOPE"
        );
    }

    /// An invalid listed Verifier fails local validation before anything
    /// is submitted.
    ///
    /// # Panics
    /// Panics when the invalid Verifier loads.
    #[test]
    fn toml_validates_listed_verifiers_locally() {
        let temp = TempDir::new().expect("temp directory creates");
        std::fs::write(
            temp.path().join("verifier.yaml"),
            "apiVersion: wyrd/v1\nkind: Verifier\nmetadata:\n  name: broken\nspec:\n  requirements: []\n  unknown: true\n",
        )
        .expect("verifier writes");
        checkout(
            temp.path(),
            Some("https://github.com/acme/widget.git"),
            "[repository.verified_by]\nverifier = [\"./verifier.yaml\"]\n",
        );
        let error = load(&temp.path().join("wyrd.toml")).expect_err("invalid Verifier refuses");
        assert_eq!(
            error.diagnostics[0].code,
            "WYRD_LOADER_400_INVALID_ENVELOPE"
        );
    }
}
