//! Verifier inputs and executions: what `observe.verify` and `observe.submit`
//! take and return, and the [`Execution`] handle any process can reconnect to.
//!
//! The hydrated Verifier's slots decide each value's wire form: an ordinary
//! JSON value fills a `json` slot, a trace-ID string a `trace` slot, a
//! [`File`] or [`ArtifactRef`] a `media` slot, and a feature object a
//! `samples` slot. Files are hashed and uploaded only here, during the call:
//! the evidence route opens shared upload sessions for digests the tenant
//! holds no completed File upload of, and the storage client transfers and
//! completes each one exactly like an Artifact file, so file contents never
//! travel in the execution request. Run evidence executes now
//! (`verify`) or is queued (`submit`); Window samples are recorded through
//! the existing Drift observation path and judged later by the schedule.

use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;
use std::time::Duration;

use bytes::Bytes;
use reqwest::Method;
use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};
use skald_spec::MediaFormat;
use wyrd_spec::auth::Sha256Hex;
use wyrd_spec::card::verifier::{InputSlot, VerifierSpec};
use wyrd_spec::error::WyrdError;
use wyrd_spec::ids::{CardUid, GroupId, SlotName};
use wyrd_spec::reference::CardRef;
use wyrd_spec::vala::ids::{RunId, TraceId};
use wyrd_spec::verification::EXECUTION_DEADLINE;
use wyrd_spec::verification::execution::{
    EnqueueVerificationResponse, EvidenceFile, EvidencePlanRequest, EvidencePlanResponse,
    InputDeclaration, VerificationRequest, VerificationResult,
};

use crate::client::WyrdClient;

mod execution;

use crate::storage::upload::sha256_file;
use crate::storage::{FileSource, WyrdStorageClient};
pub use execution::Execution;

/// Time an execute response may take to arrive after the server's
/// [`EXECUTION_DEADLINE`] elapses: upload, audit, and the 504 answer itself.
const EXECUTE_RESPONSE_GRACE: Duration = Duration::from_secs(10);

/// A local file to verify, read only when a call needs it.
///
/// Constructing a `File` performs no IO and registers nothing: during
/// `verify` or `submit` the client stream-hashes it, asks the server which
/// digests it lacks, and uploads only those. An uploaded file is evidence
/// pinned by SHA-256, never an Artifact Card.
#[derive(Debug, Clone, PartialEq)]
pub struct File {
    /// Where the bytes come from.
    bytes: FileBytes,
    /// Declared format; inferred from the filename extension when absent.
    format: Option<MediaFormat>,
    /// Display name sent with the declaration.
    filename: Option<String>,
}

/// The two places a [`File`]'s bytes can live.
#[derive(Debug, Clone, PartialEq)]
enum FileBytes {
    /// A path opened when the call hashes and uploads it.
    Path(PathBuf),
    /// Bytes already in memory.
    Memory(Bytes),
}

impl File {
    /// A file on disk; its name supplies the filename and default format.
    ///
    /// # Arguments
    /// * `path` - The file, opened only when a call uses it.
    #[must_use]
    pub fn path(path: impl Into<PathBuf>) -> Self {
        let path = path.into();
        let filename = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned());
        Self {
            bytes: FileBytes::Path(path),
            format: None,
            filename,
        }
    }

    /// In-memory bytes; declare a format unless the slot accepts only one.
    ///
    /// # Arguments
    /// * `bytes` - The file contents.
    #[must_use]
    pub fn bytes(bytes: impl Into<Bytes>) -> Self {
        Self {
            bytes: FileBytes::Memory(bytes.into()),
            format: None,
            filename: None,
        }
    }

    /// Declare the file's format instead of inferring it.
    ///
    /// # Arguments
    /// * `format` - The canonical format the server checks when reading.
    #[must_use]
    pub fn with_format(mut self, format: MediaFormat) -> Self {
        self.format = Some(format);
        self
    }

    /// Set the display name sent with the declaration.
    ///
    /// # Arguments
    /// * `filename` - A name for display only; it selects no storage path.
    #[must_use]
    pub fn with_filename(mut self, filename: impl Into<String>) -> Self {
        self.filename = Some(filename.into());
        self
    }

    /// Hash the file, returning its length, digest, and transfer source.
    ///
    /// # Errors
    /// Returns `WYRD_VERIFICATION_400_INPUT_INVALID` when the file cannot
    /// be read.
    async fn pin(&self, slot: &SlotName) -> Result<(EvidenceFile, Upload), WyrdError> {
        let (size_bytes, digest) = match &self.bytes {
            FileBytes::Path(path) => sha256_file(path).await.map_err(|error| {
                input_invalid(
                    "a verification file cannot be read",
                    json!({ "slot": slot, "path": path, "source": error.to_string() }),
                )
            })?,
            FileBytes::Memory(bytes) => (bytes.len() as u64, Sha256::digest(bytes).into()),
        };
        let sha256 = Sha256Hex::from(digest);
        let upload = match &self.bytes {
            FileBytes::Path(path) => Upload::Path(path.clone()),
            FileBytes::Memory(bytes) => Upload::Memory(bytes.clone()),
        };
        Ok((EvidenceFile { sha256, size_bytes }, upload))
    }

    /// The declared format, else the one the filename extension or the slot's
    /// single accepted format implies.
    fn format(&self, accepted: &[MediaFormat]) -> Option<MediaFormat> {
        self.format
            .or_else(|| {
                let name = self.filename.as_deref()?;
                name.rsplit_once('.')?.1.parse().ok()
            })
            .or(match accepted {
                [only] => Some(*only),
                _ => None,
            })
    }
}

/// The transfer source of one pinned file.
#[derive(Debug, Clone)]
enum Upload {
    /// Reopened from disk for the upload.
    Path(PathBuf),
    /// Sent from memory.
    Memory(Bytes),
}

/// An existing registered Artifact's file, verified with its normal identity
/// and authorization.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtifactRef {
    /// The exact Artifact Card.
    artifact: CardRef,
    /// The file inside it; required when it holds several.
    path: Option<String>,
}

impl ArtifactRef {
    /// Select `artifact`'s only file, or `path` within it.
    ///
    /// # Arguments
    /// * `artifact` - The exact registered Artifact Card.
    /// * `path` - The file inside the Artifact, when it holds several.
    #[must_use]
    pub fn new(artifact: CardRef, path: Option<String>) -> Self {
        Self { artifact, path }
    }
}

/// One caller value for a Verifier slot; the slot's type decides its form.
#[derive(Debug, Clone, PartialEq)]
pub enum Input {
    /// JSON for a `json` slot, a trace-ID string for a `trace` slot, or a
    /// feature object (or one metric value) for a `samples` slot.
    Value(Value),
    /// A local file for a `media` slot.
    File(File),
    /// A registered Artifact's file for a `media` slot.
    Artifact(ArtifactRef),
}

impl From<Value> for Input {
    /// An ordinary JSON value.
    fn from(value: Value) -> Self {
        Self::Value(value)
    }
}

impl From<File> for Input {
    /// A local file.
    fn from(file: File) -> Self {
        Self::File(file)
    }
}

impl From<ArtifactRef> for Input {
    /// A registered Artifact's file.
    fn from(artifact: ArtifactRef) -> Self {
        Self::Artifact(artifact)
    }
}

/// The values of one call: named by slot, or one value for a single-slot
/// Verifier.
#[derive(Debug, Clone, PartialEq)]
pub enum Inputs {
    /// One value for the Verifier's only slot.
    One(Input),
    /// Values keyed by slot name.
    Named(BTreeMap<String, Input>),
}

impl Inputs {
    /// Values keyed by slot name.
    ///
    /// # Arguments
    /// * `values` - `(slot, value)` pairs; a later duplicate replaces an
    ///   earlier one.
    #[must_use]
    pub fn named<K: Into<String>, V: Into<Input>>(
        values: impl IntoIterator<Item = (K, V)>,
    ) -> Self {
        Self::Named(
            values
                .into_iter()
                .map(|(name, value)| (name.into(), value.into()))
                .collect(),
        )
    }
}

impl<T: Into<Input>> From<T> for Inputs {
    /// One value for a single-slot Verifier.
    fn from(value: T) -> Self {
        Self::One(value.into())
    }
}

/// What `submit` did with the evidence.
#[derive(Debug, Clone)]
pub enum Submission {
    /// Window samples were recorded; the Verifier's schedule judges the
    /// window later, so there is no execution to poll.
    Recorded,
    /// Run evidence was staged for one queued execution.
    Queued(Execution),
}

/// One Verifier execution target resolved from a hydrated graph.
#[derive(Debug)]
pub(crate) struct Target<'a> {
    /// The hydrated Verifier, whose slots decide each value's projection.
    pub(crate) spec: &'a VerifierSpec,
    /// The exact Verifier Card version.
    pub(crate) verifier_card_uid: CardUid,
    /// The exact subject Card version.
    pub(crate) card_uid: CardUid,
    /// The application Run the execution belongs to.
    pub(crate) run_id: RunId,
    /// The caller group the execution belongs to, when grouped.
    pub(crate) group_id: Option<GroupId>,
    /// The state's configured deployment environment, sent as the
    /// execution's correlation tag; `None` when unknown.
    pub(crate) deployment_environment: Option<String>,
}

/// The evidence one call carries, projected from the caller's values.
#[derive(Debug)]
pub(crate) enum Projected {
    /// Feature values recorded for a scheduled window, as one flat object.
    Window(Value),
    /// Evidence for one execution: inline declarations plus files to pin.
    Run(RunEvidence),
}

/// Run evidence before its files are pinned.
#[derive(Debug, Default)]
pub(crate) struct RunEvidence {
    /// Ready declarations keyed by slot.
    declarations: BTreeMap<SlotName, InputDeclaration>,
    /// Files to hash, check, plan, and upload, with their slot's limits.
    files: Vec<(SlotName, File, Vec<MediaFormat>, Option<u64>)>,
}

impl Target<'_> {
    /// Project the caller's values onto this Verifier's slots.
    ///
    /// A Verifier with a `samples` slot is a Window Verifier, since
    /// registration refuses mixing samples with other slot types. Missing
    /// required slots are left to the server, which judges them as missing
    /// evidence rather than refusing the call.
    ///
    /// # Errors
    /// Returns `WYRD_VERIFICATION_400_INPUT_INVALID` for an unknown slot
    /// name, a positional value for a Verifier without exactly one slot, or
    /// a value of the wrong kind for its slot.
    pub(crate) fn project(&self, inputs: Inputs) -> Result<Projected, WyrdError> {
        let named = match inputs {
            Inputs::Named(named) => named,
            Inputs::One(value) => {
                let mut slots = self.spec.inputs.keys();
                let (Some(slot), None) = (slots.next(), slots.next()) else {
                    return Err(input_invalid(
                        "a positional value needs a Verifier with exactly one input slot",
                        json!({ "slots": self.spec.inputs.keys().collect::<Vec<_>>() }),
                    ));
                };
                BTreeMap::from([(slot.as_str().to_owned(), value)])
            }
        };
        let window = self
            .spec
            .inputs
            .values()
            .any(|slot| matches!(slot, InputSlot::Samples { .. }));
        let mut features = Map::new();
        let mut run = RunEvidence::default();
        for (name, value) in named {
            let (slot, kind) = self
                .spec
                .inputs
                .iter()
                .find(|(slot, _)| slot.as_str() == name)
                .ok_or_else(|| {
                    input_invalid(
                        "the Verifier declares no input slot with this name",
                        json!({ "slot": name, "slots": self.spec.inputs.keys().collect::<Vec<_>>() }),
                    )
                })?;
            match (kind, value) {
                (InputSlot::Json { .. }, Input::Value(value)) => {
                    run.declarations
                        .insert(slot.clone(), InputDeclaration::Json { value });
                }
                (InputSlot::Trace { .. }, Input::Value(Value::String(text))) => {
                    let trace_id = text.parse::<TraceId>().map_err(|_| {
                        input_invalid(
                            "a trace slot takes a 32-hex-digit trace ID",
                            json!({ "slot": slot }),
                        )
                    })?;
                    run.declarations
                        .insert(slot.clone(), InputDeclaration::Trace { trace_id });
                }
                (InputSlot::Media { .. }, Input::Artifact(ArtifactRef { artifact, path })) => {
                    run.declarations
                        .insert(slot.clone(), InputDeclaration::Artifact { artifact, path });
                }
                (
                    InputSlot::Media {
                        formats, max_bytes, ..
                    },
                    Input::File(file),
                ) => run
                    .files
                    .push((slot.clone(), file, formats.clone(), *max_bytes)),
                (
                    InputSlot::Samples {
                        features: names, ..
                    },
                    Input::Value(value),
                ) => {
                    samples(slot, names, value, &mut features)?;
                }
                (kind, value) => {
                    return Err(input_invalid(
                        "the value does not fit its input slot",
                        json!({ "slot": slot, "slot_type": slot_type(kind), "value": input_kind(&value) }),
                    ));
                }
            }
        }
        Ok(if window {
            Projected::Window(Value::Object(features))
        } else {
            Projected::Run(run)
        })
    }

    /// Execute Run evidence now and return the attributable result.
    ///
    /// The request carries no `Idempotency-Key`, so an ambiguous transport
    /// failure is surfaced rather than replayed: model work is never silently
    /// repeated. The client waits [`EXECUTION_DEADLINE`] plus
    /// [`EXECUTE_RESPONSE_GRACE`] even when its configured timeout is
    /// shorter, so a slow execution ends in the server's 504.
    ///
    /// # Errors
    /// Returns the local file and slot refusals of [`RunEvidence::pin`] and
    /// the server's refusal of the evidence plan, upload, or execution.
    ///
    /// # Cancellation
    /// Dropping the future abandons the request; uploads already sent stay
    /// as deduplicated tenant evidence.
    pub(crate) async fn execute(
        &self,
        client: &WyrdClient,
        run: RunEvidence,
    ) -> Result<VerificationResult, WyrdError> {
        let request = self.request(run.pin(client).await?);
        client
            .with_min_request_timeout(EXECUTION_DEADLINE + EXECUTE_RESPONSE_GRACE)
            .http
            .request_json(Method::POST, "/v1/verification/execute", Some(&request))
            .await
    }

    /// Stage Run evidence for one queued execution.
    ///
    /// One `Idempotency-Key` is minted per call and replayed on every
    /// transport retry, so a retried request names the same execution.
    /// `202` confirms staging only: replay or conflict is decided when the
    /// queue accepts it, and a conflict surfaces through the execution's
    /// status and `wait`.
    ///
    /// # Errors
    /// As [`Target::execute`].
    ///
    /// # Cancellation
    /// Dropping the future after the request was sent may still leave a
    /// staged submission whose ID the caller never learns.
    pub(crate) async fn enqueue(
        &self,
        client: &WyrdClient,
        run: RunEvidence,
    ) -> Result<Execution, WyrdError> {
        let request = self.request(run.pin(client).await?);
        let key = format!("verification-submit-{}", uuid::Uuid::now_v7());
        let response: EnqueueVerificationResponse = client
            .http
            .submit_with_idempotency_key(Method::POST, "/v1/verification/enqueue", &request, &key)
            .await?;
        Ok(Execution::new(response.execution_id, client.clone()))
    }

    /// The wire request for this target and pinned declarations.
    fn request(&self, inputs: BTreeMap<SlotName, InputDeclaration>) -> VerificationRequest {
        VerificationRequest {
            verifier_card_uid: self.verifier_card_uid.clone(),
            card_uid: self.card_uid.clone(),
            run_id: Some(self.run_id.clone()),
            group_id: self.group_id.clone(),
            inputs,
            execution_route_overrides: BTreeMap::new(),
            deployment_environment: self.deployment_environment.clone(),
        }
    }
}

impl RunEvidence {
    /// Hash every file, check it against its slot, upload the ones the
    /// server lacks, and return the complete declarations.
    ///
    /// # Errors
    /// Returns `WYRD_VERIFICATION_400_INPUT_INVALID` for an unreadable file
    /// or one whose format is unknown or not accepted by its slot,
    /// `WYRD_VERIFICATION_413_INPUT_TOO_LARGE` for a file above its slot's
    /// `max_bytes`, and the server's or storage's refusal of the plan or
    /// upload.
    ///
    /// # Cancellation
    /// Uploads run one at a time, so a failure or cancellation leaves every
    /// earlier File completed and reusable, the failing File's session open
    /// until the storage sweeper reclaims it, and later Files unplanned.
    /// Retrying replans: completed Files are skipped and each remaining one
    /// gets a new, independent session.
    async fn pin(
        mut self,
        client: &WyrdClient,
    ) -> Result<BTreeMap<SlotName, InputDeclaration>, WyrdError> {
        if self.files.is_empty() {
            return Ok(self.declarations);
        }
        let mut sources = HashMap::new();
        let mut files = Vec::new();
        for (slot, file, formats, max_bytes) in self.files {
            let format = file.format(&formats).ok_or_else(|| {
                input_invalid(
                    "declare the file's format; it has no recognised extension",
                    json!({ "slot": slot, "accepted": formats }),
                )
            })?;
            if !formats.contains(&format) {
                return Err(input_invalid(
                    "the file's format is not accepted by its slot",
                    json!({ "slot": slot, "format": format, "accepted": formats }),
                ));
            }
            let (evidence, upload) = file.pin(&slot).await?;
            if max_bytes.is_some_and(|max| evidence.size_bytes > max) {
                return Err(WyrdError::VerificationInputTooLarge {
                    message: "the file exceeds its slot's max_bytes".to_owned(),
                    details: json!({ "slot": slot, "size_bytes": evidence.size_bytes, "max_bytes": max_bytes }),
                });
            }
            self.declarations.insert(
                slot,
                InputDeclaration::File {
                    sha256: evidence.sha256,
                    size_bytes: evidence.size_bytes,
                    format,
                    filename: file.filename,
                },
            );
            if sources.insert(evidence.sha256, upload).is_none() {
                files.push(evidence);
            }
        }
        let plan = EvidencePlanRequest { files };
        let response: EvidencePlanResponse = client
            .http
            .request_json(Method::POST, "/v1/verification/evidence", Some(&plan))
            .await?;
        let storage = WyrdStorageClient::new(client);
        for upload in response.uploads {
            let Some(source) = sources.get(&upload.sha256) else {
                continue;
            };
            let key = format!("verification-evidence-{}", upload.upload_id);
            match source {
                Upload::Path(path) => {
                    let source = FileSource::open(path).await?;
                    storage
                        .upload_artifact(&upload.upload_id, &upload.plan, source, &key)
                        .await?;
                }
                Upload::Memory(bytes) => {
                    storage
                        .upload_artifact(&upload.upload_id, &upload.plan, bytes.clone(), &key)
                        .await?;
                }
            }
        }
        Ok(self.declarations)
    }
}

/// Merge one `samples` slot's value into the window's feature object.
///
/// A slot takes a flat object of its declared measurements, or one value
/// when it declares a single measurement, such as one metric.
///
/// # Errors
/// Returns `WYRD_VERIFICATION_400_INPUT_INVALID` for a measurement the slot
/// does not declare or a bare value for a multi-measurement slot.
fn samples(
    slot: &SlotName,
    names: &[wyrd_spec::ids::FeatureName],
    value: Value,
    features: &mut Map<String, Value>,
) -> Result<(), WyrdError> {
    let object = match (value, names) {
        (Value::Object(object), _) => object,
        (value, [only]) => Map::from_iter([(only.as_str().to_owned(), value)]),
        (value, _) => {
            return Err(input_invalid(
                "a samples slot takes an object of its declared measurements",
                json!({ "slot": slot, "value": json_kind(&value) }),
            ));
        }
    };
    for (name, value) in object {
        if !names.iter().any(|declared| declared.as_str() == name) {
            return Err(input_invalid(
                "the samples slot does not declare this measurement",
                json!({ "slot": slot, "measurement": name, "declared": names }),
            ));
        }
        features.insert(name, value);
    }
    Ok(())
}

/// The authored name of a slot type, for a structured refusal.
fn slot_type(slot: &InputSlot) -> &'static str {
    match slot {
        InputSlot::Json { .. } => "json",
        InputSlot::Media { .. } => "media",
        InputSlot::Trace { .. } => "trace",
        InputSlot::Samples { .. } => "samples",
        InputSlot::Source(_) => "source",
    }
}

/// Name a JSON value's type for a structured refusal.
fn json_kind(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

/// Name a caller input's kind for a structured refusal.
fn input_kind(input: &Input) -> &'static str {
    match input {
        Input::Value(value) => json_kind(value),
        Input::File(_) => "file",
        Input::Artifact(_) => "artifact",
    }
}

/// The refusal for a value that cannot fill its slot.
pub(crate) fn input_invalid(message: &str, details: Value) -> WyrdError {
    WyrdError::VerificationInputInvalid {
        message: message.to_owned(),
        details,
    }
}
