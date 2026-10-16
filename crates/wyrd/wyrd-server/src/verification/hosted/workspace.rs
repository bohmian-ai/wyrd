//! Bounded per-call scratch workspaces for step Agents and code.
//!
//! [`Workspaces`] admits at most [`HostedLimits::max_workspaces`] concurrent
//! workspaces per process. [`Workspaces::open`] reads each declared file
//! through [`TenantEvidence`], then materializes it under a fresh temporary
//! directory: files and JSON under `inputs/`, archives unpacked in place
//! after [`zip_guard`] or the equivalent tar.gz checks, and an empty
//! `work/`. Every `/inputs` file is extracted once into the located units
//! that `fs.read` and `fs.search` serve and keeps the original slots it came
//! from, so tool output stays citable against original evidence. Dropping
//! the [`Workspace`] removes the directory and releases its admission, on
//! success, failure, or cancellation alike.

use std::collections::{BTreeMap, BTreeSet};
use std::io::{Cursor, Read};
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

use monty_pool::{MountSpec, MountSpecMode};
use serde_json::{Value, json};
use skald_spec::MediaFormat;
use tempfile::TempDir;
use tokio::sync::{OwnedSemaphorePermit, Semaphore};
use vala_eval::blocks::{BlockError, CallInput, CallValue};
use wyrd_spec::ids::SlotName;
use wyrd_spec::verification::execution::Locator;

use super::HostedLimits;
use super::evidence::TenantEvidence;
use super::extract::{Extraction, units, zip_guard};

/// Process-wide admission of scratch workspaces.
pub struct Workspaces {
    /// One permit per concurrent workspace.
    permits: Arc<Semaphore>,
    /// Limits every workspace enforces.
    limits: HostedLimits,
}

impl Workspaces {
    /// Admission of at most `limits.max_workspaces` concurrent workspaces.
    #[must_use]
    pub fn new(limits: HostedLimits) -> Self {
        Self {
            permits: Arc::new(Semaphore::new(limits.max_workspaces)),
            limits,
        }
    }

    /// Materialize `files`, keyed by `/inputs/...` path, for one invocation.
    ///
    /// Waits for admission; the engine's deadline cancels the wait by
    /// dropping this future. Evidence is read and verified first, then
    /// written and extracted on the blocking pool.
    ///
    /// # Errors
    /// Returns the evidence reader's errors, [`BlockError::Terminal`] for an
    /// invalid path, combined input bytes over the limit, or any
    /// materialization refusal, and [`BlockError::Retryable`] when the
    /// blocking task fails.
    pub async fn open(
        &self,
        files: &BTreeMap<String, CallInput>,
        evidence: &TenantEvidence,
    ) -> Result<Workspace, BlockError> {
        let permit = Arc::clone(&self.permits)
            .acquire_owned()
            .await
            .map_err(|_| terminal("workspace admission is closed".to_owned()))?;
        let mut staged = Vec::with_capacity(files.len());
        let mut total = 0_u64;
        for (path, input) in files {
            let (bytes, format) = match &input.value {
                CallValue::Json(value) => (
                    serde_json::to_vec_pretty(value).unwrap_or_default(),
                    MediaFormat::Json,
                ),
                CallValue::Evidence(declaration) => evidence.read(declaration).await?,
            };
            total = total.saturating_add(u64::try_from(bytes.len()).unwrap_or(u64::MAX));
            if total > self.limits.files_max_bytes {
                return Err(terminal(format!(
                    "workspace inputs exceed the {}-byte combined file limit",
                    self.limits.files_max_bytes
                )));
            }
            staged.push(Staged {
                path: path.clone(),
                bytes,
                format: Some(format),
                sources: input.sources.clone(),
            });
        }
        let limits = self.limits;
        tokio::task::spawn_blocking(move || Workspace::materialize(staged, limits, permit))
            .await
            .map_err(|error| BlockError::Retryable {
                reason: format!("workspace materialization stopped: {error}"),
            })?
    }
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

/// One materialized `/inputs` file.
struct Input {
    /// Original slots behind the file.
    sources: BTreeSet<SlotName>,
    /// Located text units; `None` for binary files with no text.
    units: Option<Vec<(Locator, String)>>,
}

/// One invocation's scratch directory.
pub struct Workspace {
    /// Root holding `inputs/` and `work/`; removed on drop.
    root: TempDir,
    /// Materialized `/inputs` files by virtual path.
    inputs: BTreeMap<String, Input>,
    /// Bytes the materialized inputs take from the scratch limit.
    inputs_bytes: u64,
    /// Held by one `/work` writer at a time, so each sees the space the
    /// others left.
    writer: tokio::sync::Mutex<()>,
    /// Limits in force.
    limits: HostedLimits,
    /// Admission held until drop.
    _permit: Option<OwnedSemaphorePermit>,
}

impl Workspace {
    /// Write and extract `staged` into a fresh directory.
    ///
    /// # Errors
    /// Returns [`BlockError::Terminal`] for an invalid path, a duplicate
    /// path, an unsafe or oversized archive, scratch over the limit, or an
    /// extraction refusal, and [`BlockError::Retryable`] for a local IO
    /// failure.
    pub fn materialize(
        staged: Vec<Staged>,
        limits: HostedLimits,
        permit: impl Into<Option<OwnedSemaphorePermit>>,
    ) -> Result<Self, BlockError> {
        let root = tempfile::tempdir().map_err(io)?;
        std::fs::create_dir(root.path().join("inputs")).map_err(io)?;
        std::fs::create_dir(root.path().join("work")).map_err(io)?;
        let mut workspace = Self {
            root,
            inputs: BTreeMap::new(),
            inputs_bytes: 0,
            writer: tokio::sync::Mutex::new(()),
            limits,
            _permit: permit.into(),
        };
        let mut scratch = 0_u64;
        for file in staged {
            relative(&file.path, "/inputs/")
                .ok_or_else(|| terminal(format!("{} is not an /inputs path", file.path)))?;
            match file.format {
                Some(MediaFormat::Zip) => workspace.unzip(&file, &mut scratch)?,
                Some(MediaFormat::TarGz) => workspace.untar(&file, &mut scratch)?,
                _ => workspace.place(
                    file.path,
                    file.bytes,
                    file.format,
                    &file.sources,
                    &mut scratch,
                )?,
            }
        }
        workspace.inputs_bytes = scratch;
        Ok(workspace)
    }

    /// Write one file at virtual `path` and extract its units.
    ///
    /// # Errors
    /// Returns [`BlockError::Terminal`] for a duplicate path, scratch over the
    /// limit, or an extraction refusal of a known format, and
    /// [`BlockError::Retryable`] for a local IO failure.
    fn place(
        &mut self,
        path: String,
        bytes: Vec<u8>,
        format: Option<MediaFormat>,
        sources: &BTreeSet<SlotName>,
        scratch: &mut u64,
    ) -> Result<(), BlockError> {
        if self.inputs.contains_key(&path) {
            return Err(terminal(format!("{path} is materialized twice")));
        }
        *scratch = scratch.saturating_add(u64::try_from(bytes.len()).unwrap_or(u64::MAX));
        if *scratch > self.limits.scratch_max_bytes {
            return Err(terminal(format!(
                "the workspace exceeds the {}-byte scratch limit",
                self.limits.scratch_max_bytes
            )));
        }
        let format = format.or_else(|| {
            std::str::from_utf8(&bytes)
                .is_ok()
                .then_some(MediaFormat::Text)
        });
        let units = match format {
            Some(format) if format.is_extractable() => {
                let output = Extraction::new(&self.limits).run(&bytes, format)?;
                Some(units(&output, &bytes))
            }
            _ => None,
        };
        let host = self.host(&path)?;
        if let Some(parent) = host.parent() {
            std::fs::create_dir_all(parent).map_err(io)?;
        }
        std::fs::write(&host, &bytes).map_err(io)?;
        self.inputs.insert(
            path,
            Input {
                sources: sources.clone(),
                units,
            },
        );
        Ok(())
    }

    /// Unpack a zip archive under its path after [`zip_guard`].
    ///
    /// # Errors
    /// Returns [`BlockError::Terminal`] for a guarded limit, an entry that is
    /// a link or escapes the archive, or a file over the file limit.
    fn unzip(&mut self, file: &Staged, scratch: &mut u64) -> Result<(), BlockError> {
        zip_guard(&file.bytes, &self.limits)?;
        let mut archive = zip::ZipArchive::new(Cursor::new(file.bytes.as_slice()))
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
            let bytes = self.bounded(&mut entry, &file.path)?;
            self.place_entry(file, &name, bytes, scratch)?;
        }
        Ok(())
    }

    /// Unpack a tar.gz archive under its path, admitting only regular files
    /// and directories, under the entry, byte, and ratio limits.
    ///
    /// # Errors
    /// Returns [`BlockError::Terminal`] for an unreadable archive, a link or
    /// special entry, an escaping path, or a limit.
    fn untar(&mut self, file: &Staged, scratch: &mut u64) -> Result<(), BlockError> {
        let compressed = u64::try_from(file.bytes.len()).unwrap_or(u64::MAX);
        let budget = self
            .limits
            .archive_max_bytes
            .min(compressed.saturating_mul(self.limits.archive_max_ratio));
        let unreadable = |error: std::io::Error| terminal(format!("{}: {error}", file.path));
        // The decoder is capped, so a header understating sizes cannot
        // inflate the stream past the archive budget.
        let mut decoder = flate2::read::GzDecoder::new(file.bytes.as_slice()).take(budget + 1);
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
            let bytes = self.bounded(&mut entry, &file.path)?;
            self.place_entry(file, &name, bytes, scratch)?;
        }
        if decoder.limit() == 0 {
            return Err(terminal(format!(
                "{} decompresses past its {budget}-byte limit",
                file.path
            )));
        }
        Ok(())
    }

    /// Read one archive entry, refusing it past the file limit.
    ///
    /// # Errors
    /// Returns [`BlockError::Terminal`] for an unreadable or oversized entry.
    fn bounded(&self, entry: &mut impl Read, archive: &str) -> Result<Vec<u8>, BlockError> {
        let mut bytes = Vec::new();
        entry
            .take(self.limits.file_max_bytes + 1)
            .read_to_end(&mut bytes)
            .map_err(|error| terminal(format!("{archive}: {error}")))?;
        if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > self.limits.file_max_bytes {
            return Err(terminal(format!(
                "{archive} has an entry over the file limit"
            )));
        }
        Ok(bytes)
    }

    /// Place an archive entry at `<archive path>/<name>`, typed by extension.
    ///
    /// # Errors
    /// Returns [`Self::place`]'s errors.
    fn place_entry(
        &mut self,
        archive: &Staged,
        name: &Path,
        bytes: Vec<u8>,
        scratch: &mut u64,
    ) -> Result<(), BlockError> {
        let name = name.to_string_lossy();
        let format = name
            .strip_suffix(".tar.gz")
            .map(|_| MediaFormat::TarGz)
            .or_else(|| name.rsplit_once('.').and_then(|(_, ext)| ext.parse().ok()))
            .filter(|format: &MediaFormat| !format.is_archive());
        self.place(
            format!("{}/{name}", archive.path),
            bytes,
            format,
            &archive.sources,
            scratch,
        )
    }

    /// The host path of a virtual `/inputs` or `/work` path.
    ///
    /// # Errors
    /// Returns [`BlockError::Terminal`] for any other or unsafe path.
    fn host(&self, path: &str) -> Result<PathBuf, BlockError> {
        let (dir, rest) = relative(path, "/inputs/")
            .map(|rest| ("inputs", rest))
            .or_else(|| relative(path, "/work/").map(|rest| ("work", rest)))
            .ok_or_else(|| terminal(format!("{path} is not under /inputs or /work")))?;
        Ok(self.root.path().join(dir).join(rest))
    }

    /// The host directory mounted at `/inputs`.
    #[must_use]
    pub fn inputs_dir(&self) -> PathBuf {
        self.root.path().join("inputs")
    }

    /// The host directory mounted at `/work`.
    #[must_use]
    pub fn work_dir(&self) -> PathBuf {
        self.root.path().join("work")
    }

    /// Wait to be the only `/work` writer; dropping the guard lets the next
    /// one in. Cancelling the wait gives up the turn.
    pub async fn writer(&self) -> tokio::sync::MutexGuard<'_, ()> {
        self.writer.lock().await
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
        let held: u64 = walkdir(&self.work_dir(), usize::MAX)
            .0
            .iter()
            .map(|(_, size)| size)
            .sum();
        let unopened = |error: monty_pool::PoolError| BlockError::Retryable {
            reason: format!("a workspace mount failed: {error}"),
        };
        let mut work =
            MountSpec::new("/work", self.work_dir(), MountSpecMode::ReadWrite).map_err(unopened)?;
        work.write_bytes_limit = Some(
            self.limits
                .work_write_max_bytes
                .min(
                    self.limits
                        .scratch_max_bytes
                        .saturating_sub(self.inputs_bytes),
                )
                .saturating_sub(held),
        );
        Ok(vec![
            MountSpec::new("/inputs", self.inputs_dir(), MountSpecMode::ReadOnly)
                .map_err(unopened)?,
            work,
        ])
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
    /// Returns [`BlockError::Terminal`] for a path outside the workspace.
    pub fn list(&self, dir: &str) -> Result<Value, BlockError> {
        let prefix = format!("{}/", dir.trim_end_matches('/'));
        if prefix != "/inputs/" && prefix != "/work/" {
            self.host(&prefix)?;
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
            let size = std::fs::metadata(self.host(path)?).map_or(0, |meta| meta.len());
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

    /// `fs.read`: located text units of `path` from piece `start`, bounded
    /// by the tool result limit by their actual JSON size; `next` resumes.
    ///
    /// Units longer than an eighth of the result budget are split into
    /// pieces that keep the unit's original locator, so every character of
    /// a long line or page is reachable by following `next`. Piece indices
    /// depend only on the file and the limits. `sources` names the original
    /// slots of an `/inputs` file and is empty for `/work` files.
    ///
    /// # Errors
    /// Returns [`BlockError::Terminal`] for an unknown, binary, linked, or
    /// unreadable path, or a path too long for any piece to fit the limit.
    pub fn read(&self, path: &str, start: usize) -> Result<Value, BlockError> {
        let owned;
        let (units, sources) = if let Some(input) = self.inputs.get(path) {
            let units = input.units.as_deref().ok_or_else(|| {
                terminal(format!("{path} is binary; bind it as native media instead"))
            })?;
            (units, json!(input.sources))
        } else {
            owned = self.work_units(path)?;
            (owned.as_slice(), json!([]))
        };
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
                    return Err(terminal(format!(
                        "{path} cannot be read within the {}-byte tool result limit",
                        self.limits.tool_result_max_bytes
                    )));
                }
                next = Some(index);
                break;
            }
        }
        Ok(json!({"path": path, "sources": sources, "units": out.items, "next": next}))
    }

    /// `fs.search`: case-insensitive literal matches of `query` with their
    /// locators and original slots, across `/inputs` and text files in
    /// `/work`, bounded by the tool result limit.
    ///
    /// Files are visited lazily and the `/work` walk is capped like
    /// [`Self::list`], so the search stops as soon as the result is full.
    ///
    /// # Errors
    /// Returns [`BlockError::Terminal`] for an empty query.
    pub fn search(&self, query: &str) -> Result<Value, BlockError> {
        let needle = query.to_lowercase();
        if needle.is_empty() {
            return Err(terminal("the search query is empty".to_owned()));
        }
        let mut matches = Collected::new(
            &json!({"matches": [], "truncated": false}),
            self.limits.tool_result_max_bytes,
        );
        let (work, cut) = walkdir(&self.work_dir(), self.walk_limit());
        let inputs = self.inputs.iter().filter_map(|(path, input)| {
            Some((
                path.clone(),
                json!(input.sources),
                std::borrow::Cow::Borrowed(input.units.as_deref()?),
            ))
        });
        let work = work.into_iter().filter_map(|(path, _)| {
            let path = format!("/work/{path}");
            let units = self.work_units(&path).ok()?;
            Some((path, json!([]), std::borrow::Cow::Owned(units)))
        });
        for (path, sources, units) in inputs.chain(work) {
            for (locator, text) in units.iter() {
                if let Some(at) = text.to_lowercase().find(&needle) {
                    let found = json!({
                        "path": path,
                        "locator": locator,
                        "snippet": snippet(text, at, needle.len()),
                        "sources": sources,
                    });
                    if !matches.push(found) {
                        return Ok(json!({"matches": matches.items, "truncated": true}));
                    }
                }
            }
        }
        Ok(json!({"matches": matches.items, "truncated": cut}))
    }

    /// Most `/work` entries a list or search visits: no result could show
    /// more, since every listed entry takes more than 16 bytes.
    const fn walk_limit(&self) -> usize {
        self.limits.tool_result_max_bytes / 16
    }

    /// Line units of a UTF-8 `/work` file.
    ///
    /// # Errors
    /// Returns [`BlockError::Terminal`] for a path outside `/work`, a link,
    /// a missing, oversized, or non-UTF-8 file.
    fn work_units(&self, path: &str) -> Result<Vec<(Locator, String)>, BlockError> {
        relative(path, "/work/").ok_or_else(|| terminal(format!("{path} does not exist")))?;
        let host = self.host(path)?;
        let meta = std::fs::symlink_metadata(&host)
            .map_err(|_| terminal(format!("{path} does not exist")))?;
        if !meta.is_file() {
            return Err(terminal(format!("{path} is not a regular file")));
        }
        if meta.len() > self.limits.work_write_max_bytes {
            return Err(terminal(format!("{path} exceeds the work budget")));
        }
        let bytes = std::fs::read(&host).map_err(io)?;
        let output = Extraction::new(&self.limits).run(&bytes, MediaFormat::Text)?;
        Ok(units(&output, &bytes))
    }
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

/// About 80 bytes of `text` around the match at byte `at`.
fn snippet(text: &str, at: usize, len: usize) -> String {
    let text = &text[text.floor_char_boundary(at.saturating_sub(40))..];
    text[..text.floor_char_boundary(80 + len)].to_owned()
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

#[cfg(test)]
mod tests;
