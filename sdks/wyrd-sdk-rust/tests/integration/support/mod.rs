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

use secrecy::{ExposeSecret, SecretString};
use wyrd_sdk::WyrdClient;
use wyrd_sdk::cards::{CardRef, CardSelector, Cards, HydrationMode, RegistrationReceipt};
use wyrd_sdk::cli;
use wyrd_sdk::config::ClientConfig;
use wyrd_sdk::transport::{GrpcConfig, HttpConfig};
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

    /// The administrator's Cards handle.
    pub fn cards(&self) -> Cards {
        Cards::with_client(self.admin())
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

    /// Seed role `name` with exactly `permissions`, bootstrap a Service
    /// principal of the same name holding only it, and return its key.
    ///
    /// # Panics
    /// Panics when a permission does not parse or the role or principal
    /// cannot be seeded.
    pub async fn scoped_key(&self, name: &str, permissions: &[&str]) -> String {
        let permissions = permissions
            .iter()
            .map(|permission| permission.parse().expect("permission parses"))
            .collect::<Vec<_>>();
        self.server
            .seed_role(name, &permissions)
            .await
            .expect("role seeds");
        self.key(name, &[name]).await
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
