//! Optional diagnostic CPU and heap profiles of the server under test and
//! of the driver measuring it.
//!
//! `--profile [cpu|heap|both]` selects the collectors; a bare `--profile`
//! is `both` ([`ProfileMode`]). Each runs for both process roles, the
//! server and this driver, and keeps their captures apart by role.
//!
//! CPU profiles are captured by external `samply`, one capture per process
//! per measured step, started before the step's warmup and interrupted
//! after its last repetition, with the same invocation on every supported
//! OS: `samply record --save-only --unstable-presymbolicate --output
//! <dir>/<step>/cpu-<role>-<pid>.json.gz --rate <hertz> --pid <pid>`. A
//! capture spans the step's warmup, repetitions, and the gaps between them;
//! nothing aligns its samples to one repetition. Samply samples through
//! perf events on Linux and natively on macOS; the harness neither runs
//! `perf` nor changes OS permissions or limits to obtain a profile.
//!
//! Samply symbolizes each capture as it saves, while the sampled
//! executables still exist, into a `cpu-<role>-<pid>.json.syms.json`
//! sidecar of every sampled address's function and inline frames; `samply
//! load` reads it on any machine, and [`SymbolizedCapture`] reads it for
//! text reports. Each sampled executable is also kept once per build ID
//! under [`SYMBOLS`], so a capture can be symbolized again after a rebuild.
//! A capture is complete only when its sidecar resolves its executable's
//! frames ([`Symbolized`]); otherwise it is kept and noted.
//!
//! Heap profiles come from each process's own sampler,
//! [`wyrd_benchmark::heap`], which a `bench` server and this driver report
//! their allocations to: [`HeapProfiler::start`] starts both, and each
//! measured step ends with one pprof capture per role,
//! `heap-<role>-<step>.pb.gz`, of the memory the steps so far allocated and
//! of what they still hold.
//!
//! A Python or TypeScript client process the driver starts is captured by
//! the same samply invocation under its own role. The workload starts it
//! with its runtime's perf map (Python's `-X perf`, Node's
//! `--perf-basic-prof`), from which samply names interpreted and JIT frames
//! beside the native ones. Its heap captures come from the process itself,
//! saved by the workload ([`crate::experiment::Workload::capture_clients`]):
//! its runtime's managed-heap tool and, from a `bench` native module, the
//! same shared sampler's pprof of the module's native allocations.
//!
//! A profile is diagnostic evidence only. A missing profiler, a capture that
//! fails to start, exits early, or leaves no file yields a [`ProfileNote`]
//! naming why; it never changes a measurement, a criterion, or a verdict, and
//! never supplies the CPU denominator, which comes from the OS's process
//! readings. Captures stay under `target/`; a record keeps only their
//! relative paths.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::experiment::Role;

/// The profiler a `--profile` run invokes.
pub const SAMPLY: &str = "samply";

/// Sampling frequency of a capture, hertz: high, because a single-client
/// step spends only a few milliseconds per request.
pub const PROFILE_HERTZ: u32 = 999;

/// How long an interrupted profiler may take to save and symbolize its
/// capture before it is killed and the capture is noted incomplete.
const SAVE_GRACE: Duration = Duration::from_secs(180);

/// Where `--profile` runs keep the executables their CPU captures sampled,
/// relative to the repository: `<build-id>/<executable>`, one copy per build.
pub const SYMBOLS: &str = "target/benchmarks/symbols";

/// The largest share, in percent, of a capture's samples in its executable
/// that may hold a frame its sidecar leaves unresolved.
const UNRESOLVED_PERCENT: u64 = 5;

/// The collectors a `--profile` run starts, for both process roles.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProfileMode {
    /// CPU sampling only; no heap sampler starts.
    Cpu,
    /// Heap sampling only; no CPU profiler process starts.
    Heap,
    /// Both collectors, which a bare `--profile` selects.
    Both,
}

impl ProfileMode {
    /// Every mode, its value on the command line.
    pub const ALL: [Self; 3] = [Self::Cpu, Self::Heap, Self::Both];

    /// The mode's command-line value and record label.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Cpu => "cpu",
            Self::Heap => "heap",
            Self::Both => "both",
        }
    }

    /// The mode `value` names, or `None`.
    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|mode| mode.name() == value)
    }

    /// Whether CPU captures run.
    #[must_use]
    pub const fn cpu(self) -> bool {
        matches!(self, Self::Cpu | Self::Both)
    }

    /// Whether heap captures run.
    #[must_use]
    pub const fn heap(self) -> bool {
        matches!(self, Self::Heap | Self::Both)
    }
}

/// What a run's optional profiling produced, as a record keeps it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProfileNote {
    /// Whether `--profile` asked for captures.
    pub requested: bool,
    /// The collectors it asked for; `None` in an unprofiled run or a record
    /// that predates modes, whose captures were both kinds of the server.
    #[serde(default)]
    pub mode: Option<ProfileMode>,
    /// The process roles profiled; empty in a record that predates roles,
    /// which profiled the server alone.
    #[serde(default)]
    pub roles: Vec<Role>,
    /// Whether every requested capture was saved.
    pub complete: bool,
    /// Saved captures, as paths relative to the working directory.
    pub captures: Vec<String>,
    /// Why profiling was disabled or a capture is missing, one entry per
    /// cause.
    pub notes: Vec<String>,
    /// The symbol evidence of each saved CPU capture; empty in a record that
    /// predates it.
    #[serde(default)]
    pub symbols: Vec<Symbolized>,
}

impl ProfileNote {
    /// Adds `other`'s captures, symbol evidence, and notes; any note leaves
    /// this incomplete.
    pub fn absorb(&mut self, other: Self) {
        self.complete &= other.notes.is_empty();
        self.captures.extend(other.captures);
        self.notes.extend(other.notes);
        self.symbols.extend(other.symbols);
    }
}

/// How well one CPU capture's sidecar resolves the process it sampled.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Symbolized {
    /// The capture, as [`ProfileNote::captures`] names it.
    pub capture: String,
    /// Samply's symbol sidecar beside it.
    pub sidecar: String,
    /// The sampled process's executable, kept under [`SYMBOLS`]; `None` when
    /// the capture names no module of the process or keeping it failed.
    pub executable: Option<Archived>,
    /// Samples with at least one frame in the executable.
    pub samples: u64,
    /// Of those, samples with an executable frame the sidecar leaves
    /// unresolved.
    pub unresolved: u64,
}

/// One sampled executable kept by its build ID.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Archived {
    /// The build ID the capture recorded for the executable.
    pub build_id: String,
    /// The kept copy, `<SYMBOLS>/<build-id>/<executable>`.
    pub path: String,
}

/// Why a CPU capture or its sidecar could not be read.
#[derive(Debug, thiserror::Error)]
pub enum SymbolError {
    /// The file could not be opened or read.
    #[error("cannot read {path}: {source}")]
    Read {
        /// The capture or sidecar.
        path: PathBuf,
        /// The read failure.
        source: std::io::Error,
    },
    /// The file is not a Samply capture or sidecar.
    #[error("cannot parse {path}: {source}")]
    Parse {
        /// The capture or sidecar.
        path: PathBuf,
        /// The parse failure.
        source: serde_json::Error,
    },
}

/// The parts of a Samply processed-profile capture its stacks need.
#[derive(Debug, Deserialize)]
struct Capture {
    /// Every module the capture sampled.
    libs: Vec<CaptureLib>,
    /// Every sampled thread.
    threads: Vec<CaptureThread>,
}

/// One sampled module.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CaptureLib {
    /// The module's file name.
    name: String,
    /// Where the module was loaded from.
    path: String,
    /// The module's build ID, when it has one.
    code_id: Option<String>,
}

/// One sampled thread's tables.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CaptureThread {
    /// The thread's name.
    name: String,
    /// The sampled process's name, which its executable module carries.
    process_name: Option<String>,
    /// Each sample's stack.
    samples: CaptureSamples,
    /// Stacks as a frame and its caller's stack.
    stack_table: StackTable,
    /// Frames as an address and a function.
    frame_table: FrameTable,
    /// Functions as a resource.
    func_table: FuncTable,
    /// Resources as a module.
    resource_table: ResourceTable,
}

/// Each sample's stack index.
#[derive(Debug, Deserialize)]
struct CaptureSamples {
    /// The stack of each sample; `None` for a sample without one.
    stack: Vec<Option<usize>>,
}

/// Stacks, innermost frame first through `prefix`.
#[derive(Debug, Deserialize)]
struct StackTable {
    /// Each stack's frame.
    frame: Vec<usize>,
    /// Each stack's caller stack.
    prefix: Vec<Option<usize>>,
}

/// Frames.
#[derive(Debug, Deserialize)]
struct FrameTable {
    /// Each frame's module-relative address, negative or `None` when absent.
    address: Vec<Option<i64>>,
    /// Each frame's function.
    func: Vec<usize>,
}

/// Functions.
#[derive(Debug, Deserialize)]
struct FuncTable {
    /// Each function's resource, negative or `None` when absent.
    resource: Vec<Option<i64>>,
}

/// Resources.
#[derive(Debug, Deserialize)]
struct ResourceTable {
    /// Each resource's module.
    lib: Vec<Option<usize>>,
}

/// Samply's `--unstable-presymbolicate` sidecar.
#[derive(Debug, Deserialize)]
struct Sidecar {
    /// Every name the sidecar refers to by index.
    string_table: Vec<String>,
    /// One entry per symbolized module.
    data: Vec<SidecarLib>,
}

/// One module's symbols.
#[derive(Debug, Deserialize)]
struct SidecarLib {
    /// The module's file name.
    debug_name: String,
    /// The module's build ID, when it has one.
    code_id: Option<String>,
    /// Each sampled address and its [`Self::symbol_table`] entry.
    known_addresses: Vec<(u64, usize)>,
    /// Symbols of the sampled addresses.
    symbol_table: Vec<SidecarSymbol>,
}

/// One address's function and, with debug information, its inline frames.
#[derive(Debug, Deserialize)]
struct SidecarSymbol {
    /// The function's name.
    symbol: usize,
    /// Inline frames, innermost first; empty without debug information.
    #[serde(default)]
    frames: Vec<SidecarFrame>,
}

/// One inline frame.
#[derive(Debug, Deserialize)]
struct SidecarFrame {
    /// The frame's function name, when known.
    function: Option<usize>,
}

/// Samples per function name.
type Counts = HashMap<String, u64>;

/// A CPU capture read with its Samply sidecar: every sample's stack as
/// function names, inline frames included.
#[derive(Debug)]
pub struct SymbolizedCapture {
    /// The capture.
    capture: Capture,
    /// Per capture module, each sampled address's names, innermost first.
    names: HashMap<usize, HashMap<u64, Vec<String>>>,
}

impl SymbolizedCapture {
    /// The sidecar Samply writes beside `capture`: `x.json.gz` beside
    /// `x.json.syms.json`.
    #[must_use]
    pub fn sidecar(capture: &Path) -> PathBuf {
        let stem = capture.to_string_lossy();
        PathBuf::from(format!(
            "{}.syms.json",
            stem.strip_suffix(".gz").unwrap_or(&stem)
        ))
    }

    /// Reads `capture` and its [`Self::sidecar`].
    ///
    /// # Errors
    ///
    /// Returns [`SymbolError`] when either file cannot be read or parsed.
    pub fn read(capture: &Path) -> Result<Self, SymbolError> {
        let open = |path: &Path| {
            std::fs::File::open(path).map_err(|source| SymbolError::Read {
                path: path.to_path_buf(),
                source,
            })
        };
        let parsed: Capture = serde_json::from_reader(std::io::BufReader::new(
            flate2::read::GzDecoder::new(open(capture)?),
        ))
        .map_err(|source| SymbolError::Parse {
            path: capture.to_path_buf(),
            source,
        })?;
        let path = Self::sidecar(capture);
        let sidecar: Sidecar = serde_json::from_reader(std::io::BufReader::new(open(&path)?))
            .map_err(|source| SymbolError::Parse { path, source })?;
        let text = |index: Option<usize>| {
            index
                .and_then(|index| sidecar.string_table.get(index))
                .cloned()
        };
        let mut names = HashMap::new();
        for (index, lib) in parsed.libs.iter().enumerate() {
            let Some(symbols) =
                sidecar
                    .data
                    .iter()
                    .find(|symbols| match (&symbols.code_id, &lib.code_id) {
                        (Some(theirs), Some(ours)) => theirs.eq_ignore_ascii_case(ours),
                        _ => symbols.debug_name == lib.name,
                    })
            else {
                continue;
            };
            let addresses = symbols
                .known_addresses
                .iter()
                .filter_map(|(address, entry)| {
                    let symbol = symbols.symbol_table.get(*entry)?;
                    let inline: Vec<String> = symbol
                        .frames
                        .iter()
                        .filter_map(|frame| text(frame.function))
                        .collect();
                    let resolved = if inline.is_empty() {
                        vec![text(Some(symbol.symbol))?]
                    } else {
                        inline
                    };
                    Some((*address, resolved))
                })
                .collect();
            names.insert(index, addresses);
        }
        Ok(Self {
            capture: parsed,
            names,
        })
    }

    /// The module and module-relative address of `thread`'s `frame`.
    fn located(thread: &CaptureThread, frame: usize) -> Option<(usize, u64)> {
        let address = u64::try_from(thread.frame_table.address.get(frame).copied()??).ok()?;
        let function = *thread.frame_table.func.get(frame)?;
        let resource = usize::try_from(thread.func_table.resource.get(function).copied()??).ok()?;
        Some((thread.resource_table.lib.get(resource).copied()??, address))
    }

    /// Each sample of `thread` as its frames, innermost first, each the
    /// module and address it was sampled at.
    fn stacks(thread: &CaptureThread) -> impl Iterator<Item = Vec<Option<(usize, u64)>>> + '_ {
        thread.samples.stack.iter().flatten().map(|&stack| {
            let mut frames = Vec::new();
            let mut next = Some(stack);
            while let Some(index) = next {
                let Some(&frame) = thread.stack_table.frame.get(index) else {
                    break;
                };
                frames.push(Self::located(thread, frame));
                next = thread.stack_table.prefix.get(index).copied().flatten();
            }
            frames
        })
    }

    /// The names of one sampled frame, innermost inline frame first, or its
    /// module and address when the sidecar does not resolve it.
    fn names(&self, frame: Option<(usize, u64)>) -> Vec<String> {
        let Some((lib, address)) = frame else {
            return vec!["<unknown>".to_owned()];
        };
        self.names
            .get(&lib)
            .and_then(|addresses| addresses.get(&address))
            .cloned()
            .unwrap_or_else(|| {
                let module = self
                    .capture
                    .libs
                    .get(lib)
                    .map_or("<unknown>", |lib| lib.name.as_str());
                vec![format!("{module}+{address:#x}")]
            })
    }

    /// The executable of the sampled process: the module named like the
    /// first thread's process.
    fn executable(&self) -> Option<usize> {
        let process = self
            .capture
            .threads
            .iter()
            .find_map(|thread| thread.process_name.as_deref())?;
        self.capture.libs.iter().position(|lib| lib.name == process)
    }

    /// Counts the samples with a frame in module `lib` and, of those, the
    /// samples with such a frame the sidecar does not resolve.
    fn resolution(&self, lib: usize) -> (u64, u64) {
        let known = self.names.get(&lib);
        let (mut samples, mut unresolved) = (0, 0);
        for thread in &self.capture.threads {
            for frames in Self::stacks(thread) {
                let mut own = frames
                    .iter()
                    .flatten()
                    .filter(|(module, _)| *module == lib)
                    .peekable();
                if own.peek().is_none() {
                    continue;
                }
                samples += 1;
                if own.any(|(_, address)| known.is_none_or(|known| !known.contains_key(address))) {
                    unresolved += 1;
                }
            }
        }
        (samples, unresolved)
    }

    /// A text report of the threads whose name contains `threads`, grouped
    /// by name: each group's samples, then its `top` functions by self and
    /// by inclusive samples, inline frames counted as functions. A sample is
    /// about one millisecond of CPU at [`PROFILE_HERTZ`].
    #[must_use]
    pub fn report(&self, threads: &str, top: usize) -> String {
        let mut groups: BTreeMap<&str, (u64, Counts, Counts)> = BTreeMap::new();
        for thread in &self.capture.threads {
            if !thread.name.contains(threads) {
                continue;
            }
            let (total, own, inclusive) = groups.entry(thread.name.as_str()).or_default();
            for frames in Self::stacks(thread) {
                let names: Vec<String> = frames
                    .into_iter()
                    .flat_map(|frame| self.names(frame))
                    .collect();
                let Some(leaf) = names.first() else {
                    continue;
                };
                *total += 1;
                *own.entry(leaf.clone()).or_default() += 1;
                for name in names.into_iter().collect::<BTreeSet<_>>() {
                    *inclusive.entry(name).or_default() += 1;
                }
            }
        }
        let mut groups: Vec<_> = groups.into_iter().collect();
        groups.sort_by_key(|(_, (total, ..))| std::cmp::Reverse(*total));
        let mut out = String::new();
        for (name, (total, own, inclusive)) in groups {
            let _ = writeln!(out, "== {name}: {total} samples");
            for (label, counts) in [("self", own), ("inclusive", inclusive)] {
                let _ = writeln!(out, "-- {label}");
                let mut counts: Vec<_> = counts.into_iter().collect();
                counts.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
                for (function, samples) in counts.into_iter().take(top) {
                    let tenths = samples * 1_000 / total.max(1);
                    let _ = writeln!(
                        out,
                        "  {:3}.{}% {samples:8} {function}",
                        tenths / 10,
                        tenths % 10
                    );
                }
            }
        }
        out
    }
}

/// Keeps `lib`, the module of a capture, once under `symbols` by its build
/// ID, and returns where.
///
/// A hard link costs no space until the build replaces the original; a
/// file on another volume is copied. The copy lands under a temporary name
/// and is renamed into place, so concurrent runs keep one whole file.
///
/// # Errors
///
/// Returns the IO error when the module has no build ID or cannot be linked
/// or copied.
fn archive(lib: &CaptureLib, symbols: &Path) -> std::io::Result<Archived> {
    let build_id = lib.code_id.clone().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "the module has no build ID",
        )
    })?;
    let directory = symbols.join(&build_id);
    let kept = directory.join(&lib.name);
    // ponytail: trusts the capture's build ID for the file at its path; a
    // rebuild between the capture and this link would keep the wrong file.
    if !kept.is_file() {
        std::fs::create_dir_all(&directory)?;
        let partial = directory.join(format!(".{}.{}", lib.name, std::process::id()));
        let _ = std::fs::remove_file(&partial);
        if std::fs::hard_link(&lib.path, &partial).is_err() {
            std::fs::copy(&lib.path, &partial)?;
        }
        std::fs::rename(&partial, &kept)?;
    }
    Ok(Archived {
        build_id,
        path: kept.display().to_string(),
    })
}

/// Reads `capture`'s sidecar, keeps its executable under `symbols`, and
/// returns its evidence with the note, when any, that leaves it incomplete.
fn symbolize(capture: &Path, symbols: &Path, name: &str) -> (Symbolized, Option<String>) {
    let mut evidence = Symbolized {
        capture: capture.display().to_string(),
        sidecar: SymbolizedCapture::sidecar(capture).display().to_string(),
        ..Symbolized::default()
    };
    let read = match SymbolizedCapture::read(capture) {
        Ok(read) => read,
        Err(error) => {
            return (
                evidence,
                Some(format!("capture of {name} is not symbolized: {error}")),
            );
        }
    };
    let Some(lib) = read.executable() else {
        return (
            evidence,
            Some(format!("capture of {name} names no module of its process")),
        );
    };
    (evidence.samples, evidence.unresolved) = read.resolution(lib);
    let mut note = match archive(&read.capture.libs[lib], symbols) {
        Ok(archived) => {
            evidence.executable = Some(archived);
            None
        }
        Err(error) => Some(format!(
            "capture of {name}: its executable was not kept under {}: {error}",
            symbols.display()
        )),
    };
    if evidence.samples == evidence.unresolved {
        note = Some(format!(
            "capture of {name} resolves none of its {} executable samples",
            evidence.samples
        ));
    } else if evidence.unresolved * 100 > evidence.samples * UNRESOLVED_PERCENT {
        note = Some(format!(
            "capture of {name} leaves {} of {} executable samples unresolved, over {UNRESOLVED_PERCENT}%",
            evidence.unresolved, evidence.samples
        ));
    }
    (evidence, note)
}

/// The running CPU captures of one measured step.
pub struct Profiler {
    /// The profiler program, [`SAMPLY`] outside tests.
    program: PathBuf,
    /// One running capture per process: its role and PID, the profiler
    /// process, and the capture it writes.
    captures: Vec<(Role, u32, Child, PathBuf)>,
    /// Causes noted so far.
    notes: Vec<String>,
    /// Whether profiling was requested.
    requested: bool,
    /// Where sampled executables are kept, [`SYMBOLS`] outside tests.
    symbols: PathBuf,
}

impl Profiler {
    /// A profiler that was not requested: finishing it notes nothing.
    #[must_use]
    pub fn disabled() -> Self {
        Self {
            program: PathBuf::from(SAMPLY),
            captures: Vec::new(),
            notes: Vec::new(),
            requested: false,
            symbols: PathBuf::new(),
        }
    }

    /// Starts `program` on each of `targets`, a role and PID each, writing
    /// under `directory` and keeping the sampled executables under
    /// `symbols`. A PID listed twice is captured once, under its first role.
    ///
    /// Never fails: a directory that cannot be created or a profiler that
    /// cannot spawn becomes a note, and the run continues unprofiled.
    #[must_use]
    pub fn start(
        program: &Path,
        targets: &[(Role, u32)],
        directory: &Path,
        symbols: &Path,
    ) -> Self {
        let mut profiler = Self {
            program: program.to_path_buf(),
            captures: Vec::new(),
            notes: Vec::new(),
            requested: true,
            symbols: symbols.to_path_buf(),
        };
        if !targets.iter().any(|(role, _)| *role == Role::Server) {
            profiler
                .notes
                .push("no server process to profile".to_owned());
        }
        if targets.is_empty() {
            return profiler;
        }
        if let Err(error) = std::fs::create_dir_all(directory) {
            profiler.notes.push(format!(
                "profiling disabled: cannot create {}: {error}",
                directory.display()
            ));
            return profiler;
        }
        for (index, (role, pid)) in targets.iter().enumerate() {
            if targets[..index].iter().any(|(_, seen)| seen == pid) {
                continue;
            }
            let name = format!("{}-{pid}", role.name());
            let output = directory.join(format!("cpu-{name}.json.gz"));
            let spawned = std::fs::File::create(directory.join(format!("samply-{name}.log")))
                .and_then(|log| {
                    Command::new(program)
                        .args([
                            "record",
                            "--save-only",
                            "--unstable-presymbolicate",
                            "--output",
                        ])
                        .arg(&output)
                        .args(["--rate", &PROFILE_HERTZ.to_string()])
                        .args(["--pid", &pid.to_string()])
                        .stdin(Stdio::null())
                        .stdout(log.try_clone()?)
                        .stderr(log)
                        .spawn()
                });
            match spawned {
                Ok(child) => profiler.captures.push((*role, *pid, child, output)),
                Err(error) => profiler.notes.push(format!(
                    "profiling disabled for {name}: {} did not start: {error}",
                    program.display()
                )),
            }
        }
        profiler
    }

    /// Interrupts every capture at once, then waits until one shared
    /// [`SAVE_GRACE`] deadline for them to save, killing any still running,
    /// and says what was captured and how well each saved capture is
    /// symbolized ([`Symbolized`]). Blocks the calling thread while captures
    /// save and are read; the runner calls it off the async runtime.
    #[must_use]
    pub fn finish(mut self) -> ProfileNote {
        let mut saving = Vec::new();
        for (role, pid, mut child, output) in std::mem::take(&mut self.captures) {
            let name = format!("{}-{pid}", role.name());
            if let Ok(Some(status)) = child.try_wait() {
                self.notes.push(format!(
                    "capture of {name} ended early with {status}; see samply-{name}.log"
                ));
                continue;
            }
            let _ = Command::new("kill")
                .args(["-s", "INT", &child.id().to_string()])
                .status();
            saving.push((name, child, output));
        }
        let deadline = Instant::now() + SAVE_GRACE;
        let mut captures = Vec::new();
        let mut symbols = Vec::new();
        for (name, mut child, output) in saving {
            let mut exited = false;
            while Instant::now() < deadline {
                if let Ok(Some(_)) = child.try_wait() {
                    exited = true;
                    break;
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            if !exited {
                let _ = child.kill();
                let _ = child.wait();
                self.notes.push(format!(
                    "capture of {name} did not save within {}s",
                    SAVE_GRACE.as_secs()
                ));
                continue;
            }
            if output.is_file() {
                captures.push(output.display().to_string());
                let (evidence, note) = symbolize(&output, &self.symbols, &name);
                symbols.push(evidence);
                self.notes.extend(note);
            } else {
                self.notes
                    .push(format!("capture of {name} left no {}", output.display()));
            }
        }
        if self.requested && captures.is_empty() && self.notes.is_empty() {
            self.notes
                .push(format!("{} produced no capture", self.program.display()));
        }
        let notes = std::mem::take(&mut self.notes);
        ProfileNote {
            requested: self.requested,
            complete: self.requested && notes.is_empty(),
            captures,
            notes,
            symbols,
            ..ProfileNote::default()
        }
    }
}

impl Drop for Profiler {
    /// Kills captures a failed run never finished.
    fn drop(&mut self) {
        for (_, _, child, _) in &mut self.captures {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

/// The heap profiles of one experiment, read from the server under test
/// and from this driver.
pub struct HeapProfiler {
    /// The server's heap profile endpoint, or `None` when it serves none
    /// or sampling did not start.
    url: Option<String>,
    /// Whether this driver's sampler runs.
    driver: bool,
    /// Where captures are written.
    directory: PathBuf,
    /// Saved captures.
    captures: Vec<String>,
    /// Causes noted so far.
    notes: Vec<String>,
}

impl HeapProfiler {
    /// Asks the server at `url` to start sampling and, when `driver`,
    /// starts this process's sampler, writing captures under `directory`.
    /// Neither captures nothing and notes nothing.
    ///
    /// Never fails: a server that does not start sampling becomes a note,
    /// and the run continues without its heap profiles.
    pub async fn start(url: Option<String>, driver: bool, directory: &Path) -> Self {
        let mut profiler = Self {
            url: None,
            driver,
            directory: directory.to_path_buf(),
            captures: Vec::new(),
            notes: Vec::new(),
        };
        if driver {
            wyrd_benchmark::heap::start();
        }
        let Some(url) = url else {
            return profiler;
        };
        let started = reqwest::Client::new()
            .post(&url)
            .send()
            .await
            .and_then(reqwest::Response::error_for_status);
        match started {
            Ok(_) => profiler.url = Some(url),
            Err(error) => profiler.notes.push(format!(
                "heap profiling disabled for the server: {url} did not start: {error}"
            )),
        }
        profiler
    }

    /// Saves each sampling role's current heap profile as
    /// `heap-<role>-<step>.pb.gz`, or notes why it could not.
    pub async fn capture(&mut self, step: &str) {
        if let Some(url) = &self.url {
            let profile = match reqwest::get(url)
                .await
                .and_then(reqwest::Response::error_for_status)
            {
                Ok(response) => response.bytes().await.map(|bytes| bytes.to_vec()),
                Err(error) => Err(error),
            };
            self.save(
                Role::Server,
                step,
                profile.map_err(|error| error.to_string()),
            );
        }
        if self.driver {
            let profile = tokio::task::spawn_blocking(wyrd_benchmark::heap::profile)
                .await
                .map_err(|error| error.to_string());
            self.save(Role::Driver, step, profile);
        }
    }

    /// Writes `role`'s `profile` after `step`, or notes why it is missing.
    fn save(&mut self, role: Role, step: &str, profile: Result<Vec<u8>, String>) {
        let output = self
            .directory
            .join(format!("heap-{}-{step}.pb.gz", role.name()));
        let saved = profile.and_then(|profile| {
            std::fs::create_dir_all(&self.directory)
                .and_then(|()| std::fs::write(&output, profile))
                .map_err(|error| error.to_string())
        });
        match saved {
            Ok(()) => self.captures.push(output.display().to_string()),
            Err(error) => self.notes.push(format!(
                "{} heap capture after {step} failed: {error}",
                role.name()
            )),
        }
    }

    /// Adds the captures and notes to the run's `note`; any heap note
    /// leaves it incomplete.
    pub fn finish(self, note: &mut ProfileNote) {
        note.absorb(ProfileNote {
            captures: self.captures,
            notes: self.notes,
            ..ProfileNote::default()
        });
    }
}

/// Stand-in CPU profilers for tests: shell scripts that save a small Samply
/// capture of a process named `app` when interrupted.
#[cfg(test)]
pub(crate) mod fixture {
    use std::path::{Path, PathBuf};

    /// What a stand-in profiler writes beside its capture.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub(crate) enum Sidecar {
        /// A sidecar resolving both sampled addresses, one through two
        /// inline frames.
        Resolved,
        /// A sidecar resolving only the first sampled address.
        Unresolved,
        /// No sidecar.
        Missing,
    }

    /// The capture's one module, `app`, loaded from `module`, build `0f0f`:
    /// two samples, the second calling from address 16 into address 32.
    fn capture(module: &Path) -> String {
        format!(
            r#"{{"libs":[{{"name":"app","path":"{}","codeId":"0f0f"}}],"threads":[{{"name":"main","processName":"app","samples":{{"stack":[0,1]}},"stackTable":{{"frame":[0,1],"prefix":[null,0]}},"frameTable":{{"address":[16,32],"func":[0,0]}},"funcTable":{{"resource":[0]}},"resourceTable":{{"lib":[0]}}}}]}}"#,
            module.display()
        )
    }

    /// The sidecar `kind` writes, if any.
    const fn sidecar(kind: Sidecar) -> &'static str {
        match kind {
            Sidecar::Resolved => {
                r#"{"string_table":["main_loop","work","inlined_step"],"data":[{"debug_name":"app","code_id":"0f0f","known_addresses":[[16,0],[32,1]],"symbol_table":[{"rva":0,"size":20,"symbol":0},{"rva":30,"size":10,"symbol":1,"frames":[{"function":2},{"function":1}]}]}]}"#
            }
            Sidecar::Unresolved => {
                r#"{"string_table":["main_loop"],"data":[{"debug_name":"app","code_id":"0f0f","known_addresses":[[16,0]],"symbol_table":[{"rva":0,"size":20,"symbol":0}]}]}"#
            }
            Sidecar::Missing => "",
        }
    }

    /// Writes an executable stand-in profiler named `name` under
    /// `directory`. It exits 3 unless asked to presymbolicate, marks
    /// `<output>.ready` once it can be interrupted, and on interruption
    /// waits `save_seconds`, then saves the gzipped capture to its
    /// `--output` with `kind`'s sidecar.
    ///
    /// # Panics
    /// Panics when the script cannot be written or made executable.
    pub(crate) fn profiler(
        directory: &Path,
        name: &str,
        kind: Sidecar,
        save_seconds: u32,
    ) -> PathBuf {
        let script = directory.join(name);
        std::fs::write(
            &script,
            format!(
                "#!/bin/sh\nout=''\nflag=''\nfor a in \"$@\"; do [ \"$a\" = --unstable-presymbolicate ] && flag=1; done\n\
                 while [ $# -gt 0 ]; do [ \"$1\" = --output ] && out=\"$2\"; shift; done\n[ -n \"$flag\" ] || exit 3\n\
                 capture='{}'\nsidecar='{}'\n\
                 trap 'rm -f \"$out.ready\"; sleep {}; printf %s \"$capture\" | gzip > \"$out\"; [ -z \"$sidecar\" ] || printf %s \"$sidecar\" > \"${{out%.gz}}.syms.json\"; exit 0' INT\n\
                 : > \"$out.ready\"\nwhile :; do sleep 0.02; done\n",
                capture(&script),
                sidecar(kind),
                save_seconds
            ),
        )
        .expect("script");
        let mut permissions = std::fs::metadata(&script).expect("metadata").permissions();
        std::os::unix::fs::PermissionsExt::set_mode(&mut permissions, 0o755);
        std::fs::set_permissions(&script, permissions).expect("chmod");
        script
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::fixture::{Sidecar, profiler};
    use super::{HeapProfiler, ProfileNote, Profiler, Role, SymbolizedCapture};

    /// A stand-in heap endpoint that starts on `POST` and serves `profile`
    /// on `GET`; returns its URL.
    ///
    /// # Panics
    /// Panics when the local listener cannot bind or report its address.
    async fn heap_endpoint(profile: Vec<u8>) -> String {
        let router = axum::Router::new().route(
            "/benchmark/heap",
            axum::routing::get(|| async move { profile })
                .post(|| async { axum::http::StatusCode::NO_CONTENT }),
        );
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .expect("bind");
        let address = listener.local_addr().expect("address");
        tokio::spawn(async move { axum::serve(listener, router).await });
        format!("http://{address}/benchmark/heap")
    }

    /// A requested, so far complete, note.
    fn complete() -> ProfileNote {
        ProfileNote {
            requested: true,
            complete: true,
            ..ProfileNote::default()
        }
    }

    /// Waits until every capture of `outputs` under `step` is ready to be
    /// interrupted.
    ///
    /// # Panics
    /// Panics when a capture's ready marker does not appear within ten
    /// seconds.
    fn ready(step: &Path, outputs: &[String]) {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        for output in outputs {
            let marker = step.join(format!("{output}.ready"));
            while !marker.exists() {
                assert!(
                    std::time::Instant::now() < deadline,
                    "{output} never started"
                );
                std::thread::yield_now();
            }
        }
    }

    /// A served profile is saved per step and listed as a capture; a server
    /// that does not start sampling leaves a note and no capture; a workload
    /// without a heap endpoint and an unsampled driver add neither.
    ///
    /// # Panics
    ///
    /// Panics when a capture or note is wrong.
    #[tokio::test]
    async fn heap_captures_and_failures_become_profile_evidence() {
        let directory = tempfile::tempdir().expect("directory");
        let url = heap_endpoint(b"profile".to_vec()).await;
        let mut served = HeapProfiler::start(Some(url), false, directory.path()).await;
        served.capture("chat.c1").await;
        let mut note = complete();
        served.finish(&mut note);
        let saved = directory.path().join("heap-server-chat.c1.pb.gz");
        assert_eq!(note.captures, [saved.display().to_string()]);
        assert!(note.complete && note.notes.is_empty(), "{note:?}");
        assert_eq!(std::fs::read(saved).expect("saved"), b"profile");

        let mut refused = HeapProfiler::start(
            Some("http://127.0.0.1:9/benchmark/heap".to_owned()),
            false,
            directory.path(),
        )
        .await;
        refused.capture("chat.c1").await;
        let mut note = complete();
        refused.finish(&mut note);
        assert!(note.captures.is_empty());
        assert!(!note.complete);
        assert!(
            note.notes[0].contains("heap profiling disabled"),
            "{note:?}"
        );

        let mut absent = HeapProfiler::start(None, false, directory.path()).await;
        absent.capture("chat.c1").await;
        let mut note = complete();
        absent.finish(&mut note);
        assert_eq!(note, complete());
    }

    /// The driver's own heap capture and the server's are both the shared
    /// sampler's gzipped pprof, written per step under their own role.
    ///
    /// # Panics
    ///
    /// Panics when a role's capture is missing or not a sampler profile.
    #[tokio::test]
    async fn driver_and_server_heap_captures_use_the_same_sample_contract() {
        let directory = tempfile::tempdir().expect("directory");
        let url = heap_endpoint(wyrd_benchmark::heap::profile()).await;
        let mut both = HeapProfiler::start(Some(url), true, directory.path()).await;
        both.capture("verify.c1").await;
        let mut note = complete();
        both.finish(&mut note);
        assert!(note.complete, "{note:?}");
        let expected: Vec<String> = ["server", "driver"]
            .map(|role| {
                directory
                    .path()
                    .join(format!("heap-{role}-verify.c1.pb.gz"))
                    .display()
                    .to_string()
            })
            .to_vec();
        assert_eq!(note.captures, expected);
        for capture in &note.captures {
            let bytes = std::fs::read(capture).expect("saved");
            assert_eq!(bytes.get(..2), Some(&[0x1f, 0x8b][..]), "{capture} is gzip");
        }
    }

    /// One CPU capture runs per process, named by its role, so the driver's
    /// capture never passes for the server's; a PID listed under both roles
    /// is captured once; a step without a server notes it.
    ///
    /// # Panics
    ///
    /// Panics when a capture's role, count, or note is wrong.
    #[test]
    fn cpu_targets_keep_driver_and_server_roles_separate() {
        let directory = tempfile::tempdir().expect("directory");
        let program = profiler(directory.path(), "samply", Sidecar::Resolved, 0);
        let step = directory.path().join("profiles").join("op-1");
        let symbols = directory.path().join("symbols");
        let driver = std::process::id();
        let profiler = Profiler::start(
            &program,
            &[(Role::Server, 1), (Role::Driver, driver)],
            &step,
            &symbols,
        );
        let outputs = [
            "cpu-server-1.json.gz".to_owned(),
            format!("cpu-driver-{driver}.json.gz"),
        ];
        ready(&step, &outputs);
        let note = profiler.finish();
        assert!(note.complete, "{note:?}");
        assert_eq!(
            note.captures,
            [
                step.join("cpu-server-1.json.gz").display().to_string(),
                step.join(format!("cpu-driver-{driver}.json.gz"))
                    .display()
                    .to_string(),
            ]
        );

        let shared = Profiler::start(
            &program,
            &[(Role::Server, driver), (Role::Driver, driver)],
            &step,
            &symbols,
        );
        ready(&step, &[format!("cpu-server-{driver}.json.gz")]);
        assert_eq!(shared.finish().captures.len(), 1, "one capture per process");

        let alone = Profiler::start(&program, &[(Role::Driver, driver)], &step, &symbols);
        let note = alone.finish();
        assert!(!note.complete);
        assert!(
            note.notes.iter().any(|n| n.contains("no server process")),
            "{note:?}"
        );
    }

    /// Runs `kind`'s stand-in profiler on one server process and returns its
    /// note, keeping executables under `symbols`.
    ///
    /// # Panics
    /// Panics when the stand-in never becomes ready.
    fn captured(directory: &Path, kind: Sidecar, symbols: &Path) -> ProfileNote {
        let program = profiler(directory, &format!("samply-{kind:?}"), kind, 0);
        let step = directory.join(format!("{kind:?}"));
        let running = Profiler::start(&program, &[(Role::Server, 1)], &step, symbols);
        ready(&step, &["cpu-server-1.json.gz".to_owned()]);
        running.finish()
    }

    /// A capture is complete only when its sidecar resolves its process's
    /// executable: a resolved capture records its samples, keeps the
    /// executable once under its build ID, and a second capture of the same
    /// build reuses that copy; a capture without a sidecar, or one leaving
    /// half its executable samples unresolved, is kept and noted.
    ///
    /// # Panics
    ///
    /// Panics when the evidence, the kept executable, or a note is wrong.
    #[test]
    fn cpu_captures_are_symbolized_and_their_executables_kept_by_build() {
        let directory = tempfile::tempdir().expect("directory");
        let symbols = directory.path().join("symbols");
        let kept = symbols.join("0f0f").join("app");

        let note = captured(directory.path(), Sidecar::Resolved, &symbols);
        assert!(note.complete, "{note:?}");
        let [evidence] = note.symbols.as_slice() else {
            panic!("one capture's evidence: {note:?}");
        };
        assert_eq!((evidence.samples, evidence.unresolved), (2, 0));
        assert!(evidence.sidecar.ends_with("cpu-server-1.json.syms.json"));
        let executable = evidence.executable.as_ref().expect("kept");
        assert_eq!(executable.build_id, "0f0f");
        assert_eq!(executable.path, kept.display().to_string());
        assert!(kept.is_file());
        let again = captured(directory.path(), Sidecar::Resolved, &symbols);
        assert!(again.complete, "{again:?}");
        assert_eq!(again.symbols[0].executable, evidence.executable);

        let missing = captured(directory.path(), Sidecar::Missing, &symbols);
        assert!(!missing.complete);
        assert_eq!(missing.captures.len(), 1, "the capture is still kept");
        assert!(
            missing
                .notes
                .iter()
                .any(|n| n.contains("is not symbolized")),
            "{missing:?}"
        );

        let partial = captured(directory.path(), Sidecar::Unresolved, &symbols);
        assert!(!partial.complete);
        assert_eq!(
            (partial.symbols[0].samples, partial.symbols[0].unresolved),
            (2, 1)
        );
        assert!(
            partial
                .notes
                .iter()
                .any(|n| n.contains("1 of 2 executable samples unresolved")),
            "{partial:?}"
        );
    }

    /// A report names inline frames as functions: the sample at the
    /// inlined address is `inlined_step`'s own, and `work`, which inlines
    /// it, and `main_loop`, its caller, include it; an unresolved address
    /// is named by its module and offset.
    ///
    /// # Panics
    ///
    /// Panics when a function's self or inclusive count is wrong.
    #[test]
    fn reports_count_inline_frames_and_name_unresolved_addresses() {
        let directory = tempfile::tempdir().expect("directory");
        let symbols = directory.path().join("symbols");
        let note = captured(directory.path(), Sidecar::Resolved, &symbols);
        let report = SymbolizedCapture::read(Path::new(&note.captures[0]))
            .expect("read")
            .report("", 10);
        assert!(report.contains("== main: 2 samples"), "{report}");
        let (own, inclusive) = report.split_once("-- inclusive").expect("both sections");
        for line in ["50.0%        1 inlined_step", "50.0%        1 main_loop"] {
            assert!(own.contains(line), "self {line}: {report}");
        }
        for line in ["100.0%        2 main_loop", "50.0%        1 work"] {
            assert!(inclusive.contains(line), "inclusive {line}: {report}");
        }

        let note = captured(directory.path(), Sidecar::Unresolved, &symbols);
        let report = SymbolizedCapture::read(Path::new(&note.captures[0]))
            .expect("read")
            .report("main", 10);
        assert!(report.contains("app+0x20"), "{report}");
    }
}
