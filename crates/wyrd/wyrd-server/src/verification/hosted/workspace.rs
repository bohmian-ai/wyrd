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

    /// Original slots behind `/inputs` file `path`.
    #[must_use]
    pub fn sources(&self, path: &str) -> Option<&BTreeSet<SlotName>> {
        self.inputs.get(path).map(|input| &input.sources)
    }

    /// `fs.list`: every file under virtual directory `dir`, with its size.
    ///
    /// # Errors
    /// Returns [`BlockError::Terminal`] for a path outside the workspace.
    pub fn list(&self, dir: &str) -> Result<Value, BlockError> {
        let prefix = format!("{}/", dir.trim_end_matches('/'));
        if prefix != "/inputs/" && prefix != "/work/" {
            self.host(&prefix)?;
        }
        let mut files: Vec<Value> = Vec::new();
        for path in self.inputs.keys().filter(|path| path.starts_with(&prefix)) {
            let size = std::fs::metadata(self.host(path)?).map_or(0, |meta| meta.len());
            files.push(json!({"path": path, "size_bytes": size}));
        }
        if "/work/".starts_with(&prefix) || prefix.starts_with("/work/") {
            for entry in walkdir(&self.work_dir()) {
                let virtual_path = format!("/work/{}", entry.0);
                if virtual_path.starts_with(&prefix) {
                    files.push(json!({"path": virtual_path, "size_bytes": entry.1}));
                }
            }
        }
        Ok(json!({ "files": files }))
    }

    /// `fs.read`: located text units of `path` from unit `start`, bounded by
    /// the tool result limit; `next` resumes a truncated read.
    ///
    /// # Errors
    /// Returns [`BlockError::Terminal`] for an unknown, binary, linked, or
    /// unreadable path.
    pub fn read(&self, path: &str, start: usize) -> Result<Value, BlockError> {
        let owned;
        let units = if let Some(input) = self.inputs.get(path) {
            input.units.as_deref().ok_or_else(|| {
                terminal(format!("{path} is binary; bind it as native media instead"))
            })?
        } else {
            owned = self.work_units(path)?;
            &owned
        };
        let budget = self.limits.tool_result_max_bytes.saturating_sub(256);
        let mut used = 0;
        let mut out = Vec::new();
        let mut next = None;
        for (index, (locator, text)) in units.iter().enumerate().skip(start) {
            used += text.len() + 64;
            if used > budget && !out.is_empty() {
                next = Some(index);
                break;
            }
            let text = if used > budget {
                truncate(text, budget)
            } else {
                text.as_str()
            };
            out.push(json!({"locator": locator, "text": text}));
        }
        Ok(json!({"path": path, "units": out, "next": next}))
    }

    /// `fs.search`: case-insensitive literal matches of `query` with their
    /// locators, across `/inputs` and text files in `/work`, bounded by the
    /// tool result limit.
    ///
    /// # Errors
    /// Returns [`BlockError::Terminal`] for an empty query.
    pub fn search(&self, query: &str) -> Result<Value, BlockError> {
        let needle = query.to_lowercase();
        if needle.is_empty() {
            return Err(terminal("the search query is empty".to_owned()));
        }
        let mut used = 0;
        let mut matches = Vec::new();
        let work: Vec<(String, Vec<(Locator, String)>)> = walkdir(&self.work_dir())
            .into_iter()
            .filter_map(|(path, _)| {
                let path = format!("/work/{path}");
                self.work_units(&path).ok().map(|units| (path, units))
            })
            .collect();
        let inputs = self
            .inputs
            .iter()
            .filter_map(|(path, input)| Some((path.as_str(), input.units.as_deref()?)));
        let all = inputs.chain(
            work.iter()
                .map(|(path, units)| (path.as_str(), units.as_slice())),
        );
        for (path, units) in all {
            for (locator, text) in units {
                if let Some(at) = text.to_lowercase().find(&needle) {
                    let snippet = snippet(text, at, needle.len());
                    used += path.len() + snippet.len() + 96;
                    if used > self.limits.tool_result_max_bytes {
                        return Ok(json!({"matches": matches, "truncated": true}));
                    }
                    matches.push(json!({"path": path, "locator": locator, "snippet": snippet}));
                }
            }
        }
        Ok(json!({"matches": matches, "truncated": false}))
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
/// links.
fn walkdir(dir: &Path) -> Vec<(String, u64)> {
    let mut found = Vec::new();
    let mut pending = vec![dir.to_path_buf()];
    while let Some(next) = pending.pop() {
        let Ok(entries) = std::fs::read_dir(&next) else {
            continue;
        };
        for entry in entries.flatten() {
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
    found
}

/// `text` cut to at most `max` bytes on a character boundary.
fn truncate(text: &str, max: usize) -> &str {
    let mut end = max.min(text.len());
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    &text[..end]
}

/// About 80 bytes of `text` around the match at byte `at`.
fn snippet(text: &str, at: usize, len: usize) -> String {
    let mut start = at.saturating_sub(40).min(text.len());
    while !text.is_char_boundary(start) {
        start -= 1;
    }
    truncate(&text[start..], 80 + len).to_owned()
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
