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
//! The `wyrd-server` binary owns the global allocator and calls [`dispatch`]
//! first in `main`. Its allocator reports every size change through
//! [`charge`], [`refund`] and [`refused`]: in the server they are one relaxed
//! flag load, and in a worker they feed [`LIVE_MEMORY`], which Monty's soft
//! memory limit reads, and end the process with [`OOM_EXIT_CODE`] past the
//! session's memory budget plus Monty's headroom, so the parent reports
//! `MemoryError` without OS-specific limits. This library never references an
//! allocator itself, so embedders such as the Python and TypeScript SDK
//! builds link none and keep their own.
//!
//! A second thread reports the worker's live and committed bytes every
//! [`TELEMETRY_INTERVAL`] as one `MemorySample` frame, independent of the
//! interpreter's turns. Frames from both threads are written whole under the
//! stdout lock, so they never interleave; the reporter reuses one buffer, so
//! sampling allocates nothing and no allocator hook does IPC.
//!
//! The serve loop mirrors `monty-runtime`'s `subprocess` shell over
//! [`monty_proto::worker::Worker`]. A worker serves one session at a time; a
//! `Reset` between sessions drops the session's interpreter, globals, and
//! budget, and [`apply_budget`] arms each session's ceiling before its first
//! request runs and re-arms it after every request, so no interpreter state
//! crosses invocations.

use std::fmt;
use std::io::{self, Write};
use std::process::{self, ExitCode};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::thread;
use std::time::Duration;

use monty_proto::worker::{
    EventSink, HandleOutcome, SessionBudget, Worker, fatal_error_event, protocol_violation,
};
use monty_proto::{FrameError, FrameReader, encode_framed_into, pb, write_frame};
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
pub(crate) const WORKER_STACK_SIZE: usize = 16 * 1024 * 1024;

/// How often a worker reports its live and committed bytes to the parent.
pub const TELEMETRY_INTERVAL: Duration = Duration::from_millis(100);

/// `sysexits.h` protocol error: the frame stream desynchronized.
const EX_PROTOCOL: u8 = 76;

/// `sysexits.h` configuration error: the interpreter thread could not start.
const EX_CONFIG: u8 = 78;

/// Add `size` to a worker's live total, ending the worker past its hard limit.
///
/// The binary's global allocator calls this before every allocation and
/// growth; outside a worker it returns after one relaxed flag load.
#[inline]
pub fn charge(size: usize) {
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
///
/// The binary's global allocator calls this on every free and shrink.
#[inline]
pub fn refund(size: usize) {
    if COUNTING.load(Ordering::Relaxed) {
        let _ = LIVE_MEMORY.try_update(Ordering::Relaxed, Ordering::Relaxed, |live| {
            Some(live.saturating_sub(size))
        });
    }
}

/// End a worker whose allocation the allocator refused; the server keeps
/// Rust's default allocation-failure handling.
///
/// The binary's global allocator calls this when it returns null.
#[cold]
pub fn refused(size: usize) {
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
/// holds only the interpreter and telemetry threads and its own heap count.
/// `committed` reads the binary's allocator commitment, which only the binary
/// can observe.
#[must_use]
pub fn dispatch(committed: fn() -> u64) -> Option<ExitCode> {
    std::env::args_os()
        .skip(1)
        .eq([std::ffi::OsString::from(SUBPROCESS_ARG)])
        .then(|| run(committed))
}

/// Run this process as a Monty pool worker: start counting the heap and
/// reporting telemetry, then serve the protocol on a dedicated interpreter
/// thread until it ends.
fn run(committed: fn() -> u64) -> ExitCode {
    COUNTING.store(true, Ordering::Relaxed);
    if thread::Builder::new()
        .name("monty-telemetry".to_owned())
        .stack_size(TELEMETRY_STACK_SIZE)
        .spawn(move || report(committed, io::stdout()))
        .is_err()
    {
        return ExitCode::from(EX_CONFIG);
    }
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
    let mut worker = Worker::default();
    let mut sink = StdoutSink;
    // set the baseline on the pristine worker: a session's first request is
    // decoded before its ceiling is armed and must not count as overhead
    apply_budget(worker.session_budget());
    loop {
        match reader.read::<pb::ClientRequest>() {
            Ok(Some(request)) => {
                let outcome = worker.handle(request, &mut sink);
                apply_budget(worker.session_budget());
                match outcome {
                    Ok(HandleOutcome::Continue) => {}
                    Ok(HandleOutcome::Shutdown) => return ExitCode::SUCCESS,
                    Ok(HandleOutcome::Fatal) => return ExitCode::from(4),
                    Err(FrameError::FrameTooLarge { len, max }) => {
                        fatal(
                            &worker,
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
                    &worker,
                    &mut sink,
                    &format!("malformed request frame: {error}"),
                );
                return ExitCode::from(EX_PROTOCOL);
            }
        }
    }
}

/// Native stack of the telemetry thread, which only encodes one small frame.
const TELEMETRY_STACK_SIZE: usize = 64 * 1024;

/// Report the live and committed bytes to `out` every [`TELEMETRY_INTERVAL`]
/// until it closes; a worker's stdout closes only as the process ends.
///
/// Each sample reads the counters when it is written, so a write delayed by a
/// large frame on the interpreter thread coalesces into one current sample
/// rather than queueing stale ones.
fn report(committed: fn() -> u64, mut out: impl Write) {
    let mut frame = Vec::with_capacity(TELEMETRY_FRAME_CAPACITY);
    loop {
        thread::sleep(TELEMETRY_INTERVAL);
        let event = pb::WorkerEvent {
            kind: Some(pb::worker_event::Kind::MemorySample(pb::MemorySample {
                live_bytes: u64::try_from(LIVE_MEMORY.load(Ordering::Relaxed)).unwrap_or(u64::MAX),
                committed_bytes: committed(),
            })),
            ..pb::WorkerEvent::default()
        };
        if encode_framed_into(&event, &mut frame).is_err() {
            return;
        }
        // `Stdout::write_all` writes the whole frame under one lock, so it
        // never interleaves with the interpreter thread's frames.
        if out.write_all(&frame).and_then(|()| out.flush()).is_err() {
            return;
        }
    }
}

/// Room for one encoded telemetry frame, reserved once so sampling never
/// allocates.
const TELEMETRY_FRAME_CAPACITY: usize = 64;

/// Arm the hard ceiling for the session `budget` describes, relative to the
/// worker's baseline, as `monty-alloc`'s `set_hard_limit` does. Applied at
/// start-up, when a session opens, and after every request, since a session
/// can also arrive through a restored dump or end through a reset.
fn apply_budget(budget: SessionBudget) {
    let live = LIVE_MEMORY.load(Ordering::Relaxed);
    // `fetch_min` both reads and lowers the baseline: the first arming sets
    // it, and a later leaner moment can only improve it.
    let baseline = BASELINE_MEMORY.fetch_min(live, Ordering::Relaxed).min(live);
    let hard_limit = memory_limit_with_headroom(budget.max_memory, budget.type_check)
        .map_or(usize::MAX, |bytes| baseline.saturating_add(bytes));
    HARD_LIMIT.store(hard_limit, Ordering::Relaxed);
}

/// Writes framed worker events to stdout.
struct StdoutSink;

impl EventSink for StdoutSink {
    /// Frame `event` onto stdout under its lock, so a telemetry frame never
    /// lands inside it.
    ///
    /// # Errors
    /// Returns the framing or write failure.
    fn send(&mut self, event: &pb::WorkerEvent) -> Result<(), FrameError> {
        write_frame(&mut io::stdout().lock(), event)
    }

    /// Arm the ceiling of a session opened by its first request before that
    /// request runs.
    fn session_opened(&mut self, budget: SessionBudget) {
        apply_budget(budget);
    }
}

/// Emit a best-effort `FatalError` before exiting.
fn fatal(worker: &Worker, sink: &mut impl EventSink, message: &str) {
    eprintln!("wyrd-server monty worker fatal error: {message}");
    let _ = sink.send(&worker.fatal_event(message));
}

/// Emit a best-effort `FatalError` frame on panic before the default hook.
fn install_panic_hook() {
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = write_frame(
            &mut io::stdout().lock(),
            &fatal_error_event(&format!("worker panicked: {info}")),
        );
        default_hook(info);
    }));
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::AtomicU64;
    use std::sync::mpsc;

    use super::*;

    /// The committed bytes the telemetry test reports, and how often they
    /// were read.
    static COMMITTED: AtomicU64 = AtomicU64::new(1);
    static READS: AtomicUsize = AtomicUsize::new(0);

    /// The test's committed-bytes reader.
    fn committed() -> u64 {
        READS.fetch_add(1, Ordering::SeqCst);
        COMMITTED.load(Ordering::SeqCst)
    }

    /// A telemetry sink whose first write waits for the test, standing in
    /// for a worker stdout delayed behind a large interpreter frame, and
    /// which closes after its third frame.
    struct DelayedSink {
        frames: mpsc::Sender<Vec<u8>>,
        release: mpsc::Receiver<()>,
        written: usize,
    }

    impl Write for DelayedSink {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            self.written += 1;
            if self.written > 2 {
                return Err(io::ErrorKind::BrokenPipe.into());
            }
            self.frames.send(buf.to_vec()).map_err(io::Error::other)?;
            if self.written == 1 {
                self.release.recv().map_err(io::Error::other)?;
            }
            Ok(buf.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    /// The committed bytes one telemetry frame carries.
    fn sampled(frame: &[u8]) -> u64 {
        let event: pb::WorkerEvent = FrameReader::new(frame)
            .read()
            .expect("a telemetry frame decodes")
            .expect("one frame");
        match event.kind {
            Some(pb::worker_event::Kind::MemorySample(sample)) => sample.committed_bytes,
            other => panic!("not a memory sample: {other:?}"),
        }
    }

    /// A delayed telemetry write coalesces: no samples queue behind it, and
    /// the next frame reports the bytes current when it is written rather
    /// than a stale reading; telemetry stops once its output closes.
    ///
    /// # Panics
    /// Panics when a frame carries a stale reading, samples are read
    /// without being written, or the reporter outlives its closed output.
    #[test]
    fn delayed_telemetry_coalesces_into_one_current_sample() {
        let (frames, received) = mpsc::channel();
        let (release, released) = mpsc::channel();
        let reporter = thread::spawn(move || {
            report(
                committed,
                DelayedSink {
                    frames,
                    release: released,
                    written: 0,
                },
            );
        });
        let first = received.recv().expect("the first sample is written");
        assert_eq!(sampled(&first), 1);
        COMMITTED.store(2, Ordering::SeqCst);
        release.send(()).expect("the delayed write is released");
        let next = received.recv().expect("the next sample is written");
        assert_eq!(sampled(&next), 2, "the next frame reads current bytes");
        reporter
            .join()
            .expect("telemetry stops once its output closes");
        assert_eq!(
            READS.load(Ordering::SeqCst),
            3,
            "one reading per frame written, none queued behind the delay"
        );
        assert!(received.try_recv().is_err(), "no stale frames queued");
    }
}
