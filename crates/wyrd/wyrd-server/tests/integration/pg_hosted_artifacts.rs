//! Registered Artifact evidence and code through the hosted runtime's one
//! authorized loader, and a hosted Agent's tools under recovered authority,
//! against a real bound server, its Oracle, and the real Monty worker.
//!
//! The Artifact is registered through the SDK registration saga, so its Card
//! blob, inventory, and stored bytes are exactly what production reads.
//! The worker is this package's `wyrd-server`, which cargo builds in the same
//! profile before it runs these tests.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
use std::time::Duration;

use secrecy::SecretString;
use serde_json::{Value, json};
use skald_prompt::{OpenAiChatOptions, Prompt, ResponseFormat, openai_chat};
use skald_runtime::{MockProvider, ProviderRegistry};
use skald_spec::wire::openai_chat::{
    OpenAiChatChoice, OpenAiChatMessage, OpenAiChatResponse, OpenAiMessageContent, OpenAiToolCall,
    OpenAiToolFunctionCall,
};
use skald_spec::{ProviderName, ProviderResponse};
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;
use vala_verify::blocks::{
    ActionExecutor, AgentCardResolver, AgentExecutor, BlockCall, BlockClock, BlockError, CallInput,
    CallValue, CheckExecutor, DriftChecks, EngineError, EvidenceReader, Position,
    PromptCardResolver,
};
use wyrd_client::WyrdClient;
use wyrd_client::auth::AuthMiddleware;
use wyrd_client::cards::Cards;
use wyrd_client::config::ClientConfig;
use wyrd_client::transport::HttpTransport;
use wyrd_client::transport::config::HttpConfig;
use wyrd_client::transport::credential::ResolvedCredential;
use wyrd_loader::{build_declaration_input, load};
use wyrd_runtime::permission::PermissionSet;
use wyrd_runtime::{
    AcceptedAuthority, BifrostPermissionScope, BifrostTableScope, DelegationStep, Permission,
    PermissionScope, Principal, PrincipalKind, PrincipalRef,
};
use wyrd_server::components::auth::Caller;
use wyrd_server::components::gateway::GatewayCallContext;
use wyrd_server::verification::hosted::HostedLimits;
use wyrd_server::verification::hosted::blocks::{Execution, HostedBlocks};
use wyrd_server::verification::hosted::evidence::TenantEvidence;
use wyrd_server::verification::hosted::registry::TenantRegistry;
use wyrd_server::verification::hosted::sandbox::{CodeExecutor, Sandbox};
use wyrd_server::verification::hosted::tools::HostedTools;
use wyrd_server::verification::hosted::workspace::Workspaces;
use wyrd_server::verification::{RuntimeLimits, VerificationRuntime};
use wyrd_spec::DataTenantId;
use wyrd_spec::auth::{Attribution, PrincipalId, Sha256Hex};
use wyrd_spec::card::agent::{AgentRunConfigSpec, AgentSpec, BuiltinTool, ExecutionRoute};
use wyrd_spec::card::verifier::{
    Action, AgentCall, Check, CodeCall, CodeCheck, CodeRuntime, CodeSource, VerifierSpec,
};
use wyrd_spec::envelope::CardKind;
use wyrd_spec::ids::{CardUid, SlotName};
use wyrd_spec::reference::{CardRef, InlineableRef, Ref};
use wyrd_spec::request_id::RequestId;
use wyrd_spec::verification::TaskId;
use wyrd_spec::verification::VerificationVerdict;
use wyrd_spec::verification::execution::{CallProvenance, Citation, InputDeclaration, Locator};
use wyrd_testing::logs::LogCapture;
use wyrd_testing::verification::VerificationFixture;
use wyrd_testing::{Bootstrap, WyrdTestServer, WyrdTestServerBuilder};

use crate::sandbox_worker::worker;

/// The registered Artifact's text evidence.
const NOTES: &[u8] = b"alpha\nbeta\n";

/// The registered Artifact's Python source.
const CHECK: &[u8] = b"def main(x):\n    return x + 1\n";

/// A bound server holding one registered two-file Artifact.
struct Registered {
    /// The server; dropping it stops the server.
    server: WyrdTestServer,
    /// The Artifact's exact reference, UID included.
    card: CardRef,
}

impl Registered {
    /// Register `notes.txt` and `check.py` as one Artifact through the SDK.
    ///
    /// # Panics
    /// Panics when the server, bootstrap, load, or registration fails.
    async fn start() -> Self {
        Self::start_with(WyrdTestServer::builder()).await
    }

    /// Register as [`Self::start`] does on a bound server `builder` starts.
    ///
    /// # Panics
    /// Panics when the server, bootstrap, load, or registration fails.
    async fn start_with(builder: WyrdTestServerBuilder) -> Self {
        let temp = tempfile::tempdir().expect("loader workspace creates");
        std::fs::write(temp.path().join("notes.txt"), NOTES).expect("notes write");
        std::fs::write(temp.path().join("check.py"), CHECK).expect("source writes");
        let card = temp.path().join("evidence.yaml");
        std::fs::write(
            &card,
            "apiVersion: wyrd/v1\nkind: Artifact\nmetadata:\n  name: hosted-evidence\n  version: 1.0.0\nartifacts:\n  - relative_path: notes.txt\n    content_type: text/plain\n  - relative_path: check.py\n    content_type: text/x-python\nspec:\n  artifact_kind: dataset\n",
        )
        .expect("card writes");
        let input = build_declaration_input(load(&card).expect("loader tree builds"))
            .expect("registration input builds");
        let server = Box::pin(builder.start_bound())
            .await
            .expect("bound test server starts");
        let Bootstrap::User { jwt, .. } = server
            .bootstrap_user("hosted-artifact-writer", &["editor"])
            .await
            .expect("writer bootstraps")
        else {
            panic!("user bootstrap returned a non-user principal");
        };
        let cards = Cards::connect_with(&client(
            server.base_url().expect("bound server has a base URL"),
            &jwt,
        ))
        .await
        .expect("handle connects");
        let receipt = Box::pin(cards.declare(&input))
            .await
            .expect("the Artifact registers");
        Self {
            server,
            card: receipt.root,
        }
    }

    /// Evidence citing `slots`, read as a user of `tenant` holding
    /// `permissions`, under `limits`.
    fn evidence(
        &self,
        tenant: DataTenantId,
        permissions: impl IntoIterator<Item = Permission>,
        slots: BTreeMap<SlotName, InputDeclaration>,
        limits: HostedLimits,
    ) -> TenantEvidence {
        TenantEvidence::new(
            self.server.state().clone(),
            user(tenant, permissions),
            slots,
            limits,
        )
    }

    /// Evidence the server's own tenant may read, citing `slots`.
    fn readable(&self, slots: BTreeMap<SlotName, InputDeclaration>) -> TenantEvidence {
        self.evidence(
            self.server.data_tenant_id(),
            [Permission::card_read()],
            slots,
            HostedLimits::default(),
        )
    }

    /// The declaration of the Artifact file at `path`.
    fn file(&self, path: &str) -> InputDeclaration {
        InputDeclaration::Artifact {
            artifact: self.card.clone(),
            path: Some(path.to_owned()),
        }
    }
}

/// A fresh user of `tenant` holding exactly `permissions`.
fn user(tenant: DataTenantId, permissions: impl IntoIterator<Item = Permission>) -> Caller {
    Caller {
        data_tenant_id: tenant,
        principal: Principal::new(
            PrincipalId::new(uuid::Uuid::now_v7()),
            PrincipalKind::User,
            tenant,
            Vec::new(),
            PermissionSet::from_iter(permissions),
        ),
        request_id: RequestId::now_v7(),
        delegation_chain: Vec::new(),
        executed_by: None,
    }
}

/// A registry client authenticated with `jwt`.
///
/// # Panics
/// Panics when the client cannot be assembled.
fn client(base_url: &str, jwt: &str) -> WyrdClient {
    let config = ClientConfig {
        http: HttpConfig {
            base_url: base_url.to_owned(),
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

/// The `notes` slot name.
///
/// # Panics
/// Never; the name is valid.
fn notes() -> SlotName {
    SlotName::new("notes").expect("slot")
}

/// One exact Artifact file feeds native media, citations, a workspace, and
/// Python code, all through the same authorized loader.
///
/// # Panics
/// Panics when a consumer refuses the valid Artifact or sees other bytes.
#[tokio::test]
async fn artifact_evidence_and_code_reach_every_consumer() {
    let registered = Registered::start().await;
    let declaration = registered.file("notes.txt");
    let evidence = Arc::new(registered.readable(BTreeMap::from([(notes(), declaration.clone())])));

    let (bytes, format) = evidence.read(&declaration).await.expect("the file reads");
    assert_eq!(
        (bytes.as_slice(), format),
        (NOTES, skald_spec::MediaFormat::Text)
    );
    evidence
        .media(&declaration)
        .await
        .expect("text is native media");
    let cited = Citation {
        slot: notes(),
        locator: Locator::Lines { start: 1, end: 2 },
    };
    assert_eq!(
        evidence
            .cited(&BTreeSet::from([notes()]), vec![cited.clone()])
            .await
            .expect("citations check"),
        vec![cited]
    );

    let workspace = Workspaces::new(HostedLimits::default())
        .open(
            &BTreeMap::from([(
                "/inputs/notes.txt".to_owned(),
                CallInput {
                    value: CallValue::Evidence(declaration),
                    sources: BTreeSet::from([notes()]),
                },
            )]),
            &evidence,
        )
        .await
        .expect("the workspace opens");
    let read = workspace
        .read("/inputs/notes.txt", 0)
        .expect("the file mounts");
    assert_eq!(read["sources"], json!(["notes"]), "{read}");

    let sandbox = Arc::new(
        Sandbox::start(
            worker(),
            HostedLimits {
                monty_min_workers: 0,
                ..HostedLimits::default()
            },
        )
        .await
        .expect("pool starts"),
    );
    let id = TaskId::new("bump").expect("task id");
    let action = Action::Code(CodeCall {
        runtime: CodeRuntime::Python,
        source: CodeSource::Artifact {
            card: Ref::Ref(registered.card.clone()),
            path: Some("check.py".to_owned()),
        },
        entrypoint: "main".to_owned(),
        inputs: BTreeMap::new(),
    });
    let output = ActionExecutor::execute(
        &CodeExecutor::new(sandbox, Arc::clone(&evidence)),
        BlockCall {
            id: &id,
            position: Position::Step,
            block: &action,
            inputs: BTreeMap::from([(
                "x".to_owned(),
                CallInput {
                    value: CallValue::Json(json!(1)),
                    sources: BTreeSet::new(),
                },
            )]),
            workspace: BTreeMap::new(),
            deadline: Instant::now() + Duration::from_secs(30),
        },
    )
    .await
    .expect("the Artifact code runs");
    assert_eq!(output.output, json!(2));
    assert_eq!(
        output.provenance,
        Some(CallProvenance::Code {
            sha256: Sha256Hex::digest(CHECK),
            runtime: CodeRuntime::Python,
        })
    );
}

/// Wrong tenant, missing grant, wrong kind, UID, or path, an oversized file,
/// and stored bytes that no longer match the inventory all refuse before any
/// byte is used.
///
/// # Panics
/// Panics when a refused read succeeds or is retryable.
#[tokio::test]
async fn artifact_reads_refuse_before_use() {
    let registered = Registered::start().await;
    let tenant = registered.server.data_tenant_id();
    let other = registered
        .server
        .seed_tenant("hosted-artifact-other")
        .await
        .expect("the other tenant seeds");
    let card = &registered.card;
    let readable = registered.readable(BTreeMap::new());
    let refused = |result: Result<(Vec<u8>, Option<skald_spec::MediaFormat>), BlockError>| {
        let error = result.expect_err("refused");
        assert!(matches!(error, BlockError::Terminal { .. }), "{error}");
    };

    for (tenant, permissions) in [(tenant, Vec::new()), (other, vec![Permission::card_read()])] {
        let evidence = registered.evidence(
            tenant,
            permissions,
            BTreeMap::new(),
            HostedLimits::default(),
        );
        refused(evidence.artifact(card, Some("notes.txt")).await);
    }
    let prompt = CardRef {
        kind: CardKind::Prompt,
        ..card.clone()
    };
    let stranger = CardRef {
        uid: Some(CardUid::from_uuid(uuid::Uuid::now_v7()).expect("uid")),
        ..card.clone()
    };
    for (card, path) in [
        (&prompt, Some("notes.txt")),
        (&stranger, Some("notes.txt")),
        (card, Some("missing.txt")),
        (card, None),
    ] {
        refused(readable.artifact(card, path).await);
    }
    let cramped = registered.evidence(
        tenant,
        [Permission::card_read()],
        BTreeMap::new(),
        HostedLimits {
            file_max_bytes: 4,
            ..HostedLimits::default()
        },
    );
    refused(cramped.artifact(card, Some("notes.txt")).await);

    let uid = card.uid.as_ref().expect("the receipt names the UID");
    let object = wyrd_storage::tenant_path::validate(
        &wyrd_storage::tenant_path::build(tenant, uid.as_str(), "notes.txt"),
        tenant,
    )
    .expect("valid path");
    let storage = &registered.server.state().storage;
    for tampered in [&b"alpha\nbetA\n"[..], b"alpha\nbeta\ngamma\n"] {
        storage
            .put_object(&object, tampered.to_vec())
            .await
            .expect("tamper");
        refused(readable.artifact(card, Some("notes.txt")).await);
    }
}

/// Refuses every registry lookup; the Agent here is authored inline.
struct NoRegistry;

#[async_trait::async_trait]
impl AgentCardResolver for NoRegistry {
    /// Always refuses.
    ///
    /// # Errors
    /// Always [`BlockError::Terminal`].
    async fn resolve(&self, _agent_ref: &CardRef) -> Result<AgentSpec, BlockError> {
        Err(BlockError::Terminal {
            reason: "no registry".to_owned(),
        })
    }
}

#[async_trait::async_trait]
impl PromptCardResolver for NoRegistry {
    /// Always refuses.
    ///
    /// # Errors
    /// Always [`BlockError::Terminal`].
    async fn resolve(&self, _prompt_ref: &CardRef) -> Result<Prompt, BlockError> {
        Err(BlockError::Terminal {
            reason: "no registry".to_owned(),
        })
    }
}

/// One scripted OpenAI answer: `text`, or else the `calls` it requests.
fn answer(text: Option<&str>, calls: &[(&str, Value)]) -> ProviderResponse {
    let tool_calls = (!calls.is_empty()).then(|| {
        calls
            .iter()
            .enumerate()
            .map(|(index, (name, args))| OpenAiToolCall {
                id: format!("call_{index}"),
                kind: "function".to_owned(),
                function: OpenAiToolFunctionCall {
                    name: (*name).to_owned(),
                    arguments: args.to_string(),
                },
            })
            .collect()
    });
    ProviderResponse::OpenAiChatCompletion(OpenAiChatResponse {
        id: "resp".to_owned(),
        object: "chat.completion".to_owned(),
        created: 0,
        model: "gpt-test".to_owned(),
        choices: vec![OpenAiChatChoice {
            index: 0,
            message: OpenAiChatMessage {
                role: "assistant".to_owned(),
                content: text.map(|text| OpenAiMessageContent::Text(text.to_owned())),
                tool_calls,
                ..Default::default()
            },
            finish_reason: Some(
                if calls.is_empty() {
                    "stop"
                } else {
                    "tool_calls"
                }
                .to_owned(),
            ),
            logprobs: None,
        }],
        usage: None,
        system_fingerprint: None,
        service_tier: None,
    })
}

/// A native-route Agent answering any JSON object about `{{document}}`,
/// declaring `tools`.
///
/// # Panics
/// Panics when the static Prompt or schema is invalid.
fn reviewer(tools: &[&str]) -> AgentCall {
    let prompt = openai_chat(
        "gpt-test",
        OpenAiChatOptions {
            messages: vec!["Review: {{document}}".to_owned()],
            output: Some(
                ResponseFormat::json_schema("review", json!({"type": "object"})).expect("schema"),
            ),
            ..OpenAiChatOptions::default()
        },
    )
    .expect("static prompt is valid");
    AgentCall {
        agent: InlineableRef::Inline(Box::new(AgentSpec {
            prompt: InlineableRef::Inline(Box::new(prompt.into_native())),
            execution_route: ExecutionRoute::Native {},
            tool_names: tools.iter().map(|&name| name.to_owned()).collect(),
            run_config: AgentRunConfigSpec::default(),
            verified_by: Vec::new(),
        })),
        inputs: BTreeMap::new(),
        workspace: BTreeMap::new(),
    }
}

/// The `document` input carrying `value`, sourced from the `notes` slot.
///
/// # Panics
/// Never; the slot name is valid.
fn document(value: Value) -> BTreeMap<String, CallInput> {
    BTreeMap::from([(
        "document".to_owned(),
        CallInput {
            value: CallValue::Json(value),
            sources: BTreeSet::from([notes()]),
        },
    )])
}

/// A queued run's recovered authority drives a hosted Agent step and check.
///
/// The accepted principal was delegated, holds every tool grant, and reads
/// only `bifrost.logs.records`. Rebuilt through [`Caller::accepted`], its
/// query, code, and file calls run and are recorded, yet Oracle refuses the
/// other table and a join reaching it: tool grants never widen data scope.
/// The located line the file tool returned becomes the step's citation, and
/// a tool-less check judged by the real evidence reader keeps it and drops a
/// fabricated one. A call to a tool the Agent did not declare fails the step.
///
/// # Panics
/// Panics when a scope widens, a call goes unrecorded, an invalid citation
/// survives, or an undeclared tool is accepted.
#[tokio::test(flavor = "multi_thread")]
async fn recovered_authority_bounds_every_agent_tool() {
    /// A query of the one granted table.
    const LOGS: &str = "SELECT severity_text FROM bifrost.logs.records";
    /// A query of a table the authority does not hold.
    const SPANS: &str = "SELECT name FROM bifrost.traces.spans";
    /// One plan reaching both tables.
    const MIXED: &str = "SELECT r.severity_text, s.name \
         FROM bifrost.logs.records r JOIN bifrost.traces.spans s ON r.trace_id = s.trace_id";
    let registered = Registered::start().await;
    let state = registered.server.state().clone();
    let tenant = registered.server.data_tenant_id();
    let catalog = registered.server.bifrost_catalog();
    let mut uids = Vec::new();
    for (namespace, name) in [("logs", "records"), ("traces", "spans")] {
        let definition =
            vala_bifrost_redux::tables::builtin_table(namespace, name).expect("canonical built-in");
        uids.push(
            catalog
                .ensure_builtin(tenant, definition, Attribution::unresolved())
                .await
                .expect("built-in registers"),
        );
    }
    let logs_only = Permission {
        scope: PermissionScope::Bifrost(BifrostPermissionScope::Table(BifrostTableScope {
            catalog: "bifrost".to_owned(),
            schema: "logs".to_owned(),
            table_uid: uuid::Uuid::from_bytes(*uids[0].as_bytes()),
        })),
        ..Permission::bifrost_query_read()
    };
    let principal = Principal::new(
        PrincipalId::new(uuid::Uuid::now_v7()),
        PrincipalKind::User,
        tenant,
        Vec::new(),
        PermissionSet::from_iter([
            Permission::tool_invoke_any(),
            Permission::card_read(),
            logs_only,
        ]),
    );
    let initiator = Principal::new(
        PrincipalId::new(uuid::Uuid::now_v7()),
        PrincipalKind::User,
        tenant,
        Vec::new(),
        PermissionSet::from_iter([Permission::bifrost_query_read()]),
    );
    let authority = AcceptedAuthority::new(
        &principal,
        &[DelegationStep {
            principal: PrincipalRef::from_principal(&initiator),
        }],
    );
    let caller = Caller::accepted(&authority, PrincipalId::new(uuid::Uuid::now_v7()));

    let limits = HostedLimits {
        monty_min_workers: 0,
        ..HostedLimits::default()
    };
    let declaration = registered.file("notes.txt");
    let evidence = Arc::new(TenantEvidence::new(
        state.clone(),
        caller.clone(),
        BTreeMap::from([(notes(), declaration.clone())]),
        limits,
    ));
    let tools = HostedTools::new(
        state,
        caller,
        Arc::clone(&evidence),
        Arc::new(Workspaces::new(limits)),
        Arc::new(Sandbox::start(worker(), limits).await.expect("pool starts")),
        limits,
    );
    let read = json!({"path": "/inputs/notes.txt", "start": 0});
    let code = json!({"source": "from pathlib import Path\nPath('/inputs/notes.txt').read_text()"});
    let cited = json!({"slot": "notes", "locator": {"lines": {"start": 1, "end": 1}}});
    let mock = MockProvider::new(ProviderName::OpenAi);
    for response in [
        answer(
            None,
            &[
                ("bifrost_query", json!({"sql": LOGS})),
                ("bifrost_query", json!({"sql": SPANS})),
                ("bifrost_query", json!({"sql": MIXED})),
                ("code_run", code.clone()),
                ("fs_read", read.clone()),
            ],
        ),
        answer(
            Some(&json!({"finding": "alpha", "citations": [cited]}).to_string()),
            &[],
        ),
        answer(
            Some(
                &json!({"outcome": "passed", "reason": "grounded", "citations": [
                    cited,
                    {"slot": "notes", "locator": {"lines": {"start": 9, "end": 9}}}
                ]})
                .to_string(),
            ),
            &[],
        ),
        answer(None, &[("cards_list", json!({}))]),
    ] {
        mock.push_response(response);
    }
    let mut providers = ProviderRegistry::new();
    providers.register(Arc::new(mock));
    let executor = AgentExecutor::new(
        Arc::new(providers),
        Arc::new(NoRegistry),
        Arc::new(NoRegistry),
    )
    .with_toolset(Arc::new(tools))
    .with_evidence(evidence);

    let declared = reviewer(&["bifrost.query", "code.run", "fs.read"]);
    let id = TaskId::new("review").expect("task id");
    let (executor, id, registered) = (&executor, &id, &registered);
    let step = |action: Action| async move {
        ActionExecutor::execute(
            executor,
            BlockCall {
                id,
                position: Position::Step,
                block: &action,
                inputs: document(json!("the notes")),
                workspace: BTreeMap::from([(
                    "/inputs/notes.txt".to_owned(),
                    CallInput {
                        value: CallValue::Evidence(registered.file("notes.txt")),
                        sources: BTreeSet::from([notes()]),
                    },
                )]),
                deadline: Instant::now() + Duration::from_secs(60),
            },
        )
        .await
    };
    let output = step(Action::Agent(declared.clone()))
        .await
        .expect("the step runs");
    let error = |tool: BuiltinTool, args: &Value| {
        let record = output
            .tool_calls
            .iter()
            .find(|record| record.tool == tool && &record.arguments == args)
            .expect("the call is recorded");
        record.error.clone()
    };
    assert_eq!(output.tool_calls.len(), 5);
    assert_eq!(
        error(BuiltinTool::BifrostQuery, &json!({"sql": LOGS})),
        None
    );
    for sql in [SPANS, MIXED] {
        let refused = error(BuiltinTool::BifrostQuery, &json!({"sql": sql}));
        assert!(
            refused
                .as_deref()
                .is_some_and(|reason| reason.contains("query forbidden")),
            "{sql}: {refused:?}"
        );
    }
    assert_eq!(error(BuiltinTool::CodeRun, &code), None);
    assert_eq!(error(BuiltinTool::FsRead, &read), None);
    assert_eq!(output.output["citations"], json!([cited]));

    let check = Check::Agent(reviewer(&[]));
    let verdict = CheckExecutor::execute(
        executor,
        BlockCall {
            id,
            position: Position::VerifierCheck,
            block: &check,
            inputs: document(output.output.clone()),
            workspace: BTreeMap::new(),
            deadline: Instant::now() + Duration::from_secs(60),
        },
    )
    .await
    .expect("the check runs")
    .verdict;
    assert_eq!(verdict.outcome, VerificationVerdict::Passed);
    assert_eq!(
        serde_json::to_value(&verdict.citations).expect("citations"),
        json!([cited])
    );

    let refused = step(Action::Agent(declared))
        .await
        .expect_err("cards.list was not declared");
    assert!(matches!(refused, BlockError::Terminal { .. }), "{refused}");
}

/// Agent and Prompt Card resolution is the caller's audited `card:read`:
/// a caller with the grant reaches the Card (and is then told it is not an
/// Agent or Prompt), while no grant, another tenant, or no caller at all
/// refuses before the Card is read.
///
/// # Panics
/// Panics when a lookup reads a Card its caller may not read.
#[tokio::test]
async fn card_resolution_is_the_callers_read() {
    let registered = Registered::start().await;
    let state = registered.server.state().clone();
    let tenant = registered.server.data_tenant_id();
    let other = registered
        .server
        .seed_tenant("hosted-registry-other")
        .await
        .expect("the other tenant seeds");
    let card = &registered.card;
    let registry = |caller: Option<Caller>| TenantRegistry::new(state.clone(), caller);
    let reason = |error: BlockError| match error {
        BlockError::Terminal { reason } => reason,
        BlockError::Retryable { reason } => panic!("retryable: {reason}"),
        BlockError::Busy { reason } => panic!("busy: {reason}"),
    };

    let readable = registry(Some(user(tenant, [Permission::card_read()])));
    let read = reason(
        AgentCardResolver::resolve(&readable, card)
            .await
            .expect_err("an Artifact is not an Agent"),
    );
    assert!(read.contains("is not an Agent"), "{read}");
    let read = reason(
        PromptCardResolver::resolve(&readable, card)
            .await
            .expect_err("an Artifact is not a Prompt"),
    );
    assert!(read.contains("is not a Prompt"), "{read}");

    for caller in [
        Some(user(tenant, [])),
        Some(user(other, [Permission::card_read()])),
        None,
    ] {
        let refused = reason(
            AgentCardResolver::resolve(&registry(caller), card)
                .await
                .expect_err("refused"),
        );
        assert!(!refused.contains("is not an Agent"), "{refused}");
    }
}

/// Hosted limits of a one-worker pool with no waiters and one workspace, so
/// a single held admission saturates each.
fn saturable() -> HostedLimits {
    HostedLimits {
        monty_min_workers: 0,
        monty_max_workers: 1,
        monty_max_waiters: 0,
        max_workspaces: 1,
        code_timeout: Duration::from_secs(2),
        ..HostedLimits::default()
    }
}

/// The notes file as one `/inputs` workspace entry.
fn notes_workspace(registered: &Registered) -> BTreeMap<String, CallInput> {
    BTreeMap::from([(
        "/inputs/notes.txt".to_owned(),
        CallInput {
            value: CallValue::Evidence(registered.file("notes.txt")),
            sources: BTreeSet::from([notes()]),
        },
    )])
}

/// Process-capacity refusal stays busy through every hosted consumer: a
/// code action, a code check, an Agent step's `code.run`, and an Agent step
/// whose workspace finds every workspace taken. None is reported as a
/// retryable or terminal block failure, the Agent never answers the model
/// with the refusal, and the refused workspace open never waits. Once the
/// held worker admission and workspace drop, the same code runs.
///
/// # Panics
/// Panics when a refusal is not busy, a refused call waits, or capacity
/// does not return.
#[tokio::test(flavor = "multi_thread")]
async fn capacity_refusal_is_busy_at_every_hosted_consumer() {
    let registered = Registered::start().await;
    let limits = saturable();
    let evidence = Arc::new(registered.evidence(
        registered.server.data_tenant_id(),
        [Permission::card_read(), Permission::tool_invoke_any()],
        BTreeMap::from([(notes(), registered.file("notes.txt"))]),
        limits,
    ));
    let sandbox = Arc::new(Sandbox::start(worker(), limits).await.expect("pool starts"));
    let workspaces = Arc::new(Workspaces::new(limits));
    let none = serde_json::Map::new();
    let far = Instant::now() + Duration::from_secs(30);
    // One admitted operation holds the only worker admission while pending.
    let mut held = Box::pin(sandbox.run("def f():\n    return 1", "f", &none, Vec::new(), far));
    assert!(
        futures_util::FutureExt::now_or_never(&mut held).is_none(),
        "the held operation waits for its worker"
    );

    let busy = |result: Result<(), BlockError>, consumer: &str| match result {
        Err(BlockError::Busy { .. }) => {}
        other => panic!("{consumer} must be refused busy, got {other:?}"),
    };
    let id = TaskId::new("bump").expect("task id");
    let call = CodeCall {
        runtime: CodeRuntime::Python,
        source: CodeSource::Inline("def main(x):\n    return x + 1".to_owned()),
        entrypoint: "main".to_owned(),
        inputs: BTreeMap::new(),
    };
    let one = || {
        BTreeMap::from([(
            "x".to_owned(),
            CallInput {
                value: CallValue::Json(json!(1)),
                sources: BTreeSet::new(),
            },
        )])
    };
    let code = CodeExecutor::new(Arc::clone(&sandbox), Arc::clone(&evidence));
    let action = Action::Code(call.clone());
    let act = || {
        ActionExecutor::execute(
            &code,
            BlockCall {
                id: &id,
                position: Position::Step,
                block: &action,
                inputs: one(),
                workspace: BTreeMap::new(),
                deadline: far,
            },
        )
    };
    busy(act().await.map(drop), "a code action");
    let check = Check::Code(CodeCheck {
        call,
        response_format: None,
    });
    busy(
        CheckExecutor::execute(
            &code,
            BlockCall {
                id: &id,
                position: Position::VerifierCheck,
                block: &check,
                inputs: one(),
                workspace: BTreeMap::new(),
                deadline: far,
            },
        )
        .await
        .map(drop),
        "a code check",
    );

    let mock = Arc::new(MockProvider::new(ProviderName::OpenAi));
    mock.push_response(answer(None, &[("code_run", json!({"source": "1"}))]));
    mock.push_response(answer(Some("{}"), &[]));
    let mut providers = ProviderRegistry::new();
    providers.register(Arc::clone(&mock) as _);
    let agent = AgentExecutor::new(
        Arc::new(providers),
        Arc::new(NoRegistry),
        Arc::new(NoRegistry),
    )
    .with_toolset(Arc::new(HostedTools::new(
        registered.server.state().clone(),
        user(
            registered.server.data_tenant_id(),
            [Permission::card_read(), Permission::tool_invoke_any()],
        ),
        Arc::clone(&evidence),
        Arc::clone(&workspaces),
        Arc::clone(&sandbox),
        limits,
    )))
    .with_evidence(Arc::clone(&evidence) as Arc<dyn EvidenceReader>);
    let step = |tools: &[&str]| {
        let action = Action::Agent(reviewer(tools));
        let (agent, id, registered) = (&agent, &id, &registered);
        async move {
            ActionExecutor::execute(
                agent,
                BlockCall {
                    id,
                    position: Position::Step,
                    block: &action,
                    inputs: document(json!("the notes")),
                    workspace: notes_workspace(registered),
                    deadline: Instant::now() + Duration::from_secs(30),
                },
            )
            .await
            .map(drop)
        }
    };
    busy(step(&["code.run"]).await, "an Agent's code.run");
    assert_eq!(
        mock.remaining(),
        1,
        "the refusal ends the loop before the model is called again"
    );

    let taken = workspaces
        .open(&notes_workspace(&registered), &evidence)
        .await
        .expect("the only workspace opens");
    // A waiting open would never finish while `taken` is held.
    busy(step(&["fs.read"]).await, "an Agent's workspace");

    drop(taken);
    drop(held);
    let ran = act().await.expect("capacity returns once released");
    assert_eq!(ran.output, json!(2));
}

/// A queued execution refused the only Python worker returns to the queue
/// with its attempt refunded and runs once capacity frees.
///
/// The runtime hosts a one-worker pool with no waiters. The first queued
/// execution's code spins for the whole code timeout, holding the worker;
/// the second, claimed meanwhile, is refused busy and deferred rather than
/// failed, retried, or judged. Once the spinner ends, the second completes
/// on its first counted attempt with its real verdict.
///
/// # Panics
/// Panics when the server, seeding, or runtime fails, the second execution
/// is never deferred, or it settles other than `completed` on one attempt.
#[tokio::test]
async fn busy_queued_execution_is_deferred_until_capacity_frees() {
    let logs = LogCapture::install();
    let registered = Registered::start_with(
        WyrdTestServer::builder().with_verification_limits_for_test(saturable()),
    )
    .await;
    let tenant = registered.server.data_tenant_id();
    registered
        .server
        .ensure_builtin_tables_for_test(tenant)
        .await
        .expect("tenant receives every built-in table");
    let state = registered.server.state().clone();
    let seed = VerificationFixture::provision(state.postgres.wyrd(), tenant)
        .await
        .expect("tenant provisions");
    let (subject, _) = seed
        .service("busy-subject")
        .await
        .expect("subject registers");
    let code = |name: &str, source: &str, expected: Value| {
        json!({
            "inputs": { "x": { "type": "json" } },
            "steps": [{
                "id": name,
                "action": { "kind": "code", "entrypoint": "f", "source": { "inline": source },
                            "inputs": { "x": "inputs.x" } },
                "requirements": [{ "id": format!("{name}-ok"), "statement": "The code answers.",
                    "check": { "kind": "assert", "path": format!("steps.{name}.output"),
                               "operator": "equals", "expected": expected } }]
            }]
        })
    };
    let spinner = seed
        .verifier(
            "busy-spinner",
            &code("spin", "def f(x):\n    while True:\n        pass", json!(0)),
        )
        .await
        .expect("spinner registers");
    let adder = seed
        .verifier(
            "busy-adder",
            &code("add", "def f(x):\n    return x + 1", json!(2)),
        )
        .await
        .expect("adder registers");
    let one = || {
        BTreeMap::from([(
            SlotName::new("x").expect("slot"),
            InputDeclaration::Json { value: json!(1) },
        )])
    };
    let spinning = seed
        .submit(&spinner, &subject, one())
        .await
        .expect("spinner queues");
    let deferred = seed
        .submit(&adder, &subject, one())
        .await
        .expect("adder queues");

    let runtime = VerificationRuntime::builder(&state)
        .limits(RuntimeLimits {
            lease: Duration::from_mins(1),
            execution_timeout: Duration::from_secs(20),
            poll_interval: Duration::from_millis(50),
            ..RuntimeLimits::default()
        })
        .build()
        .expect("the runtime composes");
    let stop = CancellationToken::new();
    let task = tokio::spawn(runtime.run(stop.clone()));
    let deadline = Instant::now() + Duration::from_mins(1);
    let settled = loop {
        let row = seed.execution(deferred).await.expect("execution reads");
        if !matches!(row.status.as_str(), "pending" | "running" | "retrying") {
            break row;
        }
        assert!(
            Instant::now() < deadline,
            "the deferred execution never settled"
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    };
    stop.cancel();
    task.await.expect("the runtime stops");

    let text = logs.text();
    assert!(
        text.lines()
            .any(|line| line.contains("deferred by admission backpressure")
                && line.contains(&deferred.to_string())),
        "the adder was refused the busy worker and deferred:\n{text}"
    );
    assert_eq!(
        (settled.status.as_str(), settled.attempts),
        ("completed", 1),
        "a deferral spends no attempt: {settled:?}"
    );
    assert!(settled.result_id.is_some(), "{settled:?}");
    let spun = seed.execution(spinning).await.expect("spinner reads");
    assert_eq!(spun.status, "completed", "{spun:?}");
}

/// Hosted limits whose individual input-file bound admits the 11-byte notes
/// file and whose combined bound admits it once but not twice.
fn tight_files() -> HostedLimits {
    HostedLimits {
        file_max_bytes: 12,
        files_max_bytes: 20,
        ..HostedLimits::default()
    }
}

/// A Verifier extracting media slots `a` and optional `b`, accepting
/// `formats` up to `max_bytes`, whose one requirement holds when `a`
/// extracts in two separate steps.
///
/// # Panics
/// Panics when the static spec is invalid.
fn two_extracts(formats: &[&str], max_bytes: Option<u64>) -> Arc<VerifierSpec> {
    let slot = |required: bool| json!({ "type": "media", "formats": formats, "max_bytes": max_bytes, "required": required });
    let spec: VerifierSpec = serde_json::from_value(json!({
        "inputs": { "a": slot(true), "b": slot(false) },
        "steps": [
            { "id": "first", "action": { "kind": "extract", "from": "inputs.a" } },
            { "id": "again", "action": { "kind": "extract", "from": "inputs.a" } }
        ],
        "requirements": [{ "id": "read-twice", "statement": "a extracts twice.",
            "check": { "kind": "assert", "path": "steps.again.output", "operator": "is_not_null" } }]
    }))
    .expect("Verifier decodes");
    spec.validate().expect("valid Verifier");
    Arc::new(spec)
}

/// A selected Artifact file is admitted against its media slot and the
/// execution's combined input-file bound before any block runs, through the
/// per-execution engine direct and queued executions share: a format the
/// slot refuses, a file above the slot's `max_bytes`, the same file pinned
/// in two slots, and an Artifact plus an unread uploaded file whose
/// declared lengths together pass the bound are all refused, as is the
/// file in an uncapped slot on engines whose individual input-file bound
/// it passes within the combined one. One pinned Artifact read by two
/// extract steps is charged once and passes.
///
/// # Panics
/// Panics when the server, pool, or engine fails to start, a refusal is
/// not the expected input error, or the within-budget control fails.
#[tokio::test]
async fn artifact_evidence_is_admitted_against_its_slot_and_the_combined_limit() {
    let registered = Registered::start().await;
    let blocks = HostedBlocks::start(worker(), tight_files())
        .await
        .expect("pool starts");
    let caller = user(
        registered.server.data_tenant_id(),
        [Permission::card_read()],
    );
    let slot = |name: &str| SlotName::new(name).expect("slot");
    let cramped = HostedBlocks::start(
        worker(),
        HostedLimits {
            file_max_bytes: 4,
            ..tight_files()
        },
    )
    .await
    .expect("cramped pool starts");
    let run_on = |cramped_pool: bool,
                  spec: Arc<VerifierSpec>,
                  evidence: BTreeMap<SlotName, InputDeclaration>| {
        let blocks = if cramped_pool { &cramped } else { &blocks };
        let (caller, state) = (&caller, registered.server.state());
        async move {
            let routes = BTreeMap::new();
            blocks
                .engine(
                    state,
                    Execution {
                        caller,
                        evidence: &evidence,
                        routes: &routes,
                        subject: GatewayCallContext::default(),
                        defer_unlanded_traces: false,
                        drift: DriftChecks::new(None, BTreeMap::new(), BTreeMap::new()),
                        clock: BlockClock::default(),
                    },
                )
                .expect("engine builds")
                .run(spec, evidence, Instant::now() + Duration::from_secs(30))
                .await
        }
    };
    let run = |spec, evidence| run_on(false, spec, evidence);
    let unread = InputDeclaration::File {
        sha256: Sha256Hex::digest(b"never uploaded"),
        size_bytes: 10,
        format: "text".parse().expect("format"),
        filename: None,
    };
    let cases = [
        (
            two_extracts(&["pdf"], None),
            vec![("a", registered.file("notes.txt"))],
            "does not accept",
            false,
        ),
        (
            two_extracts(&["text"], Some(5)),
            vec![("a", registered.file("notes.txt"))],
            "5-byte limit",
            true,
        ),
        (
            two_extracts(&["text"], None),
            vec![
                ("a", registered.file("notes.txt")),
                ("b", registered.file("notes.txt")),
            ],
            "combined 20-byte limit",
            true,
        ),
        (
            two_extracts(&["text"], None),
            vec![("a", registered.file("notes.txt")), ("b", unread)],
            "combined 20-byte limit",
            true,
        ),
    ];
    for (spec, inputs, expected, too_large) in cases {
        let evidence = inputs
            .into_iter()
            .map(|(name, declaration)| (slot(name), declaration))
            .collect();
        let refused = run(spec, evidence).await;
        let ((Err(EngineError::InputInvalid(reason)), false)
        | (Err(EngineError::InputTooLarge(reason)), true)) = (&refused, too_large)
        else {
            panic!("expected a refusal containing {expected:?}: {refused:?}")
        };
        assert!(reason.contains(expected), "{reason}");
    }
    let refused = run_on(
        true,
        two_extracts(&["text"], None),
        BTreeMap::from([(slot("a"), registered.file("notes.txt"))]),
    )
    .await;
    let Err(EngineError::InputTooLarge(reason)) = &refused else {
        panic!("expected the individual input-file limit: {refused:?}")
    };
    assert!(reason.contains("4-byte input file limit"), "{reason}");

    let outcome = run(
        two_extracts(&["text"], Some(11)),
        BTreeMap::from([(slot("a"), registered.file("notes.txt"))]),
    )
    .await
    .expect("one pinned Artifact within every bound runs");
    assert_eq!(outcome.verdict, VerificationVerdict::Passed, "{outcome:?}");
}

/// A queued execution whose evidence does not conform to its Verifier, whose
/// one unused file passes the individual input-file bound within the
/// combined one, or whose declared files together pass the combined bound
/// settles
/// `errored` with `input_invalid` on its first attempt and no result, so no
/// verdict or Operator follows and no retry is spent; conforming evidence in
/// the same runtime completes.
///
/// # Panics
/// Panics when the server, seeding, or runtime fails, or an execution
/// settles otherwise.
#[tokio::test]
async fn nonconforming_queued_evidence_errors_without_a_verdict() {
    let registered = Registered::start_with(
        WyrdTestServer::builder().with_verification_limits_for_test(tight_files()),
    )
    .await;
    let tenant = registered.server.data_tenant_id();
    registered
        .server
        .ensure_builtin_tables_for_test(tenant)
        .await
        .expect("tenant receives every built-in table");
    let state = registered.server.state().clone();
    let seed = VerificationFixture::provision(state.postgres.wyrd(), tenant)
        .await
        .expect("tenant provisions");
    let (subject, _) = seed
        .service("conform-subject")
        .await
        .expect("subject registers");
    let verifier = seed
        .verifier(
            "conform-typed",
            &json!({
                "inputs": {
                    "x": { "type": "json", "schema": { "type": "integer" } },
                    "a": { "type": "media", "formats": ["text"], "required": false },
                    "b": { "type": "media", "formats": ["text"], "required": false }
                },
                "requirements": [{ "id": "x-present", "statement": "x is present.",
                    "check": { "kind": "assert", "path": "inputs.x", "operator": "is_not_null" } }]
            }),
        )
        .await
        .expect("Verifier registers");
    let slot = |name: &str| SlotName::new(name).expect("slot");
    let x = |value: Value| (slot("x"), InputDeclaration::Json { value });
    let file = |name: &str, body: &[u8], size_bytes| {
        (
            slot(name),
            InputDeclaration::File {
                sha256: Sha256Hex::digest(body),
                size_bytes,
                format: "text".parse().expect("format"),
                filename: None,
            },
        )
    };
    let invalid = seed
        .submit(&verifier, &subject, BTreeMap::from([x(json!("one"))]))
        .await
        .expect("schema-invalid execution queues");
    let large = seed
        .submit(
            &verifier,
            &subject,
            BTreeMap::from([x(json!(1)), file("a", b"large", 15)]),
        )
        .await
        .expect("individually oversized execution queues");
    let oversized = seed
        .submit(
            &verifier,
            &subject,
            BTreeMap::from([
                x(json!(1)),
                file("a", b"first", 11),
                file("b", b"second", 11),
            ]),
        )
        .await
        .expect("oversized execution queues");
    let valid = seed
        .submit(&verifier, &subject, BTreeMap::from([x(json!(1))]))
        .await
        .expect("valid execution queues");

    let runtime = VerificationRuntime::builder(&state)
        .limits(RuntimeLimits {
            lease: Duration::from_mins(1),
            execution_timeout: Duration::from_secs(20),
            poll_interval: Duration::from_millis(50),
            ..RuntimeLimits::default()
        })
        .build()
        .expect("the runtime composes");
    let stop = CancellationToken::new();
    let task = tokio::spawn(runtime.run(stop.clone()));
    let deadline = Instant::now() + Duration::from_mins(1);
    let mut settled = Vec::new();
    for execution in [invalid, large, oversized, valid] {
        settled.push(loop {
            let row = seed.execution(execution).await.expect("execution reads");
            if !matches!(row.status.as_str(), "pending" | "running" | "retrying") {
                break row;
            }
            assert!(Instant::now() < deadline, "{execution} never settled");
            tokio::time::sleep(Duration::from_millis(50)).await;
        });
    }
    stop.cancel();
    task.await.expect("the runtime stops");

    for refused in &settled[..3] {
        assert_eq!(
            (
                refused.status.as_str(),
                refused.attempts,
                refused.error_code.as_deref()
            ),
            ("errored", 1, Some("input_invalid")),
            "{refused:?}"
        );
        assert!(refused.result_id.is_none(), "{refused:?}");
    }
    assert_eq!(settled[3].status, "completed", "{:?}", settled[3]);
    assert!(settled[3].result_id.is_some(), "{:?}", settled[3]);
}
