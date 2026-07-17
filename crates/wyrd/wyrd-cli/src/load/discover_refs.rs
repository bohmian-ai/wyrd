//! Reference slot discovery using the canonical visitor.

use std::path::PathBuf;

use wyrd_spec::refs::ReferenceSlotVisitor;

use super::parse::AuthoredCard;

/// Discovered reference slots for a single card.
pub struct DiscoveredRefs {
    /// The card's source path.
    pub source_path: PathBuf,
    /// Slot paths collected by the visitor.
    pub slot_paths: Vec<String>,
}

/// Discover all reference slots on an authored card.
///
/// This is a thin driver over `wyrd_spec::refs::ReferenceSlotVisitor`. It does
/// not enumerate slot shapes itself — the visitor is the single source of truth.
pub fn discover_refs(card: &mut AuthoredCard) -> DiscoveredRefs {
    let source_path = card.source_path.clone();
    let mut slot_paths = Vec::new();

    ReferenceSlotVisitor::visit(&mut card.spec, |entry| {
        slot_paths.push(entry.path.clone());
    });

    DiscoveredRefs {
        source_path,
        slot_paths,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn discover_refs_yields_durable_slots() {
        let mut file = NamedTempFile::new().unwrap();
        writeln!(
            file,
            r#"
apiVersion: wyrd/v1
kind: Service
metadata:
  name: test-agent
  space: default
  version: "1.0.0"
spec:
  components:
    - alias: dependency
      ref:
        kind: Model
        name: test-model
        version: "1.0.0"
        space: default
"#
        )
        .unwrap();
        file.flush().unwrap();

        let mut cards = crate::load::parse::parse_file(file.path()).unwrap();
        assert_eq!(cards.len(), 1);

        let discovered = discover_refs(&mut cards[0]);
        assert!(
            discovered
                .slot_paths
                .iter()
                .any(|p| p == "spec.components[0].card_ref")
        );
        assert!(
            discovered
                .slot_paths
                .iter()
                .all(|p| p != "spec.publishes_to[0]")
        );
    }

    #[test]
    fn discover_refs_yields_inlineable_slots() {
        let mut file = NamedTempFile::new().unwrap();
        writeln!(
            file,
            r#"
apiVersion: wyrd/v1
kind: Agent
metadata:
  name: test
  space: default
  version: "1.0.0"
spec:
  prompt:
    kind: Prompt
    name: system-prompt
    version: "1.0.0"
    space: default
"#
        )
        .unwrap();
        file.flush().unwrap();

        let mut cards = crate::load::parse::parse_file(file.path()).unwrap();
        let discovered = discover_refs(&mut cards[0]);

        assert!(discovered.slot_paths.iter().any(|p| p == "spec.prompt"));
    }
}
