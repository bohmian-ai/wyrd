//! Wyrd's hardened Monty worker process.
//!
//! The server's Monty pool spawns this binary as `wyrd-monty-worker
//! subprocess` with an empty environment and speaks Monty's framed protocol
//! over stdio. Before reading any request the process confines itself, and it
//! refuses to serve when any step fails, so missing isolation is never
//! silently accepted:
//!
//! 1. new user, network, and IPC namespaces without ID mappings: the worker
//!    runs as the overflow `nobody` identity with no capability outside its
//!    namespace, and has no usable network interface;
//! 2. no core dumps, no regular-file writes, and a small descriptor table;
//! 3. `no_new_privs` and a Landlock ruleset that handles every filesystem,
//!    TCP, and scope right with no exceptions, so no host path can be opened
//!    or executed, no TCP socket bound or connected, and no outside process
//!    signalled;
//! 4. once the interpreter thread runs, no process or thread creation.
//!
//! Sandbox file access is serviced by the parent from per-call mounts, so the
//! worker never needs a host path. The serve loop mirrors `monty-runtime`'s
//! `subprocess` shell over [`monty_proto::worker::Child`]. Each worker serves
//! one checkout and exits, so no interpreter state crosses invocations.

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

/// `sysexits.h` configuration error: isolation could not be established.
const EX_CONFIG: u8 = 78;

/// Most open descriptors: stdio plus Monty's own needs.
const MAX_FILES: u64 = 16;

/// Confine the process, then serve the protocol on a dedicated thread.
fn main() -> ExitCode {
    if !std::env::args().skip(1).eq(["subprocess".to_owned()]) {
        eprintln!("usage: wyrd-monty-worker subprocess");
        return ExitCode::from(EX_USAGE);
    }
    if let Err(reason) = confine() {
        eprintln!("wyrd-monty-worker refused to serve: {reason}");
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

/// Enter the namespaces, limits, and Landlock domain described in the crate
/// docs. Runs while the process is still single-threaded, which unsharing a
/// user namespace requires; threads created later inherit every restriction.
///
/// # Errors
/// Returns the failed step when the kernel refuses any restriction or does
/// not enforce Landlock.
#[cfg(target_os = "linux")]
fn confine() -> Result<(), String> {
    use landlock::{
        ABI, Access, AccessFs, AccessNet, Ruleset, RulesetAttr, RulesetCreated, RulesetStatus,
        Scope,
    };
    use nix::sched::{CloneFlags, unshare};
    use nix::sys::resource::{Resource, setrlimit};

    unshare(CloneFlags::CLONE_NEWUSER | CloneFlags::CLONE_NEWNET | CloneFlags::CLONE_NEWIPC)
        .map_err(|error| format!("namespaces: {error}"))?;
    for (resource, limit) in [
        (Resource::RLIMIT_CORE, 0),
        (Resource::RLIMIT_FSIZE, 0),
        (Resource::RLIMIT_NOFILE, MAX_FILES),
    ] {
        setrlimit(resource, limit, limit).map_err(|error| format!("{resource:?}: {error}"))?;
    }
    nix::sys::prctl::set_no_new_privs().map_err(|error| format!("no_new_privs: {error}"))?;
    let abi = ABI::V6;
    let status = Ruleset::default()
        .handle_access(AccessFs::from_all(abi))
        .and_then(|ruleset| ruleset.handle_access(AccessNet::from_all(abi)))
        .and_then(|ruleset| ruleset.scope(Scope::from_all(abi)))
        .and_then(Ruleset::create)
        .and_then(RulesetCreated::restrict_self)
        .map_err(|error| format!("landlock: {error}"))?;
    if status.ruleset == RulesetStatus::NotEnforced {
        return Err("landlock: this kernel does not enforce Landlock".to_owned());
    }
    Ok(())
}

/// Isolation exists only on Linux; elsewhere the worker never serves.
///
/// # Errors
/// Always.
#[cfg(not(target_os = "linux"))]
fn confine() -> Result<(), String> {
    Err("worker isolation requires Linux namespaces and Landlock".to_owned())
}

/// Forbid process and thread creation for the rest of the worker's life.
///
/// # Errors
/// Returns the refused limit.
#[cfg(target_os = "linux")]
fn seal() -> Result<(), String> {
    use nix::sys::resource::{Resource, setrlimit};
    setrlimit(Resource::RLIMIT_NPROC, 0, 0).map_err(|error| format!("RLIMIT_NPROC: {error}"))
}

/// Nothing to seal where the worker never serves.
///
/// # Errors
/// Never.
#[cfg(not(target_os = "linux"))]
fn seal() -> Result<(), String> {
    Ok(())
}

/// The protocol loop: one request in, its events out, until EOF, `Shutdown`,
/// or a fatal error.
fn serve() -> ExitCode {
    if let Err(reason) = seal() {
        eprintln!("wyrd-monty-worker refused to serve: {reason}");
        return ExitCode::from(EX_CONFIG);
    }
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
