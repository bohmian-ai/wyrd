//! Real-server CLI journey for deployment wrapping-key maintenance.
//!
//! A deployment administrator rolls out a replacement wrapping key, then runs
//! `wyrd platform stored-secrets maintain` to move existing stored secrets
//! onto it: routine rotation keeps each value's data key and payload, and
//! compromise recovery reseals them under fresh ones.

use axum::body::Body;
use axum::http::{Method, Request, StatusCode, header};
use secrecy::{ExposeSecret as _, SecretString};
use std::process::Output;
use wyrd_testing::WyrdTestServer;
use wyrd_testing::kms::TransitFixture;

use crate::principal_journey::run_cli_async_with_credential;

/// The one stored envelope this journey seeds.
const ENVELOPE: &str =
    "SELECT secret_ciphertext FROM wyrd.gateway_provider_credentials WHERE name = 'managed'";

/// Read the seeded Gateway credential envelope.
///
/// # Panics
/// Panics when the read fails.
async fn envelope(srv: &WyrdTestServer) -> Vec<u8> {
    sqlx::query_scalar(ENVELOPE)
        .fetch_one(srv.operator_pool().pool())
        .await
        .expect("envelope reads")
}

/// The wrapping-key reference and payload tail (nonce and ciphertext) of
/// `envelope`, per the documented envelope layout.
///
/// # Panics
/// Panics when `envelope` is shorter than its declared fields.
fn parts(envelope: &[u8]) -> (String, Vec<u8>) {
    let ref_len = usize::from(u16::from_be_bytes([envelope[6], envelope[7]]));
    let key_ref = String::from_utf8(envelope[8..8 + ref_len].to_vec()).expect("UTF-8 reference");
    let at = 8 + ref_len;
    let wrapped: [u8; 4] = envelope[at..at + 4].try_into().expect("wrapped length");
    let wrapped_len = usize::try_from(u32::from_be_bytes(wrapped)).expect("fits");
    (key_ref, envelope[at + 4 + wrapped_len..].to_vec())
}

/// Run `wyrd platform stored-secrets maintain --mode <mode>` against `srv`,
/// with `credential` in `WYRD_PLATFORM_CREDENTIAL`.
async fn maintain(srv: &WyrdTestServer, mode: &str, credential: &str) -> Output {
    let arguments = [
        "platform",
        "stored-secrets",
        "maintain",
        "--mode",
        mode,
        "--server",
        srv.base_url().expect("bound server"),
    ]
    .map(str::to_owned)
    .to_vec();
    run_cli_async_with_credential(arguments, "WYRD_PLATFORM_CREDENTIAL", credential.to_owned())
        .await
}

/// Assert a command succeeded and return its stdout.
fn succeeded(what: &str, output: &Output) -> String {
    assert!(
        output.status.success(),
        "{what} failed: stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).into_owned()
}

/// Assert a command failed with one stable error code.
fn failed_with(what: &str, output: &Output, code: &str) {
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !output.status.success() && stderr.contains(code),
        "{what} must fail with {code}: stderr={stderr}"
    );
}

/// Restart `srv` sealing under Transit key `active`, keeping `retained`
/// readable.
async fn roll_out(
    srv: WyrdTestServer,
    transit: &TransitFixture,
    active: &str,
    retained: &[&str],
) -> WyrdTestServer {
    Box::pin(srv.restart_bound(
        WyrdTestServer::builder().with_kms_for_test(transit.config(active, retained)),
    ))
    .await
    .expect("server restarts")
}

/// Exchange `credential` for a tenant access token.
async fn tenant_token(srv: &WyrdTestServer, credential: &str) -> String {
    srv.exchange_api_key(&SecretString::from(credential.to_owned()))
        .await
        .expect("tenant credential exchanges")
}

/// A deployment administrator rotates, then recovers, a stored secret through
/// the shipped binary: missing and tenant credentials are refused without
/// touching it; routine moves it to the active key and keeps its payload;
/// recovery reseals it under a fresh data key; and the Gateway owner opens it
/// with only the final key configured.
///
/// # Panics
/// Panics when a refusal succeeds or mutates the envelope, a pass prints
/// unexpected counts or secret material, a mode moves the payload wrongly, or
/// the owner cannot open its secret afterwards.
#[tokio::test]
async fn administrator_rotates_and_recovers_stored_secrets_through_the_cli() {
    let transit = TransitFixture::start().await;
    let srv = Box::pin(
        WyrdTestServer::builder()
            .with_kms_for_test(transit.config("k1", &[]))
            .start_bound(),
    )
    .await
    .expect("server starts");
    let root = srv
        .initialize_platform_root()
        .await
        .expect("deployment initializes");
    let platform = root.expose_secret().to_owned();
    let created = succeeded(
        "tenant create",
        &run_cli_async_with_credential(
            [
                "platform",
                "tenant",
                "create",
                "--slug",
                "cli-keys",
                "--display-name",
                "CLI Keys",
                "--server",
                srv.base_url().expect("bound server"),
            ]
            .map(str::to_owned)
            .to_vec(),
            "WYRD_PLATFORM_CREDENTIAL",
            platform.clone(),
        )
        .await,
    );
    let admin = created
        .lines()
        .find_map(|line| line.strip_prefix("admin_credential: "))
        .expect("admin credential printed")
        .trim()
        .to_owned();
    let request = Request::builder()
        .method(Method::PUT)
        .uri("/v1/admin/gateway/provider-credentials/managed")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(
            serde_json::json!({
                "name": "managed",
                "provider": "acme",
                "source": { "managed_secret": { "secret": "sk-cli-keys" } },
            })
            .to_string(),
        ))
        .expect("request builds");
    let resp = srv
        .oneshot_authenticated(&tenant_token(&srv, &admin).await, request)
        .await
        .expect("credential route responds");
    assert!(resp.status().is_success(), "the credential stores");
    let sealed = envelope(&srv).await;

    let srv = roll_out(srv, &transit, "k2", &["k1"]).await;
    failed_with(
        "maintain without a platform credential",
        &maintain(&srv, "routine", "").await,
        "WYRD_CLI_401_NO_PLATFORM_CREDENTIAL",
    );
    failed_with(
        "maintain with a tenant credential",
        &maintain(&srv, "routine", &admin).await,
        "401",
    );
    assert_eq!(envelope(&srv).await, sealed, "refusals moved nothing");

    let routine = succeeded("routine pass", &maintain(&srv, "routine", &platform).await);
    for line in [
        "mode: routine",
        "current: 0",
        "changed: 1",
        "remaining: 0",
        "bounded: false",
    ] {
        assert!(routine.lines().any(|out| out == line), "{line}: {routine}");
    }
    assert!(
        !routine.contains("sk-cli-keys") && !routine.contains("k1"),
        "{routine}"
    );
    let rotated = envelope(&srv).await;
    assert_eq!(parts(&rotated).0, "k2");
    assert_eq!(
        parts(&rotated).1,
        parts(&sealed).1,
        "routine keeps the payload"
    );

    // An incident: activate a fresh, never-reused key and renew.
    let srv = roll_out(srv, &transit, "k3", &["k2"]).await;
    let recovery = succeeded(
        "recovery pass",
        &maintain(&srv, "recovery", &platform).await,
    );
    assert!(
        recovery.lines().any(|out| out == "changed: 1"),
        "{recovery}"
    );
    let renewed = envelope(&srv).await;
    assert_eq!(parts(&renewed).0, "k3");
    assert_ne!(
        parts(&renewed).1,
        parts(&rotated).1,
        "recovery reseals the payload"
    );

    let srv = roll_out(srv, &transit, "k3", &[]).await;
    let request = Request::builder()
        .method(Method::POST)
        .uri("/v1/admin/gateway/provider-credentials/managed/renew-encryption")
        .body(Body::empty())
        .expect("request builds");
    let resp = srv
        .oneshot_authenticated(&tenant_token(&srv, &admin).await, request)
        .await
        .expect("renew route responds");
    assert_eq!(resp.status(), StatusCode::OK, "the owner opens its secret");
    srv.shutdown().await.expect("server shuts down");
}
