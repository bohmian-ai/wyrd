//! Provider-backed journey for `wyrd auth login`, `status`, and `logout`.
//!
//! The shipped binary signs in through a real Keycloak connection: it begins a
//! one-use handoff, the fixture user signs in at the printed provider URL, the
//! provider's return reaches the server callback, and the CLI's poll claims
//! the Wyrd user credential into the private saved-login store. The handoff's
//! refusals — wrong verifier, wrong tenant, replay, expiry — are driven over
//! the same served routes, and logout revokes only its own login's refresh
//! chain, also when the server is unreachable. No command output carries a
//! token. The ignored root test `cli_oidc_handoff_journey` in `cli.rs` runs
//! it, so the identity lane selects it by its exact name.

use std::io::{BufRead, BufReader};
use std::path::Path;
use std::process::{Command, Output, Stdio};

use assert_cmd::prelude::*;
use secrecy::ExposeSecret;
use url::Url;
use wyrd_client::auth::TokenExchange;
use wyrd_client::saved_login::SavedLoginState;
use wyrd_spec::auth::{CliHandoffClaim, CliHandoffProof, SecretBearer, TokenRequest};
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

/// The stable code of a refused `/auth` call.
///
/// # Panics
/// Panics when the call succeeded.
fn refusal<T: std::fmt::Debug>(result: Result<T, wyrd_client::auth::AuthError>) -> String {
    result
        .expect_err("the call is refused")
        .into_wyrd()
        .code()
        .to_owned()
}

/// `wyrd auth login` completes a browser sign-in into a saved login without
/// printing a token; the handoff refuses every wrong initiator, replay, and
/// expiry; and logout ends only its own login, locally even when the server
/// is down.
///
/// # Panics
/// Panics when any step of the journey differs.
pub(crate) async fn cli_oidc_handoff_journey() {
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
    let config = tempfile::tempdir().expect("config home");
    let tenant: TenantSlug = FIXTURE_TENANT_SLUG.parse().expect("slug");

    // The shipped CLI prints the provider URL, the person signs in there, and
    // the CLI's own poll saves the credential.
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
    let login_url: Url = url_rx
        .await
        .expect("the CLI prints the sign-in URL")
        .parse()
        .expect("sign-in URL parses");
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
    assert_eq!(record.tenant_id, srv.data_tenant_id());
    let SavedLoginState::Ready {
        access_token,
        refresh_token,
        ..
    } = &record.state
    else {
        panic!("the saved login is ready: {:?}", record.summary());
    };
    assert!(
        printed.contains(&record.principal_id.to_string()),
        "{printed}"
    );
    for secret in [access_token.expose(), refresh_token.expose()] {
        assert!(!printed.contains(secret), "login output carries no token");
        assert!(!page.contains(secret), "the browser page carries no token");
        assert!(
            !login_url.as_str().contains(secret),
            "the URL carries no token"
        );
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

    // Refusals: no credential reaches a wrong verifier, a wrong tenant, a
    // replayed claim, or an expired handoff.
    let exchange = TokenExchange::new(&server, 30_000).expect("exchange builds");
    let handoff = exchange
        .begin_cli_handoff(&tenant)
        .await
        .expect("handoff begins");
    let proof = CliHandoffProof {
        tenant_route_key: tenant.clone(),
        poll_verifier: handoff.poll_verifier.clone(),
    };
    let wrong_verifier = CliHandoffProof {
        tenant_route_key: tenant.clone(),
        poll_verifier: SecretBearer::new("not-the-verifier".to_owned()),
    };
    let wrong_tenant = CliHandoffProof {
        tenant_route_key: "some-other-tenant".parse().expect("slug"),
        poll_verifier: handoff.poll_verifier.clone(),
    };
    assert_eq!(
        exchange
            .claim_cli_handoff(handoff.handoff_id, &proof)
            .await
            .expect("pending"),
        CliHandoffClaim::Pending {
            retry_after_seconds: 2
        }
    );
    let handoff_url: Url = handoff.login_url.as_str().parse().expect("URL parses");
    sso.sign_in(&handoff_url, "bob", "wyrd-test").await;
    assert_eq!(
        refusal(
            exchange
                .claim_cli_handoff(handoff.handoff_id, &wrong_verifier)
                .await
        ),
        "WYRD_AUTH_400_INVALID_STATE"
    );
    assert!(
        exchange
            .claim_cli_handoff(handoff.handoff_id, &wrong_tenant)
            .await
            .is_err(),
        "another tenant's route key claims nothing"
    );
    let CliHandoffClaim::Complete(second) = exchange
        .claim_cli_handoff(handoff.handoff_id, &proof)
        .await
        .expect("the verifier holder claims")
    else {
        panic!("the signed-in handoff completes");
    };
    assert_eq!(
        refusal(exchange.claim_cli_handoff(handoff.handoff_id, &proof).await),
        "WYRD_AUTH_400_INVALID_STATE",
        "a replayed claim returns nothing"
    );
    let expiring = exchange
        .begin_cli_handoff(&tenant)
        .await
        .expect("handoff begins");
    sqlx::query(
        "UPDATE wyrd.auth_cli_handoffs SET expires_at = statement_timestamp() - interval '1 \
         second' WHERE handoff_id = $1",
    )
    .bind(expiring.handoff_id)
    .execute(
        &srv.pg_fixture()
            .superuser_pool()
            .await
            .expect("superuser pool opens"),
    )
    .await
    .expect("handoff expires");
    let expired_proof = CliHandoffProof {
        tenant_route_key: tenant.clone(),
        poll_verifier: expiring.poll_verifier,
    };
    assert_eq!(
        refusal(
            exchange
                .claim_cli_handoff(expiring.handoff_id, &expired_proof)
                .await
        ),
        "WYRD_AUTH_400_INVALID_STATE",
        "an expired handoff returns nothing"
    );

    // Logout revokes the saved login's chain and only it: the second login of
    // the same person still renews.
    let saved_refresh = refresh_token.clone();
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
    assert!(
        exchange
            .exchange(&TokenRequest::RefreshToken {
                refresh_token: saved_refresh,
            })
            .await
            .is_err(),
        "the logged-out chain no longer renews"
    );
    let renewed = exchange
        .exchange(&TokenRequest::RefreshToken {
            refresh_token: second.refresh_token.clone(),
        })
        .await
        .expect("the other login still renews");

    // With the server gone, logout still removes the saved login and says the
    // revocation was not confirmed.
    let mut offline = second;
    offline.refresh_token = renewed
        .refresh_token
        .expect("rotation returns a refresh token");
    saved_logins(config.path())
        .save(wyrd_client::saved_login::SavedLogin::from_cli_login(
            wyrd_client::saved_login::canonical_origin(&server).expect("origin"),
            tenant.clone(),
            offline,
        ))
        .expect("saves");
    srv.shutdown().await.expect("server stops");
    let offline_logout = run(config.path(), &["auth", "logout", "--server", &server]).await;
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
