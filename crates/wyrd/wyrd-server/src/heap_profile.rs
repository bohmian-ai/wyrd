//! Sampled heap profiles of a `bench` build, served as pprof.
//!
//! The binary's allocator reports every allocation and free here. Until
//! [`start`] runs, each report is one relaxed load. Afterwards about one
//! allocation per [`SAMPLE_PERIOD`] allocated bytes is kept with its call
//! stack until it is freed, the exponential sampling Go, tcmalloc and
//! jemalloc use, and [`profile`] renders the live samples as a gzipped pprof
//! `Profile` whose totals estimate the live heap allocated since the start.
//!
//! The sampler's own allocations are never sampled: every path that may
//! allocate runs under [`Reentry`], which the allocator hooks check first.

use std::cell::Cell;
use std::collections::{BTreeMap, HashMap};
use std::io::Write;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Mutex, MutexGuard, PoisonError};

use flate2::Compression;
use flate2::write::GzEncoder;
use prost::Message;

/// Where the metrics listener serves the profile (`GET`) and starts
/// sampling (`POST`).
pub const PATH: &str = "/benchmark/heap";

/// Mean bytes allocated between samples, as in Go and tcmalloc.
pub const SAMPLE_PERIOD: u64 = 512 * 1024;

/// Frames kept per sampled stack, leaf first.
const MAX_FRAMES: usize = 96;

/// The allocator's entry points. Every frame up to the outermost of these
/// belongs to the allocator, the sampler, or the unwinder. Matching the entry
/// rather than each inner frame holds even when inlined inner frames resolve
/// to short names.
const ALLOCATOR_ENTRIES: [&str; 4] = [
    "__rust_alloc",
    "__rust_realloc",
    "__rg_",
    "wyrd_server::heap_profile::sample_allocation",
];

/// Source paths of the Rust standard library, whose frames between the
/// allocator and the requesting code (`Vec` growth, `Box::new`) are trimmed
/// as Go's pprof trims its runtime's.
const STANDARD_LIBRARY_SOURCES: [&str; 2] = ["/rustc/", "/rustlib/src/rust/library/"];

/// Address buckets counting the live samples that hash into each, so a free
/// of an address no sample can hold skips the lock.
const BUCKETS: usize = 1 << 16;

/// Whether [`start`] has run.
static SAMPLING: AtomicBool = AtomicBool::new(false);

/// Live samples by address.
static LIVE: Mutex<BTreeMap<usize, Sample>> = Mutex::new(BTreeMap::new());

/// Live samples per address bucket; see [`BUCKETS`].
static BUCKETED: [AtomicU32; BUCKETS] = [const { AtomicU32::new(0) }; BUCKETS];

thread_local! {
    /// Bytes this thread may still allocate before its next sample; `None`
    /// until its first draw.
    static UNTIL_SAMPLE: Cell<Option<u64>> = const { Cell::new(None) };
    /// This thread's xorshift state; zero until first seeded.
    static RANDOM: Cell<u64> = const { Cell::new(0) };
    /// Set while this thread runs sampler code.
    static IN_SAMPLER: Cell<bool> = const { Cell::new(false) };
}

/// One sampled live allocation.
struct Sample {
    /// Requested bytes.
    size: u64,
    /// Return addresses, leaf first.
    stack: Vec<usize>,
}

/// Starts sampling. Allocations made earlier are never sampled; repeating
/// it changes nothing.
pub fn start() {
    SAMPLING.store(true, Ordering::Relaxed);
}

/// Reports `size` bytes the allocator just returned at `address`.
///
/// Inlined into the allocator, so before [`start`] it is one load.
#[inline]
pub fn allocated(address: *mut u8, size: usize) {
    if SAMPLING.load(Ordering::Relaxed) {
        sample_allocation(address, size as u64);
    }
}

/// Reports that the allocation at `address` is about to be freed.
///
/// Inlined into the allocator, so before [`start`] it is one load.
#[inline]
pub fn freed(address: *mut u8) {
    if SAMPLING.load(Ordering::Relaxed) {
        forget_allocation(address);
    }
}

/// Samples the allocation at `address` when its bytes complete this
/// thread's interval.
fn sample_allocation(address: *mut u8, size: u64) {
    if address.is_null() || Reentry::active() || !take_sample(size) {
        return;
    }
    let _reentry = Reentry::enter();
    let stack = capture_stack();
    let mut live = live();
    live.insert(address.addr(), Sample { size, stack });
    bucket(address).fetch_add(1, Ordering::Relaxed);
}

/// Drops the sample at `address`, if one is live there.
fn forget_allocation(address: *mut u8) {
    if bucket(address).load(Ordering::Relaxed) == 0 || Reentry::active() {
        return;
    }
    let _reentry = Reentry::enter();
    if live().remove(&address.addr()).is_some() {
        bucket(address).fetch_sub(1, Ordering::Relaxed);
    }
}

/// The live samples as a gzipped pprof profile, symbolized in process.
///
/// Blocks while it resolves symbols; call it off the async runtime.
///
/// # Panics
///
/// Panics only if gzip into memory fails, which it cannot.
#[must_use]
pub fn profile() -> Vec<u8> {
    let _reentry = Reentry::enter();
    let samples: Vec<(u64, Vec<usize>)> = live()
        .values()
        .map(|sample| (sample.size, sample.stack.clone()))
        .collect();
    let mut builder = ProfileBuilder::default();
    for (size, stack) in samples {
        builder.add(size, &stack);
    }
    let mut gzip = GzEncoder::new(Vec::new(), Compression::default());
    gzip.write_all(&builder.finish().encode_to_vec())
        .and_then(|()| gzip.finish())
        .expect("gzip into memory cannot fail")
}

/// Marks this thread as running sampler code until dropped.
struct Reentry;

impl Reentry {
    /// Whether this thread is already inside the sampler.
    fn active() -> bool {
        IN_SAMPLER.with(Cell::get)
    }

    /// Enters the sampler on this thread.
    fn enter() -> Self {
        IN_SAMPLER.with(|flag| flag.set(true));
        Self
    }
}

impl Drop for Reentry {
    /// Leaves the sampler on this thread.
    fn drop(&mut self) {
        IN_SAMPLER.with(|flag| flag.set(false));
    }
}

/// The live table, even after a panic elsewhere poisoned it.
fn live() -> MutexGuard<'static, BTreeMap<usize, Sample>> {
    LIVE.lock().unwrap_or_else(PoisonError::into_inner)
}

/// The bucket `address` hashes into.
#[expect(
    clippy::cast_possible_truncation,
    reason = "the shift leaves only the bucket index's bits"
)]
fn bucket(address: *mut u8) -> &'static AtomicU32 {
    let hash = (address.addr() as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15);
    &BUCKETED[(hash >> (64 - BUCKETS.trailing_zeros())) as usize]
}

/// Counts `size` bytes against this thread's sampling interval and says
/// whether this allocation is the one to sample.
fn take_sample(size: u64) -> bool {
    UNTIL_SAMPLE.with(|until| {
        let left = until.get().unwrap_or_else(next_interval);
        if left > size {
            until.set(Some(left - size));
            false
        } else {
            until.set(Some(next_interval()));
            true
        }
    })
}

/// An exponentially distributed interval with mean [`SAMPLE_PERIOD`].
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "the interval is non-negative and far below u64::MAX"
)]
fn next_interval() -> u64 {
    let uniform = RANDOM.with(|state| {
        // Seed from this thread's own state address, distinct per thread.
        let mut x = match state.get() {
            0 => std::ptr::from_ref(state).addr() as u64 | 1,
            x => x,
        };
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        state.set(x);
        // The top 53 bits as a uniform draw in [0, 1).
        (x >> 11) as f64 / (1_u64 << 53) as f64
    });
    (-(1.0 - uniform).ln() * SAMPLE_PERIOD as f64) as u64 + 1
}

/// This thread's return addresses, leaf first.
fn capture_stack() -> Vec<usize> {
    let mut stack = Vec::with_capacity(MAX_FRAMES);
    backtrace::trace(|frame| {
        stack.push(frame.ip().addr());
        stack.len() < MAX_FRAMES
    });
    stack
}

/// The pprof wire messages a heap profile uses, from Google's
/// `profile.proto`; fields this profile leaves empty are omitted.
mod pprof {
    /// A profile: sample types, samples, and the tables they index.
    #[derive(Clone, PartialEq, prost::Message)]
    pub struct Profile {
        #[prost(message, repeated, tag = "1")]
        pub sample_type: Vec<ValueType>,
        #[prost(message, repeated, tag = "2")]
        pub sample: Vec<Sample>,
        #[prost(message, repeated, tag = "4")]
        pub location: Vec<Location>,
        #[prost(message, repeated, tag = "5")]
        pub function: Vec<Function>,
        #[prost(string, repeated, tag = "6")]
        pub string_table: Vec<String>,
        #[prost(message, optional, tag = "11")]
        pub period_type: Option<ValueType>,
        #[prost(int64, tag = "12")]
        pub period: i64,
    }

    /// A value's type and unit, as string-table indices.
    #[derive(Clone, PartialEq, prost::Message)]
    pub struct ValueType {
        #[prost(int64, tag = "1")]
        pub r#type: i64,
        #[prost(int64, tag = "2")]
        pub unit: i64,
    }

    /// One stack, leaf first, and its value per sample type.
    #[derive(Clone, PartialEq, prost::Message)]
    pub struct Sample {
        #[prost(uint64, repeated, tag = "1")]
        pub location_id: Vec<u64>,
        #[prost(int64, repeated, tag = "2")]
        pub value: Vec<i64>,
    }

    /// One frame.
    #[derive(Clone, PartialEq, prost::Message)]
    pub struct Location {
        #[prost(uint64, tag = "1")]
        pub id: u64,
        #[prost(message, repeated, tag = "4")]
        pub line: Vec<Line>,
    }

    /// A source line within a function.
    #[derive(Clone, PartialEq, prost::Message)]
    pub struct Line {
        #[prost(uint64, tag = "1")]
        pub function_id: u64,
        #[prost(int64, tag = "2")]
        pub line: i64,
    }

    /// A function's name and file, as string-table indices.
    #[derive(Clone, PartialEq, prost::Message)]
    pub struct Function {
        #[prost(uint64, tag = "1")]
        pub id: u64,
        #[prost(int64, tag = "2")]
        pub name: i64,
        #[prost(int64, tag = "4")]
        pub filename: i64,
    }
}

/// One symbolized frame.
#[derive(Clone, PartialEq, Eq, Hash)]
struct Frame {
    /// The demangled function name, or the address when unresolved.
    function: String,
    /// The source file, or empty when unknown.
    file: String,
    /// The source line, or 0 when unknown.
    line: i64,
}

impl Frame {
    /// Whether this frame is an allocator entry point.
    fn is_allocator_entry(&self) -> bool {
        ALLOCATOR_ENTRIES
            .iter()
            .any(|entry| self.function.contains(entry))
    }

    /// Whether this frame is Rust standard library code.
    fn is_standard_library(&self) -> bool {
        STANDARD_LIBRARY_SOURCES
            .iter()
            .any(|source| self.file.contains(source))
    }
}

/// `frames`, leaf first, from the code that asked for memory outward: every
/// frame up to the outermost allocator entry is dropped, then the standard
/// library frames that carried the request there. A stack wholly inside the
/// standard library keeps those frames.
fn requesting_frames(frames: &[Frame]) -> &[Frame] {
    let entry = frames
        .iter()
        .rposition(Frame::is_allocator_entry)
        .map_or(0, |outermost| outermost + 1);
    let frames = &frames[entry..];
    match frames.iter().position(|frame| !frame.is_standard_library()) {
        Some(requester) => &frames[requester..],
        None => frames,
    }
}

/// Builds one pprof profile, interning strings, functions, and frames.
#[derive(Default)]
struct ProfileBuilder {
    /// The profile's tables so far.
    profile: pprof::Profile,
    /// String-table index by string.
    strings: HashMap<String, i64>,
    /// Function id by name and file.
    functions: HashMap<(String, String), u64>,
    /// Location id by frame.
    locations: HashMap<Frame, u64>,
    /// Symbolized frames by return address.
    resolved: HashMap<usize, Vec<Frame>>,
    /// Estimated live objects and bytes by stack of location ids.
    totals: HashMap<Vec<u64>, (f64, f64)>,
}

impl ProfileBuilder {
    /// Adds one sampled allocation of `size` bytes, weighted by the inverse
    /// of the chance an allocation of that size is sampled.
    fn add(&mut self, size: u64, stack: &[usize]) {
        let frames: Vec<Frame> = stack
            .iter()
            .flat_map(|&address| self.resolve(address))
            .collect();
        let locations = requesting_frames(&frames)
            .iter()
            .map(|frame| self.location(frame))
            .collect();
        let weight = 1.0 / (1.0 - (-(size as f64) / SAMPLE_PERIOD as f64).exp());
        let total = self.totals.entry(locations).or_default();
        total.0 += weight;
        total.1 += weight * size as f64;
    }

    /// The frames at `address`, innermost inlined function first.
    fn resolve(&mut self, address: usize) -> Vec<Frame> {
        self.resolved
            .entry(address)
            .or_insert_with(|| {
                let mut frames = Vec::new();
                let unresolved = || format!("{address:#x}");
                backtrace::resolve(address as *mut std::ffi::c_void, |symbol| {
                    frames.push(Frame {
                        function: symbol
                            .name()
                            .map_or_else(unresolved, |name| format!("{name:#}")),
                        file: symbol
                            .filename()
                            .map_or_else(String::new, |file| file.display().to_string()),
                        line: symbol.lineno().map_or(0, i64::from),
                    });
                });
                if frames.is_empty() {
                    frames.push(Frame {
                        function: unresolved(),
                        file: String::new(),
                        line: 0,
                    });
                }
                frames
            })
            .clone()
    }

    /// The location id of `frame`, adding it on first use.
    fn location(&mut self, frame: &Frame) -> u64 {
        if let Some(&id) = self.locations.get(frame) {
            return id;
        }
        let function_id = self.function(&frame.function, &frame.file);
        let id = self.profile.location.len() as u64 + 1;
        self.profile.location.push(pprof::Location {
            id,
            line: vec![pprof::Line {
                function_id,
                line: frame.line,
            }],
        });
        self.locations.insert(frame.clone(), id);
        id
    }

    /// The function id of `name` in `file`, adding it on first use.
    fn function(&mut self, name: &str, file: &str) -> u64 {
        let key = (name.to_owned(), file.to_owned());
        if let Some(&id) = self.functions.get(&key) {
            return id;
        }
        let id = self.profile.function.len() as u64 + 1;
        let function = pprof::Function {
            id,
            name: self.string(name),
            filename: self.string(file),
        };
        self.profile.function.push(function);
        self.functions.insert(key, id);
        id
    }

    /// The string-table index of `text`, adding it on first use; index 0 is
    /// always the empty string.
    fn string(&mut self, text: &str) -> i64 {
        if self.profile.string_table.is_empty() {
            self.profile.string_table.push(String::new());
            self.strings.insert(String::new(), 0);
        }
        if let Some(&index) = self.strings.get(text) {
            return index;
        }
        let index = i64::try_from(self.profile.string_table.len())
            .expect("a profile holds fewer than i64::MAX strings");
        self.profile.string_table.push(text.to_owned());
        self.strings.insert(text.to_owned(), index);
        index
    }

    /// The finished profile: `inuse_objects` and `inuse_space` per stack,
    /// sampled every [`SAMPLE_PERIOD`] bytes.
    #[expect(
        clippy::cast_possible_truncation,
        reason = "estimated live objects and bytes fit in i64"
    )]
    fn finish(mut self) -> pprof::Profile {
        let mut value_type = |kind: &str, unit: &str| pprof::ValueType {
            r#type: self.string(kind),
            unit: self.string(unit),
        };
        let sample_type = vec![
            value_type("inuse_objects", "count"),
            value_type("inuse_space", "bytes"),
        ];
        let period_type = Some(value_type("space", "bytes"));
        let mut profile = self.profile;
        profile.sample_type = sample_type;
        profile.period_type = period_type;
        profile.period = SAMPLE_PERIOD.cast_signed();
        profile.sample = self
            .totals
            .into_iter()
            .map(|(location_id, (objects, bytes))| pprof::Sample {
                location_id,
                value: vec![objects.round() as i64, bytes.round() as i64],
            })
            .collect();
        profile
    }
}

#[cfg(test)]
mod tests {
    use std::io::Read;

    use prost::Message;

    use super::pprof::Profile;

    /// Decodes a gzipped profile.
    fn decode(gzipped: &[u8]) -> Profile {
        let mut bytes = Vec::new();
        flate2::read::GzDecoder::new(gzipped)
            .read_to_end(&mut bytes)
            .expect("gzip");
        Profile::decode(bytes.as_slice()).expect("pprof")
    }

    /// The string at `index` of the profile's string table.
    fn string(profile: &Profile, index: i64) -> &str {
        &profile.string_table[usize::try_from(index).expect("a string index")]
    }

    /// The name of the function at the location `id`.
    fn function_name(profile: &Profile, id: u64) -> &str {
        let location = profile
            .location
            .iter()
            .find(|l| l.id == id)
            .expect("a location");
        let function_id = location.line[0].function_id;
        let function = profile
            .function
            .iter()
            .find(|f| f.id == function_id)
            .expect("a function");
        string(profile, function.name)
    }

    /// The estimated live bytes on stacks through a function named
    /// `function`.
    fn bytes_through(profile: &Profile, function: &str) -> i64 {
        profile
            .sample
            .iter()
            .filter(|sample| {
                sample
                    .location_id
                    .iter()
                    .any(|&id| function_name(profile, id).contains(function))
            })
            .map(|sample| sample.value[1])
            .sum()
    }

    /// A stack starts at the code that asked for memory: the allocator
    /// through its outermost entry and the standard library's plumbing are
    /// trimmed, while a stack wholly inside the standard library is kept.
    ///
    /// # Panics
    ///
    /// Panics when a frame is trimmed wrongly.
    #[test]
    fn stacks_start_at_the_requesting_code() {
        let frame = |function: &str, file: &str| super::Frame {
            function: function.to_owned(),
            file: file.to_owned(),
            line: 1,
        };
        let library = "/rustc/0000/library/alloc/src/raw_vec/mod.rs";
        let stack = [
            frame("trace", "backtrace/src/backtrace/mod.rs"),
            frame(
                "wyrd_server::heap_profile::sample_allocation",
                "src/heap_profile.rs",
            ),
            frame("__rustc::__rust_alloc", ""),
            frame("grow_amortized", library),
            frame(
                "append_value",
                "arrow-array/src/builder/generic_bytes_builder.rs",
            ),
            frame("std::rt::lang_start", "/rustc/0000/library/std/src/rt.rs"),
        ];
        let names: Vec<&str> = super::requesting_frames(&stack)
            .iter()
            .map(|frame| frame.function.as_str())
            .collect();
        assert_eq!(names, ["append_value", "std::rt::lang_start"]);

        let only_library = [frame("__rust_alloc", ""), frame("thread_start", library)];
        assert_eq!(super::requesting_frames(&only_library).len(), 1);
    }

    /// Reports an 8 MiB allocation at `address` from a named frame.
    #[inline(never)]
    fn allocate_eight_mib(address: *mut u8) {
        super::allocated(address, 8 << 20);
    }

    /// Nothing is sampled before the start; afterwards a live allocation is
    /// attributed to its caller at about its size, its free removes it, and
    /// the profile names its sample types and period.
    ///
    /// # Panics
    ///
    /// Panics when a profile attributes the allocation wrongly.
    #[test]
    fn samples_live_allocations_from_the_start() {
        let mut block = vec![0_u8; 64];
        let address = block.as_mut_ptr();

        allocate_eight_mib(address);
        assert!(
            decode(&super::profile()).sample.is_empty(),
            "sampled before the start"
        );

        super::start();
        allocate_eight_mib(address);
        let profile = decode(&super::profile());
        let names: Vec<&str> = profile
            .sample_type
            .iter()
            .map(|value_type| string(&profile, value_type.r#type))
            .collect();
        assert_eq!(names, ["inuse_objects", "inuse_space"]);
        assert_eq!(profile.period, super::SAMPLE_PERIOD.cast_signed());
        let bytes = bytes_through(&profile, "allocate_eight_mib");
        assert!(
            (8 << 20..(8 << 20) + 1024).contains(&bytes),
            "{bytes} bytes"
        );

        super::freed(address);
        assert_eq!(
            bytes_through(&decode(&super::profile()), "allocate_eight_mib"),
            0
        );
        drop(block);
    }
}
