//! The finance release Rust program, run as a reader runs it.
//!
//! Each journey runs the built `finance_release` binary in a directory that
//! holds only the downloaded policy PDF, against a real server. A local
//! model stands in for the policy judge: it reads the policy extraction at
//! `/inputs/company-sharing-policy.pdf.json` with `fs.read`, is inconclusive for a report naming
//! `PROVIDER-UNCERTAIN`, fails one naming `BANK-ACCOUNT-DEMO-001`, and passes
//! only an `external audit` purpose, or a `regulator request` when the
//! policy it read permits regulators, citing the policy line it read.
//! Without that line in the read, it answers no verdict.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

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
use wyrd_ai::verify::Execution;
use wyrd_ai::{RequirementOutcome, VerificationVerdict};
use wyrd_testing::server::WyrdTestServer;

/// The policy line the judge must read before it judges.
const POLICY_LINE: &str = "external-audit";

/// The finance example directory, which holds both policy PDFs.
const EXAMPLE_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/finance-release");

/// What the program prints under the original policy, as the docs show it.
const FIRST_RUN: &str = include_str!("../finance-release/output.txt");

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
            "function": { "name": "fs_read", "arguments": r#"{"path":"/inputs/company-sharing-policy.pdf.json"}"# },
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
    let permits_regulators = line["text"]
        .as_str()
        .is_some_and(|text| text.contains("regulator"));
    let verdict = if release.contains("PROVIDER-UNCERTAIN") {
        json!({ "outcome": "inconclusive", "reason": "the policy does not settle this release", "citations": [] })
    } else if release.contains("BANK-ACCOUNT-DEMO-001") {
        json!({ "outcome": "failed", "reason": "the report names a bank account identifier, which the policy forbids", "citations": citations })
    } else if release.contains("external audit")
        || (permits_regulators && release.contains("regulator request"))
    {
        json!({ "outcome": "passed", "reason": "the policy permits sharing for this purpose", "citations": citations })
    } else {
        json!({ "outcome": "failed", "reason": "the policy does not permit sharing for this purpose", "citations": citations })
    };
    completion(json!({ "role": "assistant", "content": verdict.to_string() }))
}

/// A proposed external-audit release: a synthetic report with `text`, to an
/// auditor whose record says `authorized`, at a destination whose record
/// says `restricted`.
fn release(text: &str, authorized: bool, restricted: bool) -> Value {
    json!({
        "purpose": "external audit",
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
    /// forwards to it, and a reader directory holding the downloaded policy.
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
        download_policy(dir.path(), "company-sharing-policy.pdf");
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

    /// The state for `finance-release` `0.1.0`, the Service the program
    /// first declared, pulled as a deployed application would load it.
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

/// Copy the example's `file` into the reader's `dir` as
/// `company-sharing-policy.pdf`, as the Quickstart's download does.
///
/// # Panics
/// Panics when the file cannot be copied.
fn download_policy(dir: &Path, file: &str) {
    std::fs::copy(
        Path::new(EXAMPLE_DIR).join(file),
        dir.join("company-sharing-policy.pdf"),
    )
    .expect("the policy downloads");
}

/// The policy file `check` mounts for its judge: the Artifact reference and
/// the path inside it.
fn mounted_policy(check: &Value) -> Value {
    check["workspace"]["/inputs/company-sharing-policy.pdf.json"]["artifact"].clone()
}

/// The policy file `release-check` `version` mounts for its judge.
///
/// # Panics
/// Panics when the Verifier cannot be read.
async fn declared_policy(cards: &Cards, version: &str) -> Value {
    let verifier = spec(cards, "Verifier", "release-check", version).await;
    mounted_policy(&verifier["requirements"][1]["check"])
}

/// The program releases the external-audit report and holds the one naming
/// a bank account and the ones shared for purposes the policy does not
/// name, printing each failed requirement's reason; a release breaking
/// either assertion fails on its own; the judge reads the policy extraction
/// and the release's purpose through the gateway before every verdict;
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
    assert_eq!(first, FIRST_RUN);
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
    assert_eq!(verdicts.len(), 10, "one policy judgment per release");
    assert!(
        verdicts.iter().all(|body| body.contains(POLICY_LINE)
            && body.contains("/inputs/company-sharing-policy.pdf.json")),
        "each verdict follows a read of the policy extraction"
    );
    assert!(
        verdicts
            .iter()
            .any(|body| body.contains("marketing partnership")),
        "the judge sees each release's purpose"
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
    assert_eq!(
        declared_policy(&cards, "0.1.0").await["path"],
        "extracted/company-sharing-policy.pdf.json",
        "the judge reads the PDF's stored extraction: {verifier}"
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

/// Every release in one run judges the one uploaded policy. A revised
/// policy PDF declares new exact policy, Verifier, and Service versions and
/// releases the regulator request that the original held, while an
/// execution under the original Verifier still names the original policy
/// file in its details.
///
/// # Panics
/// Panics when the server cannot start, the program fails, or a version,
/// decision, or policy reference differs.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn finance_release_policy_versions_remain_attributable() {
    let story = Story::start().await;
    assert_eq!(story.run().await, FIRST_RUN);
    let verdicts = story.verdict_requests().await;
    assert_eq!(verdicts.len(), 4, "one judgment per release");
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
    assert!(
        verdicts
            .iter()
            .all(|body| source(body) == source(&verdicts[0])),
        "every release reads the one uploaded policy"
    );

    // An execution under the original Verifier, kept for inspection.
    let state = story.state().await;
    let original = state
        .run()
        .observe()
        .verify(
            "release-check",
            release(
                "Synthetic customer report: quarterly total $120.",
                true,
                true,
            ),
        )
        .await
        .expect("the release verifies");

    // The reader downloads the revised policy over the original.
    download_policy(story.dir.path(), "company-sharing-policy-revised.pdf");
    let revised = FIRST_RUN.replace(
        "regulator request · Synthetic customer report: quarterly total $120. → failed\n  \
         follows-policy: the policy does not permit sharing for this purpose\n",
        "regulator request · Synthetic customer report: quarterly total $120. → passed\n",
    );
    assert_eq!(
        story.run().await,
        revised,
        "the revised policy releases the regulator request"
    );

    let cards = story.cards().await;
    let old_policy = declared_policy(&cards, "0.1.0").await;
    assert_eq!(old_policy["card"]["version"], "0.1.0", "{old_policy}");
    assert_eq!(
        declared_policy(&cards, "0.1.1").await["card"]["version"],
        "0.1.1"
    );
    for (service, verifier) in [("0.1.0", "0.1.0"), ("0.1.1", "0.1.1")] {
        let bound = spec(&cards, "Service", "finance-release", service).await;
        assert_eq!(
            bound["verified_by"][0]["verifier"]["version"], verifier,
            "{bound}"
        );
    }

    // The original execution's details still name the original policy file.
    let execution = Execution::connect_with(original.execution_id, &story.client())
        .await
        .expect("the execution connects");
    let mut details = execution.details().await;
    for _ in 0..60 {
        if details.is_ok() {
            break;
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
        details = execution.details().await;
    }
    let details =
        serde_json::to_value(details.expect("the details read")).expect("details serialize");
    assert_eq!(details["verifier"]["version"], "0.1.0", "{details}");
    assert_eq!(
        mounted_policy(&details["outcome"]["requirements"][1]["check"]),
        old_policy,
        "{details}"
    );
    story
        .server
        .shutdown()
        .await
        .expect("test server shuts down");
}
