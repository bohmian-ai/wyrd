//! The one deployment every Rust SDK journey runs against.
//!
//! A [`Deployment`] is a real bound Wyrd server plus the administrator key the
//! deployment hands its operator; every SDK handle is built from its address
//! and a key, the way a user's process is configured. Cards come only from the
//! repository-root `fixtures/` tree ([`fixture`]).
//!
//! Some SDK and CLI calls resolve their server and credential only from the
//! ambient process environment, which a multithreaded test cannot set without
//! `unsafe`. [`Deployment::run_child`] runs one test of this binary again as a
//! child process whose environment is the deployment's; inside it
//! [`is_child`] is true and [`report`] hands one JSON value back to the parent.

use std::path::{Path, PathBuf};
use std::process::Command;

use secrecy::ExposeSecret;
use serde_json::Value;
use wyrd_sdk::WyrdClient;
use wyrd_sdk::bifrost::client_from_options;
use wyrd_sdk::cards::{
    CardGraphHydrator, CardRef, CardSelector, Cards, HydrationMode, RegistrationReceipt,
};
use wyrd_testing::Bootstrap;
use wyrd_testing::server::{WyrdTestServer, WyrdTestServerBuilder};

/// Environment variable that marks a process as a [`Deployment::run_child`] child.
const CHILD: &str = "WYRD_JOURNEY_CHILD";

/// Prefix of the one stdout line on which a child [`report`]s its outcome.
const OUTCOME: &str = "WYRD_JOURNEY_OUTCOME ";

/// Path of a checked-in file under the repository-root `fixtures/` tree.
pub fn fixture(relative: &str) -> PathBuf {
    Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures")).join(relative)
}

/// Whether this process is a [`Deployment::run_child`] child.
pub fn is_child() -> bool {
    std::env::var_os(CHILD).is_some()
}

/// Hand `outcome` from a child back to the parent's [`Deployment::run_child`].
pub fn report(outcome: &Value) {
    println!("{OUTCOME}{outcome}");
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
        let server = Box::pin(builder.start_bound())
            .await
            .expect("test server starts");
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
    /// # Panics
    /// Panics when the client cannot be assembled.
    pub fn client(&self, key: &str) -> WyrdClient {
        let grpc = self.server.grpc_url().expect("bound server has a gRPC URL");
        client_from_options(
            Some(self.server.base_url().expect("bound server has a URL")),
            Some(key),
            Some(&grpc),
        )
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

    /// Mint a key for the principal registration projected for the Service
    /// `card`, granting `roles` beyond the `workload` role it already holds.
    ///
    /// # Panics
    /// Panics when `card` projected no principal.
    pub async fn service_key(&self, card: &CardRef, roles: &[&str]) -> String {
        api_key(
            self.server
                .credential_registered_service(card, roles)
                .await
                .expect("registered Service is credentialed"),
        )
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

    /// Run `test` (`module::name`) of this binary as a child process whose
    /// environment is this deployment, authenticated with `key`, plus `env`,
    /// and return the value the child [`report`]ed, or `Null`.
    ///
    /// The child sees only `WYRD_SERVER_URL`, `WYRD_GRPC_URL`, `WYRD_API_KEY`,
    /// and an empty client configuration home, the way a user's shell runs a
    /// script against the deployment.
    ///
    /// # Panics
    /// Panics when the child cannot run, fails, or runs no test.
    pub async fn run_child(&self, test: &str, key: &str, env: &[(&str, &str)]) -> Value {
        let config_home = wyrd_testing::human_login::private_config_home();
        let mut command = Command::new(std::env::current_exe().expect("current test executable"));
        command
            .args([test, "--exact", "--include-ignored", "--nocapture"])
            .env(CHILD, "1")
            .env(
                "WYRD_SERVER_URL",
                self.server.base_url().expect("bound server has a URL"),
            )
            .env(
                "WYRD_GRPC_URL",
                self.server.grpc_url().expect("bound server has a gRPC URL"),
            )
            .env("WYRD_API_KEY", key)
            .env("WYRD_CONFIG_HOME", config_home.path())
            .env_remove("WYRD_ACCESS_TOKEN")
            .env_remove("WYRD_WORKLOAD_TOKEN")
            .env_remove("WYRD_TENANT")
            .envs(env.iter().copied());
        let output = tokio::task::spawn_blocking(move || command.output())
            .await
            .expect("child joins")
            .expect("child starts");
        let stdout = String::from_utf8_lossy(&output.stdout);
        // libtest exits 0 when `--exact` selects nothing; "1 passed" proves
        // the child actually ran.
        assert!(
            output.status.success() && stdout.contains("1 passed"),
            "child {test} failed:\n{stdout}\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
        stdout
            .lines()
            .find_map(|line| line.strip_prefix(OUTCOME))
            .map_or(Value::Null, |line| {
                serde_json::from_str(line).expect("child outcome is JSON")
            })
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

/// Register the fixture at `relative` through `cards`.
///
/// # Panics
/// Panics when registration is refused.
pub async fn register(cards: &Cards, relative: &str) -> RegistrationReceipt {
    Box::pin(cards.register_from_path(&fixture(relative)))
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
    Box::pin(CardGraphHydrator::new(cards.registry_context()).hydrate(
        &CardSelector::exact(root.clone()),
        &dir.path().join("bundle"),
        HydrationMode::Complete,
    ))
    .await
    .expect("graph hydrates");
    dir
}
