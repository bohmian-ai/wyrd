//! Path reference resolution and sandbox enforcement.

use std::path::{Path, PathBuf};

use super::error::Diagnostic;

/// Path sandbox enforcing workspace root boundaries.
pub struct PathSandbox {
    /// The workspace root — no path may escape this.
    root: PathBuf,
}

impl PathSandbox {
    /// Create a new sandbox rooted at the given path.
    pub fn new(root: &Path) -> Result<Self, Diagnostic> {
        let canonical = root
            .canonicalize()
            .map_err(|error| Diagnostic::io(root.to_path_buf(), &error))?;
        Ok(Self { root: canonical })
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
                .map_err(|e| Diagnostic::io(base_file.into(), &e))?;

            let advisory = Diagnostic::path_absolute_advisory(base_file.into(), &canonical);
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
            .map_err(|e| Diagnostic::io(base_file.into(), &e))?;
        let resolved = normalize_relative_path(&base_dir, ref_path);

        if !resolved.starts_with(&self.root) {
            return Err(Diagnostic::path_escape(base_file.into(), &resolved));
        }

        let canonical = resolved
            .canonicalize()
            .map_err(|e| Diagnostic::io(base_file.into(), &e))?;

        // Check that the canonical path is within the root
        if !canonical.starts_with(&self.root) {
            return Err(Diagnostic::path_escape(base_file.into(), &canonical));
        }

        Ok((canonical, None))
    }
}

fn normalize_relative_path(base: &Path, relative: &Path) -> PathBuf {
    let mut normalized = base.to_path_buf();
    for component in relative.components() {
        match component {
            std::path::Component::CurDir
            | std::path::Component::RootDir
            | std::path::Component::Prefix(_) => {}
            std::path::Component::ParentDir => {
                normalized.pop();
            }
            std::path::Component::Normal(part) => normalized.push(part),
        }
    }
    normalized
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn path_sandbox_accepts_valid_relative() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().to_path_buf();
        let sandbox = PathSandbox::new(&root).unwrap();

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
        let sandbox = PathSandbox::new(&root).unwrap();

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

        let sandbox = PathSandbox::new(&root).unwrap();

        let base = root.join("service.yaml");
        std::fs::write(&base, "").unwrap();

        // Try to escape via ..
        let result = sandbox.resolve(&base, Path::new("../../etc/passwd"));

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.code, "WYRD_LOADER_400_PATH_ESCAPE");
    }
}
