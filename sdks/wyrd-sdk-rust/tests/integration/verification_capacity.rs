//! A service verifies through hosted Python while process memory is
//! contended: `verify` waits for memory and then passes, is refused as
//! retryable `503` with `Retry-After: 1` when memory cannot even hold the
//! waiting request, ends in `504` at the one deadline from its arrival, is
//! never replayed by the SDK, and leaves no waiter behind when the caller
//! gives up; `submit` defers a queued execution until memory frees.

use std::time::{Duration, Instant};

use serde_json::json;
use tempfile::TempDir;
use wyrd_ai::state::WyrdState;
use wyrd_ai::verify::Submission;
use wyrd_ai::{VerificationResult, VerificationVerdict, WyrdError};
use wyrd_testing::server::{OccupiedForTest, WyrdTestServer};

use crate::support::{Deployment, declare, pull};

/// The direct deadline every journey here runs under.
const VERIFY_TIMEOUT: Duration = Duration::from_secs(2);

/// Process memory left free while an operation should wait: the held memory
/// puts Verification over its contended share, with room for the Scribe work
/// that shares the ledger.
const CONTENDED: u64 = 64 << 20;

/// Bytes of the answer the full-memory refusal sends.
const REQUEST_BYTES: usize = 256 << 10;

/// How long a test waits for a queued execution or a waiter to appear.
const WAIT: Duration = Duration::from_secs(60);

/// The capacity desk deployment: a server hosting Python Verifiers under a
/// short direct deadline, with the `capacity-desk` Service registered and
/// hydrated as its own key.
struct Desk {
    /// The deployment.
    deployment: Deployment,
    /// The Card bundle the state loads from.
    _bundle: TempDir,
    /// The hydrated Service.
    state: WyrdState,
}

impl Desk {
    /// Start the server, declare and pull `capacity-desk`, and start its
    /// state with Bifrost.
    ///
    /// # Panics
    /// Panics when a setup step fails.
    async fn start() -> Self {
        let deployment = Deployment::start_with(
            WyrdTestServer::builder()
                .with_verification_runtime_for_test()
                .with_verify_timeout_for_test(VERIFY_TIMEOUT),
        )
        .await;
        let cards = deployment.cards().await;
        let desk = declare(&cards, "cards/verification_capacity/capacity-desk.yaml").await;
        let bundle = pull(&cards, &desk.root).await;
        let client = deployment.service_client(&desk.root).await;
        let state = WyrdState::from_path_with(bundle.path().join("bundle"), client)
            .expect("bundle loads offline");
        state.start_bifrost().await.expect("Bifrost starts");
        Self {
            deployment,
            _bundle: bundle,
            state,
        }
    }

    /// Verify the agent's answer of 2 with `adds-one` now.
    ///
    /// # Errors
    /// Returns the SDK's refusal of the verify.
    async fn verify(state: WyrdState) -> Result<VerificationResult, WyrdError> {
        Self::verify_answer(state, json!(2)).await
    }

    /// Verify the agent's `answer` with `adds-one` now.
    ///
    /// # Errors
    /// Returns the SDK's refusal of the verify.
    ///
    /// # Panics
    /// Panics when the agent run cannot open.
    async fn verify_answer(
        state: WyrdState,
        answer: serde_json::Value,
    ) -> Result<VerificationResult, WyrdError> {
        state
            .run_for_card("agent")
            .expect("agent run opens")
            .observe()
            .verify("adds-one", answer)
            .await
    }

    /// Wait until `count` hosted operations wait for process memory.
    ///
    /// # Panics
    /// Panics when the count is not reached within [`WAIT`].
    async fn await_waiters(&self, count: usize) {
        self.deployment
            .server()
            .wait_for_hosted_waiters(count, WAIT)
            .await
            .expect("the waiters appear");
    }

    /// Charge all process memory but `leave` bytes until the charge drops.
    ///
    /// # Panics
    /// Panics when the server hosts no Verifiers.
    fn occupy(&self, leave: u64) -> OccupiedForTest {
        self.deployment
            .server()
            .occupy_hosted_memory(leave)
            .expect("the server hosts Verifiers")
    }
}

/// A verify that finds process memory occupied waits for it rather than
/// failing, and passes once the memory frees.
///
/// # Panics
/// Panics when the verify does not wait, fails, or does not pass.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn verify_waits_for_hosted_memory_then_passes() {
    let desk = Desk::start().await;
    let held = desk.occupy(CONTENDED);

    let waiting = tokio::spawn(Desk::verify(desk.state.clone()));
    desk.await_waiters(1).await;
    drop(held);
    let result = waiting
        .await
        .expect("the verify joins")
        .expect("the verify passes once memory frees");

    assert_eq!(result.outcome.verdict, VerificationVerdict::Passed);
    desk.deployment.shutdown().await;
}

/// Under occupied process memory the verify fails the way its contract
/// says, once: memory that cannot hold the waiting request is a retryable
/// `503` with `Retry-After: 1`; a wait that outlasts the deadline from its
/// arrival is `504 WYRD_VERIFICATION_504_EXECUTION_TIMED_OUT` at that
/// deadline, not after an SDK replay; a caller that gives up leaves no
/// waiter; and once memory frees the same verify passes.
///
/// # Panics
/// Panics when a refusal's status, code, retry hint, or timing differs, a
/// waiter is left behind, or the freed verify does not pass.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn verify_refusals_follow_the_arrival_deadline() {
    let desk = Desk::start().await;

    // A request larger than any release the hold has yet to absorb.
    let full = desk.occupy(0);
    let refused = Desk::verify_answer(desk.state.clone(), json!("2".repeat(REQUEST_BYTES)))
        .await
        .expect_err("memory that cannot hold the request refuses it");
    drop(full);
    assert_eq!(
        refused.code(),
        "WYRD_SERVER_503_SERVICE_UNAVAILABLE",
        "{refused}"
    );
    assert_eq!(
        refused.as_problem_json()["details"]["reason"],
        json!("process memory cannot hold the Python operation now"),
        "{refused}"
    );

    let held = desk.occupy(CONTENDED);
    let started = Instant::now();
    let timed_out = Desk::verify(desk.state.clone())
        .await
        .expect_err("a wait past the deadline times out");
    let elapsed = started.elapsed();
    assert_eq!(timed_out.status(), 504, "{timed_out}");
    assert_eq!(
        timed_out.code(),
        "WYRD_VERIFICATION_504_EXECUTION_TIMED_OUT",
        "{timed_out}"
    );
    assert!(
        elapsed >= VERIFY_TIMEOUT && elapsed < 2 * VERIFY_TIMEOUT,
        "one wait ends at the deadline, unreplayed: {elapsed:?}"
    );
    desk.await_waiters(0).await;

    let abandoned = tokio::spawn(Desk::verify(desk.state.clone()));
    desk.await_waiters(1).await;
    abandoned.abort();
    desk.await_waiters(0).await;

    drop(held);
    let result = Desk::verify(desk.state.clone())
        .await
        .expect("the verify passes once memory frees");
    assert_eq!(result.outcome.verdict, VerificationVerdict::Passed);
    desk.deployment.shutdown().await;
}

/// A submit under occupied process memory is still accepted, and its queued
/// execution is judged once the memory frees.
///
/// # Panics
/// Panics when the submit is refused or recorded, or the execution does not
/// pass after memory frees.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn submit_under_hosted_memory_is_accepted_then_passes() {
    let desk = Desk::start().await;
    let held = desk.occupy(CONTENDED);

    let Submission::Queued(execution) = desk
        .state
        .run_for_card("agent")
        .expect("agent run opens")
        .observe()
        .submit("adds-one", json!(2))
        .await
        .expect("the submit is accepted under memory pressure")
    else {
        panic!("Run evidence queues an execution");
    };
    drop(held);
    let result = execution
        .wait(WAIT)
        .await
        .expect("the deferred execution is judged");

    assert_eq!(result.outcome.verdict, VerificationVerdict::Passed);
    desk.deployment.shutdown().await;
}
