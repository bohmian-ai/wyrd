use std::collections::HashMap;
use std::env;
use std::time::Duration as StdDuration;

use axum::body::{Body, to_bytes};
use axum::http::{Method, Request, StatusCode, header};
use base64::Engine as _;
use chrono::Duration as ChronoDuration;
use secrecy::{ExposeSecret as _, SecretString};
use serde_json::Value;
use url::Url;
use wyrd_auth_check::AuthzCheckRequest;
use wyrd_auth_check::response::AuthzCheckDecision;
use wyrd_auth_oidc::IssuerConfigResolver;
use wyrd_auth_verify::WyrdAuthVerifySettings;
use wyrd_cli::auth::trusted_issuer::{self, AddArgs as TrustedIssuerAddArgs, TrustedIssuerCommand};
use wyrd_cli::auth::workload_binding::{
    self, AddArgs as WorkloadBindingAddArgs, WorkloadBindingCommand,
};
use wyrd_cli::error::WyrdCliError;
use wyrd_client::auth::AuthMiddleware;
use wyrd_client::config::{ClientConfig, TokenCacheMode};
use wyrd_client::transport::config::HttpConfig;
use wyrd_client::transport::credential::ResolvedCredential;
use wyrd_crypt::{SealingKeyring, SecretKey};
use wyrd_semver::VersionBlock;
use wyrd_server::config::{ClaimMappingEntry, ClientAuthEntry, IssuerEntry, WorkloadBindingEntry};
use wyrd_spec::DataTenantId;
use wyrd_spec::auth::{
    IssueKeyRequest, IssueKeyResponse, IssuerTokenPolicy, IssuerUrl, TokenAudience,
};
use wyrd_spec::envelope::CardKind;
use wyrd_spec::error::WyrdError;
use wyrd_spec::ids::{CardName, SpaceName};
use wyrd_spec::reference::CardRef;
use wyrd_testing::{
    Bootstrap, KeycloakAdmin, OidcIssuerFixture, WyrdTestServer, WyrdTestServerBuilder,
};

fn keycloak_issuer() -> String {
    env::var("WYRD_KEYCLOAK_ISSUER")
        .unwrap_or_else(|_| "http://localhost:8080/realms/wyrd-test".to_owned())
}

fn dex_issuer() -> String {
    env::var("WYRD_DEX_ISSUER").unwrap_or_else(|_| "http://localhost:5556".to_owned())
}

/// Admin credentials for the fixture realm's privileged REST operations.
///
/// Signing-key rotation and group-membership changes are both driven through
/// the Keycloak admin API with the compose file's fixed development admin, so
/// the credential is written once here rather than at each call site.
fn keycloak_admin() -> KeycloakAdmin {
    KeycloakAdmin {
        base_url: "http://localhost:8080".to_owned(),
        username: "admin".to_owned(),
        password: "admin".to_owned(),
        realm: "wyrd-test".to_owned(),
    }
}

/// Tenant slug the fixture provisions; jwt-bearer resolves it to the implicit
/// `DataTenantId` the config seam binds issuers and bindings to.
const FIXTURE_TENANT_SLUG: &str = "test-tenant-1";

// ─── Config-driven boot helpers ──────────────────────────────────────────────

/// Build a `[[trusted_issuers]]` config DTO. The config carries no tenant; the
/// harness seam binds it to `fixture.data_tenant_id()` at boot.
fn issuer_entry(issuer: &str, audience: &str, principal_kind: IssuerTokenPolicy) -> IssuerEntry {
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
        IssuerTokenPolicy::Workload,
    )
}

/// Build the server-owned `CardRef` a `[[workload_bindings]]` entry resolves to:
/// the same `uid = None` ref boot's `build_workload_bindings` produces.
fn binding_card_ref(kind: CardKind, name: &str, space: &str) -> CardRef {
    CardRef {
        kind,
        name: CardName::new(name).expect("card name is valid"),
        version: VersionBlock::parse("1.0.0").expect("version is valid"),
        space: Some(SpaceName::new(space).expect("space name is valid")),
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

/// POST a jwt-bearer assertion at `/auth/token` for the fixture tenant.
async fn post_jwt_bearer(srv: &WyrdTestServer, assertion: &str) -> axum::http::Response<Body> {
    post_jwt_bearer_for_tenant(srv, assertion, FIXTURE_TENANT_SLUG).await
}

/// POST a jwt-bearer assertion at `/auth/token` routed to an explicit tenant.
///
/// The host carries no tenant subdomain, so the route resolves the tenant from
/// the body `tenant` slug. The isolation test drives the same assertion at two
/// distinct tenant slugs to prove binding resolution is tenant-scoped.
async fn post_jwt_bearer_for_tenant(
    srv: &WyrdTestServer,
    assertion: &str,
    tenant: &str,
) -> axum::http::Response<Body> {
    let body = serde_json::json!({
        "grant_type": "urn:ietf:params:oauth:grant-type:jwt-bearer",
        "assertion": assertion,
        "tenant": tenant,
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
/// `subject_jwt` is the token of a principal holding `writer`. A freshly
/// seeded `writer` service exchanges it as the RFC 8693 actor, then runs the
/// check on its own Card — the proven guard-passing terminal (delegated chain,
/// eligible actor, `card_write` in the subject/actor intersection).
///
/// # Panics
/// Panics when bootstrap, exchange, or the check fails, or the check is not `200`.
async fn assert_v1_authz_check_ok(srv: &WyrdTestServer, subject_jwt: &str, label: &str) {
    let actor = srv
        .bootstrap_service(&format!("{label}-actor"), &["writer"])
        .await
        .expect("actor bootstraps");
    let actor_jwt = srv
        .exchange_api_key(actor.api_key().expect("actor has key"))
        .await
        .expect("actor key exchanges");
    let delegated = srv
        .delegate(subject_jwt, &actor_jwt, TokenAudience::Wyrd)
        .await
        .expect("delegation succeeds");
    let result = srv
        .authz_check(&delegated, authz_check_request(&actor, "card_write"))
        .await
        .expect("authz-check completes");
    assert_eq!(
        result.status,
        StatusCode::OK,
        "{label}: /v1/authz/check returns 200, got {}",
        result.status
    );
    assert_eq!(
        result.response.map(|response| response.decision),
        Some(AuthzCheckDecision::Allow),
        "{label}: the subject/actor intersection allows card_write"
    );
}

fn response_code(body: &Value) -> &str {
    body["code"].as_str().unwrap_or("")
}

// ─── Discovery smoke tests ────────────────────────────────────────────────────

#[tokio::test]
#[ignore = "requires the Keycloak and Dex identity lane"]
async fn discovery_resolves_keycloak() {
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
#[ignore = "requires the Keycloak and Dex identity lane"]
async fn discovery_resolves_dex() {
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
            IssuerTokenPolicy::Workload,
        )])
        .start_in_process()
        .await
        .expect("server boots through config-driven issuer discovery");

    let tenant_id = srv.data_tenant_id();
    let resolver = srv
        .state()
        .auth
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
#[ignore = "requires the Keycloak and Dex identity lane"]
async fn trust_layer_config_driven_keycloak() {
    assert_config_driven_trust_layer(&keycloak_issuer(), "wyrd-workload").await;
}

#[tokio::test]
#[ignore = "requires the Keycloak and Dex identity lane"]
async fn trust_layer_config_driven_dex() {
    assert_config_driven_trust_layer(&dex_issuer(), "wyrd-server").await;
}

// ─── Workload token claims (Keycloak) ─────────────────────────────────────────

/// The Keycloak `client_credentials` token carries the expected issuer and
/// audience claims the jwt-bearer route verifies — asserted standalone so a
/// failing journey has a clear root cause.
#[tokio::test]
#[ignore = "requires the Keycloak and Dex identity lane"]
async fn workload_token_keycloak_claims() {
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
#[ignore = "requires the Keycloak and Dex identity lane"]
async fn workload_token_wrong_aud_absent_keycloak() {
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
///
/// # Panics
/// Panics when the Keycloak token has no `sub`, the server fails to boot or
/// seed the bound principal, the jwt-bearer exchange is not `200`, the grant
/// omits `access_token` or issues a refresh token, or the delegated authz
/// check does not return `200`.
#[tokio::test]
#[ignore = "requires the Keycloak and Dex identity lane"]
async fn workload_jwt_bearer_journey_keycloak() {
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
    // writer so the minted workload token can be the terminal's subject.
    srv.seed_card_principal(&card_ref, &["writer"])
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
    // A workload renews by re-presenting its platform assertion, so the grant
    // hands back nothing to rotate.
    assert!(
        token_body.get("refresh_token").is_none_or(Value::is_null),
        "a workload grant issues no refresh token: {token_body}"
    );

    assert_v1_authz_check_ok(&srv, wyrd_token, "workload").await;
}

/// An unbound subject (issuer trusted, no matching binding) returns
/// `WYRD_AUTH_404_PRINCIPAL_NOT_FOUND` from the jwt-bearer route.
#[tokio::test]
#[ignore = "requires the Keycloak and Dex identity lane"]
async fn workload_jwt_bearer_unbound_subject_returns_404_keycloak() {
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
///
/// # Panics
/// Panics when the server, service bootstrap, or key exchange fails, when the
/// fresh token does not reach `200`, or when the expired token is not rejected
/// with `401`.
#[tokio::test]
#[ignore = "requires the Keycloak and Dex identity lane"]
async fn ttl_expiry_journey() {
    let srv = WyrdTestServerBuilder::default()
        .with_access_ttl(ChronoDuration::seconds(2))
        .with_auth_verify_settings(WyrdAuthVerifySettings {
            allowed_clock_skew: StdDuration::ZERO,
        })
        .start_in_process()
        .await
        .expect("test server starts");

    let principal = srv
        .bootstrap_service("ttl-svc", &["writer"])
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

    // Sleep past access_ttl (2s) with margin for second-granular `exp`.
    tokio::time::sleep(StdDuration::from_secs(3)).await;

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
/// admin, and show revocation governs issuance: the principal's durable key can
/// no longer exchange, while the token it already holds keeps its immutable
/// authority until its short expiry.
///
/// # Panics
/// Panics when bootstrap or key exchange fails, the revoke `POST` does not
/// succeed, the revoked key can still exchange, or the pre-revocation token
/// stops reaching `200` before its expiry.
#[tokio::test]
#[ignore = "requires the Keycloak and Dex identity lane"]
async fn revocation_journey() {
    let srv = WyrdTestServerBuilder::default()
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
        .bootstrap_service("revoke-target", &["writer"])
        .await
        .expect("target bootstraps");
    let target_key = target.api_key().expect("target has api key").clone();
    let target_token = srv
        .exchange_api_key(&target_key)
        .await
        .expect("target api key exchange succeeds");

    // Valid before revocation → real authenticated /v1 200.
    assert_v1_authz_check_ok(&srv, &target_token, "revoke").await;

    // Admin revokes the target principal, naming its kind and the reason.
    let revoke_resp = srv
        .oneshot_authenticated(
            &admin_token,
            Request::builder()
                .method(Method::POST)
                .uri(format!("/v1/principals/{}/revoke", target.id()))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    serde_json::json!({
                        "principal_kind": "service",
                        "reason": "credential believed compromised"
                    })
                    .to_string(),
                ))
                .expect("revoke request builds"),
        )
        .await
        .expect("revoke call completes");
    assert!(
        revoke_resp.status().is_success(),
        "revocation POST succeeds: {}",
        revoke_resp.status()
    );

    // The suspended principal's key mints nothing new.
    assert!(
        srv.exchange_api_key(&target_key).await.is_err(),
        "a revoked principal cannot exchange its key again"
    );

    // The token minted before revocation is a self-contained snapshot, so it
    // keeps authorizing until it expires rather than being introspected.
    assert_v1_authz_check_ok(&srv, &target_token, "revoke-window").await;
}

/// Non-admin cannot revoke a principal — must return 403.
#[tokio::test]
#[ignore = "requires the Keycloak and Dex identity lane"]
async fn revocation_requires_admin_permission() {
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
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    serde_json::json!({
                        "principal_kind": "service",
                        "reason": "unauthorized attempt"
                    })
                    .to_string(),
                ))
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
///
/// # Panics
/// Panics when bootstrap or either key exchange fails, `/auth/issue-key` does
/// not return `200` with a non-nil `key_id`, or the issued key's token does not
/// reach `/v1/authz/check` `200`.
#[tokio::test]
#[ignore = "requires the Keycloak and Dex identity lane"]
async fn service_account_issuer_full_chain() {
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
    // Holds writer so the issued token can later be the terminal's subject.
    let target = srv
        .bootstrap_service("sa-chain-target", &["writer"])
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
    assert!(!issue_body.key_id.is_nil(), "issued key has a key_id");

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

/// Deployment public origin every human journey configures; the tenant
/// connection callback URL is `{origin}/auth/callback`, which the Keycloak
/// fixture clients register.
const PUBLIC_ORIGIN: &str = "http://test-tenant-1.wyrd.test";

/// The public PKCE Keycloak client human journeys sign in through.
const PUBLIC_HUMAN_CLIENT: &str = "wyrd-human";

/// The confidential Keycloak client the rotation journey authenticates to
/// with `SecretPost`, and its fixture secret.
const CONFIDENTIAL_HUMAN_CLIENT: &str = "wyrd-human-confidential";
/// The fixture secret Keycloak registers for [`CONFIDENTIAL_HUMAN_CLIENT`].
const CONFIDENTIAL_HUMAN_SECRET: &str = "wyrd-human-confidential-secret";

/// A server builder with the deployment public origin every human journey
/// needs to stage a tenant connection.
fn human_server_builder() -> WyrdTestServerBuilder {
    WyrdTestServerBuilder::default()
        .with_public_origin(PUBLIC_ORIGIN.parse().expect("public origin parses"))
}

/// The group map every human journey grants through: the realm's
/// `wyrd-admins` group confers `writer`, so alice can be a delegation subject.
fn admins_write() -> HashMap<String, Vec<String>> {
    HashMap::from([("wyrd-admins".to_owned(), vec!["writer".to_owned()])])
}

/// A tenant administrator's access token and its recovery API key.
struct TenantAdmin {
    /// Bearer access token presented on every connection administration call.
    token: String,
    /// The admin's own API key; activation presents it as the recovery key.
    api_key: SecretString,
}

/// Bootstrap a built-in `admin` service principal in `tenant` and exchange its
/// key: the principal that holds `identity_connections:write` and whose key
/// doubles as the activation recovery key.
///
/// # Panics
/// Panics when the bootstrap or exchange fails.
async fn tenant_admin(srv: &WyrdTestServer, tenant: DataTenantId, name: &str) -> TenantAdmin {
    let admin = srv
        .bootstrap_service_in_tenant(tenant, name, &["admin"])
        .await
        .expect("tenant admin bootstraps");
    let api_key = admin.api_key().expect("admin has an api key").clone();
    let token = srv
        .exchange_api_key(&api_key)
        .await
        .expect("admin api key exchanges");
    TenantAdmin { token, api_key }
}

/// Call one authenticated JSON route and read its status and body.
///
/// # Panics
/// Panics when the request cannot be built or the router fails.
async fn call_json(
    srv: &WyrdTestServer,
    token: &str,
    method: Method,
    uri: &str,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let mut request = Request::builder().method(method).uri(uri);
    let body = match body {
        Some(body) => {
            request = request.header(header::CONTENT_TYPE, "application/json");
            Body::from(body.to_string())
        }
        None => Body::empty(),
    };
    let response = srv
        .oneshot_authenticated(token, request.body(body).expect("request builds"))
        .await
        .expect("route responds");
    let status = response.status();
    let bytes = to_bytes(response.into_body(), 262_144)
        .await
        .expect("body reads");
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

/// A `ConnectionInput` body for the Keycloak realm.
fn connection_input(
    client_id: &str,
    client_auth: &str,
    client_secret: Option<&str>,
    expected_revision: Option<u64>,
) -> Value {
    serde_json::json!({
        "issuer": keycloak_issuer(),
        "client_id": client_id,
        "client_auth": client_auth,
        "client_secret": client_secret,
        "claim_mapping": { "subject": "sub", "email": "email", "groups": "groups" },
        "group_role_map": admins_write(),
        "expected_revision": expected_revision,
    })
}

/// Stage, test, and activate one Keycloak connection for `admin`'s tenant
/// through the served connection API, returning the Active view.
///
/// Any existing candidate is replaced at its current revision.
///
/// # Panics
/// Panics when any step does not return `200`.
async fn activate_keycloak_connection(
    srv: &WyrdTestServer,
    admin: &TenantAdmin,
    client_id: &str,
    client_auth: &str,
    client_secret: Option<&str>,
) -> Value {
    let (_, listed) = call_json(srv, &admin.token, Method::GET, CONNECTIONS, None).await;
    let current = listed["candidate"]["revision"].as_u64();
    let (status, candidate) = call_json(
        srv,
        &admin.token,
        Method::PUT,
        CANDIDATE,
        Some(connection_input(
            client_id,
            client_auth,
            client_secret,
            current,
        )),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "candidate stages: {candidate}");
    let revision = candidate["revision"].as_u64().expect("candidate revision");
    let (status, tested) = call_json(
        srv,
        &admin.token,
        Method::POST,
        CANDIDATE_TEST,
        Some(serde_json::json!({ "expected_revision": revision })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "candidate tests: {tested}");
    let (status, active) = call_json(
        srv,
        &admin.token,
        Method::POST,
        CANDIDATE_ACTIVATE,
        Some(activation(revision, &admin.api_key)),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "candidate activates: {active}");
    assert_eq!(active["state"], "Active");
    active
}

/// A `ConnectionActivate` body.
fn activation(revision: u64, recovery_key: &SecretString) -> Value {
    serde_json::json!({
        "expected_revision": revision,
        "recovery_api_key": recovery_key.expose_secret(),
    })
}

/// Tenant connection administration routes.
const CONNECTIONS: &str = "/v1/identity/oidc/connections";
/// Stage or replace the tenant's Candidate connection.
const CANDIDATE: &str = "/v1/identity/oidc/candidate";
/// Probe the Candidate and stamp it Tested on success.
const CANDIDATE_TEST: &str = "/v1/identity/oidc/candidate/test";
/// Promote the Tested Candidate to Active.
const CANDIDATE_ACTIVATE: &str = "/v1/identity/oidc/candidate/activate";
/// Deactivate the tenant's Active connection.
const ACTIVE_DEACTIVATE: &str = "/v1/identity/oidc/active/deactivate";

/// Start a human journey server whose fixture tenant has an Active public
/// Keycloak connection granting `writer` through `wyrd-admins`.
///
/// # Panics
/// Panics when the server or the connection setup fails.
async fn human_server() -> WyrdTestServer {
    let srv = human_server_builder()
        .start_in_process()
        .await
        .expect("test server starts");
    let admin = tenant_admin(&srv, srv.data_tenant_id(), "human-connection-admin").await;
    activate_keycloak_connection(&srv, &admin, PUBLIC_HUMAN_CLIENT, "Public", None).await;
    srv
}

/// Drive one complete browser login and return the token response body.
///
/// This is the served human path end to end: [`authorization_code`] mints the
/// login and authenticates the user at Keycloak, and [`finish_callback`]
/// exchanges the code for a Wyrd session. Every human journey starts here,
/// and the role-change journey runs it several times against the same user,
/// so the flow is one helper rather than three copies.
///
/// # Panics
/// Panics when any leg of the flow does not return `200`.
async fn human_login(
    srv: &WyrdTestServer,
    keycloak: &OidcIssuerFixture,
    client_id: &str,
    username: &str,
    password: &str,
) -> Value {
    let (code, state) = authorization_code(srv, keycloak, client_id, username, password).await;
    let (status, session) = finish_callback(srv, &code, &state).await;
    assert_eq!(
        status,
        StatusCode::OK,
        "authorization code exchange returns 200: {session}"
    );
    session
}

/// Start a login on `srv` and authenticate `username` at Keycloak, returning
/// the authorization code and echoed state the callback will present.
///
/// `GET /auth/login` mints the authorization URL, PKCE challenge, nonce, and
/// state; the Keycloak fixture submits the real HTML login form. Nothing is
/// exchanged yet, so a journey can change the tenant's connection between
/// provider authentication and the callback.
///
/// # Panics
/// Panics when login initiation does not return `200`, when the
/// authorization URL omits the PKCE challenge or nonce, or when the IdP echoes
/// a different `state` than the one Wyrd issued.
async fn authorization_code(
    srv: &WyrdTestServer,
    keycloak: &OidcIssuerFixture,
    client_id: &str,
    username: &str,
    password: &str,
) -> (String, String) {
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

    let redirect_uri: Url = "http://test-tenant-1.wyrd.test/auth/callback"
        .parse()
        .expect("redirect URI parses");
    let login_result = keycloak
        .human_login(
            client_id,
            username,
            password,
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
    (login_result.code, login_result.state)
}

/// Present `code` and `state` to `GET /auth/callback` and return the status
/// and JSON body (`Null` when the body is not JSON).
///
/// # Panics
/// Panics when the request cannot be built or the router fails.
async fn finish_callback(srv: &WyrdTestServer, code: &str, state: &str) -> (StatusCode, Value) {
    let code_encoded: String = url::form_urlencoded::byte_serialize(code.as_bytes()).collect();
    let state_encoded: String = url::form_urlencoded::byte_serialize(state.as_bytes()).collect();
    let callback_resp = srv
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri(format!(
                    "/auth/callback?code={code_encoded}&state={state_encoded}"
                ))
                .header(header::HOST, "test-tenant-1.wyrd.test")
                .body(Body::empty())
                .expect("callback request builds"),
        )
        .await
        .expect("callback call completes");
    let status = callback_resp.status();
    let token_bytes = to_bytes(callback_resp.into_body(), 65_536)
        .await
        .expect("token body reads");
    (
        status,
        serde_json::from_slice(&token_bytes).unwrap_or(Value::Null),
    )
}

/// Read the Wyrd principal id a session's access token was minted for.
///
/// Wyrd puts the principal id in `sub`, so a journey that has only driven the
/// browser flow can still name the principal an administrator must revoke.
/// There is no served endpoint that answers "who am I" with the id, and
/// reaching into the tenant store instead would prove the revocation against
/// state the caller never sees.
///
/// # Panics
/// Panics when the token carries no string `sub`.
fn principal_id_of(access_token: &str) -> String {
    jwt_claims(access_token)["sub"]
        .as_str()
        .expect("a Wyrd access token names its principal in sub")
        .to_owned()
}

/// Drive a complete config-driven human OIDC login:
///   1. a tenant admin stages, tests, and activates the Keycloak connection
///      through `/v1/identity/oidc/*`, granting `writer` through the
///      `wyrd-admins` group so the federated human can be a subject,
///   2. `GET /auth/login` → authorization URL + state,
///   3. `OidcIssuerFixture::human_login` authenticates alice → code + state,
///   4. `GET /auth/callback` → Wyrd access token,
///   5. the human token reaches a real `/v1/authz/check` `200` via delegation.
///
/// It then rotates the refresh token, proves the successor still authorizes,
/// and proves a replay of the consumed token is refused and kills the successor.
///
/// # Panics
/// Panics when the login flow fails, the session lacks an access or refresh
/// token, rotation does not return `200` with both tokens, either access token
/// fails to reach `200`, the replay is not `401 WYRD_AUTH_401_REFRESH_REUSED`,
/// or the successor can still rotate after the replay.
#[tokio::test]
#[ignore = "requires the Keycloak and Dex identity lane"]
async fn human_oidc_login_journey() {
    let keycloak = OidcIssuerFixture::connect(&keycloak_issuer()).await;

    let srv = human_server().await;

    let token_body = human_login(
        &srv,
        &keycloak,
        PUBLIC_HUMAN_CLIENT,
        "alice",
        "alice-password",
    )
    .await;
    let access_token = token_body["access_token"]
        .as_str()
        .expect("access_token present in response");
    assert!(!access_token.is_empty(), "access token is non-empty");

    // Step 4: human token reaches a real authenticated /v1 200 via delegation.
    assert_v1_authz_check_ok(&srv, access_token, "human-sso").await;

    // Step 5: the human session carries a refresh token. Only human sessions
    // do; a machine client re-exchanges its durable credential instead.
    let refresh_token = token_body["refresh_token"]
        .as_str()
        .expect("a human session is issued a refresh token")
        .to_owned();

    // Step 6: renew the session. A real UI does this once the 15-minute access
    // token expires; the grant does not consult the clock, so presenting the
    // refresh token is the whole renewal.
    let (rotated_status, rotated_body) = post_refresh(&srv, &refresh_token).await;
    assert_eq!(
        rotated_status,
        StatusCode::OK,
        "refresh rotation returns 200: {rotated_body}"
    );
    let rotated_access = rotated_body["access_token"]
        .as_str()
        .expect("rotation returns an access token")
        .to_owned();
    let successor_refresh = rotated_body["refresh_token"]
        .as_str()
        .expect("rotation returns the successor refresh token")
        .to_owned();

    // Step 7: the successor reaches the same protected /v1 200, proving the
    // renewed session kept the authority the provider asserted at login.
    assert_v1_authz_check_ok(&srv, &rotated_access, "human-sso-rotated").await;

    // Step 8: replaying the consumed token is refused and contains the theft.
    let (replay_status, replay_body) = post_refresh(&srv, &refresh_token).await;
    assert_eq!(
        replay_status,
        StatusCode::UNAUTHORIZED,
        "replaying the consumed refresh token is refused: {replay_body}"
    );
    assert_eq!(
        response_code(&replay_body),
        "WYRD_AUTH_401_REFRESH_REUSED",
        "replay renders the reuse code: {replay_body}"
    );

    // Step 9: containment was committed with the refusal, so the successor the
    // attacker would hold is dead on a separate request and transaction.
    let (successor_status, successor_body) = post_refresh(&srv, &successor_refresh).await;
    assert_eq!(
        successor_status,
        StatusCode::UNAUTHORIZED,
        "the successor cannot rotate after replay: {successor_body}"
    );
}

/// Present a refresh token to `POST /auth/token` and read the status and body.
///
/// The rotation half of the human session journey runs several times — renew,
/// replay, and then the revoked successor — and each call needs the same
/// tenant host header and JSON envelope.
async fn post_refresh(srv: &WyrdTestServer, refresh_token: &str) -> (StatusCode, Value) {
    let body = serde_json::json!({
        "grant_type": "refresh_token",
        "refresh_token": refresh_token,
    });
    let response = srv
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/auth/token")
                .header(header::HOST, "test-tenant-1.wyrd.test")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(body.to_string()))
                .expect("refresh request builds"),
        )
        .await
        .expect("refresh call completes");
    let status = response.status();
    let bytes = to_bytes(response.into_body(), 65_536)
        .await
        .expect("refresh body reads");
    let parsed = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (status, parsed)
}

/// Revoking a human retires the session's refresh authority immediately.
///
/// A human session holds a short-lived access token and a refresh token. This
/// journey drives the served path an operator actually uses: log in, use the
/// access token, revoke the User principal through `/v1/principals/{id}/revoke`,
/// and then show the session cannot continue — the refresh token cannot rotate,
/// so no successor exists — while the access token already issued keeps its
/// snapshot authority only until its five-minute expiry.
///
/// # Panics
/// Panics when login yields no access or refresh token, admin bootstrap or
/// exchange fails, the revoke `POST` does not succeed, the issued access token
/// stops reaching `200` inside its window, or the refresh rotation is not `401`
/// or returns any successor token.
#[tokio::test]
#[ignore = "requires the Keycloak and Dex identity lane"]
async fn revoking_a_human_kills_the_session_refresh_authority() {
    let keycloak = OidcIssuerFixture::connect(&keycloak_issuer()).await;
    let srv = human_server().await;

    let token_body = human_login(
        &srv,
        &keycloak,
        PUBLIC_HUMAN_CLIENT,
        "alice",
        "alice-password",
    )
    .await;
    let access_token = token_body["access_token"]
        .as_str()
        .expect("access_token present")
        .to_owned();
    let refresh_token = token_body["refresh_token"]
        .as_str()
        .expect("a human session is issued a refresh token")
        .to_owned();

    // The live session works before anyone revokes it.
    assert_v1_authz_check_ok(&srv, &access_token, "human-revoke").await;

    // An administrator revokes the human principal by id and kind.
    let admin = srv
        .bootstrap_service("human-revoke-admin", &["admin"])
        .await
        .expect("admin bootstraps");
    let admin_token = srv
        .exchange_api_key(&admin.api_key().expect("admin has api key").clone())
        .await
        .expect("admin api key exchange succeeds");
    let revoke_resp = srv
        .oneshot_authenticated(
            &admin_token,
            Request::builder()
                .method(Method::POST)
                .uri(format!(
                    "/v1/principals/{}/revoke",
                    principal_id_of(&access_token)
                ))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    serde_json::json!({
                        "principal_kind": "user",
                        "reason": "offboarded, session must end now"
                    })
                    .to_string(),
                ))
                .expect("revoke request builds"),
        )
        .await
        .expect("revoke call completes");
    assert!(
        revoke_resp.status().is_success(),
        "revoking the human principal succeeds: {}",
        revoke_resp.status()
    );

    // The access token is a self-contained snapshot and lapses at expiry.
    assert_v1_authz_check_ok(&srv, &access_token, "human-revoke-window").await;

    // The refresh half is retired in the same transaction, so rotation is
    // refused and mints no successor for the session to continue under.
    let (refresh_status, refresh_body) = post_refresh(&srv, &refresh_token).await;
    assert_eq!(
        refresh_status,
        StatusCode::UNAUTHORIZED,
        "the revoked human's refresh token cannot rotate: {refresh_body}"
    );
    assert!(
        refresh_body["access_token"].is_null() && refresh_body["refresh_token"].is_null(),
        "a refused rotation issues no successor: {refresh_body}"
    );
}

/// A withdrawn provider role governs every token issued after it.
///
/// Wyrd's access token carries the permissions resolved at issuance, so a
/// token issued before the provider withdrew a group keeps that snapshot until
/// its five-minute expiry. This journey drives that from the provider side: log
/// in as a member of `wyrd-admins`, log in again unchanged and show the first
/// session still works, then remove the group at Keycloak and log in once more.
/// The changed login persists the reduced role set and issues a successor that
/// yields a delegated token that can no longer write; the earlier token is not
/// introspected.
///
/// The membership is restored before the journey returns, because the realm is
/// shared with every other Keycloak journey in this target.
///
/// # Panics
/// Panics when the server or actor bootstrap fails, a login yields no access
/// token, the granted session stops reaching `200` after an unchanged
/// re-login, delegation from the reduced session fails, or the reduced
/// session's delegated `card_write` check is not `Deny`. A panic skips the
/// membership restore and leaves the shared realm altered.
#[tokio::test]
#[ignore = "requires the Keycloak and Dex identity lane"]
async fn a_withdrawn_oidc_group_invalidates_the_roles_it_granted() {
    let keycloak = OidcIssuerFixture::connect(&keycloak_issuer())
        .await
        .with_keycloak_admin(keycloak_admin());
    keycloak
        .set_group_membership("alice", "wyrd-admins", true)
        .await;

    let srv = human_server().await;

    // The group grants writer, so the first session's delegated checks allow.
    let granted = human_login(
        &srv,
        &keycloak,
        PUBLIC_HUMAN_CLIENT,
        "alice",
        "alice-password",
    )
    .await;
    let granted_token = granted["access_token"]
        .as_str()
        .expect("access_token present")
        .to_owned();
    assert_v1_authz_check_ok(&srv, &granted_token, "roles-granted").await;

    // A second login asserting the same groups changes nothing, so the first
    // session keeps working: re-authenticating must not log a user out.
    let _unchanged = human_login(
        &srv,
        &keycloak,
        PUBLIC_HUMAN_CLIENT,
        "alice",
        "alice-password",
    )
    .await;
    assert_v1_authz_check_ok(&srv, &granted_token, "roles-unchanged").await;

    // The provider withdraws the group; the next login persists the reduced set.
    keycloak
        .set_group_membership("alice", "wyrd-admins", false)
        .await;
    let reduced = human_login(
        &srv,
        &keycloak,
        PUBLIC_HUMAN_CLIENT,
        "alice",
        "alice-password",
    )
    .await;
    let reduced_token = reduced["access_token"]
        .as_str()
        .expect("access_token present")
        .to_owned();

    let actor = srv
        .bootstrap_service("roles-withdrawn-actor", &["writer"])
        .await
        .expect("actor bootstraps");
    let actor_jwt = srv
        .exchange_api_key(actor.api_key().expect("actor has key"))
        .await
        .expect("actor key exchanges");

    // The successor the same login issued carries the reduced authority: it can
    // still be a subject, but the intersection no longer holds card_write.
    let delegated = srv
        .delegate(&reduced_token, &actor_jwt, TokenAudience::Wyrd)
        .await
        .expect("the reduced session is still a valid subject");
    let result = srv
        .authz_check(&delegated, authz_check_request(&actor, "card_write"))
        .await
        .expect("authz-check completes");
    assert_eq!(
        result.response.map(|response| response.decision),
        Some(AuthzCheckDecision::Deny),
        "the reduced session cannot write through its actor"
    );

    keycloak
        .set_group_membership("alice", "wyrd-admins", true)
        .await;
}

// ─── Tenant human connection administration ─────────────────────────────────

/// Assert a refusal's status and stable code.
///
/// # Panics
/// Panics when either differs.
fn assert_refused(status: StatusCode, body: &Value, expected: StatusCode, code: &str) {
    assert_eq!(status, expected, "expected {expected} {code}: {body}");
    assert_eq!(response_code(body), code, "stable code: {body}");
}

/// Start a login at the fixture tenant's host for `issuer` and return the
/// status: `200` while the tenant's Active connection trusts the issuer.
///
/// # Panics
/// Panics when the request cannot be built or the router fails.
async fn login_status(srv: &WyrdTestServer) -> (StatusCode, Value) {
    let issuer: String =
        url::form_urlencoded::byte_serialize(keycloak_issuer().as_bytes()).collect();
    let response = srv
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri(format!("/auth/login?issuer={issuer}"))
                .header(header::HOST, "test-tenant-1.wyrd.test")
                .header(header::ACCEPT, "application/json")
                .body(Body::empty())
                .expect("login request builds"),
        )
        .await
        .expect("login responds");
    let status = response.status();
    let bytes = to_bytes(response.into_body(), 65_536)
        .await
        .expect("login body reads");
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

/// Tenant human connection administration across two tenants, one issuer.
///
/// Tenant A and tenant B each configure the same Keycloak issuer through the
/// served `/v1/identity/oidc/*` API — A with the public client, B with the
/// confidential client and a `SecretPost` secret — and the journey proves:
///   1. an unauthorized tenant principal (`runtime_admin`, which holds
///      `service_accounts:write` but not `identity_connections:write`) is
///      refused on read and write, and each refusal is audited;
///   2. each tenant reads only its own redacted connection with the
///      deployment callback URL, and no response carries B's secret;
///   3. B's administrator cannot remove A's connection (RLS answers not
///      found), and A's login keeps working afterwards;
///   4. the old Human trusted-issuer HTTP and CLI writers are refused with
///      `HUMAN_CONNECTION_REQUIRED` while a Workload CLI write still succeeds;
///   5. the served OpenAPI document publishes all six operations;
///   6. deactivation stops A's login immediately, removal tombstones the
///      connection, and the retained audit history records every decision.
///
/// # Panics
/// Panics when any step deviates from the contract above.
#[tokio::test]
#[ignore = "requires the Keycloak and Dex identity lane"]
async fn tenant_connection_admin_journey() {
    let srv = human_server_builder()
        .start_bound()
        .await
        .expect("bound server starts");
    let server_url: Url = srv
        .base_url()
        .expect("bound server exposes a base URL")
        .parse()
        .expect("base URL parses");
    let tenant_a = srv.data_tenant_id();
    let tenant_b = srv
        .seed_tenant("test-tenant-2")
        .await
        .expect("tenant B provisions");
    let admin_a = tenant_admin(&srv, tenant_a, "connection-admin-a").await;
    let admin_b = tenant_admin(&srv, tenant_b, "connection-admin-b").await;
    let operator = srv
        .bootstrap_service_in_tenant(tenant_a, "connection-runtime-admin", &["runtime_admin"])
        .await
        .expect("runtime admin bootstraps");
    let operator_token = srv
        .exchange_api_key(operator.api_key().expect("runtime admin has a key"))
        .await
        .expect("runtime admin key exchanges");

    // 1. service_accounts:write alone does not administer human SSO.
    let (status, body) = call_json(&srv, &operator_token, Method::GET, CONNECTIONS, None).await;
    assert_refused(
        status,
        &body,
        StatusCode::FORBIDDEN,
        "WYRD_PERMISSION_403_DENIED_RBAC",
    );
    let (status, body) = call_json(
        &srv,
        &operator_token,
        Method::PUT,
        CANDIDATE,
        Some(connection_input(PUBLIC_HUMAN_CLIENT, "Public", None, None)),
    )
    .await;
    assert_refused(
        status,
        &body,
        StatusCode::FORBIDDEN,
        "WYRD_PERMISSION_403_DENIED_RBAC",
    );

    // 2. Both tenants trust the same issuer through their own connection.
    let active_a =
        activate_keycloak_connection(&srv, &admin_a, PUBLIC_HUMAN_CLIENT, "Public", None).await;
    let active_b = activate_keycloak_connection(
        &srv,
        &admin_b,
        CONFIDENTIAL_HUMAN_CLIENT,
        "SecretPost",
        Some(CONFIDENTIAL_HUMAN_SECRET),
    )
    .await;
    assert_eq!(
        active_a["issuer"], active_b["issuer"],
        "the same issuer in both tenants"
    );
    let callback = format!("{PUBLIC_ORIGIN}/auth/callback");
    for (admin, tenant, client) in [
        (&admin_a, tenant_a, PUBLIC_HUMAN_CLIENT),
        (&admin_b, tenant_b, CONFIDENTIAL_HUMAN_CLIENT),
    ] {
        let (status, listed) = call_json(&srv, &admin.token, Method::GET, CONNECTIONS, None).await;
        assert_eq!(status, StatusCode::OK, "admin lists: {listed}");
        assert_eq!(listed["callback_url"], callback.as_str());
        assert_eq!(listed["active"]["tenant_id"], tenant.to_string());
        assert_eq!(
            listed["active"]["client_id"], client,
            "each tenant sees its own client"
        );
        assert!(
            listed["candidate"].is_null(),
            "activation consumed the candidate"
        );
        assert!(
            !listed.to_string().contains(CONFIDENTIAL_HUMAN_SECRET),
            "no read returns a provider secret: {listed}"
        );
    }
    let sealed = srv
        .human_connection_secret_ciphertext(tenant_b, "Active")
        .await
        .expect("ciphertext reads")
        .expect("tenant B stores a sealed secret");
    assert!(
        !sealed
            .windows(CONFIDENTIAL_HUMAN_SECRET.len())
            .any(|window| window == CONFIDENTIAL_HUMAN_SECRET.as_bytes()),
        "the secret is sealed at rest"
    );

    // 3. Tenant B cannot reach tenant A's connection.
    let a_id = active_a["id"].as_str().expect("connection id").to_owned();
    let (status, body) = call_json(
        &srv,
        &admin_b.token,
        Method::DELETE,
        &format!("{CONNECTIONS}/{a_id}"),
        None,
    )
    .await;
    assert_refused(
        status,
        &body,
        StatusCode::NOT_FOUND,
        "WYRD_SPEC_404_NOT_FOUND",
    );
    let keycloak = OidcIssuerFixture::connect(&keycloak_issuer()).await;
    let session = human_login(
        &srv,
        &keycloak,
        PUBLIC_HUMAN_CLIENT,
        "alice",
        "alice-password",
    )
    .await;
    let access = session["access_token"].as_str().expect("access token");
    assert_v1_authz_check_ok(&srv, access, "tenant-a-connection").await;

    // 4. The old Human writers are refused; Workload writes remain.
    let (status, body) = call_json(
        &srv,
        &admin_a.token,
        Method::POST,
        "/v1/admin/trusted-issuers",
        Some(serde_json::json!({
            "issuer": keycloak_issuer(),
            "expected_audience": PUBLIC_HUMAN_CLIENT,
            "client_id": PUBLIC_HUMAN_CLIENT,
            "client_auth": "Public",
            "claim_mapping": { "subject": "sub" },
            "principal_kind": "human",
        })),
    )
    .await;
    assert_refused(
        status,
        &body,
        StatusCode::BAD_REQUEST,
        "WYRD_AUTH_400_HUMAN_CONNECTION_REQUIRED",
    );
    set_cli_environment("WYRD_ACCESS_TOKEN", &admin_a.token);
    let cli_issuer = |principal_kind: &str| {
        TrustedIssuerCommand::Add(Box::new(TrustedIssuerAddArgs {
            issuer: keycloak_issuer(),
            expected_audience: "wyrd-workload".to_owned(),
            client_id: "wyrd-workload".to_owned(),
            client_auth: "Public".to_owned(),
            client_secret_file: None,
            claim_subject: "sub".to_owned(),
            claim_email: None,
            claim_groups: None,
            default_roles: Vec::new(),
            group_roles: Vec::new(),
            principal_kind: principal_kind.to_owned(),
            jwks_ttl_secs: Some(300),
            server: server_url.clone(),
        }))
    };
    let refused = trusted_issuer::dispatch(cli_issuer("Human"))
        .await
        .expect_err("the CLI refuses a Human issuer");
    assert!(
        matches!(
            refused,
            WyrdCliError::Server {
                source: WyrdError::HumanConnectionRequired { .. }
            }
        ),
        "CLI refusal is HUMAN_CONNECTION_REQUIRED: {refused:?}"
    );
    trusted_issuer::dispatch(cli_issuer("Workload"))
        .await
        .expect("a Workload issuer is still authored through the CLI");

    // 5. The served contract publishes the connection surface.
    let openapi = srv
        .oneshot(
            Request::builder()
                .uri("/openapi.json")
                .body(Body::empty())
                .expect("openapi request builds"),
        )
        .await
        .expect("openapi responds");
    let document: Value = serde_json::from_slice(
        &to_bytes(openapi.into_body(), 16 * 1024 * 1024)
            .await
            .expect("openapi body reads"),
    )
    .expect("openapi is JSON");
    for (path, method) in [
        (CONNECTIONS, "get"),
        (CANDIDATE, "put"),
        (CANDIDATE_TEST, "post"),
        (CANDIDATE_ACTIVATE, "post"),
        (ACTIVE_DEACTIVATE, "post"),
        ("/v1/identity/oidc/connections/{id}", "delete"),
    ] {
        assert!(
            document["paths"][path][method].is_object(),
            "served OpenAPI publishes {method} {path}"
        );
    }

    // 6. Deactivation stops login at once; removal tombstones the record.
    let (status, body) =
        call_json(&srv, &admin_a.token, Method::POST, ACTIVE_DEACTIVATE, None).await;
    assert_eq!(status, StatusCode::NO_CONTENT, "deactivates: {body}");
    let (status, body) = login_status(&srv).await;
    assert_refused(
        status,
        &body,
        StatusCode::UNAUTHORIZED,
        "WYRD_AUTH_401_INVALID_TOKEN",
    );
    let (status, body) = call_json(
        &srv,
        &admin_a.token,
        Method::DELETE,
        &format!("{CONNECTIONS}/{a_id}"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT, "removes: {body}");
    let (status, body) = call_json(
        &srv,
        &admin_a.token,
        Method::DELETE,
        &format!("{CONNECTIONS}/{a_id}"),
        None,
    )
    .await;
    assert_refused(
        status,
        &body,
        StatusCode::NOT_FOUND,
        "WYRD_SPEC_404_NOT_FOUND",
    );
    let (_, listed) = call_json(&srv, &admin_a.token, Method::GET, CONNECTIONS, None).await;
    assert!(
        listed["active"].is_null() && listed["candidate"].is_null(),
        "{listed}"
    );
    let (_, listed_b) = call_json(&srv, &admin_b.token, Method::GET, CONNECTIONS, None).await;
    assert_eq!(
        listed_b["active"]["id"], active_b["id"],
        "tenant B is untouched"
    );

    srv.await_audit_published(tenant_a)
        .await
        .expect("tenant A audit publishes");
    let decisions = srv
        .retained_audit_records(
            tenant_a,
            "operation, outcome",
            "operation LIKE 'identity.oidc.%'",
        )
        .await
        .expect("retained audit reads");
    let decisions: Vec<(String, String)> = decisions
        .into_iter()
        .map(|row| {
            (
                row[0].clone().unwrap_or_default(),
                row[1].clone().unwrap_or_default().to_lowercase(),
            )
        })
        .collect();
    for (operation, outcome) in [
        ("identity.oidc.connections.list", "denied"),
        ("identity.oidc.candidate.put", "denied"),
        ("identity.oidc.candidate.put", "allowed"),
        ("identity.oidc.candidate.test", "allowed"),
        ("identity.oidc.candidate.tested", "allowed"),
        ("identity.oidc.candidate.activate", "allowed"),
        ("identity.oidc.active.deactivate", "allowed"),
        ("identity.oidc.connection.remove", "allowed"),
    ] {
        assert!(
            decisions
                .iter()
                .any(|(op, out)| op == operation && out == outcome),
            "retained audit records {operation} {outcome}: {decisions:?}"
        );
    }
    let audit_text = format!("{decisions:?}");
    assert!(!audit_text.contains(CONFIDENTIAL_HUMAN_SECRET));

    srv.shutdown().await.expect("server shuts down cleanly");
}

/// Candidate lifecycle, secret rotation, and replica safety for one tenant.
///
/// Two replicas share one database. Replica A seals with key K1 and retains
/// K2; the journey proves:
///   1. an unsafe issuer (cloud metadata address) stages but fails its test
///      before any connection, and a stale `expected_revision` conflicts;
///   2. a candidate with a wrong secret fails its test as
///      `client_auth_rejected` and cannot be activated;
///   3. the fixed candidate's stamp lasts fifteen minutes, a malformed
///      recovery key and one without `identity_connections:write` are
///      refused, and an injected audit failure rolls activation back;
///   4. two concurrent activations of the same revision, recovered by a
///      principal distinct from the bearer caller, yield exactly one winner;
///   5. replica B boots with K2 as the write key and K1 retained, rewraps the
///      stored secret under K2, and both replicas still sign alice in;
///   6. A, still sealing with K1, stages a late secret after B's pass; A,
///      the only K1 writer, is then shut down, so the final pass (replica C,
///      started from K2-writing B) runs with no K1 writer left in service,
///      leaves every stored secret current under K2, and a replica holding
///      only K2 then signs alice in;
///   7. from then on only K2 writers serve: a same-issuer secret rotation
///      staged on B survives a failed append of the recovery principal's
///      decision with the Active and Candidate unchanged, is then activated
///      and served by the K2-only replica at once, and deactivation on B
///      stops login there;
///   8. the canonical audit staging attributes each recovery decision to its
///      principal and verified credential: Denied for the underprivileged key, Allowed for
///      each successful activation, and nothing for the malformed key.
///
/// # Panics
/// Panics when any step deviates from the contract above.
#[tokio::test]
#[ignore = "requires the Keycloak and Dex identity lane"]
async fn tenant_connection_rotation_journey() {
    let k1 = [0x11_u8; 32];
    let k2 = [0x22_u8; 32];
    let keyring = |write: [u8; 32], retained: [u8; 32]| {
        std::sync::Arc::new(
            SealingKeyring::new(SecretKey::from_bytes(write))
                .with_retained(SecretKey::from_bytes(retained)),
        )
    };
    let replica_a = human_server_builder()
        .with_sealing_keyring(keyring(k1, k2))
        .start_in_process()
        .await
        .expect("replica A starts");
    let tenant = replica_a.data_tenant_id();
    let admin = tenant_admin(&replica_a, tenant, "rotation-admin").await;
    let token = admin.token.as_str();

    // 1. Unsafe discovery fails closed; stale revisions conflict.
    let mut unsafe_input = connection_input(PUBLIC_HUMAN_CLIENT, "Public", None, None);
    unsafe_input["issuer"] = "https://169.254.169.254".into();
    let (status, staged) = call_json(
        &replica_a,
        token,
        Method::PUT,
        CANDIDATE,
        Some(unsafe_input),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "staging does no provider IO: {staged}"
    );
    assert_eq!(staged["revision"], 1);
    let (status, body) = call_json(
        &replica_a,
        token,
        Method::POST,
        CANDIDATE_TEST,
        Some(serde_json::json!({ "expected_revision": 1 })),
    )
    .await;
    assert_refused(
        status,
        &body,
        StatusCode::SERVICE_UNAVAILABLE,
        "WYRD_AUTH_503_DISCOVERY_UNAVAILABLE",
    );
    let (status, body) = call_json(
        &replica_a,
        token,
        Method::PUT,
        CANDIDATE,
        Some(connection_input(PUBLIC_HUMAN_CLIENT, "Public", None, None)),
    )
    .await;
    assert_refused(
        status,
        &body,
        StatusCode::CONFLICT,
        "WYRD_AUTH_409_CONNECTION_CONFLICT",
    );

    // 2. A wrong secret fails the client-authentication probe.
    let (status, staged) = call_json(
        &replica_a,
        token,
        Method::PUT,
        CANDIDATE,
        Some(connection_input(
            CONFIDENTIAL_HUMAN_CLIENT,
            "SecretPost",
            Some("not-the-secret"),
            Some(1),
        )),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "candidate replaces: {staged}");
    assert_eq!(staged["revision"], 2);
    let (status, body) = call_json(
        &replica_a,
        token,
        Method::POST,
        CANDIDATE_TEST,
        Some(serde_json::json!({ "expected_revision": 2 })),
    )
    .await;
    assert_refused(
        status,
        &body,
        StatusCode::CONFLICT,
        "WYRD_AUTH_409_CONNECTION_NOT_TESTED",
    );
    assert_eq!(body["details"]["reason"], "client_auth_rejected", "{body}");
    let (status, body) = call_json(
        &replica_a,
        token,
        Method::POST,
        CANDIDATE_ACTIVATE,
        Some(activation(2, &admin.api_key)),
    )
    .await;
    assert_refused(
        status,
        &body,
        StatusCode::CONFLICT,
        "WYRD_AUTH_409_CONNECTION_NOT_TESTED",
    );

    // 3. The fixed candidate tests for fifteen minutes; activation still
    //    needs a qualifying recovery key and a durable audit decision.
    let (status, staged) = call_json(
        &replica_a,
        token,
        Method::PUT,
        CANDIDATE,
        Some(connection_input(
            CONFIDENTIAL_HUMAN_CLIENT,
            "SecretPost",
            Some(CONFIDENTIAL_HUMAN_SECRET),
            Some(2),
        )),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "candidate replaces: {staged}");
    assert!(
        staged["tested_until"].is_null(),
        "a replaced candidate is untested"
    );
    let (status, tested) = call_json(
        &replica_a,
        token,
        Method::POST,
        CANDIDATE_TEST,
        Some(serde_json::json!({ "expected_revision": 3 })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "candidate tests: {tested}");
    assert_eq!(tested["candidate"]["tested_revision"], 3);
    let tested_until: chrono::DateTime<chrono::Utc> =
        serde_json::from_value(tested["candidate"]["tested_until"].clone())
            .expect("tested_until is a timestamp");
    // Postgres stamps the deadline on its own clock, so allow a few seconds
    // of skew against the test host's clock on either side.
    let window = tested_until - chrono::Utc::now();
    assert!(
        window > ChronoDuration::minutes(14)
            && window <= ChronoDuration::minutes(15) + ChronoDuration::seconds(5),
        "the stamp lasts fifteen minutes: {window}"
    );
    let runtime_admin = replica_a
        .bootstrap_service_in_tenant(tenant, "rotation-runtime-admin", &["runtime_admin"])
        .await
        .expect("runtime admin bootstraps");
    let recovery_admin = replica_a
        .bootstrap_service_in_tenant(tenant, "rotation-recovery-admin", &["admin"])
        .await
        .expect("recovery admin bootstraps");
    let recovery_key = recovery_admin
        .api_key()
        .expect("recovery admin has a key")
        .clone();
    let (status, body) = call_json(
        &replica_a,
        token,
        Method::POST,
        CANDIDATE_ACTIVATE,
        Some(activation(3, &SecretString::from("not-a-wyrd-api-key"))),
    )
    .await;
    assert_refused(
        status,
        &body,
        StatusCode::CONFLICT,
        "WYRD_AUTH_409_CONNECTION_CONFLICT",
    );
    let (status, body) = call_json(
        &replica_a,
        token,
        Method::POST,
        CANDIDATE_ACTIVATE,
        Some(activation(
            3,
            runtime_admin.api_key().expect("runtime admin has a key"),
        )),
    )
    .await;
    assert_refused(
        status,
        &body,
        StatusCode::CONFLICT,
        "WYRD_AUTH_409_CONNECTION_CONFLICT",
    );
    assert!(
        !body.to_string().contains(
            runtime_admin
                .api_key()
                .expect("runtime admin has a key")
                .expose_secret()
        ),
        "the refusal never echoes the recovery key"
    );

    let superuser = replica_a
        .pg_fixture()
        .superuser_pool()
        .await
        .expect("superuser pool opens");
    sqlx::query(
        r#"CREATE OR REPLACE FUNCTION vala.test_fail_connection_activation_audit()
           RETURNS trigger LANGUAGE plpgsql AS $$
           BEGIN
             IF NEW.operation = 'identity.oidc.candidate.activate' THEN
               RAISE EXCEPTION 'injected connection activation audit failure';
             END IF;
             RETURN NEW;
           END;
           $$;"#,
    )
    .execute(&superuser)
    .await
    .expect("failure function installs");
    sqlx::query(
        r#"CREATE TRIGGER test_fail_connection_activation_audit
           BEFORE INSERT ON vala.audit_staging
           FOR EACH ROW EXECUTE FUNCTION vala.test_fail_connection_activation_audit()"#,
    )
    .execute(&superuser)
    .await
    .expect("failure trigger installs");
    let (status, body) = call_json(
        &replica_a,
        token,
        Method::POST,
        CANDIDATE_ACTIVATE,
        Some(activation(3, &admin.api_key)),
    )
    .await;
    assert!(
        status.is_server_error(),
        "activation without a durable decision fails closed: {status} {body}"
    );
    sqlx::query("DROP TRIGGER test_fail_connection_activation_audit ON vala.audit_staging")
        .execute(&superuser)
        .await
        .expect("failure trigger drops");
    let (_, listed) = call_json(&replica_a, token, Method::GET, CONNECTIONS, None).await;
    assert!(
        listed["active"].is_null(),
        "the failed activation rolled back: {listed}"
    );
    assert_eq!(listed["candidate"]["revision"], 3, "{listed}");

    // 4. Concurrent activations of one revision have exactly one winner.
    let first = call_json(
        &replica_a,
        token,
        Method::POST,
        CANDIDATE_ACTIVATE,
        Some(activation(3, &recovery_key)),
    );
    let second = call_json(
        &replica_a,
        token,
        Method::POST,
        CANDIDATE_ACTIVATE,
        Some(activation(3, &recovery_key)),
    );
    let ((first_status, first_body), (second_status, second_body)) = tokio::join!(first, second);
    let mut statuses = [first_status, second_status];
    statuses.sort();
    assert_eq!(
        statuses,
        [StatusCode::OK, StatusCode::CONFLICT],
        "exactly one activation wins: {first_body} / {second_body}"
    );

    let keycloak = OidcIssuerFixture::connect(&keycloak_issuer()).await;
    let session = human_login(
        &replica_a,
        &keycloak,
        CONFIDENTIAL_HUMAN_CLIENT,
        "alice",
        "alice-password",
    )
    .await;
    assert_v1_authz_check_ok(
        &replica_a,
        session["access_token"].as_str().expect("access token"),
        "rotation-k1",
    )
    .await;

    // 5. Replica B switches the write key to K2; its boot rewraps the secret.
    let under_k1 = replica_a
        .human_connection_secret_ciphertext(tenant, "Active")
        .await
        .expect("ciphertext reads")
        .expect("active connection stores a secret");
    let k2_only = SealingKeyring::new(SecretKey::from_bytes(k2));
    assert!(
        k2_only.open(&under_k1).is_err(),
        "K1 sealed the first secret"
    );
    let replica_b = replica_a
        .start_replica(human_server_builder().with_sealing_keyring(keyring(k2, k1)))
        .await
        .expect("replica B starts");
    let under_k2 = replica_a
        .human_connection_secret_ciphertext(tenant, "Active")
        .await
        .expect("ciphertext reads")
        .expect("active connection stores a secret");
    assert_ne!(under_k1, under_k2, "boot rewrapped the stored secret");
    assert_eq!(
        k2_only
            .rewrap(&under_k2)
            .expect("K2 opens the rewrapped secret"),
        None,
        "the rewrapped secret references only the write key"
    );
    for (replica, label) in [(&replica_a, "rotation-a"), (&replica_b, "rotation-b")] {
        let session = human_login(
            replica,
            &keycloak,
            CONFIDENTIAL_HUMAN_CLIENT,
            "alice",
            "alice-password",
        )
        .await;
        assert_v1_authz_check_ok(
            replica,
            session["access_token"].as_str().expect("access token"),
            label,
        )
        .await;
    }

    // 6. A still seals with K1 after B's pass reported nothing left: a late
    //    K1 write. A is then taken out of service, so no K1 writer remains;
    //    the final pass (replica C, started from K2-writing B) reseals the
    //    late write, and a K2-only replica then serves login.
    let (status, late) = call_json(
        &replica_a,
        token,
        Method::PUT,
        CANDIDATE,
        Some(connection_input(
            CONFIDENTIAL_HUMAN_CLIENT,
            "SecretPost",
            Some(CONFIDENTIAL_HUMAN_SECRET),
            None,
        )),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "A stages a late candidate: {late}");
    let late_under_k1 = replica_a
        .human_connection_secret_ciphertext(tenant, "Candidate")
        .await
        .expect("ciphertext reads")
        .expect("the late candidate stores a secret");
    assert!(
        k2_only.open(&late_under_k1).is_err(),
        "A sealed the late write under K1"
    );
    replica_a
        .shutdown()
        .await
        .expect("replica A, the last K1 writer, shuts down");
    let replica_c = replica_b
        .start_replica(human_server_builder().with_sealing_keyring(keyring(k2, k1)))
        .await
        .expect("final-pass replica C starts");
    for state in ["Active", "Candidate"] {
        let sealed = replica_b
            .human_connection_secret_ciphertext(tenant, state)
            .await
            .expect("ciphertext reads")
            .expect("the connection stores a secret");
        assert_eq!(
            k2_only
                .rewrap(&sealed)
                .expect("K2 opens every stored secret"),
            None,
            "the final pass left the {state} secret current under K2"
        );
    }
    replica_c.shutdown().await.expect("replica C shuts down");
    let replica_k2 = replica_b
        .start_replica(
            human_server_builder().with_sealing_keyring(std::sync::Arc::new(SealingKeyring::new(
                SecretKey::from_bytes(k2),
            ))),
        )
        .await
        .expect("a K2-only replica starts once nothing needs K1");
    let session = human_login(
        &replica_k2,
        &keycloak,
        CONFIDENTIAL_HUMAN_CLIENT,
        "alice",
        "alice-password",
    )
    .await;
    assert_v1_authz_check_ok(
        &replica_k2,
        session["access_token"].as_str().expect("access token"),
        "rotation-k2-only",
    )
    .await;

    // 7. A same-issuer secret rotation on B survives a failed recovery
    //    decision append unchanged, then is served by the K2-only replica
    //    without restart.
    let (_, before) = call_json(&replica_b, token, Method::GET, CONNECTIONS, None).await;
    let (status, staged) = call_json(
        &replica_b,
        token,
        Method::PUT,
        CANDIDATE,
        Some(connection_input(
            CONFIDENTIAL_HUMAN_CLIENT,
            "SecretPost",
            Some(CONFIDENTIAL_HUMAN_SECRET),
            before["candidate"]["revision"].as_u64(),
        )),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "B stages the rotation: {staged}");
    let revision = staged["revision"].as_u64().expect("candidate revision");
    let (status, tested) = call_json(
        &replica_b,
        token,
        Method::POST,
        CANDIDATE_TEST,
        Some(serde_json::json!({ "expected_revision": revision })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "the rotation tests: {tested}");
    let recovery_id = recovery_admin.id().as_uuid();
    // `recovery_id` is a UUID, so interpolating it cannot inject SQL.
    sqlx::query(sqlx::AssertSqlSafe(format!(
        r#"CREATE OR REPLACE FUNCTION vala.test_fail_recovery_decision_audit()
           RETURNS trigger LANGUAGE plpgsql AS $$
           BEGIN
             IF NEW.operation = 'identity.oidc.candidate.activate'
                AND NEW.principal_id = '{recovery_id}'::uuid THEN
               RAISE EXCEPTION 'injected recovery decision audit failure';
             END IF;
             RETURN NEW;
           END;
           $$;"#
    )))
    .execute(&superuser)
    .await
    .expect("recovery failure function installs");
    sqlx::query(
        r#"CREATE TRIGGER test_fail_recovery_decision_audit
           BEFORE INSERT ON vala.audit_staging
           FOR EACH ROW EXECUTE FUNCTION vala.test_fail_recovery_decision_audit()"#,
    )
    .execute(&superuser)
    .await
    .expect("recovery failure trigger installs");
    let (status, body) = call_json(
        &replica_b,
        token,
        Method::POST,
        CANDIDATE_ACTIVATE,
        Some(activation(revision, &recovery_key)),
    )
    .await;
    assert!(
        status.is_server_error(),
        "activation without a durable recovery decision fails closed: {status} {body}"
    );
    sqlx::query("DROP TRIGGER test_fail_recovery_decision_audit ON vala.audit_staging")
        .execute(&superuser)
        .await
        .expect("recovery failure trigger drops");
    let (_, after) = call_json(&replica_b, token, Method::GET, CONNECTIONS, None).await;
    assert_eq!(
        after["active"]["id"], before["active"]["id"],
        "the Active was not retired: {after}"
    );
    assert_eq!(
        after["active"]["revision"], before["active"]["revision"],
        "{after}"
    );
    assert_eq!(
        after["candidate"]["revision"], revision,
        "the Candidate was not promoted: {after}"
    );
    let (status, rotated) = call_json(
        &replica_b,
        token,
        Method::POST,
        CANDIDATE_ACTIVATE,
        Some(activation(revision, &recovery_key)),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "the rotation activates: {rotated}");
    let (_, listed) = call_json(&replica_k2, token, Method::GET, CONNECTIONS, None).await;
    assert_eq!(
        listed["active"]["id"], rotated["id"],
        "the K2-only replica serves B's activation"
    );
    assert_eq!(listed["active"]["revision"], 5);
    let session = human_login(
        &replica_k2,
        &keycloak,
        CONFIDENTIAL_HUMAN_CLIENT,
        "alice",
        "alice-password",
    )
    .await;
    assert!(
        session["access_token"].is_string(),
        "the K2-only replica signs in through the rotated connection"
    );
    let (status, body) = call_json(&replica_b, token, Method::POST, ACTIVE_DEACTIVATE, None).await;
    assert_eq!(status, StatusCode::NO_CONTENT, "B deactivates: {body}");
    let (status, body) = login_status(&replica_k2).await;
    assert_refused(
        status,
        &body,
        StatusCode::UNAUTHORIZED,
        "WYRD_AUTH_401_INVALID_TOKEN",
    );

    // 8. Each recovery decision is staged on the canonical audit path,
    //    attributed to the recovery principal and its verified credential;
    //    the malformed key left none. In-process replicas run no publisher,
    //    so every committed decision is still in staging.
    let recovery_decisions = sqlx::query_as::<_, (String, Option<String>, String, String)>(
        "SELECT principal_id::text, credential_id::text, permission, outcome \
         FROM vala.audit_staging \
         WHERE data_tenant_id = $1 AND operation = 'identity.oidc.candidate.activate' \
         ORDER BY seq",
    )
    .bind(tenant.as_uuid())
    .fetch_all(&superuser)
    .await
    .expect("staged activation decisions read");
    let admin_id = principal_id_of(token);
    let runtime_id = runtime_admin.id().as_uuid().to_string();
    let recovery_id = recovery_id.to_string();
    let decisions_of = |principal: &str| -> Vec<(Option<String>, String, String)> {
        recovery_decisions
            .iter()
            .filter(|(id, ..)| id == principal)
            .map(|(_, credential, permission, outcome)| {
                (credential.clone(), permission.clone(), outcome.clone())
            })
            .collect()
    };
    let write = "identity_connections:write".to_owned();
    assert_eq!(
        decisions_of(&runtime_id),
        vec![(
            Some(api_key_id(&superuser, &runtime_admin).await),
            write.clone(),
            "denied".to_owned()
        )],
        "the underprivileged recovery key is audited as Denied: {recovery_decisions:?}"
    );
    let allowed = (
        Some(api_key_id(&superuser, &recovery_admin).await),
        write,
        "allowed".to_owned(),
    );
    assert_eq!(
        decisions_of(&recovery_id),
        vec![allowed.clone(), allowed],
        "each successful activation audits its recovery principal once: \
         {recovery_decisions:?}"
    );
    assert!(
        recovery_decisions
            .iter()
            .all(|(id, ..)| [&admin_id, &runtime_id, &recovery_id].contains(&id)),
        "no decision is attributed to an unresolved key: {recovery_decisions:?}"
    );

    replica_k2
        .shutdown()
        .await
        .expect("K2-only replica shuts down");
    replica_b.shutdown().await.expect("replica B shuts down");
}

/// Read the id of `principal`'s one API key — the credential id its
/// verified recovery decisions are attributed to — as superuser.
///
/// # Panics
/// Panics when the read fails or the principal has no single key.
async fn api_key_id(superuser: &sqlx::PgPool, principal: &Bootstrap) -> String {
    sqlx::query_scalar::<_, String>(
        "SELECT id::text FROM wyrd.auth_api_keys WHERE principal_id = $1",
    )
    .bind(principal.id().as_uuid())
    .fetch_one(superuser)
    .await
    .expect("the principal's key id reads")
}

/// Count the refresh rows ever issued to `principal_id`, read as superuser so
/// the count sees every tenant row regardless of session state.
///
/// # Panics
/// Panics when the superuser pool or the read fails.
async fn refresh_rows(srv: &WyrdTestServer, principal_id: &str) -> i64 {
    let superuser = srv
        .pg_fixture()
        .superuser_pool()
        .await
        .expect("superuser pool opens");
    sqlx::query_scalar(
        "SELECT count(*) FROM wyrd.auth_refresh_tokens WHERE principal_id = $1::uuid",
    )
    .bind(principal_id)
    .fetch_one(&superuser)
    .await
    .expect("refresh rows read")
}

/// Assert `refresh_token` no longer renews on `srv` and that the refusal
/// inserted no successor for its principal.
///
/// # Panics
/// Panics when the refresh is not `401 WYRD_AUTH_401_REFRESH_REVOKED`, it
/// returns any token, or a refresh row was added.
async fn assert_refresh_cut_off(srv: &WyrdTestServer, session: &Value, label: &str) {
    let principal = principal_id_of(session["access_token"].as_str().expect("access token"));
    let before = refresh_rows(srv, &principal).await;
    let (status, body) = post_refresh(
        srv,
        session["refresh_token"].as_str().expect("refresh token"),
    )
    .await;
    assert_refused(
        status,
        &body,
        StatusCode::UNAUTHORIZED,
        "WYRD_AUTH_401_REFRESH_REVOKED",
    );
    assert!(
        body.get("refresh_token").is_none() && body.get("access_token").is_none(),
        "{label}: a stale session gets no successor: {body}"
    );
    assert_eq!(
        refresh_rows(srv, &principal).await,
        before,
        "{label}: the refusal inserted no refresh row"
    );
}

/// Human sessions end with the connection revision they signed in through,
/// across replicas.
///
/// Replica A signs alice in; replica B then changes the tenant's connection,
/// and A must refuse to renew the old session without minting a successor:
///   1. a session renews on B while its connection is unchanged;
///   2. B replaces the Active connection, and A refuses the old session;
///   3. B deactivates, and A refuses the session minted by the replacement;
///   4. B removes a freshly activated connection, and A refuses its session;
///   5. a callback paused after provider authentication — held before
///      issuance by a lock on the user-role table — fails once B's
///      deactivation commits, and inserts no refresh row.
///
/// # Panics
/// Panics when any step deviates from the contract above.
#[tokio::test]
#[ignore = "requires the Keycloak and Dex identity lane"]
async fn tenant_connection_session_cutoff_journey() {
    let replica_a = human_server_builder()
        .start_in_process()
        .await
        .expect("replica A starts");
    let replica_b = replica_a
        .start_replica(human_server_builder())
        .await
        .expect("replica B starts");
    let tenant = replica_a.data_tenant_id();
    let admin = tenant_admin(&replica_a, tenant, "cutoff-admin").await;
    let token = admin.token.as_str();
    let keycloak = OidcIssuerFixture::connect(&keycloak_issuer()).await;
    let sign_in = || {
        human_login(
            &replica_a,
            &keycloak,
            PUBLIC_HUMAN_CLIENT,
            "alice",
            "alice-password",
        )
    };

    // 1. An unchanged connection renews on the other replica.
    activate_keycloak_connection(&replica_b, &admin, PUBLIC_HUMAN_CLIENT, "Public", None).await;
    let first = sign_in().await;
    let (status, renewed) = post_refresh(
        &replica_b,
        first["refresh_token"].as_str().expect("refresh token"),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "B renews an A session: {renewed}");

    // 2. Replacement on B cuts off the renewed session on A.
    activate_keycloak_connection(&replica_b, &admin, PUBLIC_HUMAN_CLIENT, "Public", None).await;
    assert_refresh_cut_off(&replica_a, &renewed, "replacement").await;

    // 3. Deactivation on B cuts off the replacement's session on A.
    let second = sign_in().await;
    let (status, body) = call_json(&replica_b, token, Method::POST, ACTIVE_DEACTIVATE, None).await;
    assert_eq!(status, StatusCode::NO_CONTENT, "B deactivates: {body}");
    assert_refresh_cut_off(&replica_a, &second, "deactivation").await;

    // 4. Removal on B cuts off the removed connection's session on A.
    let active =
        activate_keycloak_connection(&replica_b, &admin, PUBLIC_HUMAN_CLIENT, "Public", None).await;
    let third = sign_in().await;
    let id = active["id"].as_str().expect("active connection id");
    let (status, body) = call_json(
        &replica_b,
        token,
        Method::DELETE,
        &format!("{CONNECTIONS}/{id}"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT, "B removes: {body}");
    assert_refresh_cut_off(&replica_a, &third, "removal").await;

    // 5. A callback paused after provider IO fails once deactivation commits.
    activate_keycloak_connection(&replica_b, &admin, PUBLIC_HUMAN_CLIENT, "Public", None).await;
    let principal = principal_id_of(
        sign_in().await["access_token"]
            .as_str()
            .expect("access token"),
    );
    let before = refresh_rows(&replica_a, &principal).await;
    let (code, state) = authorization_code(
        &replica_a,
        &keycloak,
        PUBLIC_HUMAN_CLIENT,
        "alice",
        "alice-password",
    )
    .await;
    let superuser = replica_a
        .pg_fixture()
        .superuser_pool()
        .await
        .expect("superuser pool opens");
    let mut hold = superuser.begin().await.expect("hold transaction begins");
    sqlx::query("LOCK TABLE wyrd.auth_user_roles IN EXCLUSIVE MODE")
        .execute(&mut *hold)
        .await
        .expect("user-role table locks");
    let release = async {
        let deadline = tokio::time::Instant::now() + StdDuration::from_secs(30);
        loop {
            let waiting: i64 = sqlx::query_scalar(
                "SELECT count(*) FROM pg_locks l JOIN pg_class c ON c.oid = l.relation \
                  WHERE c.relname = 'auth_user_roles' AND NOT l.granted",
            )
            .fetch_one(&superuser)
            .await
            .expect("lock waiters read");
            if waiting > 0 {
                break;
            }
            assert!(
                tokio::time::Instant::now() < deadline,
                "the callback never reached issuance"
            );
            tokio::time::sleep(StdDuration::from_millis(20)).await;
        }
        let (status, body) =
            call_json(&replica_b, token, Method::POST, ACTIVE_DEACTIVATE, None).await;
        assert_eq!(
            status,
            StatusCode::NO_CONTENT,
            "B deactivates mid-callback: {body}"
        );
        hold.commit().await.expect("hold releases");
    };
    let ((status, body), ()) = tokio::join!(finish_callback(&replica_a, &code, &state), release);
    assert_refused(
        status,
        &body,
        StatusCode::UNAUTHORIZED,
        "WYRD_AUTH_401_INVALID_TOKEN",
    );
    assert_eq!(
        refresh_rows(&replica_a, &principal).await,
        before,
        "the in-flight callback issued no session"
    );

    replica_b.shutdown().await.expect("replica B shuts down");
    replica_a.shutdown().await.expect("replica A shuts down");
}

// ─── Federated cloud journey: CLI-authored issuer + binding ───────────────────

/// Drive the full federated workload credential lifecycle through the REAL
/// operator path — `wyrd auth trusted-issuer add` and `wyrd auth
/// workload-binding add` over HTTP into `/admin` → Postgres — then exchange a
/// live Keycloak assertion and reach `/v1`:
///   1. an admin SA (`runtime_admin`) mints the access token the CLI presents,
///   2. the trusted issuer is authored via the CLI with a `SecretPost` client
///      secret, which must be sealed to ciphertext at rest (never plaintext),
///   3. the `(issuer, subject)` binding is authored via the CLI, resolving to a
///      server-owned Service card a principal is seeded under,
///   4. `POST /auth/token {jwt-bearer}` exchanges the Keycloak assertion → 200,
///   5. the exchanged Service token reaches a real `/v1/authz/check` 200,
///   6. the production `wyrd-client` middleware caches one exchange, reuses the
///      cached token while fresh, and re-exchanges on `force_refresh`.
///
/// Unlike the config-driven `workload_jwt_bearer_journey_keycloak`, every trust
/// record here is authored post-boot through the CLI the operator runs, proving
/// the `PgIssuerResolver`/`PgWorkloadBindingResolver` serve runtime-authored
/// rows immediately.
///
/// # Panics
/// Panics when the bound server, admin bootstrap, principal seeding, or either
/// CLI verb fails, the stored client secret is missing, empty, or equal to the
/// plaintext, the jwt-bearer exchange is not `200`, the exchanged token does not
/// reach `200`, the client middleware lifecycle assertions fail, or shutdown
/// fails.
#[tokio::test]
#[ignore = "requires the Keycloak and Dex identity lane"]
async fn federated_cloud_journey_cli_authored_keycloak() {
    // Real socket: the `wyrd auth` CLI authors through HTTP against `/admin`.
    let srv = WyrdTestServerBuilder::default()
        .start_bound()
        .await
        .expect("bound server starts");
    let server_url: Url = srv
        .base_url()
        .expect("bound server exposes a base URL")
        .parse()
        .expect("base URL parses");

    // Foreign OIDC assertion from Keycloak; bind it to its real subject.
    let keycloak = OidcIssuerFixture::connect(&keycloak_issuer()).await;
    let assertion = keycloak
        .workload_token("wyrd-workload", "wyrd-workload-secret", "wyrd-workload")
        .await;
    let subject = jwt_claims(&assertion)["sub"]
        .as_str()
        .expect("workload token carries a subject")
        .to_owned();

    // Admin SA holding runtime_admin → access token used as the CLI bearer.
    let admin = srv
        .bootstrap_service("cloud-journey-admin", &["runtime_admin"])
        .await
        .expect("admin service account bootstraps");
    let admin_token = srv
        .exchange_api_key(admin.api_key().expect("admin has api key"))
        .await
        .expect("admin api key exchange succeeds");

    // The server-owned card the binding resolves to; seed its principal.
    let card_ref = binding_card_ref(CardKind::Service, "cloud-journey-sa", "prod");
    srv.seed_card_principal(&card_ref, &["writer"])
        .await
        .expect("workload principal seeds");

    // Author the trusted issuer THROUGH the real CLI verb, with a SecretPost
    // client secret that must round-trip to ciphertext at rest.
    let client_secret = "cli-authored-issuer-secret";
    set_cli_environment("WYRD_ACCESS_TOKEN", &admin_token);
    set_cli_environment("WYRD_ISSUER_CLIENT_SECRET", client_secret);
    trusted_issuer::dispatch(TrustedIssuerCommand::Add(Box::new(TrustedIssuerAddArgs {
        issuer: keycloak_issuer(),
        expected_audience: "wyrd-workload".to_owned(),
        client_id: "wyrd-workload".to_owned(),
        client_auth: "SecretPost".to_owned(),
        client_secret_file: None,
        claim_subject: "sub".to_owned(),
        claim_email: None,
        claim_groups: None,
        default_roles: Vec::new(),
        group_roles: Vec::new(),
        principal_kind: "Workload".to_owned(),
        jwks_ttl_secs: Some(300),
        server: server_url.clone(),
    })))
    .await
    .expect("CLI trusted-issuer add succeeds");

    // Author the workload binding THROUGH the real CLI verb.
    workload_binding::dispatch(WorkloadBindingCommand::Add(WorkloadBindingAddArgs {
        issuer: keycloak_issuer(),
        subject: subject.clone(),
        audience: Some("wyrd-workload".to_owned()),
        card: card_ref.to_string(),
        server: server_url.clone(),
    }))
    .await
    .expect("CLI workload-binding add succeeds");

    // Ciphertext-at-rest: the stored secret is sealed, never the plaintext.
    let ciphertext = srv
        .trusted_issuer_secret_ciphertext(srv.data_tenant_id(), &keycloak_issuer())
        .await
        .expect("ciphertext read succeeds")
        .expect("CLI-authored issuer stores a sealed client secret");
    assert!(
        !ciphertext.is_empty(),
        "stored client secret ciphertext is non-empty"
    );
    assert_ne!(
        ciphertext.as_slice(),
        client_secret.as_bytes(),
        "stored client secret is sealed ciphertext, not plaintext"
    );

    // jwt-bearer exchange against the CLI-authored issuer+binding → 200.
    let resp = post_jwt_bearer(&srv, &assertion).await;
    assert_eq!(
        resp.status(),
        StatusCode::OK,
        "CLI-authored workload jwt-bearer exchange returns 200: {}",
        resp.status()
    );
    let bytes = to_bytes(resp.into_body(), 65_536)
        .await
        .expect("token body reads");
    let token_body: Value = serde_json::from_slice(&bytes).expect("token response is JSON");
    let wyrd_token = token_body["access_token"]
        .as_str()
        .expect("access_token present");

    // Terminal: the exchanged Service token reaches a real /v1/authz/check 200.
    assert_v1_authz_check_ok(&srv, wyrd_token, "cloud-journey").await;

    // Client lifecycle: the production wyrd-client middleware caches one
    // exchange, reuses it while fresh, and re-exchanges on force_refresh.
    assert_client_workload_lifecycle(&srv, &assertion).await;

    srv.shutdown().await.expect("server shuts down cleanly");
}

/// Assert the production `wyrd-client` workload-credential lifecycle end-to-end
/// against the live bound server: one cached exchange, reuse while fresh, and a
/// fresh re-exchange on `force_refresh` — the exact path a workload's transport
/// drives at startup and on a reactive `401`.
async fn assert_client_workload_lifecycle(srv: &WyrdTestServer, assertion: &str) {
    // Connect via `localhost` (not the bound `127.0.0.1`): the workload
    // exchange resolves the tenant from the request host first, and an IP host
    // is misread as a subdomain; a single-label `localhost` host yields no host
    // tenant, so the route falls back to the body `tenant` slug the
    // `WorkloadJwt` credential carries — the path this credential exists to
    // drive. Both names reach the same loopback listener.
    let base_url = srv
        .base_url()
        .expect("bound server exposes a base URL")
        .replace("127.0.0.1", "localhost");
    let config = ClientConfig {
        http: HttpConfig {
            base_url,
            ..HttpConfig::default()
        },
        token_cache: TokenCacheMode::InMemory,
        ..ClientConfig::default()
    };

    let middleware = AuthMiddleware::new(
        &config,
        ResolvedCredential::WorkloadJwt {
            jwt: SecretString::from(assertion.to_owned()),
            tenant: FIXTURE_TENANT_SLUG.to_owned(),
        },
    )
    .expect("workload auth middleware builds");

    // First bearer() exchanges the assertion once; the second returns the
    // cached token unchanged (a single /auth/token round-trip while fresh).
    let first = middleware
        .bearer()
        .await
        .expect("first workload exchange succeeds");
    let second = middleware.bearer().await.expect("cached bearer succeeds");
    assert_eq!(
        first.expose(),
        second.expose(),
        "cached workload token is reused while fresh (single exchange)"
    );

    // force_refresh re-exchanges, yielding a fresh, well-formed Wyrd token.
    let refreshed = middleware
        .force_refresh()
        .await
        .expect("force_refresh re-exchanges the assertion");
    assert!(
        !refreshed.expose().is_empty(),
        "refreshed token is non-empty"
    );
    assert_eq!(
        refreshed.expose().split('.').count(),
        3,
        "refreshed token is a 3-segment JWT"
    );

    // The exchanged token is a real Wyrd access token usable on /v1.
    assert_v1_authz_check_ok(srv, refreshed.expose(), "cloud-journey-client").await;
}

// ─── Same-issuer two-tenant isolation ─────────────────────────────────────────

/// Prove RLS-scoped binding isolation: the SAME issuer URL authored in two
/// tenants, with a subject bound only in tenant A, resolves in A and fails
/// closed in B.
///   1. tenant A is the fixture tenant; tenant B is provisioned fresh,
///   2. per-tenant `runtime_admin` admins each author the same issuer via the
///      real CLI (so the issuer is trusted in BOTH tenants),
///   3. the `(issuer, subject)` binding is authored via the CLI only in A, and
///      its principal is seeded only in A,
///   4. a storage cross-check drives TWO distinct `TenantConn`s (RLS, not a
///      shared filter): the binding is visible under A, invisible under B,
///   5. the same Keycloak assertion resolves at tenant A's slug → 200, and at
///      tenant B's slug fails closed with `WYRD_AUTH_404_PRINCIPAL_NOT_FOUND`
///      (issuer trusted, binding absent).
///
/// # Panics
/// Panics when tenant B, either admin, or the tenant-A principal fails to
/// provision, a CLI verb fails, the binding is not visible under A or is
/// visible under B, tenant A's exchange is not `200`, or tenant B's exchange
/// does not fail closed with `WYRD_AUTH_404_PRINCIPAL_NOT_FOUND`.
#[tokio::test]
#[ignore = "requires the Keycloak and Dex identity lane"]
async fn same_issuer_two_tenant_isolation_keycloak() {
    let srv = WyrdTestServerBuilder::default()
        .start_bound()
        .await
        .expect("bound server starts");
    let server_url: Url = srv
        .base_url()
        .expect("bound server exposes a base URL")
        .parse()
        .expect("base URL parses");

    // Tenant A is the fixture tenant; tenant B is provisioned fresh.
    const TENANT_A_SLUG: &str = FIXTURE_TENANT_SLUG;
    const TENANT_B_SLUG: &str = "test-tenant-2";
    let tenant_a = srv.data_tenant_id();
    let tenant_b = srv
        .seed_tenant(TENANT_B_SLUG)
        .await
        .expect("tenant B provisions");

    // The foreign assertion both tenants independently present.
    let keycloak = OidcIssuerFixture::connect(&keycloak_issuer()).await;
    let assertion = keycloak
        .workload_token("wyrd-workload", "wyrd-workload-secret", "wyrd-workload")
        .await;
    let subject = jwt_claims(&assertion)["sub"]
        .as_str()
        .expect("workload token carries a subject")
        .to_owned();

    // Per-tenant runtime_admin admins, each minted under its own tenant so its
    // access token authors into that tenant.
    let admin_a = srv
        .bootstrap_service_in_tenant(tenant_a, "iso-admin-a", &["runtime_admin"])
        .await
        .expect("tenant A admin bootstraps");
    let admin_token_a = srv
        .exchange_api_key(admin_a.api_key().expect("admin A has api key"))
        .await
        .expect("admin A api key exchange succeeds");
    let admin_b = srv
        .bootstrap_service_in_tenant(tenant_b, "iso-admin-b", &["runtime_admin"])
        .await
        .expect("tenant B admin bootstraps");
    let admin_token_b = srv
        .exchange_api_key(admin_b.api_key().expect("admin B has api key"))
        .await
        .expect("admin B api key exchange succeeds");

    // Author the SAME issuer URL in BOTH tenants via the real CLI.
    for token in [&admin_token_a, &admin_token_b] {
        set_cli_environment("WYRD_ACCESS_TOKEN", token);
        trusted_issuer::dispatch(TrustedIssuerCommand::Add(Box::new(TrustedIssuerAddArgs {
            issuer: keycloak_issuer(),
            expected_audience: "wyrd-workload".to_owned(),
            client_id: "wyrd-workload".to_owned(),
            client_auth: "Public".to_owned(),
            client_secret_file: None,
            claim_subject: "sub".to_owned(),
            claim_email: None,
            claim_groups: None,
            default_roles: Vec::new(),
            group_roles: Vec::new(),
            principal_kind: "Workload".to_owned(),
            jwks_ttl_secs: Some(300),
            server: server_url.clone(),
        })))
        .await
        .expect("CLI trusted-issuer add succeeds");
    }

    // Bind the subject ONLY in tenant A (CLI), and seed its principal in A only.
    let card_ref = binding_card_ref(CardKind::Service, "iso-sa", "prod");
    set_cli_environment("WYRD_ACCESS_TOKEN", &admin_token_a);
    workload_binding::dispatch(WorkloadBindingCommand::Add(WorkloadBindingAddArgs {
        issuer: keycloak_issuer(),
        subject: subject.clone(),
        audience: Some("wyrd-workload".to_owned()),
        card: card_ref.to_string(),
        server: server_url.clone(),
    }))
    .await
    .expect("CLI workload-binding add (tenant A) succeeds");
    srv.seed_card_principal_in_tenant(tenant_a, &card_ref, &["runtime_admin"])
        .await
        .expect("tenant A workload principal seeds");

    // Storage cross-check via TWO distinct tenant connections (RLS, not a
    // shared filter): the binding is visible under A, absent under B.
    assert!(
        srv.workload_binding_exists(
            tenant_a,
            &keycloak_issuer(),
            &subject,
            Some("wyrd-workload")
        )
        .await
        .expect("tenant A binding lookup succeeds"),
        "binding is visible under tenant A's RLS scope"
    );
    assert!(
        !srv.workload_binding_exists(
            tenant_b,
            &keycloak_issuer(),
            &subject,
            Some("wyrd-workload")
        )
        .await
        .expect("tenant B binding lookup succeeds"),
        "binding is invisible under tenant B's RLS scope"
    );

    // Resolve at tenant A's slug → 200.
    let resp_a = post_jwt_bearer_for_tenant(&srv, &assertion, TENANT_A_SLUG).await;
    assert_eq!(
        resp_a.status(),
        StatusCode::OK,
        "tenant A resolves the bound subject: {}",
        resp_a.status()
    );

    // The SAME subject token at tenant B's slug fails closed (issuer trusted,
    // binding absent under B).
    let resp_b = post_jwt_bearer_for_tenant(&srv, &assertion, TENANT_B_SLUG).await;
    assert_eq!(
        resp_b.status(),
        StatusCode::NOT_FOUND,
        "tenant B fails closed for the unbound subject: {}",
        resp_b.status()
    );
    let bytes = to_bytes(resp_b.into_body(), 65_536)
        .await
        .expect("body reads");
    let body: Value = serde_json::from_slice(&bytes).unwrap_or_default();
    assert_eq!(
        response_code(&body),
        "WYRD_AUTH_404_PRINCIPAL_NOT_FOUND",
        "tenant B returns principal-not-found; body={body}"
    );

    srv.shutdown().await.expect("server shuts down cleanly");
}

// ─── Conformance: server route error codes ────────────────────────────────────

/// A jwt-bearer exchange against a server with no trusted external issuer fails
/// closed with a 401-family error code.
#[tokio::test]
#[ignore = "requires the Keycloak and Dex identity lane"]
async fn conformance_untrusted_issuer_rejected() {
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
#[ignore = "requires the Keycloak and Dex identity lane"]
async fn conformance_login_rejects_bare_localhost_host() {
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
#[ignore = "requires the Keycloak and Dex identity lane"]
async fn key_rotation_keycloak_admin_api() {
    let keycloak = OidcIssuerFixture::connect(&keycloak_issuer())
        .await
        .with_keycloak_admin(keycloak_admin());

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

/// Set one environment source the in-process CLI verbs read.
///
/// The CLI takes no secret as an argument: its tenant credential comes from
/// the ambient chain and the issuer client secret from the environment, so an
/// in-process verb is authenticated exactly as an operator's shell would.
fn set_cli_environment(name: &str, value: &str) {
    // SAFETY: nextest runs each test in its own process and this lane runs a
    // single test thread, so nothing else reads the environment while it
    // changes.
    unsafe { env::set_var(name, value) };
}
