//! Local artifact manifest construction for card registration.

use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};

use base64::Engine;
use sha2::{Digest, Sha256};
use wyrd_spec::error::WyrdError;
use wyrd_spec::registry::{ArtifactManifestEntry, RelativeArtifactPath};

use crate::{Diagnostic, LoadError, Severity};

/// A typed artifact manifest and its local source provenance.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtifactManifest {
    /// Sorted manifest entries sent to the registry.
    pub entries: Vec<ArtifactManifestEntry>,
    /// Local source paths keyed by the manifest-relative path.
    pub sources: std::collections::BTreeMap<RelativeArtifactPath, PathBuf>,
}

/// Build a manifest for every regular file below `root`, excluding the card
/// envelope file.
///
/// Hashing is streamed so registration does not allocate an artifact-sized
/// buffer. The returned paths are relative to `root` and are safe to pass to
/// the registry upload saga.
pub fn build_artifact_manifest(
    root: &Path,
    envelope_name: &str,
) -> Result<ArtifactManifest, LoadError> {
    let mut files = Vec::new();
    collect_files(root, &mut files)?;
    let envelope = root.join(envelope_name);
    files.retain(|path| *path != envelope);
    manifest_of(root, files)
}

/// The sorted manifest of exactly `files`, each a regular file below
/// `root`, with streamed hashes; nothing else under `root` is read.
///
/// # Errors
/// Returns a diagnostic for a path outside `root`, an invalid artifact
/// path, or an unreadable file.
pub(crate) fn manifest_of(
    root: &Path,
    mut files: Vec<PathBuf>,
) -> Result<ArtifactManifest, LoadError> {
    files.sort();
    let mut entries = Vec::new();
    let mut sources = std::collections::BTreeMap::new();
    for path in files {
        let relative = path.strip_prefix(root).map_err(|error| {
            LoadError::single(Diagnostic::invalid_envelope(
                path.clone(),
                format!("artifact path is outside registration directory: {error}"),
            ))
        })?;
        let relative_string = relative.to_string_lossy().replace('\\', "/");
        let relative_path = RelativeArtifactPath::new(&relative_string).map_err(|error| {
            LoadError::single(Diagnostic::invalid_envelope(
                path.clone(),
                format!("invalid artifact path: {error}"),
            ))
        })?;
        let (sha256, size_bytes) = hash_file(&path)?;
        entries.push(ArtifactManifestEntry {
            relative_path: relative_path.clone(),
            sha256: Some(sha256),
            size_bytes: Some(size_bytes),
            content_type: None,
        });
        sources.insert(relative_path, path);
    }

    Ok(ArtifactManifest { entries, sources })
}

/// Every regular file below `root`, recursively.
///
/// # Errors
/// Returns an IO diagnostic for an unreadable directory, and a
/// [`WyrdError::Validation`] diagnostic with rule [`NONREGULAR`] naming the
/// entry for a link or other non-regular entry.
pub(crate) fn collect_files(root: &Path, files: &mut Vec<PathBuf>) -> Result<(), LoadError> {
    let entries = fs::read_dir(root)
        .map_err(|error| LoadError::single(Diagnostic::io(root.to_path_buf(), &error)))?;
    for entry in entries {
        let entry =
            entry.map_err(|error| LoadError::single(Diagnostic::io(root.to_path_buf(), &error)))?;
        let path = entry.path();
        let file_type = entry
            .file_type()
            .map_err(|error| LoadError::single(Diagnostic::io(path.clone(), &error)))?;
        if file_type.is_dir() {
            collect_files(&path, files)?;
        } else if file_type.is_file() {
            files.push(path);
        } else {
            let refused = nonregular(&path);
            return Err(LoadError::single(Diagnostic::from_wyrd_error(
                path,
                Severity::Error,
                None,
                &refused,
            )));
        }
    }
    Ok(())
}

/// The validation rule of an artifact source that is not a regular file or a
/// directory of regular files.
pub(crate) const NONREGULAR: &str = "artifact_source_regular";

/// The refusal of `path`, a link or other non-regular artifact source.
pub(crate) fn nonregular(path: &Path) -> WyrdError {
    WyrdError::Validation {
        message: format!(
            "source: {} is not a regular file or a directory of regular files",
            path.display()
        ),
        details: serde_json::json!({
            "rule": NONREGULAR,
            "field": "source",
            "source": path.display().to_string(),
            "expected": "a regular file or a directory of regular files",
        }),
    }
}

fn hash_file(path: &Path) -> Result<(String, u64), LoadError> {
    let mut file = File::open(path)
        .map_err(|error| LoadError::single(Diagnostic::io(path.to_path_buf(), &error)))?;
    let mut digest = Sha256::new();
    let mut size = 0_u64;
    let mut buffer = vec![0_u8; 64 * 1024].into_boxed_slice();
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|error| LoadError::single(Diagnostic::io(path.to_path_buf(), &error)))?;
        if read == 0 {
            break;
        }
        digest.update(&buffer[..read]);
        size = size.checked_add(read as u64).ok_or_else(|| {
            LoadError::single(Diagnostic::invalid_envelope(
                path.to_path_buf(),
                "artifact is too large for the registry manifest".to_owned(),
            ))
        })?;
    }
    Ok((
        base64::engine::general_purpose::STANDARD.encode(digest.finalize()),
        size,
    ))
}

#[cfg(test)]
mod tests {
    use std::fs;

    use base64::Engine;
    use sha2::{Digest, Sha256};
    use tempfile::tempdir;

    use super::build_artifact_manifest;

    #[test]
    fn builds_sorted_manifest_and_excludes_envelope() {
        let root = tempdir().expect("manifest root creates");
        fs::create_dir(root.path().join("nested")).expect("nested directory creates");
        fs::write(root.path().join("card.json"), "{}").expect("envelope writes");
        fs::write(root.path().join("z.bin"), b"z").expect("first artifact writes");
        fs::write(root.path().join("nested/a.bin"), b"abc").expect("second artifact writes");

        let manifest =
            build_artifact_manifest(root.path(), "card.json").expect("artifact manifest builds");

        assert_eq!(
            manifest
                .entries
                .iter()
                .map(|entry| entry.relative_path.as_str())
                .collect::<Vec<_>>(),
            ["nested/a.bin", "z.bin"]
        );
        assert_eq!(manifest.entries[0].size_bytes, Some(3));
        let mut digest = Sha256::new();
        digest.update(b"abc");
        assert_eq!(
            manifest.entries[0].sha256,
            Some(base64::engine::general_purpose::STANDARD.encode(digest.finalize()))
        );
        assert_eq!(manifest.sources.len(), 2);
    }

    #[cfg(unix)]
    #[test]
    fn rejects_symbolic_links_as_artifacts() {
        let root = tempdir().expect("manifest root creates");
        fs::write(root.path().join("source.bin"), b"bytes").expect("source writes");
        std::os::unix::fs::symlink(
            root.path().join("source.bin"),
            root.path().join("linked.bin"),
        )
        .expect("symlink creates");

        let error = build_artifact_manifest(root.path(), "card.json")
            .expect_err("symlink should not be accepted");

        let [refused] = error.diagnostics.as_slice() else {
            panic!("one diagnostic: {error:?}");
        };
        assert_eq!(
            (refused.code, refused.context.as_deref().map(|c| &c["rule"])),
            (
                "WYRD_SPEC_400_VALIDATION",
                Some(&serde_json::json!(super::NONREGULAR))
            )
        );
        assert_eq!(refused.path, root.path().join("linked.bin"));
    }
}
