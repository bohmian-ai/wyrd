//! Path reference resolution and sandbox enforcement.

use std::path::{Path, PathBuf};

use wyrd_spec::reference::{CardRef, InlineableRef, Ref};
use wyrd_spec::refs::{ReferenceSlotVisitor, SlotValue};

use super::error::{Diagnostic, LoadError};
use super::parse::{AuthoredCard, card_ref_for, parse_file};

/// Path sandbox enforcing workspace root boundaries.
pub struct PathSandbox {
    /// The workspace root — no path may escape this.
    root: PathBuf,
}

impl PathSandbox {
    /// Create a new sandbox rooted at the given path.
    pub fn new(root: PathBuf) -> Self {
        Self {
            root: root.canonicalize().unwrap_or(root),
        }
    }

    /// Resolve a path reference relative to the given base file.
    ///
    /// Returns the canonicalized path, or a diagnostic if the path escapes
    /// the sandbox or contains invalid traversal.
    pub fn resolve(
        &self,
        base_file: &Path,
        ref_path: &Path,
    ) -> Result<(PathBuf, Option<Diagnostic>), Diagnostic> {
        // Absolute paths are allowed with an advisory
        if ref_path.is_absolute() {
            let canonical = ref_path
                .canonicalize()
                .map_err(|e| Diagnostic::io(base_file.into(), e))?;

            let advisory = Diagnostic::path_absolute_advisory(base_file.into(), canonical.clone());
            return Ok((canonical, Some(advisory)));
        }

        // Relative paths resolve from the base file's directory
        let base_dir = base_file.parent().ok_or_else(|| {
            Diagnostic::invalid_envelope(
                base_file.into(),
                "Cannot resolve parent directory".to_string(),
            )
        })?;

        let base_dir = base_dir
            .canonicalize()
            .map_err(|e| Diagnostic::io(base_file.into(), e))?;
        let resolved = normalize_relative_path(&base_dir, ref_path);

        if !resolved.starts_with(&self.root) {
            return Err(Diagnostic::path_escape(base_file.into(), resolved));
        }

        let canonical = resolved
            .canonicalize()
            .map_err(|e| Diagnostic::io(base_file.into(), e))?;

        // Check that the canonical path is within the root
        if !canonical.starts_with(&self.root) {
            return Err(Diagnostic::path_escape(base_file.into(), canonical));
        }

        Ok((canonical, None))
    }
}

fn normalize_relative_path(base: &Path, relative: &Path) -> PathBuf {
    let mut normalized = base.to_path_buf();
    for component in relative.components() {
        match component {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                normalized.pop();
            }
            std::path::Component::Normal(part) => normalized.push(part),
            std::path::Component::RootDir | std::path::Component::Prefix(_) => {}
        }
    }
    normalized
}

/// Resolve all `path:` references in a card's spec.
///
/// When a visitor yield is `Path(PathBuf)` at a `Durable` slot, load the
/// referenced file and rewrite the parent's slot to `Ref(CardRef)` carrying
/// the sibling's authored identity. When a visitor
/// yield is `Path(PathBuf)` at an `Inlineable` slot, load the referenced file
/// the same way (still becomes a sibling submission; the loader does not
/// materialize a `path:` into `Inline(...)`).
///
/// Returns a vec of loaded sibling cards and a vec of diagnostics (warnings).
pub fn resolve_path_ref(
    sandbox: &PathSandbox,
    card: &mut AuthoredCard,
) -> Result<(Vec<AuthoredCard>, Vec<Diagnostic>), LoadError> {
    let mut siblings = Vec::new();
    let mut diagnostics = Vec::new();

    ReferenceSlotVisitor::visit(&mut card.spec, |mut entry| match &mut entry.value {
        SlotValue::Durable(ref_slot) => {
            if let Ref::Path(path) = ref_slot {
                match load_and_rewrite_ref(
                    sandbox,
                    &card.source_path,
                    path,
                    &mut siblings,
                    &mut diagnostics,
                ) {
                    Ok(card_ref) => {
                        **ref_slot = Ref::Ref(card_ref);
                    }
                    Err(diag) => {
                        diagnostics.push(diag);
                    }
                }
            }
        }
        SlotValue::InlineablePrompt(ref_slot) => {
            if let InlineableRef::Path(path) = ref_slot {
                match load_and_rewrite_ref(
                    sandbox,
                    &card.source_path,
                    path,
                    &mut siblings,
                    &mut diagnostics,
                ) {
                    Ok(card_ref) => **ref_slot = InlineableRef::Ref(card_ref),
                    Err(diag) => diagnostics.push(diag),
                }
            }
        }
        SlotValue::InlineableAgent(ref_slot) => {
            if let InlineableRef::Path(path) = ref_slot {
                match load_and_rewrite_ref(
                    sandbox,
                    &card.source_path,
                    path,
                    &mut siblings,
                    &mut diagnostics,
                ) {
                    Ok(card_ref) => **ref_slot = InlineableRef::Ref(card_ref),
                    Err(diag) => diagnostics.push(diag),
                }
            }
        }
    });

    Ok((siblings, diagnostics))
}

fn load_and_rewrite_ref(
    sandbox: &PathSandbox,
    base_file: &Path,
    ref_path: &Path,
    siblings: &mut Vec<AuthoredCard>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<CardRef, Diagnostic> {
    let (resolved_path, advisory) = sandbox.resolve(base_file, ref_path)?;

    if let Some(adv) = advisory {
        diagnostics.push(adv);
    }

    // Load the referenced file
    let mut loaded_cards = parse_file(&resolved_path)?;

    if loaded_cards.is_empty() {
        return Err(Diagnostic::invalid_envelope(
            base_file.into(),
            format!("Referenced file is empty: {}", ref_path.display()),
        ));
    }

    if loaded_cards.len() > 1 {
        return Err(Diagnostic::invalid_envelope(
            base_file.into(),
            format!(
                "Referenced file contains multiple cards: {}",
                ref_path.display()
            ),
        ));
    }

    let loaded = loaded_cards.remove(0);

    // Build a CardRef from the loaded card's metadata
    let card_ref = card_ref_for(&loaded)?;

    siblings.push(loaded);

    Ok(card_ref)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use wyrd_spec::envelope::CardKind;
    use wyrd_spec::envelope::Spec;

    #[test]
    fn path_sandbox_accepts_valid_relative() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().to_path_buf();
        let sandbox = PathSandbox::new(root.clone());

        // Create a subdirectory
        let subdir = root.join("agents");
        std::fs::create_dir(&subdir).unwrap();

        // Create a base file
        let base = root.join("service.yaml");
        std::fs::write(&base, "").unwrap();

        // Create a referenced file
        let target = subdir.join("agent.yaml");
        std::fs::write(&target, "").unwrap();

        let (resolved, advisory) = sandbox
            .resolve(&base, Path::new("agents/agent.yaml"))
            .unwrap();

        assert!(advisory.is_none());
        assert!(resolved.ends_with("agents/agent.yaml"));
    }

    #[test]
    fn path_sandbox_accepts_absolute_with_advisory() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().to_path_buf();
        let sandbox = PathSandbox::new(root.clone());

        let base = root.join("service.yaml");
        std::fs::write(&base, "").unwrap();

        let target = root.join("prompt.yaml");
        std::fs::write(&target, "").unwrap();

        let (_resolved, advisory) = sandbox.resolve(&base, &target).unwrap();

        assert!(advisory.is_some());
        let adv = advisory.unwrap();
        assert_eq!(adv.code, "WYRD_LOADER_400_PATH_ABSOLUTE_ADVISORY");
    }

    #[test]
    fn path_sandbox_rejects_relative_traversal() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().join("workspace");
        std::fs::create_dir(&root).unwrap();

        let sandbox = PathSandbox::new(root.clone());

        let base = root.join("service.yaml");
        std::fs::write(&base, "").unwrap();

        // Try to escape via ..
        let result = sandbox.resolve(&base, Path::new("../../etc/passwd"));

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.code, "WYRD_LOADER_400_PATH_ESCAPE");
    }

    #[test]
    fn path_at_inlineable_slot_rewrites_to_ref() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().to_path_buf();

        // Create a prompt file
        let prompt_path = root.join("prompt.yaml");
        std::fs::write(
            &prompt_path,
            r#"
apiVersion: wyrd/v1
kind: Prompt
metadata:
  name: system-prompt
  space: default
  version: "1.0.0"
spec:
  provider: anthropic
  model: claude-sonnet-4.5
  messages: []
"#,
        )
        .unwrap();

        // Create an agent that references the prompt via path
        let agent_path = root.join("agent.yaml");
        std::fs::write(
            &agent_path,
            r#"
apiVersion: wyrd/v1
kind: Agent
metadata:
  name: test-agent
  space: default
  version: "1.0.0"
spec:
  prompt: prompt.yaml
"#,
        )
        .unwrap();

        let sandbox = PathSandbox::new(root.clone());
        let mut cards = parse_file(&agent_path).unwrap();
        assert_eq!(cards.len(), 1);

        let (siblings, _) = resolve_path_ref(&sandbox, &mut cards[0]).unwrap();

        // The prompt should have been loaded as a sibling
        assert_eq!(siblings.len(), 1);
        assert_eq!(siblings[0].kind, CardKind::Prompt);

        // The agent prompt should now have a Ref instead of Path.
        if let Spec::Agent(agent) = &cards[0].spec {
            match &agent.prompt {
                InlineableRef::Ref(card_ref) => {
                    assert_eq!(card_ref.kind, CardKind::Prompt);
                    assert_eq!(card_ref.name.as_str(), "system-prompt");
                }
                InlineableRef::Path(_) | InlineableRef::Inline(_) => {
                    panic!("Path should have been rewritten to Ref")
                }
            }
        } else {
            panic!("Expected Agent spec");
        }
    }
}
