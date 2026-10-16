//! Wyrd's Monty worker process.
//!
//! The server's Monty pool spawns this binary as `wyrd-monty-worker
//! subprocess` with an empty environment and speaks Monty's framed protocol
//! over stdio. Isolation is Monty's own and identical on every platform:
//! sandboxed Python has no host files, network, subprocesses, environment, or
//! host callbacks; every OS call is a protocol event the parent answers from
//! the call's mounts or refuses. [`ALLOC`] bounds the whole process heap at
//! the session's memory budget plus Monty's headroom, so the parent's
//! per-worker ceiling holds without OS-specific limits.
//!
//! The serve loop mirrors `monty-runtime`'s `subprocess` shell over
//! [`monty_proto::worker::Child`]. Each worker serves one checkout and exits,
//! so no interpreter state crosses invocations.

use std::io;
use std::process::ExitCode;
use std::thread;

use monty_proto::worker::{Child, EventSink, HandleOutcome, fatal_error_event, protocol_violation};
use monty_proto::{FrameError, FrameReader, pb, write_frame};
use monty_types::memory_limit_with_headroom;

/// Arms Monty's per-session allocation ceiling over every allocation.
#[global_allocator]
static ALLOC: monty_alloc::LimitedAllocator = monty_alloc::LimitedAllocator;

/// Native stack of the interpreter thread; Monty's recursion limit needs
/// more than the platform default on some hosts.
const WORKER_STACK_SIZE: usize = 16 * 1024 * 1024;

/// `sysexits.h` usage error: the binary was not started as a pool worker.
const EX_USAGE: u8 = 64;

/// `sysexits.h` protocol error: the frame stream desynchronized.
const EX_PROTOCOL: u8 = 76;

/// `sysexits.h` configuration error: the interpreter thread could not start.
const EX_CONFIG: u8 = 78;

/// Serve the protocol on a dedicated interpreter thread.
fn main() -> ExitCode {
    if !std::env::args().skip(1).eq(["subprocess".to_owned()]) {
        eprintln!("usage: wyrd-monty-worker subprocess");
        return ExitCode::from(EX_USAGE);
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

/// Arm the allocator ceiling of the session the child now holds.
///
/// # Panics
/// Panics when the limited allocator is not the global allocator, which
/// [`ALLOC`] guarantees.
fn apply_memory_limit(child: &Child) {
    let budget = child.session_budget();
    monty_alloc::set_hard_limit(memory_limit_with_headroom(
        budget.max_memory,
        budget.type_check,
    ))
    .expect("the limited allocator is installed globally");
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
    eprintln!("wyrd-monty-worker fatal error: {message}");
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
