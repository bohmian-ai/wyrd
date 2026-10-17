//! Staged-contract gate for the committed fixture Verifier Cards.
//!
//! The shared Card loader still decodes Verifiers through the pre-blocks
//! contract until its cutover, so `wyrd plan` cannot yet prove the journey
//! fixtures. This test decodes every fixture Verifier into
//! [`blocks::VerifierSpec`], validates it with every path-referenced Agent and
//! Prompt resolved from disk.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::de::DeserializeOwned;
use serde_yaml::Value;
use wyrd_spec::Prompt;
use wyrd_spec::card::agent::AgentSpec;
use wyrd_spec::card::verifier::{ResolvedAgent, VerifierSpec};
use wyrd_spec::reference::InlineableRef;

/// Every fixture Card decoded by kind and keyed by its canonical path.
#[derive(Default)]
struct Fixtures {
    /// Agent Card specs a Verifier step or check may reference by path.
    agents: HashMap<PathBuf, AgentSpec>,
    /// Prompt Card specs an Agent may reference by path.
    prompts: HashMap<PathBuf, Prompt>,
    /// Verifier Card specs decoded into the staged blocks contract.
    verifiers: HashMap<PathBuf, VerifierSpec>,
}

impl Fixtures {
    /// Loads every `fixtures/cards/*/*.yaml` Card.
    ///
    /// # Panics
    /// Panics when a fixture cannot be read or parsed, or when an Agent,
    /// Prompt, or Verifier fails to decode.
    fn load() -> Self {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/cards");
        let mut fixtures = Self::default();
        for dir in fs::read_dir(&root).expect("fixtures/cards is readable") {
            let dir = dir.expect("fixture directory entry").path();
            if !dir.is_dir() {
                continue;
            }
            for file in fs::read_dir(&dir).expect("fixture directory is readable") {
                let path = file.expect("fixture file entry").path();
                if path.extension().is_some_and(|ext| ext == "yaml") {
                    fixtures.add(&canonical(&path));
                }
            }
        }
        fixtures
    }

    /// Decodes one Card by kind.
    ///
    /// # Panics
    /// Panics when the Card cannot be read, parsed, or decoded.
    fn add(&mut self, path: &Path) {
        let text = fs::read_to_string(path).expect("fixture is readable");
        let card: Value = serde_yaml::from_str(&text).expect("fixture is YAML");
        let spec = &card["spec"];
        match card["kind"].as_str() {
            Some("Agent") => {
                self.agents.insert(path.to_path_buf(), decode(path, spec));
            }
            Some("Prompt") => {
                self.prompts.insert(path.to_path_buf(), decode(path, spec));
            }
            Some("Verifier") => {
                self.verifiers
                    .insert(path.to_path_buf(), decode(path, spec));
            }
            _ => {}
        }
    }

    /// Resolves a Verifier's Agent reference and its Prompt from disk.
    ///
    /// # Panics
    /// Panics when a referenced Agent or Prompt is not a loaded fixture, or
    /// the reference is a registry reference no fixture can satisfy.
    fn resolve<'a>(
        &'a self,
        verifier: &Path,
        agent: &'a InlineableRef<AgentSpec>,
    ) -> ResolvedAgent<'a> {
        let (agent, agent_path) = match agent {
            InlineableRef::Inline(spec) => (spec.as_ref(), verifier.to_path_buf()),
            InlineableRef::Path(rel) => {
                let path = canonical(&sibling(verifier, &rel.to_string_lossy()));
                let spec = self.agents.get(&path).unwrap_or_else(|| {
                    panic!(
                        "{} references unknown Agent {}",
                        verifier.display(),
                        rel.display()
                    )
                });
                (spec, path)
            }
            other => panic!(
                "{} references unresolvable Agent {other:?}",
                verifier.display()
            ),
        };
        let prompt = match &agent.prompt {
            InlineableRef::Inline(prompt) => prompt.as_ref(),
            InlineableRef::Path(rel) => self
                .prompts
                .get(&canonical(&sibling(&agent_path, &rel.to_string_lossy())))
                .unwrap_or_else(|| {
                    panic!(
                        "{} references unknown Prompt {}",
                        agent_path.display(),
                        rel.display()
                    )
                }),
            other => panic!(
                "{} references unresolvable Prompt {other:?}",
                agent_path.display()
            ),
        };
        ResolvedAgent { agent, prompt }
    }
}

/// Decodes a fixture YAML value into `T` through JSON, the Card wire format,
/// so externally tagged enums read as plain maps, naming the fixture on failure.
///
/// # Panics
/// Panics when the value does not decode into `T`.
fn decode<T: DeserializeOwned>(path: &Path, value: &Value) -> T {
    serde_json::to_value(value)
        .and_then(serde_json::from_value)
        .unwrap_or_else(|error| panic!("{} does not decode: {error}", path.display()))
}

/// Resolves a Card-relative reference against the referencing Card's directory.
///
/// # Panics
/// Panics when `card` has no parent directory; every fixture path is a file
/// inside `fixtures/cards/<journey>/`, so this is an invariant.
fn sibling(card: &Path, reference: &str) -> PathBuf {
    card.parent()
        .expect("a fixture has a parent directory")
        .join(reference)
}

/// Canonicalizes a fixture path so references and directory entries compare.
///
/// # Panics
/// Panics when the path does not exist.
fn canonical(path: &Path) -> PathBuf {
    path.canonicalize()
        .unwrap_or_else(|error| panic!("{} does not resolve: {error}", path.display()))
}

/// Every fixture Verifier satisfies the staged blocks contract.
///
/// # Panics
/// Panics when a fixture cannot be loaded or decoded, a referenced Agent or
/// Prompt cannot be resolved, a Verifier fails `validate_with`, or no
/// Verifiers are found.
#[test]
fn fixture_verifiers_satisfy_the_staged_contract() {
    let fixtures = Fixtures::load();
    assert!(!fixtures.verifiers.is_empty(), "no fixture Verifiers found");
    for (path, verifier) in &fixtures.verifiers {
        verifier
            .validate_with(|agent| Some(fixtures.resolve(path, agent)))
            .unwrap_or_else(|error| panic!("{} is invalid: {error}", path.display()));
    }
}
