//! Rust SDK Workflow loading journey through the public `wyrd_sdk` crate.
//!
//! Uses the checked-in fixtures in `tests/fixtures/workflow-loading` (see its
//! README). Their Prompts send the Native Chat request to the built-in `mock`
//! provider, which answers with the rendered user message, so every output
//! shows which Prompt body ran and what was bound into it. The journey:
//!
//! 1. loads and runs the wholly local Workflow with `Workflow::from_path`,
//!    which needs no server or credentials;
//! 2. registers the team reviewer Agents;
//! 3. loads authored Workflows that reference them through the ambient
//!    environment, in a child process, and checks the refusals for no
//!    credential, a principal without read access, and a deleted Card;
//! 4. applies the `mixed` Workflow, registers a newer security Agent, and
//!    checks the Workflow and each Agent stay locked to the exact registered
//!    Agents and Prompts in both spec references and relationships;
//! 5. loads the applied Workflow through `cards.workflow()` by exact ref and
//!    by UID, runs it, and checks the newer Agent did not float in;
//! 6. checks wrong, versionless, mismatched, and unauthorized selectors are
//!    refused;
//! 7. runs the code-review example from its file through the public Wyrd
//!    gateway and through an external gateway binding, against one local
//!    upstream that counts every call, then registers it, which calls
//!    nothing, and runs the registered Workflow through the gateway.

use std::collections::HashMap;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt as _;
use std::path::{Path, PathBuf};
use std::process::Command;

use secrecy::ExposeSecret;
use serde_json::{Map, Value, json};
use skald_workflow::{WorkflowRun, WorkflowRunStatus};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};
use wyrd_client::gateway_credential::CredentialWriter;
use wyrd_sdk::Workflow;
use wyrd_sdk::bifrost::client_from_options;
use wyrd_sdk::cards::{CardKind, CardRef, CardSelector, Cards};
use wyrd_sdk::{Gateway, WyrdClient};
use wyrd_testing::Bootstrap;
use wyrd_testing::server::WyrdTestServer;

/// Environment variable naming the Workflow file the child process loads.
const CHILD_PATH: &str = "WORKFLOW_LOADING_CHILD_PATH";

/// Prefix of the one stdout line on which the child reports its outcome.
const CHILD_OUTCOME: &str = "WORKFLOW_LOADING_CHILD_OUTCOME ";

/// The `final_review` output the `mixed` Workflow produces from the
/// registered 1.0.0 team Agents.
const REGISTERED_REVIEW: &str = "final review of diff | registered security review of diff | registered correctness review of diff";

/// Secret the `review-gateway` binding sends to the external gateway.
const REVIEW_SECRET: &str = "review-secret-value";

/// Run `workflow` with the fixed input `code = "diff"`.
///
/// # Panics
/// Panics when the run refuses to start.
async fn run(workflow: &Workflow) -> WorkflowRun {
    let input = Map::from_iter([("code".to_owned(), Value::from("diff"))]);
    workflow.run(input).await.expect("workflow run starts")
}

/// Assert that `run` succeeded with exactly the declared `outputs`.
///
/// # Panics
/// Panics when the run did not succeed or an output differs.
fn assert_succeeded(run: &WorkflowRun, outputs: &Value) {
    assert_eq!(run.status, WorkflowRunStatus::Succeeded, "{:?}", run.error);
    assert_eq!(&json!(run.outputs), outputs);
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

/// Bootstrap a service principal holding `roles` and return its API key.
///
/// # Panics
/// Panics when the principal cannot bootstrap.
async fn api_key(server: &WyrdTestServer, name: &str, roles: &[&str]) -> String {
    let Bootstrap::Machine { api_key, .. } = server
        .bootstrap_service(name, roles)
        .await
        .expect("principal bootstraps")
    else {
        panic!("expected a machine principal");
    };
    api_key.expose_secret().to_owned()
}

/// Return a shared client of `server` authenticated with `api_key`.
///
/// # Panics
/// Panics when the client cannot build.
fn client_with(server: &WyrdTestServer, api_key: &str) -> WyrdClient {
    let base_url = server.base_url().expect("bound server has a URL");
    client_from_options(Some(base_url), Some(api_key), None).expect("client builds")
}

/// Return a Cards handle authenticated with `api_key`.
///
/// # Panics
/// Panics when the client cannot build.
fn cards_with(server: &WyrdTestServer, api_key: &str) -> Cards {
    Cards::with_client(client_with(server, api_key))
}

/// Load the authored Workflow at `path` in a child process whose only
/// configuration is `WYRD_SERVER_URL`, when given `WYRD_API_KEY`, and the
/// client configuration home `config_home`, or an empty one.
///
/// `Workflow::from_path` reads that configuration from the process
/// environment. Setting it inside this multithreaded test would need
/// `unsafe`, so a fresh copy of this test binary runs
/// [`authored_load_child`] with the environment set instead.
///
/// Returns the child's outcome: `{"error": "<code>"}` when loading failed, or
/// `{"run": <WorkflowRun>}` after running the loaded Workflow.
///
/// # Panics
/// Panics when the child cannot start, fails, or reports no outcome.
fn load_in_child(
    server: &WyrdTestServer,
    api_key: Option<&str>,
    path: &Path,
    config_home: Option<&Path>,
) -> Value {
    let empty = tempfile::tempdir().expect("empty config home creates");
    let mut command = Command::new(std::env::current_exe().expect("current test executable"));
    command
        .args(["authored_load_child", "--exact", "--ignored", "--nocapture"])
        .env(CHILD_PATH, path)
        .env(
            "WYRD_SERVER_URL",
            server.base_url().expect("bound server has a URL"),
        )
        .env("WYRD_CONFIG_HOME", config_home.unwrap_or(empty.path()))
        .env_remove("WYRD_API_KEY")
        .env_remove("WYRD_ACCESS_TOKEN");
    if let Some(api_key) = api_key {
        command.env("WYRD_API_KEY", api_key);
    }
    let output = command.output().expect("child process starts");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "child failed:\n{stdout}\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let line = stdout
        .lines()
        .find_map(|line| line.strip_prefix(CHILD_OUTCOME))
        .unwrap_or_else(|| panic!("child reported no outcome:\n{stdout}"));
    serde_json::from_str(line).expect("child outcome is JSON")
}

/// Register the fixture at `relative` and return the exact reference of every
/// Card it registered, keyed by Card name.
///
/// # Panics
/// Panics when registration fails.
async fn register(cards: &Cards, relative: &str) -> HashMap<String, CardRef> {
    let receipt = Box::pin(cards.register_from_path(&fixture(relative)))
        .await
        .expect("fixture registers");
    receipt
        .outcomes
        .into_iter()
        .map(|outcome| (outcome.card_ref.name.to_string(), outcome.card_ref))
        .collect()
}

/// UID of the registered Card `name` in `refs`, as a string.
///
/// # Panics
/// Panics when `name` is absent or its reference has no UID.
fn uid(refs: &HashMap<String, CardRef>, name: &str) -> String {
    refs[name]
        .uid
        .as_ref()
        .expect("registered Card has a UID")
        .to_string()
}

/// Decode the exact reference at `value`, a locked spec reference.
///
/// # Panics
/// Panics when `value` is not a Card reference.
fn spec_ref(value: &Value) -> CardRef {
    serde_json::from_value(value.clone()).expect("locked spec reference decodes")
}

/// Read the Card at `card_ref` and return its locked spec references, found
/// under `pointers`, and its server-derived outbound relationship targets.
///
/// # Panics
/// Panics when the Card cannot be read or a pointer is absent.
async fn locked_refs(
    cards: &Cards,
    card_ref: &CardRef,
    pointers: &[&str],
) -> (Vec<CardRef>, Vec<CardRef>) {
    let card = cards
        .get(CardSelector::exact(card_ref.clone()))
        .await
        .expect("registered Card reads");
    let spec = serde_json::to_value(&card.spec).expect("spec serializes");
    let spec_refs = pointers
        .iter()
        .map(|pointer| spec_ref(spec.pointer(pointer).expect("locked reference present")))
        .collect();
    let outbound = card
        .relationships
        .outbound_refs
        .into_iter()
        .map(|relationship| relationship.card_ref)
        .collect();
    (spec_refs, outbound)
}

/// Check that the applied Workflow and each of its three Agents lock their
/// dependencies to the exact registered references in `refs`, both in the
/// stored spec and in the server-derived outbound relationships.
///
/// # Panics
/// Panics when a spec reference or relationship target differs from the
/// registration receipt.
async fn assert_locked_graph(cards: &Cards, refs: &HashMap<String, CardRef>) {
    let agents = [
        &refs["security-reviewer"],
        &refs["correctness-reviewer"],
        &refs["final-reviewer"],
    ];
    let (steps, mut outbound) = locked_refs(
        cards,
        &refs["code-review"],
        &[
            "/steps/0/action/target",
            "/steps/1/action/target",
            "/steps/2/action/target",
        ],
    )
    .await;
    assert_eq!(steps.iter().collect::<Vec<_>>(), agents);
    outbound.sort_by_key(|target| target.name.to_string());
    let mut expected: Vec<&CardRef> = agents.to_vec();
    expected.sort_by_key(|target| target.name.to_string());
    assert_eq!(outbound.iter().collect::<Vec<_>>(), expected);

    for (agent, prompt) in [
        ("security-reviewer", "security-review-prompt"),
        ("correctness-reviewer", "correctness-review-prompt"),
        ("final-reviewer", "final-review-prompt"),
    ] {
        let (spec_prompt, outbound) = locked_refs(cards, &refs[agent], &["/prompt"]).await;
        assert_eq!(spec_prompt, vec![refs[prompt].clone()], "{agent} spec");
        assert_eq!(
            outbound,
            vec![refs[prompt].clone()],
            "{agent} relationships"
        );
    }
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
    assert_eq!(error.code(), "WYRD_WORKFLOW_400_INVALID_CARD_REF");

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
    assert_eq!(error.code(), "WYRD_WORKFLOW_400_INVALID_CARD_REF");

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

/// Check that authored Workflows referencing the registered team Agents load
/// and run through the ambient configuration with `reader_key`, are refused
/// with no credential or with `no_roles_key`, and that a reference to a
/// deleted Card is refused. `writer` registers and deletes that Card.
///
/// # Panics
/// Panics when a load, run, or refusal diverges from the contract.
async fn assert_authored_loads(
    server: &WyrdTestServer,
    writer: &Cards,
    reader_key: &str,
    no_roles_key: &str,
) {
    assert_eq!(
        load_in_child(server, None, &fixture("mixed/workflow.yaml"), None),
        json!({ "error": "WYRD_CLIENT_401_NO_CREDENTIALS" })
    );
    assert_eq!(
        load_in_child(
            server,
            Some(no_roles_key),
            &fixture("mixed/workflow.yaml"),
            None
        ),
        json!({ "error": "WYRD_PERMISSION_403_DENIED_RBAC" })
    );
    let mixed = load_in_child(
        server,
        Some(reader_key),
        &fixture("mixed/workflow.yaml"),
        None,
    );
    assert_eq!(mixed["run"]["status"], "succeeded");
    assert_eq!(
        mixed["run"]["outputs"],
        json!({ "review": REGISTERED_REVIEW })
    );

    // A local sibling and the registered Agent with the same identity each
    // run their own Prompt.
    let shadowed = load_in_child(
        server,
        Some(reader_key),
        &fixture("shadowed/workflow.yaml"),
        None,
    );
    assert_eq!(shadowed["run"]["status"], "succeeded");
    assert_eq!(
        shadowed["run"]["outputs"],
        json!({
            "security": "local security review of diff",
            "registered_security": "registered security review of diff",
            "review": "final review of diff | local security review of diff | local correctness review of diff",
        })
    );

    // A reference to a deleted Card is refused.
    let retired = register(writer, "retired/retired-prompt.yaml").await;
    writer
        .delete(CardSelector::uid(
            CardKind::Prompt,
            retired["retired-prompt"]
                .uid
                .clone()
                .expect("registered Card has a UID"),
        ))
        .await
        .expect("retired Prompt deletes");
    assert_eq!(
        load_in_child(
            server,
            Some(reader_key),
            &fixture("retired/workflow.yaml"),
            None
        ),
        json!({ "error": "WYRD_REGISTRY_404_CARD_NOT_FOUND" })
    );
}

/// Copy the code-review example into a temporary bundle whose Workflow sends
/// every model call over the `ext_gateway` route to `upstream` through the
/// `review-gateway` binding.
///
/// # Panics
/// Panics when the example cannot be read or the copy cannot be written.
fn external_example(upstream: &MockServer) -> tempfile::TempDir {
    let example = repo_file("examples/workflows/code-review");
    let bundle = tempfile::tempdir().expect("bundle directory creates");
    for directory in ["agents", "prompts"] {
        std::fs::create_dir(bundle.path().join(directory)).expect("bundle directory creates");
        for file in ["security", "correctness", "final-reviewer"] {
            let relative = format!("{directory}/{file}.yaml");
            std::fs::copy(example.join(&relative), bundle.path().join(&relative))
                .expect("bundle file copies");
        }
    }
    let workflow =
        std::fs::read_to_string(example.join("workflow.yaml")).expect("example Workflow reads");
    assert!(
        workflow.contains("    kind: wyrd_gateway\n"),
        "example route moved"
    );
    let route = format!(
        "    kind: ext_gateway\n    protocol: openai_chat\n    base_url: {}/v1\n    credential_binding: review-gateway\n",
        upstream.uri()
    );
    std::fs::write(
        bundle.path().join("workflow.yaml"),
        workflow.replacen("    kind: wyrd_gateway\n", &route, 1),
    )
    .expect("external Workflow writes");
    bundle
}

/// Write a client configuration home whose `review-gateway` binding speaks
/// Chat Completions to `upstream` and sends the owner-only secret file as
/// `x-review-secret`.
///
/// # Panics
/// Panics when the files cannot be written.
fn review_gateway_config(upstream: &MockServer) -> tempfile::TempDir {
    let home = tempfile::tempdir().expect("config home creates");
    let secret = home.path().join("review-secret");
    std::fs::write(&secret, REVIEW_SECRET).expect("secret writes");
    #[cfg(unix)]
    std::fs::set_permissions(&secret, std::fs::Permissions::from_mode(0o600))
        .expect("secret restricts");
    std::fs::write(
        home.path().join("config.toml"),
        format!(
            "[workflow.external_gateway_bindings.review-gateway]\n\
             protocol = \"openai_chat\"\n\
             origin = \"{}\"\n\
             secret_headers = {{ x-review-secret = {{ source = \"file\", path = \"{}\" }} }}\n",
            upstream.uri(),
            secret.display()
        ),
    )
    .expect("config writes");
    home
}

/// Start the local upstream that answers every Chat Completions call with
/// `hi`. It roots the server's gateway adapters and also serves as the
/// external gateway, so its request log counts every model call.
async fn chat_upstream() -> MockServer {
    let upstream = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": "chatcmpl-1",
            "object": "chat.completion",
            "created": 1,
            "model": "gpt-5-5",
            "choices": [{
                "index": 0,
                "message": { "role": "assistant", "content": "hi" },
                "finish_reason": "stop"
            }]
        })))
        .mount(&upstream)
        .await;
    upstream
}

/// Count the calls `upstream` received through the gateway, which carry
/// `authorization`, and through the `review-gateway` binding, which carry
/// the binding secret and no `authorization`.
///
/// # Panics
/// Panics when request recording is disabled or a call carries neither, or a
/// binding call carries the wrong secret.
async fn model_calls(upstream: &MockServer) -> (usize, usize) {
    let requests = upstream
        .received_requests()
        .await
        .expect("request recording is on");
    let gateway = requests
        .iter()
        .filter(|request| request.headers.contains_key("authorization"))
        .count();
    let external = requests
        .iter()
        .filter(|request| !request.headers.contains_key("authorization"))
        .inspect(|request| {
            assert_eq!(
                request
                    .headers
                    .get("x-review-secret")
                    .and_then(|value| value.to_str().ok()),
                Some(REVIEW_SECRET),
                "external call carries the binding secret"
            );
        })
        .count();
    (gateway, external)
}

/// Deploy `openai/gpt-5-5` for Chat Completions behind a managed provider
/// key, as the administrator `client`.
///
/// # Panics
/// Panics when the credential or deployment is refused.
async fn deploy_gpt_5_5(client: WyrdClient) {
    let credential = serde_json::from_value(json!({
        "name": "openai-key",
        "provider": "openai",
        "source": { "managed_secret": { "secret": "sk-native-upstream" } },
    }))
    .expect("credential decodes");
    CredentialWriter::new(client.clone())
        .put_credential(&credential)
        .await
        .expect("credential puts");
    let deployment = serde_json::from_value(json!({
        "name": "gpt-5-5",
        "model": { "provider": "openai", "model": "gpt-5-5" },
        "adapter": "openai",
        "auth": { "bearer": { "credential": "openai-key" } },
        "capabilities": ["chat_completions"],
        "routing_weight": 1,
    }))
    .expect("deployment decodes");
    Gateway::new(client)
        .put_deployment(&deployment)
        .await
        .expect("deployment puts");
}

/// Check that the code-review example runs from its file through the public
/// Wyrd gateway and through the `review-gateway` binding, that registering
/// it calls no model, and that the registered Workflow runs through the
/// gateway. `admin_key` may read Cards and invoke the gateway.
///
/// # Panics
/// Panics when a run, registration, or call count diverges.
async fn assert_example_routes(server: &WyrdTestServer, admin_key: &str, upstream: &MockServer) {
    deploy_gpt_5_5(client_with(server, admin_key)).await;
    let reviewed = json!({ "review": "hi" });
    let example = repo_file("examples/workflows/code-review/workflow.yaml");
    let public = load_in_child(server, Some(admin_key), &example, None);
    assert_eq!(public["run"]["status"], "succeeded", "{public}");
    assert_eq!(public["run"]["outputs"], reviewed);
    assert_eq!(model_calls(upstream).await, (3, 0));

    let bundle = external_example(upstream);
    let config_home = review_gateway_config(upstream);
    let external = load_in_child(
        server,
        Some(admin_key),
        &bundle.path().join("workflow.yaml"),
        Some(config_home.path()),
    );
    assert_eq!(external["run"]["status"], "succeeded", "{external}");
    assert_eq!(external["run"]["outputs"], reviewed);
    assert_eq!(model_calls(upstream).await, (3, 3));

    let admin = cards_with(server, admin_key);
    Box::pin(admin.register_from_path(&repo_file("examples/workflows/code-review")))
        .await
        .expect("example registers");
    assert_eq!(model_calls(upstream).await, (3, 3));
    let registered = admin
        .workflow()
        .load(&CardSelector::exact(
            serde_json::from_value(json!({
                "kind": "Workflow",
                "name": "code-review",
                "version": "1.0.0",
                "space": "engineering",
            }))
            .expect("example reference decodes"),
        ))
        .await
        .expect("registered example loads");
    assert_succeeded(&run(&registered).await, &reviewed);
    assert_eq!(model_calls(upstream).await, (6, 3));
}

/// Child half of [`load_in_child`]: load the Workflow file named by
/// `WORKFLOW_LOADING_CHILD_PATH` with the ambient configuration, run it when
/// it loads, and print the outcome on one prefixed stdout line.
///
/// # Panics
/// Panics when `WORKFLOW_LOADING_CHILD_PATH` is unset, which means this test
/// was started directly instead of by [`load_in_child`].
#[tokio::test(flavor = "multi_thread")]
#[ignore = "runs only as the child process of workflow_loading_journey"]
async fn authored_load_child() {
    let path = std::env::var(CHILD_PATH).expect("started by load_in_child");
    let outcome = match Workflow::from_path(path).await {
        Err(error) => json!({ "error": error.code() }),
        Ok(workflow) => json!({ "run": run(&workflow).await }),
    };
    println!("{CHILD_OUTCOME}{outcome}");
}

/// Prove the Rust SDK loads and runs local and mixed authored Workflows, and
/// loads and runs an applied Workflow's exact registered version through
/// Cards.
///
/// # Panics
/// Panics when any load, run, pin, or refusal diverges from the contract.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn workflow_loading_journey() {
    // 1. A wholly local Workflow loads and runs without any server.
    let local = Workflow::from_path(fixture("shadowed/local-workflow.yaml"))
        .await
        .expect("local Workflow loads");
    assert_succeeded(
        &run(&local).await,
        &json!({
            "security": "local security review of diff",
            "review": "final review of diff | local security review of diff | local correctness review of diff",
        }),
    );

    let upstream = chat_upstream().await;
    let server = Box::pin(
        WyrdTestServer::builder()
            .with_gateway_provider_root_for_test(
                upstream.uri().parse().expect("upstream URL parses"),
            )
            .start_bound(),
    )
    .await
    .expect("test server starts");
    let writer = cards_with(
        &server,
        &api_key(&server, "workflow_writer", &["writer"]).await,
    );
    let reader_key = api_key(&server, "workflow_reader", &["reader"]).await;
    let reader = cards_with(&server, &reader_key);
    let no_roles_key = api_key(&server, "workflow_no_roles", &[]).await;
    let no_roles = cards_with(&server, &no_roles_key);

    // 2. The team registers its reviewer Agents.
    let mut refs = register(&writer, "team/security.yaml").await;
    refs.extend(register(&writer, "team/correctness.yaml").await);

    // 3. Authored files that reference registered Agents load through the
    //    ambient configuration, which must be able to read them.
    assert_authored_loads(&server, &writer, &reader_key, &no_roles_key).await;

    // 4. Apply the mixed Workflow; it pins each step to the exact registered
    //    Agent, and each Agent to its exact Prompt. A newer security Agent
    //    registers and changes neither.
    refs.extend(register(&writer, "mixed/workflow.yaml").await);
    let workflow_uid = uid(&refs, "code-review");
    let workflow_ref = refs["code-review"].clone();
    register(&writer, "team-v2/security.yaml").await;
    assert_locked_graph(&reader, &refs).await;

    // 5. Load by exact ref and by UID; both run the pinned 1.0.0 Agents, not
    //    the newer one ("v2 security review of diff").
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
        let run = run(&workflow).await;
        assert_succeeded(&run, &json!({ "review": REGISTERED_REVIEW }));
        assert_eq!(
            run.steps["final_review"].text.as_deref(),
            Some(REGISTERED_REVIEW)
        );
        let identity = run.workflow.expect("registered run keeps its Workflow");
        assert_eq!(
            identity.uid.map(|uid| uid.to_string()),
            Some(workflow_uid.clone())
        );
    }

    // 6. Wrong, versionless, mismatched, and unauthorized selectors are refused.
    assert_selectors_refused(
        &reader,
        &no_roles,
        workflow_ref,
        &uid(&refs, "correctness-reviewer"),
    )
    .await;

    // 7. The code-review example runs over the public gateway and an
    //    external gateway binding; registering it runs nothing.
    let admin_key = api_key(&server, "workflow_admin", &["admin"]).await;
    assert_example_routes(&server, &admin_key, &upstream).await;
    server.shutdown().await.expect("test server shuts down");
}
