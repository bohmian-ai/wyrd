//! Bounded per-call scratch workspaces for Agent calls and code.
//!
//! [`Workspaces`] has no workspace count. Each workspace charges the
//! process scratch ledger for the bytes it holds on disk plus the one
//! `/work` write being admitted, and process memory only for the bytes in
//! flight while it stages; a charge that does not fit refuses the open busy
//! rather than queueing it, and a write charge that does not fit refuses
//! that write. [`Workspaces::open`] creates a fresh directory below the
//! server's verification scratch root and stages each declared file into it
//! through [`TenantEvidence::stage`], which streams the object from storage
//! straight to disk and verifies it there, so evidence never passes through
//! process memory whole. A verified file is then renamed into `inputs/`;
//! uploaded evidence declared as an archive is unpacked from its staged file
//! after [`zip_guard`] or the equivalent tar.gz checks, each entry streamed
//! to its own file, and the staged archive removed. A JSON input is encoded
//! in process memory, charged there until the placement pass writes it
//! straight to its `inputs/` place, and `work/` starts empty. Artifact files are staged
//! unchanged. No file is parsed when mounted: `fs.read` and `fs.search`
//! serve the actual UTF-8 lines of a file, and only `fs.extract` runs the
//! extractor, on request, into a new `/work` file. Every `/inputs` file keeps
//! the original slots it came from, so tool output stays citable against
//! original evidence. A `steps.<id>.publications` reference mounts what the
//! execution's step published, staged from its completed Files exactly as
//! evidence is: a named member at its destination, or the whole collection
//! as `manifest.json` plus `content/` beneath it; derived files keep only
//! their producer's original slots, so a generated opinion can never be
//! cited as original evidence. Dropping the [`Workspace`] empties or removes the
//! directory and then returns its scratch charge, on success, failure, or
//! cancellation alike; an opened workspace is emptied off the async worker
//! and its directory reused by a later open, so steady-state opens create
//! and remove no directory tree.

use std::collections::{BTreeMap, BTreeSet};
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

use monty_pool::{MountSpec, MountSpecMode};
use serde_json::{Value, json};
use skald_spec::MediaFormat;
use skald_tool::ToolError;
use vala_verify::blocks::{BlockError, CallInput, CallValue, tool_error};
use wyrd_loader::{EXTRACT_VERSION, Extraction, lines, zip_guard};
use wyrd_runtime::resources::{ByteCharge, ByteLedger, ProcessResources};
use wyrd_spec::error::WyrdError;
use wyrd_spec::ids::SlotName;
use wyrd_spec::verification::execution::{
    CollectionEntry, InputDeclaration, Producer, PublicationContent,
};

use super::evidence::TenantEvidence;
use super::tools::{host_stop, invocation};
use super::{HostedLimits, extract_refused};
use wyrd_spec::verification::state::AcceptedPublication;

/// Process-wide workspace admission over the process memory and scratch
/// ledgers.
pub struct Workspaces {
    /// Process memory, charged for staged bytes in flight.
    memory: ByteLedger,
    /// Where workspaces are written, charged to `scratch`.
    scratch: ScratchRoot,
    /// Limits every workspace enforces.
    limits: HostedLimits,
    /// Emptied workspace directories waiting for their next invocation.
    free: FreeRoots,
}

/// Emptied workspace roots, each holding empty `inputs/`, `work/` and
/// `staging/`, that the next [`Workspaces::open`] takes instead of making
/// a directory tree. A root enters only after a blocking-pool thread
/// emptied it, so no file from one invocation is ever seen by another.
///
/// ponytail: unbounded, but it never holds more roots than workspaces were
/// open at once, each an empty directory; trim it if that peak matters.
type FreeRoots = Arc<std::sync::Mutex<Vec<PathBuf>>>;

/// A directory for workspaces and the ledger of the filesystem it is on.
#[derive(Debug, Clone)]
pub struct ScratchRoot {
    /// Parent of every workspace directory.
    pub dir: PathBuf,
    /// The process scratch ledger of `dir`'s filesystem.
    pub ledger: ByteLedger,
}

impl Workspaces {
    /// Workspaces under `root`, charged to `process` memory and scratch.
    #[must_use]
    pub fn new(limits: HostedLimits, process: &ProcessResources, root: PathBuf) -> Self {
        Self {
            memory: process.memory().clone(),
            scratch: ScratchRoot {
                dir: root,
                ledger: process.scratch().clone(),
            },
            limits,
            free: FreeRoots::default(),
        }
    }

    /// Materialize `files`, keyed by `/inputs/...` path, for one invocation.
    ///
    /// Admission never waits: bytes that process memory or scratch cannot
    /// hold refuse the open at once, so no waiter queue can grow toward
    /// execution deadlines and the execution is deferred instead. Each file
    /// is charged to scratch for its declared size before it is fetched,
    /// then streamed into the workspace's private staging directory and
    /// verified there; a JSON input is encoded and charged to process memory
    /// instead. Only once every file is staged or encoded are they placed
    /// under `/inputs` on the blocking pool. An Artifact file keeps its stored
    /// content type only as a hint for `fs.extract`; it is never unpacked.
    ///
    /// # Errors
    /// Returns [`BlockError::Busy`] when process memory or scratch cannot
    /// hold the inputs, the evidence reader's errors, [`BlockError::Terminal`]
    /// for an invalid path, combined input bytes over the limit, or any
    /// materialization refusal, and [`BlockError::Retryable`] for a local IO
    /// failure or a failed blocking task.
    ///
    /// Profiled as `debug` spans inside the engine phase:
    /// `verification.workspace.open` for the whole open, a
    /// `verification.workspace.stage` per streamed file, and
    /// `verification.workspace.install` for the blocking placement pass.
    #[tracing::instrument(
        level = "debug",
        name = "verification.workspace.open",
        skip_all,
        fields(inputs = files.len())
    )]
    pub async fn open(
        &self,
        files: &BTreeMap<String, CallInput>,
        evidence: &TenantEvidence,
    ) -> Result<Workspace, BlockError> {
        let limits = self.limits;
        // Created inline, a few directory creations, so a refusal below
        // answers before the open first awaits.
        let reused = self
            .free
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .pop();
        let mut workspace = match reused {
            Some(root) => Workspace::empty(root, limits, Some(self.scratch.clone())),
            None => Workspace::create(limits, Some(self.scratch.clone()))?,
        };
        workspace.memory = Some(self.memory.clone());
        let mut pending = Vec::with_capacity(files.len());
        let mut directories = Vec::new();
        let mut total = 0_u64;
        let mut count = |size: u64| {
            total = total.saturating_add(size);
            if total > limits.files_max_bytes {
                return Err(terminal(format!(
                    "workspace inputs exceed the {}-byte combined file limit",
                    limits.files_max_bytes
                )));
            }
            Ok(())
        };
        for (path, input) in files {
            let sources = &input.sources;
            match &input.value {
                CallValue::Json(value) => {
                    pending.push(self.stage_json(
                        &mut workspace,
                        &mut count,
                        path,
                        value,
                        sources,
                    )?);
                }
                CallValue::Evidence(declaration) => {
                    let file = evidence.file(declaration).await?;
                    // Uploaded evidence must sniff as its declared format;
                    // an Artifact file is staged unchanged, never unpacked.
                    let (sniff, format) = match declaration {
                        InputDeclaration::Artifact { .. } => {
                            (None, file.format.filter(|format| !format.is_archive()))
                        }
                        _ => (file.format, file.format),
                    };
                    count(file.size)?;
                    workspace.reserve(file.size)?;
                    let staged = workspace.staged(pending.len());
                    evidence.stage(&file, sniff, &staged).await?;
                    pending.push(Pending {
                        path: path.clone(),
                        body: Body::Staged(staged),
                        size: file.size,
                        format,
                        sources: sources.clone(),
                    });
                }
                CallValue::Publication { producer, name } => {
                    let read = evidence.publications(&producer.step_id);
                    let publication = read
                        .iter()
                        .find(|stored| {
                            &stored.publication.producer == producer
                                && &stored.publication.name == name
                        })
                        .ok_or_else(|| {
                            terminal(format!(
                                "{path}: the successful attempt of step `{}` published no `{}`",
                                producer.step_id.as_str(),
                                name.as_str()
                            ))
                        })?;
                    self.stage_publication(
                        &mut workspace,
                        &mut count,
                        evidence,
                        (path, publication, sources),
                        (&mut pending, &mut directories),
                    )
                    .await?;
                }
                CallValue::Publications { step, collection } => {
                    let mut manifest = collection.clone();
                    // A withheld speculative contribution mounts no content:
                    // only its status, disposition, and attempts.
                    let admitted = if collection.speculation.admits() {
                        evidence.publications(step)
                    } else {
                        Vec::new()
                    };
                    for publication in admitted {
                        let relative = format!(
                            "content/{}/{}",
                            segment(&publication.publication.producer),
                            publication.publication.name.as_str()
                        );
                        self.stage_publication(
                            &mut workspace,
                            &mut count,
                            evidence,
                            (&format!("{path}/{relative}"), &publication, sources),
                            (&mut pending, &mut directories),
                        )
                        .await?;
                        let publication = publication.publication;
                        manifest.entries.push(CollectionEntry {
                            kind: publication.content.kind(),
                            name: publication.name,
                            path: relative,
                            item_index: publication.producer.item_index,
                            iteration_index: publication.producer.iteration_index,
                            attempt: publication.producer.attempt,
                        });
                    }
                    let manifest = serde_json::to_value(&manifest).unwrap_or(Value::Null);
                    pending.push(self.stage_json(
                        &mut workspace,
                        &mut count,
                        &format!("{path}/manifest.json"),
                        &manifest,
                        &BTreeSet::new(),
                    )?);
                    directories.push(path.clone());
                }
            }
        }
        let install = tracing::debug_span!(
            "verification.workspace.install",
            inputs = pending.len(),
            bytes = workspace.inputs_bytes
        );
        let mut workspace = blocking(move || {
            let _install = install.entered();
            for file in pending {
                workspace.install(file)?;
            }
            for directory in &directories {
                let host = workspace.host(directory).ok_or_else(|| {
                    terminal(format!("{directory} is not under /inputs or /work"))
                })?;
                std::fs::create_dir_all(host).map_err(io)?;
            }
            Ok(workspace)
        })
        .await?;
        // Only an opened workspace is recycled; a refused or cancelled open
        // removes its partial directory and returns its charge at once.
        workspace.free = Some(Arc::clone(&self.free));
        Ok(workspace)
    }

    /// Encode `value` as a UTF-8 JSON file for `/inputs` path `path`,
    /// charging the encoded bytes to process memory until the placement pass
    /// writes them and to the workspace's scratch for as long as they stay.
    ///
    /// # Errors
    /// Returns [`BlockError::Busy`] when memory or scratch cannot hold the
    /// file, and `count`'s combined-limit refusal.
    fn stage_json(
        &self,
        workspace: &mut Workspace,
        count: &mut impl FnMut(u64) -> Result<(), BlockError>,
        path: &str,
        value: &Value,
        sources: &BTreeSet<SlotName>,
    ) -> Result<Pending, BlockError> {
        // The encoding is counted and admitted before it is built, so a
        // refusal never allocates it.
        let size = crate::verification::encoded_len(value, true);
        let memory = self.memory.try_charge(size).map_err(|_| BlockError::Busy {
            reason: "process memory cannot hold the workspace inputs now".to_owned(),
        })?;
        count(size)?;
        workspace.reserve(size)?;
        let bytes = serde_json::to_vec_pretty(value).unwrap_or_default();
        Ok(Pending {
            path: path.to_owned(),
            body: Body::Encoded {
                bytes,
                _memory: memory,
            },
            size,
            format: Some(MediaFormat::Json),
            sources: sources.clone(),
        })
    }

    /// Stage accepted publication `stored` at `/inputs` path `path`: a file
    /// there, a JSON file there, or a directory there with its files and
    /// empty directories beneath it. Each file streams from its completed
    /// File descriptor into staging exactly as evidence does, charged to
    /// scratch for its manifest size before it is fetched, so a publication
    /// never passes through process memory whole.
    ///
    /// # Errors
    /// Returns [`Self::stage_json`]'s and [`TenantEvidence::stage`]'s
    /// errors, and [`TenantEvidence::published`]'s descriptor refusal.
    async fn stage_publication(
        &self,
        workspace: &mut Workspace,
        count: &mut impl FnMut(u64) -> Result<(), BlockError>,
        evidence: &TenantEvidence,
        (path, stored, sources): (&str, &AcceptedPublication, &BTreeSet<SlotName>),
        (pending, directories): (&mut Vec<Pending>, &mut Vec<String>),
    ) -> Result<(), BlockError> {
        let manifest = match &stored.publication.content {
            PublicationContent::Json { value } => {
                pending.push(self.stage_json(workspace, count, path, value, sources)?);
                return Ok(());
            }
            PublicationContent::File { manifest } => manifest,
            PublicationContent::Directory { manifest } => {
                directories.push(path.to_owned());
                directories.extend(
                    manifest
                        .directories
                        .iter()
                        .map(|directory| format!("{path}/{directory}")),
                );
                manifest
            }
        };
        let directory = matches!(
            stored.publication.content,
            PublicationContent::Directory { .. }
        );
        for (file, object) in manifest.files.iter().zip(&stored.objects) {
            let stored = evidence.published(file, object)?;
            count(stored.size)?;
            workspace.reserve(stored.size)?;
            let staged = workspace.staged(pending.len());
            evidence.stage(&stored, None, &staged).await?;
            pending.push(Pending {
                path: if directory {
                    format!("{path}/{}", file.path)
                } else {
                    path.to_owned()
                },
                body: Body::Staged(staged),
                size: stored.size,
                format: None,
                sources: sources.clone(),
            });
        }
        Ok(())
    }
}

/// The opaque collection segment of `producer`'s invocation attempt.
fn segment(producer: &Producer) -> String {
    let item = producer
        .item_index
        .map(|item| format!("item-{item}."))
        .unwrap_or_default();
    let iteration = producer
        .iteration_index
        .map(|iteration| format!("iteration-{iteration}."))
        .unwrap_or_default();
    format!("{item}{iteration}attempt-{}", producer.attempt)
}

/// Run materialization `work` on the blocking pool. The closure owns
/// whatever workspace it touches, so cancelling the wait still removes it
/// once the work ends.
///
/// # Errors
/// Returns `work`'s error, or [`BlockError::Retryable`] when the blocking
/// task fails.
async fn blocking<T: Send + 'static>(
    work: impl FnOnce() -> Result<T, BlockError> + Send + 'static,
) -> Result<T, BlockError> {
    tokio::task::spawn_blocking(work)
        .await
        .map_err(|error| BlockError::Retryable {
            reason: format!("workspace materialization stopped: {error}"),
        })?
}

/// The busy host stop for an `fs.extract` that process memory cannot hold.
fn memory_busy() -> ToolError {
    host_stop(BlockError::Busy {
        reason: "process memory cannot hold the extraction now".to_owned(),
    })
}

/// Why a `/work` write was refused before it ran.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum WriteRefusal {
    /// The write would pass the workspace's `/work` or scratch limit.
    #[error("the /work write limit is reached")]
    Limit,
    /// Process scratch, shared with every workspace and spill, is full.
    #[error("process scratch has no room for the /work write now")]
    Scratch,
}

/// One file to place under `/inputs`.
pub struct Staged {
    /// Authored `/inputs/...` path.
    pub path: String,
    /// Verified bytes.
    pub bytes: Vec<u8>,
    /// Format, or `None` when unknown.
    pub format: Option<MediaFormat>,
    /// Original slots behind the file.
    pub sources: BTreeSet<SlotName>,
}

/// One verified file, staged or encoded, waiting for its `/inputs` place.
struct Pending {
    /// Authored `/inputs/...` path.
    path: String,
    /// Where its bytes wait.
    body: Body,
    /// Its length, already charged to scratch.
    size: u64,
    /// Format, or `None` when unknown.
    format: Option<MediaFormat>,
    /// Original slots behind the file.
    sources: BTreeSet<SlotName>,
}

/// Where a [`Pending`] file's bytes wait for placement.
enum Body {
    /// Streamed and verified at this host path in the staging directory.
    Staged(PathBuf),
    /// Encoded JSON, written straight to its `/inputs` place, so it takes
    /// no staging file and no rename.
    Encoded {
        /// The encoded file.
        bytes: Vec<u8>,
        /// Its process memory charge, returned once the bytes are dropped.
        _memory: ByteCharge,
    },
}

impl Drop for Workspace {
    /// Empties or removes the directory before the scratch charge returns,
    /// so the charge never outlives the bytes. A workspace from
    /// [`Workspaces::open`] is emptied on a blocking-pool thread and its
    /// root goes back to the free list before the charge returns, so a
    /// caller woken by that return can reopen it; one that cannot be emptied
    /// is removed instead. Any other workspace, such as a refused or cancelled
    /// open's partial one, is removed here.
    ///
    /// The charge returns only once the directory is confirmed emptied or
    /// removed. When neither succeeds, or the cleanup is dropped unrun as a
    /// runtime shuts down, the bytes may still be on disk, so the whole
    /// charge stays promised for the rest of the process, as a surviving
    /// spill file's does; boot clears the scratch root.
    fn drop(&mut self) {
        let root = std::mem::take(&mut self.root);
        let scratch = Custody {
            charge: self.scratch.take().map(|charge| {
                charge
                    .into_inner()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
            }),
            root: root.clone(),
        };
        let Some(free) = self.free.take() else {
            if remove(&root) {
                scratch.release();
            }
            return;
        };
        let recycle = move || match empty_root(&root) {
            Ok(()) => {
                free.lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .push(root);
                scratch.release();
            }
            Err(error) => {
                tracing::warn!(root = %root.display(), %error, "workspace could not be emptied for reuse");
                if remove(&root) {
                    scratch.release();
                }
            }
        };
        match tokio::runtime::Handle::try_current() {
            Ok(runtime) => drop(runtime.spawn_blocking(recycle)),
            Err(_) => recycle(),
        }
    }
}

/// A workspace's scratch charge while its directory is being cleaned up.
///
/// [`Self::release`] returns it once the bytes are confirmed gone; dropped
/// any other way, because cleanup failed or never ran, it keeps the charge
/// promised for the rest of the process.
struct Custody {
    /// The workspace's whole scratch charge, or `None` when uncharged.
    charge: Option<ByteCharge>,
    /// The root whose bytes the charge covers, for the log.
    root: PathBuf,
}

impl Custody {
    /// Return the charge: the directory no longer holds its bytes.
    fn release(mut self) {
        drop(self.charge.take());
    }
}

impl Drop for Custody {
    /// Keep an unreleased charge for the rest of the process.
    fn drop(&mut self) {
        if let Some(charge) = self.charge.take() {
            let error = std::io::Error::other("the workspace cleanup did not confirm deletion");
            retain(charge, &self.root, &error);
        }
    }
}

/// Keep `charge` promised for the rest of the process for `path`, which
/// survived deletion with `error`, so its bytes are never offered as free.
fn retain(charge: ByteCharge, path: &Path, error: &std::io::Error) {
    tracing::error!(
        path = %path.display(),
        retained_bytes = charge.bytes(),
        %error,
        "workspace scratch survived deletion; its bytes stay charged"
    );
    std::mem::forget(charge);
}

/// Removes a workspace root, logging a failure; whether it is gone.
fn remove(root: &Path) -> bool {
    match std::fs::remove_dir_all(root) {
        Ok(()) => true,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => true,
        Err(error) => {
            tracing::warn!(root = %root.display(), %error, "workspace removal failed");
            false
        }
    }
}

/// Empties `root`'s `inputs/`, `work/` and `staging/`, keeping the three
/// directories, without following any link an invocation left.
///
/// # Errors
/// Returns the IO error when one of the three is missing or not a
/// directory, or an entry cannot be removed.
fn empty_root(root: &Path) -> std::io::Result<()> {
    for dir in WORKSPACE_DIRS {
        let dir = root.join(dir);
        if !std::fs::symlink_metadata(&dir)?.is_dir() {
            return Err(std::io::Error::other(format!(
                "{} is not a directory",
                dir.display()
            )));
        }
        for entry in std::fs::read_dir(&dir)? {
            let entry = entry?;
            if entry.file_type()?.is_dir() {
                std::fs::remove_dir_all(entry.path())?;
            } else {
                std::fs::remove_file(entry.path())?;
            }
        }
    }
    Ok(())
}

/// The directories every workspace root holds.
const WORKSPACE_DIRS: [&str; 3] = ["inputs", "work", "staging"];

/// One materialized `/inputs` file.
struct Input {
    /// Original slots behind the file.
    sources: BTreeSet<SlotName>,
    /// The declared or stored format, the format `fs.extract` reads it as.
    format: Option<MediaFormat>,
}

/// One invocation's scratch directory.
pub struct Workspace {
    /// Root holding `inputs/`, `work/`, and the unmounted `staging/`;
    /// emptied for reuse or removed on drop.
    root: PathBuf,
    /// Materialized `/inputs` files by virtual path.
    inputs: BTreeMap<String, Input>,
    /// Bytes the inputs, staged or placed, take from the scratch limit.
    inputs_bytes: u64,
    /// Held by one `/work` writer at a time, so each sees the space the
    /// others left; shared so blocking work can own its turn.
    writer: Arc<tokio::sync::Mutex<()>>,
    /// Limits in force.
    limits: HostedLimits,
    /// The scratch charge for the bytes on disk plus the `/work` write in
    /// flight, or `None` for an uncharged workspace. Declared after `root`
    /// so it returns only after the directory is removed.
    scratch: Option<std::sync::Mutex<ByteCharge>>,
    /// The process memory ledger `fs.extract` charges for the document it
    /// reads and the output it builds, or `None` for an uncharged workspace.
    memory: Option<ByteLedger>,
    /// Where an emptied root returns on drop, or `None` to remove it.
    free: Option<FreeRoots>,
}

impl Workspace {
    /// Write `staged` into a fresh directory under `scratch`, charging its
    /// ledger before each byte is written, or, with `None`, under the system
    /// temporary directory uncharged. Each file is written to the staging
    /// directory and placed exactly as [`Workspaces::open`] places a
    /// streamed one.
    ///
    /// # Errors
    /// Returns [`BlockError::Terminal`] for an invalid path, a duplicate
    /// path, an unsafe or oversized archive, or scratch over the limit,
    /// [`BlockError::Busy`] when the scratch ledger cannot hold the inputs,
    /// and [`BlockError::Retryable`] for a local IO failure.
    pub fn materialize(
        staged: Vec<Staged>,
        limits: HostedLimits,
        scratch: impl Into<Option<ScratchRoot>>,
    ) -> Result<Self, BlockError> {
        let mut workspace = Self::create(limits, scratch.into())?;
        for (index, file) in staged.into_iter().enumerate() {
            let size = u64::try_from(file.bytes.len()).unwrap_or(u64::MAX);
            workspace.reserve(size)?;
            let path = workspace.staged(index);
            std::fs::write(&path, &file.bytes).map_err(io)?;
            workspace.install(Pending {
                path: file.path,
                body: Body::Staged(path),
                size,
                format: file.format,
                sources: file.sources,
            })?;
        }
        Ok(workspace)
    }

    /// An empty workspace in a fresh directory under `scratch`, or, with
    /// `None`, under the system temporary directory uncharged.
    ///
    /// # Errors
    /// Returns [`BlockError::Retryable`] for a local IO failure.
    fn create(limits: HostedLimits, scratch: Option<ScratchRoot>) -> Result<Self, BlockError> {
        let root = match &scratch {
            Some(scratch) => tempfile::tempdir_in(&scratch.dir),
            None => tempfile::tempdir(),
        }
        .map_err(io)?;
        for dir in WORKSPACE_DIRS {
            std::fs::create_dir(root.path().join(dir)).map_err(io)?;
        }
        // From here the workspace's drop owns the removal.
        Ok(Self::empty(root.keep(), limits, scratch))
    }

    /// A workspace over `root`, which already holds the empty
    /// [`WORKSPACE_DIRS`].
    fn empty(root: PathBuf, limits: HostedLimits, scratch: Option<ScratchRoot>) -> Self {
        Self {
            root,
            inputs: BTreeMap::new(),
            inputs_bytes: 0,
            writer: Arc::new(tokio::sync::Mutex::new(())),
            limits,
            scratch: scratch.map(|scratch| std::sync::Mutex::new(scratch.ledger.empty_charge())),
            memory: None,
            free: None,
        }
    }

    /// The staging path of the `index`th input, outside every mount.
    fn staged(&self, index: usize) -> PathBuf {
        self.root.join("staging").join(index.to_string())
    }

    /// Charge `bytes` more input to the workspace scratch limit and, for a
    /// charged workspace, its scratch ledger, before they are written.
    ///
    /// # Errors
    /// Returns [`BlockError::Terminal`] past the workspace scratch limit and
    /// [`BlockError::Busy`] when the scratch ledger cannot hold `bytes` now.
    fn reserve(&mut self, bytes: u64) -> Result<(), BlockError> {
        let held = self.inputs_bytes.saturating_add(bytes);
        if held > self.limits.scratch_max_bytes {
            return Err(terminal(format!(
                "the workspace exceeds the {}-byte scratch limit",
                self.limits.scratch_max_bytes
            )));
        }
        if let Some(charge) = &mut self.scratch {
            charge
                .get_mut()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .try_grow(bytes)
                .map_err(|_| BlockError::Busy {
                    reason: "process scratch cannot hold the workspace inputs now".to_owned(),
                })?;
        }
        self.inputs_bytes = held;
        Ok(())
    }

    /// Place `file` under its `/inputs` path: write encoded bytes there,
    /// rename a staged plain file into place, or unpack an archive from its
    /// staged file, then remove that file and return its scratch.
    ///
    /// # Errors
    /// Returns [`BlockError::Terminal`] for an invalid or duplicate path, an
    /// unsafe or oversized archive, or scratch over the limit,
    /// [`BlockError::Busy`] when the scratch ledger cannot hold an unpacked
    /// entry, and [`BlockError::Retryable`] for a local IO failure.
    fn install(&mut self, file: Pending) -> Result<(), BlockError> {
        relative(&file.path, "/inputs/")
            .ok_or_else(|| terminal(format!("{} is not an /inputs path", file.path)))?;
        let staged = match &file.body {
            Body::Encoded { bytes, .. } => {
                let host = self.claim(&file.path, file.format, &file.sources)?;
                return std::fs::write(host, bytes).map_err(io);
            }
            Body::Staged(staged) => staged,
        };
        match file.format {
            Some(MediaFormat::Zip) => self.unzip(&file, staged)?,
            Some(MediaFormat::TarGz) => self.untar(&file, staged)?,
            _ => {
                let host = self.claim(&file.path, file.format, &file.sources)?;
                return std::fs::rename(staged, host).map_err(io);
            }
        }
        std::fs::remove_file(staged).map_err(io)?;
        self.inputs_bytes = self.inputs_bytes.saturating_sub(file.size);
        if let Some(charge) = &mut self.scratch {
            charge
                .get_mut()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .shrink(file.size);
        }
        Ok(())
    }

    /// Record virtual `/inputs` `path` and return its host path, with its
    /// parent directories created.
    ///
    /// # Errors
    /// Returns [`BlockError::Terminal`] for a duplicate or unsafe path, and
    /// [`BlockError::Retryable`] for a local IO failure.
    fn claim(
        &mut self,
        path: &str,
        format: Option<MediaFormat>,
        sources: &BTreeSet<SlotName>,
    ) -> Result<PathBuf, BlockError> {
        if self.inputs.contains_key(path) {
            return Err(terminal(format!("{path} is materialized twice")));
        }
        let host = self
            .host(path)
            .ok_or_else(|| terminal(format!("{path} is not under /inputs or /work")))?;
        if let Some(parent) = host.parent() {
            std::fs::create_dir_all(parent).map_err(io)?;
        }
        self.inputs.insert(
            path.to_owned(),
            Input {
                sources: sources.clone(),
                format,
            },
        );
        Ok(host)
    }

    /// Unpack `file`'s zip archive, staged at `staged`, under its path
    /// after [`zip_guard`].
    ///
    /// # Errors
    /// Returns [`BlockError::Terminal`] for a guarded limit, an entry that is
    /// a link or escapes the archive, or a file over the file limit, and
    /// [`Self::write_entry`]'s errors.
    fn unzip(&mut self, file: &Pending, staged: &Path) -> Result<(), BlockError> {
        let open = || std::fs::File::open(staged).map_err(io);
        zip_guard(open()?, &self.limits.extract()).map_err(extract_refused)?;
        let mut archive = zip::ZipArchive::new(open()?)
            .map_err(|error| terminal(format!("{}: {error}", file.path)))?;
        for index in 0..archive.len() {
            let mut entry = archive
                .by_index(index)
                .map_err(|error| terminal(format!("{}: {error}", file.path)))?;
            if entry.is_symlink() {
                return Err(terminal(format!("{} contains a link", file.path)));
            }
            if entry.is_dir() {
                continue;
            }
            let name = entry.enclosed_name().ok_or_else(|| {
                terminal(format!("{} has an entry outside the archive", file.path))
            })?;
            self.write_entry(file, &name, &mut entry)?;
        }
        Ok(())
    }

    /// Unpack `file`'s tar.gz archive, staged at `staged`, under its path,
    /// admitting only regular files and directories, under the entry, byte,
    /// and ratio limits.
    ///
    /// # Errors
    /// Returns [`BlockError::Terminal`] for an unreadable archive, a link or
    /// special entry, an escaping path, or a limit, and
    /// [`Self::write_entry`]'s errors.
    fn untar(&mut self, file: &Pending, staged: &Path) -> Result<(), BlockError> {
        let budget = self
            .limits
            .archive_max_bytes
            .min(file.size.saturating_mul(self.limits.archive_max_ratio));
        let unreadable = |error: std::io::Error| terminal(format!("{}: {error}", file.path));
        let staged = std::fs::File::open(staged).map_err(io)?;
        // The decoder is capped, so a header understating sizes cannot
        // inflate the stream past the archive budget.
        let mut decoder =
            flate2::read::GzDecoder::new(std::io::BufReader::new(staged)).take(budget + 1);
        let mut archive = tar::Archive::new(&mut decoder);
        let mut count = 0_usize;
        for entry in archive.entries().map_err(unreadable)? {
            let mut entry = entry.map_err(unreadable)?;
            count += 1;
            if count > self.limits.archive_max_entries {
                return Err(terminal(format!(
                    "{} has more than {} entries",
                    file.path, self.limits.archive_max_entries
                )));
            }
            match entry.header().entry_type() {
                tar::EntryType::Directory => continue,
                tar::EntryType::Regular => {}
                _ => {
                    return Err(terminal(format!(
                        "{} contains a link or special file",
                        file.path
                    )));
                }
            }
            let name = entry.path().map_err(unreadable)?.into_owned();
            if name
                .components()
                .any(|part| !matches!(part, Component::Normal(_)))
            {
                return Err(terminal(format!(
                    "{} has an entry outside the archive",
                    file.path
                )));
            }
            self.write_entry(file, &name, &mut entry)?;
        }
        if decoder.limit() == 0 {
            return Err(terminal(format!(
                "{} decompresses past its {budget}-byte limit",
                file.path
            )));
        }
        Ok(())
    }

    /// Stream an archive entry to `<archive path>/<name>`, typed by
    /// extension, charging scratch before each piece is written and refusing
    /// the entry past the file limit.
    ///
    /// # Errors
    /// Returns [`BlockError::Terminal`] for an unreadable or oversized entry,
    /// and [`Self::claim`]'s and [`Self::reserve`]'s errors.
    fn write_entry(
        &mut self,
        archive: &Pending,
        name: &Path,
        entry: &mut impl Read,
    ) -> Result<(), BlockError> {
        let name = name.to_string_lossy();
        let format = name
            .strip_suffix(".tar.gz")
            .map(|_| MediaFormat::TarGz)
            .or_else(|| name.rsplit_once('.').and_then(|(_, ext)| ext.parse().ok()))
            .filter(|format: &MediaFormat| !format.is_archive());
        let host = self.claim(
            &format!("{}/{name}", archive.path),
            format,
            &archive.sources,
        )?;
        let mut out = std::fs::File::create_new(host).map_err(io)?;
        let mut buffer = vec![0_u8; 64 << 10];
        let mut written = 0_u64;
        loop {
            let read = entry
                .read(&mut buffer)
                .map_err(|error| terminal(format!("{}: {error}", archive.path)))?;
            if read == 0 {
                return Ok(());
            }
            let read_bytes = u64::try_from(read).unwrap_or(u64::MAX);
            written = written.saturating_add(read_bytes);
            if written > self.limits.file_max_bytes {
                return Err(terminal(format!(
                    "{} has an entry over the file limit",
                    archive.path
                )));
            }
            self.reserve(read_bytes)?;
            out.write_all(&buffer[..read]).map_err(io)?;
        }
    }

    /// The host path of a virtual `/inputs` or `/work` path, or `None` for
    /// any other or unsafe path.
    fn host(&self, path: &str) -> Option<PathBuf> {
        let (dir, rest) = relative(path, "/inputs/")
            .map(|rest| ("inputs", rest))
            .or_else(|| relative(path, "/work/").map(|rest| ("work", rest)))?;
        Some(self.root.join(dir).join(rest))
    }

    /// The host directory mounted at `/inputs`.
    #[must_use]
    pub fn inputs_dir(&self) -> PathBuf {
        self.root.join("inputs")
    }

    /// The host directory mounted at `/work`.
    #[must_use]
    pub fn work_dir(&self) -> PathBuf {
        self.root.join("work")
    }

    /// Wait to be the only `/work` writer; dropping the guard lets the next
    /// one in. Cancelling the wait gives up the turn. The guard is owned, so
    /// blocking work moved to another thread keeps the turn until it ends.
    pub async fn writer(&self) -> tokio::sync::OwnedMutexGuard<()> {
        Arc::clone(&self.writer).lock_owned().await
    }

    /// Freeze `/work` file or directory `path` for publication, read while
    /// holding [`Self::writer`] so no `/work` writer changes it mid-read and
    /// later writes never reach the frozen copy.
    ///
    /// A directory is walked completely or refused: every entry must be a
    /// regular file or directory with a UTF-8 name free of backslashes and
    /// control characters, no two relative paths may match under NFC plus
    /// Unicode case folding, and the tree must stay within the entry, depth,
    /// and byte limits. Nothing is renamed, skipped, or truncated. Every
    /// byte read is charged to process memory first, and the returned charge
    /// covers the frozen copy for as long as the caller keeps it. The read
    /// runs on `work`, so the session settles it before its attempt does.
    ///
    /// # Errors
    /// Returns an [`invalid`] refusal: `publication_path` for a path outside
    /// `/work`, `workspace_missing` for a missing path, `regular_file` for a
    /// special file, `publication_tree` for an entry the tree rules refuse,
    /// `work_budget` for content over the byte limit; a busy host stop when
    /// process memory cannot hold the copy now; and an invocation failure
    /// for an unreadable entry.
    pub async fn freeze(
        self: &Arc<Self>,
        path: &str,
        work: &tokio_util::task::TaskTracker,
    ) -> Result<(Frozen, Option<ByteCharge>), ToolError> {
        if relative(path, "/work/").is_none() {
            return Err(invalid(
                "publication_path",
                "path",
                path,
                format!("{path} is not a file or directory this invocation wrote under /work"),
                "a path under /work without empty, . or .. segments",
            ));
        }
        let writer = self.writer().await;
        let workspace = Arc::clone(self);
        let path = path.to_owned();
        // The blocking read owns the writer turn, so a cancelled caller
        // never lets a writer in while the read still runs.
        work.spawn_blocking(move || {
            let _writer = writer;
            workspace.snapshot(&path)
        })
        .await
        .map_err(|error| invocation(error.to_string()))?
    }

    /// [`Self::freeze`]'s read of `/work` path `path`; the caller holds
    /// [`Self::writer`] and runs it on the blocking pool.
    ///
    /// # Errors
    /// Returns [`Self::freeze`]'s refusals and failures.
    fn snapshot(&self, path: &str) -> Result<(Frozen, Option<ByteCharge>), ToolError> {
        let rest = relative(path, "/work/").ok_or_else(|| outside(path))?;
        let host = self.work_dir().join(rest);
        let mut charge: Option<ByteCharge> = None;
        let mut read = |host: &Path, bytes: u64| -> Result<Vec<u8>, ToolError> {
            let more = self.charge_memory(bytes)?;
            match (&mut charge, more) {
                (Some(held), Some(more)) => held.merge(more),
                (held, more) => *held = held.take().or(more),
            }
            std::fs::read(host).map_err(failed)
        };
        if !host.symlink_metadata().map_err(|_| missing(path))?.is_dir() {
            let (host, _) = self.file(path)?;
            let name = rest.rsplit('/').next().unwrap_or(rest).to_owned();
            let size = host.metadata().map_err(failed)?.len();
            let bytes = read(&host, size)?;
            return Ok((Frozen::File { name, bytes }, charge));
        }
        let mut tree = Tree::default();
        let mut pending = vec![(host, String::new(), 0_usize)];
        while let Some((dir, prefix, depth)) = pending.pop() {
            for entry in std::fs::read_dir(&dir).map_err(failed)? {
                let entry = entry.map_err(failed)?;
                let name = entry.file_name().into_string().map_err(|name| {
                    tree_refused(path, &format!("{} is not UTF-8", name.to_string_lossy()))
                })?;
                let relative = if prefix.is_empty() {
                    name.clone()
                } else {
                    format!("{prefix}/{name}")
                };
                tree.admit(path, &name, &relative, self.limits.archive_max_entries)?;
                let meta = entry.path().symlink_metadata().map_err(failed)?;
                if meta.is_dir() {
                    if depth + 1 > PUBLICATION_MAX_DEPTH {
                        return Err(tree_refused(
                            path,
                            &format!("{relative} is deeper than {PUBLICATION_MAX_DEPTH} levels"),
                        ));
                    }
                    tree.directories.push(relative.clone());
                    pending.push((entry.path(), relative, depth + 1));
                } else if meta.is_file() {
                    tree.bytes += meta.len();
                    if tree.bytes > self.limits.work_write_max_bytes {
                        return Err(invalid(
                            "work_budget",
                            "path",
                            path,
                            format!(
                                "{path} holds more than the {}-byte /work limit",
                                self.limits.work_write_max_bytes
                            ),
                            "a smaller file or directory",
                        ));
                    }
                    let bytes = read(&entry.path(), meta.len())?;
                    tree.files.push((relative, bytes));
                } else {
                    return Err(tree_refused(
                        path,
                        &format!("{relative} is a link or special file"),
                    ));
                }
            }
        }
        tree.files.sort();
        tree.directories.sort();
        Ok((
            Frozen::Directory {
                files: tree.files,
                directories: tree.directories,
            },
            charge,
        ))
    }

    /// The sandbox mounts: `/inputs` read-only and `/work` writable up to
    /// the space `/work` may still grow by, which is the smaller of the
    /// `/work` write limit and the scratch left after inputs, less what
    /// `/work` holds now.
    ///
    /// Measured from disk, so a write that later fails or is cancelled
    /// still counts and an overwrite counts once. Opens and walks host
    /// directories, so call it on the blocking pool while holding
    /// [`Self::writer`]; otherwise concurrent writers share the remainder.
    ///
    /// # Errors
    /// Returns [`BlockError::Retryable`] when a mount directory cannot open.
    pub fn mounts(&self) -> Result<Vec<MountSpec>, BlockError> {
        let unopened = |error: monty_pool::PoolError| BlockError::Retryable {
            reason: format!("a workspace mount failed: {error}"),
        };
        let mut work =
            MountSpec::new("/work", self.work_dir(), MountSpecMode::ReadWrite).map_err(unopened)?;
        work.write_bytes_limit = Some(self.work_room());
        Ok(vec![
            MountSpec::new("/inputs", self.inputs_dir(), MountSpecMode::ReadOnly)
                .map_err(unopened)?,
            work,
        ])
    }

    /// The bytes `/work` may still grow by: the smaller of the `/work` write
    /// limit and the scratch left after inputs, less what `/work` holds now,
    /// measured from disk. Call it while holding [`Self::writer`].
    #[must_use]
    pub fn work_room(&self) -> u64 {
        self.room_beside(self.work_held())
    }

    /// Admit one `/work` write of `bytes`: refuse it past [`Self::work_room`],
    /// and for a charged workspace set its scratch charge to its inputs,
    /// what `/work` holds, and `bytes`, refusing the write when the process
    /// scratch ledger cannot grant that. Call it while holding
    /// [`Self::writer`] and before writing, so the charge always covers
    /// what is on disk plus the one write in flight; a later admission
    /// returns what the write did not use or a removed file freed.
    ///
    /// # Errors
    /// Returns [`WriteRefusal`] naming the limit that refused the write.
    pub fn admit_write(&self, bytes: u64) -> Result<(), WriteRefusal> {
        let held = self.work_held();
        if bytes > self.room_beside(held) {
            return Err(WriteRefusal::Limit);
        }
        let Some(charge) = &self.scratch else {
            return Ok(());
        };
        let mut charge = charge
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let want = self.inputs_bytes.saturating_add(held).saturating_add(bytes);
        let charged = charge.bytes();
        match want.checked_sub(charged) {
            None => {
                charge.shrink(charged - want);
                Ok(())
            }
            Some(more) => charge.try_grow(more).map_err(|_| WriteRefusal::Scratch),
        }
    }

    /// A process memory charge of `bytes`, or `None` for an uncharged
    /// workspace.
    ///
    /// # Errors
    /// Returns a busy host stop when process memory cannot hold `bytes`
    /// now, so the Agent session stops rather than answering the model.
    fn charge_memory(&self, bytes: u64) -> Result<Option<ByteCharge>, ToolError> {
        self.memory
            .as_ref()
            .map(|memory| memory.try_charge(bytes).map_err(|_| memory_busy()))
            .transpose()
    }

    /// Bytes `/work` holds on disk now.
    fn work_held(&self) -> u64 {
        walkdir(&self.work_dir(), usize::MAX)
            .0
            .iter()
            .map(|(_, size)| size)
            .sum()
    }

    /// The room `/work` has while it holds `held` bytes.
    fn room_beside(&self, held: u64) -> u64 {
        self.limits
            .work_write_max_bytes
            .min(
                self.limits
                    .scratch_max_bytes
                    .saturating_sub(self.inputs_bytes),
            )
            .saturating_sub(held)
    }

    /// Original slots behind `/inputs` file `path`.
    #[must_use]
    pub fn sources(&self, path: &str) -> Option<&BTreeSet<SlotName>> {
        self.inputs.get(path).map(|input| &input.sources)
    }

    /// `fs.list`: files under virtual directory `dir` with their sizes and,
    /// for `/inputs` files, the original slots they came from.
    ///
    /// Collection stops at the tool result limit, and the `/work` walk at a
    /// proportional entry count, so a tree of many small or empty files
    /// costs no more than the result it fills; `truncated` says so.
    ///
    /// # Errors
    /// Returns a `workspace_path` [`invalid`] refusal for a path outside the
    /// workspace.
    pub fn list(&self, dir: &str) -> Result<Value, ToolError> {
        let prefix = format!("{}/", dir.trim_end_matches('/'));
        if prefix != "/inputs/" && prefix != "/work/" && self.host(dir).is_none() {
            return Err(outside(dir));
        }
        let mut files = Collected::new(
            &json!({"files": [], "truncated": false}),
            self.limits.tool_result_max_bytes,
        );
        let mut truncated = false;
        for (path, input) in self
            .inputs
            .range(prefix.clone()..)
            .take_while(|(path, _)| path.starts_with(&prefix))
        {
            let size = self
                .host(path)
                .and_then(|host| std::fs::metadata(host).ok())
                .map_or(0, |meta| meta.len());
            if !files.push(json!({"path": path, "size_bytes": size, "sources": input.sources})) {
                truncated = true;
                break;
            }
        }
        if !truncated && ("/work/".starts_with(&prefix) || prefix.starts_with("/work/")) {
            let (found, cut) = walkdir(&self.work_dir(), self.walk_limit());
            truncated = cut;
            for (path, size) in found {
                let path = format!("/work/{path}");
                if path.starts_with(&prefix)
                    && !files.push(json!({"path": path, "size_bytes": size, "sources": []}))
                {
                    truncated = true;
                    break;
                }
            }
        }
        Ok(json!({"files": files.items, "truncated": truncated}))
    }

    /// `fs.read`: the UTF-8 lines of `path` from piece `start`, each with
    /// its line locator, bounded by the tool result limit by their actual
    /// JSON size; `next` resumes.
    ///
    /// Lines longer than an eighth of the result budget are split into
    /// pieces that keep the line's locator, so every character of a long
    /// line is reachable by following `next`. Piece indices depend only on
    /// the file and the limits. `sources` names the original slots of an
    /// `/inputs` file and is empty for `/work` files. Nothing is parsed.
    ///
    /// # Errors
    /// Returns [`Self::text`]'s errors and a `result_limit` [`invalid`]
    /// refusal for a path too long for any piece to fit the limit.
    pub fn read(&self, path: &str, start: usize) -> Result<Value, ToolError> {
        let (text, sources) = self.text(path)?;
        let units = lines(&text);
        let mut out = Collected::new(
            &json!({"path": path, "sources": sources, "units": [], "next": usize::MAX}),
            self.limits.tool_result_max_bytes,
        );
        let piece = (out.room() / 8).max(4);
        let mut next = None;
        let pieces = units
            .iter()
            .flat_map(|(locator, text)| pieces(text, piece).map(move |text| (locator, text)));
        for (index, (locator, text)) in pieces.enumerate().skip(start) {
            if !out.push(json!({"locator": locator, "text": text})) {
                if out.items.is_empty() {
                    return Err(invalid(
                        "result_limit",
                        "path",
                        path,
                        format!(
                            "{path} cannot be read within the {}-byte tool result limit",
                            self.limits.tool_result_max_bytes
                        ),
                        "a shorter path",
                    ));
                }
                next = Some(index);
                break;
            }
        }
        Ok(json!({"path": path, "sources": sources, "units": out.items, "next": next}))
    }

    /// `fs.search`: one hit per text line matching `search`, with its line
    /// locator, a bounded snippet, and the file's original slots, across
    /// `/inputs` and readable `/work` text in path then line order, bounded
    /// by the tool result limit.
    ///
    /// A literal query matches as written; `regex` reads it as Rust regex
    /// syntax. Both ignore case unless `case_sensitive`. `path` scopes the
    /// search to one file or, recursively, one directory; `glob` keeps the
    /// files whose basename (a pattern without `/`) or path relative to the
    /// scoped directory (the file's parent for a file scope, the workspace
    /// root without a scope) it matches, `*` within one segment and `**`
    /// across segments. Binary and non-UTF-8 files are skipped unparsed.
    /// Files are visited lazily and the `/work` walk is capped like
    /// [`Self::list`], so the search stops as soon as the result is full.
    ///
    /// # Errors
    /// Returns an [`invalid`] refusal: `search_query` for an empty query,
    /// `search_pattern` for an invalid pattern, [`glob`]'s refusals, and
    /// [`Self::scope`]'s.
    pub fn search(&self, search: &Search) -> Result<Value, ToolError> {
        if search.query.is_empty() {
            return Err(invalid(
                "search_query",
                "query",
                "",
                "the search query is empty".to_owned(),
                "non-empty text or regex",
            ));
        }
        let pattern = if search.regex {
            search.query.clone()
        } else {
            regex::escape(&search.query)
        };
        let matcher = regex::RegexBuilder::new(&pattern)
            .case_insensitive(!search.case_sensitive)
            .build()
            .map_err(|error| {
                invalid(
                    "search_pattern",
                    "query",
                    &search.query,
                    format!("the search pattern is invalid: {error}"),
                    "Rust regex syntax",
                )
            })?;
        let scope = search
            .path
            .as_deref()
            .map(|path| self.scope(path))
            .transpose()?;
        let glob = search.glob.as_deref().map(glob).transpose()?;
        let base = match &scope {
            None => "/".to_owned(),
            Some(Scope::File(path)) => path
                .rsplit_once('/')
                .map_or_else(String::new, |(parent, _)| format!("{parent}/")),
            Some(Scope::Dir(prefix)) => prefix.clone(),
        };
        let mut matches = Collected::new(
            &json!({"matches": [], "truncated": false}),
            self.limits.tool_result_max_bytes,
        );
        let (work, cut) = walkdir(&self.work_dir(), self.walk_limit());
        let candidates = self
            .inputs
            .keys()
            .cloned()
            .chain(work.into_iter().map(|(path, _)| format!("/work/{path}")))
            .filter(|path| match &scope {
                None => true,
                Some(Scope::File(file)) => path == file,
                Some(Scope::Dir(prefix)) => path.starts_with(prefix.as_str()),
            })
            .filter(|path| {
                glob.as_ref().is_none_or(|(pattern, nested)| {
                    let name = if *nested {
                        path.strip_prefix(base.as_str()).unwrap_or(path)
                    } else {
                        path.rsplit('/').next().unwrap_or(path)
                    };
                    pattern.matches_with(
                        name,
                        glob::MatchOptions {
                            case_sensitive: true,
                            require_literal_separator: true,
                            require_literal_leading_dot: false,
                        },
                    )
                })
            });
        for path in candidates {
            let Ok((text, sources)) = self.text(&path) else {
                continue;
            };
            for (locator, line) in lines(&text) {
                let Some(found) = matcher.find(&line) else {
                    continue;
                };
                let hit = json!({
                    "path": path,
                    "locator": locator,
                    "snippet": snippet(&line, found.start()),
                    "sources": sources,
                });
                if !matches.push(hit) {
                    return Ok(json!({"matches": matches.items, "truncated": true}));
                }
            }
        }
        Ok(json!({"matches": matches.items, "truncated": cut}))
    }

    /// `fs.extract`: run the shared extractor over workspace file `path` and
    /// write its `ExtractOutput` JSON to the new `/work` file `output_path`.
    ///
    /// The document's format is its declared or stored format, else its
    /// extension. The output is written to a temporary file outside `/work`
    /// and moved into place only when complete and never over an existing
    /// file, so a failure leaves no usable destination. Call it while
    /// holding [`Self::writer`] so the `/work` budget it checks stays true.
    ///
    /// # Errors
    /// Returns [`Self::file`]'s errors and an [`invalid`] refusal:
    /// `extractable_document` for an unextractable file,
    /// `extraction_refused` when the extractor refuses it, `output_path`
    /// for an output outside `/work`, `output_exists` for one already
    /// present, and `work_budget` for one over the `/work` budget. A local
    /// IO or serialization failure is an [`invocation`] failure.
    pub fn extract(&self, path: &str, output_path: &str) -> Result<Value, ToolError> {
        let (host, sources) = self.file(path)?;
        let format = self
            .inputs
            .get(path)
            .and_then(|input| input.format)
            .or_else(|| {
                path.rsplit_once('.')
                    .and_then(|(_, extension)| extension.parse().ok())
            })
            .filter(|format: &MediaFormat| format.is_extractable())
            .ok_or_else(|| {
                invalid(
                    "extractable_document",
                    "path",
                    path,
                    format!("{path} is not an extractable document"),
                    "a PDF, DOCX, PPTX, XLSX, HTML, or other extractable document",
                )
            })?;
        let new_path = |message: String| {
            invalid(
                "output_path",
                "output_path",
                output_path,
                message,
                "a new file path under /work",
            )
        };
        let exists = || {
            invalid(
                "output_exists",
                "output_path",
                output_path,
                format!("{output_path} already exists"),
                "a /work path that does not exist yet",
            )
        };
        let rest = relative(output_path, "/work/")
            .ok_or_else(|| new_path(format!("{output_path} is not a new file path under /work")))?;
        let target = self.work_dir().join(rest);
        if target.symlink_metadata().is_ok() {
            return Err(exists());
        }
        let size = std::fs::metadata(&host).map_err(failed)?.len();
        let mut held = self.charge_memory(size)?;
        let bytes = std::fs::read(&host).map_err(failed)?;
        let output = Extraction::new(&self.limits.extract())
            .run(&bytes, format)
            .map_err(|error| {
                invalid(
                    "extraction_refused",
                    "path",
                    path,
                    format!("{path} cannot be extracted: {}", error.0),
                    "a well-formed document within the extraction limits",
                )
            })?;
        // The encoding streams from the extracted value into its staged
        // file, so it is never held in memory whole. Its exact counted
        // length is admitted to scratch before it is written, and stands in
        // for the extracted value's own size in memory while it streams.
        let written = crate::verification::encoded_len(&output, true);
        if let Some(held) = &mut held {
            held.try_grow(written).map_err(|_| memory_busy())?;
        }
        match self.admit_write(written) {
            Ok(()) => {}
            Err(WriteRefusal::Scratch) => {
                return Err(host_stop(BlockError::Busy {
                    reason: WriteRefusal::Scratch.to_string(),
                }));
            }
            Err(refusal @ WriteRefusal::Limit) => {
                return Err(invalid(
                    "work_budget",
                    "output_path",
                    output_path,
                    format!("{output_path} cannot be written: {refusal}"),
                    "fewer or smaller /work files",
                ));
            }
        }
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent).map_err(failed)?;
            let confined = parent
                .canonicalize()
                .ok()
                .zip(self.work_dir().canonicalize().ok())
                .is_some_and(|(parent, work)| parent.starts_with(work));
            if !confined {
                return Err(new_path(format!("{output_path} escapes /work")));
            }
        }
        let mut staged = tempfile::NamedTempFile::new_in(&self.root).map_err(failed)?;
        {
            let mut writer = std::io::BufWriter::new(staged.as_file_mut());
            serde_json::to_writer_pretty(&mut writer, &output).map_err(|error| {
                invocation(format!("the extraction does not serialize: {error}"))
            })?;
            std::io::Write::flush(&mut writer).map_err(failed)?;
        }
        staged.persist_noclobber(&target).map_err(|error| {
            if error.error.kind() == std::io::ErrorKind::AlreadyExists {
                exists()
            } else {
                failed(error.error)
            }
        })?;
        Ok(json!({
            "output_path": output_path,
            "sources": sources,
            "extractor_version": EXTRACT_VERSION,
        }))
    }

    /// `fs.write`: create or replace the regular `/work` file `path` with
    /// UTF-8 `content`, creating missing parent directories, and return the
    /// path and byte count written.
    ///
    /// Run it on the blocking pool while holding [`Self::writer`], so the
    /// `/work` budget and scratch charge it admits stay true. Every existing parent must be a
    /// real directory and an existing target a regular file, so a write never
    /// follows a link out of `/work`. The budget counts a replaced file once;
    /// a refused write creates nothing, and the new bytes land by rename, so
    /// a failed write never leaves a partial file.
    ///
    /// # Errors
    /// Returns an [`invalid`] refusal: `write_path` for a path outside
    /// `/work` (`/inputs` included), `write_parent` for a parent that is not
    /// a directory, `regular_file` for a target that is a directory, link, or
    /// special file, and `work_budget` for content over the `/work` budget. A
    /// local IO failure is an [`invocation`] failure. A write process scratch
    /// cannot hold now is a busy host stop, so the session stops rather than
    /// answering the model.
    pub fn write(&self, path: &str, content: &str) -> Result<Value, ToolError> {
        let rest = relative(path, "/work/").ok_or_else(|| {
            invalid(
                "write_path",
                "path",
                path,
                format!("{path} is not a file path under /work; /inputs is read-only"),
                "a file path under /work without empty, . or .. segments",
            )
        })?;
        let segments: Vec<&str> = rest.split('/').collect();
        let (name, parents) = segments.split_last().ok_or_else(|| outside(path))?;
        let mut parent = self.work_dir();
        let mut missing_parents = Vec::new();
        for segment in parents {
            parent.push(segment);
            if !missing_parents.is_empty() {
                missing_parents.push(parent.clone());
                continue;
            }
            match parent.symlink_metadata() {
                Ok(meta) if meta.is_dir() => {}
                Ok(_) => {
                    return Err(invalid(
                        "write_parent",
                        "path",
                        path,
                        format!("a parent of {path} is a file, link, or special file"),
                        "a path whose existing parents are directories",
                    ));
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    missing_parents.push(parent.clone());
                }
                Err(error) => return Err(failed(error)),
            }
        }
        let target = parent.join(name);
        let replaced = match target.symlink_metadata() {
            Ok(meta) if meta.is_file() => meta.len(),
            Ok(_) => {
                return Err(invalid(
                    "regular_file",
                    "path",
                    path,
                    format!("{path} is a directory, link, or special file"),
                    "a new or existing regular file",
                ));
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => 0,
            Err(error) => return Err(failed(error)),
        };
        let bytes = u64::try_from(content.len()).unwrap_or(u64::MAX);
        // The new content is written to a temporary file beside the file it
        // replaces and renamed over it, so for that moment both are on disk.
        // The admission below holds the replaced file and asks only for what
        // the write adds; the rest of the copy is charged here first, so a
        // refusal leaves the original untouched.
        let ledger = self.scratch.as_ref().map(|charge| {
            charge
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .ledger()
                .clone()
        });
        let _copy = ledger
            .as_ref()
            .map(|ledger| ledger.try_charge(bytes.min(replaced)))
            .transpose()
            .map_err(|_| {
                host_stop(BlockError::Busy {
                    reason: WriteRefusal::Scratch.to_string(),
                })
            })?;
        match self.admit_write(bytes.saturating_sub(replaced)) {
            Ok(()) => {}
            Err(WriteRefusal::Scratch) => {
                return Err(host_stop(BlockError::Busy {
                    reason: WriteRefusal::Scratch.to_string(),
                }));
            }
            Err(WriteRefusal::Limit) => {
                return Err(invalid(
                    "work_budget",
                    "content",
                    path,
                    format!("{path} would exceed the /work write budget"),
                    "fewer or smaller /work files",
                ));
            }
        }
        for directory in &missing_parents {
            std::fs::create_dir(directory).map_err(failed)?;
        }
        let mut staged = tempfile::NamedTempFile::new_in(&self.root).map_err(failed)?;
        let persisted = match std::io::Write::write_all(&mut staged, content.as_bytes()) {
            Ok(()) => staged
                .persist(&target)
                .map_err(|error| (error.file, error.error)),
            Err(error) => Err((staged, error)),
        };
        if let Err((staged, error)) = persisted {
            let left = staged.path().to_owned();
            if let Err(cleanup) = staged.close() {
                // Nothing on disk accounts for a surviving temporary file,
                // so its largest size stays charged.
                if let Some(ledger) = &ledger {
                    retain(ledger.charge_infallible(bytes), &left, &cleanup);
                }
            }
            return Err(failed(error));
        }
        Ok(json!({"path": path, "bytes": bytes}))
    }

    /// Most `/work` entries a list or search visits: no result could show
    /// more, since every listed entry takes more than 16 bytes.
    const fn walk_limit(&self) -> usize {
        self.limits.tool_result_max_bytes / 16
    }

    /// The search scope `path` names: one workspace file, or a directory as
    /// its `/`-terminated prefix.
    ///
    /// # Errors
    /// Returns a `workspace_missing` [`invalid`] refusal for a path naming
    /// neither.
    fn scope(&self, path: &str) -> Result<Scope, ToolError> {
        if self.file(path).is_ok() {
            return Ok(Scope::File(path.to_owned()));
        }
        let prefix = format!("{}/", path.trim_end_matches('/'));
        let input_dir = prefix == "/inputs/"
            || self
                .inputs
                .range(prefix.clone()..)
                .next()
                .is_some_and(|(input, _)| input.starts_with(&prefix));
        let work_dir = prefix == "/work/"
            || relative(path.trim_end_matches('/'), "/work/").is_some_and(|rest| {
                self.work_dir()
                    .join(rest)
                    .symlink_metadata()
                    .is_ok_and(|meta| meta.is_dir())
            });
        if input_dir || work_dir {
            Ok(Scope::Dir(prefix))
        } else {
            Err(missing(path))
        }
    }

    /// The UTF-8 text and sources of workspace file `path`.
    ///
    /// # Errors
    /// Returns [`Self::file`]'s errors, a `utf8_text` [`invalid`] refusal
    /// for bytes that are not UTF-8 text, and an [`invocation`] failure for
    /// a local IO failure.
    fn text(&self, path: &str) -> Result<(String, Value), ToolError> {
        let (host, sources) = self.file(path)?;
        let text = String::from_utf8(std::fs::read(&host).map_err(failed)?).map_err(|_| {
            invalid(
                "utf8_text",
                "path",
                path,
                format!(
                    "{path} is not UTF-8 text; extract it with fs.extract or read it with code.run"
                ),
                "a UTF-8 text file",
            )
        })?;
        Ok((text, sources))
    }

    /// The host path and sources of regular workspace file `path`: a
    /// materialized `/inputs` file, or a `/work` file within the work budget.
    ///
    /// # Errors
    /// Returns an [`invalid`] refusal: `workspace_path` for a path outside
    /// the workspace, `workspace_missing` for a missing file,
    /// `regular_file` for a directory or link, and `work_budget` for an
    /// oversized file.
    fn file(&self, path: &str) -> Result<(PathBuf, Value), ToolError> {
        if let Some(input) = self.inputs.get(path) {
            return Ok((
                self.host(path).ok_or_else(|| outside(path))?,
                json!(input.sources),
            ));
        }
        let prefix = format!("{}/", path.trim_end_matches('/'));
        let directory = || {
            invalid(
                "regular_file",
                "path",
                path,
                format!("{path} is a directory; list it with fs.list"),
                "a regular file",
            )
        };
        if prefix == "/inputs/"
            || prefix == "/work/"
            || self
                .inputs
                .range(prefix.clone()..)
                .next()
                .is_some_and(|(input, _)| input.starts_with(&prefix))
        {
            return Err(directory());
        }
        let host = self.host(path).ok_or_else(|| outside(path))?;
        let meta = relative(path, "/work/")
            .and_then(|_| host.symlink_metadata().ok())
            .ok_or_else(|| missing(path))?;
        if meta.is_dir() {
            return Err(directory());
        }
        if !meta.is_file() {
            return Err(invalid(
                "regular_file",
                "path",
                path,
                format!("{path} is not a regular file"),
                "a regular file",
            ));
        }
        if meta.len() > self.limits.work_write_max_bytes {
            return Err(invalid(
                "work_budget",
                "path",
                path,
                format!("{path} exceeds the work budget"),
                "a file within the /work write budget",
            ));
        }
        Ok((host, json!([])))
    }
}

/// The arguments of one `fs.search`.
#[derive(Debug, Default, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Search {
    /// The text, or with `regex` the pattern, a line must contain.
    pub query: String,
    /// Read `query` as Rust regex syntax instead of literal text.
    #[serde(default)]
    pub regex: bool,
    /// Match case exactly instead of ignoring it.
    #[serde(default)]
    pub case_sensitive: bool,
    /// One workspace file or directory to search instead of all of them.
    #[serde(default)]
    pub path: Option<String>,
    /// A filename pattern the searched files must match.
    #[serde(default)]
    pub glob: Option<String>,
}

/// What one `fs.search` scope names.
enum Scope {
    /// Exactly this workspace file.
    File(String),
    /// Every file under this `/`-terminated directory prefix.
    Dir(String),
}

/// `pattern` compiled as a filename glob, and whether it matches whole
/// relative paths (it holds a `/`) rather than basenames.
///
/// # Errors
/// Returns a `search_glob` [`invalid`] refusal for an absolute, `..`, or
/// malformed pattern.
fn glob(pattern: &str) -> Result<(glob::Pattern, bool), ToolError> {
    let refused = |message: String| {
        invalid(
            "search_glob",
            "glob",
            pattern,
            message,
            "a relative glob without .. segments",
        )
    };
    if pattern.starts_with('/') || pattern.split('/').any(|segment| segment == "..") {
        return Err(refused(format!(
            "the glob {pattern} must be relative without .. segments"
        )));
    }
    let compiled = glob::Pattern::new(pattern)
        .map_err(|error| refused(format!("the glob {pattern} is invalid: {error}")))?;
    Ok((compiled, pattern.contains('/')))
}

/// The relative remainder of `path` under `prefix` when every segment is
/// a plain name.
fn relative<'p>(path: &'p str, prefix: &str) -> Option<&'p str> {
    path.strip_prefix(prefix).filter(|rest| {
        rest.split('/')
            .all(|segment| !segment.is_empty() && segment != "." && segment != "..")
    })
}

/// Regular files under `dir` as `(relative path, size)`, never following
/// links, sorted by path; stops after visiting `most` entries and then
/// reports `true`.
fn walkdir(dir: &Path, most: usize) -> (Vec<(String, u64)>, bool) {
    let mut found = Vec::new();
    let mut visited = 0_usize;
    let mut pending = vec![dir.to_path_buf()];
    while let Some(next) = pending.pop() {
        let Ok(entries) = std::fs::read_dir(&next) else {
            continue;
        };
        for entry in entries.flatten() {
            visited += 1;
            if visited > most {
                found.sort();
                return (found, true);
            }
            let Ok(meta) = entry.path().symlink_metadata() else {
                continue;
            };
            if meta.is_dir() {
                pending.push(entry.path());
            } else if meta.is_file()
                && let Ok(rest) = entry.path().strip_prefix(dir)
            {
                found.push((rest.to_string_lossy().into_owned(), meta.len()));
            }
        }
    }
    found.sort();
    (found, false)
}

/// Most directory levels below a published directory's root.
// ponytail: fixed depth bound; make it a Deployment limit if trees need it.
const PUBLICATION_MAX_DEPTH: usize = 32;

/// A `/work` file or directory frozen for publication.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Frozen {
    /// One regular file.
    File {
        /// Its basename.
        name: String,
        /// Its bytes.
        bytes: Vec<u8>,
    },
    /// One directory tree, every path relative to its root.
    Directory {
        /// Every regular file and its bytes, sorted by path.
        files: Vec<(String, Vec<u8>)>,
        /// Every directory, empty ones included, sorted.
        directories: Vec<String>,
    },
}

/// A directory tree being frozen.
#[derive(Default)]
struct Tree {
    /// Regular files and their bytes.
    files: Vec<(String, Vec<u8>)>,
    /// Directories.
    directories: Vec<String>,
    /// Folded relative paths seen so far.
    folded: std::collections::HashSet<unicase::UniCase<String>>,
    /// File bytes so far.
    bytes: u64,
}

impl Tree {
    /// Admit entry `name` at `relative` under published `path`: a safe name,
    /// no collision under NFC plus case folding, and within `most` entries.
    ///
    /// # Errors
    /// Returns a `publication_tree` refusal naming the violated rule.
    fn admit(
        &mut self,
        path: &str,
        name: &str,
        relative: &str,
        most: usize,
    ) -> Result<(), ToolError> {
        use unicode_normalization::UnicodeNormalization as _;
        if name.contains('\\') || name.chars().any(char::is_control) {
            return Err(tree_refused(
                path,
                &format!("{relative:?} holds a backslash or control character"),
            ));
        }
        if !self
            .folded
            .insert(unicase::UniCase::unicode(relative.nfc().collect()))
        {
            return Err(tree_refused(
                path,
                &format!("{relative} collides with another entry ignoring case and normalization"),
            ));
        }
        if self.folded.len() > most {
            return Err(tree_refused(
                path,
                &format!("it holds more than {most} entries"),
            ));
        }
        Ok(())
    }
}

/// A directory publication refused because of `reason`.
fn tree_refused(path: &str, reason: &str) -> ToolError {
    invalid(
        "publication_tree",
        "path",
        path,
        format!("{path} cannot be published: {reason}"),
        "a tree of regular files and directories with distinct UTF-8 names",
    )
}

/// `text` in consecutive pieces of at most `size` bytes on character
/// boundaries; empty text is one empty piece. `size` must be at least 4,
/// the longest UTF-8 character.
fn pieces(text: &str, size: usize) -> impl Iterator<Item = &str> {
    let mut rest = Some(text);
    std::iter::from_fn(move || {
        let text = rest?;
        let (head, tail) = text.split_at(text.floor_char_boundary(size));
        rest = (!tail.is_empty()).then_some(tail);
        Some(head)
    })
}

/// JSON items gathered until their serialized size would overflow a tool
/// result whose fixed envelope is already accounted for.
struct Collected {
    /// Items that fit.
    items: Vec<Value>,
    /// Bytes still free for items and their separators.
    room: usize,
}

impl Collected {
    /// Room for items in a `max`-byte result shaped like `envelope`, whose
    /// item array is empty and whose other fields are at their largest.
    fn new(envelope: &Value, max: usize) -> Self {
        Self {
            items: Vec::new(),
            room: max.saturating_sub(envelope.to_string().len()),
        }
    }

    /// Bytes still free for items.
    const fn room(&self) -> usize {
        self.room
    }

    /// Keep `item` when its serialized size and separator fit; `false`
    /// once the result is full.
    fn push(&mut self, item: Value) -> bool {
        let size = item.to_string().len() + 1;
        if size > self.room {
            return false;
        }
        self.room -= size;
        self.items.push(item);
        true
    }
}

/// At most 160 bytes of `text` from 40 before the match at byte `at`,
/// cut on UTF-8 boundaries and independent of the match's length, so a
/// regex spanning a long line still yields a hit that fits the result.
fn snippet(text: &str, at: usize) -> String {
    let text = &text[text.floor_char_boundary(at.saturating_sub(40))..];
    text[..text.floor_char_boundary(160)].to_owned()
}

/// A terminal error with `reason`.
const fn terminal(reason: String) -> BlockError {
    BlockError::Terminal { reason }
}

/// A retryable local IO failure.
fn io(error: std::io::Error) -> BlockError {
    BlockError::Retryable {
        reason: format!("workspace IO failed: {error}"),
    }
}

/// A local IO failure during a tool call.
fn failed(error: std::io::Error) -> ToolError {
    invocation(format!("workspace IO failed: {error}"))
}

/// A tool call the model can repair: Validation naming the violated `rule`,
/// the argument `field`, its `value`, and what was `expected`.
fn invalid(rule: &str, field: &str, value: &str, message: String, expected: &str) -> ToolError {
    tool_error(&WyrdError::Validation {
        message,
        details: json!({"rule": rule, "field": field, "value": value, "expected": expected}),
    })
}

/// A path outside `/inputs` and `/work`, or with an unsafe segment.
fn outside(path: &str) -> ToolError {
    invalid(
        "workspace_path",
        "path",
        path,
        format!("{path} is not under /inputs or /work"),
        "a path under /inputs or /work without empty, . or .. segments",
    )
}

/// A workspace path naming no file or directory.
fn missing(path: &str) -> ToolError {
    invalid(
        "workspace_missing",
        "path",
        path,
        format!("{path} does not exist"),
        "a path fs.list reports",
    )
}

#[cfg(test)]
mod pg_tests;
#[cfg(test)]
mod tests;
