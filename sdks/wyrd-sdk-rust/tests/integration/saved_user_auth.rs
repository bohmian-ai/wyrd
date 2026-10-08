//! A person signs in once with the CLI, and their local scripts use the
//! saved logins through the Rust SDK, on the Keycloak identity lane.
//!
//! A script's configuration is the Wyrd configuration home holding those
//! logins plus, at most, a tenant selector (what `WYRD_TENANT` sets). Each
//! test passes that configuration explicitly as a fixed
//! [`Environment`], so no test reads or changes the process environment.

use secrecy::SecretString;
use tempfile::TempDir;
use wyrd_sdk::cards::{CardKind, Cards, ListCardsRequest};
use wyrd_sdk::config::ClientConfig;
use wyrd_sdk::environment::Environment;
use wyrd_sdk::{GlobalConfig, WyrdClient, WyrdError};
use wyrd_testing::human_login::{FIXTURE_TENANT_SLUG, HUMAN_PUBLIC_ORIGIN, HumanSso};
use wyrd_testing::server::WyrdTestServer;

use crate::support::{Deployment, fixture};

/// The second tenant, where alice administers.
const ADMIN_TENANT: &str = "saved-login-two";

/// The Prompt every write registers.
const PROMPT: &str = "cards/gateway_inference/ask-prompt.yaml";

/// A deployment accepting human single sign-on in its own tenant, and a
/// fresh configuration home holding bob's saved reader login there.
struct SavedLogins {
    /// The deployment.
    deployment: Deployment,
    /// The served human-login steps.
    sso: HumanSso,
    /// The configuration home holding the saved logins.
    config: TempDir,
}

impl SavedLogins {
    /// Start the deployment, activate Keycloak sign-on for its tenant, and
    /// save bob's login.
    ///
    /// # Panics
    /// Panics when a setup step fails.
    async fn start() -> Self {
        let deployment = Deployment::start_with(
            WyrdTestServer::builder()
                .with_public_origin(HUMAN_PUBLIC_ORIGIN.parse().expect("origin parses")),
        )
        .await;
        let sso = HumanSso::new(
            deployment
                .server()
                .base_url()
                .expect("bound server has a URL"),
        );
        sso.activate_keycloak(&deployment.key("sso_admin", &["admin"]).await)
            .await;
        let config = wyrd_testing::human_login::private_config_home();
        sso.save_login(config.path(), FIXTURE_TENANT_SLUG, "bob", "wyrd-test")
            .await;
        Self {
            deployment,
            sso,
            config,
        }
    }

    /// Activate Keycloak sign-on for [`ADMIN_TENANT`] and save alice's
    /// administrator login there, making it the newest saved login.
    ///
    /// # Panics
    /// Panics when the tenant cannot be seeded or the login fails.
    async fn save_alices_admin_login(&self) {
        self.sso
            .activate_keycloak(&self.deployment.other_tenant_admin(ADMIN_TENANT).await)
            .await;
        self.sso
            .save_login(self.config.path(), ADMIN_TENANT, "alice", "alice-password")
            .await;
    }

    /// The client configuration a script gets from this configuration home,
    /// selecting `tenant`'s saved login when given.
    ///
    /// # Panics
    /// Panics when the paths are not UTF-8 or the server has no URL.
    fn config(&self, tenant: Option<&str>) -> ClientConfig {
        let mut config = ClientConfig::from_environment(
            Environment::from([(
                "WYRD_CONFIG_HOME",
                self.config.path().to_str().expect("UTF-8 config home"),
            )]),
            &GlobalConfig::default(),
            self.deployment.server().base_url(),
            None,
        );
        config.tenant = tenant.map(str::to_owned);
        config
    }

    /// `Cards` resolved from the saved logins, selecting `tenant`'s.
    ///
    /// # Errors
    /// Returns the client's refusal when no usable credential resolves.
    fn cards(&self, tenant: Option<&str>) -> Result<Cards, WyrdError> {
        Ok(Cards::with_client(WyrdClient::with_config(
            self.config(tenant),
        )?))
    }
}

/// A listing of the registered Prompts, a read every saved login may make.
fn prompts() -> ListCardsRequest {
    ListCardsRequest {
        kind: Some(CardKind::Prompt),
        space: None,
        name: None,
        version_range: None,
        status: None,
        filter: None,
        include_prerelease: false,
        limit: None,
        cursor: None,
    }
}

/// Without a selector the newest saved login, alice's administrator login,
/// is used, and it registers a Prompt.
///
/// # Panics
/// Panics when a setup step or the registration fails, or the receipt
/// differs.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the Keycloak identity lane; run via `mise run test:identity:journey`"]
async fn newest_saved_login_is_used_without_a_selector() {
    let logins = SavedLogins::start().await;
    logins.save_alices_admin_login().await;

    let receipt = logins
        .cards(None)
        .expect("the newest login resolves")
        .register_from_path(&fixture(PROMPT))
        .await
        .expect("alice registers the Prompt");

    assert_eq!(
        (receipt.root.kind, receipt.root.name.as_str()),
        (CardKind::Prompt, "ask-prompt")
    );
    logins.deployment.shutdown().await;
}

/// Bob's saved reader login cannot register a Card.
///
/// # Panics
/// Panics when the registration succeeds or is refused with another code.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the Keycloak identity lane; run via `mise run test:identity:journey`"]
async fn saved_reader_login_is_denied_a_write() {
    let logins = SavedLogins::start().await;

    let refused = logins
        .cards(Some(FIXTURE_TENANT_SLUG))
        .expect("bob's login resolves")
        .register_from_path(&fixture(PROMPT))
        .await
        .expect_err("a reader cannot register");

    assert_eq!(refused.code(), "WYRD_PERMISSION_403_DENIED_RBAC");
    logins.deployment.shutdown().await;
}

/// A stale saved login renews through Wyrd on use, and the renewal is saved.
///
/// # Panics
/// Panics when the read fails or the saved login stays stale.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the Keycloak identity lane; run via `mise run test:identity:journey`"]
async fn stale_login_refreshes_and_saves_the_renewal() {
    let logins = SavedLogins::start().await;
    logins
        .sso
        .expire_saved(logins.config.path(), FIXTURE_TENANT_SLUG);

    logins
        .cards(Some(FIXTURE_TENANT_SLUG))
        .expect("bob's login resolves")
        .list(prompts())
        .await
        .expect("the stale login renews");

    assert!(
        !logins
            .sso
            .saved_is_stale(logins.config.path(), FIXTURE_TENANT_SLUG)
    );
    logins.deployment.shutdown().await;
}

/// A saved login whose refresh chain was revoked cannot renew.
///
/// # Panics
/// Panics when the read succeeds or is refused with another code.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the Keycloak identity lane; run via `mise run test:identity:journey`"]
async fn revoked_login_is_refused() {
    let logins = SavedLogins::start().await;
    logins
        .sso
        .revoke_saved(logins.config.path(), FIXTURE_TENANT_SLUG)
        .await;
    logins
        .sso
        .expire_saved(logins.config.path(), FIXTURE_TENANT_SLUG);

    let refused = logins
        .cards(Some(FIXTURE_TENANT_SLUG))
        .expect("bob's login resolves")
        .list(prompts())
        .await
        .expect_err("a revoked login cannot renew");

    assert_eq!(refused.code(), "WYRD_CLIENT_401_SAVED_LOGIN_UNUSABLE");
    logins.deployment.shutdown().await;
}

/// A tenant selector naming no saved login is refused rather than replaced
/// by another identity.
///
/// # Panics
/// Panics when the client resolves or is refused with another code.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the Keycloak identity lane; run via `mise run test:identity:journey`"]
async fn selector_naming_no_saved_login_is_refused() {
    let logins = SavedLogins::start().await;

    let refused = logins
        .cards(Some("no-such-tenant"))
        .err()
        .expect("a selector must name a saved login");

    assert_eq!(refused.code(), "WYRD_CLIENT_401_SAVED_LOGIN_UNUSABLE");
    logins.deployment.shutdown().await;
}

/// An explicit machine key names its own tenant, so a tenant selector beside
/// it is refused.
///
/// # Panics
/// Panics when the client resolves or is refused with another code.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the Keycloak identity lane; run via `mise run test:identity:journey`"]
async fn explicit_key_beside_a_tenant_selector_is_refused() {
    let logins = SavedLogins::start().await;
    let mut config = logins.config(Some(FIXTURE_TENANT_SLUG));
    config.credential = Some(SecretString::from(
        logins.deployment.key("script_admin", &["admin"]).await,
    ));

    let refused = WyrdClient::with_config(config).expect_err("a selector beside a key is refused");

    assert_eq!(refused.code(), "WYRD_CLIENT_400_CONFIG_INVALID");
    logins.deployment.shutdown().await;
}
