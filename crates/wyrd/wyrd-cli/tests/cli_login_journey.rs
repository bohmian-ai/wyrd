//! Provider-backed journey for `wyrd auth login`, `status`, and `logout`.
//!
//! The shipped binary signs in through a real Keycloak connection with the
//! RFC 8628 device-code grant: it prints a user code and the verification
//! URL, the fixture user approves the code there and signs in at the provider
//! it redirects to, the provider's return reaches the server callback, and
//! the CLI's token poll saves the Wyrd user credential into the private
//! saved-login store. The device code's refusals — cross-origin or unknown
//! approval, replay, denial, expiry — are driven through the same `oauth2`
//! client over the served routes, and logout revokes only its own login's refresh
//! chain, and still deletes the record when the server is unreachable. No
//! command output carries a token. The ignored root test
//! `cli_device_login_journey` in `cli.rs` runs it, so the identity lane
//! selects it by its exact name.

use std::io::{BufRead, BufReader};
use std::path::Path;
use std::process::{Command, Output, Stdio};

use assert_cmd::prelude::*;
use secrecy::ExposeSecret;
use url::Url;
use wyrd_client::auth::{AuthError, TokenExchange};
use wyrd_client::saved_login::{SavedLogin, canonical_origin};
use wyrd_spec::ids::TenantSlug;
use wyrd_testing::WyrdTestServerBuilder;
use wyrd_testing::human_login::{FIXTURE_TENANT_SLUG, HUMAN_PUBLIC_ORIGIN, HumanSso, saved_logins};

/// The shipped CLI with every ambient `WYRD_*` source cleared and its
/// configuration directory at `config_home`.
///
/// # Panics
/// Panics when the `wyrd` binary cannot be built or located.
fn cli(config_home: &Path) -> Command {
    let mut command = Command::cargo_bin("wyrd").expect("wyrd binary builds");
    command
        .env("WYRD_CONFIG_HOME", config_home)
        .env_remove("WYRD_ACCESS_TOKEN")
        .env_remove("WYRD_WORKLOAD_TOKEN")
        .env_remove("WYRD_TENANT")
        .env_remove("WYRD_API_KEY")
        .env_remove("WYRD_SERVER_URL")
        .env_remove("WYRD_REFRESH_TOKEN");
    command
}

/// Run one CLI command off the async runtime.
///
/// # Panics
/// Panics when the process cannot run.
async fn run(config_home: &Path, arguments: &[&str]) -> Output {
    let mut command = cli(config_home);
    command.args(arguments);
    tokio::task::spawn_blocking(move || command.output().expect("wyrd command runs"))
        .await
        .expect("CLI subprocess joins")
}

/// Both output streams of a finished command.
fn transcript(output: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

/// The RFC 6749 §5.2 / RFC 8628 §3.5 `error` of a refused device-code poll,
/// which the client carries in the problem's `details.error`.
///
/// # Panics
/// Panics when the poll succeeded or the refusal names no RFC error.
fn device_error<T: std::fmt::Debug>(result: Result<T, AuthError>) -> String {
    let problem = result
        .expect_err("the poll is refused")
        .into_wyrd()
        .problem();
    problem.details["error"]
        .as_str()
        .expect("the refusal names its error")
        .to_owned()
}

/// Post a decision on `user_code` to the verification page, from `origin`
/// when given, and return the response status.
///
/// # Panics
/// Panics when the page does not answer.
async fn decide(
    server: &str,
    origin: Option<&str>,
    tenant: &str,
    user_code: &str,
    decision: &str,
) -> reqwest::StatusCode {
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .expect("client builds");
    let mut request = client.post(format!("{server}/auth/device")).form(&[
        ("tenant", tenant),
        ("user_code", user_code),
        ("decision", decision),
    ]);
    if let Some(origin) = origin {
        request = request.header(reqwest::header::ORIGIN, origin);
    }
    request
        .send()
        .await
        .expect("verification page answers")
        .status()
}

/// `wyrd auth login` refuses a remote cleartext server and completes a
/// device-code sign-in into a saved login without printing a token; a device
/// code issues nothing while pending, to a wrong code, on a cross-origin or
/// unknown approval, on replay, after denial, or after expiry; and logout
/// ends only its own login, locally even when the server is down.
///
/// # Panics
/// Panics when any step of the journey differs.
pub(crate) async fn cli_device_login_journey() {
    let srv = WyrdTestServerBuilder::default()
        .with_public_origin(HUMAN_PUBLIC_ORIGIN.parse().expect("origin parses"))
        .start_bound()
        .await
        .expect("server starts");
    let server = srv.base_url().expect("bound server has a URL").to_owned();
    let admin = srv
        .bootstrap_service("cli-login-admin", &["admin"])
        .await
        .expect("admin bootstraps");
    let sso = HumanSso::new(&server);
    sso.activate_keycloak(admin.api_key().expect("admin key").expose_secret())
        .await;
    let config = wyrd_testing::human_login::private_config_home();
    let tenant: TenantSlug = FIXTURE_TENANT_SLUG.parse().expect("slug");

    // A remote cleartext server never receives a device-code request or a
    // refresh token: login and refresh refuse it before any request.
    for arguments in [
        &[
            "auth",
            "login",
            "--server",
            "http://wyrd.example.com",
            "--tenant",
            FIXTURE_TENANT_SLUG,
            "--no-browser",
        ][..],
        &["auth", "refresh", "--server", "http://wyrd.example.com"][..],
    ] {
        let mut command = cli(config.path());
        command
            .args(arguments)
            .env("WYRD_REFRESH_TOKEN", "not-a-real-token");
        let output = tokio::task::spawn_blocking(move || command.output().expect("wyrd runs"))
            .await
            .expect("CLI subprocess joins");
        assert!(!output.status.success(), "{}", transcript(&output));
        assert!(
            transcript(&output).contains("remote cleartext HTTP is not allowed"),
            "{}",
            transcript(&output)
        );
    }
    // A server URL carrying userinfo is refused before any request, and the
    // password is never echoed.
    let with_userinfo = server.replacen("://", "://alice:hunter2@", 1);
    for arguments in [
        &[
            "auth",
            "login",
            "--server",
            &with_userinfo,
            "--tenant",
            FIXTURE_TENANT_SLUG,
            "--no-browser",
        ][..],
        &["auth", "logout", "--server", &with_userinfo][..],
    ] {
        let output = run(config.path(), arguments).await;
        let text = transcript(&output);
        assert!(!output.status.success(), "{text}");
        assert!(text.contains("userinfo"), "{text}");
        assert!(!text.contains("hunter2"), "{text}");
    }
    assert!(
        saved_logins(config.path())
            .list()
            .expect("lists")
            .is_empty()
    );

    // The shipped CLI prints the user code and verification URL; the person
    // approves the code there and signs in, and the CLI's own poll saves the
    // credential.
    let mut command = cli(config.path());
    command
        .args([
            "auth",
            "login",
            "--server",
            &server,
            "--tenant",
            FIXTURE_TENANT_SLUG,
        ])
        .arg("--no-browser")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command.spawn().expect("wyrd auth login starts");
    let stderr = child.stderr.take().expect("stderr is piped");
    let (url_tx, url_rx) = tokio::sync::oneshot::channel();
    let reader = std::thread::spawn(move || {
        let mut url_tx = Some(url_tx);
        let mut text = String::new();
        for line in BufReader::new(stderr).lines() {
            let line = line.expect("stderr reads");
            if let Some(url) = line
                .trim()
                .strip_prefix("http")
                .map(|rest| format!("http{rest}"))
                && let Some(url_tx) = url_tx.take()
            {
                let _ = url_tx.send(url);
            }
            text.push_str(&line);
            text.push('\n');
        }
        text
    });
    let verification_url: Url = url_rx
        .await
        .expect("the CLI prints the verification URL")
        .parse()
        .expect("verification URL parses");
    let (_, user_code) = verification_url
        .query_pairs()
        .find(|(name, _)| name == "user_code")
        .expect("the URL carries the user code");
    let login_url = sso.approve(&tenant, &user_code).await;
    let page = sso.sign_in(&login_url, "bob", "wyrd-test").await;
    let output = tokio::task::spawn_blocking(move || child.wait_with_output())
        .await
        .expect("CLI joins")
        .expect("CLI exits");
    let stderr = reader.join().expect("stderr reader joins");
    let printed = format!("{}{stderr}", String::from_utf8_lossy(&output.stdout));
    assert!(output.status.success(), "login succeeds: {printed}");

    let saved = saved_logins(config.path()).list().expect("store lists");
    assert_eq!(saved.len(), 1, "one saved login");
    let record = &saved[0];
    assert_eq!(record.tenant_key, tenant);
    assert!(printed.contains(user_code.as_ref()), "{printed}");
    let (access_token, refresh_token) = (&record.access_token, &record.refresh_token);
    for secret in [access_token.expose(), refresh_token.expose()] {
        assert!(!printed.contains(secret), "login output carries no token");
        assert!(!page.contains(secret), "the browser page carries no token");
        for url in [&verification_url, &login_url] {
            assert!(!url.as_str().contains(secret), "the URL carries no token");
        }
    }

    let status = run(config.path(), &["auth", "status"]).await;
    let status_text = transcript(&status);
    assert!(status.status.success(), "{status_text}");
    assert!(status_text.contains(FIXTURE_TENANT_SLUG), "{status_text}");
    assert!(
        !status_text.contains(access_token.expose()),
        "status prints no token"
    );
    assert!(
        !status_text.contains(refresh_token.expose()),
        "status prints no token"
    );

    // Refusals: no credential reaches a cross-origin or unknown approval, a
    // replay, a denied code, or an expired one. The server's single-poll
    // answers (pending, unknown code) are its own device-grant journey's.
    let exchange = TokenExchange::new(&server, 30_000).expect("exchange builds");
    let device = exchange
        .device_authorization(&tenant)
        .await
        .expect("device login begins");
    assert_eq!(
        decide(
            &server,
            None,
            FIXTURE_TENANT_SLUG,
            device.user_code().secret(),
            "approve"
        )
        .await,
        reqwest::StatusCode::FORBIDDEN,
        "a post from another origin approves nothing"
    );
    for (tenant, user_code) in [
        ("some-other-tenant", device.user_code().secret().as_str()),
        (FIXTURE_TENANT_SLUG, "BCDF-GHJK"),
    ] {
        assert_eq!(
            decide(
                &server,
                Some(HUMAN_PUBLIC_ORIGIN),
                tenant,
                user_code,
                "approve"
            )
            .await,
            reqwest::StatusCode::BAD_REQUEST,
            "an unknown code or tenant approves nothing"
        );
    }
    let approved = sso.approve(&tenant, device.user_code().secret()).await;
    sso.sign_in(&approved, "bob", "wyrd-test").await;
    let second = exchange
        .device_access_token(&device)
        .await
        .expect("the approved device code redeems");
    assert_eq!(
        device_error(exchange.device_access_token(&device).await),
        "invalid_grant",
        "a redeemed device code returns nothing"
    );

    let denied = exchange
        .device_authorization(&tenant)
        .await
        .expect("device login begins");
    assert_eq!(
        decide(
            &server,
            Some(HUMAN_PUBLIC_ORIGIN),
            FIXTURE_TENANT_SLUG,
            denied.user_code().secret(),
            "deny"
        )
        .await,
        reqwest::StatusCode::OK
    );
    assert_eq!(
        device_error(exchange.device_access_token(&denied).await),
        "access_denied"
    );

    let expiring = exchange
        .device_authorization(&tenant)
        .await
        .expect("device login begins");
    sqlx::query(
        "UPDATE wyrd.auth_device_authorizations SET created_at = statement_timestamp() - \
         interval '11 minutes', expires_at = statement_timestamp() - interval '1 minute' \
         WHERE user_code = $1",
    )
    .bind(expiring.user_code().secret())
    .execute(
        &srv.pg_fixture()
            .superuser_pool()
            .expect("superuser pool opens"),
    )
    .await
    .expect("device code expires");
    assert_eq!(
        device_error(exchange.device_access_token(&expiring).await),
        "expired_token",
        "an expired device code returns nothing"
    );

    // Logout revokes the saved login's chain and only it: the second login of
    // the same person still renews.
    let saved_refresh = refresh_token.clone();
    let second_refresh = second
        .refresh_token
        .clone()
        .expect("a device login returns a refresh token");
    let logout = run(
        config.path(),
        &[
            "auth",
            "logout",
            "--server",
            &server,
            "--tenant",
            FIXTURE_TENANT_SLUG,
        ],
    )
    .await;
    assert!(logout.status.success(), "{}", transcript(&logout));
    assert!(
        saved_logins(config.path())
            .list()
            .expect("lists")
            .is_empty()
    );
    let renewed = exchange
        .refresh(&second_refresh)
        .await
        .expect("the other login still renews");
    // Presenting the revoked token is a replay, so it is checked last: the
    // server's containment then ends every chain of this person.
    assert!(
        exchange.refresh(&saved_refresh).await.is_err(),
        "the logged-out chain no longer renews"
    );

    // With the server gone, logout still removes the saved login and says the
    // revocation was not confirmed.
    saved_logins(config.path())
        .save(
            SavedLogin::from_token(
                canonical_origin(&server).expect("origin"),
                tenant.clone(),
                renewed,
            )
            .expect("rotation returns a refresh token"),
        )
        .expect("saves");
    srv.shutdown().await.expect("server stops");
    // Another spelling of the same origin selects the same saved login.
    let respelled = format!(
        "{}/some/path/?x=1#f",
        server.replacen("http://", "HTTP://", 1)
    );
    let offline_logout = run(config.path(), &["auth", "logout", "--server", &respelled]).await;
    let offline_text = transcript(&offline_logout);
    assert!(offline_logout.status.success(), "{offline_text}");
    assert!(offline_text.contains("warning"), "{offline_text}");
    assert!(
        saved_logins(config.path())
            .list()
            .expect("lists")
            .is_empty()
    );
}
