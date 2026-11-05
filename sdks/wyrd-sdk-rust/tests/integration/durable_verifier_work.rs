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

/// The `code` of a refused call.
///
/// # Panics
/// Panics when the call succeeded.
fn refused<T: std::fmt::Debug>(result: Result<T, wyrd_ai::WyrdError>) -> String {
    result.expect_err("the call is refused").code().to_owned()
}

/// A queued execution's invocations and publications read back as stable,
/// authorized pages, each publication by its exact producer attempt, and
/// cancelling it is durable and repeatable.
///
/// With no runtime, the fixture holds the execution's lease, begins two
/// steps, and accepts a JSON value, JSON `null`, a file, and a directory.
/// Pages of one entry follow their cursors in insertion order to the end;
/// a cursor is refused on another listing, and a limit outside 1–200 is
/// refused. Each publication reads back with its value or manifest, a file
/// and a listed directory file plan their downloads with the manifest's size
/// and digest, and a JSON or unlisted download, a path on a read, and
/// another attempt are refused. A caller with `cards:read` but no `verifier:read`, or
/// with it for another Verifier only, is refused; one scoped to this Verifier reads.
/// Another tenant sees no such execution. A viewer cannot cancel; the
/// administrator's cancel and its repeat both answer `cancelled`, and the
/// publications stay listed.
///
/// # Panics
/// Panics when any read, refusal, or cancellation differs.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lane"]
async fn execution_pages_are_stable_and_tenant_scoped() {
    use wyrd_ai::verify::{
        ExecutionPageQuery, InvocationProgress, PublicationContent, PublicationKind,
        PublicationName, PublicationSelector, TaskId, VerificationExecutionStatus,
    };
    use wyrd_testing::verification::Published;

    let deployment = crate::support::Deployment::start().await;
    let server = deployment.server();
    let fixture = VerificationFixture::provision(server.state(), server.data_tenant_id())
        .await
        .expect("the tenant provisions");
    let (subject, _) = fixture.service("pages-subject").await.expect("service");
    let verifier = fixture.drift_verifier("pages").await.expect("verifier");
    let other = fixture
        .drift_verifier("pages-other")
        .await
        .expect("verifier");
    fixture
        .submit(&verifier, &subject, std::collections::BTreeMap::new())
        .await
        .expect("the execution is admitted");
    let tree = vec![
        ("a.md".to_owned(), b"alpha".to_vec()),
        ("nested/b.md".to_owned(), b"beta".to_vec()),
    ];
    let id = fixture
        .publish(
            server.state(),
            &[
                ("review", "verdict", Published::Json(json!({"ok": true}))),
                (
                    "review",
                    "nothing",
                    Published::Json(serde_json::Value::Null),
                ),
                ("draft", "report", Published::File(b"# Report\n".to_vec())),
                ("draft", "tree", Published::Directory(tree)),
            ],
        )
        .await
        .expect("the held execution publishes");
    let admin = deployment.admin();
    let execution = Execution::connect_with(id, &admin)
        .await
        .expect("the execution handle connects");

    let page = |cursor: Option<String>| ExecutionPageQuery {
        limit: Some(1),
        cursor,
    };
    let mut invocations = Vec::new();
    let mut cursor = None;
    loop {
        let read = execution
            .invocations(&page(cursor))
            .await
            .expect("an invocation page reads");
        assert_eq!(read.execution_id, id);
        assert_eq!(read.status, VerificationExecutionStatus::Running);
        invocations.extend(read.items);
        cursor = read.next_cursor;
        if cursor.is_none() {
            break;
        }
    }
    let steps: Vec<_> = invocations
        .iter()
        .map(|item| {
            (
                item.producer.step_id.as_str(),
                item.producer.attempt,
                item.progress,
            )
        })
        .collect();
    assert_eq!(
        steps,
        [
            ("review", 1, InvocationProgress::Started),
            ("draft", 1, InvocationProgress::Started),
        ]
    );

    let mut publications = Vec::new();
    let mut cursor = None;
    loop {
        let read = execution
            .publications(&page(cursor))
            .await
            .expect("a publication page reads");
        publications.extend(read.items);
        cursor = read.next_cursor;
        if cursor.is_none() {
            break;
        }
    }
    let listed: Vec<_> = publications
        .iter()
        .map(|item| {
            (
                item.producer.step_id.as_str(),
                item.name.as_str(),
                item.kind,
            )
        })
        .collect();
    assert_eq!(
        listed,
        [
            ("review", "verdict", PublicationKind::Json),
            ("review", "nothing", PublicationKind::Json),
            ("draft", "report", PublicationKind::File),
            ("draft", "tree", PublicationKind::Directory),
        ]
    );
    let first = execution
        .invocations(&page(None))
        .await
        .expect("an invocation page reads");
    let foreign_cursor = ExecutionPageQuery {
        limit: Some(1),
        cursor: first.next_cursor,
    };
    assert_eq!(
        refused(execution.publications(&foreign_cursor).await),
        "WYRD_SPEC_400_VALIDATION",
        "an invocation cursor does not continue the publication listing"
    );
    for limit in [0, 201] {
        let query = ExecutionPageQuery {
            limit: Some(limit),
            cursor: None,
        };
        assert_eq!(
            refused(execution.invocations(&query).await),
            "WYRD_SPEC_400_VALIDATION"
        );
    }

    let step = |name: &str| TaskId::new(name).expect("step");
    let name = |name: &str| PublicationName::new(name).expect("name");
    let attempt = |path: Option<&str>| PublicationSelector {
        attempt: 1,
        item_index: None,
        iteration_index: None,
        path: path.map(str::to_owned),
    };
    let content = |publication: wyrd_ai::verify::Publication| publication.content;
    assert_eq!(
        content(
            execution
                .publication(&step("review"), &name("verdict"), &attempt(None))
                .await
                .expect("the JSON publication reads")
        ),
        PublicationContent::Json {
            value: json!({"ok": true})
        }
    );
    assert_eq!(
        content(
            execution
                .publication(&step("review"), &name("nothing"), &attempt(None))
                .await
                .expect("the null publication reads")
        ),
        PublicationContent::Json {
            value: serde_json::Value::Null
        }
    );
    let PublicationContent::File { manifest: file } = content(
        execution
            .publication(&step("draft"), &name("report"), &attempt(None))
            .await
            .expect("the file publication reads"),
    ) else {
        panic!("the report is a file publication");
    };
    let PublicationContent::Directory {
        manifest: directory,
    } = content(
        execution
            .publication(&step("draft"), &name("tree"), &attempt(None))
            .await
            .expect("the directory publication reads"),
    )
    else {
        panic!("the tree is a directory publication");
    };
    let paths: Vec<_> = directory
        .files
        .iter()
        .map(|file| file.path.as_str())
        .collect();
    assert_eq!(paths, ["a.md", "nested/b.md"]);

    let digest = |file: &wyrd_ai::verify::PublicationFile| {
        use base64::Engine as _;
        base64::engine::general_purpose::STANDARD.encode(file.sha256.as_bytes())
    };
    let plan = execution
        .download(&step("draft"), &name("report"), &attempt(None))
        .await
        .expect("the file download plans");
    assert_eq!(
        (plan.size_bytes, plan.sha256),
        (file.files[0].size_bytes, digest(&file.files[0]))
    );
    let plan = execution
        .download(&step("draft"), &name("tree"), &attempt(Some("nested/b.md")))
        .await
        .expect("the directory file download plans");
    assert_eq!(
        (plan.size_bytes, plan.sha256),
        (directory.files[1].size_bytes, digest(&directory.files[1]))
    );
    for (publication, selector) in [
        ("verdict", attempt(None)),
        ("tree", attempt(None)),
        ("tree", attempt(Some("missing.md"))),
        ("report", attempt(Some("a.md"))),
    ] {
        let step = if publication == "verdict" {
            "review"
        } else {
            "draft"
        };
        assert_eq!(
            refused(
                execution
                    .download(
                        &TaskId::new(step).expect("step"),
                        &name(publication),
                        &selector
                    )
                    .await
            ),
            "WYRD_SPEC_400_VALIDATION",
            "{publication} {selector:?}"
        );
    }
    assert_eq!(
        refused(
            execution
                .publication(&step("review"), &name("verdict"), &attempt(Some("a.md")))
                .await
        ),
        "WYRD_SPEC_400_VALIDATION"
    );
    let second = PublicationSelector {
        attempt: 2,
        ..attempt(None)
    };
    assert_eq!(
        refused(
            execution
                .publication(&step("review"), &name("verdict"), &second)
                .await
        ),
        "WYRD_VERIFICATION_404_PUBLICATION_NOT_FOUND"
    );

    let grant = |uid: &dyn std::fmt::Display| json!({"resource": "verifier", "action": "read", "scope": {"verifier": uid.to_string()}});
    let session = &deployment;
    let as_key = |key: String| async move {
        Execution::connect_with(id, &session.client(&key))
            .await
            .expect("the execution handle connects")
    };
    let cards_only = json!({"resource": "cards", "action": "read", "scope": "all"});
    let nobody = as_key(deployment.granted_key("pages_cards", &[cards_only]).await).await;
    assert_eq!(
        refused(nobody.invocations(&ExecutionPageQuery::default()).await),
        "WYRD_PERMISSION_403_DENIED_RBAC"
    );
    let elsewhere = as_key(
        deployment
            .granted_key("pages_elsewhere", &[grant(&other)])
            .await,
    )
    .await;
    assert_eq!(
        refused(elsewhere.publications(&ExecutionPageQuery::default()).await),
        "WYRD_PERMISSION_403_DENIED_RBAC"
    );
    let exact = as_key(
        deployment
            .granted_key("pages_exact", &[grant(&verifier)])
            .await,
    )
    .await;
    assert_eq!(
        exact
            .publications(&ExecutionPageQuery::default())
            .await
            .expect("an exact verifier:read grant reads")
            .items,
        publications
    );
    let foreign = as_key(deployment.other_tenant_admin("pages-foreign").await).await;
    assert_eq!(
        refused(foreign.invocations(&ExecutionPageQuery::default()).await),
        "WYRD_VERIFICATION_404_EXECUTION_NOT_FOUND"
    );
    let viewer = as_key(deployment.key("pages_viewer", &["viewer"]).await).await;
    viewer
        .invocations(&ExecutionPageQuery::default())
        .await
        .expect("a viewer reads");
    assert_eq!(
        refused(viewer.cancel().await),
        "WYRD_PERMISSION_403_DENIED_RBAC"
    );

    for _ in 0..2 {
        let cancelled = execution.cancel().await.expect("the cancel commits");
        assert_eq!(
            (cancelled.execution_id, cancelled.status),
            (id, VerificationExecutionStatus::Cancelled)
        );
    }
    assert_eq!(
        execution
            .publications(&ExecutionPageQuery::default())
            .await
            .expect("a cancelled execution's publications read")
            .items,
        publications
    );
    deployment.shutdown().await;
}
