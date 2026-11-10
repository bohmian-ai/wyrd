//! Direct and queued verification hold separate capacity, end to end:
//! `capacity-desk` is verified by one Verifier per hosted admission (an
//! execution slot, a workspace, a Python worker, a model call), so each can
//! be saturated alone. With a queued admission held, a direct SDK `verify`
//! that needs the same resource still completes, while queued work waits for
//! its own lane and progresses once it is released; with a direct admission
//! held, direct work is refused although the queued lane is free.
//!
//! Every lane holds one of each admission and the runtime one queued
//! execution slot. The local upstream answers each Agent by its prompt
//! marker. The server's own log events witness queued waits: a model call
//! waiting for its lane, and an execution deferred by admission backpressure.

use std::collections::BTreeSet;
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use tokio::sync::OwnedSemaphorePermit;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, Request, ResponseTemplate};
use wyrd_ai::bifrost::BifrostOptions;
use wyrd_ai::cards::CardRef;
use wyrd_ai::gateway_admin::{GatewayCaptureMode, GatewayCapturePolicyWrite};
use wyrd_ai::state::WyrdState;
use wyrd_ai::verify::{Execution, Submission, VerificationExecutionStatus};
use wyrd_ai::{Bifrost, Gateway, RequirementOutcome, VerificationResult};
use wyrd_testing::logs::LogCapture;
use wyrd_testing::server::{
    ExecutionMode, HostedLimits, LaneAdmission, LaneLimits, RuntimeLimits, WyrdTestServer,
};

use crate::hosted_agent_tools::{calls, done, messages, text, turn};
use crate::local_development::configure_gateway;
use crate::support::{Deployment, declare, pull};
use crate::verifier_building_blocks::{grants, requirement};

/// The stable code of a refusal for a full direct admission.
const UNAVAILABLE: &str = "WYRD_SERVER_503_SERVICE_UNAVAILABLE";

/// The event a queued model call logs while its lane admits none.
const MODEL_WAIT: &str = "the model call waits for its lane's admission";

/// The event the runner logs when a queued execution meets a full admission.
const DEFERRED: &str = "verification execution deferred by admission backpressure";

/// One admission of every kind per lane, so holding one saturates it.
const ONE: LaneLimits = LaneLimits {
    workspaces: 1,
    python_workers: 1,
    python_waiters: 0,
    model_calls: 1,
};

/// The scripted model: the writer writes one scratch file and then
/// finishes; every other Agent finishes at once.
fn reply(request: &Request) -> ResponseTemplate {
    let messages = messages(request);
    if text(&messages).contains("WRITE:") && turn(&messages) == 0 {
        return calls(&[(
            "w1",
            "fs.write",
            json!({ "path": "/work/answer.md", "content": "answer" }),
        )]);
    }
    done()
}

/// The capacity desk deployment: a server whose lanes each admit one of
/// everything and whose runtime claims one queued execution at a time, with
/// metadata capture of every gateway call and `capacity-desk` registered
/// and hydrated as a principal granted every tool.
struct CapacityDesk {
    /// The deployment.
    deployment: Deployment,
    /// The scripted model upstream; kept alive for the deployment.
    _upstream: MockServer,
    /// The registered `capacity-desk` Service.
    root: CardRef,
    /// The hydrated Service, as a principal granted every tool.
    state: WyrdState,
}

impl CapacityDesk {
    /// Start the scripted upstream and the server, deploy `openai` on the
    /// upstream, capture every gateway call's metadata, and declare, pull,
    /// and load `capacity-desk`.
    ///
    /// # Panics
    /// Panics when a setup step fails.
    async fn start() -> Self {
        let upstream = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(reply)
            .mount(&upstream)
            .await;
        let deployment = Deployment::start_with(
            WyrdTestServer::builder()
                .with_verification_limits_for_test(HostedLimits {
                    monty_min_workers: 0,
                    direct_executions: 1,
                    direct: ONE,
                    queued: ONE,
                    ..HostedLimits::default()
                })
                .with_verification_runtime_limits_for_test(RuntimeLimits {
                    execution_slots: 1,
                    poll_interval: Duration::from_millis(50),
                    ..RuntimeLimits::default()
                })
                .with_verification_runtime_for_test()
                .with_gateway_provider_root_for_test(
                    upstream.uri().parse().expect("upstream URL parses"),
                ),
        )
        .await;
        configure_gateway(&deployment.admin()).await;
        Gateway::connect_with(&deployment.admin())
            .await
            .expect("the gateway handle connects")
            .put_capture_policy(&GatewayCapturePolicyWrite {
                mode: GatewayCaptureMode::Metadata,
                payload_fields: BTreeSet::new(),
            })
            .await
            .expect("the capture policy puts");
        let cards = deployment.cards().await;
        let desk = declare(&cards, "cards/isolated_capacity/capacity-desk.yaml").await;
        let bundle = pull(&cards, &desk.root).await;
        let key = deployment
            .granted_key("every_tool", &grants(json!("all")))
            .await;
        let state =
            WyrdState::from_path_with(bundle.path().join("bundle"), deployment.client(&key))
                .expect("bundle loads offline");
        Self {
            deployment,
            _upstream: upstream,
            root: desk.root,
            state,
        }
    }

    /// Hold every free `admission` of the `mode` lane until the returned
    /// permit drops.
    ///
    /// # Panics
    /// Panics when the server hosts no blocks or the admission is in use.
    fn hold(&self, mode: ExecutionMode, admission: LaneAdmission) -> OwnedSemaphorePermit {
        self.deployment
            .server()
            .state()
            .verification
            .blocks()
            .expect("blocks are hosted")
            .hold_for_test(mode, admission)
    }

    /// Verify `question` with `verifier` now.
    ///
    /// # Errors
    /// Returns the server's refusal.
    async fn verify(
        &self,
        verifier: &str,
        question: &str,
    ) -> Result<VerificationResult, wyrd_ai::WyrdError> {
        self.state
            .run()
            .observe()
            .verify(verifier, json!(question))
            .await
    }

    /// Submit `question` to `verifier` and return its queued execution, read
    /// as the deployment's admin.
    ///
    /// # Panics
    /// Panics when the submission is refused or records instead of queuing.
    async fn submit(&self, verifier: &str, question: &str) -> Execution {
        let Submission::Queued(queued) = self
            .state
            .run()
            .observe()
            .submit(verifier, json!(question))
            .await
            .expect("the submission is accepted")
        else {
            panic!("Run evidence queues an execution");
        };
        Execution::connect_with(*queued.id(), &self.deployment.admin())
            .await
            .expect("the execution handle connects")
    }

    /// Every retained `bifrost.gateway.calls` row's subject Card UID, after
    /// publishing captured rows.
    ///
    /// # Panics
    /// Panics when the flush or the read fails.
    async fn call_subjects(&self) -> Vec<Option<String>> {
        self.deployment
            .server()
            .flush_bifrost()
            .await
            .expect("rows publish");
        let rows: Vec<Value> =
            Bifrost::connect_with(&self.deployment.admin(), BifrostOptions::default())
                .await
                .expect("Bifrost connects")
                .sql_as("SELECT subject_card_uid FROM bifrost.gateway.calls", &[])
                .await
                .expect("calls read back");
        rows.iter()
            .map(|row| row["subject_card_uid"].as_str().map(str::to_owned))
            .collect()
    }
}

/// `result`'s only requirement passed.
///
/// # Panics
/// Panics when the requirement is missing or did not pass.
fn passed(result: &VerificationResult, id: &str) {
    assert_eq!(
        requirement(result, id).outcome,
        RequirementOutcome::Passed,
        "{result:?}"
    );
}

/// Wait until one captured log line carries every `part`.
///
/// # Panics
/// Panics when no such line is logged within a minute.
async fn logged(logs: &LogCapture, parts: &[&str]) {
    let deadline = Instant::now() + Duration::from_mins(1);
    while !logs
        .text()
        .lines()
        .any(|line| parts.iter().all(|part| line.contains(part)))
    {
        assert!(Instant::now() < deadline, "never logged {parts:?}");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

/// Wait for `execution` to settle and return its result.
///
/// # Panics
/// Panics when it does not settle within 90 seconds.
async fn settled(execution: &Execution) -> VerificationResult {
    execution
        .wait(Duration::from_secs(90))
        .await
        .expect("the queued execution settles")
}

/// Each queued admission held at capacity leaves direct verification of the
/// same resource free: with the queued model-call permit held, a queued
/// Agent waits for its lane while holding the only queued execution slot, a
/// direct Agent and a direct assert-only Verifier complete, and a second
/// queued run stays pending; both queued runs complete once the permit is
/// released. With queued Python or the queued workspace held, the direct code
/// or workspace Agent Verifier completes while its queued twin is deferred,
/// then completes on release. Every model call, direct or queued, is
/// recorded on the ordinary gateway path against the verified Service.
/// Holding a direct admission refuses direct work with the stable
/// unavailable code although the queued lane is free, and queued work of
/// that resource still completes.
///
/// # Panics
/// Panics when the process already has a global subscriber, or a run, wait,
/// refusal, or gateway attribution differs.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn direct_verification_runs_while_queued_capacity_is_held() {
    let logs = LogCapture::install_global().expect("the journey owns the process subscriber");
    let desk = CapacityDesk::start().await;

    let model_held = desk.hold(ExecutionMode::Queued, LaneAdmission::ModelCalls);
    let queued_model = desk.submit("model-check", "QUEUED-MODEL").await;
    logged(&logs, &[MODEL_WAIT, "queued"]).await;
    let queued_answer = desk.submit("answer-check", "QUEUED-ANSWER").await;
    let direct_model = desk
        .verify("model-check", "DIRECT-MODEL")
        .await
        .expect("a direct Agent runs while queued model calls are held");
    passed(&direct_model, "answered");
    let direct_answer = desk
        .verify("answer-check", "DIRECT-ANSWER")
        .await
        .expect("a direct Verifier runs while the queued execution slot is held");
    passed(&direct_answer, "answered");
    assert_eq!(
        queued_answer
            .status()
            .await
            .expect("the status reads")
            .status,
        VerificationExecutionStatus::Pending,
        "a queued run waits for the queued execution slot"
    );
    drop(model_held);
    passed(&settled(&queued_model).await, "answered");
    passed(&settled(&queued_answer).await, "answered");

    let python_held = desk.hold(ExecutionMode::Queued, LaneAdmission::Python);
    let direct_python = desk
        .verify("python-check", "DIRECT-PYTHON")
        .await
        .expect("direct code runs while queued Python is held");
    passed(&direct_python, "measured");
    let queued_python = desk.submit("python-check", "QUEUED-PYTHON").await;
    logged(&logs, &[DEFERRED, &queued_python.id().to_string()]).await;
    drop(python_held);
    passed(&settled(&queued_python).await, "measured");

    let workspace_held = desk.hold(ExecutionMode::Queued, LaneAdmission::Workspaces);
    let direct_scratch = desk
        .verify("scratch-check", "DIRECT-SCRATCH")
        .await
        .expect("a direct workspace Agent runs while queued workspaces are held");
    passed(&direct_scratch, "written");
    let queued_scratch = desk.submit("scratch-check", "QUEUED-SCRATCH").await;
    logged(&logs, &[DEFERRED, &queued_scratch.id().to_string()]).await;
    drop(workspace_held);
    passed(&settled(&queued_scratch).await, "written");

    let subjects = desk.call_subjects().await;
    let root = desk
        .root
        .uid
        .as_ref()
        .expect("the declared root carries its UID")
        .to_string();
    assert_eq!(
        subjects.len(),
        6,
        "two model and two two-turn writer runs: {subjects:?}"
    );
    assert!(
        subjects.iter().all(|subject| subject.as_deref() == Some(&root)),
        "every lane-admitted call is attributed to the verified Service: {subjects:?}"
    );

    for (admission, verifier, question, id) in [
        (LaneAdmission::Workspaces, "scratch-check", "FULL-SCRATCH", "written"),
        (LaneAdmission::Python, "python-check", "FULL-PYTHON", "measured"),
    ] {
        let held = desk.hold(ExecutionMode::Direct, admission);
        let refused = desk
            .verify(verifier, question)
            .await
            .expect_err("direct work never borrows queued capacity");
        assert_eq!(refused.code(), UNAVAILABLE, "{admission:?}");
        let queued = desk.submit(verifier, question).await;
        passed(&settled(&queued).await, id);
        drop(held);
    }
    let held = desk
        .deployment
        .server()
        .state()
        .verification
        .blocks()
        .expect("blocks are hosted")
        .hold_direct_executions_for_test();
    let refused = desk
        .verify("answer-check", "FULL-DIRECT")
        .await
        .expect_err("a full direct execution ceiling refuses");
    assert_eq!(refused.code(), UNAVAILABLE);
    drop(held);
    desk.deployment.shutdown().await;
}
