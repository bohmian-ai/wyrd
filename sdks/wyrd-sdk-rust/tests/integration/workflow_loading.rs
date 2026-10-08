//! A team loads Workflows from authored files and from the registry: local
//! Agents run with no server, registry refs resolve as the loading client,
//! and an applied Workflow stays pinned to the exact Cards it registered.
//!
//! The `workflow_loading/` fixture Prompts send their Chat request to the
//! built-in `mock` provider, which answers with the rendered user message, so
//! every output names the Prompt body that ran.

use std::collections::HashMap;
use std::path::PathBuf;

use serde_json::{Map, Value, json};
use wyrd_sdk::cards::{CardKind, CardRef, CardSelector, Cards};
use wyrd_sdk::{Card, Workflow, WorkflowRun, WorkflowRunStatus, WyrdClient};

use crate::support::{Deployment, fixture, register};

/// The `review` output of a Workflow whose three reviewers are all local.
const LOCAL_REVIEW: &str =
    "final review of diff | local security review of diff | local correctness review of diff";

/// The `review` output of the `mixed` Workflow run against the registered
/// 1.0.0 team.
const REGISTERED_REVIEW: &str = "final review of diff | registered security review of diff | registered correctness review of diff";

/// Path of a file under `fixtures/cards/workflow_loading`.
fn workflow_fixture(relative: &str) -> PathBuf {
    fixture(&format!("cards/workflow_loading/{relative}"))
}

/// The code-review example Workflow, whose steps call the Wyrd gateway.
fn example() -> PathBuf {
    PathBuf::from(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../examples/workflows/code-review/workflow.yaml"
    ))
}

/// Run `workflow` with the fixed input `code = "diff"`.
///
/// # Panics
/// Panics when the run is refused before it starts.
async fn run(workflow: &Workflow) -> WorkflowRun {
    workflow
        .run(Map::from_iter([("code".to_owned(), Value::from("diff"))]))
        .await
        .expect("the run starts")
}

/// Assert that `run` succeeded with exactly `outputs`.
///
/// # Panics
/// Panics when the run did not succeed or an output differs.
fn assert_succeeded(run: &WorkflowRun, outputs: &Value) {
    assert_eq!(run.status, WorkflowRunStatus::Succeeded, "{:?}", run.error);
    assert_eq!(&json!(run.outputs), outputs);
}

/// Register the `workflow_loading` fixtures at `relative`, in order, through
/// `cards`, and return the exact reference of every Card they registered,
/// keyed by Card name.
///
/// # Panics
/// Panics when a registration is refused.
async fn register_all(cards: &Cards, relative: &[&str]) -> HashMap<String, CardRef> {
    let mut refs = HashMap::new();
    for file in relative {
        let receipt = register(cards, &format!("cards/workflow_loading/{file}")).await;
        refs.extend(
            receipt
                .outcomes
                .into_iter()
                .map(|outcome| (outcome.card_ref.name.to_string(), outcome.card_ref)),
        );
    }
    refs
}

/// The registered reviewer team: the security and correctness Agents and
/// their Prompts, keyed by Card name.
///
/// # Panics
/// Panics when a registration is refused.
async fn team(deployment: &Deployment) -> HashMap<String, CardRef> {
    register_all(
        &deployment.cards(),
        &["team/security.yaml", "team/correctness.yaml"],
    )
    .await
}

/// The applied `code-review` Workflow, the team it references, and its local
/// final reviewer, keyed by Card name; a newer security Agent is registered
/// after it.
///
/// # Panics
/// Panics when a registration is refused.
async fn applied(deployment: &Deployment) -> HashMap<String, CardRef> {
    let refs = register_all(
        &deployment.cards(),
        &[
            "team/security.yaml",
            "team/correctness.yaml",
            "mixed/workflow.yaml",
        ],
    )
    .await;
    register(
        &deployment.cards(),
        "cards/workflow_loading/team-v2/security.yaml",
    )
    .await;
    refs
}

/// A client of a principal that may only read Cards.
///
/// # Panics
/// Panics when the principal cannot be seeded.
async fn reader(deployment: &Deployment) -> WyrdClient {
    deployment.client(
        &deployment
            .scoped_key("workflow_reader", &["cards:read"])
            .await,
    )
}

/// A wholly local Workflow loads and runs with no server and no credential.
///
/// # Panics
/// Panics when the load or run fails or the outputs differ.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "a story journey; runs in the Rust SDK journey lane"]
async fn local_workflow_runs_without_credentials() {
    let local = Workflow::from_path(workflow_fixture("shadowed/local-workflow.yaml"))
        .await
        .expect("the local Workflow loads");

    let run = run(&local).await;

    assert_succeeded(
        &run,
        &json!({ "security": "local security review of diff", "review": LOCAL_REVIEW }),
    );
}

/// Text input is shorthand for a declared input named `input`; the local
/// Workflow declares only `code`, so a text run is refused.
///
/// # Panics
/// Panics when the load fails or the run is not refused with the run-request
/// code.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "a story journey; runs in the Rust SDK journey lane"]
async fn text_input_needs_a_declared_input_named_input() {
    let local = Workflow::from_path(workflow_fixture("shadowed/local-workflow.yaml"))
        .await
        .expect("the local Workflow loads");

    let refused = local
        .run("diff")
        .await
        .expect_err("text input needs an `input` input");

    assert_eq!(refused.code(), "WYRD_WORKFLOW_422_RUN_REQUEST");
}

/// YAML text loads with its identity and steps but has no directory, so its
/// relative Agent targets never resolve; text that is no Workflow is refused.
///
/// # Panics
/// Panics when the load fails, the identity or steps differ, or a refusal
/// carries another code.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "a story journey; runs in the Rust SDK journey lane"]
async fn yaml_workflow_loads_without_resolving_file_targets() {
    let text = std::fs::read_to_string(workflow_fixture("shadowed/local-workflow.yaml"))
        .expect("the fixture reads");
    let yaml = Workflow::from_yaml(&text).expect("the YAML Workflow loads");

    let refused = yaml
        .run(Map::from_iter([("code".to_owned(), Value::from("diff"))]))
        .await
        .expect_err("a relative Agent target never resolves");
    let invalid = Workflow::from_yaml("kind: Nope").expect_err("`kind: Nope` is no Workflow");

    assert_eq!(
        [yaml.space(), yaml.name(), yaml.version()],
        [
            Some("workflow-loading"),
            Some("local-review"),
            Some("1.0.0")
        ]
    );
    assert_eq!(yaml.steps(), ["security", "correctness", "final_review"]);
    assert_eq!(refused.code(), "WYRD_WORKFLOW_404_AGENT");
    assert_eq!(invalid.code(), "WYRD_WORKFLOW_422_VALIDATION");
}

/// A Workflow routed through the Wyrd gateway loads with no credential, and
/// its run is refused before any step calls a model.
///
/// # Panics
/// Panics when the load fails or the run is not refused with the binding
/// code.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "a story journey; runs in the Rust SDK journey lane"]
async fn gateway_workflow_without_credentials_is_refused_before_any_step() {
    let example = Workflow::from_path(example())
        .await
        .expect("the example loads");

    let refused = example
        .run(Map::from_iter([("code".to_owned(), Value::from("diff"))]))
        .await
        .expect_err("no gateway client is available");

    assert_eq!(refused.code(), "WYRD_WORKFLOW_503_BINDING_UNAVAILABLE");
}

/// A file whose steps reference registered Agents loads through the
/// registry as the client it is given and runs the registered Prompts.
///
/// # Panics
/// Panics when the load or run fails or the outputs differ.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn registry_refs_resolve_through_the_registry() {
    let deployment = Deployment::start().await;
    team(&deployment).await;
    let mixed = Workflow::from_path_with_client(
        workflow_fixture("mixed/workflow.yaml"),
        reader(&deployment).await,
    )
    .await
    .expect("the registry refs resolve");

    let run = run(&mixed).await;

    assert_succeeded(&run, &json!({ "review": REGISTERED_REVIEW }));
    deployment.shutdown().await;
}

/// A client that cannot read Cards cannot load registry refs.
///
/// # Panics
/// Panics when the load succeeds or is refused with another code.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn registry_refs_without_read_access_are_refused() {
    let deployment = Deployment::start().await;
    team(&deployment).await;
    let no_roles = deployment.client(&deployment.key("workflow_no_roles", &[]).await);

    let refused =
        Workflow::from_path_with_client(workflow_fixture("mixed/workflow.yaml"), no_roles)
            .await
            .expect_err("the registry read is refused");

    assert_eq!(refused.code(), "WYRD_PERMISSION_403_DENIED_RBAC");
    deployment.shutdown().await;
}

/// A local Agent and the registered Agent with the same identity each run
/// their own Prompt.
///
/// # Panics
/// Panics when the load or run fails or the outputs differ.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn local_sibling_never_satisfies_a_registry_ref() {
    let deployment = Deployment::start().await;
    team(&deployment).await;
    let shadowed = Workflow::from_path_with_client(
        workflow_fixture("shadowed/workflow.yaml"),
        reader(&deployment).await,
    )
    .await
    .expect("the shadowed Workflow loads");

    let run = run(&shadowed).await;

    assert_succeeded(
        &run,
        &json!({
            "security": "local security review of diff",
            "registered_security": "registered security review of diff",
            "review": LOCAL_REVIEW,
        }),
    );
    deployment.shutdown().await;
}

/// A file referencing a Card deleted from the registry is refused.
///
/// # Panics
/// Panics when a setup step fails, or the load succeeds or is refused with
/// another code.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn deleted_registry_card_is_refused() {
    let deployment = Deployment::start().await;
    let cards = deployment.cards();
    let retired = register(&cards, "cards/workflow_loading/retired/retired-prompt.yaml").await;
    cards
        .delete(CardSelector::exact(retired.root))
        .await
        .expect("the Prompt deletes");

    let refused = Workflow::from_path_with_client(
        workflow_fixture("retired/workflow.yaml"),
        reader(&deployment).await,
    )
    .await
    .expect_err("a deleted Card is refused");

    assert_eq!(refused.code(), "WYRD_REGISTRY_404_CARD_NOT_FOUND");
    deployment.shutdown().await;
}

/// The applied Workflow and each of its Agents keep the exact registered
/// references, in their stored specs and in their derived relationships,
/// after a newer security Agent registers.
///
/// # Panics
/// Panics when a read fails or a reference differs from the registration.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn applied_workflow_stays_pinned_to_its_registered_cards() {
    let deployment = Deployment::start().await;
    let refs = applied(&deployment).await;
    let reader = Cards::with_client(reader(&deployment).await);
    let agents = [
        "security-reviewer",
        "correctness-reviewer",
        "final-reviewer",
    ]
    .map(|agent| refs[agent].clone());

    let workflow = reader
        .get(CardSelector::exact(refs["code-review"].clone()))
        .await
        .expect("the Workflow reads");

    assert_eq!(workflow.kind, CardKind::Workflow);
    let steps = json!(workflow.spec)["steps"]
        .as_array()
        .expect("the Workflow has steps")
        .iter()
        .map(|step| step["action"]["target"].clone())
        .collect::<Vec<_>>();
    assert_eq!(
        steps,
        agents.iter().map(|agent| json!(agent)).collect::<Vec<_>>()
    );
    assert_eq!(outbound(&workflow), sorted(agents.to_vec()));
    for (agent, prompt) in [
        ("security-reviewer", "security-review-prompt"),
        ("correctness-reviewer", "correctness-review-prompt"),
        ("final-reviewer", "final-review-prompt"),
    ] {
        let stored = reader
            .get(CardSelector::exact(refs[agent].clone()))
            .await
            .expect("the Agent reads");
        assert_eq!(json!(stored.spec)["prompt"], json!(refs[prompt]), "{agent}");
        assert_eq!(outbound(&stored), [refs[prompt].clone()], "{agent}");
    }
    deployment.shutdown().await;
}

/// The server-derived outbound relationship targets of `card`, by name.
fn outbound(card: &Card) -> Vec<CardRef> {
    sorted(
        card.relationships
            .outbound_refs
            .iter()
            .map(|relationship| relationship.card_ref.clone())
            .collect(),
    )
}

/// `refs` ordered by Card name.
fn sorted(mut refs: Vec<CardRef>) -> Vec<CardRef> {
    refs.sort_by_key(|card_ref| card_ref.name.to_string());
    refs
}

/// The applied Workflow, loaded by identity and by UID, runs its pinned
/// 1.0.0 Agents, never the newer one, and its run names the Workflow.
///
/// # Panics
/// Panics when a load or run fails or a run differs.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn loaded_workflow_runs_its_pinned_cards() {
    let deployment = Deployment::start().await;
    let refs = applied(&deployment).await;
    let reader = Cards::with_client(reader(&deployment).await);
    let code_review = &refs["code-review"];
    let uid = code_review.uid.clone().expect("registered Card has a UID");
    let by_identity = CardRef {
        uid: None,
        ..code_review.clone()
    };

    for selector in [
        CardSelector::exact(by_identity),
        CardSelector::uid(CardKind::Workflow, uid.clone()),
    ] {
        let workflow = reader
            .workflow()
            .load(&selector)
            .await
            .expect("the registered Workflow loads");

        let run = run(&workflow).await;

        assert_succeeded(&run, &json!({ "review": REGISTERED_REVIEW }));
        assert_eq!(
            run.steps["final_review"].text.as_deref(),
            Some(REGISTERED_REVIEW)
        );
        assert_eq!(
            run.workflow.and_then(|workflow| workflow.uid),
            Some(uid.clone())
        );
    }
    deployment.shutdown().await;
}

/// An Agent's UID names no Workflow.
///
/// # Panics
/// Panics when the load succeeds or is refused with another code.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn loading_a_bad_selector_is_refused() {
    let deployment = Deployment::start().await;
    let refs = team(&deployment).await;
    let agent_uid = refs["security-reviewer"]
        .uid
        .clone()
        .expect("registered Card has a UID");

    let refused = Cards::with_client(reader(&deployment).await)
        .workflow()
        .load(&CardSelector::uid(CardKind::Workflow, agent_uid))
        .await
        .expect_err("an Agent's UID names no Workflow");

    assert_eq!(refused.code(), "WYRD_REGISTRY_404_CARD_NOT_FOUND");
    deployment.shutdown().await;
}
