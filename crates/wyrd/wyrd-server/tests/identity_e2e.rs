use std::env;
use std::time::Duration as StdDuration;

use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use chrono::Duration as ChronoDuration;
use serde_json::Value;
use wyrd_auth_verify::WyrdAuthVerifySettings;
use wyrd_testing::{KeycloakAdmin, OidcIssuerFixture, WyrdTestServerBuilder};

fn e2e_enabled() -> bool {
    env::var("WYRD_IDENTITY_E2E").is_ok()
}

fn keycloak_issuer() -> String {
    env::var("WYRD_KEYCLOAK_ISSUER")
        .unwrap_or_else(|_| "http://localhost:8080/realms/wyrd-test".to_owned())
}

fn dex_issuer() -> String {
    env::var("WYRD_DEX_ISSUER").unwrap_or_else(|_| "http://localhost:5556".to_owned())
}

// ─── Commit 08 smoke tests: discovery ────────────────────────────────────────

#[tokio::test]
async fn discovery_resolves_keycloak() {
    if !e2e_enabled() {
        return;
    }
    let fixture = OidcIssuerFixture::connect(&keycloak_issuer()).await;
    let meta = fixture.metadata();
    assert!(
        meta.authorization_endpoint.as_str().contains("openid-connect/auth"),
        "Keycloak authorization_endpoint: {}",
        meta.authorization_endpoint
    );
    assert!(
        meta.token_endpoint
            .as_ref()
            .map(|u| u.as_str().contains("openid-connect/token"))
            .unwrap_or(false),
        "Keycloak token_endpoint missing or unexpected"
    );
    assert!(
        meta.jwks_uri.as_str().contains("openid-connect/certs"),
        "Keycloak jwks_uri: {}",
        meta.jwks_uri
    );
}

#[tokio::test]
async fn discovery_resolves_dex() {
    if !e2e_enabled() {
        return;
    }
    let fixture = OidcIssuerFixture::connect(&dex_issuer()).await;
    let meta = fixture.metadata();
    assert!(
        meta.jwks_uri.as_str().contains("/keys"),
        "Dex jwks_uri: {}",
        meta.jwks_uri
    );
}

// ─── Conformance: JWKS reachable for both providers ───────────────────────────

#[tokio::test]
async fn conformance_jwks_reachable_keycloak() {
    if !e2e_enabled() {
        return;
    }
    let fixture = OidcIssuerFixture::connect(&keycloak_issuer()).await;
    let resp = reqwest::get(fixture.metadata().jwks_uri.clone())
        .await
        .expect("JWKS fetch succeeds");
    assert!(resp.status().is_success(), "Keycloak JWKS returns 200");
    let body: Value = resp.json().await.expect("JWKS is JSON");
    assert!(
        body["keys"].as_array().map(|k| !k.is_empty()).unwrap_or(false),
        "Keycloak JWKS contains at least one key"
    );
}

#[tokio::test]
async fn conformance_jwks_reachable_dex() {
    if !e2e_enabled() {
        return;
    }
    let fixture = OidcIssuerFixture::connect(&dex_issuer()).await;
    let resp = reqwest::get(fixture.metadata().jwks_uri.clone())
        .await
        .expect("JWKS fetch succeeds");
    assert!(resp.status().is_success(), "Dex JWKS returns 200");
    let body: Value = resp.json().await.expect("JWKS is JSON");
    assert!(
        body["keys"].as_array().is_some(),
        "Dex JWKS has 'keys' array"
    );
}

// ─── Journey 3: Workload token claims (Keycloak) ─────────────────────────────

/// Verify that the Keycloak client_credentials token carries the expected
/// audience and issuer claims.  The jwt-bearer route verifies these at exchange
/// time; this test asserts the pre-conditions so a failing route test has a
/// clear root cause.
#[tokio::test]
async fn workload_token_keycloak_claims() {
    if !e2e_enabled() {
        return;
    }
    let keycloak = OidcIssuerFixture::connect(&keycloak_issuer()).await;
    let raw_token = keycloak
        .workload_token("wyrd-workload", "wyrd-workload-secret", "wyrd-workload")
        .await;
    assert!(!raw_token.is_empty(), "workload token is non-empty");

    let parts: Vec<&str> = raw_token.split('.').collect();
    assert_eq!(parts.len(), 3, "workload token has 3 JWT segments");

    let payload = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(parts[1])
        .expect("payload base64 decodes");
    let claims: Value = serde_json::from_slice(&payload).expect("payload is JSON");

    assert_eq!(
        claims["iss"].as_str().unwrap_or(""),
        keycloak_issuer().trim_end_matches('/'),
        "issuer matches Keycloak realm"
    );
    let aud = &claims["aud"];
    let has_aud = aud.as_str() == Some("wyrd-workload")
        || aud
            .as_array()
            .map(|a| a.iter().any(|v| v == "wyrd-workload"))
            .unwrap_or(false);
    assert!(has_aud, "workload token aud contains 'wyrd-workload': {aud}");
}

/// Dex client_credentials token carries the expected issuer claim.
#[tokio::test]
async fn workload_token_dex_claims() {
    if !e2e_enabled() {
        return;
    }
    let dex = OidcIssuerFixture::connect(&dex_issuer()).await;
    let raw_token = dex
        .workload_token("wyrd-workload", "wyrd-workload-dex-secret", "wyrd-workload")
        .await;
    assert!(!raw_token.is_empty(), "Dex workload token is non-empty");

    let parts: Vec<&str> = raw_token.split('.').collect();
    assert_eq!(parts.len(), 3, "Dex workload token has 3 JWT segments");

    let payload = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(parts[1])
        .expect("payload decodes");
    let claims: Value = serde_json::from_slice(&payload).expect("payload is JSON");
    assert_eq!(
        claims["iss"].as_str().unwrap_or(""),
        dex_issuer().trim_end_matches('/'),
        "Dex token iss matches"
    );
}

/// A workload token must NOT carry an unrelated audience.
#[tokio::test]
async fn workload_token_wrong_aud_absent_keycloak() {
    if !e2e_enabled() {
        return;
    }
    let keycloak = OidcIssuerFixture::connect(&keycloak_issuer()).await;
    let raw_token = keycloak
        .workload_token("wyrd-workload", "wyrd-workload-secret", "wyrd-workload")
        .await;
    let parts: Vec<&str> = raw_token.split('.').collect();
    let payload = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(parts[1])
        .expect("decodes");
    let claims: Value = serde_json::from_slice(&payload).expect("JSON");
    let aud = &claims["aud"];
    let has_wrong = aud.as_str() == Some("wrong-audience")
        || aud
            .as_array()
            .map(|a| a.iter().any(|v| v == "wrong-audience"))
            .unwrap_or(false);
    assert!(
        !has_wrong,
        "workload token must NOT contain 'wrong-audience' in aud: {aud}"
    );
}

// ─── Journey 5: TTL expiry ────────────────────────────────────────────────────

/// Mint an access token with `access_ttl = 2s`, verify it works, sleep past
/// `exp`, and assert it is then rejected.
///
/// Uses `WyrdTestServerBuilder::with_access_ttl` and `with_auth_verify_settings`
/// (commit 09 additions) to control real expiry without mocking the clock.
#[tokio::test]
async fn ttl_expiry_journey() {
    if !e2e_enabled() {
        return;
    }

    let srv = WyrdTestServerBuilder::default()
        .with_access_ttl(ChronoDuration::seconds(2))
        .with_auth_verify_settings(WyrdAuthVerifySettings {
            cache_ttl: StdDuration::from_secs(1),
            allowed_clock_skew: StdDuration::ZERO,
            ..WyrdAuthVerifySettings::default()
        })
        .start_in_process()
        .await
        .expect("test server starts");

    let principal = srv
        .bootstrap_service("ttl-svc", &[])
        .await
        .expect("service bootstraps");

    let api_key = principal.api_key().expect("machine principal has api key").clone();
    let access_token = srv
        .exchange_api_key(&api_key)
        .await
        .expect("api key exchange succeeds");

    // Token works before expiry
    let resp = srv
        .oneshot_authenticated(
            &access_token,
            Request::builder()
                .method(Method::GET)
                .uri("/healthz")
                .body(Body::empty())
                .expect("request builds"),
        )
        .await
        .expect("authenticated call completes");
    assert_ne!(
        resp.status(),
        StatusCode::UNAUTHORIZED,
        "token is valid before expiry: {}",
        resp.status()
    );

    // Sleep past access_ttl + cache_ttl (2s TTL + 1s cache = 3s minimum; use 4s)
    tokio::time::sleep(StdDuration::from_secs(4)).await;

    // Same token is now rejected
    let resp = srv
        .oneshot_authenticated(
            &access_token,
            Request::builder()
                .method(Method::GET)
                .uri("/healthz")
                .body(Body::empty())
                .expect("request builds"),
        )
        .await
        .expect("request completes after expiry");
    assert_eq!(
        resp.status(),
        StatusCode::UNAUTHORIZED,
        "token is rejected after expiry"
    );
}

// ─── Key rotation (Keycloak admin API) ───────────────────────────────────────

/// Force a Keycloak signing-key rotation via the admin REST API and verify that
/// both pre- and post-rotation tokens are well-formed JWTs with a `kid` header.
/// The full refetch-on-unknown-kid behavior is exercised when a post-rotation
/// token is presented to the jwt-bearer exchange route.
#[tokio::test]
async fn key_rotation_keycloak_admin_api() {
    if !e2e_enabled() {
        return;
    }
    let keycloak = OidcIssuerFixture::connect(&keycloak_issuer())
        .await
        .with_keycloak_admin(KeycloakAdmin {
            base_url: "http://localhost:8080".to_owned(),
            username: "admin".to_owned(),
            password: "admin".to_owned(),
            realm: "wyrd-test".to_owned(),
        });

    let pre_token = keycloak
        .workload_token("wyrd-workload", "wyrd-workload-secret", "wyrd-workload")
        .await;
    let pre_kid = extract_kid(&pre_token).expect("pre-rotation token has kid");

    keycloak.rotate_signing_key().await;

    let post_token = keycloak
        .workload_token("wyrd-workload", "wyrd-workload-secret", "wyrd-workload")
        .await;
    let post_kid = extract_kid(&post_token).expect("post-rotation token has kid");

    assert!(
        !pre_kid.is_empty() && !post_kid.is_empty(),
        "pre and post rotation tokens both carry a kid header"
    );
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

use base64::Engine as _;

fn extract_kid(jwt: &str) -> Option<String> {
    let header_b64 = jwt.split('.').next()?;
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(header_b64)
        .ok()?;
    let header: Value = serde_json::from_slice(&bytes).ok()?;
    header["kid"].as_str().map(|s| s.to_owned())
}
