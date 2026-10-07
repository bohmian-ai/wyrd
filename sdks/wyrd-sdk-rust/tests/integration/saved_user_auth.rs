//! A person signs in once with the CLI, and their local scripts use the saved
//! logins through the Rust SDK on the Keycloak identity lane.

use std::path::Path;

use secrecy::{ExposeSecret, SecretString};
use wyrd_sdk::cards::{Cards, ListCardsRequest};
use wyrd_sdk::config::ClientConfig;
use wyrd_sdk::saved_login::canonical_origin;
use wyrd_sdk::{GlobalConfig, WyrdClient};
use wyrd_testing::Bootstrap;
use wyrd_testing::human_login::{
    FIXTURE_TENANT_SLUG, HUMAN_PUBLIC_ORIGIN, HumanSso, expire_saved_access, saved_login,
};
use wyrd_testing::server::{WyrdTestServer, WyrdTestServerBuilder};

use crate::support::fixture;

/// Mint a machine API key through the harness's real bootstrap route.
///
/// # Panics
/// Panics when bootstrapping fails or returns a user principal.
async fn machine_key(server: &WyrdTestServer, name: &str, roles: &[&str]) -> String {
    match server
        .bootstrap_service(name, roles)
        .await
        .expect("service bootstraps")
    {
        Bootstrap::Machine { api_key, .. } => api_key.expose_secret().to_owned(),
        Bootstrap::User { .. } => panic!("service bootstrap returned a user principal"),
    }
}

/// A listing of every Card, the read both saved-login principals may make.
fn card_listing() -> ListCardsRequest {
    ListCardsRequest {
        kind: None,
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

/// Environment naming the phase a [`saved_user_auth_script`] child runs.
const SCRIPT_PHASE: &str = "WYRD_SAVED_LOGIN_PHASE";

/// Run one phase of [`saved_user_auth_script`] as a separate local process,
/// the way a person's script uses the CLI-established login: only the
/// configuration directory and server URL reach it, plus the machine key the
/// override phase presents explicitly.
///
/// # Panics
/// Panics when the child cannot run or its phase fails.
async fn run_script(phase: &str, config: &Path, base_url: &str, second: &str, machine: &str) {
    let mut command = std::process::Command::new(std::env::current_exe().expect("test binary"));
    command
        .args([
            "--exact",
            "saved_user_auth::saved_user_auth_script",
            "--include-ignored",
            "--nocapture",
        ])
        .env(SCRIPT_PHASE, phase)
        .env("WYRD_CONFIG_HOME", config)
        .env("WYRD_SAVED_LOGIN_SERVER", base_url)
        .env("WYRD_SAVED_LOGIN_SECOND_TENANT", second)
        .env("WYRD_SAVED_LOGIN_MACHINE_KEY", machine)
        .env_remove("WYRD_ACCESS_TOKEN")
        .env_remove("WYRD_WORKLOAD_TOKEN")
        .env_remove("WYRD_API_KEY")
        .env_remove("WYRD_TENANT");
    let output = tokio::task::spawn_blocking(move || command.output())
        .await
        .expect("script joins")
        .expect("script runs");
    let stdout = String::from_utf8_lossy(&output.stdout);
    // libtest exits 0 when `--exact` selects nothing; "1 passed" proves the
    // phase actually ran.
    assert!(
        output.status.success() && stdout.contains("1 passed"),
        "script phase {phase} failed:\n{stdout}{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

/// One phase of a local script using the saved logins, run only as the child
/// [`run_script`] starts; without [`SCRIPT_PHASE`] it does nothing. A phase
/// selects a saved login through the configured tenant (`ClientConfig.tenant`,
/// what `WYRD_TENANT` sets), since constructors take no tenant.
///
/// `select`: without a selector the newest login (alice's admin login to the
/// second tenant) registers a Card, a selector naming no saved login fails, the reader's saved login lists Cards
/// and is denied a registration, and the second tenant's login resolves by
/// its tenant route key. `renew`: a stale login renews through Wyrd.
/// `override`: an explicit machine credential wins over the saved reader, and
/// a tenant selector beside it is refused because the key names its own
/// tenant. `revoked`: a revoked login fails renewal and asks for a new login.
///
/// # Panics
/// Panics when the phase's expectation differs.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "a child process of saved_user_auth_journey"]
async fn saved_user_auth_script() {
    let Ok(phase) = std::env::var(SCRIPT_PHASE) else {
        return;
    };
    let base_url = std::env::var("WYRD_SAVED_LOGIN_SERVER").expect("server URL");
    let connect = |credential: Option<String>, tenant: Option<&str>| {
        let mut config = ClientConfig::from_global_with_overrides(
            &GlobalConfig::default(),
            Some(&base_url),
            None,
        );
        config.credential = credential.map(SecretString::from);
        if let Some(tenant) = tenant {
            config.tenant = Some(tenant.to_owned());
        }
        WyrdClient::with_config(config).map(Cards::with_client)
    };
    match phase.as_str() {
        "select" => {
            let newest = connect(None, None).expect("the newest login resolves");
            Box::pin(
                newest.register_from_path(&fixture("cards/gateway_inference/ask-prompt.yaml")),
            )
            .await
            .expect("the newest login, alice's admin login, registers");
            let unmatched = connect(None, Some("no-such-tenant"))
                .err()
                .expect("a selector naming no saved login fails");
            assert!(
                unmatched.to_string().contains("(tenant_mismatch)"),
                "{unmatched}"
            );
            let reader = connect(None, Some(FIXTURE_TENANT_SLUG)).expect("reader resolves");
            reader
                .list(card_listing())
                .await
                .expect("the reader lists Cards");
            let denied = Box::pin(
                reader.register_from_path(&fixture("cards/gateway_inference/ask-prompt.yaml")),
            )
            .await
            .expect_err("the reader cannot register");
            assert_eq!(denied.status(), 403);
            let second = std::env::var("WYRD_SAVED_LOGIN_SECOND_TENANT").expect("tenant key");
            connect(None, Some(&second))
                .expect("the second tenant's login resolves by its route key")
                .list(card_listing())
                .await
                .expect("alice lists in her tenant");
        }
        "renew" => {
            connect(None, Some(FIXTURE_TENANT_SLUG))
                .expect("resolves")
                .list(card_listing())
                .await
                .expect("a stale saved login renews through Wyrd");
        }
        "override" => {
            let machine = std::env::var("WYRD_SAVED_LOGIN_MACHINE_KEY").expect("machine key");
            let selected = connect(Some(machine.clone()), Some(FIXTURE_TENANT_SLUG))
                .err()
                .expect("a selector beside a machine key is refused");
            assert_eq!(selected.code(), "WYRD_CLIENT_400_CONFIG_INVALID");
            assert!(
                selected.to_string().contains("already names its tenant"),
                "{selected}"
            );
            let cards = connect(Some(machine), None).expect("resolves");
            Box::pin(cards.register_from_path(&fixture("cards/gateway_inference/ask-prompt.yaml")))
                .await
                .expect("the explicit machine credential overrides the saved reader");
        }
        "revoked" => {
            let revoked = connect(None, Some(FIXTURE_TENANT_SLUG))
                .expect("resolves")
                .list(card_listing())
                .await
                .expect_err("a revoked login cannot renew");
            assert_eq!(revoked.code(), "WYRD_CLIENT_401_SAVED_LOGIN_UNUSABLE");
            assert!(
                revoked.to_string().contains("(refresh_refused)"),
                "{revoked}"
            );
        }
        other => panic!("unknown script phase {other}"),
    }
}

/// The Rust SDK uses CLI-established user logins from separate local
/// processes: it uses the newest of two same-server tenants' logins without a
/// selector and the named one with it, refuses an unmatched selection, makes
/// the reader's allowed read and is denied its write, renews without the
/// provider, lets an
/// explicit machine credential override, and fails closed once the login's
/// refresh chain is revoked.
///
/// # Panics
/// Panics when any journey step differs.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the Keycloak identity lane; run via `mise run test:identity:journey`"]
async fn saved_user_auth_journey() {
    let config = wyrd_testing::human_login::private_config_home();
    let server = Box::pin(
        WyrdTestServerBuilder::default()
            .with_public_origin(HUMAN_PUBLIC_ORIGIN.parse().expect("origin parses"))
            .start_bound(),
    )
    .await
    .expect("test server starts");
    let base_url = server
        .base_url()
        .expect("bound server has a URL")
        .to_owned();
    let origin = canonical_origin(&base_url).expect("origin");
    let sso = HumanSso::new(&base_url);
    let admin_key = machine_key(&server, "saved_login_admin", &["admin"]).await;
    sso.activate_keycloak(&admin_key).await;
    let second = server
        .seed_tenant("saved-login-two")
        .await
        .expect("tenant seeds");
    let second_admin = match server
        .bootstrap_service_in_tenant(second, "saved_login_two_admin", &["admin"])
        .await
        .expect("second admin bootstraps")
    {
        Bootstrap::Machine { api_key, .. } => api_key.expose_secret().to_owned(),
        Bootstrap::User { .. } => panic!("service bootstrap returned a user principal"),
    };
    sso.activate_keycloak(&second_admin).await;
    sso.save_login(config.path(), FIXTURE_TENANT_SLUG, "bob", "wyrd-test")
        .await;
    sso.save_login(config.path(), "saved-login-two", "alice", "alice-password")
        .await;
    let second = "saved-login-two".to_owned();
    let script =
        |phase: &'static str| run_script(phase, config.path(), &base_url, &second, &admin_key);

    script("select").await;

    let stale = saved_login(config.path(), &origin, FIXTURE_TENANT_SLUG);
    expire_saved_access(config.path(), &origin, FIXTURE_TENANT_SLUG);
    script("renew").await;
    let renewed = saved_login(config.path(), &origin, FIXTURE_TENANT_SLUG);
    assert!(
        renewed.refresh_token.expose() != stale.refresh_token.expose(),
        "the renewal rotated the refresh token"
    );
    assert!(
        renewed.access_expires_at > chrono::Utc::now(),
        "the renewed token is fresh"
    );

    script("override").await;

    sso.revoke(&renewed.refresh_token).await;
    expire_saved_access(config.path(), &origin, FIXTURE_TENANT_SLUG);
    script("revoked").await;

    server.shutdown().await.expect("test server shuts down");
}
