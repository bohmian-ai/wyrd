//! The Python sandbox against real `wyrd-server subprocess` Monty workers.
//!
//! Cargo builds this package's `wyrd-server` binary, which serves as its own
//! worker, in the same profile before it runs these tests.

use std::path::PathBuf;
use std::time::Duration;

use monty_pool::{MountSpec, MountSpecMode};
use serde_json::{Map, Value, json};
use tokio::time::Instant;
use vala_verify::blocks::BlockError;
use wyrd_server::verification::hosted::HostedLimits;
use wyrd_server::verification::hosted::sandbox::{CodeRun, Sandbox};
use wyrd_server::verification::hosted::workspace::{Staged, Workspace};

/// The worker binary: this package's `wyrd-server`, which serves as its own
/// worker.
pub(crate) fn worker() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_wyrd-server"))
}

/// A sandbox under `limits` with no prewarmed workers.
///
/// # Panics
/// Panics when the pool cannot start.
async fn start(limits: HostedLimits) -> Sandbox {
    Sandbox::start(
        worker(),
        HostedLimits {
            monty_min_workers: 0,
            ..limits
        },
    )
    .await
    .expect("pool starts")
}

/// Run `entrypoint` in `source` with `inputs` and no mounts.
///
/// # Errors
/// Returns the sandbox's [`BlockError`] for refused or failed code.
///
/// # Panics
/// Panics when `inputs` is not a JSON object.
async fn run(
    sandbox: &Sandbox,
    source: &str,
    entrypoint: &str,
    inputs: Value,
) -> Result<CodeRun, BlockError> {
    let Value::Object(inputs) = inputs else {
        panic!("inputs are an object")
    };
    sandbox
        .run(source, entrypoint, &inputs, Vec::new(), far())
        .await
}

/// A deadline no test reaches.
fn far() -> Instant {
    Instant::now() + Duration::from_secs(60)
}

/// The failure's reason when it is terminal.
///
/// # Panics
/// Panics on success or a retryable failure.
fn terminal(result: Result<CodeRun, BlockError>) -> String {
    match result {
        Err(BlockError::Terminal { reason }) => reason,
        other => panic!("expected a terminal failure, got {other:?}"),
    }
}

/// Entry points receive JSON inputs, return JSON, and keep bounded prints;
/// nothing defined by one operation is visible to the next.
///
/// # Panics
/// Panics when values, prints, or state cross wrongly.
#[tokio::test]
async fn sandbox_runs_entrypoints_without_shared_state() {
    let limits = HostedLimits {
        tool_result_max_bytes: 64,
        ..HostedLimits::default()
    };
    let sandbox = start(limits).await;
    let ran = run(
        &sandbox,
        "seen = 1\ndef f(a):\n    for _ in range(100):\n        print('x' * 10)\n    return {'sum': a + 1}",
        "f",
        json!({"a": 1}),
    )
    .await
    .expect("runs");
    assert_eq!(ran.value, json!({"sum": 2}));
    assert_eq!(ran.output.len(), 64);

    let (value, output) = sandbox
        .exec(
            "print('hi')\nx = 2\nx * 21".to_owned(),
            Vec::new(),
            Vec::new(),
            far(),
        )
        .await
        .expect("exec");
    assert_eq!((value.py_repr().as_str(), output.as_str()), ("42", "hi\n"));

    let reason = terminal(run(&sandbox, "def g():\n    return seen", "g", json!({})).await);
    assert!(reason.contains("NameError"), "{reason}");
    assert!(
        terminal(run(&sandbox, "", "f; import os", json!({})).await)
            .contains("not a Python function")
    );
}

/// Printed output is cut on a character boundary: a multibyte character
/// that would cross the cap is dropped whole, one that exactly fills it is
/// kept, and the sandbox keeps serving.
///
/// # Panics
/// Panics when output splits a character or the sandbox stops serving.
#[tokio::test]
async fn sandbox_cuts_output_on_character_boundaries() {
    let sandbox = start(HostedLimits {
        tool_result_max_bytes: 4,
        ..HostedLimits::default()
    })
    .await;
    for (text, kept) in [("abcé", "abc"), ("abé", "abé")] {
        let ran = run(
            &sandbox,
            &format!("def f():\n    print('{text}', end='')"),
            "f",
            json!({}),
        )
        .await
        .expect("runs");
        assert_eq!(ran.output, kept);
    }
}

/// Host files and functions are unreachable; mounts are the only paths, and
/// `/inputs` is read-only.
///
/// # Panics
/// Panics when host state is reachable or a mount mode is ignored.
#[tokio::test]
async fn sandbox_reaches_only_its_mounts() {
    let sandbox = start(HostedLimits::default()).await;
    for source in [
        "from pathlib import Path\ndef f():\n    return Path('/etc/hostname').read_text()",
        "def f():\n    return open('/etc/hostname').read()",
    ] {
        let reason = terminal(run(&sandbox, source, "f", json!({})).await);
        assert!(reason.contains("PermissionError"), "{reason}");
    }
    let reason = terminal(
        run(
            &sandbox,
            "import os\ndef f():\n    return os.getenv('HOME')",
            "f",
            json!({}),
        )
        .await,
    );
    assert!(reason.contains("not supported"), "{reason}");

    let inputs = tempfile::tempdir().expect("inputs");
    let work = tempfile::tempdir().expect("work");
    std::fs::write(inputs.path().join("a.txt"), "hello").expect("input written");
    let mounts = || {
        let mut writable =
            MountSpec::new("/work", work.path(), MountSpecMode::ReadWrite).expect("work");
        writable.write_bytes_limit = Some(16);
        vec![
            MountSpec::new("/inputs", inputs.path(), MountSpecMode::ReadOnly).expect("inputs"),
            writable,
        ]
    };
    let copy = "from pathlib import Path\ndef f():\n    text = Path('/inputs/a.txt').read_text()\n    Path('/work/b.txt').write_text(text)\n    return text";
    let ran = sandbox
        .run(copy, "f", &Map::new(), mounts(), far())
        .await
        .expect("mounted run");
    assert_eq!(ran.value, json!("hello"));
    assert_eq!(
        std::fs::read_to_string(work.path().join("b.txt")).expect("work file"),
        "hello"
    );

    for source in [
        "from pathlib import Path\ndef f():\n    Path('/inputs/a.txt').write_text('x')",
        "from pathlib import Path\ndef f():\n    Path('/work/c.txt').write_text('x' * 64)",
    ] {
        terminal(sandbox.run(source, "f", &Map::new(), mounts(), far()).await);
    }
    assert_eq!(
        std::fs::read_to_string(inputs.path().join("a.txt")).expect("input"),
        "hello"
    );
}

/// Run `source` in `workspace` the way `code.run` does: as the only `/work`
/// writer, with the mounts measured when its turn comes.
///
/// # Errors
/// Returns the mount or sandbox [`BlockError`]; files written before a
/// failure stay and keep counting.
async fn write(sandbox: &Sandbox, workspace: &Workspace, source: &str) -> Result<(), BlockError> {
    let _writer = workspace.writer().await;
    let mounts = workspace.mounts()?;
    sandbox
        .exec(source.to_owned(), Vec::new(), mounts, far())
        .await
        .map(drop)
}

/// `/work` writes are limited across calls, not per call: each call gets
/// only the space earlier calls left, counting files a failed call wrote,
/// overwrites, concurrent writers, a cancelled writer, and the scratch
/// inputs already use.
///
/// # Panics
/// Panics when writes together exceed the limit, earlier files are lost, or
/// a cancelled writer keeps the lock.
#[tokio::test]
async fn workspace_writes_share_one_budget() {
    let limits = HostedLimits {
        work_write_max_bytes: 16,
        scratch_max_bytes: 24,
        ..HostedLimits::default()
    };
    let sandbox = start(limits).await;
    let workspace = Workspace::materialize(
        vec![Staged {
            path: "/inputs/a.bin".to_owned(),
            bytes: vec![0; 4],
            format: None,
            sources: std::collections::BTreeSet::new(),
        }],
        limits,
        None,
    )
    .expect("workspace");
    let put = |name: &str, size: usize, then: &str| {
        format!("from pathlib import Path\nPath('/work/{name}').write_text('x' * {size})\n{then}")
    };
    // Written, then the call fails: the bytes still count.
    assert!(
        write(&sandbox, &workspace, &put("a", 8, "1 / 0"))
            .await
            .is_err()
    );
    // 16 allowed, 8 held: overwriting `a` with 8 more does not fit.
    assert!(write(&sandbox, &workspace, &put("a", 9, "")).await.is_err());
    write(&sandbox, &workspace, &put("b", 4, ""))
        .await
        .expect("fits");
    let (c, d) = (put("c", 4, ""), put("d", 4, ""));
    let (left, right) = tokio::join!(
        write(&sandbox, &workspace, &c),
        write(&sandbox, &workspace, &d),
    );
    assert_eq!(usize::from(left.is_ok()) + usize::from(right.is_ok()), 1);
    let work = workspace.mounts().expect("mounts")[1]
        .host_path()
        .to_owned();
    let held: u64 = std::fs::read_dir(&work)
        .expect("work")
        .map(|entry| entry.expect("entry").metadata().expect("meta").len())
        .sum();
    assert_eq!(held, 16);
    assert_eq!(
        std::fs::read_to_string(work.join("a")).expect("a"),
        "x".repeat(8)
    );

    // Inputs leave 20 bytes of scratch, so /work gets 12 of its 16.
    let tight = Workspace::materialize(
        vec![Staged {
            path: "/inputs/a.bin".to_owned(),
            bytes: vec![0; 12],
            format: None,
            sources: std::collections::BTreeSet::new(),
        }],
        HostedLimits {
            scratch_max_bytes: 24,
            ..limits
        },
        None,
    )
    .expect("workspace");
    assert!(write(&sandbox, &tight, &put("a", 13, "")).await.is_err());
    write(&sandbox, &tight, &put("a", 12, ""))
        .await
        .expect("fits");

    // A writer cancelled mid-call frees the writer lock; whatever it wrote
    // still counts, and the next writer gets only what is left.
    let fresh = Workspace::materialize(Vec::new(), limits, None).expect("workspace");
    let cancelled = tokio::time::timeout(
        Duration::from_secs(3),
        write(&sandbox, &fresh, &put("a", 8, "while True:\n    pass")),
    )
    .await;
    assert!(cancelled.is_err(), "the endless writer was cancelled");
    assert!(write(&sandbox, &fresh, &put("b", 9, "")).await.is_err());
    write(&sandbox, &fresh, &put("b", 8, ""))
        .await
        .expect("the lock is free and the rest fits");
}

/// Memory and time breaches end only the offending operation; the sandbox
/// keeps serving, and limits that cannot hold one worker refuse to start.
///
/// # Panics
/// Panics when a breach succeeds, poisons the pool, or impossible limits
/// start.
#[tokio::test]
async fn sandbox_contains_runaway_code() {
    let sandbox = start(HostedLimits {
        code_timeout: Duration::from_secs(1),
        ..HostedLimits::default()
    })
    .await;
    let reason = terminal(
        run(
            &sandbox,
            "def f():\n    return len('x' * (512 * 1024 * 1024))",
            "f",
            json!({}),
        )
        .await,
    );
    assert!(reason.contains("MemoryError"), "{reason}");
    // Small allocations pass every size precheck, so only the worker
    // allocator's live-byte count can stop this growth.
    let reason = terminal(
        run(
            &sandbox,
            "def f():\n    xs = []\n    while True:\n        xs.append('x' * 4096)",
            "f",
            json!({}),
        )
        .await,
    );
    assert!(reason.contains("MemoryError"), "{reason}");
    let reason = terminal(
        run(
            &sandbox,
            "def f():\n    while True:\n        pass",
            "f",
            json!({}),
        )
        .await,
    );
    assert!(reason.contains("time limit"), "{reason}");
    assert_eq!(
        run(&sandbox, "def f():\n    return 1", "f", json!({}))
            .await
            .expect("still serves")
            .value,
        json!(1)
    );

    for limits in [
        HostedLimits {
            monty_pool_memory_bytes: 1,
            ..HostedLimits::default()
        },
        HostedLimits {
            monty_process_memory_bytes: HostedLimits::default().monty_memory_bytes,
            ..HostedLimits::default()
        },
    ] {
        assert!(Sandbox::start(worker(), limits).await.is_err());
    }
}

/// Beyond `max_workers + max_waiters` operations are refused at once, and a
/// waiter gives up at its deadline; both are busy, the process-capacity
/// refusal the engine defers rather than retries or judges.
///
/// # Panics
/// Panics when admission or the checkout deadline is not enforced.
#[tokio::test]
async fn sandbox_bounds_workers_and_waiters() {
    let sandbox = start(HostedLimits {
        monty_max_workers: 1,
        monty_max_waiters: 1,
        code_timeout: Duration::from_secs(2),
        ..HostedLimits::default()
    })
    .await;
    let spin = "def f():\n    while True:\n        pass";
    let soon = Instant::now() + Duration::from_millis(300);
    let none = Map::new();
    let (busy, waiting, refused) = tokio::join!(
        run(&sandbox, spin, "f", json!({})),
        sandbox.run("def f():\n    return 1", "f", &none, Vec::new(), soon),
        run(&sandbox, "def f():\n    return 1", "f", json!({})),
    );
    terminal(busy);
    for (result, expected) in [(waiting, "deadline"), (refused, "capacity")] {
        match result {
            Err(BlockError::Busy { reason }) => assert!(reason.contains(expected), "{reason}"),
            other => panic!("expected a busy {expected} refusal, got {other:?}"),
        }
    }
}

/// Shutdown reaps every worker by its deadline and closes admission, and
/// never reports a checkout still running as settled.
///
/// A spinning operation holds the only worker when the first shutdown's
/// short deadline passes, so that shutdown reports `false`; the spin still
/// ends as a terminal failure rather than hanging. Repeating shutdown is safe
/// and settles. A sandbox whose operations all finished settles at once, and
/// either sandbox then refuses new work as busy.
///
/// # Panics
/// Panics when a busy checkout is reported settled, shutdown does not
/// settle once idle, or a closed sandbox admits work.
#[tokio::test]
async fn sandbox_shutdown_settles_checkouts_and_reaps_workers() {
    let busy = start(HostedLimits {
        monty_max_workers: 1,
        code_timeout: Duration::from_secs(2),
        ..HostedLimits::default()
    })
    .await;
    let spin = "def f():\n    while True:\n        pass";
    // `join!` polls the spin first, and its admission is taken before its
    // first await, so shutdown always finds the checkout live.
    let (spun, settled) = tokio::join!(
        run(&busy, spin, "f", json!({})),
        busy.shutdown(Instant::now() + Duration::from_millis(200)),
    );
    assert!(!settled, "a live checkout is never reported settled");
    terminal(spun);
    assert!(busy.shutdown(far()).await, "a repeated shutdown settles");

    let idle = start(HostedLimits::default()).await;
    let answered = run(&idle, "def f():\n    return 1", "f", json!({}))
        .await
        .expect("the operation answers");
    assert_eq!(answered.value, json!(1));
    assert!(idle.shutdown(far()).await, "an idle sandbox settles");
    for closed in [&busy, &idle] {
        match run(closed, "def f():\n    return 1", "f", json!({})).await {
            Err(BlockError::Busy { .. }) => {}
            other => panic!("a closed sandbox refuses work as busy, got {other:?}"),
        }
    }
}
