//! Pinned original evidence read from the execution tenant's storage.
//!
//! [`TenantEvidence`] is the one byte reader for file evidence, registered
//! Artifact files, and Artifact code, and the one citation validator over an
//! execution's pinned input slots. A file is read from
//! `<tenant>/evidence/sha256/<digest>`; an Artifact file is resolved through
//! the registry's authorized, audited Card read and its server-owned
//! inventory. Either read is bounded by the expected length and the file
//! limit and returns bytes only when their length and SHA-256 match, and
//! evidence also only when its sniffed format matches, so a mismatched file
//! can never feed a pass. Extraction, workspaces, citations, and native media all read through
//! it; it is also the Agent executor's [`EvidenceReader`] and the `extract`
//! action's [`ActionExecutor`].

use std::collections::{BTreeMap, BTreeSet};

use async_trait::async_trait;
use base64::Engine as _;
use skald_spec::{MediaFormat, MediaRef, MediaSource};
use vala_verify::blocks::{
    ActionExecutor, ActionOutput, BlockCall, BlockError, CallValue, EngineError, EvidenceReader,
};
use wyrd_spec::auth::Sha256Hex;
use wyrd_spec::card::verifier::{Action, InputSlot};
use wyrd_spec::envelope::CardKind;
use wyrd_spec::ids::SlotName;
use wyrd_spec::reference::CardRef;
use wyrd_spec::verification::execution::{CallProvenance, Citation, InputDeclaration, Locator};
use wyrd_storage::error::StorageError;
use wyrd_storage::tenant_path::{self, ValidatedPath};

use super::HostedLimits;
use super::extract::{EXTRACT_VERSION, Extraction, locates, units};
use super::registry::refusal;
use crate::components::auth::Caller;
use crate::components::cards::routes::get_card_by_ref_for;
use crate::components::cards::service::list_card_artifacts;
use crate::state::AppState;

/// Reads one execution's pinned evidence from its tenant's storage.
pub struct TenantEvidence {
    /// Server state: the storage every read uses and the registry Artifact
    /// reads resolve against.
    state: AppState,
    /// The authority every Artifact read is authorized and audited under;
    /// its tenant is the evidence namespace.
    caller: Caller,
    /// The execution's pinned input declarations, the only citable sources.
    slots: BTreeMap<SlotName, InputDeclaration>,
    /// File limits in force.
    limits: HostedLimits,
}

impl TenantEvidence {
    /// A reader of `caller`'s tenant evidence whose citable sources are the
    /// execution's pinned `slots`, under `limits`.
    #[must_use]
    pub const fn new(
        state: AppState,
        caller: Caller,
        slots: BTreeMap<SlotName, InputDeclaration>,
        limits: HostedLimits,
    ) -> Self {
        Self {
            state,
            caller,
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
                &wyrd_spec::card::verifier::extract::ExtractOutput::Json {
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

    /// The verified bytes and format of file or Artifact `evidence`.
    ///
    /// A file is the tenant's pinned evidence object; an Artifact file is
    /// loaded through [`Self::artifact`] and takes its format from its
    /// stored content type. Either way the sniffed format must match.
    ///
    /// # Errors
    /// Returns [`BlockError::Terminal`] for JSON or trace evidence, an
    /// Artifact file with no supported content type, a format mismatch,
    /// and [`Self::stored`] and [`Self::artifact`] refusals;
    /// [`BlockError::Retryable`] for a transient storage or registry
    /// failure.
    pub async fn read(
        &self,
        evidence: &InputDeclaration,
    ) -> Result<(Vec<u8>, MediaFormat), BlockError> {
        let (bytes, format) = match evidence {
            InputDeclaration::File {
                sha256,
                size_bytes,
                format,
                ..
            } => {
                let path = tenant_path::evidence(self.caller.data_tenant_id, sha256);
                (self.stored(&path, *size_bytes, sha256).await?, *format)
            }
            InputDeclaration::Artifact { artifact, path } => {
                let (bytes, format) = self.artifact(artifact, path.as_deref()).await?;
                let format = format.ok_or_else(|| {
                    terminal("the Artifact file has no supported media content type")
                })?;
                (bytes, format)
            }
            InputDeclaration::Json { .. } | InputDeclaration::Trace { .. } => {
                return Err(terminal(
                    "only file and Artifact evidence have readable bytes",
                ));
            }
        };
        if !sniffs_as(&bytes, format) {
            return Err(BlockError::Terminal {
                reason: format!("the file content is not {format}"),
            });
        }
        Ok((bytes, format))
    }

    /// The verified bytes of one file of exact Artifact `card`, and the
    /// media format its stored content type names.
    ///
    /// The file is located through [`Self::artifact_file`], then its bytes
    /// are read through [`Self::stored`] against the inventory's size and
    /// digest. Code sources and evidence share this one loader.
    ///
    /// # Errors
    /// Returns [`Self::artifact_file`] and [`Self::stored`] refusals.
    pub async fn artifact(
        &self,
        card: &CardRef,
        path: Option<&str>,
    ) -> Result<(Vec<u8>, Option<MediaFormat>), BlockError> {
        let file = self.artifact_file(card, path).await?;
        let bytes = self.stored(&file.object, file.size, &file.sha256).await?;
        Ok((bytes, file.format))
    }

    /// The stored metadata of one file of exact Artifact `card`, read
    /// without its bytes.
    ///
    /// The Card is resolved through the registry's own authorized read, so
    /// the caller's `card:read` decision is checked and audited and a
    /// mismatched UID is refused. The file comes from the Card's
    /// server-authoritative inventory: `path` names it, or the Artifact must
    /// hold exactly one. Admission and [`Self::artifact`] share this lookup.
    ///
    /// # Errors
    /// Returns [`BlockError::Terminal`] for a non-Artifact reference, a
    /// refused or missing Card, a path the inventory does not hold, or a
    /// malformed inventory row, and [`BlockError::Retryable`] when the
    /// registry is unavailable.
    async fn artifact_file(
        &self,
        card: &CardRef,
        path: Option<&str>,
    ) -> Result<ArtifactFile, BlockError> {
        if card.kind != CardKind::Artifact {
            return Err(terminal("the reference does not name an Artifact Card"));
        }
        let resolved = get_card_by_ref_for(&self.state, &self.caller, card)
            .await
            .map_err(|error| refusal(&error))?;
        let uid = resolved
            .card
            .metadata
            .uid
            .ok_or_else(|| terminal("the Artifact Card has no UID"))?;
        let inventory = list_card_artifacts(&self.state, &self.caller, &uid)
            .await
            .map_err(|error| refusal(&error))?
            .artifacts;
        let entry = match (path, inventory.as_slice()) {
            (Some(path), entries) => entries
                .iter()
                .find(|entry| entry.relative_path.as_str() == path),
            (None, [only]) => Some(only),
            (None, _) => None,
        }
        .ok_or_else(|| terminal("the Artifact holds no such file; name one of its paths"))?;
        let size = u64::try_from(entry.size_bytes)
            .map_err(|_| terminal("the Artifact file has a negative stored size"))?;
        let sha256 = base64::engine::general_purpose::STANDARD
            .decode(&entry.sha256)
            .ok()
            .and_then(|digest| Sha256Hex::new(&hex::encode(digest)).ok())
            .ok_or_else(|| terminal("the Artifact file has a malformed stored digest"))?;
        let tenant = self.caller.data_tenant_id;
        let object = tenant_path::validate(
            &tenant_path::build(tenant, uid.as_str(), entry.relative_path.as_str()),
            tenant,
        )
        .map_err(|_| terminal("the Artifact file path is not a valid tenant path"))?;
        Ok(ArtifactFile {
            object,
            size,
            sha256,
            format: entry
                .content_type
                .as_deref()
                .and_then(|mime| mime.parse().ok()),
        })
    }

    /// The bytes of stored object `path`, which must be `size` bytes long
    /// and hash to `sha256`.
    ///
    /// The size is checked against the file limit and the stored object's
    /// length before the read, which is bounded by `size`; the body's
    /// length and digest are checked after it.
    ///
    /// # Errors
    /// Returns [`BlockError::Terminal`] for a file over the limit, an
    /// absent object, or a length or digest mismatch, and
    /// [`BlockError::Retryable`] for another storage failure.
    async fn stored(
        &self,
        path: &ValidatedPath,
        size: u64,
        sha256: &Sha256Hex,
    ) -> Result<Vec<u8>, BlockError> {
        if size > self.limits.file_max_bytes {
            return Err(terminal("the file exceeds the input file limit"));
        }
        let storage = &self.state.storage;
        let stored = storage
            .object_len(path)
            .await
            .map_err(|error| self.failed(&error))?;
        if stored != size {
            return Err(terminal(
                "the stored file length does not match its declaration",
            ));
        }
        let bytes = storage
            .get_object_bounded(path, size)
            .await
            .map_err(|error| self.failed(&error))?;
        if u64::try_from(bytes.len()).ok() != Some(size) {
            return Err(terminal(
                "the stored file length does not match its declaration",
            ));
        }
        if Sha256Hex::digest(&bytes) != *sha256 {
            return Err(terminal(
                "the stored file digest does not match its declaration",
            ));
        }
        Ok(bytes)
    }

    /// Classify a storage failure without exposing its key or backend text.
    fn failed(&self, error: &StorageError) -> BlockError {
        if matches!(error, StorageError::ObjectNotFound { .. }) {
            return terminal("the evidence file was never uploaded");
        }
        tracing::warn!(backend = ?self.state.storage.backend(), "verification evidence read failed");
        BlockError::Retryable {
            reason: "the evidence storage read failed".to_owned(),
        }
    }
}

/// One registered Artifact file's stored metadata, from its inventory.
struct ArtifactFile {
    /// The file's tenant storage object.
    object: ValidatedPath,
    /// Stored length in bytes.
    size: u64,
    /// Stored SHA-256.
    sha256: Sha256Hex,
    /// The media format its stored content type names, if supported.
    format: Option<MediaFormat>,
}

#[async_trait]
impl EvidenceReader for TenantEvidence {
    /// Admit the execution's original input files before any block runs.
    ///
    /// Each selected Artifact file's metadata is resolved through
    /// [`Self::artifact_file`], under the caller's audited `card:read`, and
    /// must be a format its media slot accepts and within the slot's
    /// `max_bytes`; uploaded files were checked by the engine from their
    /// declarations. Every file and selected Artifact file, used or not,
    /// must then fit the Deployment's individual input-file limit, and
    /// their declared lengths must fit its combined input-file limit
    /// together. Each original input counts once, whatever later
    /// reads, workspaces, or retries do with it; no bytes are read here.
    ///
    /// # Errors
    /// Returns [`EngineError::InputTooLarge`] past a slot, individual, or
    /// combined limit,
    /// [`EngineError::InputInvalid`] for an Artifact file that cannot be
    /// resolved or that its slot does not accept, and
    /// [`EngineError::EvidenceUnavailable`] when the registry is unavailable.
    async fn admit(
        &self,
        slots: &BTreeMap<SlotName, InputSlot>,
        evidence: &BTreeMap<SlotName, InputDeclaration>,
    ) -> Result<(), EngineError> {
        let mut total = 0_u64;
        for (name, declaration) in evidence {
            let size = match declaration {
                InputDeclaration::File { size_bytes, .. } => *size_bytes,
                InputDeclaration::Artifact { artifact, path } => {
                    let file = self
                        .artifact_file(artifact, path.as_deref())
                        .await
                        .map_err(|error| match error {
                            BlockError::Retryable { reason } => {
                                EngineError::EvidenceUnavailable(format!("input {name}: {reason}"))
                            }
                            error => EngineError::InputInvalid(format!("input {name}: {error}")),
                        })?;
                    if let Some(InputSlot::Media {
                        formats, max_bytes, ..
                    }) = slots.get(name)
                    {
                        if !file.format.is_some_and(|format| formats.contains(&format)) {
                            return Err(EngineError::InputInvalid(format!(
                                "input {name}: the slot does not accept the Artifact file's format"
                            )));
                        }
                        if let Some(max) = max_bytes.filter(|max| file.size > *max) {
                            return Err(EngineError::InputTooLarge(format!(
                                "input {name}: the Artifact file exceeds the slot's {max}-byte limit"
                            )));
                        }
                    }
                    file.size
                }
                InputDeclaration::Json { .. } | InputDeclaration::Trace { .. } => continue,
            };
            if size > self.limits.file_max_bytes {
                return Err(EngineError::InputTooLarge(format!(
                    "input {name}: the file exceeds the {}-byte input file limit",
                    self.limits.file_max_bytes
                )));
            }
            total = total
                .checked_add(size)
                .filter(|total| *total <= self.limits.files_max_bytes)
                .ok_or_else(|| {
                    EngineError::InputTooLarge(format!(
                        "the execution's input files exceed the combined {}-byte limit",
                        self.limits.files_max_bytes
                    ))
                })?;
        }
        Ok(())
    }

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
mod pg_tests {
    //! Pinned evidence reads accept only exact, matching files.
    //!
    //! The reader owns server state, so these run over the shared Postgres
    //! fixture on the process-wide runtime.

    use std::future::Future;

    use vala_verify::blocks::{CallInput, Position};
    use wyrd_spec::vala::eval::TaskId;

    use super::*;
    use crate::test_support::{test_caller, test_state, test_tenant};

    /// Drive `test` on the process-wide runtime the fixture's pool lives on.
    fn run(test: impl Future<Output = ()>) {
        wyrd_runtime::runtime().block_on(test);
    }

    /// Store `bytes` as fixture-tenant evidence and return their digest.
    ///
    /// # Panics
    /// Panics when the write fails.
    async fn put(bytes: &[u8]) -> Sha256Hex {
        let sha256 = Sha256Hex::digest(bytes);
        crate::test_support::test_storage()
            .put_object(
                &tenant_path::evidence(test_tenant(), &sha256),
                bytes.to_vec(),
            )
            .await
            .expect("the evidence writes");
        sha256
    }

    /// Evidence citing `slots` read by a caller with no grants.
    ///
    /// # Panics
    /// Panics when the shared Postgres fixture is unavailable.
    async fn reader(slots: BTreeMap<SlotName, InputDeclaration>) -> TenantEvidence {
        TenantEvidence::new(
            test_state().await,
            test_caller([]),
            slots,
            HostedLimits::default(),
        )
    }

    /// Evidence over the fixture tenant holding `bytes` under their digest.
    ///
    /// # Panics
    /// Panics when the write fails or the shared Postgres fixture is
    /// unavailable.
    async fn stored(bytes: &[u8]) -> (TenantEvidence, Sha256Hex) {
        let sha256 = put(bytes).await;
        (reader(BTreeMap::new()).await, sha256)
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
    #[test]
    fn evidence_reads_only_matching_files() {
        run(async {
            let (evidence, sha256) = stored(b"%PDF-1.5 body").await;
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
        });
    }

    /// A stored body whose digest differs from its key is refused.
    ///
    /// # Panics
    /// Panics when the tampered body reads.
    #[test]
    fn evidence_refuses_tampered_body() {
        run(async {
            let (evidence, sha256) = stored(b"hello").await;
            crate::test_support::test_storage()
                .put_object(
                    &tenant_path::evidence(test_tenant(), &sha256),
                    b"jello".to_vec(),
                )
                .await
                .expect("tamper");
            let error = evidence
                .read(&file(sha256, 5, MediaFormat::Text))
                .await
                .expect_err("digest mismatch");
            assert!(error.to_string().contains("digest"), "{error}");
        });
    }

    /// Native media is base64 with the format's MIME type; non-native
    /// formats are refused.
    ///
    /// # Panics
    /// Panics when media binding differs.
    #[test]
    fn evidence_binds_native_media_only() {
        run(async {
            let (evidence, sha256) = stored(b"%PDF-1.5").await;
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
            let (evidence, sha256) = stored(b"a,b").await;
            assert!(
                evidence
                    .media(&file(sha256, 3, MediaFormat::Csv))
                    .await
                    .is_err()
            );
        });
    }

    /// `extract` reads `from` evidence and returns the locked shape with
    /// extractor provenance; non-evidence `from` is refused.
    ///
    /// # Panics
    /// Panics when the output or provenance differs.
    #[test]
    fn extract_action_returns_locked_shape() {
        run(async {
            let (evidence, sha256) = stored(b"a,b\n1,2").await;
            let id = TaskId::new("pull").expect("task id");
            let block: Action = serde_json::from_value(
                serde_json::json!({"kind": "extract", "from": "inputs.sheet"}),
            )
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
        });
    }

    /// Only reachable, pinned slots with existing locations stay cited:
    /// PDF pages and regions, text lines, JSON pointers into inline JSON,
    /// and canonical DOCX pointers.
    ///
    /// # Panics
    /// Panics when a fabricated or unreachable citation survives.
    #[test]
    fn citations_must_locate_reachable_evidence() {
        run(async {
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
                let sha256 = put(&bytes).await;
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
            let evidence = reader(slots).await;
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
        });
    }
}
