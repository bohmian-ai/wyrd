//! Pinned original evidence read from the execution tenant's storage.
//!
//! [`TenantEvidence`] is the one byte reader for file evidence and the one
//! citation validator over an execution's pinned input slots. It reads
//! `<tenant>/evidence/sha256/<digest>` bounded by the declared length and the
//! file limit, and returns bytes only when their length, SHA-256, and sniffed
//! format all match the declaration, so a mismatched file can never feed a
//! pass. Extraction, workspaces, citations, and native media all read through
//! it; it is also the Agent executor's [`EvidenceReader`] and the `extract`
//! action's [`ActionExecutor`].

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use async_trait::async_trait;
use base64::Engine as _;
use skald_spec::{MediaFormat, MediaRef, MediaSource};
use vala_eval::blocks::{
    ActionExecutor, ActionOutput, BlockCall, BlockError, CallValue, EvidenceReader,
};
use wyrd_spec::DataTenantId;
use wyrd_spec::auth::Sha256Hex;
use wyrd_spec::card::blocks::Action;
use wyrd_spec::ids::SlotName;
use wyrd_spec::verification::execution::{CallProvenance, Citation, InputDeclaration, Locator};
use wyrd_storage::error::StorageError;
use wyrd_storage::handle::StorageHandle;
use wyrd_storage::tenant_path;

use super::HostedLimits;
use super::extract::{EXTRACT_VERSION, Extraction, locates, units};

/// Reads one execution's pinned evidence from its tenant's storage.
pub struct TenantEvidence {
    /// The server's storage handle.
    storage: Arc<StorageHandle>,
    /// Tenant whose evidence namespace every read uses.
    tenant: DataTenantId,
    /// The execution's pinned input declarations, the only citable sources.
    slots: BTreeMap<SlotName, InputDeclaration>,
    /// File limits in force.
    limits: HostedLimits,
}

impl TenantEvidence {
    /// A reader of `tenant`'s evidence in `storage` whose citable sources
    /// are the execution's pinned `slots`, under `limits`.
    #[must_use]
    pub const fn new(
        storage: Arc<StorageHandle>,
        tenant: DataTenantId,
        slots: BTreeMap<SlotName, InputDeclaration>,
        limits: HostedLimits,
    ) -> Self {
        Self {
            storage,
            tenant,
            slots,
            limits,
        }
    }

    /// The located units of pinned slot `declaration`, or `None` when it
    /// has none: inline JSON leaves, extracted file units, or for a native
    /// image the single page a region may cite.
    ///
    /// # Errors
    /// Returns [`BlockError::Retryable`] when the evidence cannot be read
    /// now; a terminal read or extraction failure yields `None`.
    async fn located(
        &self,
        declaration: &InputDeclaration,
    ) -> Result<Option<Vec<(Locator, String)>>, BlockError> {
        if let InputDeclaration::Json { value } = declaration {
            return Ok(Some(units(
                &wyrd_spec::card::blocks::extract::ExtractOutput::Json {
                    value: value.clone(),
                },
                b"",
            )));
        }
        let (bytes, format) = match self.read(declaration).await {
            Ok(read) => read,
            Err(BlockError::Terminal { .. }) => return Ok(None),
            Err(error) => return Err(error),
        };
        if format.native_kind() == Some(skald_spec::MediaKind::Image) {
            return Ok(Some(vec![(Locator::Page(1), String::new())]));
        }
        let limits = self.limits;
        let extracted = tokio::task::spawn_blocking(move || {
            Extraction::new(&limits)
                .run(&bytes, format)
                .map(|output| units(&output, &bytes))
        })
        .await
        .map_err(|error| BlockError::Retryable {
            reason: format!("citation extraction stopped: {error}"),
        })?;
        Ok(extracted.ok())
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

    /// Keep each citation whose slot is reachable and pinned and whose
    /// locator names a location in that slot's extraction; each cited slot
    /// is read and extracted once per call.
    ///
    /// # Errors
    /// Returns [`BlockError::Retryable`] when cited evidence cannot be read
    /// now.
    async fn cited(
        &self,
        reachable: &BTreeSet<SlotName>,
        citations: Vec<Citation>,
    ) -> Result<Vec<Citation>, BlockError> {
        let mut located: BTreeMap<SlotName, Option<Vec<(Locator, String)>>> = BTreeMap::new();
        let mut valid = Vec::new();
        for citation in citations {
            let Some(declaration) = self
                .slots
                .get(&citation.slot)
                .filter(|_| reachable.contains(&citation.slot))
            else {
                continue;
            };
            if !located.contains_key(&citation.slot) {
                let units = self.located(declaration).await?;
                located.insert(citation.slot.clone(), units);
            }
            if let Some(Some(units)) = located.get(&citation.slot)
                && locates(units, &citation.locator)
            {
                valid.push(citation);
            }
        }
        Ok(valid)
    }
}

#[async_trait]
impl ActionExecutor for TenantEvidence {
    /// Run an `extract` action: read its `from` evidence and extract it on
    /// the blocking pool into the locked output shape.
    ///
    /// # Errors
    /// Returns [`BlockError::Terminal`] for a non-extract action or a `from`
    /// that is not evidence, [`Self::read`]'s errors, extraction refusals,
    /// and [`BlockError::Retryable`] when the blocking task fails.
    async fn execute(&self, call: BlockCall<'_, Action>) -> Result<ActionOutput, BlockError> {
        let Action::Extract { .. } = call.block else {
            return Err(terminal("the extract executor runs only extract actions"));
        };
        let Some(CallValue::Evidence(declaration)) =
            call.inputs.get("from").map(|input| &input.value)
        else {
            return Err(terminal("extract reads only media evidence"));
        };
        let (bytes, format) = self.read(declaration).await?;
        let limits = self.limits;
        let output =
            tokio::task::spawn_blocking(move || Extraction::new(&limits).run(&bytes, format))
                .await
                .map_err(|error| BlockError::Retryable {
                    reason: format!("extraction stopped: {error}"),
                })??;
        Ok(ActionOutput {
            output: serde_json::to_value(output).map_err(|error| BlockError::Terminal {
                reason: format!("extraction output does not serialize: {error}"),
            })?,
            provenance: Some(CallProvenance::Extract {
                version: EXTRACT_VERSION.to_owned(),
            }),
            tool_calls: Vec::new(),
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

    use vala_eval::blocks::{CallInput, Position};
    use wyrd_spec::vala::eval::TaskId;
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
            TenantEvidence::new(storage, tenant, BTreeMap::new(), HostedLimits::default()),
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

    /// `extract` reads `from` evidence and returns the locked shape with
    /// extractor provenance; non-evidence `from` is refused.
    ///
    /// # Panics
    /// Panics when the output or provenance differs.
    #[tokio::test]
    async fn extract_action_returns_locked_shape() {
        let (_root, evidence, sha256) = stored(b"a,b\n1,2").await;
        let id = TaskId::new("pull").expect("task id");
        let block: Action =
            serde_json::from_value(serde_json::json!({"kind": "extract", "from": "inputs.sheet"}))
                .expect("extract action");
        let call = |value| BlockCall {
            id: &id,
            position: Position::Step,
            block: &block,
            inputs: BTreeMap::from([(
                "from".to_owned(),
                CallInput {
                    value,
                    sources: std::collections::BTreeSet::new(),
                },
            )]),
            workspace: BTreeMap::new(),
            deadline: tokio::time::Instant::now(),
        };
        let output = ActionExecutor::execute(
            &evidence,
            call(CallValue::Evidence(file(sha256, 7, MediaFormat::Csv))),
        )
        .await
        .expect("csv extracts");
        assert_eq!(
            output.output,
            serde_json::json!({"format": "csv", "rows": [["a", "b"], ["1", "2"]]})
        );
        assert_eq!(
            output.provenance,
            Some(CallProvenance::Extract {
                version: EXTRACT_VERSION.to_owned()
            })
        );
        assert!(
            ActionExecutor::execute(&evidence, call(CallValue::Json(serde_json::json!(1))))
                .await
                .is_err()
        );
    }

    /// Only reachable, pinned slots with existing locations stay cited:
    /// PDF pages and regions, text lines, JSON pointers into inline JSON,
    /// and canonical DOCX pointers.
    ///
    /// # Panics
    /// Panics when a fabricated or unreachable citation survives.
    #[tokio::test]
    async fn citations_must_locate_reachable_evidence() {
        let root = tempfile::tempdir().expect("temp dir");
        let storage = Arc::new(StorageHandle::new(
            LocalSigner::new(root.path().to_path_buf()).expect("local signer"),
        ));
        let tenant = DataTenantId::new_v7();
        let mut slots = BTreeMap::new();
        for (slot, bytes, format) in [
            (
                "deck",
                super::super::extract::tests::pdf(&["one", "two"]),
                MediaFormat::Pdf,
            ),
            (
                "memo",
                super::super::extract::tests::docx(),
                MediaFormat::Docx,
            ),
            ("notes", b"alpha\nbeta\ngamma".to_vec(), MediaFormat::Text),
        ] {
            let sha256 = Sha256Hex::digest(&bytes);
            storage
                .put_object(&tenant_path::evidence(tenant, &sha256), bytes.clone())
                .await
                .expect("the evidence writes");
            slots.insert(
                SlotName::new(slot).expect("slot"),
                file(sha256, bytes.len() as u64, format),
            );
        }
        slots.insert(
            SlotName::new("facts").expect("slot"),
            InputDeclaration::Json {
                value: serde_json::json!({"rows": [{"days": 30}]}),
            },
        );
        let evidence = TenantEvidence::new(storage, tenant, slots, HostedLimits::default());
        let cite = |slot: &str, locator: Locator| Citation {
            slot: SlotName::new(slot).expect("slot"),
            locator,
        };
        let valid = vec![
            cite("deck", Locator::Page(2)),
            cite(
                "deck",
                Locator::Region {
                    page: 1,
                    x: 0.0,
                    y: 0.0,
                    width: 10.0,
                    height: 10.0,
                },
            ),
            cite("notes", Locator::Lines { start: 2, end: 3 }),
            cite("facts", Locator::JsonPointer("/rows/0/days".to_owned())),
            cite("facts", Locator::JsonPointer("/rows/0".to_owned())),
            cite("memo", Locator::JsonPointer("/blocks/0/text".to_owned())),
        ];
        let invalid = vec![
            cite("deck", Locator::Page(3)),
            cite("deck", Locator::Slide(1)),
            cite("notes", Locator::Lines { start: 3, end: 4 }),
            cite("notes", Locator::Lines { start: 0, end: 1 }),
            cite("facts", Locator::JsonPointer("/rows/1".to_owned())),
            cite("facts", Locator::JsonPointer("/ro".to_owned())),
            cite("memo", Locator::JsonPointer("/format".to_owned())),
            cite("ghost", Locator::Page(1)),
        ];
        let reachable: BTreeSet<SlotName> = ["deck", "memo", "notes", "facts", "ghost"]
            .into_iter()
            .map(|slot| SlotName::new(slot).expect("slot"))
            .collect();
        let mut all = valid.clone();
        all.extend(invalid);
        assert_eq!(evidence.cited(&reachable, all).await.expect("cited"), valid);

        let only_notes = BTreeSet::from([SlotName::new("notes").expect("slot")]);
        assert!(
            evidence
                .cited(&only_notes, vec![cite("deck", Locator::Page(1))])
                .await
                .expect("cited")
                .is_empty()
        );
    }
}
