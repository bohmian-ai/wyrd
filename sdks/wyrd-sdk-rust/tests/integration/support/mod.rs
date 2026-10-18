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
use wyrd_ai::cards::{CardRef, CardSelector, Cards, HydrationMode, RegistrationReceipt};
use wyrd_ai::cli;
use wyrd_ai::config::ClientConfig;
use wyrd_ai::transport::{GrpcConfig, HttpConfig};
use wyrd_ai::verify::Execution;
use wyrd_ai::{Bifrost, ExecutionId, SpanRecord, TraceId, VerificationResult, WyrdClient};
use wyrd_spec::ids::BindingId;
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
        let admin_key = api_key(
            server
                .bootstrap_service("journey_admin", &["admin"])
                .await
                .expect("administrator bootstraps"),
        );
        Self { server, admin_key }
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

    /// Issue a key for the principal registration projected for the Service
    /// `card`, as the administrator through the `wyrd auth issue-key` CLI
    /// function, and return its plaintext.
    ///
    /// The key holds exactly the default `workload` Role every Service
    /// principal starts with; nothing is granted beyond it.
    ///
    /// # Panics
    /// Panics when the CLI refuses to issue the key.
    pub async fn service_key(&self, card: &CardRef) -> String {
        let [kind, name, version, space] = coordinates(card);
        cli::issue_key(
            &kind,
            &name,
            &version,
            &space,
            None,
            None,
            Some(self.admin()),
        )
        .await
        .expect("the CLI issues the Service key")
        .key
        .expose()
        .to_owned()
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
    /// administrator: publish captured rows until `vala.verification.results`
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
        /// One `vala.verification.results` row's execution identity.
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
                    "SELECT execution_id FROM vala.verification.results WHERE binding_id = $1",
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

/// The `kind`, `name`, `version`, and `space` arguments the card-selecting
/// CLI functions take for `card`; an unpinned space is `default`.
pub fn coordinates(card: &CardRef) -> [String; 4] {
    [
        card.kind.wire_name().to_owned(),
        card.name.as_str().to_owned(),
        card.version.to_string(),
        card.space
            .as_ref()
            .map_or_else(|| "default".to_owned(), ToString::to_string),
    ]
}

/// Register the fixture at `relative` through `cards`.
///
/// # Panics
/// Panics when registration is refused.
pub async fn register(cards: &Cards, relative: &str) -> RegistrationReceipt {
    cards
        .register_from_path(&fixture(relative))
        .await
        .unwrap_or_else(|error| panic!("{relative} registers: {error}"))
}

/// The exact reference `receipt` registered for the Card named `name`.
///
/// # Panics
/// Panics when the receipt registered no Card of that name.
pub fn registered(receipt: &RegistrationReceipt, name: &str) -> CardRef {
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
pub async fn hydrate(cards: &Cards, root: &CardRef) -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("bundle directory creates");
    cards
        .hydrate(
            CardSelector::exact(root.clone()),
            dir.path().join("bundle"),
            HydrationMode::Complete,
        )
        .await
        .expect("graph hydrates");
    dir
}
