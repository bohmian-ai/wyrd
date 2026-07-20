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

    /// Return the canonical workspace root used for containment checks.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
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
                .map_err(|error| Diagnostic::io(base_file.to_path_buf(), &error))?;

            let advisory = Diagnostic::path_absolute_advisory(base_file.into(), &canonical);
            return Ok((canonical, Some(advisory)));
        }

        // Relative paths resolve from the base file's directory
        Ok((self.resolve_contained(base_file, ref_path)?, None))
    }

    /// Resolve a relative path and require its canonical target to remain in
    /// this sandbox.
    pub fn resolve_contained(
        &self,
        base_file: &Path,
        ref_path: &Path,
    ) -> Result<PathBuf, Diagnostic> {
        if ref_path.is_absolute() {
            return Err(Diagnostic::path_escape(base_file.to_path_buf(), ref_path));
        }

        let base_dir = base_file.parent().ok_or_else(|| {
            Diagnostic::invalid_envelope(
                base_file.to_path_buf(),
                "Cannot resolve parent directory".to_owned(),
            )
        })?;
        let base_dir = base_dir
            .canonicalize()
            .map_err(|error| Diagnostic::io(base_file.to_path_buf(), &error))?;
        let resolved = normalize_relative_path(&base_dir, ref_path);
        if !resolved.starts_with(&self.root) {
            return Err(Diagnostic::path_escape(base_file.to_path_buf(), &resolved));
        }
        self.canonicalize_contained(base_file, &resolved)
    }

    /// Canonicalize an existing path and require it to remain in this sandbox.
    pub fn ensure_contained(&self, base_file: &Path, path: &Path) -> Result<PathBuf, Diagnostic> {
        self.canonicalize_contained(base_file, path)
    }

    /// Resolve a contained path and verify that it is a readable regular file.
    pub fn resolve_regular_file(
        &self,
        base_file: &Path,
        ref_path: &Path,
    ) -> Result<PathBuf, Diagnostic> {
        let canonical = self.resolve_contained(base_file, ref_path)?;
        let metadata = std::fs::metadata(&canonical)
            .map_err(|error| Diagnostic::io(base_file.to_path_buf(), &error))?;
        if !metadata.is_file() {
            return Err(Diagnostic::invalid_envelope(
                base_file.to_path_buf(),
                format!(
                    "referenced artifact is not a regular file: {}",
                    ref_path.display()
                ),
            ));
        }
        std::fs::File::open(&canonical)
            .map_err(|error| Diagnostic::io(base_file.to_path_buf(), &error))?;
        Ok(canonical)
    }

    fn canonicalize_contained(&self, base_file: &Path, path: &Path) -> Result<PathBuf, Diagnostic> {
        let canonical = path
            .canonicalize()
            .map_err(|error| Diagnostic::io(base_file.to_path_buf(), &error))?;
        if !canonical.starts_with(&self.root) {
            return Err(Diagnostic::path_escape(base_file.to_path_buf(), &canonical));
        }
        Ok(canonical)
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

    #[cfg(unix)]
    #[test]
    fn path_sandbox_allows_absolute_target_with_advisory() {
        let root_temp = TempDir::new().unwrap();
        let outside_temp = TempDir::new().unwrap();
        let outside = outside_temp.path().join("secret.yaml");
        std::fs::write(&outside, "secret").unwrap();
        let base = root_temp.path().join("service.yaml");
        std::fs::write(&base, "").unwrap();
        let sandbox = PathSandbox::new(root_temp.path()).unwrap();

        let (resolved, advisory) = sandbox
            .resolve(&base, &outside)
            .expect("absolute target remains an allowed portability escape");
        assert_eq!(resolved, outside.canonicalize().unwrap());
        assert_eq!(
            advisory.expect("absolute target emits advisory").code,
            "WYRD_LOADER_400_PATH_ABSOLUTE_ADVISORY"
        );
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

    #[cfg(unix)]
    #[test]
    fn path_sandbox_rejects_symlink_target_outside_root() {
        use std::os::unix::fs::symlink;

        let root_temp = TempDir::new().unwrap();
        let outside_temp = TempDir::new().unwrap();
        let outside = outside_temp.path().join("secret.yaml");
        std::fs::write(&outside, "secret").unwrap();
        let link = root_temp.path().join("linked.yaml");
        symlink(&outside, &link).unwrap();

        let sandbox = PathSandbox::new(root_temp.path()).unwrap();
        let error = sandbox
            .ensure_contained(&link, &link)
            .expect_err("symlink target must remain inside root");
        assert_eq!(error.code, "WYRD_LOADER_400_PATH_ESCAPE");
    }

    #[cfg(unix)]
    #[test]
    fn path_discovery_does_not_follow_symlink_directories() {
        use std::os::unix::fs::symlink;

        let temp = TempDir::new().unwrap();
        let cards = temp.path().join("cards");
        std::fs::create_dir(&cards).unwrap();
        std::fs::write(cards.join("prompt.yaml"), "apiVersion: wyrd/v1\nkind: Prompt\nmetadata:\n  name: prompt\n  space: default\n  version: 1.0.0\nspec:\n  provider: anthropic\n  model: claude\n  messages: []\n").unwrap();
        symlink(&cards, temp.path().join("linked-cards")).unwrap();

        let loaded = crate::parse::parse_path(temp.path()).unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].metadata.name.as_str(), "prompt");
    }

    #[cfg(unix)]
    #[test]
    fn path_discovery_reports_dangling_yaml_symlink() {
        use std::os::unix::fs::symlink;

        let temp = TempDir::new().unwrap();
        symlink("missing.yaml", temp.path().join("dangling.yaml")).unwrap();

        let error = crate::parse::parse_path(temp.path())
            .expect_err("dangling YAML symlink must not be read");
        assert_eq!(error.diagnostics[0].code, "WYRD_LOADER_400_IO");
    }
}
