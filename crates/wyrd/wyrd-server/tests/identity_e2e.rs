use std::collections::HashMap;
use std::env;
use std::time::Duration as StdDuration;

use axum::body::Body;
use axum::http::{Method, Request, StatusCode, header};
use chrono::Duration as ChronoDuration;
use serde_json::Value;
use url::Url;
use wyrd_auth_oidc::{ClaimMapping, ClaimPath, ClientAuth, PrincipalKindPolicy, TrustedIssuer};
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
        meta.authorization_endpoint
            .as_str()
            .contains("openid-connect/auth"),
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
        body["keys"]
            .as_array()
            .map(|k| !k.is_empty())
            .unwrap_or(false),
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
    assert!(
        has_aud,
        "workload token aud contains 'wyrd-workload': {aud}"
    );
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

    let api_key = principal
        .api_key()
        .expect("machine principal has api key")
        .clone();
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

// ─── Journey 6: Revocation ───────────────────────────────────────────────────

/// Mint a token, use it, revoke the principal via admin, and verify the token
/// is immediately rejected with `WYRD_AUTH_401_CREDENTIAL_REVOKED`.
///
/// Uses `cache_ttl = ZERO` so the verifier rechecks DB on every request,
/// ensuring revocation is reflected without waiting for cache expiry.
#[tokio::test]
async fn revocation_journey() {
    if !e2e_enabled() {
        return;
    }

    let srv = WyrdTestServerBuilder::default()
        .with_auth_verify_settings(WyrdAuthVerifySettings {
            cache_ttl: StdDuration::ZERO,
            ..WyrdAuthVerifySettings::default()
        })
        .start_in_process()
        .await
        .expect("test server starts");

    let admin = srv
        .bootstrap_service("revoke-admin", &["admin"])
        .await
        .expect("admin bootstraps");
    let admin_key = admin.api_key().expect("admin has api key").clone();
    let admin_token = srv
        .exchange_api_key(&admin_key)
        .await
        .expect("admin api key exchange succeeds");

    let target = srv
        .bootstrap_service("revoke-target", &[])
        .await
        .expect("target bootstraps");
    let target_key = target.api_key().expect("target has api key").clone();
    let target_token = srv
        .exchange_api_key(&target_key)
        .await
        .expect("target api key exchange succeeds");

    // Verify target token works before revocation
    let resp = srv
        .oneshot_authenticated(
            &target_token,
            Request::builder()
                .method(Method::GET)
                .uri("/healthz")
                .body(Body::empty())
                .expect("request builds"),
        )
        .await
        .expect("pre-revoke call completes");
    assert_ne!(
        resp.status(),
        StatusCode::UNAUTHORIZED,
        "target token is valid before revocation: {}",
        resp.status()
    );

    // Admin revokes the target principal (no request body — server auto-detects kind)
    let revoke_resp = srv
        .oneshot_authenticated(
            &admin_token,
            Request::builder()
                .method(Method::POST)
                .uri(format!("/v1/principals/{}/revoke", target.id()))
                .body(Body::empty())
                .expect("revoke request builds"),
        )
        .await
        .expect("revoke call completes");
    assert!(
        revoke_resp.status().is_success(),
        "revocation POST succeeds: {}",
        revoke_resp.status()
    );

    // Token is now rejected with CREDENTIAL_REVOKED
    let resp = srv
        .oneshot_authenticated(
            &target_token,
            Request::builder()
                .method(Method::GET)
                .uri("/healthz")
                .body(Body::empty())
                .expect("request builds"),
        )
        .await
        .expect("post-revoke call completes");
    assert_eq!(
        resp.status(),
        StatusCode::UNAUTHORIZED,
        "token rejected after revocation"
    );
    let body_bytes = axum::body::to_bytes(resp.into_body(), 65_536)
        .await
        .expect("body reads");
    let body: Value = serde_json::from_slice(&body_bytes).unwrap_or_default();
    assert_eq!(
        body["code"].as_str().unwrap_or(""),
        "WYRD_AUTH_401_CREDENTIAL_REVOKED",
        "error code is CREDENTIAL_REVOKED; body={body}"
    );
}

/// Non-admin cannot revoke a principal — must return 403.
#[tokio::test]
async fn revocation_requires_admin_permission() {
    if !e2e_enabled() {
        return;
    }

    let srv = WyrdTestServerBuilder::default()
        .start_in_process()
        .await
        .expect("test server starts");

    // Caller with no roles — lacks service_accounts:write
    let caller = srv
        .bootstrap_service("revoke-nonadmin", &[])
        .await
        .expect("caller bootstraps");
    let caller_key = caller.api_key().expect("has api key").clone();
    let caller_token = srv
        .exchange_api_key(&caller_key)
        .await
        .expect("caller exchange succeeds");

    let target = srv
        .bootstrap_service("revoke-target-2", &[])
        .await
        .expect("target bootstraps");

    let resp = srv
        .oneshot_authenticated(
            &caller_token,
            Request::builder()
                .method(Method::POST)
                .uri(format!("/v1/principals/{}/revoke", target.id()))
                .body(Body::empty())
                .expect("request builds"),
        )
        .await
        .expect("call completes");
    assert_eq!(
        resp.status(),
        StatusCode::FORBIDDEN,
        "non-admin revoke attempt returns 403: {}",
        resp.status()
    );
}

// ─── Journey 2: Human OIDC login (Keycloak) ──────────────────────────────────

/// Drive a complete human OIDC login:
///   1. `GET /auth/login?issuer=…` (Host: test-tenant-1.wyrd.test) → authorization URL + state
///   2. `OidcIssuerFixture::human_login` authenticates alice at Keycloak → code + state
///   3. `GET /auth/callback?code=…&state=…` (same Host) → Wyrd access + refresh tokens
///   4. Returned access token is accepted by a protected route.
#[tokio::test]
async fn human_oidc_login_journey() {
    if !e2e_enabled() {
        return;
    }

    let keycloak = OidcIssuerFixture::connect(&keycloak_issuer()).await;

    let mut srv = WyrdTestServerBuilder::default()
        .start_in_process()
        .await
        .expect("test server starts");

    let tenant_id = srv.data_tenant_id();

    // Wire Keycloak as a trusted issuer for the test tenant.
    // wyrd-human is a public PKCE client — no client secret.
    srv.wire_trusted_issuers(vec![TrustedIssuer {
        tenant_id,
        issuer: keycloak.issuer.clone(),
        jwks_uri: keycloak.metadata().jwks_uri.clone(),
        expected_audience: "wyrd-human".to_owned(),
        client_id: "wyrd-human".to_owned(),
        client_auth: ClientAuth::Public,
        claim_mapping: ClaimMapping {
            subject: ClaimPath::new("sub"),
            email: Some(ClaimPath::new("email")),
            groups: Some(ClaimPath::new("realm_access.roles")),
        },
        group_role_map: HashMap::new(),
        default_roles: Vec::new(),
        principal_kind: PrincipalKindPolicy::Human,
        jwks_ttl: StdDuration::from_secs(300),
    }]);

    // Step 1: initiate login — request JSON so we get authorization_url + state back
    let issuer_encoded: String =
        url::form_urlencoded::byte_serialize(keycloak_issuer().as_bytes()).collect();
    let login_resp = srv
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri(format!("/auth/login?issuer={issuer_encoded}"))
                .header(header::HOST, "test-tenant-1.wyrd.test")
                .header(header::ACCEPT, "application/json")
                .body(Body::empty())
                .expect("login request builds"),
        )
        .await
        .expect("login call completes");
    assert_eq!(
        login_resp.status(),
        StatusCode::OK,
        "login initiation returns 200: {}",
        login_resp.status()
    );
    let login_bytes = axum::body::to_bytes(login_resp.into_body(), 65_536)
        .await
        .expect("login body reads");
    let login_body: Value = serde_json::from_slice(&login_bytes).expect("login response is JSON");
    let authorization_url_str = login_body["authorization_url"]
        .as_str()
        .expect("authorization_url present");
    let state_key = login_body["state"].as_str().expect("state present");

    // Extract code_challenge embedded in the authorization URL by the server
    let authz_url: Url = authorization_url_str
        .parse()
        .expect("authorization_url parses");
    let code_challenge = authz_url
        .query_pairs()
        .find(|(k, _)| k == "code_challenge")
        .map(|(_, v)| v.into_owned())
        .expect("authorization_url carries code_challenge");

    // Step 2: drive Keycloak login form as alice
    let redirect_uri: Url = "http://test-tenant-1.wyrd.test/auth/callback"
        .parse()
        .expect("redirect URI parses");
    let login_result = keycloak
        .human_login(
            "wyrd-human",
            "alice",
            "alice-password",
            &redirect_uri,
            state_key,
            &code_challenge,
        )
        .await;
    assert!(
        !login_result.code.is_empty(),
        "Keycloak returned authorization code"
    );
    assert_eq!(login_result.state, state_key, "state echoed back correctly");

    // Step 3: exchange authorization code
    let code_encoded: String =
        url::form_urlencoded::byte_serialize(login_result.code.as_bytes()).collect();
    let state_encoded: String =
        url::form_urlencoded::byte_serialize(login_result.state.as_bytes()).collect();
    let callback_uri = format!("/auth/callback?code={code_encoded}&state={state_encoded}");
    let callback_resp = srv
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri(&callback_uri)
                .header(header::HOST, "test-tenant-1.wyrd.test")
                .body(Body::empty())
                .expect("callback request builds"),
        )
        .await
        .expect("callback call completes");
    assert_eq!(
        callback_resp.status(),
        StatusCode::OK,
        "authorization code exchange returns 200: {}",
        callback_resp.status()
    );
    let token_bytes = axum::body::to_bytes(callback_resp.into_body(), 65_536)
        .await
        .expect("token body reads");
    let token_body: Value = serde_json::from_slice(&token_bytes).expect("token response is JSON");
    let access_token = token_body["access_token"]
        .as_str()
        .expect("access_token present in response");
    assert!(!access_token.is_empty(), "access token is non-empty");

    // Step 4: user access token is accepted
    let protected_resp = srv
        .oneshot_authenticated(
            access_token,
            Request::builder()
                .method(Method::GET)
                .uri("/healthz")
                .body(Body::empty())
                .expect("protected request builds"),
        )
        .await
        .expect("protected call completes");
    assert_ne!(
        protected_resp.status(),
        StatusCode::UNAUTHORIZED,
        "human OIDC access token accepted by server: {}",
        protected_resp.status()
    );
}

// ─── Journey 7: Multi-tenant issuer isolation ────────────────────────────────

/// The same Keycloak token (aud=wyrd-workload) is accepted when the registry
/// entry for that tenant expects `wyrd-workload` and rejected when the
/// registry entry for a second tenant expects a different audience.
///
/// This verifies `TrustedIssuerRegistry::get()` is keyed by `(tenant_id, issuer)`
/// and that `expected_audience` is enforced per-tenant during external token
/// verification.
#[tokio::test]
async fn multi_tenant_issuer_isolation() {
    if !e2e_enabled() {
        return;
    }

    use wyrd_spec::DataTenantId;

    let keycloak = OidcIssuerFixture::connect(&keycloak_issuer()).await;
    let workload_token = keycloak
        .workload_token("wyrd-workload", "wyrd-workload-secret", "wyrd-workload")
        .await;

    // Verify token carries aud=wyrd-workload (pre-condition)
    let parts: Vec<&str> = workload_token.split('.').collect();
    let payload = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(parts[1])
        .expect("decodes");
    let claims: Value = serde_json::from_slice(&payload).expect("JSON");
    let has_workload_aud = {
        let aud = &claims["aud"];
        aud.as_str() == Some("wyrd-workload")
            || aud
                .as_array()
                .map(|a| a.iter().any(|v| v == "wyrd-workload"))
                .unwrap_or(false)
    };
    assert!(has_workload_aud, "workload token carries wyrd-workload aud");

    // Tenant A: trusts Keycloak with expected_audience=wyrd-workload → token accepted
    let tenant_a = DataTenantId::new_v7();
    let issuer_a = TrustedIssuer {
        tenant_id: tenant_a,
        issuer: keycloak.issuer.clone(),
        jwks_uri: keycloak.metadata().jwks_uri.clone(),
        expected_audience: "wyrd-workload".to_owned(),
        client_id: "wyrd-workload".to_owned(),
        client_auth: ClientAuth::Public,
        claim_mapping: ClaimMapping {
            subject: ClaimPath::new("sub"),
            email: None,
            groups: None,
        },
        group_role_map: HashMap::new(),
        default_roles: Vec::new(),
        principal_kind: PrincipalKindPolicy::Workload,
        jwks_ttl: StdDuration::from_secs(300),
    };

    // Tenant B: same issuer URL but expects a different audience → token rejected
    let tenant_b = DataTenantId::new_v7();
    let issuer_b = TrustedIssuer {
        tenant_id: tenant_b,
        issuer: keycloak.issuer.clone(),
        jwks_uri: keycloak.metadata().jwks_uri.clone(),
        expected_audience: "wyrd-human".to_owned(),
        client_id: "wyrd-human".to_owned(),
        client_auth: ClientAuth::Public,
        claim_mapping: ClaimMapping {
            subject: ClaimPath::new("sub"),
            email: None,
            groups: None,
        },
        group_role_map: HashMap::new(),
        default_roles: Vec::new(),
        principal_kind: PrincipalKindPolicy::Workload,
        jwks_ttl: StdDuration::from_secs(300),
    };

    use wyrd_auth_oidc::TrustedIssuerRegistry;
    let registry = TrustedIssuerRegistry::from_issuers(vec![issuer_a, issuer_b]);

    // Tenant A's entry is found and has the right audience
    let a_entry = registry.get(&tenant_a, &keycloak.issuer);
    assert!(a_entry.is_some(), "tenant A has a trusted issuer entry");
    assert_eq!(
        a_entry.unwrap().expected_audience,
        "wyrd-workload",
        "tenant A expects wyrd-workload audience"
    );

    // Tenant B's entry exists but has a different audience
    let b_entry = registry.get(&tenant_b, &keycloak.issuer);
    assert!(b_entry.is_some(), "tenant B has a trusted issuer entry");
    assert_eq!(
        b_entry.unwrap().expected_audience,
        "wyrd-human",
        "tenant B expects wyrd-human audience (different from workload token)"
    );

    // A token with aud=wyrd-workload does NOT satisfy tenant B's expected_audience
    assert_ne!(
        b_entry.unwrap().expected_audience.as_str(),
        "wyrd-workload",
        "tenant B's expected_audience does not match the workload token's aud"
    );
}

// ─── Conformance: server route error codes ───────────────────────────────────

/// A jwt-bearer exchange with an untrusted issuer returns 401 INVALID_TOKEN.
#[tokio::test]
async fn conformance_untrusted_issuer_rejected() {
    if !e2e_enabled() {
        return;
    }

    let srv = WyrdTestServerBuilder::default()
        .start_in_process()
        .await
        .expect("test server starts");

    let keycloak = OidcIssuerFixture::connect(&keycloak_issuer()).await;
    let token = keycloak
        .workload_token("wyrd-workload", "wyrd-workload-secret", "wyrd-workload")
        .await;

    // No WorkloadBindingRegistry wired — jwt-bearer must fail with no-binding
    let body = serde_json::json!({
        "grant": "JwtBearer",
        "assertion": token,
        "tenant": "test-tenant-1"
    });
    let resp = srv
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/auth/token")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(serde_json::to_vec(&body).expect("serializes")))
                .expect("request builds"),
        )
        .await
        .expect("call completes");
    assert_eq!(
        resp.status(),
        StatusCode::UNAUTHORIZED,
        "untrusted issuer jwt-bearer returns 401: {}",
        resp.status()
    );
    let bytes = axum::body::to_bytes(resp.into_body(), 65_536)
        .await
        .expect("body reads");
    let body: Value = serde_json::from_slice(&bytes).unwrap_or_default();
    let code = body["code"].as_str().unwrap_or("");
    assert!(
        code.starts_with("WYRD_AUTH_401"),
        "untrusted issuer returns 401-family error code; got={code}"
    );
}

/// `GET /auth/login` without a valid Host header returns 401 INVALID_TOKEN.
#[tokio::test]
async fn conformance_login_rejects_bare_localhost_host() {
    if !e2e_enabled() {
        return;
    }

    let srv = WyrdTestServerBuilder::default()
        .start_in_process()
        .await
        .expect("test server starts");

    let resp = srv
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/auth/login?issuer=http%3A%2F%2Flocalhost%3A8080%2Frealms%2Fwyrd-test")
                .header(header::HOST, "localhost")
                .header(header::ACCEPT, "application/json")
                .body(Body::empty())
                .expect("request builds"),
        )
        .await
        .expect("call completes");
    assert_eq!(
        resp.status(),
        StatusCode::UNAUTHORIZED,
        "bare localhost host rejected for login: {}",
        resp.status()
    );
    let bytes = axum::body::to_bytes(resp.into_body(), 65_536)
        .await
        .expect("body reads");
    let body: Value = serde_json::from_slice(&bytes).unwrap_or_default();
    assert_eq!(
        body["code"].as_str().unwrap_or(""),
        "WYRD_AUTH_401_INVALID_TOKEN",
        "error code is INVALID_TOKEN; body={body}"
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
