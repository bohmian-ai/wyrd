//! Pinned original evidence read from the execution tenant's storage.
//!
//! [`TenantEvidence`] is the one byte reader for file evidence. It reads
//! `<tenant>/evidence/sha256/<digest>` bounded by the declared length and the
//! file limit, and returns bytes only when their length, SHA-256, and sniffed
//! format all match the declaration, so a mismatched file can never feed a
//! pass. Extraction, workspaces, citations, and native media all read through
//! it; it is also the Agent executor's [`EvidenceReader`].

use std::sync::Arc;

use async_trait::async_trait;
use base64::Engine as _;
use skald_spec::{MediaFormat, MediaRef, MediaSource};
use vala_eval::blocks::{BlockError, EvidenceReader};
use wyrd_spec::DataTenantId;
use wyrd_spec::auth::Sha256Hex;
use wyrd_spec::verification::execution::InputDeclaration;
use wyrd_storage::error::StorageError;
use wyrd_storage::handle::StorageHandle;
use wyrd_storage::tenant_path;

use super::HostedLimits;

/// Reads one tenant's pinned file evidence.
pub struct TenantEvidence {
    /// The server's storage handle.
    storage: Arc<StorageHandle>,
    /// Tenant whose evidence namespace every read uses.
    tenant: DataTenantId,
    /// File limits in force.
    limits: HostedLimits,
}

impl TenantEvidence {
    /// A reader of `tenant`'s evidence in `storage` under `limits`.
    #[must_use]
    pub const fn new(
        storage: Arc<StorageHandle>,
        tenant: DataTenantId,
        limits: HostedLimits,
    ) -> Self {
        Self {
            storage,
            tenant,
            limits,
        }
    }

    /// The verified bytes and format of file `evidence`.
    ///
    /// The declared length is checked against the file limit and the stored
    /// object's length before the bounded read; the body's length, digest,
    /// and sniffed format are checked after it.
    ///
    /// # Errors
    /// Returns [`BlockError::Terminal`] for a non-file declaration, a file
    /// over the limit, an absent object, or any length, digest, or format
    /// mismatch, and [`BlockError::Retryable`] for another storage failure.
    pub async fn read(
        &self,
        evidence: &InputDeclaration,
    ) -> Result<(Vec<u8>, MediaFormat), BlockError> {
        let InputDeclaration::File {
            sha256,
            size_bytes,
            format,
            ..
        } = evidence
        else {
            return Err(terminal("only uploaded file evidence has readable bytes"));
        };
        if *size_bytes > self.limits.file_max_bytes {
            return Err(terminal("the file exceeds the input file limit"));
        }
        let path = tenant_path::evidence(self.tenant, sha256);
        let stored = self
            .storage
            .object_len(&path)
            .await
            .map_err(|error| self.failed(&error))?;
        if stored != *size_bytes {
            return Err(terminal(
                "the stored file length does not match its declaration",
            ));
        }
        let bytes = self
            .storage
            .get_object_bounded(&path, *size_bytes)
            .await
            .map_err(|error| self.failed(&error))?;
        if u64::try_from(bytes.len()).ok() != Some(*size_bytes) {
            return Err(terminal(
                "the stored file length does not match its declaration",
            ));
        }
        if Sha256Hex::digest(&bytes) != *sha256 {
            return Err(terminal(
                "the stored file digest does not match its declaration",
            ));
        }
        if !sniffs_as(&bytes, *format) {
            return Err(BlockError::Terminal {
                reason: format!("the file content is not {format}"),
            });
        }
        Ok((bytes, *format))
    }

    /// Classify a storage failure without exposing its key or backend text.
    fn failed(&self, error: &StorageError) -> BlockError {
        if matches!(error, StorageError::ObjectNotFound { .. }) {
            return terminal("the evidence file was never uploaded");
        }
        tracing::warn!(backend = ?self.storage.backend(), "verification evidence read failed");
        BlockError::Retryable {
            reason: "the evidence storage read failed".to_owned(),
        }
    }
}

#[async_trait]
impl EvidenceReader for TenantEvidence {
    /// Read `evidence` as inline provider-native media.
    ///
    /// # Errors
    /// Returns [`Self::read`]'s errors and [`BlockError::Terminal`] for a
    /// format with no native media kind.
    async fn media(&self, evidence: &InputDeclaration) -> Result<MediaRef, BlockError> {
        let (bytes, format) = self.read(evidence).await?;
        let kind = format.native_kind().ok_or_else(|| BlockError::Terminal {
            reason: format!(
                "{format} is not provider-native media; read it through a step workspace"
            ),
        })?;
        Ok(MediaRef {
            kind,
            source: MediaSource::Base64 {
                mime_type: format.mime().to_owned(),
                data: base64::engine::general_purpose::STANDARD.encode(bytes),
            },
        })
    }
}

/// Whether `bytes` carry the signature of `format`.
///
/// Binary formats match their magic bytes; OOXML formats must also contain
/// their main part; text formats must be UTF-8.
#[must_use]
pub fn sniffs_as(bytes: &[u8], format: MediaFormat) -> bool {
    let ooxml = |part: &str| {
        zip::ZipArchive::new(std::io::Cursor::new(bytes))
            .is_ok_and(|archive| archive.index_for_name(part).is_some())
    };
    match format {
        MediaFormat::Pdf => bytes.starts_with(b"%PDF-"),
        MediaFormat::Png => bytes.starts_with(b"\x89PNG\r\n\x1a\n"),
        MediaFormat::Jpeg => bytes.starts_with(&[0xFF, 0xD8, 0xFF]),
        MediaFormat::Gif => bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a"),
        MediaFormat::Webp => {
            bytes.len() >= 12 && bytes.starts_with(b"RIFF") && &bytes[8..12] == b"WEBP"
        }
        MediaFormat::Zip => bytes.starts_with(b"PK\x03\x04") || bytes.starts_with(b"PK\x05\x06"),
        MediaFormat::Docx => ooxml("word/document.xml"),
        MediaFormat::Xlsx => ooxml("xl/workbook.xml"),
        MediaFormat::Pptx => ooxml("ppt/presentation.xml"),
        MediaFormat::Doc | MediaFormat::Xls | MediaFormat::Ppt => {
            bytes.starts_with(&[0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1])
        }
        MediaFormat::TarGz => bytes.starts_with(&[0x1F, 0x8B]),
        MediaFormat::Text
        | MediaFormat::Markdown
        | MediaFormat::Html
        | MediaFormat::Csv
        | MediaFormat::Json => std::str::from_utf8(bytes).is_ok(),
    }
}

/// A terminal error with fixed `reason`.
fn terminal(reason: &str) -> BlockError {
    BlockError::Terminal {
        reason: reason.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    //! Pinned evidence reads accept only exact, matching files.

    use wyrd_storage::local::LocalSigner;

    use super::*;

    /// Evidence over a fresh local store holding `bytes` under their digest.
    async fn stored(bytes: &[u8]) -> (tempfile::TempDir, TenantEvidence, Sha256Hex) {
        let root = tempfile::tempdir().expect("temp dir");
        let storage = Arc::new(StorageHandle::new(
            LocalSigner::new(root.path().to_path_buf()).expect("local signer"),
        ));
        let tenant = DataTenantId::new_v7();
        let sha256 = Sha256Hex::digest(bytes);
        storage
            .put_object(&tenant_path::evidence(tenant, &sha256), bytes.to_vec())
            .await
            .expect("the evidence writes");
        (
            root,
            TenantEvidence::new(storage, tenant, HostedLimits::default()),
            sha256,
        )
    }

    /// A file declaration.
    fn file(sha256: Sha256Hex, size_bytes: u64, format: MediaFormat) -> InputDeclaration {
        InputDeclaration::File {
            sha256,
            size_bytes,
            format,
            filename: None,
        }
    }

    /// The exact file reads; wrong length, digest, format, limit, absent
    /// objects, and non-file declarations are terminal.
    ///
    /// # Panics
    /// Panics when a mismatched read succeeds.
    #[tokio::test]
    async fn evidence_reads_only_matching_files() {
        let (_root, evidence, sha256) = stored(b"%PDF-1.5 body").await;
        let (bytes, format) = evidence
            .read(&file(sha256, 13, MediaFormat::Pdf))
            .await
            .expect("the pinned file reads");
        assert_eq!(
            (bytes.as_slice(), format),
            (&b"%PDF-1.5 body"[..], MediaFormat::Pdf)
        );

        for declaration in [
            file(sha256, 12, MediaFormat::Pdf),
            file(sha256, 13, MediaFormat::Png),
            file(Sha256Hex::digest(b"other"), 13, MediaFormat::Pdf),
            file(sha256, 21 * 1024 * 1024, MediaFormat::Pdf),
            InputDeclaration::Json {
                value: serde_json::json!(1),
            },
        ] {
            let error = evidence.read(&declaration).await.expect_err("refused");
            assert!(matches!(error, BlockError::Terminal { .. }), "{error}");
        }
    }

    /// A stored body whose digest differs from its key is refused.
    ///
    /// # Panics
    /// Panics when the tampered body reads.
    #[tokio::test]
    async fn evidence_refuses_tampered_body() {
        let (root, evidence, sha256) = stored(b"hello").await;
        let key = tenant_path::evidence(evidence.tenant, &sha256).full;
        std::fs::write(root.path().join(key), b"jello").expect("tamper");
        let error = evidence
            .read(&file(sha256, 5, MediaFormat::Text))
            .await
            .expect_err("digest mismatch");
        assert!(error.to_string().contains("digest"), "{error}");
    }

    /// Native media is base64 with the format's MIME type; non-native
    /// formats are refused.
    ///
    /// # Panics
    /// Panics when media binding differs.
    #[tokio::test]
    async fn evidence_binds_native_media_only() {
        let (_root, evidence, sha256) = stored(b"%PDF-1.5").await;
        let media = evidence
            .media(&file(sha256, 8, MediaFormat::Pdf))
            .await
            .expect("pdf is native");
        assert_eq!(
            media.source,
            MediaSource::Base64 {
                mime_type: "application/pdf".to_owned(),
                data: "JVBERi0xLjU=".to_owned(),
            }
        );
        let (_root, evidence, sha256) = stored(b"a,b").await;
        assert!(
            evidence
                .media(&file(sha256, 3, MediaFormat::Csv))
                .await
                .is_err()
        );
    }
}
