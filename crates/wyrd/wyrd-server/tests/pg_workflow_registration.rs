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
use skald_tool::ToolRegistry;
use skald_workflow::{
    WorkflowExecutionDependencies, WorkflowRunOptions, WorkflowRunStatus, WyrdGatewayCall,
    WyrdGatewayCaller,
};
use tokio_util::sync::CancellationToken;
use wyrd_client::auth::AuthMiddleware;
use wyrd_client::cards::Cards;
use wyrd_client::config::ClientConfig;
use wyrd_client::transport::HttpTransport;
use wyrd_client::transport::config::HttpConfig;
use wyrd_client::transport::credential::ResolvedCredential;
use wyrd_client::{WorkflowLoader, WyrdClient};
use wyrd_loader::{build_registration_input, load};
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

/// Copy the bundle into a temp directory, applying `agent_edit` to each Agent
/// file and `workflow_edit` to the Workflow file.
///
/// # Panics
/// Panics when a bundle file cannot be read or written.
fn edited_bundle(
    agent_edit: impl Fn(String) -> String,
    workflow_edit: impl Fn(String) -> String,
) -> tempfile::TempDir {
    let temp = tempfile::TempDir::new().expect("temp directory creates");
    std::fs::create_dir(temp.path().join("agents")).expect("agents directory creates");
    for agent in ["security", "correctness", "final-reviewer"] {
        let file = format!("agents/{agent}.yaml");
        let body = std::fs::read_to_string(bundle().join(&file)).expect("agent reads");
        std::fs::write(temp.path().join(&file), agent_edit(body)).expect("agent writes");
    }
    let workflow = std::fs::read_to_string(bundle().join("workflow.yaml")).expect("workflow reads");
    std::fs::write(temp.path().join("workflow.yaml"), workflow_edit(workflow))
        .expect("workflow writes");
    temp
}

/// Replace every `path:` step target with an external `ref:` to the
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
            &format!("          path: ./agents/{file}"),
            &format!(
                "          ref:\n            kind: Agent\n            name: {name}\n            version: \"1.0.0\""
            ),
        )
    })
}

/// Rename the Workflow so each registration attempt has a distinct identity.
fn rename_workflow(workflow: &str, name: &str) -> String {
    workflow.replacen("name: code-review", &format!("name: {name}"), 1)
}

/// Build an exact Card reference from its wire fields.
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
async fn operation_count(server: &WyrdTestServer) -> i64 {
    count(
        server,
        "SELECT count(*) FROM wyrd.card_registration_operations",
        &[],
    )
    .await
}

/// Count Card rows named `name`.
async fn card_count(server: &WyrdTestServer, name: &str) -> i64 {
    count(
        server,
        "SELECT count(*) FROM wyrd.cards WHERE name = $1",
        &[name],
    )
    .await
}

/// One invalidating edit applied to a submitted Workflow spec body.
type SpecMutation = fn(&mut Value);

/// Submit the loaded bundle under `dir` straight to the registration route
/// with `mutate` applied to its Workflow spec, returning the refusal code.
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
) -> String {
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
    problem["code"].as_str().expect("problem code").to_owned()
}

#[tokio::test(flavor = "current_thread")]
/// Composite registration stores exact Workflow relationships and locked
/// Agent/Prompt refs, accepts Native and built-in tool declarations, and
/// refuses invalid bindings, outputs, and routes identically for sibling and
/// external dependencies without any durable write.
async fn registers_only_valid_explicit_workflow_graphs() {
    let storage_root = tempfile::tempdir().expect("storage root creates");
    let (server, jwt) = start_server(storage_root.path()).await;
    let cards = Cards::with_client(registry_client(&server, &jwt));

    let receipt = cards
        .register_from_path(&bundle().join("workflow.yaml"))
        .await
        .expect("code-review bundle registers");
    assert_eq!(receipt.outcomes.len(), 4);
    assert!(
        receipt
            .outcomes
            .iter()
            .all(|outcome| outcome.status == CardLifecycleStatus::Active)
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
            let wyrd_spec::card::workflow::WorkflowAction::Agent(target) = &step.action;
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
    let relationships: Vec<(String, String, String, uuid::Uuid)> = sqlx::query_as(
        "SELECT target_kind, target_name, target_version, target_uid \
         FROM wyrd.card_relationships WHERE card_uid = $1 ORDER BY target_name",
    )
    .bind(workflow_uid.as_uuid())
    .fetch_all(&mut **conn.transaction())
    .await
    .expect("Workflow relationships read");
    conn.commit().await.expect("assertion transaction commits");
    assert_eq!(relationships.len(), 3);
    for (kind, name, version, uid) in &relationships {
        assert_eq!(kind, "Agent");
        assert_eq!(version, "1.0.0");
        let expected = outcome_uid(&receipt, name);
        assert_eq!(uid, &expected.as_uuid());
    }

    let tooling = tempfile::tempdir().expect("tooling bundle creates");
    std::fs::write(
        tooling.path().join("prompt.yaml"),
        "apiVersion: wyrd/v1\nkind: Prompt\nmetadata:\n  space: engineering\n  name: tooling-prompt\n  version: \"1.0.0\"\nspec:\n  model: gpt-5-5\n  request:\n    model: gpt-5-5\n    messages:\n      - role: user\n        content: \"Summarize {{code}}\"\n  variables: [code]\n  response_type: text\n",
    )
    .expect("prompt writes");
    std::fs::write(
        tooling.path().join("agent.yaml"),
        "apiVersion: wyrd/v1\nkind: Agent\nmetadata:\n  space: engineering\n  name: tooling-agent\n  version: \"1.0.0\"\nspec:\n  prompt:\n    path: ./prompt.yaml\n  tool_names: [bifrost.query, cards.get]\n  run_config:\n    max_iterations: 2\n",
    )
    .expect("agent writes");
    std::fs::write(
        tooling.path().join("workflow.yaml"),
        "apiVersion: wyrd/v1\nkind: Workflow\nmetadata:\n  space: engineering\n  name: tooling-review\n  version: \"1.0.0\"\nspec:\n  llm_route:\n    kind: wyrd_gateway\n  inputs:\n    code:\n      type: str\n      value: \"\"\n  steps:\n    - id: summarize\n      action:\n        type: agent\n        target:\n          path: ./agent.yaml\n      inputs:\n        code: input.code\n  outputs:\n    summary: steps.summarize.output.text\n",
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
        let sibling_code = refused_registration(&server, &jwt, sibling.path(), mutate).await;

        let external_name = format!("invalid-{label}-external");
        let external = edited_bundle(
            |agent| agent,
            |yaml| rename_workflow(&external_targets(yaml), &external_name),
        );
        let external_code = refused_registration(&server, &jwt, external.path(), mutate).await;

        assert_eq!(
            sibling_code, external_code,
            "{label}: sibling and external dependencies fail identically"
        );
        assert!(
            sibling_code.starts_with("WYRD_WORKFLOW_422_"),
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
    server.shutdown().await.expect("test server shuts down");
}

#[tokio::test(flavor = "current_thread")]
/// A registered Workflow is fetched over HTTP at its exact locked versions and
/// executed locally; newer Agent versions never float in, and missing,
/// foreign, mismatched, non-exact, and inactive references are refused.
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

    let newer = tempfile::tempdir().expect("newer Agent workspace creates");
    let security = std::fs::read_to_string(bundle().join("agents/security.yaml"))
        .expect("security Agent reads")
        .replacen("version: \"1.0.0\"", "version: \"2.0.0\"", 1)
        .replacen("You are a security reviewer.", "You are a v2 auditor.", 1);
    std::fs::write(newer.path().join("security.yaml"), security).expect("newer Agent writes");
    cards
        .register_from_path(&newer.path().join("security.yaml"))
        .await
        .expect("newer security Agent registers");

    let loader = WorkflowLoader::new(Arc::new(ToolRegistry::new()))
        .with_client(registry_client(&server, &jwt));
    let registered = card_ref("Workflow", "code-review", "1.0.0", None);
    let workflow = loader
        .load_registered(&registered)
        .await
        .expect("registered graph loads");
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

    let authored = edited_bundle(|agent| agent, external_targets);
    let local = loader
        .load_file(&authored.path().join("workflow.yaml"))
        .await
        .expect("authored external refs resolve through exact reads");
    let local_run = local
        .run_with_options(&dependencies, input(), WorkflowRunOptions::default())
        .await
        .expect("authored run starts");
    assert_eq!(local_run.status, WorkflowRunStatus::Succeeded);
    assert_eq!(local_run.outputs, run.outputs);
    assert_eq!(
        local_run.steps.keys().collect::<Vec<_>>(),
        run.steps.keys().collect::<Vec<_>>()
    );
    assert!(local_run.workflow.is_none());
    assert_eq!(gateway.requests().len(), 6);

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
            card_ref("Workflow", "code-review", "^1.0.0", None),
            "WYRD_REGISTRY_400_INVALID_CARD_SPEC",
        ),
        (
            card_ref("Agent", "security-reviewer", "1.0.0", None),
            "WYRD_REGISTRY_400_INVALID_CARD_SPEC",
        ),
    ];
    for (reference, code) in refusals {
        let error: WyrdError = loader
            .load_registered(&reference)
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
    let error = loader
        .load_registered(&card_ref("Workflow", "foreign-review", "1.0.0", None))
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
        let error = loader
            .load_registered(&registered)
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
        6,
        "refused loads dispatch nothing"
    );

    server.shutdown().await.expect("test server shuts down");
}
