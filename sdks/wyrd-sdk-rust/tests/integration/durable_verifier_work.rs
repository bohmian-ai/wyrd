//! Queued Verifier work is acknowledged only once it is recoverable: a
//! `202` from `enqueue` means the execution is durably admitted, so a
//! response lost after it is recovered by replaying the same key, even on a
//! restarted server, and work that never reached the queue is never
//! acknowledged.

use serde_json::json;
use wyrd_ai::VerificationVerdict;
use wyrd_ai::verify::Execution;

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
