//! Registered Artifact evidence and code through the hosted runtime's one
//! authorized loader, against a real bound server and the real Monty worker.
//!
//! The Artifact is registered through the SDK registration saga, so its Card
//! blob, inventory, and stored bytes are exactly what production reads.
//! Ignored in the fast lane because the code case needs the worker binary;
//! `mise run test:verification:sandbox` builds it and runs them.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
use std::time::Duration;

use secrecy::SecretString;
use serde_json::json;
use tokio::time::Instant;
use vala_eval::blocks::{
    ActionExecutor, BlockCall, BlockError, CallInput, CallValue, EvidenceReader, Position,
};
use wyrd_client::WyrdClient;
use wyrd_client::auth::AuthMiddleware;
use wyrd_client::cards::Cards;
use wyrd_client::config::ClientConfig;
use wyrd_client::transport::HttpTransport;
use wyrd_client::transport::config::HttpConfig;
use wyrd_client::transport::credential::ResolvedCredential;
use wyrd_loader::{build_registration_input, load};
use wyrd_runtime::permission::PermissionSet;
use wyrd_runtime::{Permission, Principal, PrincipalKind};
use wyrd_server::components::auth::Caller;
use wyrd_server::verification::hosted::HostedLimits;
use wyrd_server::verification::hosted::evidence::TenantEvidence;
use wyrd_server::verification::hosted::sandbox::{CodeExecutor, Sandbox};
use wyrd_server::verification::hosted::workspace::Workspaces;
use wyrd_spec::DataTenantId;
use wyrd_spec::auth::{PrincipalId, Sha256Hex};
use wyrd_spec::card::blocks::{Action, CodeCall, CodeRuntime, CodeSource};
use wyrd_spec::envelope::CardKind;
use wyrd_spec::ids::{CardUid, SlotName};
use wyrd_spec::reference::{CardRef, Ref};
use wyrd_spec::request_id::RequestId;
use wyrd_spec::vala::eval::TaskId;
use wyrd_spec::verification::execution::{CallProvenance, Citation, InputDeclaration, Locator};
use wyrd_testing::{Bootstrap, WyrdTestServer};

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
        let temp = tempfile::tempdir().expect("loader workspace creates");
        std::fs::write(temp.path().join("notes.txt"), NOTES).expect("notes write");
        std::fs::write(temp.path().join("check.py"), CHECK).expect("source writes");
        let card = temp.path().join("evidence.yaml");
        std::fs::write(
            &card,
            "apiVersion: wyrd/v1\nkind: Artifact\nmetadata:\n  name: hosted-evidence\n  version: 1.0.0\n  space: default\nartifacts:\n  - relative_path: notes.txt\n    content_type: text/plain\n  - relative_path: check.py\n    content_type: text/x-python\nspec:\n  artifact_kind: dataset\n",
        )
        .expect("card writes");
        let input = build_registration_input(load(&card).expect("loader tree builds"))
            .expect("registration input builds");
        let server = Box::pin(WyrdTestServer::start_bound())
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
        let receipt = Box::pin(cards.register(&input))
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
        let caller = Caller {
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
        };
        TenantEvidence::new(self.server.state().clone(), caller, slots, limits)
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
#[ignore = "needs the Monty worker; run `mise run test:verification:sandbox`"]
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
#[ignore = "needs the Monty worker; run `mise run test:verification:sandbox`"]
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
