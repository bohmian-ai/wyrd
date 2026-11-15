//! Direct and queued verification share hosted capacity charged only while
//! in use, end to end: `capacity-desk` is verified by one Verifier per
//! hosted resource (an execution slot, workspace scratch, Python memory, a
//! model call), so each can be saturated alone. Execution slots stay
//! separate per mode: with the queued one held, direct verification still
//! completes. Model calls, Python memory, and workspace scratch are the
//! process's own, reserved for neither mode: an idle server holds no
//! scratch; with model calls held, a call of either mode waits; and with
//! Python memory or scratch held, direct work is refused while queued work
//! is deferred and completes once it is released.
//!
//! The process admits one model call and the runtime one queued execution
//! slot. The local upstream answers each Agent by its prompt marker. The
//! server's own log events witness waits: a model call waiting for
//! admission, and an execution deferred by admission backpressure.

use std::collections::BTreeSet;
use std::path::Path;
use std::time::{Duration, Instant};

use secrecy::ExposeSecret;
use serde_json::{Value, json};
use tokio::sync::OwnedSemaphorePermit;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, Request, ResponseTemplate};
use wyrd_client::bifrost::BifrostOptions;
use wyrd_client::cards::{BundleMode, CardRef, CardSelector, Cards};
use wyrd_client::gateway_admin::{
    DeploymentAccess, GatewayCaptureMode, GatewayCapturePolicyWrite, GatewayOperation,
    ProviderAdapter, ProviderAuth, ProviderCredentialName, ProviderCredentialWrite,
    ProviderCredentialWriteSource, ProviderDeployment, ProviderDeploymentName, ProviderId,
};
use wyrd_client::gateway_credential::CredentialWriter;
use wyrd_client::operator_connections::SecretBearer;
use wyrd_client::state::WyrdState;
use wyrd_client::verify::{Execution, Submission, VerificationExecutionStatus};
use wyrd_client::{Bifrost, Gateway, WyrdClient};
use wyrd_spec::error::WyrdError;
use wyrd_spec::verification::execution::{RequirementOutcome, VerificationResult};
use wyrd_testing::logs::LogCapture;
use wyrd_testing::server::{HostedLimits, RuntimeLimits};
use wyrd_testing::{Bootstrap, WyrdTestServer};

use super::native_client;

/// The stable code of a refusal for a full direct admission.
const UNAVAILABLE: &str = "WYRD_SERVER_503_SERVICE_UNAVAILABLE";

/// The event a model call logs while no model-call permit is free.
const MODEL_WAIT: &str = "the model call waits for its lane's admission";

/// The stable code of a direct verification past its deadline.
const TIMED_OUT: &str = "WYRD_VERIFICATION_504_EXECUTION_TIMED_OUT";

/// The event the runner logs when a queued execution meets a full admission.
const DEFERRED: &str = "verification execution deferred by admission backpressure";

/// A Chat Completions answer carrying the assistant `message`.
fn completion(message: Value) -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_json(json!({
        "id": "chatcmpl-capacity",
        "object": "chat.completion",
        "created": 1,
        "model": "gpt-4o",
        "choices": [{ "index": 0, "finish_reason": "stop", "message": message }],
        "usage": { "prompt_tokens": 1, "completion_tokens": 1, "total_tokens": 2 },
    }))
}

/// The scripted model: the writer writes one scratch file and then
/// finishes; every other Agent finishes at once.
fn reply(request: &Request) -> ResponseTemplate {
    let body: Value = serde_json::from_slice(&request.body).unwrap_or_default();
    let messages = body["messages"].as_array().cloned().unwrap_or_default();
    let writes = messages
        .iter()
        .filter_map(|message| message["content"].as_str())
        .any(|content| content.contains("WRITE:"));
    let answered = messages
        .iter()
        .any(|message| message["role"] == "assistant");
    if writes && !answered {
        let arguments = json!({ "path": "/work/answer.md", "content": "answer" });
        return completion(json!({
            "role": "assistant",
            "content": null,
            "tool_calls": [{
                "id": "w1",
                "type": "function",
                "function": { "name": "fs_write", "arguments": arguments.to_string() },
            }],
        }));
    }
    completion(json!({ "role": "assistant", "content": r#"{"done":true}"# }))
}

/// The plaintext API key of a bootstrapped machine principal.
///
/// # Panics
/// Panics when `bootstrap` is a user principal.
fn api_key(bootstrap: Bootstrap) -> String {
    match bootstrap {
        Bootstrap::Machine { api_key, .. } => api_key.expose_secret().to_owned(),
        Bootstrap::User { .. } => panic!("expected a machine principal"),
    }
}

/// The capacity desk deployment: a server whose modes each admit one model
/// call and whose runtime claims one queued execution at a time, with
/// metadata capture of every gateway call and `capacity-desk` registered
/// and hydrated as a principal granted every tool.
struct CapacityDesk {
    /// The bound server.
    server: WyrdTestServer,
    /// The administrator's client.
    admin: WyrdClient,
    /// The scripted model upstream; kept alive for the server.
    _upstream: MockServer,
    /// The hydrated bundle's directory; kept alive for `state`.
    _bundle: tempfile::TempDir,
    /// The registered `capacity-desk` Service.
    root: CardRef,
    /// The hydrated Service, as a principal granted every tool.
    state: WyrdState,
}

impl CapacityDesk {
    /// Start the scripted upstream and the server, deploy `openai` on the
    /// upstream through the public gateway administration, capture every
    /// gateway call's metadata, and declare, pull, and load `capacity-desk`.
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
        let server = Box::pin(
            WyrdTestServer::builder()
                .with_verification_limits_for_test(HostedLimits {
                    monty_min_workers: 0,
                    direct_executions: 1,
                    model_calls: 1,
                    verify_timeout: Duration::from_secs(10),
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
                )
                .start_bound(),
        )
        .await
        .expect("the server starts");
        let admin = native_client(
            &server,
            &api_key(
                server
                    .bootstrap_service("capacity_admin", &["admin"])
                    .await
                    .expect("the administrator bootstraps"),
            ),
        )
        .expect("the admin client builds");
        CredentialWriter::new(admin.clone())
            .put_credential(&ProviderCredentialWrite {
                name: ProviderCredentialName::new("openai-key").expect("credential name"),
                provider: ProviderId::new("openai").expect("provider"),
                source: ProviderCredentialWriteSource::ManagedSecret {
                    secret: SecretBearer::new("sk-local-upstream".to_owned()),
                },
            })
            .await
            .expect("the credential puts");
        let gateway = Gateway::connect_with(&admin)
            .await
            .expect("the gateway handle connects");
        gateway
            .put_deployment(&ProviderDeployment {
                name: ProviderDeploymentName::new("openai").expect("deployment name"),
                adapter: ProviderAdapter::OpenAi,
                auth: ProviderAuth::Bearer {
                    credential: ProviderCredentialName::new("openai-key").expect("credential"),
                },
                models: None,
                capabilities: Some(BTreeSet::from([GatewayOperation::ChatCompletions])),
                access: DeploymentAccess::Open,
            })
            .await
            .expect("the deployment puts");
        gateway
            .put_capture_policy(&GatewayCapturePolicyWrite {
                mode: GatewayCaptureMode::Metadata,
                payload_fields: BTreeSet::new(),
            })
            .await
            .expect("the capture policy puts");
        let cards = Cards::connect_with(&admin)
            .await
            .expect("the Cards handle connects");
        let root = Box::pin(cards.declare_from_path(Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../fixtures/cards/isolated_capacity/capacity-desk.yaml"
        ))))
        .await
        .expect("capacity-desk declares")
        .root;
        let bundle = tempfile::tempdir().expect("the bundle directory creates");
        Box::pin(cards.pull(
            CardSelector::exact(root.clone()),
            bundle.path().join("bundle"),
            BundleMode::Complete,
        ))
        .await
        .expect("the graph pulls");
        let grants = [
            json!({ "resource": "verifier", "action": "run", "scope": "all" }),
            json!({ "resource": "gateway", "action": "invoke", "scope": "all" }),
            json!({ "resource": "tool", "action": "invoke", "scope": "all" }),
            json!({ "resource": "cards", "action": "read", "scope": "all" }),
            json!({ "resource": "bifrost_query", "action": "read", "scope": "all" }),
        ];
        server
            .seed_role("every_tool", &grants)
            .await
            .expect("the role seeds");
        let key = api_key(
            server
                .bootstrap_service("every_tool", &["every_tool"])
                .await
                .expect("the principal bootstraps"),
        );
        let state = WyrdState::from_path_with(
            bundle.path().join("bundle"),
            native_client(&server, &key).expect("the client builds"),
        )
        .expect("the bundle loads offline");
        Self {
            server,
            admin,
            _upstream: upstream,
            _bundle: bundle,
            root,
            state,
        }
    }

    /// The server's hosted blocks.
    ///
    /// # Panics
    /// Panics when the server hosts no blocks.
    fn blocks(&self) -> &wyrd_server::verification::hosted::blocks::HostedBlocks {
        self.server
            .state()
            .verification
            .blocks()
            .expect("blocks are hosted")
    }

    /// Hold every free model call until the returned permit drops.
    ///
    /// # Panics
    /// Panics when the server hosts no blocks or a permit is in use.
    fn hold_model_calls(&self) -> OwnedSemaphorePermit {
        self.blocks().hold_model_calls_for_test()
    }

    /// Hold all free process memory, so every Python operation is refused,
    /// until the returned hold drops.
    ///
    /// # Panics
    /// Panics when the server hosts no blocks.
    fn hold_python(&self) -> impl Sized {
        self.blocks().sandbox().occupy_for_test(0)
    }

    /// Hold all free process scratch, so every workspace is refused, until
    /// the returned charge drops.
    ///
    /// # Panics
    /// Panics when the server hosts no blocks.
    fn hold_scratch(&self) -> impl Sized {
        let scratch = self.blocks().resources().scratch().clone();
        scratch.charge_infallible(scratch.available())
    }

    /// Verify `question` with `verifier` now.
    ///
    /// # Errors
    /// Returns the server's refusal.
    async fn verify(
        &self,
        verifier: &str,
        question: &str,
    ) -> Result<VerificationResult, WyrdError> {
        self.state
            .run()
            .observe()
            .verify(verifier, json!(question))
            .await
    }

    /// Submit `question` to `verifier` and return its queued execution, read
    /// as the administrator.
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
        Execution::connect_with(*queued.id(), &self.admin)
            .await
            .expect("the execution handle connects")
    }

    /// Every retained `bifrost.gateway.calls` row's subject Card UID, after
    /// publishing captured rows.
    ///
    /// # Panics
    /// Panics when the flush or the read fails.
    async fn call_subjects(&self) -> Vec<Option<String>> {
        self.server.flush_bifrost().await.expect("rows publish");
        let rows: Vec<Value> = Bifrost::connect_with(&self.admin, BifrostOptions::default())
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

/// `result`'s requirement `id` passed.
///
/// # Panics
/// Panics when the requirement is missing or did not pass.
fn passed(result: &VerificationResult, id: &str) {
    let requirement = result
        .outcome
        .requirements
        .iter()
        .find(|requirement| requirement.id.as_str() == id)
        .unwrap_or_else(|| panic!("requirement {id}: {result:#?}"));
    assert_eq!(
        requirement.outcome,
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

/// A held queued execution slot leaves direct verification free: with the
/// model-call permit held, a queued Agent waits for admission while holding
/// the only queued execution slot, a direct assert-only Verifier completes,
/// and a second queued run stays pending; both queued runs complete once the
/// permit is released, and a direct Agent then runs. Python memory and scratch are reserved for neither
/// mode: an idle server holds no scratch, and with either held at capacity a
/// queued code or workspace run is deferred and completes on release, while
/// direct work of that resource is refused with the stable unavailable code.
/// Every model call is recorded on the ordinary gateway path against the
/// verified Service. A direct model call waits for the shared permit like a
/// queued one, and a wait past the direct deadline is cancelled without
/// leaking a permit.
///
/// # Panics
/// Panics when the process already has a global subscriber, or a run, wait,
/// refusal, or gateway attribution differs.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the serialized Postgres-backed journey lane"]
async fn direct_verification_runs_while_queued_capacity_is_held() {
    let logs = LogCapture::install_global().expect("the journey owns the process subscriber");
    let desk = CapacityDesk::start().await;

    let model_held = desk.hold_model_calls();
    let queued_model = desk.submit("model-check", "QUEUED-MODEL").await;
    logged(&logs, &[MODEL_WAIT, "queued"]).await;
    let queued_answer = desk.submit("answer-check", "QUEUED-ANSWER").await;
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
    let direct_model = desk
        .verify("model-check", "DIRECT-MODEL")
        .await
        .expect("a direct Agent runs once the model call is free");
    passed(&direct_model, "answered");

    assert_eq!(
        desk.blocks().resources().scratch().used(),
        0,
        "an idle server reserves no scratch for either mode"
    );
    let held = desk.hold_python();
    let queued_python = desk.submit("python-check", "QUEUED-PYTHON").await;
    logged(&logs, &[DEFERRED, &queued_python.id().to_string()]).await;
    drop(held);
    passed(&settled(&queued_python).await, "measured");

    let held = desk.hold_scratch();
    let queued_scratch = desk.submit("scratch-check", "QUEUED-SCRATCH").await;
    logged(&logs, &[DEFERRED, &queued_scratch.id().to_string()]).await;
    drop(held);
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
        4,
        "two model runs and one two-turn writer run: {subjects:?}"
    );
    assert!(
        subjects
            .iter()
            .all(|subject| subject.as_deref() == Some(&root)),
        "every admitted call is attributed to the verified Service: {subjects:?}"
    );

    let held = desk.hold_scratch();
    let refused = desk
        .verify("scratch-check", "FULL-SCRATCH")
        .await
        .expect_err("direct work needs free process scratch");
    assert_eq!(refused.code(), UNAVAILABLE, "scratch");
    drop(held);
    let held = desk.hold_python();
    let refused = desk
        .verify("python-check", "FULL-PYTHON")
        .await
        .expect_err("direct work needs free process memory");
    assert_eq!(refused.code(), UNAVAILABLE, "python");
    drop(held);
    let held = desk.hold_model_calls();
    let (waited, ()) = tokio::join!(desk.verify("model-check", "FULL-MODEL"), async {
        logged(&logs, &[MODEL_WAIT, "direct"]).await;
        drop(held);
    });
    passed(
        &waited.expect("the direct call proceeds once the permit frees"),
        "answered",
    );
    let held = desk.hold_model_calls();
    let refused = desk
        .verify("model-check", "STUCK-MODEL")
        .await
        .expect_err("a direct call waiting past the deadline is cancelled");
    assert_eq!(refused.code(), TIMED_OUT);
    drop(held);
    passed(
        &desk
            .verify("model-check", "AFTER-STUCK")
            .await
            .expect("the cancelled wait leaked no permit"),
        "answered",
    );

    let held = desk
        .server
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
    desk.server.shutdown().await.expect("the server stops");
}
