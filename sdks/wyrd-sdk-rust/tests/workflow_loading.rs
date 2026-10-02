//! Rust SDK Workflow loading journey through the public `wyrd_sdk` crate.
//!
//! A team registers shared reviewer Agents and Prompts; an authored Workflow
//! file mixes external refs to them with a local Agent. `Workflow::from_path`
//! reads those refs through the ambient client configuration, so each ambient
//! load runs in a child process of this test binary whose environment carries
//! the server URL and credential. The parent applies the authored file, loads
//! the registered Workflow through `cards.workflow()`, proves it stays pinned
//! after a newer Agent version registers, and covers missing, denied,
//! inactive, and mismatched refs and wrong selectors, none of which dispatch.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use secrecy::ExposeSecret;
use serde_json::{Map, Value, json};
use skald_providers::ProviderError;
use skald_spec::ProviderResponse;
use skald_spec::wire::openai_chat::OpenAiChatResponse;
use skald_workflow::{
    WorkflowExecutionDependencies, WorkflowRunOptions, WorkflowRunStatus, WyrdGatewayCall,
    WyrdGatewayCaller,
};
use tokio_util::sync::CancellationToken;
use wyrd_sdk::Workflow;
use wyrd_sdk::bifrost::client_from_options;
use wyrd_sdk::cards::{CardKind, CardRef, CardSelector, Cards};
use wyrd_testing::Bootstrap;
use wyrd_testing::server::WyrdTestServer;

/// Environment variable naming the Workflow file a child probe loads.
const PROBE_ENV: &str = "WYRD_WORKFLOW_PROBE";

/// Stdout prefix of the single JSON line a child probe reports.
const PROBE_PREFIX: &str = "WORKFLOW-PROBE-RESULT ";

/// Deterministic Wyrd gateway answering each reviewer by its system role.
#[derive(Default)]
struct ReviewGateway {
    /// Serialized provider requests in arrival order.
    requests: Mutex<Vec<String>>,
}

#[async_trait]
impl WyrdGatewayCaller for ReviewGateway {
    /// Record the request and answer with the fixed review for its reviewer.
    ///
    /// # Errors
    /// Never returns an error; every request receives its fixed review.
    ///
    /// # Panics
    /// Panics if the static completion fixture stops decoding, a test-fixture
    /// invariant.
    async fn call(
        &self,
        call: WyrdGatewayCall,
        _cancellation: &CancellationToken,
    ) -> Result<ProviderResponse, ProviderError> {
        let request = serde_json::to_string(&call.request).unwrap_or_default();
        let text = if request.contains("security reviewer") {
            "SECURITY-FINDINGS"
        } else if request.contains("correctness reviewer") {
            "CORRECTNESS-FINDINGS"
        } else {
            "FINAL-REVIEW"
        };
        self.requests
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(request);
        let response: OpenAiChatResponse = serde_json::from_value(json!({
            "id": "resp",
            "object": "chat.completion",
            "created": 0,
            "model": "gpt-5-5",
            "choices": [{
                "index": 0,
                "message": { "role": "assistant", "content": text },
                "finish_reason": "stop"
            }]
        }))
        .expect("static completion decodes");
        Ok(ProviderResponse::OpenAiChatCompletion(response))
    }
}

impl ReviewGateway {
    /// Snapshot the recorded requests.
    fn requests(&self) -> Vec<String> {
        self.requests
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    /// Run `workflow` against this gateway with the bundle's `code` input.
    ///
    /// # Panics
    /// Panics when the run refuses to start.
    async fn run(self: &Arc<Self>, workflow: &Workflow) -> skald_workflow::WorkflowRun {
        let dependencies =
            WorkflowExecutionDependencies::new(skald_runtime::ProviderRegistry::new())
                .with_wyrd_gateway(Arc::clone(self) as Arc<dyn WyrdGatewayCaller>);
        let input = Map::from_iter([(
            "code".to_owned(),
            Value::from("diff --git a/src/auth.rs b/src/auth.rs"),
        )]);
        workflow
            .as_skald()
            .run_with_options(&dependencies, input, WorkflowRunOptions::default())
            .await
            .expect("workflow run starts")
    }
}

/// The checked-in code-review bundle directory.
fn bundle() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/workflows/code-review")
}

/// Read one bundle file.
///
/// # Panics
/// Panics when the file cannot be read.
fn bundle_file(file: &str) -> String {
    std::fs::read_to_string(bundle().join(file)).expect("bundle file reads")
}

/// Write `body` to `root/file`, creating parent directories.
///
/// # Panics
/// Panics when the directory or file cannot be written.
fn write(root: &Path, file: &str, body: &str) -> PathBuf {
    let path = root.join(file);
    std::fs::create_dir_all(path.parent().expect("fixture file has a parent"))
        .expect("fixture directory creates");
    std::fs::write(&path, body).expect("fixture file writes");
    path
}

/// Write a standalone team Agent from the bundle reviewer `file`, with its
/// Prompt beside it, applying `edit` to both Cards; returns the Agent path.
fn team_agent(root: &Path, file: &str, edit: impl Fn(String) -> String) -> PathBuf {
    write(
        root,
        &format!("prompts/{file}.yaml"),
        &edit(bundle_file(&format!("prompts/{file}.yaml"))),
    );
    write(
        root,
        &format!("{file}.yaml"),
        &edit(bundle_file(&format!("agents/{file}.yaml")).replacen("../prompts/", "prompts/", 1)),
    )
}

/// Replace the path target of `file`'s step with an external ref to `name`
/// at 1.0.0, adding `extra` lines (such as a UID pin) under the ref.
fn external_target(workflow: &str, file: &str, name: &str, extra: &str) -> String {
    workflow.replacen(
        &format!("        target: ./agents/{file}.yaml"),
        &format!(
            "        target:\n          kind: Agent\n          name: {name}\n          version: \"1.0.0\"{extra}"
        ),
        1,
    )
}

/// Write an authored Workflow whose security and correctness steps reference
/// the team Agents `security` and `correctness` externally while the final
/// reviewer stays a local path; returns the Workflow path.
fn mixed_workflow(root: &Path, security: &str, correctness: &str, pin: &str) -> PathBuf {
    for dir in ["agents", "prompts"] {
        let file = format!("{dir}/final-reviewer.yaml");
        write(root, &file, &bundle_file(&file));
    }
    let workflow = external_target(&bundle_file("workflow.yaml"), "security", security, pin);
    let workflow = external_target(&workflow, "correctness", correctness, "");
    write(root, "workflow.yaml", &workflow)
}

/// Write the full bundle with the local security Prompt rewritten as an
/// auditor, plus a step referencing the registered `security-reviewer@1.0.0`
/// that shares the local sibling's identity; returns the Workflow path.
fn shadowed_workflow(root: &Path) -> PathBuf {
    for dir in ["agents", "prompts"] {
        for file in ["security", "correctness", "final-reviewer"] {
            let file = format!("{dir}/{file}.yaml");
            let body = bundle_file(&file).replacen(
                "You are a security reviewer.",
                "You are a local auditor.",
                1,
            );
            write(root, &file, &body);
        }
    }
    let workflow = bundle_file("workflow.yaml").replacen(
        "    - id: correctness",
        "    - id: registered_security\n      action:\n        type: agent\n        target:\n          kind: Agent\n          name: security-reviewer\n          version: \"1.0.0\"\n      inputs:\n        code: input.code\n\n    - id: correctness",
        1,
    );
    write(root, "workflow.yaml", &workflow)
}

/// Build an exact reference in the bundle's `engineering` space.
///
/// # Panics
/// Panics when the fields do not decode as a Card reference.
fn card_ref(kind: &str, name: &str, version: &str, uid: Option<&str>) -> CardRef {
    serde_json::from_value(json!({
        "kind": kind,
        "name": name,
        "version": version,
        "space": "engineering",
        "uid": uid,
    }))
    .expect("test card reference decodes")
}

/// Unwrap a machine bootstrap into its raw API key.
///
/// # Panics
/// Panics when the bootstrap is a user principal.
fn api_key(bootstrap: Bootstrap) -> String {
    match bootstrap {
        Bootstrap::Machine { api_key, .. } => api_key.expose_secret().to_owned(),
        Bootstrap::User { .. } => panic!("expected a machine principal"),
    }
}

/// One journey's server address, credentials, fixtures, and registered team.
struct Journey {
    /// Fixture root owning every authored file.
    root: tempfile::TempDir,
    /// Bound test server URL every client and child targets.
    base_url: String,
    /// Writer-scoped Cards handle registering team Cards and the Workflow.
    team: Cards,
    /// API key of a principal holding only the reader role.
    reader: String,
    /// API key of a principal holding no role.
    denied: String,
    /// Registered team Card UIDs by Card name.
    team_uids: Map<String, Value>,
}

impl Journey {
    /// Bootstrap writer, reader, and role-less principals, register the
    /// security and correctness team Agents with their Prompts, and register
    /// then soft-delete a `retired-reviewer` Agent.
    ///
    /// # Panics
    /// Panics when a principal bootstraps or a Card registers unsuccessfully.
    async fn start(server: &WyrdTestServer) -> Self {
        let base_url = server
            .base_url()
            .expect("bound server has a URL")
            .to_owned();
        let mut keys = Vec::new();
        for (name, roles) in [
            ("workflow_team_writer", &["writer"][..]),
            ("workflow_team_reader", &["reader"][..]),
            ("workflow_no_roles", &[][..]),
        ] {
            let bootstrap = server
                .bootstrap_service(name, roles)
                .await
                .expect("principal bootstraps");
            keys.push(api_key(bootstrap));
        }
        let [writer, reader, denied] = <[String; 3]>::try_from(keys).expect("three principals");
        let mut journey = Self {
            root: tempfile::tempdir().expect("fixture root creates"),
            team: Cards::with_client(
                client_from_options(Some(&base_url), Some(&writer), None).expect("client builds"),
            ),
            base_url,
            reader,
            denied,
            team_uids: Map::new(),
        };
        for file in ["security", "correctness"] {
            let agent = team_agent(&journey.root.path().join("team"), file, |card| card);
            journey.register(&agent).await;
        }
        let retired = team_agent(
            &journey.root.path().join("retired-team"),
            "security",
            |card| {
                card.replacen("name: security-reviewer", "name: retired-reviewer", 1)
                    .replacen("name: security-review-prompt", "name: retired-prompt", 1)
            },
        );
        journey.register(&retired).await;
        journey
            .team
            .delete(CardSelector::exact(card_ref(
                "Agent",
                "retired-reviewer",
                "1.0.0",
                None,
            )))
            .await
            .expect("retired Agent deletes");
        journey
    }

    /// Register `path` as the team writer and record every outcome UID.
    ///
    /// # Panics
    /// Panics when registration fails or an outcome carries no UID.
    async fn register(&mut self, path: &Path) -> CardRef {
        let receipt = Box::pin(self.team.register_from_path(path))
            .await
            .expect("team Cards register");
        for outcome in receipt.outcomes {
            self.team_uids.insert(
                outcome.card_ref.name.to_string(),
                json!(outcome.card_ref.uid.expect("registered Card has a UID")),
            );
        }
        receipt.root
    }

    /// Return the registered UID of the team Card `name` as text.
    ///
    /// # Panics
    /// Panics when no Card named `name` registered.
    fn uid(&self, name: &str) -> &str {
        self.team_uids[name].as_str().expect("UID is text")
    }

    /// Return a Cards handle authenticated as `credential`.
    ///
    /// # Panics
    /// Panics when the client cannot build.
    fn cards(&self, credential: &str) -> Cards {
        Cards::with_client(
            client_from_options(Some(&self.base_url), Some(credential), None)
                .expect("client builds"),
        )
    }

    /// Return a fixture directory named `name` under the journey root.
    fn dir(&self, name: &str) -> PathBuf {
        self.root.path().join(name)
    }

    /// Load `workflow` in a child of this test binary whose ambient
    /// environment names the server and, when given, `credential`, then
    /// return its report.
    ///
    /// # Panics
    /// Panics when the child cannot run, fails, or reports no result.
    fn probe(&self, workflow: &Path, credential: Option<&str>) -> Value {
        let config_home = tempfile::tempdir().expect("config home creates");
        let mut command = Command::new(std::env::current_exe().expect("test binary resolves"));
        command
            .args([
                "--exact",
                "ambient_workflow_probe",
                "--ignored",
                "--nocapture",
            ])
            .env(PROBE_ENV, workflow)
            .env("WYRD_SERVER_URL", &self.base_url)
            .env("WYRD_CONFIG_HOME", config_home.path())
            .env_remove("WYRD_API_KEY")
            .env_remove("WYRD_ACCESS_TOKEN");
        if let Some(credential) = credential {
            command.env("WYRD_API_KEY", credential);
        }
        let output = command.output().expect("probe child runs");
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            output.status.success(),
            "probe child failed: {stdout}\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let line = stdout
            .lines()
            .find_map(|line| line.strip_prefix(PROBE_PREFIX))
            .unwrap_or_else(|| panic!("probe child reported no result: {stdout}"));
        serde_json::from_str(line).expect("probe result decodes")
    }

    /// Assert an ambient load of `workflow` as `credential` refuses with
    /// `code` before dispatching anything.
    ///
    /// # Panics
    /// Panics when the load succeeds or refuses with another code.
    fn refuses(&self, workflow: &Path, credential: Option<&str>, code: &str) {
        let report = self.probe(workflow, credential);
        assert_eq!(
            report["code"],
            json!(code),
            "{}: {report}",
            workflow.display()
        );
        assert!(
            report.get("requests").is_none(),
            "a refused load dispatches nothing"
        );
    }

    /// Prove authored loading: a mixed file resolves team refs through the
    /// ambient credential and runs; a sibling and a same-identity external
    /// ref run their own bodies; missing, denied, retired, unknown, and
    /// UID-pinned refs refuse. Returns the mixed Workflow path.
    ///
    /// # Panics
    /// Panics when any authored load diverges from the contract.
    fn authored_loads(&self) -> PathBuf {
        let mixed = mixed_workflow(
            &self.dir("mixed"),
            "security-reviewer",
            "correctness-reviewer",
            "",
        );
        self.refuses(&mixed, None, "WYRD_CLIENT_401_NO_CREDENTIALS");
        self.refuses(
            &mixed,
            Some(&self.denied),
            "WYRD_PERMISSION_403_DENIED_RBAC",
        );
        let loaded = self.probe(&mixed, Some(&self.reader));
        assert_eq!(loaded["status"], json!("Succeeded"), "{loaded}");
        assert_eq!(loaded["outputs"]["review"], json!("FINAL-REVIEW"));
        assert_eq!(loaded["requests"].as_array().map(Vec::len), Some(3));

        let shadow = self.probe(
            &shadowed_workflow(&self.dir("shadowed")),
            Some(&self.reader),
        );
        let requests = shadow["requests"]
            .as_array()
            .expect("shadowed run reports requests");
        assert_eq!(requests.len(), 4, "{shadow}");
        let sent = |text: &str| {
            requests.iter().any(|request| {
                request
                    .as_str()
                    .is_some_and(|request| request.contains(text))
            })
        };
        assert!(
            sent("You are a local auditor."),
            "the sibling runs its local body"
        );
        assert!(
            sent("You are a security reviewer."),
            "the external ref runs the registered body"
        );

        let retired = mixed_workflow(
            &self.dir("retired"),
            "retired-reviewer",
            "correctness-reviewer",
            "",
        );
        self.refuses(
            &retired,
            Some(&self.reader),
            "WYRD_REGISTRY_404_CARD_NOT_FOUND",
        );
        let unknown = mixed_workflow(
            &self.dir("unknown"),
            "security-reviewer",
            "correctness-reviewer",
            "",
        );
        let body = std::fs::read_to_string(&unknown)
            .expect("unknown fixture reads")
            .replacen(
                "name: security-reviewer\n          version: \"1.0.0\"",
                "name: security-reviewer\n          version: \"9.9.9\"",
                1,
            );
        let unknown = write(&self.dir("unknown"), "workflow.yaml", &body);
        self.refuses(
            &unknown,
            Some(&self.reader),
            "WYRD_REGISTRY_404_CARD_NOT_FOUND",
        );
        let pinned = mixed_workflow(
            &self.dir("pinned"),
            "security-reviewer",
            "correctness-reviewer",
            &format!("\n          uid: {}", self.uid("correctness-reviewer")),
        );
        self.refuses(
            &pinned,
            Some(&self.reader),
            "WYRD_REGISTRY_400_INVALID_CARD_SPEC",
        );
        mixed
    }

    /// Prove registered loading: apply `mixed`, check its stored step UIDs,
    /// register a newer security Agent, load the Workflow by exact ref and by
    /// UID as the reader and run it pinned, then refuse wrong, versionless,
    /// mismatched, and unauthorized selectors without dispatch.
    ///
    /// # Panics
    /// Panics when any registered load diverges from the contract.
    async fn registered_loads(&mut self, mixed: &Path) {
        let workflow_ref = self.register(mixed).await;
        let workflow_uid = workflow_ref
            .uid
            .clone()
            .expect("applied Workflow has a UID");
        let stored = self
            .team
            .get(CardSelector::exact(workflow_ref.clone()))
            .await
            .expect("applied Workflow reads");
        let stored = serde_json::to_value(&stored.spec).expect("Workflow spec serializes");
        for (step, name) in [(0, "security-reviewer"), (1, "correctness-reviewer")] {
            assert_eq!(
                stored["steps"][step]["action"]["target"]["uid"],
                json!(self.uid(name))
            );
        }
        let newer = team_agent(&self.dir("newer"), "security", |card| {
            card.replacen("version: \"1.0.0\"", "version: \"2.0.0\"", 1)
                .replacen("You are a security reviewer.", "You are a v2 auditor.", 1)
        });
        self.register(&newer).await;

        let reads = self.cards(&self.reader);
        let gateway = Arc::new(ReviewGateway::default());
        for selector in [
            CardSelector::exact(workflow_ref.clone()),
            CardSelector::uid(CardKind::Workflow, workflow_uid.clone()),
        ] {
            let workflow = reads
                .workflow()
                .load(&selector)
                .await
                .expect("registered Workflow loads");
            let run = gateway.run(&workflow).await;
            assert_eq!(run.status, WorkflowRunStatus::Succeeded);
            assert_eq!(run.outputs["review"], json!("FINAL-REVIEW"));
            let identity = run.workflow.expect("registered run keeps its Workflow");
            assert_eq!(identity.uid, Some(workflow_uid.clone()));
        }
        assert_eq!(gateway.requests().len(), 6);
        assert!(
            gateway
                .requests()
                .iter()
                .all(|request| !request.contains("v2 auditor")),
            "a newer Agent version must not float into the pinned graph"
        );

        let refusals = [
            (
                &reads,
                CardSelector::exact(card_ref("Agent", "security-reviewer", "1.0.0", None)),
                "WYRD_REGISTRY_400_INVALID_CARD_SPEC",
            ),
            (
                &reads,
                CardSelector::named(
                    CardKind::Workflow,
                    serde_json::from_value(json!("engineering")).expect("space decodes"),
                    serde_json::from_value(json!("code-review")).expect("name decodes"),
                ),
                "WYRD_REGISTRY_400_VERSION_REQUIRED",
            ),
            (
                &reads,
                CardSelector::exact(card_ref(
                    "Workflow",
                    "code-review",
                    "1.0.0",
                    Some(self.uid("correctness-reviewer")),
                )),
                "WYRD_REGISTRY_400_CARD_REF_UID_NOT_RESOLVABLE_HERE",
            ),
            (
                &self.cards(&self.denied),
                CardSelector::exact(workflow_ref),
                "WYRD_PERMISSION_403_DENIED_RBAC",
            ),
        ];
        for (cards, selector, code) in refusals {
            let error = cards
                .workflow()
                .load(&selector)
                .await
                .expect_err("the selector is refused");
            assert_eq!(error.code(), code, "{selector:?}");
        }
        assert_eq!(
            gateway.requests().len(),
            6,
            "refused loads dispatch nothing"
        );
    }
}

/// Child entry point: load the Workflow named by `WYRD_WORKFLOW_PROBE`
/// through the ambient client configuration, run it on success, and print
/// one JSON report of the refusal code or the run outputs and requests.
///
/// Returns immediately when the probe variable is unset, so it does nothing
/// unless `workflow_loading_journey` spawns it.
///
/// # Panics
/// Panics when a successful load fails to run.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "spawned by workflow_loading_journey with an ambient client environment"]
async fn ambient_workflow_probe() {
    let Ok(path) = std::env::var(PROBE_ENV) else {
        return;
    };
    let report = match Workflow::from_path(&path).await {
        Err(error) => json!({ "code": error.code(), "message": error.to_string() }),
        Ok(workflow) => {
            let gateway = Arc::new(ReviewGateway::default());
            let run = gateway.run(&workflow).await;
            json!({
                "status": format!("{:?}", run.status),
                "outputs": run.outputs,
                "requests": gateway.requests(),
            })
        }
    };
    println!("{PROBE_PREFIX}{report}");
}

/// Prove the Rust SDK loads authored Workflows with automatic external refs,
/// applies one, and loads its exact registered version through Cards.
///
/// # Panics
/// Panics when any load, run, pin, or refusal diverges from the contract.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn workflow_loading_journey() {
    let server = Box::pin(WyrdTestServer::start_bound())
        .await
        .expect("test server starts");
    let mut journey = Journey::start(&server).await;
    let mixed = journey.authored_loads();
    journey.registered_loads(&mixed).await;
    server.shutdown().await.expect("test server shuts down");
}
