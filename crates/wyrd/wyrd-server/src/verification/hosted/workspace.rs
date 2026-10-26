//! Bounded per-call scratch workspaces for Agent calls and code.
//!
//! [`Workspaces`] admits at most [`HostedLimits::max_workspaces`] concurrent
//! workspaces per process and refuses, rather than queues, the next one. [`Workspaces::open`] reads each declared file
//! through [`TenantEvidence`], then materializes it under a fresh temporary
//! directory: files and JSON under `inputs/`, uploaded evidence declared as
//! an archive unpacked in place after [`zip_guard`] or the equivalent tar.gz
//! checks, and an empty `work/`. Artifact files are staged unchanged. No
//! file is parsed when mounted: `fs.read` and `fs.search` serve the actual
//! UTF-8 lines of a file, and only `fs.extract` runs the extractor, on
//! request, into a new `/work` file. Every `/inputs` file keeps the
//! original slots it came from, so tool output stays citable against
//! original evidence. Dropping the [`Workspace`] removes the directory and
//! releases its admission, on success, failure, or cancellation alike.

use std::collections::{BTreeMap, BTreeSet};
use std::io::{Cursor, Read};
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

use monty_pool::{MountSpec, MountSpecMode};
use serde_json::{Value, json};
use skald_spec::MediaFormat;
use tempfile::TempDir;
use tokio::sync::{OwnedSemaphorePermit, Semaphore};
use vala_verify::blocks::{BlockError, CallInput, CallValue};
use wyrd_loader::{EXTRACT_VERSION, Extraction, lines, zip_guard};
use wyrd_spec::ids::SlotName;
use wyrd_spec::verification::execution::InputDeclaration;

use super::evidence::TenantEvidence;
use super::{HostedLimits, extract_refused};

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
    /// Admission never waits: with every workspace taken the open is refused
    /// at once, so no waiter queue can grow toward execution deadlines and
    /// the execution is deferred instead. Evidence is read and verified
    /// first, then written on the blocking pool. An Artifact file keeps its
    /// stored content type only as a hint for `fs.extract`; it is never
    /// unpacked.
    ///
    /// # Errors
    /// Returns [`BlockError::Busy`] when every workspace is taken, the
    /// evidence reader's errors, [`BlockError::Terminal`] for an invalid
    /// path, combined input bytes over the limit, or any materialization
    /// refusal, and [`BlockError::Retryable`] when the blocking task fails.
    pub async fn open(
        &self,
        files: &BTreeMap<String, CallInput>,
        evidence: &TenantEvidence,
    ) -> Result<Workspace, BlockError> {
        let permit =
            Arc::clone(&self.permits)
                .try_acquire_owned()
                .map_err(|_| BlockError::Busy {
                    reason: "every workspace is in use".to_owned(),
                })?;
        let mut staged = Vec::with_capacity(files.len());
        let mut total = 0_u64;
        for (path, input) in files {
            let (bytes, format) = match &input.value {
                CallValue::Json(value) => (
                    serde_json::to_vec_pretty(value).unwrap_or_default(),
                    Some(MediaFormat::Json),
                ),
                CallValue::Evidence(InputDeclaration::Artifact { artifact, path }) => {
                    let (bytes, format) = evidence.artifact(artifact, path.as_deref()).await?;
                    (bytes, format.filter(|format| !format.is_archive()))
                }
                CallValue::Evidence(declaration) => {
                    let (bytes, format) = evidence.read(declaration).await?;
                    (bytes, Some(format))
                }
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
                format,
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
    /// The declared or stored format, the format `fs.extract` reads it as.
    format: Option<MediaFormat>,
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
    /// Write `staged` into a fresh directory.
    ///
    /// # Errors
    /// Returns [`BlockError::Terminal`] for an invalid path, a duplicate
    /// path, an unsafe or oversized archive, or scratch over the limit, and
    /// [`BlockError::Retryable`] for a local IO failure.
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

    /// Write one file at virtual `path`, unparsed.
    ///
    /// # Errors
    /// Returns [`BlockError::Terminal`] for a duplicate path or scratch over
    /// the limit, and [`BlockError::Retryable`] for a local IO failure.
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
        let host = self.host(&path)?;
        if let Some(parent) = host.parent() {
            std::fs::create_dir_all(parent).map_err(io)?;
        }
        std::fs::write(&host, &bytes).map_err(io)?;
        self.inputs.insert(
            path,
            Input {
                sources: sources.clone(),
                format,
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
        zip_guard(&file.bytes, &self.limits.extract()).map_err(extract_refused)?;
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
    fn work_room(&self) -> u64 {
        let held: u64 = walkdir(&self.work_dir(), usize::MAX)
            .0
            .iter()
            .map(|(_, size)| size)
            .sum();
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
    /// Returns [`BlockError::Terminal`] for a directory, an unknown, linked,
    /// or non-UTF-8 path, or a path too long for any piece to fit the limit.
    pub fn read(&self, path: &str, start: usize) -> Result<Value, BlockError> {
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
    /// Returns [`BlockError::Terminal`] for an empty query, an invalid
    /// pattern or glob, an absolute or `..` glob, or a missing scope.
    pub fn search(&self, search: &Search) -> Result<Value, BlockError> {
        if search.query.is_empty() {
            return Err(terminal("the search query is empty".to_owned()));
        }
        let pattern = if search.regex {
            search.query.clone()
        } else {
            regex::escape(&search.query)
        };
        let matcher = regex::RegexBuilder::new(&pattern)
            .case_insensitive(!search.case_sensitive)
            .build()
            .map_err(|error| terminal(format!("the search pattern is invalid: {error}")))?;
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
                    "snippet": snippet(&line, found.start(), found.len()),
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
    /// Returns [`BlockError::Terminal`] for a directory, unknown, or
    /// unextractable file, an extraction refusal, an output path outside
    /// `/work` or already present, or an output over the `/work` budget, and
    /// [`BlockError::Retryable`] for a local IO failure.
    pub fn extract(&self, path: &str, output_path: &str) -> Result<Value, BlockError> {
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
            .ok_or_else(|| terminal(format!("{path} is not an extractable document")))?;
        let rest = relative(output_path, "/work/")
            .ok_or_else(|| terminal(format!("{output_path} is not a new file path under /work")))?;
        let target = self.work_dir().join(rest);
        if target.symlink_metadata().is_ok() {
            return Err(terminal(format!("{output_path} already exists")));
        }
        let bytes = std::fs::read(&host).map_err(io)?;
        let output = Extraction::new(&self.limits.extract())
            .run(&bytes, format)
            .map_err(extract_refused)?;
        let json = serde_json::to_vec_pretty(&output)
            .map_err(|error| terminal(format!("the extraction does not serialize: {error}")))?;
        if u64::try_from(json.len()).unwrap_or(u64::MAX) > self.work_room() {
            return Err(terminal(format!(
                "{output_path} would exceed the /work write budget"
            )));
        }
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent).map_err(io)?;
            let confined = parent
                .canonicalize()
                .ok()
                .zip(self.work_dir().canonicalize().ok())
                .is_some_and(|(parent, work)| parent.starts_with(work));
            if !confined {
                return Err(terminal(format!("{output_path} escapes /work")));
            }
        }
        let mut staged = tempfile::NamedTempFile::new_in(self.root.path()).map_err(io)?;
        std::io::Write::write_all(&mut staged, &json).map_err(io)?;
        staged.persist_noclobber(&target).map_err(|error| {
            if error.error.kind() == std::io::ErrorKind::AlreadyExists {
                terminal(format!("{output_path} already exists"))
            } else {
                io(error.error)
            }
        })?;
        Ok(json!({
            "output_path": output_path,
            "sources": sources,
            "extractor_version": EXTRACT_VERSION,
        }))
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
    /// Returns [`BlockError::Terminal`] for a path naming neither.
    fn scope(&self, path: &str) -> Result<Scope, BlockError> {
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
            Err(terminal(format!("{path} does not exist")))
        }
    }

    /// The UTF-8 text and sources of workspace file `path`.
    ///
    /// # Errors
    /// Returns [`Self::file`]'s errors and [`BlockError::Terminal`] for
    /// bytes that are not UTF-8 text.
    fn text(&self, path: &str) -> Result<(String, Value), BlockError> {
        let (host, sources) = self.file(path)?;
        let text = String::from_utf8(std::fs::read(&host).map_err(io)?).map_err(|_| {
            terminal(format!(
                "{path} is not UTF-8 text; extract it with fs.extract or read it with code.run"
            ))
        })?;
        Ok((text, sources))
    }

    /// The host path and sources of regular workspace file `path`: a
    /// materialized `/inputs` file, or a `/work` file within the work budget.
    ///
    /// # Errors
    /// Returns [`BlockError::Terminal`] for a directory, a link, or a missing
    /// or oversized file.
    fn file(&self, path: &str) -> Result<(PathBuf, Value), BlockError> {
        if let Some(input) = self.inputs.get(path) {
            return Ok((self.host(path)?, json!(input.sources)));
        }
        let host = relative(path, "/work/")
            .map(|_| self.host(path))
            .transpose()?
            .filter(|host| host.symlink_metadata().is_ok());
        let Some(host) = host else {
            let prefix = format!("{}/", path.trim_end_matches('/'));
            if prefix == "/inputs/"
                || prefix == "/work/"
                || self
                    .inputs
                    .range(prefix.clone()..)
                    .next()
                    .is_some_and(|(input, _)| input.starts_with(&prefix))
            {
                return Err(terminal(format!(
                    "{path} is a directory; list it with fs.list"
                )));
            }
            return Err(terminal(format!("{path} does not exist")));
        };
        let meta = host.symlink_metadata().map_err(io)?;
        if meta.is_dir() {
            return Err(terminal(format!(
                "{path} is a directory; list it with fs.list"
            )));
        }
        if !meta.is_file() {
            return Err(terminal(format!("{path} is not a regular file")));
        }
        if meta.len() > self.limits.work_write_max_bytes {
            return Err(terminal(format!("{path} exceeds the work budget")));
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
/// Returns [`BlockError::Terminal`] for an absolute, `..`, or malformed
/// pattern.
fn glob(pattern: &str) -> Result<(glob::Pattern, bool), BlockError> {
    if pattern.starts_with('/') || pattern.split('/').any(|segment| segment == "..") {
        return Err(terminal(format!(
            "the glob {pattern} must be relative without .. segments"
        )));
    }
    let compiled = glob::Pattern::new(pattern)
        .map_err(|error| terminal(format!("the glob {pattern} is invalid: {error}")))?;
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
mod pg_tests;
#[cfg(test)]
mod tests;
