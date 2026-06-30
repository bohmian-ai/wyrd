use std::collections::HashMap;
use std::env;
use std::time::Duration as StdDuration;

use axum::body::{Body, to_bytes};
use axum::http::{Method, Request, StatusCode, header};
use base64::Engine as _;
use chrono::Duration as ChronoDuration;
use secrecy::SecretString;
use serde_json::Value;
use url::Url;
use wyrd_auth_check::AuthzCheckRequest;
use wyrd_auth_oidc::IssuerConfigResolver;
use wyrd_auth_verify::WyrdAuthVerifySettings;
use wyrd_semver::VersionBlock;
use wyrd_server::config::{
    ClaimMappingEntry, ClientAuthEntry, IssuerEntry, PrincipalKindEntry, WorkloadBindingEntry,
};
use wyrd_spec::auth::{IssueKeyRequest, IssueKeyResponse, IssuerUrl};
use wyrd_spec::envelope::CardKind;
use wyrd_spec::ids::{CardName, SpaceName};
use wyrd_spec::reference::CardRef;
use wyrd_testing::{
    Bootstrap, KeycloakAdmin, OidcIssuerFixture, WyrdTestServer, WyrdTestServerBuilder,
};

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

/// Tenant slug the fixture provisions; jwt-bearer resolves it to the implicit
/// `DataTenantId` the config seam binds issuers and bindings to.
const FIXTURE_TENANT_SLUG: &str = "test-tenant-1";

// ─── Config-driven boot helpers ──────────────────────────────────────────────

/// Build a `[[trusted_issuers]]` config DTO. The config carries no tenant; the
/// harness seam binds it to `fixture.data_tenant_id()` at boot.
fn issuer_entry(issuer: &str, audience: &str, principal_kind: PrincipalKindEntry) -> IssuerEntry {
    IssuerEntry {
        issuer: issuer.to_owned(),
        client_id: audience.to_owned(),
        expected_audience: audience.to_owned(),
        client_auth: ClientAuthEntry::Public,
        claim_mapping: ClaimMappingEntry::default(),
        group_role_map: HashMap::new(),
        default_roles: Vec::new(),
        principal_kind,
        jwks_ttl_secs: Some(300),
    }
}

/// The Keycloak workload issuer, principal-kind Workload (jwt-bearer issuance).
fn keycloak_workload_issuer() -> IssuerEntry {
    issuer_entry(
        &keycloak_issuer(),
        "wyrd-workload",
        PrincipalKindEntry::Workload,
    )
}

/// Build the server-owned `CardRef` a `[[workload_bindings]]` entry resolves to:
/// the same `uid = None` ref boot's `build_workload_bindings` produces.
fn binding_card_ref(kind: CardKind, name: &str, space: &str) -> CardRef {
    CardRef {
        kind,
        name: CardName::new(name).expect("card name is valid"),
        version: VersionBlock::parse("1.0.0").expect("version is valid"),
        space: SpaceName::new(space).expect("space name is valid"),
        uid: None,
    }
}

/// Decode an unverified JWT payload into a JSON value.
fn jwt_claims(jwt: &str) -> Value {
    let payload = jwt.split('.').nth(1).expect("jwt has a payload segment");
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(payload)
        .expect("payload base64 decodes");
    serde_json::from_slice(&bytes).expect("payload is JSON")
}

/// POST a jwt-bearer assertion at `/auth/token`, returning the raw HTTP response.
async fn post_jwt_bearer(srv: &WyrdTestServer, assertion: &str) -> axum::http::Response<Body> {
    let body = serde_json::json!({
        "grant_type": "urn:ietf:params:oauth:grant-type:jwt-bearer",
        "assertion": assertion,
        "tenant": FIXTURE_TENANT_SLUG,
    });
    srv.oneshot(
        Request::builder()
            .method(Method::POST)
            .uri("/auth/token")
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(serde_json::to_vec(&body).expect("serializes")))
            .expect("request builds"),
    )
    .await
    .expect("jwt-bearer call completes")
}

fn authz_check_request(target: &Bootstrap, action: &str) -> AuthzCheckRequest {
    AuthzCheckRequest {
        target: target
            .card_ref()
            .expect("machine target carries a card_ref")
            .clone(),
        action: action.to_owned(),
        context: serde_json::json!({}),
    }
}

/// Drive a delegated token to a real authenticated `/v1/authz/check` `200`.
///
/// `delegator_jwt` must belong to a Service/Agent (or Human) principal that
/// holds `delegation_issue` (e.g. `runtime_admin`). It delegates to a freshly
/// seeded `writer` callee, then runs the check on that callee — the proven
/// guard-passing terminal (delegated chain, eligible callee, `card_write`).
async fn assert_v1_authz_check_ok(srv: &WyrdTestServer, delegator_jwt: &str, label: &str) {
    let callee = srv
        .bootstrap_service(&format!("{label}-callee"), &["writer"])
        .await
        .expect("callee bootstraps");
    let delegated = srv
        .delegate(
            delegator_jwt,
            callee.card_ref().expect("callee carries a card_ref"),
        )
        .await
        .expect("delegation succeeds");
    let result = srv
        .authz_check(&delegated, authz_check_request(&callee, "card_write"))
        .await
        .expect("authz-check completes");
    assert_eq!(
        result.status,
        StatusCode::OK,
        "{label}: /v1/authz/check returns 200, got {}",
        result.status
    );
}

fn response_code(body: &Value) -> &str {
    body["code"].as_str().unwrap_or("")
}

// ─── Discovery smoke tests ────────────────────────────────────────────────────

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

// ─── Trust/validation matrix: parameterized across Keycloak and Dex ───────────

/// Config-only trust/validation surface, run against either provider:
///   1. discovery resolves the provider,
///   2. JWKS is reachable,
///   3. the issuer boots through the production `ConfigFileIssuerResolver` +
///      OIDC discovery into the verifier's trust registry (fails closed if
///      discovery is unreachable), with `jwks_uri` filled from discovery.
///
/// This proves the OIDC trust/discovery/JWKS layer is spec-compliant and not
/// Keycloak-coupled. Dex participates here (validation only); it cannot mint
/// `client_credentials` tokens, so it never drives the workload issuance journey.
async fn assert_config_driven_trust_layer(issuer: &str, audience: &str) {
    let fixture = OidcIssuerFixture::connect(issuer).await;
    let jwks = reqwest::get(fixture.metadata().jwks_uri.clone())
        .await
        .expect("JWKS fetch succeeds");
    assert!(jwks.status().is_success(), "{issuer}: JWKS reachable");
    let body: Value = jwks.json().await.expect("JWKS is JSON");
    assert!(
        body["keys"].as_array().is_some(),
        "{issuer}: JWKS exposes a 'keys' array"
    );

    let srv = WyrdTestServerBuilder::default()
        .with_trusted_issuer_configs(vec![issuer_entry(
            issuer,
            audience,
            PrincipalKindEntry::Workload,
        )])
        .start_in_process()
        .await
        .expect("server boots through config-driven issuer discovery");

    let tenant_id = srv.data_tenant_id();
    let resolver = srv
        .state()
        .trusted_issuer_resolver
        .as_ref()
        .expect("config seam populated the trusted-issuer resolver");
    let key = IssuerUrl::new(issuer.to_owned()).expect("issuer URL is a valid issuer");
    let issuers = resolver
        .trusted_issuers(&tenant_id)
        .await
        .expect("issuer resolution from Postgres succeeds");
    let entry = issuers
        .iter()
        .find(|candidate| candidate.issuer == key)
        .expect("issuer resolves under the implicit tenant (F02 keying)");
    assert!(
        entry.jwks_uri.as_str().starts_with("http"),
        "{issuer}: jwks_uri filled from discovery, got {}",
        entry.jwks_uri
    );
}

#[tokio::test]
async fn trust_layer_config_driven_keycloak() {
    if !e2e_enabled() {
        return;
    }
    assert_config_driven_trust_layer(&keycloak_issuer(), "wyrd-workload").await;
}

#[tokio::test]
async fn trust_layer_config_driven_dex() {
    if !e2e_enabled() {
        return;
    }
    assert_config_driven_trust_layer(&dex_issuer(), "wyrd-server").await;
}

// ─── Workload token claims (Keycloak) ─────────────────────────────────────────

/// The Keycloak `client_credentials` token carries the expected issuer and
/// audience claims the jwt-bearer route verifies — asserted standalone so a
/// failing journey has a clear root cause.
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
    assert_eq!(
        raw_token.split('.').count(),
        3,
        "workload token has 3 JWT segments"
    );

    let claims = jwt_claims(&raw_token);
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
    let claims = jwt_claims(&raw_token);
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

// ─── Workload jwt-bearer journey (Keycloak only) ──────────────────────────────

/// Full config-driven workload journey:
///   1. Keycloak mints a `client_credentials` token (foreign OIDC JWT).
///   2. Config-driven boot trusts the issuer and binds `(issuer, subject)` →
///      a server-owned Service `card_ref`; a principal is seeded under it.
///   3. `POST /auth/token {jwt-bearer}` exchanges the assertion for a Wyrd
///      Service token.
///   4. The Service token delegates and reaches a real `/v1/authz/check` `200`.
#[tokio::test]
async fn workload_jwt_bearer_journey_keycloak() {
    if !e2e_enabled() {
        return;
    }

    let keycloak = OidcIssuerFixture::connect(&keycloak_issuer()).await;
    let assertion = keycloak
        .workload_token("wyrd-workload", "wyrd-workload-secret", "wyrd-workload")
        .await;
    let subject = jwt_claims(&assertion)["sub"]
        .as_str()
        .expect("workload token carries a subject")
        .to_owned();

    let card_ref = binding_card_ref(CardKind::Service, "wyrd-workload-sa", "prod");

    let srv = WyrdTestServerBuilder::default()
        .with_trusted_issuer_configs(vec![keycloak_workload_issuer()])
        .with_workload_binding_configs(vec![WorkloadBindingEntry {
            issuer: keycloak_issuer(),
            subject: subject.clone(),
            audience: Some("wyrd-workload".to_owned()),
            kind: "service".to_owned(),
            name: "wyrd-workload-sa".to_owned(),
            space: "prod".to_owned(),
            version: "1.0.0".to_owned(),
        }])
        .start_in_process()
        .await
        .expect("server boots config-driven");

    // Seed the bound principal under the exact server-owned card_ref. It holds
    // runtime_admin so the minted workload token can delegate to the terminal.
    srv.seed_card_principal(&card_ref, &["runtime_admin"])
        .await
        .expect("workload principal seeds");

    let resp = post_jwt_bearer(&srv, &assertion).await;
    assert_eq!(
        resp.status(),
        StatusCode::OK,
        "bound workload jwt-bearer exchange returns 200: {}",
        resp.status()
    );
    let bytes = to_bytes(resp.into_body(), 65_536)
        .await
        .expect("token body reads");
    let token_body: Value = serde_json::from_slice(&bytes).expect("token response is JSON");
    let wyrd_token = token_body["access_token"]
        .as_str()
        .expect("access_token present");

    assert_v1_authz_check_ok(&srv, wyrd_token, "workload").await;
}

/// An unbound subject (issuer trusted, no matching binding) returns
/// `WYRD_AUTH_404_PRINCIPAL_NOT_FOUND` from the jwt-bearer route.
#[tokio::test]
async fn workload_jwt_bearer_unbound_subject_returns_404_keycloak() {
    if !e2e_enabled() {
        return;
    }

    let keycloak = OidcIssuerFixture::connect(&keycloak_issuer()).await;
    let assertion = keycloak
        .workload_token("wyrd-workload", "wyrd-workload-secret", "wyrd-workload")
        .await;

    // Registry is populated but bound to a different subject, so the live
    // subject misses the lookup → principal-not-found.
    let srv = WyrdTestServerBuilder::default()
        .with_trusted_issuer_configs(vec![keycloak_workload_issuer()])
        .with_workload_binding_configs(vec![WorkloadBindingEntry {
            issuer: keycloak_issuer(),
            subject: "system:serviceaccount:unbound:other".to_owned(),
            audience: Some("wyrd-workload".to_owned()),
            kind: "service".to_owned(),
            name: "unbound-sa".to_owned(),
            space: "prod".to_owned(),
            version: "1.0.0".to_owned(),
        }])
        .start_in_process()
        .await
        .expect("server boots config-driven");

    let resp = post_jwt_bearer(&srv, &assertion).await;
    assert_eq!(
        resp.status(),
        StatusCode::NOT_FOUND,
        "unbound workload subject returns 404: {}",
        resp.status()
    );
    let bytes = to_bytes(resp.into_body(), 65_536)
        .await
        .expect("body reads");
    let body: Value = serde_json::from_slice(&bytes).unwrap_or_default();
    assert_eq!(
        response_code(&body),
        "WYRD_AUTH_404_PRINCIPAL_NOT_FOUND",
        "unbound subject error code; body={body}"
    );
}

// ─── TTL expiry journey ───────────────────────────────────────────────────────

/// Mint a short-lived access token, reach a real `/v1/authz/check` `200`, sleep
/// past `exp`, then assert the same token is rejected `401` on `/v1`.
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
        .bootstrap_service("ttl-svc", &["runtime_admin"])
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

    // Valid before expiry → real authenticated /v1 200.
    assert_v1_authz_check_ok(&srv, &access_token, "ttl").await;

    // Sleep past access_ttl + cache_ttl (2s + 1s; use 4s).
    tokio::time::sleep(StdDuration::from_secs(4)).await;

    // Expired token is rejected on /v1 (verify fails before the delegation guard).
    let callee = srv
        .bootstrap_service("ttl-after-callee", &["writer"])
        .await
        .expect("callee bootstraps");
    let result = srv
        .authz_check(&access_token, authz_check_request(&callee, "card_write"))
        .await
        .expect("post-expiry call completes");
    assert_eq!(
        result.status,
        StatusCode::UNAUTHORIZED,
        "expired token rejected on /v1: {}",
        result.status
    );
}

// ─── Revocation journey ───────────────────────────────────────────────────────

/// Mint a token, reach a real `/v1/authz/check` `200`, revoke the principal via
/// admin, and verify the token is immediately rejected `401` with
/// `WYRD_AUTH_401_CREDENTIAL_REVOKED` on `/v1`.
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
        .bootstrap_service("revoke-target", &["runtime_admin"])
        .await
        .expect("target bootstraps");
    let target_key = target.api_key().expect("target has api key").clone();
    let target_token = srv
        .exchange_api_key(&target_key)
        .await
        .expect("target api key exchange succeeds");

    // Valid before revocation → real authenticated /v1 200.
    assert_v1_authz_check_ok(&srv, &target_token, "revoke").await;

    // Admin revokes the target principal (server auto-detects kind).
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

    // Revoked token rejected on /v1 with CREDENTIAL_REVOKED (verify runs first).
    let callee = srv
        .bootstrap_service("revoke-after-callee", &["writer"])
        .await
        .expect("callee bootstraps");
    let resp = srv
        .oneshot_authenticated(
            &target_token,
            Request::builder()
                .method(Method::POST)
                .uri("/v1/authz/check")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    serde_json::to_vec(&authz_check_request(&callee, "card_write"))
                        .expect("serializes"),
                ))
                .expect("request builds"),
        )
        .await
        .expect("post-revoke call completes");
    assert_eq!(
        resp.status(),
        StatusCode::UNAUTHORIZED,
        "token rejected after revocation: {}",
        resp.status()
    );
    let body_bytes = to_bytes(resp.into_body(), 65_536)
        .await
        .expect("body reads");
    let body: Value = serde_json::from_slice(&body_bytes).unwrap_or_default();
    assert_eq!(
        response_code(&body),
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

// ─── Service-account issuer full chain ────────────────────────────────────────

/// Full-chain API-key journey with a **service-account** issuer:
///   1. Admin SA (`runtime_admin`) bootstrapped via harness SQL.
///   2. Target SA bootstrapped via harness SQL (the key is issued for this card).
///   3. Admin key → `POST /auth/token` → access token.
///   4. `POST /auth/issue-key` authenticated as the admin SA; `created_by` is the
///      SA's `service_account.id` — the direct assertion that the relaxed FK
///      (commit 01) accepts a non-user issuer (pre-migration this would 500).
///   5. Issued key → `POST /auth/token` → access token.
///   6. `assert_v1_authz_check_ok` → `/v1/authz/check 200`.
#[tokio::test]
async fn service_account_issuer_full_chain() {
    if !e2e_enabled() {
        return;
    }

    let srv = WyrdTestServerBuilder::default()
        .start_in_process()
        .await
        .expect("test server starts");

    // Admin service account: holds runtime_admin so it has key-issuance permission.
    let admin = srv
        .bootstrap_service("sa-chain-admin", &["runtime_admin"])
        .await
        .expect("admin service account bootstraps");
    let admin_key = admin.api_key().expect("admin has api key").clone();

    // Exchange admin API key → access token (principal is the SA, not a user).
    let admin_token = srv
        .exchange_api_key(&admin_key)
        .await
        .expect("admin api key exchange succeeds");

    // Target service card: the card a key will be issued for.
    // Holds runtime_admin so the issued token can later delegate to the terminal.
    let target = srv
        .bootstrap_service("sa-chain-target", &["runtime_admin"])
        .await
        .expect("target service account bootstraps");
    let target_card_ref = target
        .card_ref()
        .expect("target carries a card_ref")
        .clone();

    // POST /auth/issue-key as the admin SA.
    // The issuer principal is a service account (not a user), so `created_by`
    // must resolve to `service_account.id` — the FK-relaxation path from commit 01.
    let issue_req = IssueKeyRequest {
        card_ref: target_card_ref,
        label: Some("sa-chain".to_owned()),
        expires_in_seconds: None,
    };
    let issue_resp = srv
        .oneshot_authenticated(
            &admin_token,
            Request::builder()
                .method(Method::POST)
                .uri("/auth/issue-key")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    serde_json::to_vec(&issue_req).expect("issue-key request serializes"),
                ))
                .expect("issue-key request builds"),
        )
        .await
        .expect("issue-key call completes");
    assert_eq!(
        issue_resp.status(),
        StatusCode::OK,
        "service-account issuer: issue-key returns 200, got {}",
        issue_resp.status()
    );
    let issue_bytes = to_bytes(issue_resp.into_body(), 65_536)
        .await
        .expect("issue-key body reads");
    let issue_body: IssueKeyResponse =
        serde_json::from_slice(&issue_bytes).expect("issue-key response is valid JSON");
    assert!(!issue_body.key_id.is_empty(), "issued key has a key_id");

    // Exchange the issued key → access token.
    let issued_secret = SecretString::from(issue_body.key.expose().to_owned());
    let issued_token = srv
        .exchange_api_key(&issued_secret)
        .await
        .expect("issued key exchange succeeds");

    // Terminal: /v1/authz/check 200 — proves the issued token is valid and the
    // full chain succeeds with a service-account issuer.
    assert_v1_authz_check_ok(&srv, &issued_token, "sa-chain").await;
}

// ─── Human OIDC login journey (Keycloak) ──────────────────────────────────────

/// Drive a complete config-driven human OIDC login:
///   1. boot trusts Keycloak via `[[trusted_issuers]]` (config-driven), granting
///      `runtime_admin` as a default role so the federated human can delegate,
///   2. `GET /auth/login` → authorization URL + state,
///   3. `OidcIssuerFixture::human_login` authenticates alice → code + state,
///   4. `GET /auth/callback` → Wyrd access token,
///   5. the human token reaches a real `/v1/authz/check` `200` via delegation.
#[tokio::test]
async fn human_oidc_login_journey() {
    if !e2e_enabled() {
        return;
    }

    let keycloak = OidcIssuerFixture::connect(&keycloak_issuer()).await;

    // wyrd-human is a public PKCE client. Config-driven boot binds it to the
    // fixture's implicit tenant and runs OIDC discovery for jwks_uri.
    let mut human_issuer = IssuerEntry {
        client_auth: ClientAuthEntry::Public,
        claim_mapping: ClaimMappingEntry {
            subject: "sub".to_owned(),
            email: Some("email".to_owned()),
            groups: Some("realm_access.roles".to_owned()),
        },
        default_roles: vec!["runtime_admin".to_owned()],
        ..issuer_entry(&keycloak_issuer(), "wyrd-human", PrincipalKindEntry::Human)
    };
    human_issuer.client_id = "wyrd-human".to_owned();

    let srv = WyrdTestServerBuilder::default()
        .with_trusted_issuer_configs(vec![human_issuer])
        .start_in_process()
        .await
        .expect("test server starts");

    // Step 1: initiate login — request JSON to get authorization_url + state.
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
    let login_bytes = to_bytes(login_resp.into_body(), 65_536)
        .await
        .expect("login body reads");
    let login_body: Value = serde_json::from_slice(&login_bytes).expect("login response is JSON");
    let authorization_url_str = login_body["authorization_url"]
        .as_str()
        .expect("authorization_url present");
    let state_key = login_body["state"].as_str().expect("state present");

    let authz_url: Url = authorization_url_str
        .parse()
        .expect("authorization_url parses");
    let code_challenge = authz_url
        .query_pairs()
        .find(|(k, _)| k == "code_challenge")
        .map(|(_, v)| v.into_owned())
        .expect("authorization_url carries code_challenge");
    let nonce = authz_url
        .query_pairs()
        .find(|(k, _)| k == "nonce")
        .map(|(_, v)| v.into_owned())
        .expect("authorization_url carries nonce");

    // Step 2: drive Keycloak login form as alice.
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
            &nonce,
        )
        .await;
    assert!(
        !login_result.code.is_empty(),
        "Keycloak returned authorization code"
    );
    assert_eq!(login_result.state, state_key, "state echoed back correctly");

    // Step 3: exchange authorization code.
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
    let token_bytes = to_bytes(callback_resp.into_body(), 65_536)
        .await
        .expect("token body reads");
    let token_body: Value = serde_json::from_slice(&token_bytes).expect("token response is JSON");
    let access_token = token_body["access_token"]
        .as_str()
        .expect("access_token present in response");
    assert!(!access_token.is_empty(), "access token is non-empty");

    // Step 4: human token reaches a real authenticated /v1 200 via delegation.
    assert_v1_authz_check_ok(&srv, access_token, "human-sso").await;
}

// ─── Conformance: server route error codes ────────────────────────────────────

/// A jwt-bearer exchange against a server with no trusted external issuer fails
/// closed with a 401-family error code.
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

    let resp = post_jwt_bearer(&srv, &token).await;
    assert_eq!(
        resp.status(),
        StatusCode::UNAUTHORIZED,
        "untrusted issuer jwt-bearer returns 401: {}",
        resp.status()
    );
    let bytes = to_bytes(resp.into_body(), 65_536)
        .await
        .expect("body reads");
    let body: Value = serde_json::from_slice(&bytes).unwrap_or_default();
    assert!(
        response_code(&body).starts_with("WYRD_AUTH_401"),
        "untrusted issuer returns 401-family error code; got={}",
        response_code(&body)
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
    let bytes = to_bytes(resp.into_body(), 65_536)
        .await
        .expect("body reads");
    let body: Value = serde_json::from_slice(&bytes).unwrap_or_default();
    assert_eq!(
        response_code(&body),
        "WYRD_AUTH_401_INVALID_TOKEN",
        "error code is INVALID_TOKEN; body={body}"
    );
}

// ─── Key rotation (Keycloak admin API) ────────────────────────────────────────

/// Force a Keycloak signing-key rotation via the admin REST API and verify that
/// both pre- and post-rotation tokens are well-formed JWTs with a `kid` header.
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

// ─── Helpers ──────────────────────────────────────────────────────────────────

fn extract_kid(jwt: &str) -> Option<String> {
    let header_b64 = jwt.split('.').next()?;
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(header_b64)
        .ok()?;
    let header: Value = serde_json::from_slice(&bytes).ok()?;
    header["kid"].as_str().map(|s| s.to_owned())
}
