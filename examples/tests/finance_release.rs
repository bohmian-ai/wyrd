//! The finance release Rust program, run as a reader runs it.
//!
//! Each journey runs the built `finance_release` binary in an empty
//! directory against a real server. A local model stands in for the policy
//! judge: it reads `/inputs/policy.md` with `fs.read`, then fails a report naming
//! `BANK-ACCOUNT-DEMO-001`, is inconclusive for one naming
//! `PROVIDER-UNCERTAIN`, and otherwise passes, citing the policy line it
//! read. Without that line in the read, it answers no verdict.

use std::path::PathBuf;
use std::process::Command;

use secrecy::{ExposeSecret, SecretString};
use serde_json::{Value, json};
use tempfile::TempDir;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, Request, ResponseTemplate};
use wyrd_ai::WyrdClient;
use wyrd_ai::cards::{BundleMode, CardRef, CardSelector, Cards};
use wyrd_ai::config::ClientConfig;
use wyrd_ai::state::WyrdState;
use wyrd_ai::transport::{GrpcConfig, HttpConfig};
use wyrd_ai::{RequirementOutcome, VerificationVerdict};
use wyrd_testing::server::WyrdTestServer;

/// The policy line the judge must read before it judges.
const POLICY_LINE: &str = "external-audit";

/// One Chat Completions reply carrying `message`.
fn completion(message: Value) -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_json(json!({
        "id": "chatcmpl-judge",
        "object": "chat.completion",
        "created": 1,
        "model": "gpt-4o",
        "choices": [{ "index": 0, "finish_reason": "stop", "message": message }],
        "usage": { "prompt_tokens": 1, "completion_tokens": 1, "total_tokens": 2 },
    }))
}

/// The policy judge's reply to `request`: a `fs.read` of the mounted policy
/// first, then a verdict on the rendered report that cites the policy line
/// it read.
fn judge_policy(request: &Request) -> ResponseTemplate {
    let body: Value = serde_json::from_slice(&request.body).unwrap_or_default();
    let messages = body["messages"].as_array().cloned().unwrap_or_default();
    let release = messages
        .iter()
        .find(|message| message["role"] == "user")
        .map(|message| message["content"].to_string())
        .unwrap_or_default();
    let Some(read) = messages.iter().find(|message| message["role"] == "tool") else {
        let call = json!({
            "id": "call-policy",
            "type": "function",
            "function": { "name": "fs_read", "arguments": r#"{"path":"/inputs/policy.md"}"# },
        });
        return completion(json!({ "role": "assistant", "content": null, "tool_calls": [call] }));
    };
    let output: Value = read["content"]
        .as_str()
        .and_then(|content| serde_json::from_str(content).ok())
        .unwrap_or_default();
    let Some(line) = output["units"].as_array().and_then(|units| {
        units.iter().find(|unit| {
            unit["text"]
                .as_str()
                .is_some_and(|text| text.contains(POLICY_LINE))
        })
    }) else {
        return completion(json!({ "role": "assistant", "content": "the policy did not read" }));
    };
    let citations = json!([
        { "slot": "report", "locator": { "json_pointer": "/text" } },
        { "slot": output["sources"][0], "locator": line["locator"] },
    ]);
    let verdict = if release.contains("PROVIDER-UNCERTAIN") {
        json!({ "outcome": "inconclusive", "reason": "the policy does not settle this release", "citations": [] })
    } else if release.contains("BANK-ACCOUNT-DEMO-001") {
        json!({ "outcome": "failed", "reason": "the report names a bank account identifier", "citations": citations })
    } else {
        json!({ "outcome": "passed", "reason": "an external audit may receive this report", "citations": citations })
    };
    completion(json!({ "role": "assistant", "content": verdict.to_string() }))
}

/// A proposed release: a synthetic report with `text`, to an auditor whose
/// record says `authorized`, at a destination whose record says
/// `restricted`.
fn release(text: &str, authorized: bool, restricted: bool) -> Value {
    json!({
        "report": { "text": text },
        "recipient": { "email": "auditor@example.test", "authorized": authorized },
        "destination": { "url": "https://auditor.example.test/reports", "restricted": restricted },
    })
}

/// A running server, its scripted policy judge, and a reader's empty
/// directory.
struct Story {
    /// The verifying server the reader's program reaches.
    server: WyrdTestServer,
    /// The scripted model provider behind the gateway; it records every
    /// request.
    model: MockServer,
    /// The reader's directory.
    dir: TempDir,
    /// An empty Wyrd config directory, so no saved developer setup applies.
    config: TempDir,
    /// The server's HTTP address.
    server_url: String,
    /// The server's gRPC address.
    grpc_url: String,
    /// The tenant administrator's key, which `wyrd init` would save.
    key: String,
}

impl Story {
    /// Start the scripted judge and a server whose `openai` deployment
    /// forwards to it, and an empty reader directory.
    ///
    /// # Panics
    /// Panics when the server cannot start or the judge model cannot deploy.
    async fn start() -> Self {
        let model = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(judge_policy)
            .mount(&model)
            .await;
        let server = WyrdTestServer::builder()
            .with_verification_runtime_for_test()
            .with_gateway_provider_root_for_test(model.uri().parse().expect("model URL parses"))
            .start_bound()
            .await
            .expect("test server starts");
        server
            .deploy_gateway_model_for_test("gpt-4o", "policy_judge_invoker")
            .await
            .expect("the judge model deploys");
        let dir = tempfile::tempdir().expect("reader directory creates");
        let server_url = server
            .base_url()
            .expect("bound server has a URL")
            .to_owned();
        let grpc_url = server.grpc_url().expect("bound server has a gRPC URL");
        let key = server
            .tenant_admin_key()
            .await
            .expect("administrator key issues")
            .expose_secret()
            .to_owned();
        Self {
            server,
            model,
            dir,
            config: tempfile::tempdir().expect("config directory creates"),
            server_url,
            grpc_url,
            key,
        }
    }

    /// Run the program in the reader's directory and return what it printed.
    ///
    /// # Panics
    /// Panics when the binary cannot start or exits unsuccessfully.
    async fn run(&self) -> String {
        let command = self.command();
        // The in-process server keeps serving while the binary runs.
        let output = tokio::task::spawn_blocking(move || {
            Command::new(env!("CARGO_BIN_EXE_finance_release"))
                .current_dir(&command.0)
                .env("WYRD_CONFIG_HOME", &command.1)
                .env("WYRD_SERVER_URL", &command.2)
                .env("WYRD_GRPC_URL", &command.3)
                .env("WYRD_API_KEY", &command.4)
                .env_remove("WYRD_ACCESS_TOKEN")
                .env_remove("WYRD_WORKLOAD_TOKEN")
                .env_remove("WYRD_TENANT")
                .output()
                .expect("the finance_release binary starts")
        })
        .await
        .expect("the program task joins");
        assert!(
            output.status.success(),
            "the program fails: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout).expect("the program prints UTF-8")
    }

    /// The state for the `finance-release` Service the program declared,
    /// pulled as a deployed application would load it.
    ///
    /// # Panics
    /// Panics when the Service cannot be pulled or loaded.
    async fn state(&self) -> WyrdState {
        let bundle = self.dir.path().join("bundle");
        let service: CardRef = serde_json::from_value(
            json!({ "kind": "Service", "name": "finance-release", "version": "0.1.0" }),
        )
        .expect("card reference parses");
        self.cards()
            .await
            .pull(CardSelector::exact(service), &bundle, BundleMode::Complete)
            .await
            .expect("the Service pulls");
        WyrdState::from_path_with(&bundle, self.client()).expect("the bundle loads")
    }

    /// The reader's directory, config directory, server URL, gRPC URL, and
    /// key, owned for the blocking program task.
    fn command(&self) -> (PathBuf, PathBuf, String, String, String) {
        (
            self.dir.path().to_owned(),
            self.config.path().to_owned(),
            self.server_url.clone(),
            self.grpc_url.clone(),
            self.key.clone(),
        )
    }

    /// The tenant administrator's Card registry, for public inspection.
    ///
    /// # Panics
    /// Panics when the client cannot build or connect.
    async fn cards(&self) -> Cards {
        Cards::connect_with(&self.client())
            .await
            .expect("Cards connects")
    }

    /// The tenant administrator's client.
    ///
    /// # Panics
    /// Panics when the client cannot build.
    fn client(&self) -> WyrdClient {
        WyrdClient::with_config(ClientConfig {
            http: HttpConfig {
                base_url: self.server_url.clone(),
                ..HttpConfig::default()
            },
            grpc: GrpcConfig {
                endpoint: self.grpc_url.clone(),
                ..GrpcConfig::default()
            },
            credential: Some(SecretString::from(self.key.clone())),
            ..ClientConfig::default()
        })
        .expect("client builds")
    }

    /// The policy judge's verdict requests: each carries the `fs.read`
    /// result it judged.
    ///
    /// # Panics
    /// Panics when the model is not recording requests.
    async fn verdict_requests(&self) -> Vec<String> {
        self.model
            .received_requests()
            .await
            .expect("the model records requests")
            .iter()
            .map(|request| String::from_utf8_lossy(&request.body).into_owned())
            .filter(|body| body.contains(r#""role":"tool""#))
            .collect()
    }
}

/// The registered spec of `kind` `name` at `version`, as JSON.
///
/// # Panics
/// Panics when the Card cannot be read.
async fn spec(cards: &Cards, kind: &str, name: &str, version: &str) -> Value {
    let card_ref: CardRef =
        serde_json::from_value(json!({ "kind": kind, "name": name, "version": version }))
            .expect("card reference parses");
    let card = cards
        .get(CardSelector::exact(card_ref))
        .await
        .expect("the Card reads");
    serde_json::to_value(&card.spec).expect("spec serializes")
}

/// The policy version `release-check` `version` mounts for its judge.
///
/// # Panics
/// Panics when the Verifier cannot be read.
async fn mounted_policy(cards: &Cards, version: &str) -> Value {
    let verifier = spec(cards, "Verifier", "release-check", version).await;
    verifier["requirements"][1]["check"]["workspace"]["/inputs/policy.md"]["artifact"]["card"]
        ["version"]
        .clone()
}

/// The program passes the permitted report and fails the one naming a bank
/// account, and a release breaking either assertion fails on its own; the judge reads
/// the saved policy through the gateway before every verdict;
/// one Service binds one Verifier with exactly the three requirements; and
/// an unchanged rerun prints the same exact versions.
///
/// # Panics
/// Panics when the server cannot start, the program fails, or it prints
/// anything else.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn finance_release_passes_and_fails() {
    let story = Story::start().await;

    let first = story.run().await;
    assert_eq!(
        first,
        "passed: Synthetic customer report: quarterly total $120.\n\
         failed: Synthetic customer report: paid from BANK-ACCOUNT-DEMO-001.\n",
    );
    assert_eq!(story.run().await, first, "the rerun declares nothing new");

    // Each assertion is judged on its own.
    let state = story.state().await;
    let run = state.run();
    for evidence in [
        release(
            "Synthetic customer report: quarterly total $120.",
            false,
            true,
        ),
        release(
            "Synthetic customer report: quarterly total $120.",
            true,
            false,
        ),
    ] {
        let result = run
            .observe()
            .verify("release-check", evidence)
            .await
            .expect("the release verifies");
        assert_eq!(result.outcome.verdict, VerificationVerdict::Failed);
    }

    let verdicts = story.verdict_requests().await;
    assert_eq!(verdicts.len(), 6, "one policy judgment per release");
    assert!(
        verdicts.iter().all(|body| body.contains(POLICY_LINE)),
        "each verdict follows a read of the policy"
    );

    let cards = story.cards().await;
    let service = spec(&cards, "Service", "finance-release", "0.1.0").await;
    let bound = service["verified_by"]
        .as_array()
        .expect("verified_by lists");
    assert_eq!(bound.len(), 1, "{service}");
    let verifier = spec(&cards, "Verifier", "release-check", "0.1.0").await;
    let requirements: Vec<&str> = verifier["requirements"]
        .as_array()
        .expect("requirements list")
        .iter()
        .filter_map(|requirement| requirement["id"].as_str())
        .collect();
    assert_eq!(
        requirements,
        [
            "recipient-authorized",
            "follows-policy",
            "destination-restricted"
        ]
    );
    assert_eq!(
        verifier["requirements"][1]["check"]["agent"]["kind"], "Agent",
        "the policy requirement is an Agent check: {verifier}"
    );
    story
        .server
        .shutdown()
        .await
        .expect("test server shuts down");
}

/// An uncertain policy judgment returns `inconclusive`, and so does a release
/// without a destination record: its `destination` slot is missing evidence,
/// so `destination-restricted` cannot be judged. Neither is released.
///
/// # Panics
/// Panics when the server cannot start, the program fails, or either verdict
/// differs.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn finance_release_holds_uncertain_releases() {
    let story = Story::start().await;
    story.run().await;
    let state = story.state().await;
    let run = state.run();

    let uncertain = run
        .observe()
        .verify(
            "release-check",
            release("Synthetic report flagged PROVIDER-UNCERTAIN.", true, true),
        )
        .await
        .expect("the release verifies");
    assert_eq!(uncertain.outcome.verdict, VerificationVerdict::Inconclusive);

    let mut missing = release("Synthetic customer report.", true, true);
    missing
        .as_object_mut()
        .expect("a release is an object")
        .remove("destination");
    let incomplete = run
        .observe()
        .verify("release-check", missing)
        .await
        .expect("the incomplete release verifies");
    assert!(incomplete.is_inconclusive(), "{:?}", incomplete.outcome);
    let destination = incomplete
        .outcome
        .requirements
        .iter()
        .find(|requirement| requirement.id.as_str() == "destination-restricted")
        .expect("the destination requirement is judged");
    assert_eq!(destination.outcome, RequirementOutcome::Inconclusive);
    story
        .server
        .shutdown()
        .await
        .expect("test server shuts down");
}

/// Both releases in one run judge the one uploaded policy; an edited policy
/// file declares new exact policy, Verifier, and Service versions, while the
/// old Verifier still mounts the old policy.
///
/// # Panics
/// Panics when the server cannot start, the program fails, or a version
/// differs.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn finance_release_policy_versions_remain_attributable() {
    let story = Story::start().await;
    let first = story.run().await;
    let verdicts = story.verdict_requests().await;
    assert_eq!(verdicts.len(), 2, "one judgment per release");
    let source = |body: &str| -> Value {
        let request: Value = serde_json::from_str(body).expect("request parses");
        let messages = request["messages"].as_array().cloned().unwrap_or_default();
        let read = messages
            .iter()
            .find(|message| message["role"] == "tool")
            .expect("the verdict request carries the read");
        let output: Value = serde_json::from_str(read["content"].as_str().unwrap_or_default())
            .expect("the read parses");
        output["sources"][0].clone()
    };
    assert_eq!(
        source(&verdicts[0]),
        source(&verdicts[1]),
        "both releases read the one uploaded policy"
    );

    // The program saved the policy on its first run; the reader edits it.
    let policy = story.dir.path().join("company-sharing-policy.md");
    let mut revised = std::fs::read_to_string(&policy).expect("the saved policy reads");
    revised.push_str("Revision 2.\n");
    std::fs::write(&policy, revised).expect("revised policy writes");
    let changed = story.run().await;
    assert_eq!(changed, first, "the new policy judges the same releases");

    let cards = story.cards().await;
    assert_eq!(mounted_policy(&cards, "0.1.0").await, "0.1.0");
    assert_eq!(mounted_policy(&cards, "0.1.1").await, "0.1.1");
    for (service, verifier) in [("0.1.0", "0.1.0"), ("0.1.1", "0.1.1")] {
        let bound = spec(&cards, "Service", "finance-release", service).await;
        assert_eq!(
            bound["verified_by"][0]["verifier"]["version"], verifier,
            "{bound}"
        );
    }
    story
        .server
        .shutdown()
        .await
        .expect("test server shuts down");
}
