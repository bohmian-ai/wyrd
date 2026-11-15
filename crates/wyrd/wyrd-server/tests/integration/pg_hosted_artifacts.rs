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
    PromptCardResolver, ToolCallsOwner,
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
use wyrd_server::verification::hosted::evidence::{Loaded, TenantEvidence};
use wyrd_server::verification::hosted::registry::TenantRegistry;
use wyrd_server::verification::hosted::sandbox::{CodeExecutor, Sandbox};
use wyrd_server::verification::hosted::tools::HostedTools;
use wyrd_server::verification::hosted::workspace::Workspaces;
use wyrd_server::verification::telemetry::{Envelopes, ExecutionMode};
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
use wyrd_spec::verification::execution::{
    CallProvenance, Citation, InputDeclaration, InvocationSettlement, Locator,
};
use wyrd_testing::logs::LogCapture;
use wyrd_testing::verification::VerificationFixture;
use wyrd_testing::{Bootstrap, WyrdTestServer, WyrdTestServerBuilder};

use crate::sandbox_worker::{roomy, worker, workspaces};

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

    /// Register Experiment `seeded` 1.0.0, whose structured `seed`
    /// default parameter is `u64::MAX`, as another writer of the tenant.
    ///
    /// # Panics
    /// Panics when bootstrap, load, or registration fails.
    async fn seeded_experiment(&self) {
        let temp = tempfile::tempdir().expect("loader workspace creates");
        let card = temp.path().join("experiment.yaml");
        std::fs::write(
            &card,
            format!(
                "apiVersion: wyrd/v1\nkind: Experiment\nmetadata:\n  name: seeded\n  version: 1.0.0\nspec:\n  default_parameters:\n    seed:\n      type: json\n      value: {}\n",
                u64::MAX
            ),
        )
        .expect("card writes");
        let input = build_declaration_input(load(&card).expect("loader tree builds"))
            .expect("registration input builds");
        let Bootstrap::User { jwt, .. } = self
            .server
            .bootstrap_user("hosted-experiment-writer", &["editor"])
            .await
            .expect("writer bootstraps")
        else {
            panic!("user bootstrap returned a non-user principal");
        };
        let cards = Cards::connect_with(&client(
            self.server.base_url().expect("bound server has a base URL"),
            &jwt,
        ))
        .await
        .expect("handle connects");
        Box::pin(cards.declare(&input))
            .await
            .expect("the Experiment registers");
    }

    /// Evidence read as a user of `tenant` holding `permissions`, under
    /// `limits`, citing what it admits.
    fn evidence(
        &self,
        tenant: DataTenantId,
        permissions: impl IntoIterator<Item = Permission>,
        limits: HostedLimits,
    ) -> TenantEvidence {
        TenantEvidence::new(
            self.server.state().clone(),
            user(tenant, permissions),
            limits,
            wyrd_runtime::resources::ByteLedger::new(u64::MAX),
        )
    }

    /// Evidence the server's own tenant may read, citing what it admits.
    fn readable(&self) -> TenantEvidence {
        self.evidence(
            self.server.data_tenant_id(),
            [Permission::card_read()],
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
pub(crate) fn user(
    tenant: DataTenantId,
    permissions: impl IntoIterator<Item = Permission>,
) -> Caller {
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
pub(crate) fn client(base_url: &str, jwt: &str) -> WyrdClient {
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
    let evidence = Arc::new(registered.readable());
    let pinned = Arc::new(BTreeMap::from([(notes(), declaration.clone())]));
    evidence
        .admit(&BTreeMap::new(), &pinned)
        .await
        .expect("the notes are admitted");

    let (bytes, format) = evidence.read(&declaration).await.expect("the file reads");
    assert_eq!(
        (bytes.bytes.as_slice(), format),
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

    let workspace = workspaces(HostedLimits::default())
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
            &roomy(),
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
                    value: CallValue::Json(json!(1).into()),
                    sources: BTreeSet::new(),
                },
            )]),
            workspace: BTreeMap::new(),
            deadline: Instant::now() + Duration::from_secs(30),
            attempt: 1,
            item_index: None,
            iteration_index: None,
            tool_calls: ToolCallsOwner::default().handle(),
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
    let readable = registered.readable();
    let refused = |result: Result<(Loaded, Option<skald_spec::MediaFormat>), BlockError>| {
        let error = result.expect_err("refused");
        assert!(matches!(error, BlockError::Terminal { .. }), "{error}");
    };

    for (tenant, permissions) in [(tenant, Vec::new()), (other, vec![Permission::card_read()])] {
        let evidence = registered.evidence(tenant, permissions, HostedLimits::default());
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

/// An evidence read charges process memory for the declared size before
/// it fetches a byte: a read the ledger cannot hold is refused busy and
/// leaves no charge, and a read that fits holds its charge exactly as long
/// as the caller keeps the bytes.
///
/// # Panics
/// Panics when a read past process memory is not busy, or a charge is
/// left behind or ends before the bytes do.
#[tokio::test]
async fn evidence_reads_charge_process_memory_before_reading() {
    let registered = Registered::start().await;
    let reader = |memory: &wyrd_runtime::resources::ByteLedger| {
        TenantEvidence::new(
            registered.server.state().clone(),
            user(
                registered.server.data_tenant_id(),
                [Permission::card_read()],
            ),
            HostedLimits::default(),
            memory.clone(),
        )
    };
    let size = NOTES.len() as u64;

    let cramped = wyrd_runtime::resources::ByteLedger::new(size - 1);
    let refused = reader(&cramped)
        .artifact(&registered.card, Some("notes.txt"))
        .await
        .expect_err("process memory cannot hold the read");
    assert!(matches!(refused, BlockError::Busy { .. }), "{refused}");
    assert_eq!(cramped.used(), 0, "a refused read leaves no charge");

    let roomy = wyrd_runtime::resources::ByteLedger::new(size);
    let (loaded, _) = reader(&roomy)
        .artifact(&registered.card, Some("notes.txt"))
        .await
        .expect("the read fits");
    assert_eq!(loaded.bytes, NOTES);
    assert_eq!(roomy.used(), size, "the bytes held are charged");
    drop(loaded);
    assert_eq!(roomy.used(), 0, "dropping the bytes returns the charge");
}

/// A workspace streams uploaded evidence from storage straight into its
/// `/inputs` file: a File four times the size of process memory stages,
/// with memory charged only for the chunk in flight and the whole file
/// charged to scratch, and dropping the workspace returns it. A stored body
/// that no longer matches its declaration is refused terminal and leaves no
/// file or charge behind.
///
/// # Panics
/// Panics when the File does not stage byte for byte, a charge is wrong or
/// left behind, or the tampered body stages.
#[tokio::test]
async fn workspaces_stream_evidence_to_disk() {
    let registered = Registered::start().await;
    let server = &registered.server;
    let tenant = server.data_tenant_id();
    let Bootstrap::User { jwt, .. } = server
        .bootstrap_user("hosted-stream-writer", &["editor"])
        .await
        .expect("writer bootstraps")
    else {
        panic!("user bootstrap returned a non-user principal");
    };
    let document = "streamed evidence line\n".repeat(12_000).into_bytes();
    let stored = crate::pg_verification_routes::upload_file(server, &jwt, &document).await;
    let size = document.len() as u64;
    let process = wyrd_runtime::resources::ProcessResources::new(
        std::num::NonZeroUsize::MIN,
        size / 4,
        1 << 30,
    );
    let evidence = TenantEvidence::new(
        server.state().clone(),
        Caller {
            executed_by: Some(PrincipalId::new(uuid::Uuid::now_v7())),
            ..user(tenant, [Permission::card_read()])
        },
        HostedLimits::default(),
        process.memory().clone(),
    );
    let root = tempfile::tempdir().expect("scratch root");
    let workspaces = Workspaces::new(HostedLimits::default(), &process, root.path().to_owned());
    let files = BTreeMap::from([(
        "/inputs/doc.txt".to_owned(),
        CallInput {
            value: CallValue::Evidence(InputDeclaration::File {
                sha256: Sha256Hex::digest(&document),
                size_bytes: size,
                format: skald_spec::MediaFormat::Text,
                filename: None,
            }),
            sources: BTreeSet::from([notes()]),
        },
    )]);

    let workspace = workspaces
        .open(&files, &evidence)
        .await
        .expect("a File larger than process memory stages");
    assert_eq!(
        std::fs::read(workspace.inputs_dir().join("doc.txt")).expect("staged file"),
        document
    );
    assert_eq!(process.memory().used(), 0, "no chunk stays charged");
    assert_eq!(
        process.scratch().used(),
        size,
        "the file is charged to scratch"
    );
    drop(workspace);
    assert_eq!(process.scratch().used(), 0);

    let mut tampered = document.clone();
    tampered[0] ^= 1;
    let object = wyrd_storage::tenant_path::validate_evidence(&stored, tenant).expect("valid path");
    server
        .state()
        .storage
        .put_object(&object, tampered)
        .await
        .expect("tamper");
    let refused = workspaces.open(&files, &evidence).await.map(drop);
    assert!(
        matches!(&refused, Err(BlockError::Terminal { reason }) if reason.contains("digest")),
        "{refused:?}"
    );
    assert_eq!(process.scratch().used(), 0);
    assert_eq!(process.memory().used(), 0);
    assert_eq!(
        std::fs::read_dir(root.path())
            .expect("scratch root")
            .count(),
        0,
        "the refused workspace is removed"
    );
}

/// One execution resolves each input's metadata once: an uploaded File's
/// recorded location is read from the object inventory, and an exact
/// Artifact's files from its Card manifest, a single time each, at
/// admission, and every later selection, read, citation, native media, and
/// workspace reuses them.
///
/// The one inventory read runs as the execution's SYSTEM executor, here a
/// fresh principal the tenant registry never named, so it does not look the
/// SYSTEM principal up in Postgres; the Artifact never reads the inventory.
/// Every Artifact use still makes its own
/// `card:read` decision: a caller without the grant or in another tenant is
/// refused before any object is read, and a foreign tenant's caller finds
/// no completed File for the same digest.
///
/// # Panics
/// Panics when the server, upload, or a read fails, a refused read
/// succeeds, or the inventory is read other than once per input.
#[tokio::test]
async fn evidence_metadata_is_resolved_once() {
    let registered = Registered::start().await;
    let server = &registered.server;
    let tenant = server.data_tenant_id();
    let Bootstrap::User { jwt, .. } = server
        .bootstrap_user("hosted-metadata-writer", &["editor"])
        .await
        .expect("writer bootstraps")
    else {
        panic!("user bootstrap returned a non-user principal");
    };
    let document = br#"{"answer":1}"#;
    crate::pg_verification_routes::upload_file(server, &jwt, document).await;
    let doc = SlotName::new("doc").expect("slot");
    let file = InputDeclaration::File {
        sha256: Sha256Hex::digest(document),
        size_bytes: document.len() as u64,
        format: skald_spec::MediaFormat::Json,
        filename: None,
    };
    let declared = Arc::new(BTreeMap::from([
        (doc.clone(), file.clone()),
        (notes(), registered.file("notes.txt")),
    ]));
    let executor = PrincipalId::new(uuid::Uuid::now_v7());
    let caller = Caller {
        executed_by: Some(executor),
        ..user(tenant, [Permission::card_read()])
    };
    let evidence = TenantEvidence::new(
        server.state().clone(),
        caller,
        HostedLimits::default(),
        wyrd_runtime::resources::ByteLedger::new(u64::MAX),
    );
    let before = server
        .bifrost_read_decision_count()
        .await
        .expect("read decisions count");

    evidence
        .admit(&BTreeMap::new(), &declared)
        .await
        .expect("both inputs are admitted");
    for _ in 0..2 {
        let (read, _) = evidence.read(&file).await.expect("the File reads");
        assert_eq!(read.bytes, document);
        let (bytes, _) = evidence
            .artifact(&registered.card, Some("notes.txt"))
            .await
            .expect("the Artifact file reads");
        assert_eq!(bytes.bytes, NOTES);
    }
    assert_eq!(
        evidence
            .select(&registered.card, None)
            .await
            .expect("the Artifact selects"),
        ["check.py", "notes.txt"]
    );
    evidence
        .media(&registered.file("notes.txt"))
        .await
        .expect("text is native media");
    let cited = Citation {
        slot: doc.clone(),
        locator: Locator::Lines { start: 1, end: 1 },
    };
    assert_eq!(
        evidence
            .cited(&BTreeSet::from([doc.clone()]), vec![cited.clone()])
            .await
            .expect("citations check"),
        vec![cited]
    );
    let mounted = |value: InputDeclaration, slot: &SlotName| CallInput {
        value: CallValue::Evidence(value),
        sources: BTreeSet::from([slot.clone()]),
    };
    let workspace = workspaces(HostedLimits::default())
        .open(
            &BTreeMap::from([
                ("/inputs/doc.json".to_owned(), mounted(file.clone(), &doc)),
                (
                    "/inputs/notes.txt".to_owned(),
                    mounted(registered.file("notes.txt"), &notes()),
                ),
            ]),
            &evidence,
        )
        .await
        .expect("the workspace opens");
    for path in ["/inputs/doc.json", "/inputs/notes.txt"] {
        workspace.read(path, 0).expect("the input mounts");
    }

    let after = server
        .bifrost_read_decision_count()
        .await
        .expect("read decisions count");
    assert_eq!(
        after - before,
        1,
        "one inventory read for the File and none for the Artifact"
    );
    let executed = server
        .retained_audit_records(
            tenant,
            "operation",
            &format!(
                "operation = 'bifrost.query.read_decision' AND sub_principal_id = '{executor}'"
            ),
        )
        .await
        .expect("retained audit reads");
    assert_eq!(
        executed.len(),
        1,
        "the File read ran as the executor: {executed:?}"
    );

    let other = server
        .seed_tenant("hosted-metadata-other")
        .await
        .expect("the other tenant seeds");
    for (tenant, permissions) in [(tenant, Vec::new()), (other, vec![Permission::card_read()])] {
        let refused = registered.evidence(tenant, permissions, HostedLimits::default());
        let error = refused
            .artifact(&registered.card, Some("notes.txt"))
            .await
            .expect_err("the Card read is refused");
        assert!(matches!(error, BlockError::Terminal { .. }), "{error}");
    }
    let foreign = registered.evidence(other, [Permission::card_read()], HostedLimits::default());
    let error = foreign
        .read(&file)
        .await
        .expect_err("another tenant holds no such File");
    assert!(matches!(error, BlockError::Retryable { .. }), "{error}");
}

/// Refuses every registry lookup; the Agent here is authored inline.
pub(crate) struct NoRegistry;

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
pub(crate) fn answer(text: Option<&str>, calls: &[(&str, Value)]) -> ProviderResponse {
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
pub(crate) fn reviewer(tools: &[&str]) -> AgentCall {
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
            value: CallValue::Json(value.into()),
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
        limits,
        wyrd_runtime::resources::ByteLedger::new(u64::MAX),
    ));
    let pinned = Arc::new(BTreeMap::from([(notes(), declaration.clone())]));
    evidence
        .admit(&BTreeMap::new(), &pinned)
        .await
        .expect("the notes are admitted");
    let tools = HostedTools::new(
        state,
        caller,
        Arc::clone(&evidence),
        Arc::new(workspaces(limits)),
        Arc::new(
            Sandbox::start(worker(), limits, &roomy())
                .await
                .expect("pool starts"),
        ),
        limits,
    );
    let read = json!({"path": "/inputs/notes.txt", "start": 0});
    let code = json!({"source": "from pathlib import Path\nPath('/inputs/notes.txt').read_text()"});
    // Python catches the nested denial and repairs it with an allowed read.
    let nested = json!({"source": format!(
        "import json\ntry:\n    await call_tool('bifrost.query', {{'sql': '{SPANS}'}})\n    denied = {{}}\nexcept RuntimeError as error:\n    denied = json.loads(str(error))\nrows = (await call_tool('bifrost.query', {{'sql': '{LOGS}'}}))['rows']\nprint(json.dumps({{'code': denied.get('code'), 'value': 'rows' in denied, 'rows': rows}}))"
    )});
    let cited = json!({"slot": "notes", "locator": {"lines": {"start": 1, "end": 1}}});
    let mock = Scripted::new([
        answer(
            None,
            &[
                ("bifrost_query", json!({"sql": LOGS})),
                ("bifrost_query", json!({"sql": SPANS})),
                ("bifrost_query", json!({"sql": MIXED})),
                ("code_run", code.clone()),
                ("fs_read", read.clone()),
                ("code_run", nested.clone()),
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
    ]);
    let mut providers = ProviderRegistry::new();
    providers.register(Arc::new(mock.clone()));
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
    let calls = ToolCallsOwner::default();
    let step = async |action: Action, calls: &ToolCallsOwner| {
        let output = ActionExecutor::execute(
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
                attempt: 1,
                item_index: None,
                iteration_index: None,
                tool_calls: calls.handle(),
            },
        )
        .await;
        calls.settle().await;
        output
    };
    let output = step(Action::Agent(declared.clone()), &calls)
        .await
        .expect("the step runs");
    let records = calls.settled();
    let error = |tool: BuiltinTool, args: &Value| {
        let record = records
            .iter()
            .find(|record| record.tool == tool && &record.arguments == args)
            .expect("the call is recorded");
        record.error.clone()
    };
    assert_eq!(records.len(), 8);
    let nested_records: Vec<_> = records[5..]
        .iter()
        .map(|record| {
            (
                record.tool,
                serde_json::to_value(record.recipient).expect("recipient"),
                record.arguments["sql"].clone(),
                record
                    .error
                    .as_deref()
                    .map(|error| error.contains("query forbidden")),
            )
        })
        .collect();
    assert_eq!(
        nested_records,
        [
            (BuiltinTool::CodeRun, json!("model"), Value::Null, None),
            (
                BuiltinTool::BifrostQuery,
                json!("code"),
                json!(SPANS),
                Some(true)
            ),
            (BuiltinTool::BifrostQuery, json!("code"), json!(LOGS), None),
        ],
        "nested calls keep the accepted authority's narrow scope"
    );
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
    let shown = mock.tool_results();
    assert_eq!(
        printed(shown.last().expect("the nested program's result")),
        json!({"code": "WYRD_VALA_403_QUERY_FORBIDDEN", "value": false, "rows": []}),
        "the caught denial is the safe problem with no rows, and the repair succeeds"
    );

    let check = Check::Agent(reviewer(&[]));
    let checked = ToolCallsOwner::default();
    let verdict = CheckExecutor::execute(
        executor,
        BlockCall {
            id,
            position: Position::VerifierCheck,
            block: &check,
            inputs: document(output.output.clone()),
            workspace: BTreeMap::new(),
            deadline: Instant::now() + Duration::from_secs(60),
            attempt: 1,
            item_index: None,
            iteration_index: None,
            tool_calls: checked.handle(),
        },
    )
    .await;
    checked.settle().await;
    let verdict = verdict.expect("the check runs").verdict;
    assert_eq!(verdict.outcome, VerificationVerdict::Passed);
    assert_eq!(
        serde_json::to_value(&verdict.citations).expect("citations"),
        json!([cited])
    );

    let refused = step(Action::Agent(declared), &ToolCallsOwner::default())
        .await
        .expect_err("cards.list was not declared");
    assert!(matches!(refused, BlockError::Terminal { .. }), "{refused}");
}

/// Agent and Prompt Card resolution is the caller's audited `verifier:run`
/// on the executing Verifier, never `cards:read`: a caller holding only that
/// grant resolves the exact Agent and its Prompt, which it could not read as
/// Cards. Another kind, a mismatched UID, a `cards:read`-only caller,
/// another tenant, or a deleted Agent is unavailable, and no caller at all
/// refuses.
///
/// # Panics
/// Panics when a lookup resolves what its caller may not run, or refuses
/// what it may.
#[tokio::test]
async fn card_resolution_is_the_verifier_run() {
    let registered = Registered::start().await;
    let state = registered.server.state().clone();
    let tenant = registered.server.data_tenant_id();
    let other = registered
        .server
        .seed_tenant("hosted-registry-other")
        .await
        .expect("the other tenant seeds");
    let Bootstrap::User { jwt, .. } = registered
        .server
        .bootstrap_user("hosted-registry-writer", &["editor"])
        .await
        .expect("writer bootstraps")
    else {
        panic!("user bootstrap returned a non-user principal");
    };
    let uid = crate::pg_principal_roles::register_agent(&registered.server, &jwt, "reviewer").await;
    let exact = |kind: &str, name: &str, uid: Option<&CardUid>| -> CardRef {
        serde_json::from_value(json!({"kind": kind, "name": name, "version": "1.0.0", "uid": uid}))
            .expect("reference")
    };
    let agent = exact("Agent", "reviewer", Some(&uid));
    let prompt = exact("Prompt", "reviewer-prompt", None);
    let verifier = CardUid::from_uuid(uuid::Uuid::now_v7()).expect("uid");
    let registry =
        |caller: Option<Caller>| TenantRegistry::new(state.clone(), caller, verifier.clone());
    let runner = user(tenant, [Permission::verifier_run(verifier.clone())]);
    assert!(
        state
            .auth
            .permission_check
            .check(&runner.principal, &Permission::card_read())
            .into_result()
            .is_err(),
        "the runner may not read Cards"
    );
    AgentCardResolver::resolve(&registry(Some(runner.clone())), &agent)
        .await
        .expect("the exact Agent resolves");
    PromptCardResolver::resolve(&registry(Some(runner.clone())), &prompt)
        .await
        .expect("its exact Prompt resolves");

    let mismatched = exact("Agent", "reviewer", registered.card.uid.as_ref());
    for (caller, reference) in [
        (Some(runner.clone()), &registered.card),
        (Some(runner), &mismatched),
        (Some(user(tenant, [Permission::card_read()])), &agent),
        (
            Some(user(other, [Permission::verifier_run(verifier.clone())])),
            &agent,
        ),
    ] {
        let refused = AgentCardResolver::resolve(&registry(caller), reference)
            .await
            .expect_err("unavailable");
        assert!(
            matches!(&refused, BlockError::Unavailable { reason } if reason.contains("is unavailable")),
            "{refused}"
        );
    }
    let refused = AgentCardResolver::resolve(&registry(None), &agent)
        .await
        .expect_err("no caller");
    assert!(matches!(refused, BlockError::Terminal { .. }), "{refused}");

    let mut conn = registered
        .server
        .tenant_conn_for(tenant)
        .await
        .expect("tenant connection");
    sqlx::query("UPDATE wyrd.cards SET status = 'deleted' WHERE card_uid = $1")
        .bind(uid.as_uuid())
        .execute(&mut **conn.transaction())
        .await
        .expect("the Agent is deleted");
    conn.commit().await.expect("deletion commits");
    let deleted = AgentCardResolver::resolve(
        &registry(Some(user(
            tenant,
            [Permission::verifier_run(verifier.clone())],
        ))),
        &agent,
    )
    .await
    .expect_err("a deleted Agent is unavailable");
    assert!(
        matches!(deleted, BlockError::Unavailable { .. }),
        "{deleted}"
    );
}

/// Hosted limits of one workspace, no prewarmed worker, and a short code
/// timeout, so a single held workspace saturates the workspaces and an
/// operation waiting for process memory gives up quickly.
pub(crate) fn saturable() -> HostedLimits {
    HostedLimits {
        monty_min_workers: 0,
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
/// whose workspace finds process scratch full. None is reported as a
/// retryable or terminal block failure, the Agent never answers the model
/// with the refusal, and the refused workspace open never waits. Once the
/// occupied process memory and scratch drop, the same code runs.
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
        limits,
    ));
    let pinned = Arc::new(BTreeMap::from([(notes(), registered.file("notes.txt"))]));
    evidence
        .admit(&BTreeMap::new(), &pinned)
        .await
        .expect("the notes are admitted");
    let sandbox = Arc::new(
        Sandbox::start(worker(), limits, &roomy())
            .await
            .expect("pool starts"),
    );
    let process = roomy();
    let workspaces = Arc::new(Workspaces::new(limits, &process, std::env::temp_dir()));
    let far = Instant::now() + Duration::from_secs(30);
    // Holding all of this sandbox's process memory refuses each operation's
    // waiting bytes.
    let held = sandbox.occupy_for_test(0);

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
                value: CallValue::Json(json!(1).into()),
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
                attempt: 1,
                item_index: None,
                iteration_index: None,
                tool_calls: ToolCallsOwner::default().handle(),
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
                attempt: 1,
                item_index: None,
                iteration_index: None,
                tool_calls: ToolCallsOwner::default().handle(),
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
    let step_recording = async |tools: &[&str], calls: &ToolCallsOwner| {
        let action = Action::Agent(reviewer(tools));
        let output = ActionExecutor::execute(
            &agent,
            BlockCall {
                id: &id,
                position: Position::Step,
                block: &action,
                inputs: document(json!("the notes")),
                workspace: notes_workspace(&registered),
                deadline: Instant::now() + Duration::from_secs(30),
                attempt: 1,
                item_index: None,
                iteration_index: None,
                tool_calls: calls.handle(),
            },
        )
        .await;
        calls.settle().await;
        output.map(drop)
    };
    let step = async |tools: &[&str]| step_recording(tools, &ToolCallsOwner::default()).await;
    busy(step(&["code.run"]).await, "an Agent's code.run");
    assert_eq!(
        mock.remaining(),
        1,
        "the refusal ends the loop before the model is called again"
    );

    let scratch = process.scratch();
    let taken = scratch.charge_infallible(scratch.available());
    // A waiting open would never finish while `taken` is held.
    busy(step(&["fs.read"]).await, "an Agent's workspace");

    drop(taken);
    drop(held);
    let ran = act().await.expect("capacity returns once released");
    assert_eq!(ran.output, json!(2));

    step(&["code.run"])
        .await
        .expect("the answer the refused step left unread finishes a step");
    // Two programs at once with 4 KiB free beside the one idle worker: the
    // short catching program is admitted once that worker is reaped for it,
    // and the sibling, whose 8 KiB source is more than the free waiting
    // bytes, is refused at once and stops the step busy. The admitted one,
    // though it catches `RuntimeError` around its nested read, cannot turn
    // the refusal into a result.
    assert_eq!(
        sandbox.charged_worker_pids().len(),
        1,
        "one worker served every step so far"
    );
    let one_session = sandbox.occupy_for_test(4096);
    let catching = "try:\n    await call_tool('fs.read', {'path': '/inputs/notes.txt', 'start': 0})\nexcept RuntimeError:\n    pass\n'caught'";
    mock.push_response(answer(
        None,
        &[
            ("code_run", json!({"source": catching})),
            (
                "code_run",
                json!({"source": format!("# {}\n1", "x".repeat(8192))}),
            ),
        ],
    ));
    busy(
        step(&["code.run", "fs.read"]).await,
        "a program beside a refused sibling",
    );
    assert_eq!(mock.remaining(), 0, "no model turn follows the refusal");
    drop(one_session);
    mock.push_response(answer(None, &[("code_run", json!({"source": catching}))]));
    mock.push_response(answer(Some("{}"), &[]));
    let calls = ToolCallsOwner::default();
    step_recording(&["code.run", "fs.read"], &calls)
        .await
        .expect("the same program runs once capacity allows");
    assert_eq!(
        mock.remaining(),
        0,
        "the program ran, then the model answered"
    );
    let ran: Vec<_> = calls
        .settled()
        .into_iter()
        .map(|record| (record.tool, record.error, record.output_sha256.is_some()))
        .collect();
    assert_eq!(
        ran,
        [
            (BuiltinTool::CodeRun, None, true),
            (BuiltinTool::FsRead, None, true),
        ],
        "the program and its nested read both ran and returned"
    );
}

/// A queued execution that process memory cannot admit returns to the
/// queue with its attempt refunded and runs once memory frees.
///
/// The test occupies process memory short of one session envelope. The
/// queued execution waits out its code timeout, is refused busy, and is
/// deferred rather than failed, retried, or judged; once the memory frees,
/// it completes on its first counted attempt with its real verdict.
///
/// # Panics
/// Panics when the server, seeding, or runtime fails, the execution is never
/// deferred, settles while the memory is occupied, or settles other than
/// `completed` on one attempt.
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
    let seed = VerificationFixture::provision(&state, tenant)
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
    let sandbox = Arc::clone(
        state
            .verification
            .blocks()
            .expect("blocks are hosted")
            .sandbox(ExecutionMode::Queued),
    );
    // Process memory short of one session envelope, with room left for the
    // Scribe and Oracle work that shares it, makes the queued operation
    // wait until its code timeout and then defer.
    assert!(Sandbox::envelope_bytes(&saturable()).expect("the envelope fits u64") > 64 << 20);
    let held = sandbox.occupy_for_test(64 << 20);
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
    while !logs.text().lines().any(|line| {
        line.contains("deferred by admission backpressure") && line.contains(&deferred.to_string())
    }) {
        assert!(Instant::now() < deadline, "the adder was never deferred");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let waiting = seed.execution(deferred).await.expect("execution reads");
    assert_eq!(
        waiting.result_id, None,
        "a deferral judges nothing: {waiting:?}"
    );
    drop(held);
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

    assert_eq!(
        (settled.status.as_str(), settled.attempts),
        ("completed", 1),
        "a deferral spends no attempt: {settled:?}"
    );
    assert!(settled.result_id.is_some(), "{settled:?}");
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

/// A Verifier with media slots `a` and `b`, each accepting `formats` up to
/// `max_bytes`, whose one requirement holds when `a` extracts in two
/// separate steps; evidence omitted for `b` stays missing, and nothing reads
/// it.
///
/// # Panics
/// Panics when the static spec is invalid.
fn two_extracts(formats: &[&str], max_bytes: Option<u64>) -> Arc<VerifierSpec> {
    let slot = json!({ "type": "media", "formats": formats, "max_bytes": max_bytes });
    let spec: VerifierSpec = serde_json::from_value(json!({
        "inputs": { "a": slot.clone(), "b": slot },
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
    let blocks = HostedBlocks::start(worker(), tight_files(), &roomy(), std::env::temp_dir())
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
        &roomy(),
        std::env::temp_dir(),
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
            let verifier = CardUid::from_uuid(uuid::Uuid::now_v7()).expect("uid");
            blocks
                .engine(
                    state,
                    Execution {
                        caller,
                        verifier: &verifier,
                        routes: &routes,
                        subject: GatewayCallContext::default(),
                        defer_unlanded_traces: false,
                        drift: DriftChecks::new(None, BTreeMap::new(), BTreeMap::new()),
                        clock: BlockClock::default(),
                        execution: None,
                        mode: ExecutionMode::Direct,
                        pull_request: None,
                        envelopes: Envelopes::default(),
                    },
                )
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
    let seed = VerificationFixture::provision(&state, tenant)
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
                    "a": { "type": "media", "formats": ["text"] },
                    "b": { "type": "media", "formats": ["text"] }
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

/// A queued execution whose exact Agent, or that Agent's Prompt, was
/// deleted before its Agent step starts settles `errored` with
/// `capability_unavailable` on its first attempt under a submitter holding
/// only `verifier:run` on the Verifier: the predecessor code step's
/// committed checkpoint stays readable, the Agent step leaves no invocation,
/// and no verdict is judged. The hosted engine's native provider registry
/// is empty and the test gateway reaches no provider, so no model call can
/// happen.
///
/// # Panics
/// Panics when the server, seeding, or runtime fails, or an execution
/// settles otherwise or loses its predecessor checkpoint.
#[tokio::test]
async fn deleted_agent_definition_errors_queued_execution_as_unavailable() {
    let registered = Registered::start().await;
    let tenant = registered.server.data_tenant_id();
    registered
        .server
        .ensure_builtin_tables_for_test(tenant)
        .await
        .expect("tenant receives every built-in table");
    let state = registered.server.state().clone();
    let seed = VerificationFixture::provision(&state, tenant)
        .await
        .expect("tenant provisions");
    let (subject, _) = seed
        .service("unavailable-subject")
        .await
        .expect("subject registers");
    let Bootstrap::User { jwt, .. } = registered
        .server
        .bootstrap_user("unavailable-writer", &["editor"])
        .await
        .expect("writer bootstraps")
    else {
        panic!("user bootstrap returned a non-user principal");
    };
    let mut executions = Vec::new();
    for deleted_kind in ["Agent", "Prompt"] {
        let name = format!("unavailable-{}", deleted_kind.to_lowercase());
        let agent =
            crate::pg_principal_roles::register_agent(&registered.server, &jwt, &name).await;
        let verifier = seed
            .verifier(
                &format!("{name}-verifier"),
                &json!({
                    "steps": [
                        { "id": "prepare",
                          "action": { "kind": "code", "entrypoint": "f",
                                      "source": { "inline": "def f():\n    return 1" } } },
                        { "id": "review",
                          "action": { "kind": "agent",
                                      "agent": { "kind": "Agent", "name": name, "version": "1.0.0", "uid": agent },
                                      "inputs": { "prepared": "steps.prepare.output" } },
                          "requirements": [{ "id": "reviewed", "statement": "The Agent reviews.",
                              "check": { "kind": "assert", "path": "steps.review.output",
                                         "operator": "is_not_null" } }] }
                    ]
                }),
            )
            .await
            .expect("Verifier registers");
        let deleted = if deleted_kind == "Agent" {
            name.clone()
        } else {
            format!("{name}-prompt")
        };
        let mut conn = registered
            .server
            .tenant_conn_for(tenant)
            .await
            .expect("tenant connection");
        sqlx::query("UPDATE wyrd.cards SET status = 'deleted' WHERE name = $1")
            .bind(&deleted)
            .execute(&mut **conn.transaction())
            .await
            .expect("the definition is deleted");
        conn.commit().await.expect("deletion commits");
        let runner = seed
            .clone()
            .with_actor_permissions(PermissionSet::from_iter([Permission::verifier_run(
                verifier.clone(),
            )]));
        executions.push(
            runner
                .submit(&verifier, &subject, BTreeMap::new())
                .await
                .expect("execution queues"),
        );
    }

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
    for &execution in &executions {
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

    for (execution, row) in executions.into_iter().zip(settled) {
        assert_eq!(
            (row.status.as_str(), row.attempts, row.error_code.as_deref()),
            ("errored", 1, Some("capability_unavailable")),
            "{row:?}"
        );
        assert!(row.result_id.is_none(), "no verdict is judged: {row:?}");
        let accepted = seed
            .execution_state(&state, execution)
            .await
            .expect("the state reads")
            .expect("the execution claimed its state");
        let invocations: Vec<(&str, bool)> = accepted
            .invocations
            .iter()
            .map(|entry| {
                (
                    entry.producer.step_id.as_str(),
                    matches!(entry.settlement, Some(InvocationSettlement::Action { .. })),
                )
            })
            .collect();
        assert_eq!(
            invocations,
            [("prepare", true)],
            "only the predecessor's settled checkpoint remains"
        );
    }
}

/// A scripted model that keeps every request it receives.
#[derive(Clone)]
struct Scripted {
    /// The scripted answers, in order.
    mock: MockProvider,
    /// Every received request as JSON, in order.
    seen: Arc<std::sync::Mutex<Vec<Value>>>,
}

impl Scripted {
    /// A model answering `responses` in order.
    fn new(responses: impl IntoIterator<Item = ProviderResponse>) -> Self {
        let mock = MockProvider::new(ProviderName::OpenAi);
        for response in responses {
            mock.push_response(response);
        }
        Self {
            mock,
            seen: Arc::default(),
        }
    }

    /// The requests received so far.
    ///
    /// # Panics
    /// Panics when the lock is poisoned.
    fn seen(&self) -> Vec<Value> {
        self.seen.lock().expect("requests").clone()
    }

    /// The JSON content of every tool message in the latest request, in
    /// order: each tool result the model was shown.
    ///
    /// # Panics
    /// Panics when no request arrived or a tool message is not JSON text.
    fn tool_results(&self) -> Vec<Value> {
        let last = self.seen().pop().expect("the model was called");
        last["body"]["messages"]
            .as_array()
            .expect("chat messages")
            .iter()
            .filter(|message| message["role"] == "tool")
            .map(|message| {
                serde_json::from_str(message["content"].as_str().expect("text content"))
                    .expect("JSON tool result")
            })
            .collect()
    }
}

#[async_trait::async_trait]
impl skald_runtime::provider::Provider for Scripted {
    /// Keep `request`, then answer the next scripted response.
    ///
    /// # Errors
    /// Returns the mock's error once the script is spent.
    ///
    /// # Panics
    /// Panics when the request cannot serialize or the lock is poisoned.
    async fn send(
        &self,
        request: &skald_spec::ProviderRequest,
    ) -> Result<ProviderResponse, skald_providers::ProviderError> {
        self.seen
            .lock()
            .expect("requests")
            .push(serde_json::to_value(request).expect("request serializes"));
        self.mock.send(request).await
    }

    /// Delegates to the mock.
    ///
    /// # Errors
    /// Returns the mock's stream error.
    async fn stream(
        &self,
        request: &skald_spec::ProviderRequest,
    ) -> Result<skald_providers::ProviderStream, skald_providers::ProviderError> {
        self.mock.stream(request).await
    }

    /// The mock's name.
    fn name(&self) -> ProviderName {
        self.mock.name()
    }
}

/// A model turn asking `code.run` to run `source`.
fn code_run(source: &str) -> ProviderResponse {
    answer(None, &[("code_run", json!({"source": source}))])
}

/// A hosted Agent over the notes file, running as one user of the server's
/// tenant.
struct CodeAgent {
    /// The Agent executor.
    executor: AgentExecutor,
    /// Its model.
    model: Scripted,
    /// Its sandbox.
    sandbox: Arc<Sandbox>,
}

impl CodeAgent {
    /// An Agent whose user holds `permissions`, under `limits`, answering
    /// `responses` in order.
    ///
    /// # Panics
    /// Panics when the pool does not start.
    async fn start(
        registered: &Registered,
        permissions: &[Permission],
        limits: HostedLimits,
        responses: impl IntoIterator<Item = ProviderResponse>,
    ) -> Self {
        let tenant = registered.server.data_tenant_id();
        let evidence = Arc::new(registered.evidence(tenant, permissions.iter().cloned(), limits));
        let pinned = Arc::new(BTreeMap::from([(notes(), registered.file("notes.txt"))]));
        evidence
            .admit(&BTreeMap::new(), &pinned)
            .await
            .expect("the notes are admitted");
        let sandbox = Arc::new(
            Sandbox::start(worker(), limits, &roomy())
                .await
                .expect("pool starts"),
        );
        let model = Scripted::new(responses);
        let mut providers = ProviderRegistry::new();
        providers.register(Arc::new(model.clone()));
        let executor = AgentExecutor::new(
            Arc::new(providers),
            Arc::new(NoRegistry),
            Arc::new(NoRegistry),
        )
        .with_toolset(Arc::new(HostedTools::new(
            registered.server.state().clone(),
            user(tenant, permissions.iter().cloned()),
            Arc::clone(&evidence),
            Arc::new(workspaces(limits)),
            Arc::clone(&sandbox),
            limits,
        )))
        .with_evidence(evidence as Arc<dyn EvidenceReader>);
        Self {
            executor,
            model,
            sandbox,
        }
    }

    /// Run one Agent step declaring `tools` over the notes workspace,
    /// recording into `calls` and returning once its host work has settled.
    ///
    /// # Errors
    /// Returns the step's [`BlockError`].
    async fn step(
        &self,
        registered: &Registered,
        tools: &[&str],
        calls: &ToolCallsOwner,
    ) -> Result<Value, BlockError> {
        self.call(registered, "reduce", reviewer(tools), calls)
            .await
    }

    /// Run `call` as Agent step `id` over the notes workspace, recording
    /// into `calls` and returning once its host work has settled; steps
    /// sharing `id` share its Python budget.
    ///
    /// # Errors
    /// Returns the step's [`BlockError`].
    ///
    /// # Panics
    /// Panics when `id` is not a valid task id.
    async fn call(
        &self,
        registered: &Registered,
        id: &str,
        call: AgentCall,
        calls: &ToolCallsOwner,
    ) -> Result<Value, BlockError> {
        let id = TaskId::new(id).expect("task id");
        let action = Action::Agent(call);
        let output = ActionExecutor::execute(
            &self.executor,
            BlockCall {
                id: &id,
                position: Position::Step,
                block: &action,
                inputs: document(json!("the notes")),
                workspace: notes_workspace(registered),
                deadline: Instant::now() + Duration::from_secs(30),
                attempt: 1,
                item_index: None,
                iteration_index: None,
                tool_calls: calls.handle(),
            },
        )
        .await;
        calls.settle().await;
        output.map(|output| output.output)
    }
}

/// The JSON a `code.run` program printed, from its tool result.
///
/// # Panics
/// Panics when the output is not one JSON document.
fn printed(result: &Value) -> Value {
    serde_json::from_str(result["output"].as_str().expect("printed output"))
        .expect("the program printed JSON")
}

/// `call_tool` reaches exactly the Agent's declared tools through the one
/// recorded path.
///
/// A program awaits `fs.list`, then reads the file it named: the dependent
/// result reaches Python, both children are recorded for `code` after the
/// outer `code.run` for the `model`, and the model sees only the outer
/// result. An Experiment's structured `u64::MAX` seed reaches Python as the
/// exact integer. Undeclared, unknown, and recursive targets and unconvertible
/// arguments are refused catchably with their codes before anything is
/// dispatched or recorded; arguments the tool itself rejects are recorded
/// failures. An undefined host function stays an ordinary `NameError`. An
/// uncaught error after a delivered read fails the step with no later model
/// turn; the read keeps its digest and a call Python never awaited is not
/// recorded as returned. An Agent declaring only `code.run` reaches no other
/// tool, and authored code has no bridge.
///
/// # Panics
/// Panics when a refused call dispatches, a value is lost or misrecorded, a
/// failed program is answered, or authored code reaches a tool.
#[tokio::test(flavor = "multi_thread")]
async fn code_bridge_preserves_declared_tool_boundaries() {
    const PROGRAM: &str = r"import json
async def code(name, arguments):
    try:
        await call_tool(name, arguments)
        return 'ok'
    except RuntimeError as error:
        return json.loads(str(error))['code']
listing = await call_tool('fs.list', {'path': '/inputs'})
read = await call_tool('fs.read', {'path': listing['files'][0]['path'], 'start': 0})
experiment = await call_tool('cards.get', {'card_ref': {'kind': 'Experiment', 'name': 'seeded', 'version': '1.0.0'}})
seed = experiment['card']['spec']['default_parameters']['seed']['value']
codes = {
    'undeclared': await code('cards.list', {}),
    'recursive': await code('code.run', {'source': '1'}),
    'unknown': await code('fs.nope', {}),
    'non_string_key': await code('fs.read', {1: 'x'}),
    'non_finite': await code('fs.read', {'path': float('nan')}),
    'not_an_object': await code('fs.read', ['/inputs/notes.txt']),
    'wrong_type': await code('fs.read', {'path': 1}),
    'identity_field': await code('fs.read', {'path': '/inputs/notes.txt', 'tenant': 'other'}),
}
try:
    other_host_function()
    codes['host_function'] = 'ok'
except NameError:
    codes['host_function'] = 'NameError'
print(json.dumps({'lines': [unit['text'] for unit in read['units']], 'seed': str(seed), 'max': seed == 2 ** 64 - 1, 'codes': codes}))
";
    const FAILING: &str = r"read = await call_tool('fs.read', {'path': '/inputs/notes.txt', 'start': 0})
pending = call_tool('fs.list', {'path': '/inputs'})
raise ValueError('after delivery')
";
    let registered = Registered::start().await;
    registered.seeded_experiment().await;
    let agent = CodeAgent::start(
        &registered,
        &[Permission::card_read(), Permission::tool_invoke_any()],
        HostedLimits {
            monty_min_workers: 0,
            ..HostedLimits::default()
        },
        [
            code_run(PROGRAM),
            answer(Some("{}"), &[]),
            code_run(FAILING),
            code_run("import json\ntry:\n    await call_tool('fs.read', {'path': '/inputs/notes.txt', 'start': 0})\nexcept RuntimeError as error:\n    print(json.loads(str(error))['code'])"),
            answer(Some("{}"), &[]),
        ],
    )
    .await;
    let calls = ToolCallsOwner::default();
    agent
        .step(
            &registered,
            &["code.run", "fs.list", "fs.read", "cards.get"],
            &calls,
        )
        .await
        .expect("the step runs");
    let results = agent.model.tool_results();
    assert_eq!(
        results.len(),
        1,
        "the model sees only code.run: {results:?}"
    );
    assert_eq!(
        printed(&results[0]),
        json!({
            "lines": ["alpha", "beta"],
            "seed": u64::MAX.to_string(),
            "max": true,
            "codes": {
                "undeclared": "WYRD_AGENT_404_TOOL_NOT_IN_AGENT",
                "recursive": "WYRD_AGENT_404_TOOL_NOT_IN_AGENT",
                "unknown": "WYRD_AGENT_404_TOOL_NOT_IN_AGENT",
                "non_string_key": "WYRD_TOOL_422_INPUT",
                "non_finite": "WYRD_TOOL_422_INPUT",
                "not_an_object": "WYRD_TOOL_422_INPUT",
                "wrong_type": "WYRD_TOOL_422_INPUT",
                "identity_field": "WYRD_TOOL_422_INPUT",
                "host_function": "NameError",
            },
        }),
        "{}",
        results[0]
    );
    let records = calls.settled();
    let shape: Vec<_> = records
        .iter()
        .map(|record| {
            (
                record.tool,
                serde_json::to_value(record.recipient).expect("recipient"),
                record.error.is_some(),
            )
        })
        .collect();
    assert_eq!(
        shape,
        [
            (BuiltinTool::CodeRun, json!("model"), false),
            (BuiltinTool::FsList, json!("code"), false),
            (BuiltinTool::FsRead, json!("code"), false),
            (BuiltinTool::CardsGet, json!("code"), false),
            (BuiltinTool::FsRead, json!("code"), true),
            (BuiltinTool::FsRead, json!("code"), true),
        ],
        "only admitted calls are recorded, in start order: {records:?}"
    );

    let seen = agent.model.seen().len();
    let failing = ToolCallsOwner::default();
    let failure = agent
        .step(&registered, &["code.run", "fs.list", "fs.read"], &failing)
        .await
        .expect_err("an uncaught program error fails the step");
    assert!(
        matches!(&failure, BlockError::Terminal { reason } if reason.contains("ValueError")),
        "{failure}"
    );
    assert_eq!(
        agent.model.seen().len(),
        seen + 1,
        "no model turn follows the failed program"
    );
    let records = failing.settled();
    assert_eq!(
        (records[0].tool, records[0].error.is_some()),
        (BuiltinTool::CodeRun, true),
        "{records:?}"
    );
    assert!(
        records[1].tool == BuiltinTool::FsRead
            && records[1].error.is_none()
            && records[1].output_sha256.is_some(),
        "the delivered read keeps its digest: {records:?}"
    );
    assert!(
        records[2..]
            .iter()
            .all(|record| record.tool == BuiltinTool::FsList
                && record.output_sha256.is_none()
                && record.returned_bytes == 0
                && record
                    .error
                    .as_deref()
                    .is_some_and(|error| error.starts_with("cancelled"))),
        "the never-awaited list is never recorded as returned: {records:?}"
    );

    let only_code = ToolCallsOwner::default();
    agent
        .step(&registered, &["code.run"], &only_code)
        .await
        .expect("the code-only step runs");
    assert_eq!(
        agent.model.tool_results()[0]["output"],
        json!("WYRD_AGENT_404_TOOL_NOT_IN_AGENT\n"),
        "declaring only code.run reaches no other tool"
    );
    assert_eq!(only_code.settled().len(), 1);

    let authored = agent
        .sandbox
        .run(
            DataTenantId::new_v7(),
            "def f():\n    return call_tool('fs.read', {'path': '/inputs/notes.txt'})",
            "f",
            &serde_json::Map::new(),
            Vec::new(),
            Instant::now() + Duration::from_secs(30),
        )
        .await
        .expect_err("authored code has no bridge");
    assert!(
        matches!(&authored, BlockError::Terminal { reason } if reason.contains("NameError")),
        "{authored}"
    );
}

/// Python and nested extraction share one `/work`: no lock recursion, no
/// overwrite, one write budget.
///
/// The program awaits `fs.extract` and reads its output back (it is visible
/// once awaited, and the suspended program held no lock extraction needed).
/// Extraction never replaces a file, whether extraction or Python wrote it.
/// Python then fills `/work` so the next extraction no longer fits and is
/// refused; a Python write past what is left is an `OSError` even though
/// this program alone wrote less than the limit, and a write of exactly
/// what is left lands.
///
/// # Panics
/// Panics when extraction deadlocks, overwrites, or either writer exceeds
/// the shared budget.
#[tokio::test(flavor = "multi_thread")]
async fn code_calls_share_workspace_mutations() {
    const LIMIT: u64 = 8192;
    const PROGRAM: &str = r"import json
from pathlib import Path
async def rule(arguments):
    try:
        await call_tool('fs.extract', arguments)
        return 'ok'
    except RuntimeError as error:
        return json.loads(str(error))['details']['rule']
notes = '/inputs/notes.txt'
await call_tool('fs.extract', {'path': notes, 'output_path': '/work/a.json'})
extracted = Path('/work/a.json').read_text()
size = len(extracted)
Path('/work/b.txt').write_text('mine')
report = {
    'visible': 'alpha' in extracted,
    'over_extraction': await rule({'path': notes, 'output_path': '/work/a.json'}),
    'over_python': await rule({'path': notes, 'output_path': '/work/b.txt'}),
    'b': Path('/work/b.txt').read_text(),
}
Path('/work/filler').write_text('x' * (LIMIT - 2 * size - 4 + 1))
report['no_room'] = await rule({'path': notes, 'output_path': '/work/c.json'})
try:
    Path('/work/d').write_text('x' * size)
    report['past_room'] = 'ok'
except OSError:
    report['past_room'] = 'OSError'
Path('/work/d').write_text('x' * (size - 1))
report['held'] = sum(len(Path('/work/' + name).read_text()) for name in ['a.json', 'b.txt', 'filler', 'd'])
print(json.dumps(report))
";
    let registered = Registered::start().await;
    let agent = CodeAgent::start(
        &registered,
        &[Permission::card_read(), Permission::tool_invoke_any()],
        HostedLimits {
            monty_min_workers: 0,
            work_write_max_bytes: LIMIT,
            ..HostedLimits::default()
        },
        [
            code_run(&PROGRAM.replace("LIMIT", &LIMIT.to_string())),
            answer(Some("{}"), &[]),
        ],
    )
    .await;
    let calls = ToolCallsOwner::default();
    tokio::time::timeout(
        Duration::from_secs(20),
        agent.step(&registered, &["code.run", "fs.extract"], &calls),
    )
    .await
    .expect("nested extraction never waits on the suspended program")
    .expect("the step runs");
    let results = agent.model.tool_results();
    assert_eq!(
        printed(&results[0]),
        json!({
            "visible": true,
            "over_extraction": "output_exists",
            "over_python": "output_exists",
            "b": "mine",
            "no_room": "work_budget",
            "past_room": "OSError",
            "held": LIMIT,
        }),
        "{}",
        results[0]
    );
}

/// [`reviewer`] declaring `tools` with tool concurrency `cap`.
///
/// # Panics
/// Never; [`reviewer`] is inline.
fn capped(tools: &[&str], cap: usize) -> AgentCall {
    let mut call = reviewer(tools);
    let InlineableRef::Inline(spec) = &mut call.agent else {
        unreachable!("reviewer is inline")
    };
    spec.run_config.tool_concurrency_cap = Some(cap);
    call
}

/// Concurrent Python futures share the Agent's tool concurrency and each
/// gets its own answer.
///
/// At caps 1 and 2, a program gathers four reads of different pieces of the
/// notes and two Bifrost queries. Each completes (the suspended `code.run`
/// holds no permit its children need), each future receives the result of
/// its own request, the two concurrent queries run under their own request
/// identities without colliding in the running-query registry, and records
/// keep start order. The peak of active operations at the shared boundary
/// is pinned with deterministic barriers in Vala's
/// `code_bridge_shares_the_recorded_path`.
///
/// # Panics
/// Panics when a cap deadlocks, a value reaches the wrong future, or records
/// lose order.
#[tokio::test(flavor = "multi_thread")]
async fn code_calls_share_non_code_concurrency() {
    const PROGRAM: &str = r"import asyncio, json
reads = [call_tool('fs.read', {'path': '/inputs/notes.txt', 'start': start}) for start in [1, 0, 1, 0]]
queries = [call_tool('bifrost.query', {'sql': 'SELECT severity_text FROM bifrost.logs.records'}) for _ in range(2)]
done = await asyncio.gather(*reads, *queries)
print(json.dumps({
    'reads': [[unit['text'] for unit in read['units']] for read in done[:4]],
    'rows': [query['rows'] for query in done[4:]],
}))
";
    let registered = Registered::start().await;
    let tenant = registered.server.data_tenant_id();
    registered
        .server
        .ensure_builtin_tables_for_test(tenant)
        .await
        .expect("tenant receives every built-in table");
    for cap in [1, 2] {
        let agent = CodeAgent::start(
            &registered,
            &[
                Permission::card_read(),
                Permission::tool_invoke_any(),
                Permission::bifrost_query_read(),
            ],
            HostedLimits {
                monty_min_workers: 0,
                ..HostedLimits::default()
            },
            [code_run(PROGRAM), answer(Some("{}"), &[])],
        )
        .await;
        let calls = ToolCallsOwner::default();
        tokio::time::timeout(
            Duration::from_secs(20),
            agent.call(
                &registered,
                "reduce",
                capped(&["code.run", "fs.read", "bifrost.query"], cap),
                &calls,
            ),
        )
        .await
        .unwrap_or_else(|_| panic!("cap {cap} completes"))
        .unwrap_or_else(|error| panic!("cap {cap}: {error}"));
        let results = agent.model.tool_results();
        assert_eq!(
            printed(&results[0]),
            json!({
                "reads": [["beta"], ["alpha", "beta"], ["beta"], ["alpha", "beta"]],
                "rows": [[], []],
            }),
            "cap {cap}: {}",
            results[0]
        );
        let order: Vec<_> = calls
            .settled()
            .iter()
            .map(|record| {
                (
                    record.tool,
                    record.arguments["start"].clone(),
                    record.error.clone(),
                )
            })
            .collect();
        let query = (BuiltinTool::BifrostQuery, Value::Null, None);
        assert_eq!(
            order,
            [
                (BuiltinTool::CodeRun, Value::Null, None),
                (BuiltinTool::FsRead, json!(1), None),
                (BuiltinTool::FsRead, json!(0), None),
                (BuiltinTool::FsRead, json!(1), None),
                (BuiltinTool::FsRead, json!(0), None),
                query.clone(),
                query,
            ],
            "cap {cap} keeps start order"
        );
    }
}

/// One request allowance spans direct calls, `code.run`, its nested calls,
/// and refused requests across attempts, and is never refunded.
///
/// With five requests, a first attempt spends four (an outer `code.run`,
/// one read, one unconvertible request, one undeclared target) and a second
/// spends the last on a direct read and still answers. A third attempt's
/// request is refused with the catalog exhaustion code and the model is
/// never asked again. In a fresh execution allowing two, a program that
/// catches `RuntimeError` around its over-budget request cannot continue:
/// the step fails with exhaustion, nothing past the limit is recorded, and
/// no later model work starts.
///
/// # Panics
/// Panics when a request is refunded or skipped, the final allowance is
/// refused, or exhaustion is caught.
#[tokio::test(flavor = "multi_thread")]
async fn agent_tool_budget_spans_origins_and_retries() {
    const SPEND: &str = r"import json
await call_tool('fs.read', {'path': '/inputs/notes.txt', 'start': 0})
for name, arguments in [('fs.read', {1: 'x'}), ('cards.list', {})]:
    try:
        await call_tool(name, arguments)
    except RuntimeError:
        pass
";
    const CATCH: &str = r"from pathlib import Path
await call_tool('fs.read', {'path': '/inputs/notes.txt', 'start': 0})
try:
    await call_tool('fs.read', {'path': '/inputs/notes.txt', 'start': 0})
except RuntimeError:
    pass
await call_tool('fs.read', {'path': '/inputs/notes.txt', 'start': 0})
";
    let registered = Registered::start().await;
    let read = json!({"path": "/inputs/notes.txt", "start": 0});
    let limits = |max_calls| HostedLimits {
        monty_min_workers: 0,
        agent_tool_max_calls: max_calls,
        ..HostedLimits::default()
    };
    let permissions = [Permission::card_read(), Permission::tool_invoke_any()];
    let agent = CodeAgent::start(
        &registered,
        &permissions,
        limits(5),
        [
            code_run(SPEND),
            answer(Some("{}"), &[]),
            answer(None, &[("fs_read", read.clone())]),
            answer(Some("{}"), &[]),
            answer(None, &[("fs_read", read.clone())]),
            answer(Some("{\"never\": true}"), &[]),
        ],
    )
    .await;
    let tools = ["code.run", "fs.read"];
    let calls = ToolCallsOwner::default();
    agent
        .step(&registered, &tools, &calls)
        .await
        .expect("the first attempt answers");
    assert_eq!(calls.settled().len(), 2, "refused requests leave no record");
    agent
        .step(&registered, &tools, &calls)
        .await
        .expect("the final allowance still answers");
    let exhausted = agent
        .step(&registered, &tools, &calls)
        .await
        .expect_err("a sixth request is refused");
    assert!(
        matches!(&exhausted, BlockError::Terminal { reason }
            if reason.starts_with("WYRD_VERIFICATION_422_TOOL_BUDGET_EXCEEDED")),
        "{exhausted}"
    );
    assert_eq!(
        agent.model.mock.remaining(),
        1,
        "no model turn follows exhaustion"
    );
    assert_eq!(calls.settled().len(), 3);

    let fresh = CodeAgent::start(
        &registered,
        &permissions,
        limits(3),
        [code_run(CATCH), answer(Some("{\"never\": true}"), &[])],
    )
    .await;
    let calls = ToolCallsOwner::default();
    let caught = fresh
        .step(&registered, &tools, &calls)
        .await
        .expect_err("exhaustion cannot be caught");
    assert!(
        matches!(&caught, BlockError::Terminal { reason }
            if reason.starts_with("WYRD_VERIFICATION_422_TOOL_BUDGET_EXCEEDED")),
        "{caught}"
    );
    assert_eq!(
        fresh.model.mock.remaining(),
        1,
        "no model turn follows exhaustion"
    );
    let records = calls.settled();
    assert_eq!(
        records.iter().map(|record| record.tool).collect::<Vec<_>>(),
        [
            BuiltinTool::CodeRun,
            BuiltinTool::FsRead,
            BuiltinTool::FsRead
        ],
        "a fresh execution starts with its own allowance: {records:?}"
    );
}

/// Released Oracle resources: no admission, memory, or peer slot held.
const RELEASED: wyrd_testing::server::BifrostQueryResourceSnapshot =
    wyrd_testing::server::BifrostQueryResourceSnapshot {
        admission_slots: 0,
        memory_bytes: 0,
        peer_slots: 0,
    };

/// Host waits end with their invocation and are charged as Python time.
///
/// A program catching `RuntimeError` awaits a nested query held after its
/// schema. Dropping the step cancels the query, which returns every Oracle
/// resource; the later read is never dispatched and both admitted calls
/// settle as cancelled, with no digest and no bytes. When the same wait
/// instead outlasts the action's Python budget, the time limit ends the
/// program, its query, and the step, which fails before the model sees
/// anything, though the program caught every `RuntimeError`; a retry of the
/// step gets none of the spent time back and stops the same way. A new
/// action then runs a query to completion on the same one-worker pool.
///
/// # Panics
/// Panics when a query outlives its invocation, a later call dispatches,
/// the model answers after a time stop, spent time is restored, or the
/// pool does not recover.
#[tokio::test(flavor = "multi_thread")]
async fn code_host_waits_obey_deadlines_and_cancellation() {
    const WAIT: &str = r"try:
    await call_tool('bifrost.query', {'sql': 'SELECT severity_text FROM bifrost.logs.records'})
except RuntimeError:
    pass
await call_tool('fs.read', {'path': '/inputs/notes.txt', 'start': 0})
";
    const QUERY: &str = "rows = (await call_tool('bifrost.query', {'sql': 'SELECT severity_text FROM bifrost.logs.records'}))['rows']\nprint(len(rows))";
    let registered = Registered::start().await;
    let server = &registered.server;
    server
        .ensure_builtin_tables_for_test(server.data_tenant_id())
        .await
        .expect("tenant receives every built-in table");
    let agent = CodeAgent::start(
        &registered,
        &[
            Permission::card_read(),
            Permission::tool_invoke_any(),
            Permission::bifrost_query_read(),
        ],
        HostedLimits {
            monty_min_workers: 0,
            code_timeout: Duration::from_secs(2),
            agent_code_budget: Duration::from_secs(2),
            ..HostedLimits::default()
        },
        [
            code_run(WAIT),
            code_run(WAIT),
            code_run("1"),
            code_run(QUERY),
            answer(Some("{}"), &[]),
        ],
    )
    .await;
    let tools = ["code.run", "fs.read", "bifrost.query"];

    server.stall_next_query_after_schema();
    let calls = ToolCallsOwner::default();
    let query = {
        let step = agent.call(&registered, "cancelled", reviewer(&tools), &calls);
        tokio::pin!(step);
        tokio::select! {
            ended = &mut step => panic!("the step ended before its query stalled: {ended:?}"),
            stalled = server.wait_query_schema_stall() => stalled.expect("the nested query stalls"),
        }
    };
    // The step future dropped with its block.
    server
        .wait_bifrost_query_resources_released(&query, RELEASED)
        .await
        .expect("cancelling the step releases the query");
    let cancelled: Vec<_> = calls
        .settled()
        .into_iter()
        .map(|record| {
            (
                record.tool,
                record.output_sha256,
                record.returned_bytes,
                record.error,
            )
        })
        .collect();
    let gone = Some("cancelled: the invocation ended before the call returned".to_owned());
    assert_eq!(
        cancelled,
        [
            (BuiltinTool::CodeRun, None, 0, gone.clone()),
            (BuiltinTool::BifrostQuery, None, 0, gone.clone()),
        ],
        "the later read never dispatches"
    );

    server.stall_next_query_after_schema();
    let calls = ToolCallsOwner::default();
    let waited = {
        let step = agent.call(&registered, "timed", reviewer(&tools), &calls);
        tokio::pin!(step);
        let query = tokio::select! {
            ended = &mut step => panic!("the step ended before its query stalled: {ended:?}"),
            stalled = server.wait_query_schema_stall() => stalled.expect("the nested query stalls"),
        };
        let ended = step.await;
        server
            .wait_bifrost_query_resources_released(&query, RELEASED)
            .await
            .expect("the time limit releases the query");
        ended
    };
    let stopped = |result: Result<Value, BlockError>, expected: &str| match result {
        Err(BlockError::Terminal { reason }) if reason.contains(expected) => {}
        other => panic!("the step must fail with {expected:?}, got {other:?}"),
    };
    stopped(waited, "time limit");
    assert_eq!(
        agent.model.seen().len(),
        2,
        "no model turn follows the stop"
    );
    let errors: Vec<_> = calls
        .settled()
        .into_iter()
        .map(|record| (record.tool, record.error.unwrap_or_default()))
        .collect();
    assert_eq!(
        errors,
        [
            (BuiltinTool::CodeRun, errors[0].1.clone()),
            (BuiltinTool::BifrostQuery, gone.clone().unwrap_or_default()),
        ],
        "the stop cancels the pending query and dispatches nothing more"
    );
    assert!(errors[0].1.contains("time limit"), "{errors:?}");
    let retried = ToolCallsOwner::default();
    stopped(
        agent
            .call(&registered, "timed", reviewer(&tools), &retried)
            .await,
        "budget is spent",
    );
    assert_eq!(agent.model.seen().len(), 3, "the retry asks the model once");
    assert!(
        retried.settled()[0]
            .error
            .as_deref()
            .is_some_and(|error| error.contains("budget is spent")),
        "a retry restores no time: {:?}",
        retried.settled()
    );

    let fresh = ToolCallsOwner::default();
    agent
        .call(&registered, "fresh", reviewer(&tools), &fresh)
        .await
        .expect("a new action runs");
    assert_eq!(
        agent.model.tool_results()[0]["output"],
        json!("0\n"),
        "the one worker serves a new action"
    );
}

/// Sandbox shutdown settles a program suspended on host work only once its
/// invocation ends.
///
/// While a program awaits a nested query held after its schema, shutdown
/// cannot settle by its deadline: the worker stays checked out. Ending the
/// step drops the query, which returns every Oracle resource, and the
/// worker's checkout; shutdown then settles and reaps, and later code is
/// refused busy.
///
/// # Panics
/// Panics when shutdown settles under a running program, the query outlives
/// its step, or the closed sandbox admits code.
#[tokio::test(flavor = "multi_thread")]
async fn code_shutdown_settles_pending_host_work() {
    let registered = Registered::start().await;
    let server = &registered.server;
    server
        .ensure_builtin_tables_for_test(server.data_tenant_id())
        .await
        .expect("tenant receives every built-in table");
    let agent = CodeAgent::start(
        &registered,
        &[
            Permission::card_read(),
            Permission::tool_invoke_any(),
            Permission::bifrost_query_read(),
        ],
        HostedLimits {
            monty_min_workers: 0,
            ..HostedLimits::default()
        },
        [code_run(
            "await call_tool('bifrost.query', {'sql': 'SELECT severity_text FROM bifrost.logs.records'})",
        )],
    )
    .await;
    server.stall_next_query_after_schema();
    let calls = ToolCallsOwner::default();
    let query = {
        let step = agent.call(
            &registered,
            "shutdown",
            reviewer(&["code.run", "bifrost.query"]),
            &calls,
        );
        tokio::pin!(step);
        let query = tokio::select! {
            ended = &mut step => panic!("the step ended before its query stalled: {ended:?}"),
            stalled = server.wait_query_schema_stall() => stalled.expect("the nested query stalls"),
        };
        assert!(
            !agent
                .sandbox
                .shutdown(Instant::now() + Duration::from_millis(300))
                .await,
            "a suspended program keeps its worker"
        );
        query
    };
    server
        .wait_bifrost_query_resources_released(&query, RELEASED)
        .await
        .expect("ending the step releases the query");
    assert!(
        agent
            .sandbox
            .shutdown(Instant::now() + Duration::from_secs(10))
            .await,
        "shutdown settles once the program ends"
    );
    let refused = agent
        .sandbox
        .run(
            DataTenantId::new_v7(),
            "def f():\n    return 1",
            "f",
            &serde_json::Map::new(),
            Vec::new(),
            Instant::now() + Duration::from_secs(5),
        )
        .await
        .expect_err("a closed sandbox admits nothing");
    assert!(matches!(refused, BlockError::Busy { .. }), "{refused}");
}
