//! End-to-end auth test against the real `WyrdTestServer` router.
//!
//! It assembles a real [`WyrdClient`] from config — credential resolution →
//! `AuthMiddleware` API-key exchange → `HttpTransport` — and drives requests
//! *through the client* against the live server, proving the data-plane header
//! contract: the server authenticates from `x-wyrd-access-token` and does
//! **not** read the application-owned `Authorization` header.
//!
//! Two signals, both decisive:
//!  1. **Token-layer e2e** — `client.auth().bearer()` exchanges the resolved
//!     API key at the real `/auth/token` and mints a JWT. A success proves the
//!     client's API-key → JWT path works against the live server.
//!  2. **Data-plane header contract** — `client.request_json` (which sends
//!     `x-wyrd-access-token`) to `GET /v1/cards` is *authenticated* and
//!     rejected only at the permission check (`403 PERMISSION_403_DENIED_RBAC`,
//!     the service holds no role), never `401`. The negative control re-sends
//!     the *same* JWT in `Authorization` only and gets `401`, proving the
//!     server ignores it.
//!
//! A role-less principal on the card list is used deliberately: the route
//! needs no pre-created card, and its permission check runs right after
//! authentication — so a `403` there is an unambiguous "auth passed" signal,
//! while the `Authorization`-only control yields a clean `401`.
//!
//! Ungated: it runs in the Postgres test
//! lane the fixture requires.

// Wrapped in `mod pg_tests` so the fast family lane skips it via
// `--skip pg_tests` (it needs the real `WyrdTestServer` + Postgres); the
// infra e2e lane selects it by `--test` and runs it.
use std::os::unix::fs::PermissionsExt;

use secrecy::ExposeSecret;
use wyrd_client::saved_login::canonical_origin;
use wyrd_spec::ids::TenantSlug;
use wyrd_testing::human_login::{
    FIXTURE_TENANT_SLUG, HUMAN_PUBLIC_ORIGIN, HumanSso, expire_saved_access, saved_login,
    saved_logins,
};

mod pg_tests {
    use wyrd_client::WyrdClient;
    use wyrd_client::config::ClientConfig;

    #[tokio::test(flavor = "multi_thread")]
    async fn wyrd_client_authenticates_via_wyrd_access_token_header() {
        let srv = wyrd_testing::WyrdTestServer::start_bound()
            .await
            .expect("test server starts");
        let base_url = srv
            .base_url()
            .expect("bound server has a base url")
            .to_owned();

        // A real, hashed-into-the-DB service API key.
        let bootstrap = srv
            .bootstrap_service("wyrd-client-e2e", &[])
            .await
            .expect("service bootstraps");
        let api_key = bootstrap
            .api_key()
            .expect("machine bootstrap yields an api key")
            .clone();

        // Assemble the full client the way a caller would: config in, client out.
        // Setting `api_key` makes resolve_credential pick it as the tier-0 source,
        // so this also exercises credential resolution, not just direct wiring.
        let mut config = ClientConfig::default();
        config.http.base_url = base_url.clone();
        config.credential = Some(api_key);
        let client = WyrdClient::with_config(config).expect("client assembles");

        // (1) Token-layer e2e: the real /auth/token accepts the resolved API key and
        // mints a JWT. Reused below as the negative-control credential.
        let jwt = client
            .auth()
            .bearer()
            .await
            .expect("api key exchanges for an access token against the real server")
            .expose()
            .to_owned();

        // (2) Data-plane: the request carries x-wyrd-access-token, so the server
        // authenticates the principal and rejects only at the permission check.
        let err = client
            .request_json::<serde_json::Value, serde_json::Value>(
                reqwest::Method::GET,
                "/v1/cards",
                None,
            )
            .await
            .expect_err("a role-less token is authenticated, then permission-rejected");
        assert_eq!(
            err.code(),
            "WYRD_PERMISSION_403_DENIED_RBAC",
            "auth must succeed from x-wyrd-access-token (403 at the permission check), not fail at 401; got {err:?}"
        );

        // Negative control: the SAME JWT in Authorization only must be unauthenticated,
        // proving the server never reads Authorization for the data plane.
        let raw = reqwest::Client::new()
            .get(format!("{base_url}/v1/cards"))
            .header("Authorization", format!("Bearer {jwt}"))
            .send()
            .await
            .expect("control request sends");
        assert_eq!(
            raw.status().as_u16(),
            401,
            "a valid JWT presented only in Authorization must be rejected as unauthenticated"
        );

        let _ = srv.shutdown().await;
    }
}

/// Environment marking a [`saved_renewal_child`] process.
const RENEWAL_CHILD: &str = "WYRD_SAVED_RENEWAL_CHILD";

/// One local process minting through the saved login, as an SDK script does,
/// run only as the child [`mint_in_children`] starts.
///
/// Prints `outcome=ok` or `outcome=<error>`; the error carries the stable
/// reason and never a token.
///
/// # Panics
/// Panics when the client cannot be assembled for a reason other than the
/// saved login.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "a child process of concurrent_saved_renewal"]
async fn saved_renewal_child() {
    if std::env::var(RENEWAL_CHILD).is_err() {
        return;
    }
    let mut config = wyrd_client::config::ClientConfig::from_env();
    config.tenant = Some(FIXTURE_TENANT_SLUG.to_owned());
    let outcome = match wyrd_client::WyrdClient::with_config(config) {
        Ok(client) => client
            .auth()
            .bearer()
            .await
            .map(|_| ())
            .map_err(|error| error.to_string()),
        Err(error) => Err(error.to_string()),
    };
    match outcome {
        Ok(()) => println!("outcome=ok"),
        Err(error) => println!("outcome={error}"),
    }
}

/// Start `count` separate processes minting through the saved login under
/// `config` at once, and return each one's printed outcome.
///
/// # Panics
/// Panics when a child cannot run or prints no outcome.
async fn mint_in_children(count: usize, config: &std::path::Path, server: &str) -> Vec<String> {
    let children: Vec<_> = (0..count)
        .map(|_| {
            std::process::Command::new(std::env::current_exe().expect("test binary"))
                .args([
                    "--exact",
                    "pg_auth_e2e_against_fixture::saved_renewal_child",
                    "--include-ignored",
                    "--nocapture",
                ])
                .env(RENEWAL_CHILD, "1")
                .env("WYRD_CONFIG_HOME", config)
                .env("WYRD_SERVER_URL", server)
                .env_remove("WYRD_ACCESS_TOKEN")
                .env_remove("WYRD_WORKLOAD_TOKEN")
                .env_remove("WYRD_API_KEY")
                .env_remove("WYRD_TENANT")
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .spawn()
                .expect("child starts")
        })
        .collect();
    tokio::task::spawn_blocking(move || {
        children
            .into_iter()
            .map(|child| {
                let output = child.wait_with_output().expect("child exits");
                String::from_utf8_lossy(&output.stdout)
                    .lines()
                    .find_map(|line| line.strip_prefix("outcome=").map(ToOwned::to_owned))
                    .unwrap_or_else(|| {
                        panic!(
                            "child printed no outcome: {}",
                            String::from_utf8_lossy(&output.stderr)
                        )
                    })
            })
            .collect()
    })
    .await
    .expect("children join")
}

/// Separate local processes sharing one saved login renew it once under the
/// file lock and never replay a rotated refresh token; an unsafe file fails
/// closed; and a logout racing renewals leaves no record behind.
///
/// # Panics
/// Panics when any step differs.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the Keycloak identity lane; run via `mise run test:identity:journey`"]
async fn concurrent_saved_renewal() {
    let srv = wyrd_testing::WyrdTestServerBuilder::default()
        .with_public_origin(HUMAN_PUBLIC_ORIGIN.parse().expect("origin parses"))
        .start_bound()
        .await
        .expect("server starts");
    let server = srv.base_url().expect("bound server has a URL").to_owned();
    let origin = canonical_origin(&server).expect("origin");
    let admin = srv
        .bootstrap_service("renewal-admin", &["admin"])
        .await
        .expect("admin bootstraps");
    let sso = HumanSso::new(&server);
    sso.activate_keycloak(admin.api_key().expect("admin key").expose_secret())
        .await;
    let config = wyrd_testing::human_login::private_config_home();
    sso.save_login(config.path(), FIXTURE_TENANT_SLUG, "bob", "wyrd-test")
        .await;
    let store = saved_logins(config.path());
    let current = || saved_login(config.path(), &origin, FIXTURE_TENANT_SLUG);

    // Four processes race one stale login: the first rotates it under the
    // lock, the rest reread the file and use the rotated token.
    let stale = current();
    expire_saved_access(config.path(), &origin, FIXTURE_TENANT_SLUG);
    let outcomes = mint_in_children(4, config.path(), &server).await;
    assert!(
        outcomes.iter().all(|outcome| outcome == "ok"),
        "{outcomes:?}"
    );
    let winner = current();
    assert_ne!(
        winner.refresh_token.expose(),
        stale.refresh_token.expose(),
        "the refresh token rotated"
    );
    assert!(winner.access_expires_at > chrono::Utc::now());

    // The winner's chain was never replayed, so it still renews.
    expire_saved_access(config.path(), &origin, FIXTURE_TENANT_SLUG);
    assert_eq!(mint_in_children(1, config.path(), &server).await, ["ok"]);
    assert_ne!(
        current().refresh_token.expose(),
        winner.refresh_token.expose()
    );

    // An unsafe file is refused, not read.
    {
        let record = config.path().join("credentials.toml");
        std::fs::set_permissions(
            &record,
            <std::fs::Permissions as PermissionsExt>::from_mode(0o644),
        )
        .expect("chmod");
        let unsafe_store = mint_in_children(1, config.path(), &server).await;
        assert!(
            unsafe_store[0].contains("(unsafe_store)"),
            "{unsafe_store:?}"
        );
        std::fs::set_permissions(
            &record,
            <std::fs::Permissions as PermissionsExt>::from_mode(0o600),
        )
        .expect("chmod");
    }

    // A logout racing renewals leaves no record behind: it deletes under the
    // same lock the renewals hold.
    expire_saved_access(config.path(), &origin, FIXTURE_TENANT_SLUG);
    let tenant_key = TenantSlug::new(FIXTURE_TENANT_SLUG).expect("tenant key");
    let racers = mint_in_children(3, config.path(), &server);
    let logout = {
        let (store, origin, server) = (store.clone(), origin.clone(), server.clone());
        async move {
            let removed = tokio::task::spawn_blocking(move || store.remove(&origin, &tenant_key))
                .await
                .expect("joins")
                .expect("removes");
            if let Some(login) = removed {
                HumanSso::new(&server).revoke(&login.refresh_token).await;
            }
        }
    };
    let (outcomes, ()) = tokio::join!(racers, logout);
    assert!(
        store.list().expect("lists").is_empty(),
        "no renewal restored a logged-out login: {outcomes:?}"
    );
    // Each racer renewed before the logout, found the login gone under the
    // lock, or started after it and found no credential at all.
    assert!(
        outcomes.iter().all(|outcome| outcome == "ok"
            || outcome.contains("(logged_out)")
            || outcome.starts_with("no credentials available")),
        "{outcomes:?}"
    );

    srv.shutdown().await.expect("server stops");
}
