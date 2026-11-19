//! Workspace admission, which opens through the execution's evidence
//! reader and therefore the shared Postgres fixture.

use std::collections::{BTreeMap, BTreeSet};
use std::num::NonZeroUsize;

use futures_util::FutureExt;
use serde_json::json;
use vala_verify::blocks::{CallInput, CallValue};

use super::*;
use crate::test_support::{test_caller, test_state};

/// Process resources of `memory` and `scratch` bytes on one CPU.
fn process(memory: u64, scratch: u64) -> ProcessResources {
    ProcessResources::new(NonZeroUsize::MIN, memory, scratch)
}

/// One JSON input of about 60 KiB at `/inputs/a.txt`.
fn large_input() -> BTreeMap<String, CallInput> {
    BTreeMap::from([(
        "/inputs/a.txt".to_owned(),
        CallInput {
            value: CallValue::Json(json!("line\n".repeat(10_000)).into()),
            sources: BTreeSet::new(),
        },
    )])
}

/// An open workspace lives under its scratch root and charges process
/// scratch, not memory, once its inputs are written; dropping it empties
/// the directory, then returns the charge, and the next open reuses the
/// emptied directory with none of the earlier files.
///
/// # Panics
/// Panics when the directory is misplaced, keeps a file past the drop or is
/// not reused, or a charge is wrong.
#[test]
fn workspace_cleans_up_on_drop() {
    wyrd_runtime::runtime().block_on(async {
        let limits = HostedLimits::default();
        let process = process(1 << 30, 1 << 30);
        let root = tempfile::tempdir().expect("scratch root");
        let evidence = TenantEvidence::new(
            test_state().await,
            test_caller([]),
            limits,
            process.memory().clone(),
        );
        let workspaces = Workspaces::new(limits, &process, root.path().to_owned());
        let files = BTreeMap::from([(
            "/inputs/a.json".to_owned(),
            CallInput {
                value: CallValue::Json(json!({"a": 1}).into()),
                sources: BTreeSet::new(),
            },
        )]);

        let workspace = workspaces.open(&files, &evidence).await.expect("opens");
        let dir = workspace.inputs_dir();
        assert_eq!(
            std::fs::read(dir.join("a.json")).expect("input"),
            serde_json::to_vec_pretty(&json!({"a": 1})).expect("encodes"),
            "the JSON input is placed as its pretty encoding"
        );
        assert_eq!(
            std::fs::read_dir(dir.with_file_name("staging"))
                .expect("staging")
                .count(),
            0,
            "a JSON input leaves no staging file"
        );
        assert!(dir.starts_with(root.path()), "{}", dir.display());
        assert_eq!(process.memory().used(), 0, "staged bytes end once written");
        let written = std::fs::metadata(dir.join("a.json")).expect("input").len();
        assert_eq!(process.scratch().used(), written);
        let work = workspace.work_dir();
        std::fs::create_dir(work.join("nested")).expect("a code-made directory");
        std::fs::write(work.join("nested/out.txt"), b"x").expect("a code-made file");

        let returned = process.scratch().changed();
        let mut returned = std::pin::pin!(returned);
        returned.as_mut().enable();
        drop(workspace);
        tokio::time::timeout(std::time::Duration::from_secs(10), returned)
            .await
            .expect("the charge returns once the directory is emptied");
        assert_eq!(process.scratch().used(), 0);
        for empty in [&dir, &work] {
            assert_eq!(
                std::fs::read_dir(empty).expect("kept").count(),
                0,
                "{}",
                empty.display()
            );
        }

        let reused = workspaces
            .open(&BTreeMap::new(), &evidence)
            .await
            .expect("reopens");
        assert_eq!(reused.inputs_dir(), dir, "the emptied directory is reused");
    });
}

/// Dropping `open` while its blocking materialization runs leaks no
/// charge: once the started task returns, its workspace drops and every
/// byte comes back.
///
/// # Panics
/// Panics when the first poll completes or a charge never returns.
#[test]
fn cancelled_open_returns_its_charges() {
    wyrd_runtime::runtime().block_on(async {
        let limits = HostedLimits::default();
        let process = process(1 << 30, 1 << 30);
        let root = tempfile::tempdir().expect("scratch root");
        let evidence = TenantEvidence::new(
            test_state().await,
            test_caller([]),
            limits,
            process.memory().clone(),
        );
        let workspaces = Workspaces::new(limits, &process, root.path().to_owned());

        let files = large_input();
        let mut open = Box::pin(workspaces.open(&files, &evidence));
        assert!((&mut open).now_or_never().is_none());
        drop(open);
        tokio::time::timeout(std::time::Duration::from_secs(10), async {
            loop {
                let changed = process.scratch().changed();
                tokio::pin!(changed);
                changed.as_mut().enable();
                if process.scratch().used() == 0 && process.memory().used() == 0 {
                    return;
                }
                changed.await;
            }
        })
        .await
        .expect("the cancelled open's charges return");
        assert_eq!(std::fs::read_dir(root.path()).expect("root").count(), 0);
    });
}

/// Inputs that process memory cannot stage are refused busy at once.
///
/// # Panics
/// Panics when the open waits or is not refused busy, or leaves a charge.
#[test]
fn workspace_inputs_beyond_process_memory_are_busy() {
    wyrd_runtime::runtime().block_on(async {
        let limits = HostedLimits::default();
        let process = process(1024, 1 << 30);
        let root = tempfile::tempdir().expect("scratch root");
        let evidence = TenantEvidence::new(
            test_state().await,
            test_caller([]),
            limits,
            process.memory().clone(),
        );
        let workspaces = Workspaces::new(limits, &process, root.path().to_owned());

        let refused = workspaces
            .open(&large_input(), &evidence)
            .now_or_never()
            .expect("admission never waits");

        assert!(
            matches!(&refused, Err(BlockError::Busy { reason }) if reason.contains("memory")),
            "{:?}",
            refused.map(drop)
        );
        assert_eq!(process.memory().used(), 0);
    });
}

/// Inputs that process scratch cannot hold are refused busy, and the
/// partial workspace is removed with its charge.
///
/// # Panics
/// Panics when the open is not refused busy or leaves files or a charge.
#[test]
fn workspace_inputs_beyond_process_scratch_are_busy() {
    wyrd_runtime::runtime().block_on(async {
        let limits = HostedLimits::default();
        let process = process(1 << 30, 1024);
        let root = tempfile::tempdir().expect("scratch root");
        let evidence = TenantEvidence::new(
            test_state().await,
            test_caller([]),
            limits,
            process.memory().clone(),
        );
        let workspaces = Workspaces::new(limits, &process, root.path().to_owned());

        let refused = workspaces.open(&large_input(), &evidence).await;

        assert!(
            matches!(&refused, Err(BlockError::Busy { reason }) if reason.contains("scratch")),
            "{:?}",
            refused.map(drop)
        );
        assert_eq!(process.scratch().used(), 0);
        assert_eq!(process.memory().used(), 0);
        assert_eq!(std::fs::read_dir(root.path()).expect("root").count(), 0);
    });
}

/// Records the name of every span opened under it.
struct SpanNames(std::sync::Arc<std::sync::Mutex<Vec<&'static str>>>);

impl<S: tracing::Subscriber> tracing_subscriber::Layer<S> for SpanNames {
    /// Record the new span's name.
    fn on_new_span(
        &self,
        attrs: &tracing::span::Attributes<'_>,
        _: &tracing::span::Id,
        _: tracing_subscriber::layer::Context<'_, S>,
    ) {
        self.0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(attrs.metadata().name());
    }
}

/// An open is profiled under the engine phase as its own
/// `verification.workspace.open` span and a `verification.workspace.install`
/// span for the blocking placement pass, so a benchmark profile attributes
/// staging time to the workspace.
///
/// # Panics
/// Panics when the open fails or either span is missing.
#[test]
fn workspace_open_is_profiled_as_its_own_spans() {
    use tracing_subscriber::layer::SubscriberExt as _;

    let names = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let subscriber = tracing_subscriber::registry().with(SpanNames(std::sync::Arc::clone(&names)));
    tracing::subscriber::with_default(subscriber, || {
        wyrd_runtime::runtime().block_on(async {
            let limits = HostedLimits::default();
            let process = process(1 << 30, 1 << 30);
            let root = tempfile::tempdir().expect("scratch root");
            let evidence = TenantEvidence::new(
                test_state().await,
                test_caller([]),
                limits,
                process.memory().clone(),
            );
            let workspaces = Workspaces::new(limits, &process, root.path().to_owned());
            workspaces
                .open(&large_input(), &evidence)
                .await
                .expect("opens");
        });
    });
    let names = names
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    for span in [
        "verification.workspace.open",
        "verification.workspace.install",
    ] {
        assert!(names.contains(&span), "{span} is opened: {names:?}");
    }
}
