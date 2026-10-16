//! The Python sandbox against the real hardened `wyrd-monty-worker`.
//!
//! Ignored in the fast lane because they need the worker binary; `mise run
//! test:verification:sandbox` builds it and runs them.

use std::io::{Read, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::Duration;

use monty_pool::{MountSpec, MountSpecMode};
use serde_json::{Map, Value, json};
use tokio::time::Instant;
use vala_eval::blocks::BlockError;
use wyrd_server::verification::hosted::HostedLimits;
use wyrd_server::verification::hosted::sandbox::{CodeRun, Sandbox};

/// Why every test here is ignored in the fast lane.
const NEEDS_WORKER: &str = "needs the Monty worker; run `mise run test:verification:sandbox`";

/// The worker binary: `WYRD_VERIFICATION_MONTY_WORKER`, else the
/// `build:monty-worker` output.
///
/// # Panics
/// Panics when the binary is missing.
fn worker() -> PathBuf {
    let path = std::env::var_os("WYRD_VERIFICATION_MONTY_WORKER").map_or_else(
        || {
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../../target/monty-worker/release/wyrd-monty-worker")
        },
        PathBuf::from,
    );
    assert!(path.is_file(), "{}: {NEEDS_WORKER}", path.display());
    path
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

/// The worker confines itself before serving: new user, network, and IPC
/// namespaces, and `no_new_privs`.
///
/// # Panics
/// Panics when any confinement is missing.
#[test]
#[ignore = "needs the Monty worker; run `mise run test:verification:sandbox`"]
fn worker_enters_isolation_before_serving() {
    let mut child = Command::new(worker())
        .arg("subprocess")
        .env_clear()
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("worker spawns");
    // An undecodable frame draws a protocol-violation reply, which the
    // worker can only send after confinement.
    let mut stdin = child.stdin.take().expect("stdin");
    stdin.write_all(&[1, 0, 0, 0, 0xff]).expect("frame written");
    let mut reply = [0_u8; 4];
    child
        .stdout
        .as_mut()
        .expect("stdout")
        .read_exact(&mut reply)
        .expect("worker replies");

    let pid = child.id();
    for namespace in ["user", "net", "ipc"] {
        let ours = std::fs::read_link(format!("/proc/self/ns/{namespace}")).expect("own ns");
        let its = std::fs::read_link(format!("/proc/{pid}/ns/{namespace}")).expect("worker ns");
        assert_ne!(ours, its, "{namespace} namespace is shared");
    }
    let status = std::fs::read_to_string(format!("/proc/{pid}/status")).expect("status");
    assert!(
        status.lines().any(|line| line == "NoNewPrivs:\t1"),
        "{status}"
    );
    drop(stdin);
    assert!(child.wait().expect("worker exits").success());
}

/// Entry points receive JSON inputs, return JSON, and keep bounded prints;
/// nothing defined by one operation is visible to the next.
///
/// # Panics
/// Panics when values, prints, or state cross wrongly.
#[tokio::test]
#[ignore = "needs the Monty worker; run `mise run test:verification:sandbox`"]
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

    let reason = terminal(run(&sandbox, "def g():\n    return seen", "g", json!({})).await);
    assert!(reason.contains("NameError"), "{reason}");
    assert!(
        terminal(run(&sandbox, "", "f; import os", json!({})).await)
            .contains("not a Python function")
    );
}

/// Host files and functions are unreachable; mounts are the only paths, and
/// `/inputs` is read-only.
///
/// # Panics
/// Panics when host state is reachable or a mount mode is ignored.
#[tokio::test]
#[ignore = "needs the Monty worker; run `mise run test:verification:sandbox`"]
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

/// Memory, time, and group-memory breaches end only the offending
/// operation; the sandbox keeps serving.
///
/// # Panics
/// Panics when a breach succeeds or poisons the pool.
#[tokio::test]
#[ignore = "needs the Monty worker; run `mise run test:verification:sandbox`"]
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

    let starved = start(HostedLimits {
        monty_pool_memory_bytes: 1,
        ..HostedLimits::default()
    })
    .await;
    let reason = terminal(
        run(
            &starved,
            "def f():\n    while True:\n        pass",
            "f",
            json!({}),
        )
        .await,
    );
    assert!(reason.contains("stopped"), "{reason}");
}

/// Beyond `max_workers + max_waiters` operations are refused at once, and a
/// waiter gives up at its deadline; both are retryable.
///
/// # Panics
/// Panics when admission or the checkout deadline is not enforced.
#[tokio::test]
#[ignore = "needs the Monty worker; run `mise run test:verification:sandbox`"]
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
            Err(BlockError::Retryable { reason }) => assert!(reason.contains(expected), "{reason}"),
            other => panic!("expected a retryable {expected} failure, got {other:?}"),
        }
    }
}
