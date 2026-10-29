//! Pinned original evidence read from the execution tenant's storage.
//!
//! [`TenantEvidence`] is the one byte reader for file evidence, registered
//! Artifact files, and Artifact code, and the one citation validator over an
//! execution's pinned input slots. A file is read from the location its
//! completed File upload recorded in the object inventory; an Artifact file
//! is resolved through the registry's authorized, audited Card read and the
//! objects its hydration read from the same inventory. Each File's location
//! and each Artifact's files are resolved once per execution, at admission,
//! and reused by every later selection, read, and workspace; each Artifact
//! use still makes its own audited `card:read` decision. Either read is
//! bounded by the expected length and the file
//! limit and returns bytes only when their length and SHA-256 match, and
//! evidence also only when its sniffed format matches, so a mismatched file
//! can never feed a pass. Extraction, workspaces, citations, and native media all read through
//! it; it is also the Agent executor's [`EvidenceReader`] and the `extract`
//! action's [`ActionExecutor`].

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::{Arc, Mutex, PoisonError};

use async_trait::async_trait;
use base64::Engine as _;
use skald_spec::{MediaFormat, MediaRef, MediaSource};
use vala_verify::blocks::{
    ActionExecutor, ActionOutput, BlockCall, BlockError, CallValue, EngineError, EvidenceReader,
};
use wyrd_loader::{EXTRACT_VERSION, Extraction, lines, locates, units};
use wyrd_spec::auth::Sha256Hex;
use wyrd_spec::card::verifier::{Action, InputSlot};
use wyrd_spec::envelope::CardKind;
use wyrd_spec::ids::{CardUid, SlotName};
use wyrd_spec::reference::{CardRef, CardRefIdentity};
use wyrd_spec::registry::StoredArtifactEntry;
use wyrd_spec::verification::execution::{CallProvenance, Citation, InputDeclaration, Locator};
use wyrd_storage::error::StorageError;
use wyrd_storage::tenant_path::{self, ValidatedPath};

use super::registry::refusal;
use super::{HostedLimits, extract_refused};
use crate::components::auth::Caller;
use crate::components::cards::routes::{authorize_card_ref, get_card_objects_by_ref_for};
use crate::components::storage::inventory::ObjectInventory;
use crate::state::AppState;

/// Reads one execution's pinned evidence from its tenant's storage.
pub struct TenantEvidence {
    /// Server state: the storage every read uses and the registry Artifact
    /// reads resolve against.
    state: AppState,
    /// The authority every Artifact read is authorized and audited under;
    /// its tenant is the evidence namespace.
    caller: Caller,
    /// The execution's pinned input declarations, the only citable sources;
    /// [`EvidenceReader::admit`] adds the engine's fixed attachment slots.
    slots: Mutex<BTreeMap<SlotName, InputDeclaration>>,
    /// Each File's recorded location by declared digest and size, resolved
    /// once for this execution.
    files: Mutex<HashMap<(Sha256Hex, u64), ValidatedPath>>,
    /// Each exact Artifact's UID and files, resolved once for this execution.
    artifacts: Mutex<BTreeMap<(CardRefIdentity, Option<CardUid>), Arc<ArtifactInventory>>>,
    /// File limits in force.
    limits: HostedLimits,
}

impl TenantEvidence {
    /// A reader of `caller`'s tenant evidence whose citable sources are the
    /// execution's pinned `slots`, under `limits`.
    #[must_use]
    pub fn new(
        state: AppState,
        caller: Caller,
        slots: BTreeMap<SlotName, InputDeclaration>,
        limits: HostedLimits,
    ) -> Self {
        Self {
            state,
            caller,
            slots: Mutex::new(slots),
            files: Mutex::default(),
            artifacts: Mutex::default(),
            limits,
        }
    }

    /// The located units of pinned slot `declaration`, or `None` when it
    /// has none: inline JSON leaves, extracted file units, for a native
    /// image the single page a region may cite, and for UTF-8 bytes their
    /// actual lines, the lines `fs.read` and `fs.search` serve. Artifact
    /// files are read raw, so a file with no supported content type is still
    /// citable by line.
    ///
    /// # Errors
    /// Returns [`BlockError::Retryable`] when the evidence cannot be read
    /// now; a terminal read or extraction failure yields `None`.
    async fn located(
        &self,
        declaration: &InputDeclaration,
    ) -> Result<Option<Vec<(Locator, String)>>, BlockError> {
        if let InputDeclaration::Json { value } = declaration {
            let mut found = units(
                &wyrd_spec::card::verifier::extract::ExtractOutput::Json {
                    value: value.clone(),
                },
                b"",
            );
            found.extend(lines(
                &serde_json::to_string_pretty(value).unwrap_or_default(),
            ));
            return Ok(Some(found));
        }
        let read = match declaration {
            InputDeclaration::Artifact { artifact, path } => {
                self.artifact(artifact, path.as_deref()).await
            }
            declaration => self
                .read(declaration)
                .await
                .map(|(bytes, format)| (bytes, Some(format))),
        };
        let (bytes, format) = match read {
            Ok(read) => read,
            Err(BlockError::Terminal { .. }) => return Ok(None),
            Err(error) => return Err(error),
        };
        if format.and_then(MediaFormat::native_kind) == Some(skald_spec::MediaKind::Image) {
            return Ok(Some(vec![(Locator::Page(1), String::new())]));
        }
        let limits = self.limits.extract();
        let found = tokio::task::spawn_blocking(move || {
            let mut found = format
                .filter(|format| format.is_extractable())
                .and_then(|format| Extraction::new(&limits).run(&bytes, format).ok())
                .map(|output| units(&output, &bytes))
                .unwrap_or_default();
            if let Ok(text) = std::str::from_utf8(&bytes) {
                found.extend(lines(text));
            }
            found
        })
        .await
        .map_err(|error| BlockError::Retryable {
            reason: format!("citation extraction stopped: {error}"),
        })?;
        Ok((!found.is_empty()).then_some(found))
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
                let path = self.file_object(sha256, *size_bytes).await?;
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

    /// The recorded location of the completed File upload of `sha256` and
    /// `size`.
    ///
    /// The location comes only from the tenant's object inventory, read once
    /// per execution as the caller's SYSTEM executor, so the read adds no
    /// registry lookup; a File with no visible completed record is not
    /// ready, never a path guess.
    ///
    /// # Errors
    /// Returns [`BlockError::Terminal`] for a file over the input file
    /// limit, which can never be uploaded, and [`BlockError::Retryable`] when
    /// the inventory holds no such completed File upload yet or cannot be
    /// read now.
    async fn file_object(
        &self,
        sha256: &Sha256Hex,
        size: u64,
    ) -> Result<ValidatedPath, BlockError> {
        if size > self.limits.file_max_bytes {
            return Err(terminal("the file exceeds the input file limit"));
        }
        let key = (*sha256, size);
        if let Some(path) = lock(&self.files).get(&key) {
            return Ok(path.clone());
        }
        match ObjectInventory::new(&self.state)
            .executed_by(self.caller.executed_by)
            .file_object(self.caller.data_tenant_id, sha256, size)
            .await
        {
            Ok(Some(path)) => {
                lock(&self.files).insert(key, path.clone());
                Ok(path)
            }
            Ok(None) => Err(BlockError::Retryable {
                reason: "the evidence file has no completed upload record yet".to_owned(),
            }),
            Err(error) => {
                tracing::warn!(code = error.code(), "the evidence inventory cannot be read");
                Err(BlockError::Retryable {
                    reason: "the evidence inventory cannot be read now".to_owned(),
                })
            }
        }
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
    /// The Card is resolved through [`Self::inventory`], so the caller's
    /// `card:read` decision is checked and audited and a mismatched UID is
    /// refused. The file comes from the Card's server-authoritative
    /// inventory: `path` names it, or the Artifact must hold exactly one.
    /// Admission and [`Self::artifact`] share this lookup.
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
        let inventory = self.inventory(card).await?;
        let entry = match path {
            Some(path) => inventory.files.get(path),
            None if inventory.files.len() == 1 => inventory.files.values().next(),
            None => None,
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
            &tenant_path::build(tenant, inventory.uid.as_str(), entry.relative_path.as_str()),
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

    /// The UID and server-authoritative files of exact Artifact `card`.
    ///
    /// Every use makes the caller's own audited `card:read` decision. The
    /// first resolves the Card through the registry's authorized read, so a
    /// mismatched UID is refused, and keeps the files its hydration read
    /// from the object inventory; later uses in this execution reuse them.
    ///
    /// # Errors
    /// Returns [`BlockError::Terminal`] for a non-Artifact reference or a
    /// refused or missing Card, and [`BlockError::Retryable`] when the
    /// registry or inventory is unavailable.
    async fn inventory(&self, card: &CardRef) -> Result<Arc<ArtifactInventory>, BlockError> {
        if card.kind != CardKind::Artifact {
            return Err(terminal("the reference does not name an Artifact Card"));
        }
        let key = (card.identity_key(), card.uid.clone());
        let known = lock(&self.artifacts).get(&key).cloned();
        if let Some(inventory) = known {
            authorize_card_ref(&self.state, &self.caller, card).map_err(|error| refusal(&error))?;
            return Ok(inventory);
        }
        let (resolved, objects) = get_card_objects_by_ref_for(&self.state, &self.caller, card)
            .await
            .map_err(|error| refusal(&error))?;
        let uid = resolved
            .card
            .metadata
            .uid
            .ok_or_else(|| terminal("the Artifact Card has no UID"))?;
        let inventory = Arc::new(ArtifactInventory {
            uid,
            files: objects.files,
        });
        lock(&self.artifacts).insert(key, Arc::clone(&inventory));
        Ok(inventory)
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

/// One exact Artifact's resolved identity and completed files.
struct ArtifactInventory {
    /// The Card's UID, the namespace of its stored files.
    uid: CardUid,
    /// Completed files by Card-relative path.
    files: BTreeMap<String, StoredArtifactEntry>,
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
    /// The inventory paths of exact Artifact `card` that `path` selects, in
    /// path order, under the caller's audited `card:read`.
    ///
    /// # Errors
    /// Returns [`Self::inventory`]'s errors and [`BlockError::Terminal`] for
    /// a selection matching no file.
    async fn select(&self, card: &CardRef, path: Option<&str>) -> Result<Vec<String>, BlockError> {
        let inventory = self.inventory(card).await?;
        let selected: Vec<String> = inventory
            .files
            .keys()
            .filter(|file| {
                path.is_none_or(|path| {
                    let dir = path.trim_end_matches('/');
                    *file == path
                        || file
                            .strip_prefix(dir)
                            .is_some_and(|rest| rest.starts_with('/'))
                })
            })
            .cloned()
            .collect();
        if selected.is_empty() {
            return Err(terminal(
                "the Artifact holds no file at the selected path; name one of its files or directories",
            ));
        }
        Ok(selected)
    }

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
    /// reads, workspaces, or retries do with it. Each uploaded file's
    /// recorded location is then resolved through [`Self::file_object`];
    /// no bytes are read here. Admitted declarations the execution did not
    /// pin, the engine's fixed attachments, become citable slots.
    ///
    /// # Errors
    /// Returns [`EngineError::InputTooLarge`] past a slot, individual, or
    /// combined limit,
    /// [`EngineError::InputInvalid`] for an Artifact file that cannot be
    /// resolved or that its slot does not accept, and
    /// [`EngineError::EvidenceUnavailable`] when the registry or inventory
    /// is unavailable or an uploaded file has no visible completed record
    /// yet.
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
                        .map_err(|error| admission(name, error))?;
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
        for (name, declaration) in evidence {
            if let InputDeclaration::File {
                sha256, size_bytes, ..
            } = declaration
            {
                self.file_object(sha256, *size_bytes)
                    .await
                    .map_err(|error| admission(name, error))?;
            }
        }
        let mut pinned = lock(&self.slots);
        for (name, declaration) in evidence {
            pinned
                .entry(name.clone())
                .or_insert_with(|| declaration.clone());
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
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .get(&citation.slot)
                .filter(|_| reachable.contains(&citation.slot))
                .cloned()
            else {
                continue;
            };
            if !located.contains_key(&citation.slot) {
                let units = self.located(&declaration).await?;
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
        let limits = self.limits.extract();
        let output =
            tokio::task::spawn_blocking(move || Extraction::new(&limits).run(&bytes, format))
                .await
                .map_err(|error| BlockError::Retryable {
                    reason: format!("extraction stopped: {error}"),
                })?
                .map_err(extract_refused)?;
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

/// The admission refusal of input `name` whose metadata resolution failed
/// with `error`: retryable readiness stays retryable, anything else is
/// invalid input.
fn admission(name: &SlotName, error: BlockError) -> EngineError {
    match error {
        BlockError::Retryable { reason } => {
            EngineError::EvidenceUnavailable(format!("input {name}: {reason}"))
        }
        error => EngineError::InputInvalid(format!("input {name}: {error}")),
    }
}

/// Lock `memo`, recovering a poisoned lock: every value in it is complete.
fn lock<T>(memo: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    memo.lock().unwrap_or_else(PoisonError::into_inner)
}

/// A terminal error with fixed `reason`.
fn terminal(reason: &str) -> BlockError {
    BlockError::Terminal {
        reason: reason.to_owned(),
    }
}
