//! The `wyrd-server subprocess` Monty worker and the allocator that bounds it.
//!
//! The server's Monty pool spawns the `wyrd-server` executable it was given
//! as `wyrd-server subprocess` with an empty environment and speaks Monty's framed protocol
//! over stdio, so one binary ships both the server and its Python workers.
//! Isolation is Monty's own and identical on every platform: sandboxed Python
//! has no host files, network, subprocesses, environment, or host callbacks;
//! every OS call is a protocol event the parent answers from the call's mounts
//! or refuses. A worker that crashes, aborts, or exceeds its budget ends only
//! its own process; the pool replaces it.
//!
//! [`CountingMiMalloc`] is the binary's global allocator; the library only
//! defines it, so embedders keep theirs, and the `wyrd-server` binary installs
//! it and calls [`dispatch`] first in `main`. In the server it is
//! mimalloc behind one relaxed flag load. In a worker it also feeds
//! [`LIVE_MEMORY`], which Monty's soft memory limit reads, and ends the
//! process with [`OOM_EXIT_CODE`] past the session's memory budget plus
//! Monty's headroom, so the parent reports `MemoryError` without OS-specific
//! limits.
//!
//! The serve loop mirrors `monty-runtime`'s `subprocess` shell over
//! [`monty_proto::worker::Child`]. Each worker serves one checkout and exits,
//! so no interpreter state crosses invocations.

use std::alloc::{GlobalAlloc, Layout};
use std::fmt;
use std::io::{self, Write};
use std::process::{self, ExitCode};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::thread;

use mimalloc::MiMalloc;
use monty_proto::worker::{Child, EventSink, HandleOutcome, fatal_error_event, protocol_violation};
use monty_proto::{FrameError, FrameReader, pb, write_frame};
use monty_types::{BASELINE_MEMORY, LIVE_MEMORY, OOM_EXIT_CODE, memory_limit_with_headroom};

/// The argument the Monty pool spawns a worker with.
pub const SUBPROCESS_ARG: &str = "subprocess";

/// Whether this process is a worker that counts its heap. Set once, first
/// thing in `main`, before the interpreter thread exists; the server never
/// sets it.
static COUNTING: AtomicBool = AtomicBool::new(false);

/// The absolute ceiling for counted live bytes; `usize::MAX` until a session
/// arms a budget.
static HARD_LIMIT: AtomicUsize = AtomicUsize::new(usize::MAX);

/// Native stack of the interpreter thread; Monty's recursion limit needs
/// more than the platform default on some hosts.
const WORKER_STACK_SIZE: usize = 16 * 1024 * 1024;

/// `sysexits.h` protocol error: the frame stream desynchronized.
const EX_PROTOCOL: u8 = 76;

/// `sysexits.h` configuration error: the interpreter thread could not start.
const EX_CONFIG: u8 = 78;

/// mimalloc, plus the worker's live-byte count that enforces Monty's memory
/// limits and a null check that ends a worker deliberately.
///
/// Counting starts in `main`, after the runtime's few pre-`main`
/// allocations, so a free of one of those saturates the count at zero rather
/// than wrapping; the count can undercount by at most those bytes.
pub struct CountingMiMalloc;

// SAFETY: every method forwards its arguments unchanged to `MiMalloc` and
// returns what `MiMalloc` returned (or diverges). No pointer is fabricated,
// aliased or freed here, so this upholds exactly the invariants `MiMalloc`
// upholds.
#[expect(
    unsafe_code,
    reason = "a global allocator is unsafe; the counting it adds is safe"
)]
unsafe impl GlobalAlloc for CountingMiMalloc {
    /// Charge `layout` to a worker's budget, then allocate it from mimalloc.
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        charge(layout.size());
        // SAFETY: `layout` comes from the caller and is forwarded unchanged.
        let ptr = unsafe { MiMalloc.alloc(layout) };
        if ptr.is_null() {
            refused(layout.size());
        }
        ptr
    }

    /// Refund `layout` to a worker's budget, then free it through mimalloc.
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        refund(layout.size());
        // SAFETY: `ptr` came from our `alloc`/`realloc` with this same `layout`.
        unsafe { MiMalloc.dealloc(ptr, layout) };
    }

    /// Charge `layout`, then take mimalloc's pre-zeroed allocation.
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        charge(layout.size());
        // SAFETY: `layout` comes from the caller and is forwarded unchanged.
        let ptr = unsafe { MiMalloc.alloc_zeroed(layout) };
        if ptr.is_null() {
            refused(layout.size());
        }
        ptr
    }

    /// Charge or refund the size change, then let mimalloc resize in place
    /// when it can.
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        match new_size.checked_sub(layout.size()) {
            Some(growth) => charge(growth),
            None => refund(layout.size() - new_size),
        }
        // SAFETY: `ptr`/`layout` describe a live block from this allocator, and
        // `new_size` is the caller's — all forwarded unchanged.
        let new_ptr = unsafe { MiMalloc.realloc(ptr, layout, new_size) };
        if new_ptr.is_null() {
            refused(new_size);
        }
        new_ptr
    }
}

/// Add `size` to a worker's live total, ending the worker past its hard limit.
#[inline]
fn charge(size: usize) {
    if !COUNTING.load(Ordering::Relaxed) {
        return;
    }
    let live = LIVE_MEMORY
        .fetch_add(size, Ordering::Relaxed)
        .saturating_add(size);
    if live > HARD_LIMIT.load(Ordering::Relaxed) {
        out_of_memory(format_args!(
            "monty worker: allocation of {size} bytes exceeds the memory limit"
        ));
    }
}

/// Return `size` to a worker's live total, saturating at zero for a block
/// allocated before counting began. `Relaxed` throughout: the count only has
/// to be eventually right, and no other memory is published through it.
#[inline]
fn refund(size: usize) {
    if COUNTING.load(Ordering::Relaxed) {
        let _ = LIVE_MEMORY.try_update(Ordering::Relaxed, Ordering::Relaxed, |live| {
            Some(live.saturating_sub(size))
        });
    }
}

/// End a worker whose allocation mimalloc refused; the server keeps Rust's
/// default allocation-failure handling.
#[cold]
fn refused(size: usize) {
    if COUNTING.load(Ordering::Relaxed) {
        out_of_memory(format_args!(
            "monty worker: allocation of {size} bytes failed"
        ));
    }
}

/// Report why memory ran out and exit with [`OOM_EXIT_CODE`], which the pool
/// reports as `MemoryError` — never by panicking, whose machinery allocates.
/// Skipping destructors can leave a partial frame on the transport, which the
/// pool already treats as a dead worker.
#[cold]
#[inline(never)]
fn out_of_memory(reason: fmt::Arguments<'_>) -> ! {
    // A genuinely exhausted host can fail the write and re-enter: let the first
    // caller write and send any re-entrant one straight to the end.
    static REPORTING: AtomicBool = AtomicBool::new(false);
    // Lift the limit first: writing to stderr allocates, which under an
    // exceeded limit would re-enter. Sound because this path never returns.
    HARD_LIMIT.store(usize::MAX, Ordering::Relaxed);
    if !REPORTING.swap(true, Ordering::Relaxed) {
        let _ = writeln!(io::stderr(), "{reason}");
    }
    process::exit(OOM_EXIT_CODE);
}

/// Run this process as a Monty pool worker when the pool spawned it.
///
/// Returns `Some` exit code after serving the worker protocol when the only
/// argument is [`SUBPROCESS_ARG`], and `None` for every other invocation so
/// the caller continues as the server. The binary calls this first in
/// `main`, before any runtime, CLI, or telemetry exists, so a worker process
/// holds only the interpreter thread and its own heap count.
#[must_use]
pub fn dispatch() -> Option<ExitCode> {
    std::env::args_os()
        .skip(1)
        .eq([std::ffi::OsString::from(SUBPROCESS_ARG)])
        .then(run)
}

/// Run this process as a Monty pool worker: start counting the heap, then
/// serve the protocol on a dedicated interpreter thread until it ends.
fn run() -> ExitCode {
    COUNTING.store(true, Ordering::Relaxed);
    thread::Builder::new()
        .name("monty-worker".to_owned())
        .stack_size(WORKER_STACK_SIZE)
        .spawn(serve)
        .map_or(ExitCode::from(EX_CONFIG), |worker| {
            worker.join().unwrap_or(ExitCode::from(101))
        })
}

/// The protocol loop: one request in, its events out, until EOF, `Shutdown`,
/// or a fatal error.
fn serve() -> ExitCode {
    install_panic_hook();
    let mut reader = FrameReader::new(io::stdin().lock());
    let mut child = Child::default();
    let mut sink = StdoutSink;
    loop {
        match reader.read::<pb::ParentRequest>() {
            Ok(Some(request)) => {
                let outcome = child.handle(request, &mut sink);
                apply_memory_limit(&child);
                match outcome {
                    Ok(HandleOutcome::Continue) => {}
                    Ok(HandleOutcome::Shutdown) => return ExitCode::SUCCESS,
                    Ok(HandleOutcome::Fatal) => return ExitCode::from(4),
                    Err(FrameError::FrameTooLarge { len, max }) => {
                        fatal(
                            &child,
                            &mut sink,
                            &format!("response frame of {len} bytes exceeds {max} bytes"),
                        );
                        return ExitCode::from(EX_PROTOCOL);
                    }
                    Err(_) => return ExitCode::from(3),
                }
            }
            Ok(None) => return ExitCode::SUCCESS,
            Err(FrameError::Decode(error)) => {
                if sink
                    .send(&protocol_violation(&format!("malformed request: {error}")))
                    .is_err()
                {
                    return ExitCode::from(3);
                }
            }
            Err(error) => {
                fatal(
                    &child,
                    &mut sink,
                    &format!("malformed request frame: {error}"),
                );
                return ExitCode::from(EX_PROTOCOL);
            }
        }
    }
}

/// Arm the hard ceiling of the session the child now holds, relative to the
/// worker's baseline, as `monty-alloc`'s `set_hard_limit` does. Re-applied
/// after every request, since a session can also arrive through a restored
/// dump or end through a reset.
fn apply_memory_limit(child: &Child) {
    let budget = child.session_budget();
    let live = LIVE_MEMORY.load(Ordering::Relaxed);
    // `fetch_min` both reads and lowers the baseline: the first arming sets
    // it, and a later leaner moment can only improve it.
    let baseline = BASELINE_MEMORY.fetch_min(live, Ordering::Relaxed).min(live);
    let hard_limit = memory_limit_with_headroom(budget.max_memory, budget.type_check)
        .map_or(usize::MAX, |bytes| baseline.saturating_add(bytes));
    HARD_LIMIT.store(hard_limit, Ordering::Relaxed);
}

/// Writes framed child events to stdout.
struct StdoutSink;

impl EventSink for StdoutSink {
    /// Frame `event` onto stdout.
    ///
    /// # Errors
    /// Returns the framing or write failure.
    fn send(&mut self, event: &pb::ChildEvent) -> Result<(), FrameError> {
        write_frame(&mut io::stdout(), event)
    }
}

/// Emit a best-effort `FatalError` before exiting.
fn fatal(child: &Child, sink: &mut impl EventSink, message: &str) {
    eprintln!("wyrd-server monty worker fatal error: {message}");
    let _ = sink.send(&child.fatal_event(message));
}

/// Emit a best-effort `FatalError` frame on panic before the default hook.
fn install_panic_hook() {
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = write_frame(
            &mut io::stdout(),
            &fatal_error_event(&format!("worker panicked: {info}")),
        );
        default_hook(info);
    }));
}
