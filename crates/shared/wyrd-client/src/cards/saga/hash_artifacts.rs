//! Client-owned artifact provenance and aggregate manifest hashing.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use base64::Engine;
use sha2::{Digest, Sha256};
use tokio::io::{AsyncReadExt, BufReader};
use wyrd_spec::error::WyrdError;
use wyrd_spec::registry::{CardSubmission, RelativeArtifactPath};

use crate::cards::error::RegistryEngineError;

/// Validate source bytes before any registration network call and stamp the
/// manifest hash that the server validates at the registration boundary.
pub(crate) async fn validate_and_stamp(
    submission: &mut CardSubmission,
    sources: &BTreeMap<RelativeArtifactPath, PathBuf>,
) -> Result<(), RegistryEngineError> {
    submission
        .artifacts
        .sort_by(|left, right| left.relative_path.cmp(&right.relative_path));
    for artifact in &submission.artifacts {
        let Some(source) = sources.get(&artifact.relative_path) else {
            continue;
        };
        let (actual_size, actual_sha256) = hash_source(source).await?;
        if actual_size != artifact.size_bytes {
            return Err(WyrdError::RegistryManifestHashMismatch {
                message: "local artifact size does not match its manifest".to_owned(),
                details: serde_json::json!({
                    "relative_path": artifact.relative_path,
                    "expected_size_bytes": artifact.size_bytes,
                    "actual_size_bytes": actual_size,
                }),
            }
            .into());
        }
        if actual_sha256 != artifact.sha256 {
            return Err(WyrdError::RegistryManifestHashMismatch {
                message: "local artifact digest does not match its manifest".to_owned(),
                details: serde_json::json!({ "relative_path": artifact.relative_path }),
            }
            .into());
        }
    }
    if submission.artifacts.is_empty() {
        submission.metadata.artifact_hash = None;
        return Ok(());
    }
    let bytes =
        serde_jcs::to_vec(&submission.artifacts).map_err(WyrdError::from_spec_serialization)?;
    submission.metadata.artifact_hash = Some(blake3::hash(&bytes).to_hex().to_string());
    Ok(())
}

/// Stream one local artifact file and return its byte length and base64
/// SHA-256 digest, the two values compared against the manifest entry by
/// [`validate_and_stamp`].
///
/// The file is read in 64 KiB buffers so large artifacts are never loaded
/// into memory. The operation is read-only; cancellation simply drops the
/// open file handle.
///
/// # Errors
///
/// Returns an IO error when the file cannot be opened or read, and
/// `RegistrySpecTooLarge` when the cumulative size overflows `u64`.
///
/// # Panics
///
/// Panics only if a buffer length (at most 64 KiB) does not fit in `u64`,
/// which cannot happen on supported targets.
async fn hash_source(source: &Path) -> Result<(u64, String), RegistryEngineError> {
    let file = tokio::fs::File::open(source).await?;
    let mut reader = BufReader::new(file);
    let mut hasher = Sha256::new();
    let mut size = 0_u64;
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = reader.read(&mut buffer).await?;
        if read == 0 {
            break;
        }
        let read_u64 = u64::try_from(read).expect("invariant: read buffer length fits u64");
        size = size.checked_add(read_u64).ok_or_else(|| {
            RegistryEngineError::Wyrd(WyrdError::RegistrySpecTooLarge {
                message: "local artifact exceeds the supported size range".to_owned(),
                details: serde_json::json!({ "source": source }),
            })
        })?;
        hasher.update(&buffer[..read]);
    }
    Ok((
        size,
        base64::engine::general_purpose::STANDARD.encode(hasher.finalize()),
    ))
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use base64::Engine as _;
    use sha2::Digest;
    use tempfile::tempdir;
    use wyrd_spec::api_version::ApiVersion;
    use wyrd_spec::envelope::{CardKind, Metadata};
    use wyrd_spec::registry::{ArtifactManifestEntry, CardSubmission, RelativeArtifactPath};

    use super::validate_and_stamp;
    use crate::cards::error::RegistryEngineError;

    fn submission(entry: ArtifactManifestEntry) -> CardSubmission {
        CardSubmission {
            api_version: ApiVersion::v1(),
            kind: CardKind::Prompt,
            metadata: Metadata {
                name: "hash-test".parse().expect("test name is valid"),
                version: Some("1.0.0".parse().expect("test version is valid")),
                bump: None,
                space: Some("default".parse().expect("test space is valid")),
                uid: None,
                labels: Default::default(),
                annotations: Default::default(),
                spec_hash: None,
                artifact_hash: None,
                origin: None,
            },
            spec: serde_json::json!({}),
            artifacts: vec![entry],
        }
    }

    #[tokio::test]
    async fn validates_sources_and_stamps_sorted_manifest_hash() {
        let directory = tempdir().expect("temporary directory creates");
        let source = directory.path().join("weights.bin");
        let bytes = b"weights";
        tokio::fs::write(&source, bytes)
            .await
            .expect("artifact writes");
        let relative_path = RelativeArtifactPath::new("weights.bin").expect("path is valid");
        let entry = ArtifactManifestEntry {
            relative_path: relative_path.clone(),
            sha256: base64::engine::general_purpose::STANDARD.encode(sha2::Sha256::digest(bytes)),
            size_bytes: u64::try_from(bytes.len()).expect("test bytes length fits u64"),
            content_type: None,
        };
        let mut card = submission(entry);
        let sources = BTreeMap::from([(relative_path, source)]);

        validate_and_stamp(&mut card, &sources)
            .await
            .expect("manifest validates");

        assert!(card.metadata.artifact_hash.is_some());
    }

    #[tokio::test]
    async fn rejects_source_digest_mismatch_before_upload() {
        let directory = tempdir().expect("temporary directory creates");
        let source = directory.path().join("weights.bin");
        tokio::fs::write(&source, b"actual")
            .await
            .expect("artifact writes");
        let relative_path = RelativeArtifactPath::new("weights.bin").expect("path is valid");
        let entry = ArtifactManifestEntry {
            relative_path: relative_path.clone(),
            sha256: "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=".to_owned(),
            size_bytes: 6,
            content_type: None,
        };
        let mut card = submission(entry);
        let sources = BTreeMap::from([(relative_path, source)]);

        let error = validate_and_stamp(&mut card, &sources)
            .await
            .expect_err("digest mismatch must fail");
        assert!(matches!(
            error,
            RegistryEngineError::Wyrd(
                wyrd_spec::error::WyrdError::RegistryManifestHashMismatch { .. }
            )
        ));
    }

    #[tokio::test]
    async fn rejects_short_and_long_sources_before_upload() {
        for (bytes, expected_size) in [(b"short".as_slice(), 6_u64), (b"longer".as_slice(), 5)] {
            let directory = tempdir().expect("temporary directory creates");
            let source = directory.path().join("weights.bin");
            tokio::fs::write(&source, bytes)
                .await
                .expect("artifact writes");
            let relative_path = RelativeArtifactPath::new("weights.bin").expect("path is valid");
            let entry = ArtifactManifestEntry {
                relative_path: relative_path.clone(),
                sha256: base64::engine::general_purpose::STANDARD
                    .encode(sha2::Sha256::digest(bytes)),
                size_bytes: expected_size,
                content_type: None,
            };
            let mut card = submission(entry);
            let sources = BTreeMap::from([(relative_path, source)]);

            let error = validate_and_stamp(&mut card, &sources)
                .await
                .expect_err("size mismatch must fail");
            assert!(matches!(
                error,
                RegistryEngineError::Wyrd(
                    wyrd_spec::error::WyrdError::RegistryManifestHashMismatch { .. }
                )
            ));
        }
    }
}
