//! The one deployment every Rust SDK journey runs against.
//!
//! A [`Deployment`] is a real bound Wyrd server plus the administrator key the
//! deployment hands its operator; every SDK handle is built from its address
//! and a key, the way a user's process is configured. Cards come only from the
//! repository-root `fixtures/` tree ([`fixture`]).
//!
//! The test process carries no Wyrd configuration of its own: a test acting
//! as some principal builds that principal's client with
//! [`Deployment::client`] and passes it to the SDK or CLI call explicitly.

use std::path::{Path, PathBuf};
use std::time::Duration;
use wyrd_ai::bifrost::{BifrostOptions, QueryParam};

use secrecy::{ExposeSecret, SecretString};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};
use wiremock::{Mock, MockServer, Request, Respond, ResponseTemplate};
use wyrd_ai::auth::{
    CreateServicePrincipalRequest, PrincipalId, PrincipalKindTag, PrincipalQuery, Principals,
    WorkloadFamily, WorkloadKind, WorkloadRestriction,
};
use wyrd_ai::cards::{BundleMode, CardRef, CardSelector, Cards, DeclarationReceipt};
use wyrd_ai::config::ClientConfig;
use wyrd_ai::operator_connections::{
    ConnectionName, CreateOperatorConnectionRequest, HttpConnectionAuth, HttpsOrigin,
    OperatorConnections, SecretBearer,
};
use wyrd_ai::transport::{GrpcConfig, HttpConfig};
use wyrd_ai::verify::Execution;
use wyrd_ai::{Bifrost, ExecutionId, SpanRecord, TraceId, VerificationResult, WyrdClient};
use wyrd_spec::ids::BindingId;
use wyrd_spec::verification::VerificationExecutionStatus;
use wyrd_spec::verification::execution::VerificationExecutionState;
use wyrd_testing::Bootstrap;
use wyrd_testing::server::{WyrdTestServer, WyrdTestServerBuilder};

/// Path of a checked-in file under the repository-root `fixtures/` tree.
pub fn fixture(relative: &str) -> PathBuf {
    Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures")).join(relative)
}

/// Return the plaintext API key of a machine principal.
///
/// # Panics
/// Panics when `bootstrap` is a user principal.
fn api_key(bootstrap: Bootstrap) -> String {
    match bootstrap {
        Bootstrap::Machine { api_key, .. } => api_key.expose_secret().to_owned(),
        Bootstrap::User { .. } => panic!("expected a machine principal"),
    }
}

/// A bound Wyrd server and its administrator key.
pub struct Deployment {
    /// The real server every journey call reaches.
    server: WyrdTestServer,
    /// Key of the tenant administrator the deployment bootstraps.
    admin_key: String,
    /// The administrator's own fixture Service root.
    admin_root: CardRef,
}

impl Deployment {
    /// Start a server with the default deployment configuration.
    ///
    /// # Panics
    /// Panics when the server cannot start or bootstrap its administrator.
    pub async fn start() -> Self {
        Self::start_with(WyrdTestServer::builder()).await
    }

    /// Start a server configured by `builder` and bootstrap its administrator.
    ///
    /// # Panics
    /// Panics when the server cannot start or bootstrap its administrator.
    pub async fn start_with(builder: WyrdTestServerBuilder) -> Self {
        let server = builder.start_bound().await.expect("test server starts");
        let admin = server
            .bootstrap_service("journey_admin", &["admin"])
            .await
            .expect("administrator bootstraps");
        let admin_root = admin
            .card_ref()
            .expect("a bootstrapped Service has a root")
            .clone();
        Self {
            server,
            admin_key: api_key(admin),
            admin_root,
        }
    }

    /// The server, for its address and its documented test controls.
    pub fn server(&self) -> &WyrdTestServer {
        &self.server
    }

    /// A client of this deployment that authenticates with `key`.
    ///
    /// Built through the public `WyrdClient::with_config` spelling: the
    /// explicit credential is tier 0 of the chain, so no ambient credential
    /// can replace it.
    ///
    /// # Panics
    /// Panics when the client cannot be assembled.
    pub fn client(&self, key: &str) -> WyrdClient {
        WyrdClient::with_config(ClientConfig {
            http: HttpConfig {
                base_url: self
                    .server
                    .base_url()
                    .expect("bound server has a URL")
                    .to_owned(),
                ..HttpConfig::default()
            },
            grpc: GrpcConfig {
                endpoint: self.server.grpc_url().expect("bound server has a gRPC URL"),
                ..GrpcConfig::default()
            },
            credential: Some(SecretString::from(key)),
            ..ClientConfig::default()
        })
        .expect("client builds")
    }

    /// A client of this deployment's administrator.
    pub fn admin(&self) -> WyrdClient {
        self.client(&self.admin_key)
    }

    /// A client that authenticates with `key` and runs as the exact
    /// registered root `root`, as a delegating actor must.
    ///
    /// # Panics
    /// Panics when the client cannot be assembled or `root` carries no uid.
    pub fn rooted(&self, key: &str, root: &CardRef) -> WyrdClient {
        self.client(key)
            .for_workload(root.uid.clone().expect("a registered root carries a uid"))
    }

    /// The administrator's client run as its own fixture Service root.
    pub fn rooted_admin(&self) -> WyrdClient {
        self.rooted(&self.admin_key, &self.admin_root)
    }

    /// The administrator's Cards handle, connected eagerly so the
    /// administrator key is exchanged before a story's first call.
    ///
    /// # Panics
    /// Panics when the server refuses or cannot be reached for the exchange.
    pub async fn cards(&self) -> Cards {
        Cards::connect_with(&self.admin())
            .await
            .expect("handle connects")
    }

    /// Bootstrap a Service principal `name` holding built-in `roles` and
    /// return its key.
    ///
    /// # Panics
    /// Panics when the principal cannot bootstrap.
    pub async fn key(&self, name: &str, roles: &[&str]) -> String {
        api_key(
            self.server
                .bootstrap_service(name, roles)
                .await
                .expect("principal bootstraps"),
        )
    }

    /// Seed role `name` with exactly `permissions`, each a
    /// `resource:action` string granting every object of that operation,
    /// through [`Self::granted_key`], and return the key of the Service
    /// principal of the same name holding only it.
    ///
    /// # Panics
    /// Panics when a permission is not `resource:action` or the role or
    /// principal cannot be seeded.
    pub async fn scoped_key(&self, name: &str, permissions: &[&str]) -> String {
        let grants = permissions
            .iter()
            .map(|permission| {
                let (resource, action) = permission
                    .split_once(':')
                    .expect("permission is resource:action");
                serde_json::json!({ "resource": resource, "action": action, "scope": "all" })
            })
            .collect::<Vec<_>>();
        self.granted_key(name, &grants).await
    }

    /// Seed role `name` granting `verifier:run` on exactly `verifier`,
    /// bootstrap a Card-bound Service principal of the same name holding only
    /// it, and return its key.
    ///
    /// # Panics
    /// Panics when `verifier` has no UID or the role or principal cannot be
    /// seeded.
    pub async fn verifier_key(&self, name: &str, verifier: &CardRef) -> String {
        let uid = verifier
            .uid
            .as_ref()
            .expect("registered Verifier has a UID");
        self.granted_key(
            name,
            &[serde_json::json!({
                "resource": "verifier", "action": "run", "scope": { "verifier": uid }
            })],
        )
        .await
    }

    /// Seed role `name` with exactly `grants`, each a serialized scoped
    /// permission such as `{"resource": "gateway", "action": "invoke",
    /// "scope": "all"}`, bootstrap a Service principal of the same name
    /// holding only it, and return its key.
    ///
    /// # Panics
    /// Panics when the role or principal cannot be seeded.
    pub async fn granted_key(&self, name: &str, grants: &[serde_json::Value]) -> String {
        api_key(self.granted(name, grants).await)
    }

    /// Seed role `name` with exactly `grants` as [`Self::granted_key`] does
    /// and return the bootstrapped principal, whose id a story compares with
    /// retained subject and actor attribution.
    ///
    /// Grants are stored unparsed, so a grant the current schema rejects
    /// reaches the server and is refused by its own issuance decode.
    ///
    /// # Panics
    /// Panics when the role or principal cannot be seeded.
    pub async fn granted(&self, name: &str, grants: &[serde_json::Value]) -> Bootstrap {
        self.server
            .seed_role(name, grants)
            .await
            .expect("role seeds");
        self.server
            .bootstrap_service(name, &[name])
            .await
            .expect("principal bootstraps")
    }

    /// A client of the Service `card`'s family runtime account, run as the
    /// exact root `card`, as the administrator provisions it through
    /// [`runtime_account`].
    ///
    /// # Panics
    /// Panics when the account cannot be provisioned or `card` carries no uid.
    pub async fn service_client(&self, card: &CardRef) -> WyrdClient {
        let (_, key) = runtime_account(&self.admin(), card).await;
        self.rooted(&key, card)
    }

    /// Bootstrap an administrator in a new tenant `slug` and return its key.
    ///
    /// # Panics
    /// Panics when the tenant or its administrator cannot be seeded.
    pub async fn other_tenant_admin(&self, slug: &str) -> String {
        let tenant = self.server.seed_tenant(slug).await.expect("tenant seeds");
        api_key(
            self.server
                .bootstrap_service_in_tenant(tenant, "other_tenant_admin", &["admin"])
                .await
                .expect("other tenant's administrator bootstraps"),
        )
    }

    /// Publish every captured row, then read the timeline of `trace`, given
    /// as 32 hex characters, as the administrator.
    ///
    /// # Panics
    /// Panics when the id is malformed or the flush or the read fails.
    pub async fn timeline(&self, trace: &str) -> Vec<SpanRecord> {
        self.server.flush_bifrost().await.expect("rows publish");
        Bifrost::connect_with(&self.admin(), BifrostOptions::default())
            .await
            .expect("Bifrost connects")
            .trace_timeline(TraceId::from_hex(trace).expect("trace id"))
            .await
            .expect("the timeline reads")
    }

    /// The one verification binding `owner` declares, read from its served
    /// status.
    ///
    /// # Panics
    /// Panics when the read fails or `owner` declares no binding.
    pub async fn binding(&self, owner: &CardRef) -> BindingId {
        self.cards()
            .await
            .get(CardSelector::exact(owner.clone()))
            .await
            .expect("the owner reads")
            .status
            .and_then(|status| status.verification)
            .expect("the owner serves verification status")
            .binding_ids
            .into_iter()
            .next()
            .expect("the owner has one binding")
    }

    /// `binding`'s status, read as the administrator through
    /// `GET /v1/verification/bindings/{binding_id}`.
    ///
    /// # Panics
    /// Panics when the token exchange or the read fails.
    pub async fn binding_status(
        &self,
        binding: &BindingId,
    ) -> wyrd_spec::verification::VerificationBindingStatus {
        let token = self
            .admin()
            .access_token()
            .await
            .expect("the administrator token mints");
        reqwest::Client::new()
            .get(format!(
                "{}/v1/verification/bindings/{binding}",
                self.server.base_url().expect("bound server has a URL")
            ))
            .header("x-wyrd-access-token", format!("Bearer {}", token.expose()))
            .send()
            .await
            .expect("the server answers")
            .error_for_status()
            .expect("the binding reads")
            .json()
            .await
            .expect("the binding status decodes")
    }

    /// The first retained result of `binding`, read back as the
    /// administrator: publish captured rows until `bifrost.verification.results`
    /// names one, then wait for that execution's typed result by its ID.
    ///
    /// # Panics
    /// Panics when no result is retained within `timeout`, a read fails, or
    /// the execution settles without a result.
    pub async fn scheduled_result(
        &self,
        binding: &BindingId,
        timeout: Duration,
    ) -> VerificationResult {
        /// One `bifrost.verification.results` row's execution identity.
        #[derive(serde::Deserialize)]
        struct Retained {
            /// The execution the result belongs to.
            execution_id: String,
        }
        let admin = self.admin();
        let bifrost = Bifrost::connect_with(&admin, BifrostOptions::default())
            .await
            .expect("Bifrost connects");
        let deadline = tokio::time::Instant::now() + timeout;
        let id = loop {
            self.server.flush_bifrost().await.expect("rows publish");
            let rows: Vec<Retained> = bifrost
                .sql_as(
                    "SELECT execution_id FROM bifrost.verification.results WHERE binding_id = $1",
                    &[QueryParam::String(binding.to_string())],
                )
                .await
                .expect("results read back");
            if let Some(row) = rows.into_iter().next() {
                break row.execution_id;
            }
            assert!(
                tokio::time::Instant::now() < deadline,
                "binding {binding} retained no result within {timeout:?}"
            );
            tokio::time::sleep(Duration::from_millis(500)).await;
        };
        Execution::connect_with(
            id.parse::<ExecutionId>()
                .expect("the row names an execution ID"),
            &admin,
        )
        .await
        .expect("the execution handle connects")
        .wait(timeout)
        .await
        .expect("the retained result reads")
    }

    /// The newest completed execution of group `binding` other than `after`,
    /// read as the administrator: publish captured rows so the group poller
    /// discovers them, then read the binding's `last_execution_id` and its
    /// execution state until that execution completes with a result.
    ///
    /// # Panics
    /// Panics when no such execution completes within `timeout` or a read
    /// fails.
    pub async fn group_execution(
        &self,
        binding: &BindingId,
        after: Option<&ExecutionId>,
        timeout: Duration,
    ) -> VerificationExecutionState {
        let admin = self.admin();
        let deadline = tokio::time::Instant::now() + timeout;
        loop {
            self.server.flush_bifrost().await.expect("rows publish");
            let latest = self.binding_status(binding).await.last_execution_id;
            if let Some(id) = latest.filter(|id| Some(id) != after) {
                let state = Execution::connect_with(id, &admin)
                    .await
                    .expect("the execution handle connects")
                    .status()
                    .await
                    .expect("the execution reads");
                if state.status == VerificationExecutionStatus::Completed && state.result.is_some()
                {
                    return state;
                }
            }
            assert!(
                tokio::time::Instant::now() < deadline,
                "group binding {binding} completed no new execution within {timeout:?}"
            );
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
    }

    /// Start a local Operator hook and create the HTTP connection
    /// `connection` pointing at it with bearer `token`, so a registered
    /// Operator naming that connection delivers to the hook.
    ///
    /// # Panics
    /// Panics when the connection cannot be created.
    pub async fn hooks(&self, connection: &str, token: &str) -> Hooks {
        let server = MockServer::start().await;
        let (sender, received) = unbounded_channel();
        Mock::given(wiremock::matchers::any())
            .respond_with(Forward(sender))
            .mount(&server)
            .await;
        OperatorConnections::connect_with(&self.admin())
            .await
            .expect("handle connects")
            .create(&CreateOperatorConnectionRequest::Http {
                name: ConnectionName::new(connection).expect("connection name"),
                origin: HttpsOrigin::parse(&server.uri()).expect("hook origin parses"),
                auth: HttpConnectionAuth::Bearer {
                    token: SecretBearer::new(token.to_owned()),
                },
            })
            .await
            .expect("connection creates");
        Hooks {
            _server: server,
            received,
        }
    }

    /// Shut the server down and boot `builder` over the same Postgres and
    /// artifact storage, as a process restart does. Durable rows, the
    /// administrator key among them, survive; the address changes, so
    /// clients are built again from the returned deployment.
    ///
    /// # Panics
    /// Panics when the server does not shut down or the replacement does
    /// not start.
    pub async fn restart(self, builder: WyrdTestServerBuilder) -> Self {
        let server = Box::pin(self.server.restart_bound(builder))
            .await
            .expect("test server restarts");
        Self {
            server,
            admin_key: self.admin_key,
            admin_root: self.admin_root,
        }
    }

    /// Stop the server.
    ///
    /// # Panics
    /// Panics when the server does not shut down cleanly.
    pub async fn shutdown(self) {
        self.server
            .shutdown()
            .await
            .expect("test server shuts down");
    }
}

/// A local Operator hook a [`Deployment::hooks`] connection points at.
pub struct Hooks {
    /// The local hook; kept so it keeps listening.
    _server: MockServer,
    /// Every request the hook receives, in arrival order.
    pub received: UnboundedReceiver<Request>,
}

impl Hooks {
    /// Assert the hook receives nothing for `quiet`.
    ///
    /// # Panics
    /// Panics when a delivery arrives.
    pub async fn assert_quiet(&mut self, quiet: Duration) {
        if let Ok(Some(delivery)) = tokio::time::timeout(quiet, self.received.recv()).await {
            panic!("no delivery is due: {} {}", delivery.method, delivery.url);
        }
    }
}

/// The hook's responder: answers 200 and hands every request to the story.
struct Forward(
    /// The story's delivery channel; a send after the story dropped its
    /// receiver is ignored.
    UnboundedSender<Request>,
);

impl Respond for Forward {
    /// Forward `request` to the story and acknowledge it.
    fn respond(&self, request: &Request) -> ResponseTemplate {
        // The story may have finished and dropped its receiver.
        let _ = self.0.send(request.clone());
        ResponseTemplate::new(200)
    }
}

/// `span` followed by each ancestor its parent ids reach within `timeline`,
/// nearest first; the last entry's parent, if any, lies outside the timeline.
pub fn ancestry<'a>(timeline: &'a [SpanRecord], span: &'a SpanRecord) -> Vec<&'a SpanRecord> {
    let mut chain = vec![span];
    while let Some(parent) = chain.last().and_then(|child| {
        timeline.iter().find(|parent| {
            parent.trace_id == child.trace_id && Some(parent.span_id) == child.parent_span_id
        })
    }) {
        chain.push(parent);
    }
    chain
}

/// The `card` family's runtime account in `admin`'s tenant and one new key
/// for it.
///
/// Registration creates no principal, so the account restricted to `card`'s
/// kind and name is created holding only `workload` on first use, and issued
/// one more credential afterwards.
///
/// # Panics
/// Panics when `card` is not a Service, or a principal call fails.
pub async fn runtime_account(admin: &WyrdClient, card: &CardRef) -> (PrincipalId, String) {
    let principals = Principals::connect_with(admin)
        .await
        .expect("handle connects");
    let name = format!("{}-runtime", card.name);
    let existing = principals
        .list(&PrincipalQuery {
            kind: Some(PrincipalKindTag::Service),
            name: Some(name.clone()),
            ..PrincipalQuery::default()
        })
        .await
        .expect("principals list")
        .principals
        .into_iter()
        .next();
    if let Some(account) = existing {
        let issued = principals
            .issue_credential(&account.principal_id)
            .await
            .expect("the runtime account is issued a key");
        return (account.principal_id, issued.credential.expose().to_owned());
    }
    let created = principals
        .create_service_principal(&CreateServicePrincipalRequest {
            name,
            roles: vec!["workload".to_owned()],
            description: None,
            workload_restriction: Some(WorkloadRestriction::Family(WorkloadFamily {
                kind: WorkloadKind::from_card_kind(&card.kind)
                    .expect("a runtime account names a Service"),
                name: card.name.clone(),
            })),
        })
        .await
        .expect("the runtime account is created");
    (created.principal_id, created.credential.expose().to_owned())
}

/// Register the fixture at `relative` through `cards`.
///
/// # Panics
/// Panics when registration is refused.
pub async fn declare(cards: &Cards, relative: &str) -> DeclarationReceipt {
    cards
        .declare_from_path(&fixture(relative))
        .await
        .unwrap_or_else(|error| panic!("{relative} declares: {error}"))
}

/// The exact reference `receipt` registered for the Card named `name`.
///
/// # Panics
/// Panics when the receipt registered no Card of that name.
pub fn declared(receipt: &DeclarationReceipt, name: &str) -> CardRef {
    receipt
        .outcomes
        .iter()
        .find(|outcome| outcome.card_ref.name.as_str() == name)
        .unwrap_or_else(|| panic!("{name} was registered"))
        .card_ref
        .clone()
}

/// Hydrate the complete graph of `root` into a fresh directory and return it;
/// the bundle is at `<dir>/bundle`.
///
/// # Panics
/// Panics when hydration fails.
pub async fn pull(cards: &Cards, root: &CardRef) -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("bundle directory creates");
    cards
        .pull(
            CardSelector::exact(root.clone()),
            dir.path().join("bundle"),
            BundleMode::Complete,
        )
        .await
        .expect("graph pulls");
    dir
}
