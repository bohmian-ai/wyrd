//! The Python sandbox against real `wyrd-server subprocess` Monty workers.
//!
//! Cargo builds this package's `wyrd-server` binary, which serves as its own
//! worker, in the same profile before it runs these tests.

use std::num::NonZeroUsize;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use monty_pool::{MountSpec, MountSpecMode};
use serde_json::{Map, Value, json};
use tokio::time::Instant;
use vala_verify::blocks::BlockError;
use wyrd_runtime::resources::{ByteLedger, ProcessResources};
use wyrd_server::verification::hosted::HostedLimits;
use wyrd_server::verification::hosted::sandbox::{CodeRun, Sandbox};
use wyrd_server::verification::hosted::workspace::{ScratchRoot, Staged, Workspace, Workspaces};
use wyrd_server::verification::telemetry::Envelopes;
use wyrd_spec::DataTenantId;

/// The worker binary: this package's `wyrd-server`, which serves as its own
/// worker, launched once to completion before its path is first returned.
///
/// The first launch of a freshly linked binary can pay a one-time operating
/// system verification cost longer than a test's code deadline, and a launch
/// killed before it finishes never completes that cost. Paying it here, with
/// no stdin, keeps it outside every sandbox deadline, as the production pool's
/// prewarmed workers do.
///
/// # Panics
/// Panics when the worker binary cannot be launched.
pub(crate) fn worker() -> PathBuf {
    static WORKER: OnceLock<PathBuf> = OnceLock::new();
    WORKER
        .get_or_init(|| {
            let path = PathBuf::from(env!("CARGO_BIN_EXE_wyrd-server"));
            Command::new(&path)
                .arg("subprocess")
                .env_clear()
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .expect("the worker binary launches");
            path
        })
        .clone()
}

/// Process resources of `memory` bytes on four CPUs with ample scratch.
pub(crate) fn process(memory: u64) -> ProcessResources {
    ProcessResources::new(
        NonZeroUsize::new(4).expect("4 is non-zero"),
        memory,
        16 << 30,
    )
}

/// Process resources with room for many concurrent operations.
pub(crate) fn roomy() -> ProcessResources {
    process(16 << 30)
}

/// Workspaces under `limits` in the system temporary directory, charged to
/// [`roomy`] process resources.
pub(crate) fn workspaces(limits: HostedLimits) -> Workspaces {
    Workspaces::new(limits, &roomy(), std::env::temp_dir())
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
        &roomy(),
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
        .run(
            DataTenantId::new_v7(),
            source,
            entrypoint,
            &inputs,
            Vec::new(),
            far(),
        )
        .await
}

/// A deadline no test reaches.
fn far() -> Instant {
    Instant::now() + Duration::from_secs(60)
}

/// The value of the rendered Prometheus `series` (name and labels exactly as
/// rendered), or zero when no sample was recorded. Every series read here
/// counts operations, workers, events, or bytes.
///
/// # Panics
/// Panics when the sample's value is not a whole number.
fn sample(rendered: &str, series: &str) -> u64 {
    rendered
        .lines()
        .find_map(|line| line.strip_prefix(series)?.strip_prefix(' '))
        .map_or(0, |value| value.parse().expect("a whole-number sample"))
}

/// The hosted capacity gauges a settled sandbox leaves at zero: active
/// operations and bytes, and runnable and suspended workers.
const SETTLED: [&str; 4] = [
    "wyrd_verification_hosted_operations{state=\"active\"}",
    "wyrd_verification_hosted_bytes{state=\"active\"}",
    "wyrd_verification_python_workers{state=\"runnable\"}",
    "wyrd_verification_python_workers{state=\"suspended\"}",
];

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
            DataTenantId::new_v7(),
            "print('hi')\nx = 2\nx * 21".to_owned(),
            Vec::new(),
            Vec::new(),
            far(),
            None,
            None,
            &Envelopes::default(),
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
        .run(
            DataTenantId::new_v7(),
            copy,
            "f",
            &Map::new(),
            mounts(),
            far(),
        )
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
        terminal(
            sandbox
                .run(
                    DataTenantId::new_v7(),
                    source,
                    "f",
                    &Map::new(),
                    mounts(),
                    far(),
                )
                .await,
        );
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
        .exec(
            DataTenantId::new_v7(),
            source.to_owned(),
            Vec::new(),
            mounts,
            far(),
            None,
            None,
            &Envelopes::default(),
        )
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
        Box::pin(write(
            &sandbox,
            &fresh,
            &put("a", 8, "while True:\n    pass"),
        )),
    )
    .await;
    assert!(cancelled.is_err(), "the endless writer was cancelled");
    assert!(write(&sandbox, &fresh, &put("b", 9, "")).await.is_err());
    write(&sandbox, &fresh, &put("b", 8, ""))
        .await
        .expect("the lock is free and the rest fits");
}

/// A Python `/work` write that process scratch cannot hold stops the
/// operation busy before it runs, even when the program catches `OSError`,
/// so process load never reaches the program or its result; a write that
/// fits is charged exactly its bytes.
///
/// # Panics
/// Panics when the refused write reaches Python, lands, or leaves a charge,
/// or a write that fits is refused or charged wrongly.
#[tokio::test]
async fn python_writes_beyond_process_scratch_stop_busy() {
    let limits = HostedLimits {
        monty_min_workers: 0,
        ..HostedLimits::default()
    };
    let sandbox = start(limits).await;
    let scratch = ByteLedger::new(8);
    let workspace = Arc::new(
        Workspace::materialize(
            Vec::new(),
            limits,
            ScratchRoot {
                dir: std::env::temp_dir(),
                ledger: scratch.clone(),
            },
        )
        .expect("workspace"),
    );
    let envelopes = Envelopes::default();
    let exec = |size: usize| {
        let source = format!(
            "from pathlib import Path\ntry:\n    Path('/work/a').write_text('x' * {size})\n    result = 'written'\nexcept OSError:\n    result = 'caught'\nresult"
        );
        let mounts = workspace.mounts().expect("mounts");
        sandbox.exec(
            DataTenantId::new_v7(),
            source,
            Vec::new(),
            mounts,
            far(),
            Some(&workspace),
            None,
            &envelopes,
        )
    };

    let refused = exec(16).await.expect_err("process scratch is full");
    assert!(
        matches!(&refused, BlockError::Busy { reason } if reason.contains("scratch")),
        "{refused}"
    );
    assert_eq!(scratch.used(), 0, "the refused write leaves no charge");
    assert!(
        !workspace.mounts().expect("mounts")[1]
            .host_path()
            .join("a")
            .exists()
    );

    let (value, _) = exec(4).await.expect("the write fits");
    assert_eq!(value.py_repr(), "'written'");
    assert_eq!(scratch.used(), 4);
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

    let limits = HostedLimits::default();
    let envelope = Sandbox::envelope_bytes(&limits).expect("the envelope fits u64");
    assert!(
        Sandbox::start(worker(), limits, &process(envelope))
            .await
            .is_err(),
        "process memory below one session's envelope refuses to start"
    );
}

/// With process memory for one operation, a waiter whose deadline passes,
/// and one whose waiting bytes find no memory at all, are busy; then of two
/// concurrent operations the second waits, the idle worker is reaped for it,
/// and it runs. Busy is the process-capacity refusal the engine
/// defers rather than retries or judges.
///
/// # Panics
/// Panics when an operation that fits is refused, or a refusal is not busy.
#[tokio::test]
async fn sandbox_waits_for_process_memory() {
    let limits = HostedLimits {
        monty_min_workers: 0,
        code_timeout: Duration::from_secs(5),
        ..HostedLimits::default()
    };
    let one = Sandbox::envelope_bytes(&limits).expect("the envelope fits u64");
    let sandbox = Sandbox::start(worker(), limits, &process(one + 4096))
        .await
        .expect("pool starts");
    let none = Map::new();
    let occupied = sandbox.occupy_for_test(4096);
    let soon = Instant::now() + Duration::from_millis(100);
    match sandbox
        .run(
            DataTenantId::new_v7(),
            "def f():\n    return 1",
            "f",
            &none,
            Vec::new(),
            soon,
        )
        .await
    {
        Err(BlockError::Busy { reason }) => assert!(reason.contains("deadline"), "{reason}"),
        other => panic!("expected a busy deadline refusal, got {other:?}"),
    }
    drop(occupied);
    let occupied = sandbox.occupy_for_test(0);
    match run(&sandbox, "def f():\n    return 1", "f", json!({})).await {
        Err(BlockError::Busy { reason }) => assert!(reason.contains("cannot hold"), "{reason}"),
        other => panic!("expected a busy memory refusal, got {other:?}"),
    }
    drop(occupied);

    let count =
        "def f(n):\n    total = 0\n    for i in range(n):\n        total += i\n    return total";
    let (first, second) = tokio::join!(
        run(&sandbox, count, "f", json!({"n": 100_000})),
        run(&sandbox, count, "f", json!({"n": 10})),
    );
    assert_eq!(
        first.expect("the first runs").value,
        json!(4_999_950_000_u64)
    );
    assert_eq!(
        second.expect("the second waits, then runs").value,
        json!(45)
    );
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

/// Whether process `pid` still exists, zombie or not, by asking `ps`, which
/// both supported platforms ship.
///
/// # Panics
/// Panics when `ps` cannot run.
fn exists(pid: u32) -> bool {
    Command::new("ps")
        .args(["-p", &pid.to_string()])
        .stdout(Stdio::null())
        .status()
        .expect("ps runs")
        .success()
}

/// The resident set of process `pid` in bytes, from `ps`.
///
/// # Panics
/// Panics when `ps` cannot run or reports no size.
fn resident_set(pid: u32) -> u64 {
    let out = Command::new("ps")
        .args(["-o", "rss=", "-p", &pid.to_string()])
        .output()
        .expect("ps runs");
    String::from_utf8_lossy(&out.stdout)
        .trim()
        .parse::<u64>()
        .expect("ps reports the resident set in KiB")
        * 1024
}

/// Wait until every charged worker that is gone has had its charge returned
/// and `charged` workers remain, checking on every poll that no worker's
/// charge returned while its process still existed.
///
/// # Panics
/// Panics when a live worker lost its charge, or the charges do not settle
/// within ten seconds.
async fn settle(sandbox: &Sandbox, charged: usize) {
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut seen = sandbox.charged_worker_pids();
    loop {
        let now = sandbox.charged_worker_pids();
        for pid in seen.iter().filter(|pid| !now.contains(pid)) {
            assert!(
                !exists(*pid),
                "worker {pid} lost its charge while it still exists"
            );
        }
        if now.len() == charged {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "worker charges never settled: {now:?}"
        );
        seen = now;
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

/// One healthy worker serves tenant A, then tenant B, then A again. Each
/// session starts fresh: B sees none of A's globals, mounts, or output, and
/// a Python error in B still resets the worker for reuse. Between sessions
/// the idle worker keeps one session envelope charged, never more, and that
/// charge covers its resident set after a session that allocated most of its
/// budget.
///
/// # Panics
/// Panics when the process is not reused, a session sees another's state,
/// an idle worker's charge grows across sessions, or its resident set exceeds
/// its charge, or the worker telemetry does not count one spawn and a reset
/// per session.
#[tokio::test]
async fn sandbox_reuses_processes_with_fresh_tenant_sessions() {
    let recorder = metrics_exporter_prometheus::PrometheusBuilder::new().build_recorder();
    let metrics = recorder.handle();
    let _recording = metrics::set_default_local_recorder(&recorder);
    let limits = HostedLimits {
        monty_min_workers: 0,
        ..HostedLimits::default()
    };
    let process = roomy();
    let memory = process.memory().clone();
    let sandbox = Sandbox::start(worker(), limits, &process)
        .await
        .expect("pool starts");
    let (a, b) = (DataTenantId::new_v7(), DataTenantId::new_v7());
    let inputs = tempfile::tempdir().expect("inputs");
    std::fs::write(inputs.path().join("secret.txt"), "for A").expect("input written");
    let mounts =
        || vec![MountSpec::new("/inputs", inputs.path(), MountSpecMode::ReadOnly).expect("inputs")];
    let envelopes = Envelopes::default();
    let exec = |tenant, code: &str, mounts| {
        sandbox.exec(
            tenant,
            code.to_owned(),
            Vec::new(),
            mounts,
            far(),
            None,
            None,
            &envelopes,
        )
    };

    let (value, output) = exec(
        a,
        "from pathlib import Path\nsecret = Path('/inputs/secret.txt').read_text()\nprint(secret)\nblob = 'x' * (200 * 1024 * 1024)\nlen(blob)",
        mounts(),
    )
    .await
    .expect("A runs");
    assert_eq!(
        (value.py_repr().as_str(), output.as_str()),
        ("209715200", "for A\n")
    );
    let pids = sandbox.idle_worker_pids();
    assert_eq!(pids.len(), 1, "A's worker returns to the pool");
    let envelope = Sandbox::envelope_bytes(&limits).expect("the envelope fits u64");
    assert_eq!(
        (memory.used(), sandbox.worker_bytes()),
        (envelope, envelope),
        "the idle worker keeps its session envelope"
    );
    let rss = resident_set(pids[0]);
    assert!(
        rss <= envelope,
        "the reset worker's resident set {rss} exceeds its {envelope}-byte charge"
    );

    for probe in ["secret", "blob", "open('/inputs/secret.txt').read()"] {
        let failed = exec(b, probe, Vec::new()).await;
        assert!(
            matches!(&failed, Err(BlockError::Terminal { reason }) if !reason.contains("for A")),
            "B must not see A's {probe}: {failed:?}"
        );
        assert_eq!(
            sandbox.idle_worker_pids(),
            pids,
            "a Python error still reuses the worker"
        );
    }
    let (value, output) = exec(b, "1 + 1", Vec::new()).await.expect("B runs");
    assert_eq!((value.py_repr().as_str(), output.as_str()), ("2", ""));
    let (value, _) = exec(
        a,
        "try:\n    secret\n    seen = True\nexcept NameError:\n    seen = False\nseen",
        Vec::new(),
    )
    .await
    .expect("A runs again");
    assert_eq!(value.py_repr(), "False");
    assert_eq!(
        sandbox.idle_worker_pids(),
        pids,
        "one process served every session"
    );
    assert_eq!(
        memory.used(),
        envelope,
        "later sessions take the charge over"
    );
    let rendered = metrics.render();
    let event = |event: &str| {
        sample(
            &rendered,
            &format!("wyrd_verification_python_worker_events_total{{event=\"{event}\"}}"),
        )
    };
    assert_eq!(
        (event("spawn"), event("reset"), event("reap")),
        (1, 6, 0),
        "one worker spawned and reset after each of six sessions: {rendered}"
    );
    for series in SETTLED {
        assert_eq!(sample(&rendered, series), 0, "{series}: {rendered}");
    }
}

/// A program suspended on the `/work` writer gives up its runnable turn but
/// keeps its envelope: with one effective CPU, another operation runs to
/// completion meanwhile, and the suspended one finishes once the writer frees.
///
/// # Panics
/// Panics when the suspended program blocks the other operation, releases
/// its envelope while suspended, or does not finish, or the telemetry does
/// not show one active, suspended operation holding its envelope.
#[tokio::test]
async fn suspended_sessions_release_runnable_capacity() {
    let recorder = metrics_exporter_prometheus::PrometheusBuilder::new().build_recorder();
    let metrics = recorder.handle();
    let _recording = metrics::set_default_local_recorder(&recorder);
    let limits = HostedLimits {
        monty_min_workers: 0,
        ..HostedLimits::default()
    };
    let process = ProcessResources::new(NonZeroUsize::MIN, 16 << 30, 16 << 30);
    let memory = process.memory().clone();
    let sandbox = Sandbox::start(worker(), limits, &process)
        .await
        .expect("pool starts");
    assert_eq!(sandbox.runnable_turns(), 1);
    let workspace = Arc::new(Workspace::materialize(Vec::new(), limits, None).expect("workspace"));
    let mounts = workspace.mounts().expect("mounts");
    let writer = workspace.writer().await;
    let envelopes = Envelopes::default();
    let suspended = sandbox.exec(
        DataTenantId::new_v7(),
        "from pathlib import Path\nPath('/work/a.txt').write_text('done')\n'written'".to_owned(),
        Vec::new(),
        mounts,
        far(),
        Some(&workspace),
        None,
        &envelopes,
    );
    tokio::pin!(suspended);
    let envelope = Sandbox::envelope_bytes(&limits).expect("the envelope fits u64");
    // Active, suspended, and holding no turn: with nothing else running, only
    // a program parked on the held writer shows this.
    let parked = [1, envelope, 0, 1];
    let settled = || {
        let rendered = metrics.render();
        SETTLED.map(|series| sample(&rendered, series)) == parked
    };
    let deadline = Instant::now() + Duration::from_secs(10);
    while !settled() {
        assert!(
            Instant::now() < deadline,
            "the program never parked on the writer"
        );
        tokio::select! {
            biased;
            ended = &mut suspended => panic!("the program ended while its writer was held: {ended:?}"),
            () = tokio::time::sleep(Duration::from_millis(20)) => {}
        }
    }
    let other = run(&sandbox, "def f():\n    return 7", "f", json!({})).await;
    assert_eq!(other.expect("the other operation runs").value, json!(7));
    assert!(
        memory.used() >= envelope,
        "the suspended program keeps its envelope while suspended"
    );
    assert!(settled(), "{}", metrics.render());
    drop(writer);
    let (value, _) = suspended.await.expect("the program finishes");
    assert_eq!(value.py_repr(), "'written'");
    assert_eq!(sandbox.runnable_turns(), 1);
    let rendered = metrics.render();
    for series in SETTLED {
        assert_eq!(sample(&rendered, series), 0, "{series}: {rendered}");
    }
}

/// A worker killed by its time limit or by a cancelled operation keeps its whole charge until its process is gone,
/// and later operations still run.
///
/// # Panics
/// Panics when a charge returns while its process exists, it never
/// returns, or later work fails, or the worker telemetry does not count each
/// spawn, the one reset, and both reaps.
#[tokio::test]
async fn failed_workers_settle_before_capacity_returns() {
    let recorder = metrics_exporter_prometheus::PrometheusBuilder::new().build_recorder();
    let metrics = recorder.handle();
    let _recording = metrics::set_default_local_recorder(&recorder);
    let limits = HostedLimits {
        monty_min_workers: 0,
        code_timeout: Duration::from_secs(1),
        ..HostedLimits::default()
    };
    let process = roomy();
    let memory = process.memory().clone();
    let sandbox = Sandbox::start(worker(), limits, &process)
        .await
        .expect("pool starts");
    let spin = "def f():\n    while True:\n        pass";

    let reason = terminal(run(&sandbox, spin, "f", json!({})).await);
    assert!(reason.contains("time limit"), "{reason}");
    settle(&sandbox, 0).await;

    let cancelled = tokio::time::timeout(
        Duration::from_millis(300),
        Box::pin(run(&sandbox, spin, "f", json!({}))),
    )
    .await;
    assert!(cancelled.is_err(), "the spin is cancelled mid-run");
    settle(&sandbox, 0).await;
    assert_eq!(
        memory.used(),
        0,
        "every failed worker's charge returned once it was gone"
    );

    let ran = run(&sandbox, "def f():\n    return 1", "f", json!({}))
        .await
        .expect("later work runs");
    assert_eq!(ran.value, json!(1));
    assert_eq!(sandbox.charged_worker_pids().len(), 1);
    let rendered = metrics.render();
    let event = |event: &str| {
        sample(
            &rendered,
            &format!("wyrd_verification_python_worker_events_total{{event=\"{event}\"}}"),
        )
    };
    assert_eq!(
        (event("spawn"), event("reset"), event("reap")),
        (3, 1, 2),
        "two discarded workers reaped and a third reset: {rendered}"
    );
    for series in SETTLED {
        assert_eq!(sample(&rendered, series), 0, "{series}: {rendered}");
    }
}

/// Prewarmed workers are charged like any worker, and when an operation's
/// envelope does not fit beside them they are reaped so it runs, leaving
/// fewer than `monty_min_workers`.
///
/// # Panics
/// Panics when prewarmed workers are uncharged, block the operation, or
/// survive the pressure.
#[tokio::test]
async fn prewarmed_workers_are_charged_and_reaped_under_pressure() {
    let limits = HostedLimits {
        monty_min_workers: 2,
        ..HostedLimits::default()
    };
    let envelope = Sandbox::envelope_bytes(&limits).expect("the envelope fits u64");
    let process = process(envelope + 1024);
    let memory = process.memory().clone();
    let sandbox = Sandbox::start(worker(), limits, &process)
        .await
        .expect("pool starts");
    let warm = sandbox.idle_worker_pids();
    assert_eq!(warm.len(), 2, "both prewarmed workers fit");
    assert_eq!(memory.used(), sandbox.worker_bytes());
    assert!(memory.used() > 0, "prewarmed workers are charged");

    let ran = run(&sandbox, "def f():\n    return 1", "f", json!({}))
        .await
        .expect("the operation runs once idle workers are reaped");
    assert_eq!(ran.value, json!(1));
    settle(&sandbox, 1).await;
    for pid in warm {
        assert!(
            !exists(pid),
            "prewarmed worker {pid} was reaped under pressure"
        );
    }
}
