//! Client→server journeys for composite Workflow registration and pinned
//! registered local execution of the checked-in code-review bundle.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use axum::body::{Body, to_bytes};
use axum::http::{Method, Request, StatusCode, header};
use secrecy::SecretString;
use serde_json::{Value, json};
use skald_providers::ProviderError;
use skald_spec::ProviderResponse;
use skald_spec::wire::openai_chat::OpenAiChatResponse;
use skald_workflow::{
    WorkflowExecutionDependencies, WorkflowRunOptions, WorkflowRunStatus, WyrdGatewayCall,
    WyrdGatewayCaller,
};
use tempfile::TempDir;
use tokio_util::sync::CancellationToken;
use wyrd_client::WyrdClient;
use wyrd_client::auth::AuthMiddleware;
use wyrd_client::cards::{CardSelector, Cards};
use wyrd_client::config::ClientConfig;
use wyrd_client::transport::HttpTransport;
use wyrd_client::transport::config::HttpConfig;
use wyrd_client::transport::credential::ResolvedCredential;
use wyrd_loader::{build_registration_input, load};
use wyrd_spec::card::workflow::WorkflowAction;
use wyrd_spec::envelope::{CardKind, Spec};
use wyrd_spec::error::WyrdError;
use wyrd_spec::ids::{CardUid, DataTenantId};
use wyrd_spec::reference::{CardRef, InlineableRef};
use wyrd_spec::registry::{CardLifecycleStatus, RegistrationReceipt};
use wyrd_sql::queries::cards::get_card_by_uid;
use wyrd_storage::settings::{BackendConfig, StorageSettings};
use wyrd_testing::{Bootstrap, WyrdTestServer};

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
    /// Panics if the request cannot serialize or the static completion
    /// fixture stops decoding, both test-fixture invariants.
    async fn call(
        &self,
        call: WyrdGatewayCall,
        _cancellation: &CancellationToken,
    ) -> Result<ProviderResponse, ProviderError> {
        let request = serde_json::to_string(&call.request).expect("request serializes");
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
}

/// The checked-in code-review bundle directory.
fn bundle() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../examples/workflows/code-review")
}

/// Copy the bundle into a temp directory, applying `card_edit` to each Agent
/// and Prompt file and `workflow_edit` to the Workflow file.
///
/// # Panics
/// Panics when a bundle file cannot be read or written.
fn edited_bundle(
    card_edit: impl Fn(String) -> String,
    workflow_edit: impl Fn(String) -> String,
) -> TempDir {
    let temp = TempDir::new().expect("temp directory creates");
    for dir in ["agents", "prompts"] {
        std::fs::create_dir(temp.path().join(dir)).expect("bundle directory creates");
        for card in ["security", "correctness", "final-reviewer"] {
            let file = format!("{dir}/{card}.yaml");
            let body = std::fs::read_to_string(bundle().join(&file)).expect("card reads");
            std::fs::write(temp.path().join(&file), card_edit(body)).expect("card writes");
        }
    }
    let workflow = std::fs::read_to_string(bundle().join("workflow.yaml")).expect("workflow reads");
    std::fs::write(temp.path().join("workflow.yaml"), workflow_edit(workflow))
        .expect("workflow writes");
    temp
}

/// Replace every path step target with an external Card reference to the
/// registered 1.0.0 reviewer Agents.
fn external_targets(workflow: String) -> String {
    [
        ("security.yaml", "security-reviewer"),
        ("correctness.yaml", "correctness-reviewer"),
        ("final-reviewer.yaml", "final-reviewer"),
    ]
    .into_iter()
    .fold(workflow, |yaml, (file, name)| {
        yaml.replace(
            &format!("        target: ./agents/{file}"),
            &format!(
                "        target:\n          kind: Agent\n          name: {name}\n          version: \"1.0.0\""
            ),
        )
    })
}

/// Insert a step before `correctness` that runs the Agent `name@1.0.0`
/// through an external Card reference with the same `code` binding.
fn add_registered_step(workflow: &str, id: &str, name: &str) -> String {
    workflow.replacen(
        "    - id: correctness",
        &format!(
            "    - id: {id}\n      action:\n        type: agent\n        target:\n          kind: Agent\n          name: {name}\n          version: \"1.0.0\"\n      inputs:\n        code: input.code\n\n    - id: correctness"
        ),
        1,
    )
}

/// Rename the Workflow so each registration attempt has a distinct identity.
fn rename_workflow(workflow: &str, name: &str) -> String {
    workflow.replacen("name: code-review", &format!("name: {name}"), 1)
}

/// Build an exact Card reference in the bundle's `engineering` space from its
/// wire fields.
///
/// # Panics
/// Panics when the fields do not decode as a Card reference, which is a
/// fixture invariant of every caller.
fn card_ref(kind: &str, name: &str, version: &str, uid: Option<&CardUid>) -> CardRef {
    serde_json::from_value(json!({
        "kind": kind,
        "name": name,
        "version": version,
        "space": "engineering",
        "uid": uid,
    }))
    .expect("test card reference decodes")
}

/// Start a bound server with local artifact storage and a writer principal.
///
/// # Panics
/// Panics when the server cannot start or the writer cannot bootstrap.
async fn start_server(storage_root: &Path) -> (WyrdTestServer, String) {
    let server = WyrdTestServer::builder()
        .with_storage_settings(StorageSettings {
            backend: BackendConfig::Local {
                root: storage_root.to_path_buf(),
            },
            require_encryption: false,
            presign_ttl: Duration::from_secs(600),
            part_size_bytes: 16 * 1024 * 1024,
            multipart_threshold_bytes: 100 * 1024 * 1024,
        })
        .start_bound()
        .await
        .expect("bound test server starts");
    let Bootstrap::User { jwt, .. } = server
        .bootstrap_user("workflow-registry-writer", &["writer"])
        .await
        .expect("writer bootstraps")
    else {
        panic!("user bootstrap returned a non-user principal");
    };
    (server, jwt)
}

/// Assemble the production HTTP client used for real registry calls.
///
/// # Panics
/// Panics when the client authentication or transport cannot build.
fn registry_client(server: &WyrdTestServer, jwt: &str) -> WyrdClient {
    let config = ClientConfig {
        http: HttpConfig {
            base_url: server
                .base_url()
                .expect("bound server has a base URL")
                .to_owned(),
            ..HttpConfig::default()
        },
        ..ClientConfig::default()
    };
    let auth = AuthMiddleware::new(
        &config,
        ResolvedCredential::BearerToken(SecretString::from(jwt.to_owned())),
    )
    .expect("client auth builds");
    let transport = HttpTransport::new(&config.http, Arc::clone(&auth)).expect("transport builds");
    WyrdClient::from_parts(auth, transport, config.grpc)
}

/// Return the receipt outcome UID for the Card named `name`.
///
/// # Panics
/// Panics when no outcome names `name` or it carries no UID.
fn outcome_uid(receipt: &RegistrationReceipt, name: &str) -> CardUid {
    receipt
        .outcomes
        .iter()
        .find(|outcome| outcome.card_ref.name.as_str() == name)
        .and_then(|outcome| outcome.card_ref.uid.clone())
        .unwrap_or_else(|| panic!("receipt has a UID for {name}"))
}

/// Run a single tenant-scoped count query binding each text parameter.
///
/// # Panics
/// Panics when the tenant connection or the count fails.
async fn count(server: &WyrdTestServer, sql: &'static str, binds: &[&str]) -> i64 {
    let mut conn = server
        .tenant_conn_for(server.data_tenant_id())
        .await
        .expect("tenant connection opens");
    let value: i64 = binds
        .iter()
        .fold(sqlx::query_scalar(sql), |query, bind| query.bind(*bind))
        .fetch_one(&mut **conn.transaction())
        .await
        .expect("count reads");
    conn.commit().await.expect("count transaction commits");
    value
}

/// Count every registration operation this tenant has recorded.
///
/// # Panics
/// Panics when the tenant count query fails.
async fn operation_count(server: &WyrdTestServer) -> i64 {
    count(
        server,
        "SELECT count(*) FROM wyrd.card_registration_operations",
        &[],
    )
    .await
}

/// Count Card rows named `name`, in any lifecycle state.
///
/// # Panics
/// Panics when the tenant count query fails.
async fn card_count(server: &WyrdTestServer, name: &str) -> i64 {
    count(
        server,
        "SELECT count(*) FROM wyrd.cards WHERE name = $1",
        &[name],
    )
    .await
}

/// Count every principal across tenants, so registration can be shown to
/// create none.
///
/// # Panics
/// Panics when the superuser pool cannot open or the count fails.
async fn principal_count(server: &WyrdTestServer) -> i64 {
    let pool = server
        .pg_fixture()
        .superuser_pool()
        .await
        .expect("superuser pool opens");
    sqlx::query_scalar("SELECT count(*) FROM platform.principals")
        .fetch_one(&pool)
        .await
        .expect("principal count reads")
}

/// One invalidating edit applied to a submitted Workflow spec body.
type SpecMutation = fn(&mut Value);

/// Submit the loaded bundle under `dir` straight to the registration route
/// with `mutate` applied to its Workflow spec, returning the problem body.
///
/// The offline loader already refuses pure Workflow contract violations, so
/// this bypasses it to prove the server enforces every rule on its own.
///
/// # Panics
/// Panics when the bundle cannot load, the request cannot be sent, or the
/// server does not answer `422 Unprocessable Entity`.
async fn refused_registration(
    server: &WyrdTestServer,
    jwt: &str,
    dir: &Path,
    mutate: SpecMutation,
) -> Value {
    let input = build_registration_input(load(&dir.join("workflow.yaml")).expect("bundle loads"))
        .expect("registration input builds");
    let mut body =
        serde_json::to_value(input.to_create_card_request()).expect("request serializes");
    let workflow = body["submissions"]
        .as_array_mut()
        .expect("submissions are an array")
        .iter_mut()
        .find(|submission| submission["kind"] == "Workflow")
        .expect("the request submits the Workflow");
    mutate(&mut workflow["spec"]);
    let request = Request::builder()
        .method(Method::POST)
        .uri("/v1/cards")
        .header(header::CONTENT_TYPE, "application/json")
        .header("Idempotency-Key", uuid::Uuid::now_v7().to_string())
        .body(Body::from(body.to_string()))
        .expect("registration request builds");
    let response = server
        .oneshot_authenticated(jwt, request)
        .await
        .expect("registration responds");
    let status = response.status();
    let bytes = to_bytes(response.into_body(), 1 << 20)
        .await
        .expect("response body reads");
    let problem: Value = serde_json::from_slice(&bytes).expect("response body is JSON");
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{problem}");
    problem
}

#[tokio::test(flavor = "current_thread")]
/// Composite registration stores the checked-in bundle's exact Workflow,
/// Agent, and Prompt versions with UID-bearing Workflow-to-Agent and
/// Agent-to-Prompt refs and relationships, accepts Native and built-in tool
/// declarations, and refuses invalid bindings, outputs, and routes
/// identically for sibling and external dependencies without any durable
/// write. An external ref sharing a sibling's identity is validated against
/// its own registered body.
///
/// # Panics
/// Panics when any registration outcome, stored ref, relationship row, or
/// refusal diverges from the asserted contract.
async fn registers_only_valid_explicit_workflow_graphs() {
    let storage_root = tempfile::tempdir().expect("storage root creates");
    let (server, jwt) = start_server(storage_root.path()).await;
    let cards = Cards::with_client(registry_client(&server, &jwt));

    let principals = principal_count(&server).await;
    let receipt = cards
        .register_from_path(&bundle().join("workflow.yaml"))
        .await
        .expect("code-review bundle registers");
    assert_eq!(
        principal_count(&server).await,
        principals,
        "registering a Workflow creates no principal"
    );
    assert_eq!(receipt.outcomes.len(), 7);
    assert!(
        receipt
            .outcomes
            .iter()
            .all(|outcome| outcome.status == CardLifecycleStatus::Active
                && outcome.card_ref.version.to_string() == "1.0.0")
    );
    let workflow_uid = outcome_uid(&receipt, "code-review");
    let agent_uids: Vec<CardUid> = [
        "security-reviewer",
        "correctness-reviewer",
        "final-reviewer",
    ]
    .into_iter()
    .map(|name| outcome_uid(&receipt, name))
    .collect();

    let mut conn = server
        .tenant_conn_for(server.data_tenant_id())
        .await
        .expect("tenant connection opens");
    let stored = get_card_by_uid(&mut conn, &workflow_uid)
        .await
        .expect("registered Workflow loads");
    let Spec::Workflow(workflow) = stored.spec else {
        panic!("registered Card is not a Workflow");
    };
    let locked: Vec<CardRef> = workflow
        .steps
        .iter()
        .map(|step| {
            let WorkflowAction::Agent(target) = &step.action;
            let InlineableRef::Ref(target) = target else {
                panic!("registration must lock every step target to a UID-bearing ref");
            };
            target.clone()
        })
        .collect();
    assert_eq!(
        locked
            .iter()
            .map(|target| target.uid.clone().expect("locked UID"))
            .collect::<Vec<_>>(),
        agent_uids
    );
    assert!(
        locked
            .iter()
            .all(|target| target.version.to_string() == "1.0.0")
    );
    let relationships = |uid: &CardUid| {
        sqlx::query_as::<_, (String, String, String, uuid::Uuid)>(
            "SELECT target_kind, target_name, target_version, target_uid \
             FROM wyrd.card_relationships WHERE card_uid = $1 ORDER BY target_name",
        )
        .bind(uid.as_uuid())
    };
    let workflow_relationships = relationships(&workflow_uid)
        .fetch_all(&mut **conn.transaction())
        .await
        .expect("Workflow relationships read");
    assert_eq!(workflow_relationships.len(), 3);
    for (kind, name, version, uid) in &workflow_relationships {
        assert_eq!(kind, "Agent");
        assert_eq!(version, "1.0.0");
        assert_eq!(uid, &outcome_uid(&receipt, name).as_uuid());
    }
    for (agent, prompt) in [
        ("security-reviewer", "security-review-prompt"),
        ("correctness-reviewer", "correctness-review-prompt"),
        ("final-reviewer", "final-review-prompt"),
    ] {
        let agent_uid = outcome_uid(&receipt, agent);
        let prompt_uid = outcome_uid(&receipt, prompt);
        let stored = get_card_by_uid(&mut conn, &agent_uid)
            .await
            .expect("registered Agent loads");
        let Spec::Agent(spec) = stored.spec else {
            panic!("{agent} is not an Agent");
        };
        let InlineableRef::Ref(locked) = spec.prompt else {
            panic!("{agent} must lock its Prompt to a UID-bearing ref");
        };
        assert_eq!(locked.name.as_str(), prompt);
        assert_eq!(locked.version.to_string(), "1.0.0");
        assert_eq!(locked.uid, Some(prompt_uid.clone()));
        let rows = relationships(&agent_uid)
            .fetch_all(&mut **conn.transaction())
            .await
            .expect("Agent relationships read");
        assert_eq!(
            rows,
            vec![(
                "Prompt".to_owned(),
                prompt.to_owned(),
                "1.0.0".to_owned(),
                prompt_uid.as_uuid()
            )],
            "{agent}"
        );
    }
    conn.commit().await.expect("assertion transaction commits");

    let tooling = tempfile::tempdir().expect("tooling bundle creates");
    std::fs::write(
        tooling.path().join("prompt.yaml"),
        "apiVersion: wyrd/v1\nkind: Prompt\nmetadata:\n  space: engineering\n  name: tooling-prompt\n  version: \"1.0.0\"\nspec:\n  model: gpt-5-5\n  request:\n    model: gpt-5-5\n    messages:\n      - role: user\n        content: \"Summarize {{code}}\"\n  variables: [code]\n  response_type: text\n",
    )
    .expect("prompt writes");
    std::fs::write(
        tooling.path().join("agent.yaml"),
        "apiVersion: wyrd/v1\nkind: Agent\nmetadata:\n  space: engineering\n  name: tooling-agent\n  version: \"1.0.0\"\nspec:\n  prompt: ./prompt.yaml\n  tool_names: [bifrost.query, cards.get]\n  run_config:\n    max_iterations: 2\n",
    )
    .expect("agent writes");
    std::fs::write(
        tooling.path().join("workflow.yaml"),
        "apiVersion: wyrd/v1\nkind: Workflow\nmetadata:\n  space: engineering\n  name: tooling-review\n  version: \"1.0.0\"\nspec:\n  llm_route:\n    kind: wyrd_gateway\n  inputs:\n    code:\n      type: str\n      value: \"\"\n  steps:\n    - id: summarize\n      action:\n        type: agent\n        target: ./agent.yaml\n      inputs:\n        code: input.code\n  outputs:\n    summary: steps.summarize.output.text\n",
    )
    .expect("workflow writes");
    let receipt = cards
        .register_from_path(&tooling.path().join("workflow.yaml"))
        .await
        .expect("Prompt-ref and built-in tool graph registers");
    assert_eq!(receipt.outcomes.len(), 3);
    let tooling_agent_uid = outcome_uid(&receipt, "tooling-agent");
    let prompt_uid = outcome_uid(&receipt, "tooling-prompt");
    let mut conn = server
        .tenant_conn_for(server.data_tenant_id())
        .await
        .expect("tenant connection opens");
    let stored = get_card_by_uid(&mut conn, &tooling_agent_uid)
        .await
        .expect("registered Agent loads");
    conn.commit().await.expect("assertion transaction commits");
    let Spec::Agent(agent) = stored.spec else {
        panic!("registered Card is not an Agent");
    };
    let InlineableRef::Ref(prompt) = agent.prompt else {
        panic!("registration must lock the Prompt to a UID-bearing ref");
    };
    assert_eq!(prompt.uid, Some(prompt_uid));
    assert_eq!(agent.tool_names, ["bifrost.query", "cards.get"]);

    let extra_binding = |yaml: String| {
        yaml.replacen(
            "        code: input.code\n      timeout_seconds",
            "        code: input.code\n        extra: input.code\n      timeout_seconds",
            1,
        )
    };
    let operations = operation_count(&server).await;
    let client_refused = edited_bundle(
        |agent| agent,
        |yaml| rename_workflow(&extra_binding(external_targets(yaml)), "client-refused"),
    );
    let error = cards
        .register_from_path(&client_refused.path().join("workflow.yaml"))
        .await
        .expect_err("the server refuses an invalid resolved binding");
    assert_eq!(error.code(), "WYRD_WORKFLOW_422_VALIDATION");
    assert!(
        error.to_string().contains("steps[0].inputs.extra"),
        "{error}"
    );
    assert_eq!(operation_count(&server).await, operations);
    assert_eq!(card_count(&server, "client-refused").await, 0);

    let invalid: [(&str, SpecMutation); 4] = [
        ("binding", |spec| {
            spec["steps"][0]["inputs"]["extra"] = json!("input.code");
        }),
        ("output", |spec| {
            spec["outputs"]["review"] = json!("steps.final_review.output.structured.verdict");
        }),
        ("graph", |spec| {
            spec["steps"][0]["depends_on"] = json!(["final_review"]);
        }),
        ("route", |spec| {
            spec["llm_route"] = json!({
                "kind": "ext_gateway",
                "protocol": "anthropic_messages",
                "base_url": "https://gateway.example.com",
                "credential_binding": "review-gateway",
            });
        }),
    ];
    for (label, mutate) in invalid {
        let operations = operation_count(&server).await;
        let sibling_name = format!("invalid-{label}-sibling");
        let agent_prefix = format!("sibling-{label}-");
        let sibling = edited_bundle(
            |agent| {
                agent
                    .replace(
                        "name: security-reviewer",
                        &format!("name: {agent_prefix}security"),
                    )
                    .replace(
                        "name: correctness-reviewer",
                        &format!("name: {agent_prefix}correctness"),
                    )
                    .replace(
                        "name: final-reviewer",
                        &format!("name: {agent_prefix}final"),
                    )
            },
            |yaml| rename_workflow(&yaml, &sibling_name),
        );
        let sibling_code =
            refused_registration(&server, &jwt, sibling.path(), mutate).await["code"].clone();

        let external_name = format!("invalid-{label}-external");
        let external = edited_bundle(
            |agent| agent,
            |yaml| rename_workflow(&external_targets(yaml), &external_name),
        );
        let external_code =
            refused_registration(&server, &jwt, external.path(), mutate).await["code"].clone();

        assert_eq!(
            sibling_code, external_code,
            "{label}: sibling and external dependencies fail identically"
        );
        assert!(
            sibling_code
                .as_str()
                .is_some_and(|code| code.starts_with("WYRD_WORKFLOW_422_")),
            "{label}: {sibling_code}"
        );
        assert_eq!(operation_count(&server).await, operations, "{label}");
        for name in [
            sibling_name.clone(),
            external_name,
            format!("{agent_prefix}security"),
            format!("{agent_prefix}correctness"),
            format!("{agent_prefix}final"),
        ] {
            assert_eq!(card_count(&server, &name).await, 0, "{label}: {name}");
        }
    }

    let registered = tempfile::tempdir().expect("registered Agent workspace creates");
    std::fs::write(
        registered.path().join("agent.yaml"),
        "apiVersion: wyrd/v1\nkind: Agent\nmetadata:\n  space: engineering\n  name: collision-reviewer\n  version: \"1.0.0\"\nspec:\n  prompt:\n    model: gpt-5-5\n    request:\n      model: gpt-5-5\n      messages:\n        - role: user\n          content: \"Inspect {{diff}}\"\n    variables: [diff]\n    response_type: text\n",
    )
    .expect("registered Agent writes");
    cards
        .register_from_path(&registered.path().join("agent.yaml"))
        .await
        .expect("incompatible registered Agent registers");
    let operations = operation_count(&server).await;
    let collision = edited_bundle(
        |card| card.replace("name: security-reviewer", "name: collision-reviewer"),
        |yaml| {
            rename_workflow(
                &add_registered_step(&yaml, "registered_collision", "collision-reviewer"),
                "collision-review",
            )
        },
    );
    let error = cards
        .register_from_path(&collision.path().join("workflow.yaml"))
        .await
        .expect_err("the external slot validates its own registered body");
    assert_eq!(error.code(), "WYRD_WORKFLOW_422_VALIDATION", "{error}");
    assert!(error.to_string().contains("steps[1]"), "{error}");
    assert_eq!(operation_count(&server).await, operations);
    assert_eq!(card_count(&server, "collision-review").await, 0);
    assert_eq!(card_count(&server, "collision-reviewer").await, 1);

    // A fresh Workflow whose version is omitted or scoped registers at the
    // version the server allocates and reloads at that exact version, while
    // an invalid binding under either intent is still refused unwritten.
    for (name, version_line, allocated) in [
        ("unversioned-review", "", "0.1.0"),
        ("scoped-review", "  version: \"2\"\n", "2.0.0"),
    ] {
        let dir = tempfile::tempdir().expect("version-intent workspace creates");
        std::fs::write(
            dir.path().join("workflow.yaml"),
            inline_workflow(name, version_line),
        )
        .expect("workflow writes");
        let operations = operation_count(&server).await;
        let problem = refused_registration(&server, &jwt, dir.path(), |spec| {
            spec["steps"][0]["inputs"]["extra"] = json!("input.code");
        })
        .await;
        assert_eq!(problem["code"], "WYRD_WORKFLOW_422_VALIDATION", "{name}");
        assert_eq!(
            problem["details"]["field"], "steps[0].inputs.extra",
            "{problem}"
        );
        assert_eq!(operation_count(&server).await, operations, "{name}");
        assert_eq!(card_count(&server, name).await, 0, "{name}");

        let receipt = cards
            .register_from_path(&dir.path().join("workflow.yaml"))
            .await
            .expect("version-intent Workflow registers");
        assert_eq!(receipt.root.version.to_string(), allocated, "{name}");
        let workflow = cards
            .workflow()
            .load(&CardSelector::exact(receipt.root.clone()))
            .await
            .expect("allocated Workflow version reloads");
        assert_eq!(workflow.as_skald().step_ids(), vec!["summarize"], "{name}");
    }
    server.shutdown().await.expect("test server shuts down");
}

/// A singleton Workflow named `name` whose one step runs an inline Agent with
/// an inline Prompt, so the request has no dependency to resolve.
/// `version_line` is the authored `metadata.version` line, or empty to omit it.
fn inline_workflow(name: &str, version_line: &str) -> String {
    format!(
        "apiVersion: wyrd/v1\nkind: Workflow\nmetadata:\n  space: engineering\n  name: {name}\n{version_line}spec:\n  llm_route:\n    kind: wyrd_gateway\n  inputs:\n    code:\n      type: str\n      value: \"\"\n  steps:\n    - id: summarize\n      action:\n        type: agent\n        target:\n          prompt:\n            model: gpt-5-5\n            request:\n              model: gpt-5-5\n              messages:\n                - role: user\n                  content: \"Summarize {{{{code}}}}\"\n            variables: [code]\n            response_type: text\n          tool_names: []\n          run_config:\n            max_iterations: 1\n      inputs:\n        code: input.code\n  outputs:\n    summary: steps.summarize.output.text\n"
    )
}

#[tokio::test(flavor = "current_thread")]
/// A registered Workflow is loaded through the Cards Workflow view at its
/// exact locked versions and executed locally; newer Agent versions never
/// float in, and missing, foreign, mismatched, non-exact, wrong-kind, and
/// inactive references are refused before any dispatch.
///
/// # Panics
/// Panics when registration, loading, execution, or any refusal diverges
/// from the asserted contract.
async fn fetches_and_executes_locked_workflow_graph() {
    let storage_root = tempfile::tempdir().expect("storage root creates");
    let (server, jwt) = start_server(storage_root.path()).await;
    let cards = Cards::with_client(registry_client(&server, &jwt));
    let receipt = cards
        .register_from_path(&bundle().join("workflow.yaml"))
        .await
        .expect("code-review bundle registers");
    let workflow_uid = outcome_uid(&receipt, "code-review");
    let security_uid = outcome_uid(&receipt, "security-reviewer");

    // The newer Agent sits at the workspace root so its Prompt path stays
    // inside the loader's workspace.
    let newer = tempfile::tempdir().expect("newer Agent workspace creates");
    std::fs::create_dir(newer.path().join("prompts")).expect("newer directory creates");
    for (source, target) in [
        ("agents/security.yaml", "security.yaml"),
        ("prompts/security.yaml", "prompts/security.yaml"),
    ] {
        let body = std::fs::read_to_string(bundle().join(source))
            .expect("security Card reads")
            .replacen("version: \"1.0.0\"", "version: \"2.0.0\"", 1)
            .replacen("You are a security reviewer.", "You are a v2 auditor.", 1)
            .replacen("../prompts/", "prompts/", 1);
        std::fs::write(newer.path().join(target), body).expect("newer Card writes");
    }
    cards
        .register_from_path(&newer.path().join("security.yaml"))
        .await
        .expect("newer security Agent and Prompt register");

    let workflows = cards.workflow();
    let registered = CardSelector::exact(card_ref("Workflow", "code-review", "1.0.0", None));
    let workflow = workflows
        .load(&registered)
        .await
        .expect("registered graph loads")
        .into_skald();
    assert_eq!(
        workflow.step_ids(),
        vec!["security", "correctness", "final_review"]
    );

    let gateway = Arc::new(ReviewGateway::default());
    let dependencies = WorkflowExecutionDependencies::new(skald_runtime::ProviderRegistry::new())
        .with_wyrd_gateway(Arc::clone(&gateway) as Arc<dyn WyrdGatewayCaller>);
    let input = || {
        serde_json::Map::from_iter([(
            "code".to_owned(),
            Value::from("diff --git a/src/auth.rs b/src/auth.rs"),
        )])
    };
    let run = workflow
        .run_with_options(&dependencies, input(), WorkflowRunOptions::default())
        .await
        .expect("registered run starts");
    assert_eq!(run.status, WorkflowRunStatus::Succeeded);
    assert_eq!(run.outputs["review"], json!("FINAL-REVIEW"));
    let identity = run.workflow.expect("registered run keeps its Workflow");
    assert_eq!(identity.kind, CardKind::Workflow);
    assert_eq!(identity.name.as_str(), "code-review");
    assert_eq!(identity.version.to_string(), "1.0.0");
    assert_eq!(identity.uid, Some(workflow_uid.clone()));
    let requests = gateway.requests();
    assert_eq!(requests.len(), 3);
    assert!(
        requests
            .iter()
            .all(|request| !request.contains("v2 auditor")),
        "a newer Agent version must not float into the pinned graph"
    );
    let last = requests.last().expect("final request recorded");
    assert!(last.contains("SECURITY-FINDINGS") && last.contains("CORRECTNESS-FINDINGS"));

    let refusals = [
        (
            card_ref("Workflow", "code-review", "9.9.9", None),
            "WYRD_REGISTRY_404_CARD_NOT_FOUND",
        ),
        (
            card_ref("Workflow", "code-review", "1.0.0", Some(&security_uid)),
            "WYRD_REGISTRY_400_CARD_REF_UID_NOT_RESOLVABLE_HERE",
        ),
        (
            card_ref("Agent", "security-reviewer", "1.0.0", None),
            "WYRD_WORKFLOW_400_INVALID_CARD_REF",
        ),
    ];
    let ranged = json!({
        "kind": "Workflow",
        "name": "code-review",
        "version": "^1.0.0",
        "space": "engineering",
    });
    assert!(
        serde_json::from_value::<CardRef>(ranged).is_err(),
        "a version range cannot become an exact Workflow reference"
    );
    for (reference, code) in refusals {
        let error: WyrdError = workflows
            .load(&CardSelector::exact(reference.clone()))
            .await
            .expect_err("refused reference");
        assert_eq!(error.code(), code, "{reference}: {error}");
    }

    let pool = server
        .pg_fixture()
        .superuser_pool()
        .await
        .expect("superuser pool opens");
    let foreign_tenant = DataTenantId::new_v7();
    sqlx::query(
        "INSERT INTO platform.tenants \
            (data_tenant_id, slug, display_name, status) VALUES ($1, $2, $3, 'active')",
    )
    .bind(foreign_tenant.as_uuid())
    .bind(format!("foreign-{}", foreign_tenant.as_uuid()))
    .bind("Foreign Workflow tenant")
    .execute(&pool)
    .await
    .expect("foreign tenant inserts");
    sqlx::query(
        "INSERT INTO wyrd.cards \
            (card_uid, data_tenant_id, kind, space, name, version, spec, spec_hash, status) \
         SELECT $1, $2, kind, space, 'foreign-review', version, spec, spec_hash, 'active' \
         FROM wyrd.cards WHERE card_uid = $3",
    )
    .bind(uuid::Uuid::now_v7())
    .bind(foreign_tenant.as_uuid())
    .bind(workflow_uid.as_uuid())
    .execute(&pool)
    .await
    .expect("foreign Workflow inserts");
    let error = workflows
        .load(&CardSelector::exact(card_ref(
            "Workflow",
            "foreign-review",
            "1.0.0",
            None,
        )))
        .await
        .expect_err("a foreign tenant's Workflow is invisible");
    assert_eq!(error.code(), "WYRD_REGISTRY_404_CARD_NOT_FOUND");

    for status in ["pending", "deleted"] {
        sqlx::query("UPDATE wyrd.cards SET status = $1 WHERE card_uid = $2")
            .bind(status)
            .bind(security_uid.as_uuid())
            .execute(&pool)
            .await
            .expect("security Agent deactivates");
        let error = workflows
            .load(&registered)
            .await
            .expect_err("an inactive locked dependency is refused");
        let expected = if status == "pending" {
            "WYRD_REGISTRY_422_UNRESOLVED_DEPENDENCY"
        } else {
            "WYRD_REGISTRY_404_CARD_NOT_FOUND"
        };
        assert_eq!(error.code(), expected, "{status}: {error}");
    }
    assert_eq!(
        gateway.requests().len(),
        3,
        "refused loads dispatch nothing"
    );

    server.shutdown().await.expect("test server shuts down");
}

#[tokio::test(flavor = "current_thread")]
/// A registration whose preflight validated Agent UID A is refused when UID B
/// replaces A at the same identity before the write transaction rechecks it:
/// nothing commits, and a fresh request validates B on its own.
///
/// The test holds the tenant's audit chain-head row lock, which the write
/// transaction takes before its dependency recheck, so the replacement lands
/// strictly after preflight committed and before the write-time recheck.
///
/// # Panics
/// Panics when the interleaving cannot be established, the stale plan is not
/// refused with `WYRD_REGISTRY_422_UNRESOLVED_DEPENDENCY`, anything durable
/// commits, or the fresh request is not refused by Workflow validation.
async fn refuses_stale_preflight_after_dependency_replacement() {
    let storage_root = tempfile::tempdir().expect("storage root creates");
    let (server, jwt) = start_server(storage_root.path()).await;
    let cards = Cards::with_client(registry_client(&server, &jwt));
    let receipt = cards
        .register_from_path(&bundle().join("workflow.yaml"))
        .await
        .expect("code-review bundle registers");
    let validated_uid = outcome_uid(&receipt, "security-reviewer");
    let stale = edited_bundle(
        |card| card,
        |yaml| rename_workflow(&external_targets(yaml), "stale-plan"),
    );
    let operations = operation_count(&server).await;

    let pool = server
        .pg_fixture()
        .superuser_pool()
        .await
        .expect("superuser pool opens");
    let mut replacement = pool.begin().await.expect("replacement transaction begins");
    sqlx::query("SELECT 1 FROM vala.audit_chain_head WHERE data_tenant_id = $1 FOR UPDATE")
        .bind(server.data_tenant_id().as_uuid())
        .fetch_one(&mut *replacement)
        .await
        .expect("audit chain head locks");

    let stale_path = stale.path().join("workflow.yaml");
    let stale_cards = cards.clone();
    let registration =
        tokio::spawn(async move { stale_cards.register_from_path(&stale_path).await });
    tokio::time::timeout(Duration::from_secs(30), async {
        loop {
            let waiting: i64 = sqlx::query_scalar(
                "SELECT count(*) FROM pg_stat_activity \
                 WHERE wait_event_type = 'Lock' AND query LIKE '%FROM vala.audit_chain_head%'",
            )
            .fetch_one(&pool)
            .await
            .expect("lock waiters read");
            if waiting > 0 {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("the registration write waits behind the audit chain head");

    let replacement_uid = uuid::Uuid::now_v7();
    sqlx::query("UPDATE wyrd.cards SET status = 'deleted' WHERE card_uid = $1")
        .bind(validated_uid.as_uuid())
        .execute(&mut *replacement)
        .await
        .expect("validated Agent deletes");
    sqlx::query(
        "INSERT INTO wyrd.cards \
            (card_uid, data_tenant_id, kind, space, name, version, spec, spec_hash, status) \
         SELECT $1, data_tenant_id, kind, space, name, version, \
                jsonb_set(spec, '{prompt}', $2::jsonb), 'replacement-spec-hash', 'active' \
         FROM wyrd.cards WHERE card_uid = $3",
    )
    .bind(replacement_uid)
    .bind(json!({
        "model": "gpt-5-5",
        "request": {
            "model": "gpt-5-5",
            "messages": [{ "role": "user", "content": "Inspect {{diff}}" }]
        },
        "variables": ["diff"],
        "response_type": "text"
    }))
    .bind(validated_uid.as_uuid())
    .execute(&mut *replacement)
    .await
    .expect("replacement Agent activates at the same identity");
    replacement
        .commit()
        .await
        .expect("replacement commits and releases the lock");

    let error = registration
        .await
        .expect("registration task joins")
        .expect_err("the stale preflight plan is refused");
    assert_eq!(
        error.code(),
        "WYRD_REGISTRY_422_UNRESOLVED_DEPENDENCY",
        "{error}"
    );
    assert_eq!(operation_count(&server).await, operations);
    assert_eq!(card_count(&server, "stale-plan").await, 0);
    let inbound: i64 =
        sqlx::query_scalar("SELECT count(*) FROM wyrd.card_relationships WHERE target_uid = $1")
            .bind(replacement_uid)
            .fetch_one(&pool)
            .await
            .expect("replacement relationships read");
    assert_eq!(inbound, 0);

    let error = cards
        .register_from_path(&stale.path().join("workflow.yaml"))
        .await
        .expect_err("a fresh request validates the replacement body");
    assert_eq!(error.code(), "WYRD_WORKFLOW_422_VALIDATION", "{error}");
    assert_eq!(operation_count(&server).await, operations);
    server.shutdown().await.expect("test server shuts down");
}
