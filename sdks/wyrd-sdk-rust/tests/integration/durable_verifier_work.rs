//! Queued Verifier work is acknowledged only once it is recoverable: a
//! `202` from `enqueue` means the execution is durably admitted, so a
//! response lost after it is recovered by replaying the same key, even on a
//! restarted server, and work that never reached the queue is never
//! acknowledged.

use serde_json::json;
use wiremock::matchers::{body_string_contains, method};
use wiremock::{Mock, ResponseTemplate};
use wyrd_ai::VerificationVerdict;
use wyrd_ai::verify::Execution;

use wyrd_testing::verification::VerificationFixture;

use crate::support::{declare, declared};
use crate::verify_in_real_time::{Assistant, enqueue_as, enqueue_response};

/// How long a test waits for a queued execution to settle.
const WAIT: std::time::Duration = std::time::Duration::from_secs(90);

/// An acknowledged submission survives losing its response and a restart:
/// the replay under its key names the same execution, which settles once.
/// While its input record is held before Scribe acknowledges it, a
/// submission is not acknowledged, and replaying it once released admits it
/// once. A different request under
/// an admitted key is refused at once.
///
/// # Panics
/// Panics when an acknowledgement precedes admission, a replay names
/// another execution, or a conflict is not refused.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn queued_acceptance_survives_response_loss() {
    let assistant = Assistant::start().await;
    let service = assistant
        .deployment
        .service_client(&assistant.service)
        .await;
    let (verifier, subject) = (
        assistant.answer_check.uid.clone(),
        assistant.agent.uid.clone(),
    );
    let body = |answer: &str| {
        json!({
            "verifier_card_uid": verifier,
            "card_uid": subject,
            "inputs": {"answer": {"kind": "json", "value": answer}},
        })
    };
    let accepted = enqueue_as(&service, &assistant.deployment, "durable-0001", &body("no")).await;

    let assistant = assistant.restart().await;
    let service = assistant
        .deployment
        .service_client(&assistant.service)
        .await;
    let replayed = enqueue_as(&service, &assistant.deployment, "durable-0001", &body("no")).await;
    assert_eq!(
        replayed, accepted,
        "the replay names the admitted execution"
    );
    let result = Execution::connect_with(accepted, &service)
        .await
        .expect("the execution handle connects")
        .wait(WAIT)
        .await
        .expect("the admitted execution settles");
    assert_eq!(result.outcome.verdict, VerificationVerdict::Failed);
    let (status, conflict) = enqueue_response(
        &service,
        &assistant.deployment,
        "durable-0001",
        &body("yes"),
    )
    .await
    .expect("the server answers");
    assert_eq!(status, 409, "{conflict}");
    assert_eq!(conflict["code"], "WYRD_REGISTRY_409_IDEMPOTENCY_CONFLICT");

    let held = assistant
        .deployment
        .server()
        .stall_next_bifrost_write_into("bifrost.verification.execution_inputs")
        .expect("the server runs Scribe");
    let unacknowledged =
        enqueue_response(&service, &assistant.deployment, "durable-0002", &body("no")).await;
    assert!(
        unacknowledged.is_err(),
        "a submission whose input is not yet durable is not acknowledged: {unacknowledged:?}"
    );
    held.release();
    let admitted = enqueue_as(&service, &assistant.deployment, "durable-0002", &body("no")).await;
    Execution::connect_with(admitted, &service)
        .await
        .expect("the execution handle connects")
        .wait(WAIT)
        .await
        .expect("the replayed submission settles");
    assistant.deployment.shutdown().await;
}

/// Every judge call so far whose body contains `marker`.
///
/// # Panics
/// Panics when the judge stops recording requests.
async fn calls(assistant: &Assistant, marker: &str) -> usize {
    assistant
        .judge
        .received_requests()
        .await
        .expect("the judge records requests")
        .iter()
        .filter(|request| String::from_utf8_lossy(&request.body).contains(marker))
        .count()
}

/// A restart after a step action committed but before its requirement
/// settled restores the action without calling its Agent again and judges
/// the requirement once more, and a restart after the final commit returns
/// the committed result without calling either again.
///
/// The `reviewed-verifier` review step's Agent answers at once, and the
/// first judgment of its requirement is held at the judge while the server
/// restarts without releasing its lease, as a crash does; the lease is then
/// expired on the database clock so the new server reclaims the execution.
///
/// # Panics
/// Panics when a committed action runs again, the result differs, or a
/// settled execution calls the judge again.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lane"]
async fn action_checkpoint_survives_unfinished_requirements() {
    const REVIEW: &str = "Review the answer";
    const JUDGE: &str = "Claim: The review agrees";
    let assistant = Assistant::start().await;
    let reviewed = declare(
        &assistant.deployment.cards().await,
        "cards/verify_in_real_time/reviewed.yaml",
    )
    .await;
    Mock::given(method("POST"))
        .and(body_string_contains(JUDGE))
        .respond_with(ResponseTemplate::new(200).set_delay(std::time::Duration::from_secs(600)))
        .up_to_n_times(1)
        .with_priority(1)
        .mount(&assistant.judge)
        .await;
    let body = json!({
        "verifier_card_uid": declared(&reviewed, "reviewed-verifier").uid,
        "card_uid": assistant.agent.uid,
        "inputs": {"answer": {"kind": "json", "value": "yes"}},
    });
    let service = assistant.deployment.service_client(&reviewed.root).await;
    let execution = enqueue_as(&service, &assistant.deployment, "reviewed-0001", &body).await;
    let deadline = std::time::Instant::now() + WAIT;
    while calls(&assistant, JUDGE).await == 0 {
        assert!(
            std::time::Instant::now() < deadline,
            "the requirement was never judged"
        );
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    assert_eq!(calls(&assistant, REVIEW).await, 1, "the action committed");
    // The harness keeps Bifrost's local data root only across a clean
    // shutdown, so the acknowledged input record is published first.
    assistant
        .deployment
        .server()
        .flush_bifrost()
        .await
        .expect("Bifrost flushes");

    let assistant = assistant.restart().await;
    let server = assistant.deployment.server();
    VerificationFixture::provision(server.state(), server.data_tenant_id())
        .await
        .expect("the tenant provisions")
        .expire_deadlines(execution)
        .await
        .expect("the crashed process's lease expires");
    let service = assistant.deployment.service_client(&reviewed.root).await;
    let result = Execution::connect_with(execution, &service)
        .await
        .expect("the execution handle connects")
        .wait(WAIT)
        .await
        .expect("the recovered execution settles");
    assert_eq!(result.outcome.verdict, VerificationVerdict::Passed);
    let review = &result.outcome.steps[0];
    assert_eq!((review.id.as_str(), review.attempts), ("review", 1));
    let judged = (
        calls(&assistant, REVIEW).await,
        calls(&assistant, JUDGE).await,
    );
    assert_eq!(judged, (1, 2), "only the unsettled requirement ran again");
    assistant
        .deployment
        .server()
        .flush_bifrost()
        .await
        .expect("Bifrost flushes");

    let assistant = assistant.restart().await;
    let service = assistant.deployment.service_client(&reviewed.root).await;
    let committed = Execution::connect_with(execution, &service)
        .await
        .expect("the execution handle connects")
        .wait(WAIT)
        .await
        .expect("the committed execution reads back");
    assert_eq!(committed.outcome, result.outcome);
    assert_eq!(
        (
            calls(&assistant, REVIEW).await,
            calls(&assistant, JUDGE).await
        ),
        judged,
        "a settled execution calls nothing again"
    );
    assistant.deployment.shutdown().await;
}
