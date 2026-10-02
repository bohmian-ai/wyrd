//! Rust SDK Workflow loading journey through the public `wyrd_sdk` crate.
//!
//! Uses the checked-in fixtures in `tests/fixtures/workflow-loading` (see its
//! README). The journey:
//!
//! 1. loads the wholly local code-review example with `Workflow::from_path`,
//!    which needs no server or credentials;
//! 2. registers the team reviewer Agents, then applies the `mixed` Workflow
//!    that references them;
//! 3. registers a newer security Agent;
//! 4. loads the applied Workflow through `cards.workflow()` by exact ref and
//!    by UID, runs it, and checks the newer Agent did not float in;
//! 5. checks wrong, versionless, mismatched, and unauthorized selectors are
//!    refused without dispatching anything.
//!
//! Authored loading with registry refs reads credentials from the process
//! environment, so it is proved by the Python and TypeScript journeys, where
//! setting the environment is ordinary test setup.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use secrecy::ExposeSecret;
use serde_json::{Map, Value, json};
use skald_providers::ProviderError;
use skald_spec::ProviderResponse;
use skald_spec::wire::openai_chat::OpenAiChatResponse;
use skald_workflow::{
    WorkflowExecutionDependencies, WorkflowRun, WorkflowRunOptions, WorkflowRunStatus,
    WyrdGatewayCall, WyrdGatewayCaller,
};
use tokio_util::sync::CancellationToken;
use wyrd_sdk::Workflow;
use wyrd_sdk::bifrost::client_from_options;
use wyrd_sdk::cards::{CardKind, CardRef, CardSelector, Cards};
use wyrd_testing::Bootstrap;
use wyrd_testing::server::WyrdTestServer;

/// Fake Wyrd gateway that answers each reviewer with a fixed text and records
/// every request it receives.
#[derive(Default)]
struct ReviewGateway {
    /// Serialized provider requests in arrival order.
    requests: Mutex<Vec<String>>,
}

#[async_trait]
impl WyrdGatewayCaller for ReviewGateway {
    /// Record the request and answer with the fixed text for its reviewer.
    ///
    /// # Errors
    /// Never returns an error.
    ///
    /// # Panics
    /// Panics if the static completion fixture stops decoding.
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
    /// Return a copy of every request recorded so far.
    fn requests(&self) -> Vec<String> {
        self.requests
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }
}

/// Run `workflow` with `gateway` as its Wyrd gateway and a fixed `code` input.
///
/// # Panics
/// Panics when the run refuses to start.
async fn run(workflow: &Workflow, gateway: &Arc<ReviewGateway>) -> WorkflowRun {
    let dependencies = WorkflowExecutionDependencies::new(skald_runtime::ProviderRegistry::new())
        .with_wyrd_gateway(Arc::clone(gateway) as Arc<dyn WyrdGatewayCaller>);
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

/// Path to a file under the repository root.
fn repo_file(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(relative)
}

/// Path to a file under `tests/fixtures/workflow-loading`.
fn fixture(relative: &str) -> PathBuf {
    repo_file("tests/fixtures/workflow-loading").join(relative)
}

/// Bootstrap a service principal holding `roles` and return a Cards handle
/// authenticated as it.
///
/// # Panics
/// Panics when the principal cannot bootstrap or the client cannot build.
async fn cards_as(server: &WyrdTestServer, name: &str, roles: &[&str]) -> Cards {
    let Bootstrap::Machine { api_key, .. } = server
        .bootstrap_service(name, roles)
        .await
        .expect("principal bootstraps")
    else {
        panic!("expected a machine principal");
    };
    let base_url = server.base_url().expect("bound server has a URL");
    let client = client_from_options(Some(base_url), Some(api_key.expose_secret()), None)
        .expect("client builds");
    Cards::with_client(client)
}

/// Exact reference to a Card in the `workflow-loading` space.
///
/// # Panics
/// Panics when the fields do not decode as a Card reference.
fn card_ref(kind: &str, name: &str, version: &str, uid: Option<&str>) -> CardRef {
    serde_json::from_value(json!({
        "kind": kind,
        "name": name,
        "version": version,
        "space": "workflow-loading",
        "uid": uid,
    }))
    .expect("test card reference decodes")
}

/// Register the fixture at `relative` and return the UID of every Card it
/// registered, keyed by Card name.
///
/// # Panics
/// Panics when registration fails or an outcome has no UID.
async fn register(cards: &Cards, relative: &str) -> HashMap<String, String> {
    let receipt = Box::pin(cards.register_from_path(&fixture(relative)))
        .await
        .expect("fixture registers");
    receipt
        .outcomes
        .into_iter()
        .map(|outcome| {
            let uid = outcome.card_ref.uid.expect("registered Card has a UID");
            (outcome.card_ref.name.to_string(), uid.to_string())
        })
        .collect()
}

/// Check that `cards.workflow().load` refuses an Agent selector, a
/// versionless selector, a Workflow ref carrying `other_uid` (another Card's
/// UID), and a principal without read access, each with its exact code.
///
/// # Panics
/// Panics when a selector loads or is refused with another code.
async fn assert_selectors_refused(
    reader: &Cards,
    no_roles: &Cards,
    workflow_ref: CardRef,
    other_uid: &str,
) {
    let agent_selector = CardSelector::exact(card_ref("Agent", "security-reviewer", "1.0.0", None));
    let error = reader
        .workflow()
        .load(&agent_selector)
        .await
        .expect_err("not a Workflow");
    assert_eq!(error.code(), "WYRD_REGISTRY_400_INVALID_CARD_SPEC");

    let versionless = CardSelector::named(
        CardKind::Workflow,
        serde_json::from_value(json!("workflow-loading")).expect("space decodes"),
        serde_json::from_value(json!("code-review")).expect("name decodes"),
    );
    let error = reader
        .workflow()
        .load(&versionless)
        .await
        .expect_err("no version");
    assert_eq!(error.code(), "WYRD_REGISTRY_400_VERSION_REQUIRED");

    let wrong_uid = card_ref("Workflow", "code-review", "1.0.0", Some(other_uid));
    let error = reader
        .workflow()
        .load(&CardSelector::exact(wrong_uid))
        .await
        .expect_err("UID names another Card");
    assert_eq!(
        error.code(),
        "WYRD_REGISTRY_400_CARD_REF_UID_NOT_RESOLVABLE_HERE"
    );

    let error = no_roles
        .workflow()
        .load(&CardSelector::exact(workflow_ref))
        .await
        .expect_err("principal without read access");
    assert_eq!(error.code(), "WYRD_PERMISSION_403_DENIED_RBAC");
}

/// Prove the Rust SDK loads a local Workflow file, and loads an applied
/// Workflow's exact registered version through Cards.
///
/// # Panics
/// Panics when any load, run, pin, or refusal diverges from the contract.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn workflow_loading_journey() {
    // 1. A wholly local Workflow file loads without any server.
    let local = Workflow::from_path(repo_file("examples/workflows/code-review/workflow.yaml"))
        .await
        .expect("local example loads");
    assert_eq!(
        local.as_skald().step_ids(),
        vec!["security", "correctness", "final_review"]
    );

    let server = Box::pin(WyrdTestServer::start_bound())
        .await
        .expect("test server starts");
    let writer = cards_as(&server, "workflow_writer", &["writer"]).await;
    let reader = cards_as(&server, "workflow_reader", &["reader"]).await;
    let no_roles = cards_as(&server, "workflow_no_roles", &[]).await;

    // 2. Register the team Agents, then apply the Workflow that references them.
    let mut uids = register(&writer, "team/security.yaml").await;
    uids.extend(register(&writer, "team/correctness.yaml").await);
    let workflow_uid = register(&writer, "mixed/workflow.yaml").await["code-review"].clone();
    let workflow_ref = card_ref("Workflow", "code-review", "1.0.0", Some(&workflow_uid));

    // The stored Workflow pins each step to the exact registered Agent UID.
    let stored = writer
        .get(CardSelector::exact(workflow_ref.clone()))
        .await
        .expect("applied Workflow reads");
    let stored = serde_json::to_value(&stored.spec).expect("Workflow spec serializes");
    assert_eq!(
        stored["steps"][0]["action"]["target"]["uid"],
        json!(uids["security-reviewer"])
    );
    assert_eq!(
        stored["steps"][1]["action"]["target"]["uid"],
        json!(uids["correctness-reviewer"])
    );

    // 3. A newer security Agent registers after the Workflow was applied.
    register(&writer, "team-v2/security.yaml").await;

    // 4. Load by exact ref and by UID; both run the pinned 1.0.0 Agents.
    let gateway = Arc::new(ReviewGateway::default());
    let workflow_selector = CardSelector::uid(
        CardKind::Workflow,
        serde_json::from_value(json!(workflow_uid)).expect("UID decodes"),
    );
    for selector in [CardSelector::exact(workflow_ref.clone()), workflow_selector] {
        let workflow = reader
            .workflow()
            .load(&selector)
            .await
            .expect("registered Workflow loads");
        let run = run(&workflow, &gateway).await;
        assert_eq!(run.status, WorkflowRunStatus::Succeeded);
        assert_eq!(run.outputs["review"], json!("FINAL-REVIEW"));
        let identity = run.workflow.expect("registered run keeps its Workflow");
        assert_eq!(
            identity.uid.map(|uid| uid.to_string()),
            Some(workflow_uid.clone())
        );
    }
    let requests = gateway.requests();
    assert_eq!(requests.len(), 6, "two runs of three steps each");
    assert!(
        requests
            .iter()
            .all(|request| !request.contains("v2 auditor")),
        "the newer Agent version must not float into the pinned Workflow"
    );

    // 5. Refused selectors read nothing they should not and dispatch nothing.
    assert_selectors_refused(
        &reader,
        &no_roles,
        workflow_ref,
        &uids["correctness-reviewer"],
    )
    .await;
    assert_eq!(
        gateway.requests().len(),
        6,
        "refused loads dispatch nothing"
    );
    server.shutdown().await.expect("test server shuts down");
}
