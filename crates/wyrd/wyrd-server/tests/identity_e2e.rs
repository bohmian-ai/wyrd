use std::collections::HashMap;
use std::env;
use std::time::Duration as StdDuration;

use axum::body::{Body, to_bytes};
use axum::http::{Method, Request, StatusCode, header};
use base64::Engine as _;
use chrono::{DateTime, Duration as ChronoDuration, Utc};
use oauth2::TokenResponse as _;
use secrecy::{ExposeSecret as _, SecretString};
use serde_json::Value;
use sqlx::PgPool;
use url::Url;
use uuid::Uuid;
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};
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
    IssueKeyRequest, IssueKeyResponse, IssuerTokenPolicy, IssuerUrl, PrincipalId, Sha256Hex,
    TokenAudience,
};
use wyrd_spec::envelope::CardKind;
use wyrd_spec::error::WyrdError;
use wyrd_spec::ids::{CardName, SpaceName};
use wyrd_spec::reference::CardRef;
use wyrd_testing::{
    Bootstrap, KeycloakAdmin, OidcIssuerFixture, WyrdTestServer, WyrdTestServerBuilder,
    provider_sign_in,
};

fn keycloak_issuer() -> String {
    env::var("WYRD_KEYCLOAK_ISSUER")
        .unwrap_or_else(|_| "http://localhost:8180/realms/wyrd-test".to_owned())
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
        base_url: "http://localhost:8180".to_owned(),
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
/// the form's `tenant` slug. The isolation test drives the same assertion at
/// two distinct tenant slugs to prove binding resolution is tenant-scoped.
async fn post_jwt_bearer_for_tenant(
    srv: &WyrdTestServer,
    assertion: &str,
    tenant: &str,
) -> axum::http::Response<Body> {
    let body = url::form_urlencoded::Serializer::new(String::new())
        .append_pair("grant_type", "urn:ietf:params:oauth:grant-type:jwt-bearer")
        .append_pair("assertion", assertion)
        .append_pair("tenant", tenant)
        .finish();
    srv.oneshot(
        Request::builder()
            .method(Method::POST)
            .uri("/auth/token")
            .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
            .body(Body::from(body))
            .expect("request builds"),
    )
    .await
    .expect("jwt-bearer call completes")
}

/// Register a fresh `probe` Service Card as `jwt` and return the HTTP status.
///
/// Card registration is a `card_write`-guarded Wyrd API write, so the status
/// is the server's own permission decision for the token: `201` when the
/// token carries `card_write`, `403` when it does not, and `401` when the token
/// no longer verifies.
///
/// # Panics
/// Panics when the request cannot be built or the route does not respond.
async fn card_write_status(srv: &WyrdTestServer, jwt: &str, label: &str) -> StatusCode {
    let name = format!("{label}-probe-{}", Uuid::new_v4().simple());
    let body = serde_json::json!({ "submissions": [{
        "apiVersion": "wyrd/v1", "kind": "Service",
        "metadata": { "name": name, "version": "1.0.0", "space": "probe" },
        "spec": {},
        "artifacts": []
    }] });
    srv.oneshot_authenticated(
        jwt,
        Request::builder()
            .method(Method::POST)
            .uri("/v1/cards")
            .header(header::CONTENT_TYPE, "application/json")
            .header("Idempotency-Key", name)
            .body(Body::from(body.to_string()))
            .expect("probe registration request builds"),
    )
    .await
    .expect("probe registration responds")
    .status()
}

/// Exchange `subject_jwt` through a fresh `writer` actor into a delegated token.
///
/// Returns the delegated token and the actor. The actor's API-key exchange is
/// a qualifying machine exchange, so it is the one runtime activation this
/// helper causes.
///
/// # Panics
/// Panics when bootstrap, key exchange, or delegation fails.
async fn delegate_through_writer(
    srv: &WyrdTestServer,
    subject_jwt: &str,
    label: &str,
) -> (String, Bootstrap) {
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
    (delegated, actor)
}

/// Drive a delegated token to a real authorized Wyrd API write.
///
/// `subject_jwt` is the token of a principal holding `writer`. A freshly
/// seeded `writer` service exchanges it as the RFC 8693 actor, then registers
/// a Card with the delegated token, which Wyrd authorizes only when
/// `card_write` survives the subject/actor intersection.
///
/// Returns the actor's principal id, so a journey asserting on activity names
/// the one activation this helper causes.
///
/// # Panics
/// Panics when bootstrap, exchange, or delegation fails, or the write is not
/// `201`.
async fn assert_v1_delegated_write_ok(
    srv: &WyrdTestServer,
    subject_jwt: &str,
    label: &str,
) -> PrincipalId {
    let (delegated, actor) = delegate_through_writer(srv, subject_jwt, label).await;
    assert_eq!(
        card_write_status(srv, &delegated, label).await,
        StatusCode::CREATED,
        "{label}: the subject/actor intersection allows card_write"
    );
    actor.id()
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
        meta.authorization_endpoint()
            .as_str()
            .contains("openid-connect/auth"),
        "Keycloak authorization_endpoint: {}",
        meta.authorization_endpoint().as_str()
    );
    assert!(
        meta.token_endpoint()
            .is_some_and(|u| u.as_str().contains("openid-connect/token")),
        "Keycloak token_endpoint missing or unexpected"
    );
    assert!(
        meta.jwks_uri().as_str().contains("openid-connect/certs"),
        "Keycloak jwks_uri: {}",
        meta.jwks_uri().as_str()
    );
}

#[tokio::test]
#[ignore = "requires the Keycloak and Dex identity lane"]
async fn discovery_resolves_dex() {
    let fixture = OidcIssuerFixture::connect(&dex_issuer()).await;
    let meta = fixture.metadata();
    assert!(
        meta.jwks_uri().as_str().contains("/keys"),
        "Dex jwks_uri: {}",
        meta.jwks_uri().as_str()
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
    let jwks = reqwest::get(fixture.metadata().jwks_uri().url().clone())
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
///   4. The Service token delegates and reaches a real `/v1/cards` `200`.
///
/// # Panics
/// Panics when the Keycloak token has no `sub`, the server fails to boot or
/// seed the bound principal, the jwt-bearer exchange is not `200`, the grant
/// omits `access_token` or issues a refresh token, or the delegated
/// write does not return `201`.
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

    assert_v1_delegated_write_ok(&srv, wyrd_token, "workload").await;
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

    assert_jwt_bearer_refused(
        post_jwt_bearer(&srv, &assertion).await,
        "unbound workload subject",
    )
    .await;
}

/// The tenant's machine principals that record runtime activity, sorted.
///
/// Activity is written only by a qualifying API-key or workload `jwt-bearer`
/// exchange for a Card-bound Service or Agent, so naming the exact set — not a
/// bare count — is what proves an excluded path recorded nothing: a grant that
/// wrongly activated would add an id the caller never exchanged for.
///
/// # Panics
/// Panics when the tenant connection or the query fails.
async fn activated_principals(srv: &WyrdTestServer) -> Vec<Uuid> {
    let mut conn = srv
        .tenant_conn_for(srv.data_tenant_id())
        .await
        .expect("tenant connection opens");
    let mut ids: Vec<Uuid> = sqlx::query_scalar(
        "SELECT id FROM wyrd.auth_service_accounts WHERE last_authenticated_at IS NOT NULL",
    )
    .fetch_all(&mut **conn.transaction())
    .await
    .expect("activity ids read");
    conn.commit().await.expect("assertion transaction commits");
    ids.sort_unstable();
    ids
}

/// Register an empty Service `name` at 1.0.0 in `space` through the public route.
///
/// Registration projects the Card-bound principal a workload binding selects;
/// returns the Service's registered UID.
///
/// # Panics
/// Panics when the route fails to respond, refuses the Card, or returns no UID.
async fn register_service(srv: &WyrdTestServer, jwt: &str, name: &str, space: &str) -> Uuid {
    let body = serde_json::json!({ "submissions": [{
        "apiVersion": "wyrd/v1", "kind": "Service",
        "metadata": { "name": name, "version": "1.0.0", "space": space },
        "spec": {},
        "artifacts": []
    }] });
    let response = srv
        .oneshot_authenticated(
            jwt,
            Request::builder()
                .method(Method::POST)
                .uri("/v1/cards")
                .header(header::CONTENT_TYPE, "application/json")
                .header("Idempotency-Key", format!("{name}-{space}"))
                .body(Body::from(body.to_string()))
                .expect("registration request builds"),
        )
        .await
        .expect("registration responds");
    let status = response.status();
    let bytes = to_bytes(response.into_body(), 65_536)
        .await
        .expect("registration body reads");
    let body: Value = serde_json::from_slice(&bytes).expect("registration body is JSON");
    assert_eq!(status, StatusCode::CREATED, "{body}");
    uuid::Uuid::parse_str(
        body["outcomes"][0]["card_ref"]["uid"]
            .as_str()
            .expect("Service outcome UID exists"),
    )
    .expect("Service UID parses")
}

/// The last qualifying exchange of the principal the Service `owner` projects.
///
/// # Panics
/// Panics when the read fails or the owner projects no principal.
async fn owner_last_authenticated(srv: &WyrdTestServer, owner: Uuid) -> Option<DateTime<Utc>> {
    let mut conn = srv
        .tenant_conn_for(srv.data_tenant_id())
        .await
        .expect("tenant connection opens");
    let seen = sqlx::query_scalar(
        "SELECT last_authenticated_at FROM wyrd.auth_service_accounts WHERE card_uid = $1",
    )
    .bind(owner)
    .fetch_one(&mut **conn.transaction())
    .await
    .expect("owner principal reads");
    conn.commit().await.expect("assertion transaction commits");
    seen
}

/// A configured workload `jwt-bearer` exchange activates only its exact owner.
///
/// Registers one Service identity in the `prod` and `staging` spaces, binds the
/// live Keycloak workload subject to the `prod` version, and exchanges the real
/// assertion twice. The first exchange activates exactly the `prod` owner; the
/// second, re-presenting the platform assertion as a workload does to renew,
/// renews that one timestamp. The same-named `staging` owner is never touched.
///
/// # Panics
/// Panics when the server fails to boot, a registration or exchange fails, or
/// an activity expectation does not hold.
#[tokio::test]
#[ignore = "requires the Keycloak and Dex identity lane"]
async fn workload_jwt_bearer_activates_only_its_exact_owner_keycloak() {
    let keycloak = OidcIssuerFixture::connect(&keycloak_issuer()).await;
    let assertion = keycloak
        .workload_token("wyrd-workload", "wyrd-workload-secret", "wyrd-workload")
        .await;
    let subject = jwt_claims(&assertion)["sub"]
        .as_str()
        .expect("workload token carries a subject")
        .to_owned();
    let srv = WyrdTestServerBuilder::default()
        .with_trusted_issuer_configs(vec![keycloak_workload_issuer()])
        .with_workload_binding_configs(vec![WorkloadBindingEntry {
            issuer: keycloak_issuer(),
            subject,
            audience: Some("wyrd-workload".to_owned()),
            kind: "service".to_owned(),
            name: "wyrd-workload-live".to_owned(),
            space: "prod".to_owned(),
            version: "1.0.0".to_owned(),
        }])
        .start_in_process()
        .await
        .expect("server boots config-driven");
    let Bootstrap::User { jwt, .. } = srv
        .bootstrap_user("workload-live-writer", &["writer"])
        .await
        .expect("writer bootstraps")
    else {
        panic!("user bootstrap returned a non-user principal");
    };
    let prod = register_service(&srv, &jwt, "wyrd-workload-live", "prod").await;
    let staging = register_service(&srv, &jwt, "wyrd-workload-live", "staging").await;
    assert_eq!(owner_last_authenticated(&srv, prod).await, None);

    let first = post_jwt_bearer(&srv, &assertion).await;
    assert_eq!(first.status(), StatusCode::OK, "first workload exchange");
    let activated = owner_last_authenticated(&srv, prod)
        .await
        .expect("the workload exchange activates its exact owner");
    assert_eq!(owner_last_authenticated(&srv, staging).await, None);

    let renewal = post_jwt_bearer(&srv, &assertion).await;
    assert_eq!(
        renewal.status(),
        StatusCode::OK,
        "renewal workload exchange"
    );
    let renewed = owner_last_authenticated(&srv, prod)
        .await
        .expect("renewal keeps activity");
    assert!(
        renewed >= activated,
        "renewal never moves activity backward"
    );
    assert_eq!(
        owner_last_authenticated(&srv, staging).await,
        None,
        "the same-named owner in another space is never activated"
    );
}

// ─── TTL expiry journey ───────────────────────────────────────────────────────

/// Mint a short-lived access token, reach a real `/v1/cards` `200`, sleep
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
    assert_v1_delegated_write_ok(&srv, &access_token, "ttl").await;

    // Sleep past access_ttl (2s) with margin for second-granular `exp`.
    tokio::time::sleep(StdDuration::from_secs(3)).await;

    // Expired token is rejected on /v1 (verify fails before the delegation guard).
    assert_eq!(
        card_write_status(&srv, &access_token, "ttl-after").await,
        StatusCode::UNAUTHORIZED,
        "expired token rejected on /v1"
    );
}

// ─── Revocation journey ───────────────────────────────────────────────────────

/// Mint a token, reach a real `/v1/cards` `200`, revoke the principal via
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
    assert_v1_delegated_write_ok(&srv, &target_token, "revoke").await;

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
    assert_v1_delegated_write_ok(&srv, &target_token, "revoke-window").await;
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
///   6. `assert_v1_delegated_write_ok` → `POST /v1/cards 201`.
///
/// # Panics
/// Panics when bootstrap or either key exchange fails, `/auth/issue-key` does
/// not return `200` with a non-nil `key_id`, or the issued key's token does not
/// reach `/v1/cards` `200`.
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

    // Terminal: /v1/cards 200 — proves the issued token is valid and the
    // full chain succeeds with a service-account issuer.
    assert_v1_delegated_write_ok(&srv, &issued_token, "sa-chain").await;
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
        .with_ui_client_secret(UI_CLIENT_SECRET)
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
    activate_connection(
        srv,
        admin,
        connection_input(client_id, client_auth, client_secret, None),
    )
    .await
}

/// Stage `input` as the candidate (at the current candidate revision, if
/// any), test it through one real sign-in, and activate it, returning the
/// Active view.
///
/// # Panics
/// Panics when any step does not return `200`.
async fn activate_connection(srv: &WyrdTestServer, admin: &TenantAdmin, mut input: Value) -> Value {
    let (_, listed) = call_json(srv, &admin.token, Method::GET, CONNECTIONS, None).await;
    input["expected_revision"] = listed["candidate"]["revision"].clone();
    let (status, candidate) =
        call_json(srv, &admin.token, Method::PUT, CANDIDATE, Some(input)).await;
    assert_eq!(status, StatusCode::OK, "candidate stages: {candidate}");
    let revision = candidate["revision"].as_u64().expect("candidate revision");
    let (status, begun) = begin_test(srv, &admin.token, revision).await;
    assert_eq!(status, StatusCode::OK, "candidate test begins: {begun}");
    assert_test_completion(&finish_test_sign_in(srv, &begun).await);
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

/// `POST` the candidate test for `revision` as `token`, returning the status
/// and body; a begun test's body carries only the provider authorization URL.
async fn begin_test(srv: &WyrdTestServer, token: &str, revision: u64) -> (StatusCode, Value) {
    call_json(
        srv,
        token,
        Method::POST,
        CANDIDATE_TEST,
        Some(serde_json::json!({ "expected_revision": revision })),
    )
    .await
}

/// Complete a begun candidate test as a browser would: sign Keycloak's
/// `alice` in at the returned authorization URL (a mock provider redirects
/// at once), then present every parameter of the provider's return —
/// `code`, `state`, and any `iss` — to the common callback.
///
/// # Panics
/// Panics when the body carries no authorization URL or the provider
/// sign-in does not return to the deployment callback.
async fn finish_test_sign_in(srv: &WyrdTestServer, begun: &Value) -> CallbackReply {
    let authorization_url: Url = begun["authorization_url"]
        .as_str()
        .unwrap_or_else(|| panic!("a begun test returns an authorization URL: {begun}"))
        .parse()
        .expect("authorization URL parses");
    let returned = provider_sign_in(
        &authorization_url,
        "alice",
        "alice-password",
        &format!("{PUBLIC_ORIGIN}/auth/callback"),
    )
    .await;
    let params: Vec<(String, String)> = returned.query_pairs().into_owned().collect();
    let params: Vec<(&str, &str)> = params
        .iter()
        .map(|(name, value)| (name.as_str(), value.as_str()))
        .collect();
    callback_reply_with(srv, &params, None, "test-tenant-1.wyrd.test").await
}

/// Assert a candidate test sign-in completed: `200` with the static test
/// page, which carries no token.
///
/// # Panics
/// Panics when the reply differs.
fn assert_test_completion(reply: &CallbackReply) {
    assert_eq!(
        reply.status,
        StatusCode::OK,
        "the test sign-in completes: {}",
        reply.body
    );
    assert!(
        reply.body.contains("Connection test complete"),
        "{}",
        reply.body
    );
    for leaked in ["access_token", "refresh_token"] {
        assert!(
            !reply.body.contains(leaked),
            "the test page carries no {leaked}"
        );
    }
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
/// Check the Candidate's provider and begin its test sign-in.
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

/// The `wyrd-ui` client secret every human journey server registers; the
/// journey plays that confidential client at the token endpoint.
const UI_CLIENT_SECRET: &str = "identity-journey-ui-secret";

/// The opaque `state` the journey's `wyrd-ui` client sends to
/// `GET /auth/authorize` and expects echoed on every redirect back.
const UI_STATE: &str = "journey-ui-state";

/// The `wyrd-ui` client's registered redirect URI.
fn ui_redirect() -> String {
    format!("{PUBLIC_ORIGIN}/login/callback")
}

/// A fresh RFC 7636 code verifier and its S256 challenge, as the `wyrd-ui`
/// client generates per authorization request.
fn pkce_pair() -> (String, String) {
    let verifier = format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple());
    let challenge = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .encode(<sha2::Sha256 as sha2::Digest>::digest(verifier.as_bytes()));
    (verifier, challenge)
}

/// `GET /auth/authorize` as the `wyrd-ui` client for `tenant_slug` with the
/// S256 `challenge`, under hostile `Host` and forwarded headers that must play
/// no part; returns the status and `Location`.
///
/// # Panics
/// Panics when the request cannot be built, the router fails, or the
/// `Location` does not parse.
async fn authorize(
    srv: &WyrdTestServer,
    tenant_slug: &str,
    challenge: &str,
) -> (StatusCode, Option<Url>) {
    let query = url::form_urlencoded::Serializer::new(String::new())
        .append_pair("response_type", "code")
        .append_pair("client_id", "wyrd-ui")
        .append_pair("redirect_uri", &ui_redirect())
        .append_pair("code_challenge", challenge)
        .append_pair("code_challenge_method", "S256")
        .append_pair("state", UI_STATE)
        .append_pair("tenant", tenant_slug)
        .finish();
    let response = srv
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri(format!("/auth/authorize?{query}"))
                .header(header::HOST, "attacker.example.net")
                .header("x-forwarded-host", "attacker.example.net")
                .header("x-forwarded-proto", "https")
                .body(Body::empty())
                .expect("authorize request builds"),
        )
        .await
        .expect("auth call completes");
    let location = response
        .headers()
        .get(header::LOCATION)
        .and_then(|value| value.to_str().ok())
        .map(|value| value.parse().expect("Location parses"));
    (response.status(), location)
}

/// The RFC 6749 §4.1.2.1 `error` a redirect to the `wyrd-ui` client
/// carries, after asserting it echoes the client's `state` and carries no
/// code.
///
/// # Panics
/// Panics when `location` is not a refusal redirect to the registered
/// redirect URI.
fn client_refusal(location: &str) -> String {
    let params = client_redirect_params(location);
    assert!(
        !params.iter().any(|(name, _)| name == "code"),
        "a refusal carries no code: {location}"
    );
    params
        .into_iter()
        .find_map(|(name, value)| (name == "error").then_some(value))
        .unwrap_or_else(|| panic!("a refusal names its error: {location}"))
}

/// The query of a redirect to the `wyrd-ui` client's registered redirect
/// URI, after asserting it echoes the client's `state`.
///
/// # Panics
/// Panics when `location` is not the registered redirect URI or omits the
/// state.
fn client_redirect_params(location: &str) -> Vec<(String, String)> {
    let url: Url = location.parse().expect("client redirect parses");
    assert_eq!(
        &url[..url::Position::AfterPath],
        ui_redirect(),
        "the redirect returns to the registered redirect URI"
    );
    let params: Vec<(String, String)> = url.query_pairs().into_owned().collect();
    assert!(
        params.contains(&("state".to_owned(), UI_STATE.to_owned())),
        "the redirect echoes the client state: {location}"
    );
    params
}

/// Begin an authorization request for the fixture tenant and return the
/// RFC 6749 error it was refused with.
///
/// # Panics
/// Panics when the request is not refused with a redirect to the client.
async fn login_refusal(srv: &WyrdTestServer) -> String {
    let (_, challenge) = pkce_pair();
    let (status, location) = authorize(srv, FIXTURE_TENANT_SLUG, &challenge).await;
    assert_eq!(status, StatusCode::SEE_OTHER, "authorize redirects");
    client_refusal(location.expect("a refusal redirects").as_str())
}

/// A provider's redirect back to Wyrd for one begun authorization request.
struct ProviderReturn {
    /// The authorization code the provider issued.
    code: String,
    /// The state the provider echoed; it names the login-state row.
    state: String,
    /// The RFC 9207 issuer the provider returned, forwarded to the callback
    /// exactly as a browser following the redirect would.
    iss: Option<String>,
    /// The `wyrd-ui` client's PKCE verifier for the Wyrd authorization code.
    verifier: String,
}

/// Begin an authorization request for `tenant_slug` and authenticate
/// `username` at `keycloak`, returning the provider's redirect back to Wyrd.
///
/// `GET /auth/authorize` redirects to the provider with Wyrd's own PKCE
/// challenge, nonce, and state, and the Keycloak fixture submits the real
/// HTML login form. Nothing is exchanged yet, so a journey can change the
/// tenant's connection between provider authentication and the callback.
///
/// # Panics
/// Panics when the request does not redirect to the provider with the PKCE
/// challenge, nonce, and state, or when the IdP echoes a different `state`.
async fn authorization_code_for(
    srv: &WyrdTestServer,
    tenant_slug: &str,
    keycloak: &OidcIssuerFixture,
    client_id: &str,
    username: &str,
    password: &str,
) -> ProviderReturn {
    let (verifier, challenge) = pkce_pair();
    let (status, location) = authorize(srv, tenant_slug, &challenge).await;
    assert_eq!(status, StatusCode::SEE_OTHER, "authorize redirects");
    let authz_url = location.expect("authorize redirects to the provider");
    let query = |name: &str| {
        authz_url
            .query_pairs()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.into_owned())
            .unwrap_or_else(|| panic!("the provider redirect carries {name}: {authz_url}"))
    };
    let (code_challenge, nonce, state) = (query("code_challenge"), query("nonce"), query("state"));
    let redirect_uri: Url = format!("{PUBLIC_ORIGIN}/auth/callback")
        .parse()
        .expect("redirect URI parses");
    let login = keycloak
        .human_login(
            client_id,
            username,
            password,
            &redirect_uri,
            &state,
            &code_challenge,
            &nonce,
        )
        .await;
    assert!(
        !login.code.is_empty(),
        "Keycloak returned an authorization code"
    );
    assert_eq!(login.state, state, "state echoed back correctly");
    ProviderReturn {
        code: login.code,
        state: login.state,
        iss: login.iss,
        verifier,
    }
}

/// [`authorization_code_for`] the fixture tenant.
async fn authorization_code(
    srv: &WyrdTestServer,
    keycloak: &OidcIssuerFixture,
    client_id: &str,
    username: &str,
    password: &str,
) -> ProviderReturn {
    authorization_code_for(
        srv,
        FIXTURE_TENANT_SLUG,
        keycloak,
        client_id,
        username,
        password,
    )
    .await
}

/// The raw reply of the common callback.
struct CallbackReply {
    /// Response status.
    status: StatusCode,
    /// `Location` header, when present.
    location: Option<String>,
    /// Response body as text.
    body: String,
}

impl CallbackReply {
    /// The body as problem JSON (`Null` when it is not JSON).
    fn problem(&self) -> Value {
        serde_json::from_str(&self.body).unwrap_or(Value::Null)
    }
}

/// Present `code`, `state`, and the RFC 9207 `iss` (when `Some`) to
/// `GET /auth/callback` with `host` as the request `Host`, and read the raw
/// reply.
///
/// # Panics
/// Panics when the request cannot be built or the router fails.
async fn callback_reply(
    srv: &WyrdTestServer,
    code: &str,
    state: &str,
    iss: Option<&str>,
    host: &str,
) -> CallbackReply {
    callback_reply_with(srv, &[("code", code), ("state", state)], iss, host).await
}

/// Present `params` plus the RFC 9207 `iss` (when `Some`) as the query of
/// `GET /auth/callback` with `host` as the request `Host`, and read the raw
/// reply; lets a journey add provider parameters Wyrd must tolerate.
///
/// # Panics
/// Panics when the request cannot be built or the router fails.
async fn callback_reply_with(
    srv: &WyrdTestServer,
    params: &[(&str, &str)],
    iss: Option<&str>,
    host: &str,
) -> CallbackReply {
    let mut query = url::form_urlencoded::Serializer::new(String::new());
    query.extend_pairs(params);
    if let Some(iss) = iss {
        query.append_pair("iss", iss);
    }
    let query = query.finish();
    let response = srv
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri(format!("/auth/callback?{query}"))
                .header(header::HOST, host)
                .body(Body::empty())
                .expect("callback request builds"),
        )
        .await
        .expect("auth call completes");
    let status = response.status();
    let location = response
        .headers()
        .get(header::LOCATION)
        .and_then(|value| value.to_str().ok())
        .map(ToOwned::to_owned);
    let bytes = to_bytes(response.into_body(), 65_536)
        .await
        .expect("callback body reads");
    CallbackReply {
        status,
        location,
        body: String::from_utf8_lossy(&bytes).into_owned(),
    }
}

/// Present `code`, `state`, and the optional RFC 9207 `iss` to the callback
/// and return the status and problem body, for journeys asserting a refusal.
async fn finish_callback(
    srv: &WyrdTestServer,
    code: &str,
    state: &str,
    iss: Option<&str>,
) -> (StatusCode, Value) {
    let reply = callback_reply(srv, code, state, iss, "test-tenant-1.wyrd.test").await;
    (reply.status, reply.problem())
}

/// The Wyrd authorization code a successful callback redirected the
/// `wyrd-ui` client with, after asserting the `303` echoes the client state
/// and the response carries neither a token nor the provider's code.
///
/// # Panics
/// Panics when the reply is not that redirect.
fn authorized_code(reply: &CallbackReply, provider_code: &str) -> String {
    assert_eq!(
        reply.status,
        StatusCode::SEE_OTHER,
        "a login completes with a redirect: {}",
        reply.body
    );
    let location = reply.location.as_deref().expect("the callback redirects");
    for leaked in ["access_token", "refresh_token", provider_code] {
        assert!(
            !reply.body.contains(leaked) && !location.contains(leaked),
            "the callback response carries no {leaked}"
        );
    }
    client_redirect_params(location)
        .into_iter()
        .find_map(|(name, value)| (name == "code").then_some(value))
        .unwrap_or_else(|| panic!("the redirect carries the code: {location}"))
}

/// The RFC 6749 error a refused callback redirected the `wyrd-ui` client
/// with.
///
/// # Panics
/// Panics when the reply is not a refusal redirect to the client.
fn callback_refusal(reply: &CallbackReply) -> String {
    assert_eq!(
        reply.status,
        StatusCode::SEE_OTHER,
        "a refused login redirects to the client: {}",
        reply.body
    );
    client_refusal(reply.location.as_deref().expect("the callback redirects"))
}

/// Present a provider return to the callback and return the RFC 6749 error
/// the refused login redirected the client with.
///
/// # Panics
/// Panics when the login is not refused with a redirect to the client.
async fn refused_login(srv: &WyrdTestServer, code: &str, state: &str, iss: Option<&str>) -> String {
    callback_refusal(&callback_reply(srv, code, state, iss, "test-tenant-1.wyrd.test").await)
}

/// Redeem `code` at `POST /auth/token` as the confidential `wyrd-ui` client
/// with `verifier`, returning the status and RFC 6749 JSON body.
///
/// # Panics
/// Panics when the request cannot be built or the router fails.
async fn redeem(srv: &WyrdTestServer, code: &str, verifier: &str) -> (StatusCode, Value) {
    let redirect_uri = ui_redirect();
    token_call(
        srv,
        &[
            ("grant_type", "authorization_code"),
            ("code", code),
            ("redirect_uri", &redirect_uri),
            ("code_verifier", verifier),
        ],
    )
    .await
}

/// `POST /auth/token` with the form `params`, authenticated as the
/// confidential `wyrd-ui` client (`client_secret_basic`), returning the
/// status and RFC 6749 JSON body.
///
/// # Panics
/// Panics when the request cannot be built or the router fails.
async fn token_call(srv: &WyrdTestServer, params: &[(&str, &str)]) -> (StatusCode, Value) {
    let form = url::form_urlencoded::Serializer::new(String::new())
        .extend_pairs(params)
        .finish();
    let (status, _, body) = token_request(
        srv,
        UI_CLIENT_SECRET,
        "application/x-www-form-urlencoded",
        form,
    )
    .await;
    (status, body)
}

/// `POST /auth/token` with `body` of `content_type`, authenticated as
/// `wyrd-ui` with `secret`, returning the status, headers, and JSON body.
///
/// # Panics
/// Panics when the request cannot be built or the router fails.
async fn token_request(
    srv: &WyrdTestServer,
    secret: &str,
    content_type: &str,
    body: String,
) -> (StatusCode, header::HeaderMap, Value) {
    let basic = base64::engine::general_purpose::STANDARD.encode(format!("wyrd-ui:{secret}"));
    let response = srv
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/auth/token")
                .header(header::AUTHORIZATION, format!("Basic {basic}"))
                .header(header::CONTENT_TYPE, content_type)
                .body(Body::from(body.clone()))
                .expect("token request builds"),
        )
        .await
        .expect("auth call completes");
    let status = response.status();
    let headers = response.headers().clone();
    let bytes = to_bytes(response.into_body(), 65_536)
        .await
        .expect("token body reads");
    (
        status,
        headers,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

/// Complete one provider return through the callback and the token
/// endpoint, returning the session JSON.
///
/// # Panics
/// Panics when the callback does not redirect the client with a code, or the
/// code does not redeem exactly once.
async fn complete_login(srv: &WyrdTestServer, provider: &ProviderReturn) -> Value {
    let reply = callback_reply(
        srv,
        &provider.code,
        &provider.state,
        provider.iss.as_deref(),
        "attacker.example.net",
    )
    .await;
    let code = authorized_code(&reply, &provider.code);
    let (status, session) = redeem(srv, &code, &provider.verifier).await;
    assert_eq!(status, StatusCode::OK, "the code redeems: {session}");
    let (status, replay) = redeem(srv, &code, &provider.verifier).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "a code redeems once");
    assert_eq!(replay["error"], "invalid_grant", "{replay}");
    session
}

/// Drive one complete authorization-code login for the fixture tenant and
/// return the session JSON.
///
/// This is the served human path end to end: authorize as `wyrd-ui`,
/// authenticate at Keycloak, receive the `303` with a code from the common
/// callback, and redeem it once at the token endpoint. Every human journey
/// starts here.
///
/// # Panics
/// Panics when any leg of the flow fails.
async fn human_login(
    srv: &WyrdTestServer,
    keycloak: &OidcIssuerFixture,
    client_id: &str,
    username: &str,
    password: &str,
) -> Value {
    let provider = authorization_code(srv, keycloak, client_id, username, password).await;
    complete_login(srv, &provider).await
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

/// The off-the-shelf `oauth2` crate client the CLI journeys play `wyrd-cli`
/// with: a public client sending its `client_id` in the form body.
type CliClient = oauth2::basic::BasicClient<
    oauth2::EndpointNotSet,
    oauth2::EndpointSet,
    oauth2::EndpointNotSet,
    oauth2::EndpointNotSet,
    oauth2::EndpointSet,
>;

/// The `wyrd-cli` client, configured with nothing but the endpoint URLs the
/// server's RFC 8414 metadata publishes.
///
/// # Panics
/// Panics when an endpoint URL does not parse.
fn cli_client() -> CliClient {
    oauth2::basic::BasicClient::new(oauth2::ClientId::new("wyrd-cli".to_owned()))
        .set_auth_type(oauth2::AuthType::RequestBody)
        .set_device_authorization_url(
            oauth2::DeviceAuthorizationUrl::new(format!(
                "{PUBLIC_ORIGIN}/auth/device_authorization"
            ))
            .expect("device authorization URL parses"),
        )
        .set_token_uri(
            oauth2::TokenUrl::new(format!("{PUBLIC_ORIGIN}/auth/token")).expect("token URL parses"),
        )
}

/// Serve one `oauth2` crate request through `srv`'s router, exactly as the
/// crate's network client would deliver it.
///
/// # Errors
/// Never fails: a router failure panics instead.
///
/// # Panics
/// Panics when the router fails or the body cannot be read.
async fn oauth_http(
    srv: &WyrdTestServer,
    request: oauth2::HttpRequest,
) -> Result<oauth2::HttpResponse, std::convert::Infallible> {
    let response = srv
        .oneshot({
            let mut rebuilt = Request::builder()
                .method(request.method().clone())
                .uri(request.uri().clone());
            for (name, value) in request.headers() {
                rebuilt = rebuilt.header(name, value);
            }
            rebuilt
                .body(Body::from(request.body().clone()))
                .expect("request rebuilds")
        })
        .await
        .expect("auth call completes");
    let (parts, body) = response.into_parts();
    let body = to_bytes(body, 65_536).await.expect("body reads");
    Ok(oauth2::HttpResponse::from_parts(parts, body.to_vec()))
}

/// Complete one CLI device login (RFC 8628) for the fixture tenant as
/// `username` with the `oauth2` crate: authorize a device code, approve its
/// user code on the verification page, sign in at the provider it redirects
/// to, deliver the provider's return to the common callback, and poll the
/// token endpoint until the device code redeems.
///
/// # Panics
/// Panics when any step fails.
async fn cli_login(
    srv: &WyrdTestServer,
    cli: &CliClient,
    username: &str,
    password: &str,
) -> oauth2::basic::BasicTokenResponse {
    let http = |request| oauth_http(srv, request);
    let device = begin_device_login(srv, cli).await;
    let sign_in = approve_device(srv, device.user_code().secret()).await;
    let returned = provider_sign_in(
        &sign_in,
        username,
        password,
        &format!("{PUBLIC_ORIGIN}/auth/callback"),
    )
    .await;
    let params: Vec<(String, String)> = returned.query_pairs().into_owned().collect();
    let params: Vec<(&str, &str)> = params
        .iter()
        .map(|(name, value)| (name.as_str(), value.as_str()))
        .collect();
    let reply = callback_reply_with(srv, &params, None, "test-tenant-1.wyrd.test").await;
    assert_eq!(reply.status, StatusCode::OK, "the device sign-in completes");
    assert!(reply.body.contains("Sign-in complete"), "{}", reply.body);
    cli.exchange_device_access_token(&device)
        .request_async(&http, tokio::time::sleep, None)
        .await
        .expect("the device code redeems")
}

/// Approve `user_code` for the fixture tenant on the verification page and
/// return the provider sign-in URL it redirects to.
///
/// # Panics
/// Panics when the page refuses the approval.
async fn approve_device(srv: &WyrdTestServer, user_code: &str) -> Url {
    let response = decide_device(srv, user_code, "approve").await;
    assert_eq!(
        response.status(),
        StatusCode::SEE_OTHER,
        "approval redirects to sign-in"
    );
    response
        .headers()
        .get(header::LOCATION)
        .and_then(|location| location.to_str().ok())
        .expect("approval names the sign-in URL")
        .parse()
        .expect("sign-in URL parses")
}

/// Post `decision` (`approve` or `deny`) for `user_code` and the fixture
/// tenant to the verification page from the deployment's origin, as the
/// page's own form does.
///
/// # Panics
/// Panics when the request cannot be built or the router fails.
async fn decide_device(
    srv: &WyrdTestServer,
    user_code: &str,
    decision: &str,
) -> axum::http::Response<Body> {
    let form = url::form_urlencoded::Serializer::new(String::new())
        .append_pair("tenant", FIXTURE_TENANT_SLUG)
        .append_pair("user_code", user_code)
        .append_pair("decision", decision)
        .finish();
    srv.oneshot(
        Request::builder()
            .method(Method::POST)
            .uri("/auth/device")
            .header(header::ORIGIN, PUBLIC_ORIGIN)
            .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
            .body(Body::from(form.clone()))
            .expect("device decision request builds"),
    )
    .await
    .expect("auth call completes")
}

/// Poll `POST /auth/token` once with `device_code` as the public `wyrd-cli`
/// client (RFC 8628 §3.4) and read the status and RFC 6749 body.
///
/// # Panics
/// Panics when the request cannot be built, the router fails, or the body
/// is not JSON.
async fn device_poll(srv: &WyrdTestServer, device_code: &str) -> (StatusCode, Value) {
    let form = url::form_urlencoded::Serializer::new(String::new())
        .append_pair("grant_type", "urn:ietf:params:oauth:grant-type:device_code")
        .append_pair("device_code", device_code)
        .append_pair("client_id", "wyrd-cli")
        .finish();
    let response = srv
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/auth/token")
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .body(Body::from(form.clone()))
                .expect("device poll builds"),
        )
        .await
        .expect("auth call completes");
    let status = response.status();
    let body = to_bytes(response.into_body(), 65_536)
        .await
        .expect("poll body reads");
    (
        status,
        serde_json::from_slice(&body).expect("poll body is JSON"),
    )
}

/// Begin one device login for the fixture tenant as `wyrd-cli` with the
/// `oauth2` crate.
///
/// # Panics
/// Panics when the device authorization request is refused.
async fn begin_device_login(
    srv: &WyrdTestServer,
    cli: &CliClient,
) -> oauth2::StandardDeviceAuthorizationResponse {
    cli.exchange_device_code()
        .add_extra_param("tenant", FIXTURE_TENANT_SLUG)
        .request_async(&|request| oauth_http(srv, request))
        .await
        .expect("the device login begins")
}

/// Whether `result` is the RFC 6749 §5.2 `invalid_grant` refusal.
fn is_invalid_grant<T>(
    result: &Result<
        T,
        oauth2::RequestTokenError<std::convert::Infallible, oauth2::basic::BasicErrorResponse>,
    >,
) -> bool {
    matches!(
        result,
        Err(oauth2::RequestTokenError::ServerResponse(error))
            if *error.error() == oauth2::basic::BasicErrorResponseType::InvalidGrant
    )
}

/// Drive a complete config-driven human OIDC login through the CLI's device
/// grant with the off-the-shelf `oauth2` crate:
///   1. a tenant admin stages, tests, and activates the Keycloak connection
///      through `/v1/identity/oidc/*`, granting `writer` through the
///      `wyrd-admins` group so the federated human can be a subject,
///   2. `POST /auth/device_authorization` as the public `wyrd-cli` client,
///   3. alice approves the user code and signs in at Keycloak; the common
///      callback records only the approval,
///   4. the device-code poll at `POST /auth/token` mints the session,
///   5. the human token reaches a real Card write through delegation.
///
/// It then rotates the public client's refresh token, proves the successor
/// still authorizes, and proves a replay of the consumed token is refused
/// with `invalid_grant` and kills the successor (RFC 9700 §4.14.2).
///
/// # Panics
/// Panics when the server fails to start, a login, refresh, or check step
/// fails, or any status, code, or activity expectation does not hold.
#[tokio::test]
#[ignore = "requires the Keycloak and Dex identity lane"]
async fn human_oidc_login_journey() {
    let srv = human_server().await;
    let baseline_activity = activated_principals(&srv).await;
    let cli = cli_client();
    let http = |request| oauth_http(&srv, request);

    let token = cli_login(&srv, &cli, "alice", "alice-password").await;
    let access_token = token.access_token().secret();
    assert!(!access_token.is_empty(), "access token is non-empty");

    // Step 5: human token reaches a real authenticated /v1 200 via delegation.
    let first_actor = assert_v1_delegated_write_ok(&srv, access_token, "human-sso").await;

    // Step 6: the CLI session carries a refresh token and renews by rotating
    // it. The grant does not consult the clock, so presenting the refresh
    // token is the whole renewal.
    let refresh_token = token
        .refresh_token()
        .expect("a human session is issued a refresh token");
    let rotated = cli
        .exchange_refresh_token(refresh_token)
        .request_async(&http)
        .await
        .expect("the refresh token rotates");
    let successor = rotated
        .refresh_token()
        .expect("rotation returns the successor refresh token");
    assert_ne!(
        successor.secret(),
        refresh_token.secret(),
        "the token rotated"
    );

    // Step 7: the successor reaches the same protected /v1 200, proving the
    // renewed session kept the authority the provider asserted at login.
    let second_actor =
        assert_v1_delegated_write_ok(&srv, rotated.access_token().secret(), "human-sso-rotated")
            .await;

    // Neither the human login, refresh, nor delegation records new machine
    // activity. Connection setup may have activated its card-bound admin;
    // after login, only the two writer actors may be added to that baseline.
    let mut expected_actors = baseline_activity;
    expected_actors.extend([first_actor.as_uuid(), second_actor.as_uuid()]);
    expected_actors.sort_unstable();
    assert_eq!(
        activated_principals(&srv).await,
        expected_actors,
        "only the delegation actors' own key exchanges record runtime activity"
    );

    // Step 8: replaying the consumed token is refused and contains the theft.
    let replay = cli
        .exchange_refresh_token(refresh_token)
        .request_async(&http)
        .await;
    assert!(
        is_invalid_grant(&replay),
        "replaying the consumed refresh token is refused: {replay:?}"
    );

    // Step 9: containment was committed with the refusal, so the successor the
    // attacker would hold is dead on a separate request and transaction.
    let after_replay = cli
        .exchange_refresh_token(successor)
        .request_async(&http)
        .await;
    assert!(
        is_invalid_grant(&after_replay),
        "the successor cannot rotate after replay: {after_replay:?}"
    );
}

/// The CLI's device grant issues nothing it should not (RFC 8628 §3.5), and
/// the authorization server publishes the endpoints the `oauth2` crate is
/// configured with (RFC 8414):
///   1. the metadata names the token, device, revocation, and authorization
///      endpoints under the public origin and S256 as the only PKCE method,
///   2. a pending code answers `authorization_pending`, then `slow_down` when
///      polled again within the interval; an unknown code is `invalid_grant`,
///   3. a denied code answers `access_denied` once and `invalid_grant` after,
///   4. an expired code answers `expired_token` once and `invalid_grant`
///      after, and can no longer be approved,
///   5. revoking an unknown token answers `200` (RFC 7009 §2.2).
///
/// No refused poll returns a token.
///
/// # Panics
/// Panics when the server fails to start or any answer differs.
#[tokio::test]
#[ignore = "requires the Keycloak and Dex identity lane"]
async fn device_grant_refusal_journey() {
    let srv = human_server().await;
    let cli = cli_client();

    // Step 1: RFC 8414 metadata.
    let response = srv
        .oneshot(
            Request::builder()
                .uri("/.well-known/oauth-authorization-server")
                .body(Body::empty())
                .expect("metadata request builds"),
        )
        .await
        .expect("auth call completes");
    assert_eq!(response.status(), StatusCode::OK);
    let metadata: Value = serde_json::from_slice(
        &to_bytes(response.into_body(), 65_536)
            .await
            .expect("metadata reads"),
    )
    .expect("metadata is JSON");
    assert_eq!(metadata["issuer"], PUBLIC_ORIGIN, "{metadata}");
    for (field, path) in [
        ("authorization_endpoint", "/auth/authorize"),
        ("token_endpoint", "/auth/token"),
        (
            "device_authorization_endpoint",
            "/auth/device_authorization",
        ),
        ("revocation_endpoint", "/auth/revoke"),
    ] {
        assert_eq!(metadata[field], format!("{PUBLIC_ORIGIN}{path}"), "{field}");
    }
    assert_eq!(
        metadata["code_challenge_methods_supported"],
        serde_json::json!(["S256"])
    );

    // Step 2: pending, too fast, unknown.
    let pending = begin_device_login(&srv, &cli).await;
    let code = pending.device_code().secret();
    for expected in ["authorization_pending", "slow_down"] {
        let (status, body) = device_poll(&srv, code).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
        assert_eq!(body["error"], expected, "{body}");
    }
    let (status, body) = device_poll(&srv, "not-a-device-code").await;
    assert_eq!(
        (status, &body["error"]),
        (StatusCode::BAD_REQUEST, &serde_json::json!("invalid_grant"))
    );

    // Step 3: denied.
    let denial = decide_device(&srv, pending.user_code().secret(), "deny").await;
    assert_eq!(
        denial.status(),
        StatusCode::OK,
        "the page records the denial"
    );
    for expected in ["access_denied", "invalid_grant"] {
        let (status, body) = device_poll(&srv, code).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
        assert_eq!(body["error"], expected, "{body}");
        assert!(body.get("access_token").is_none(), "{body}");
    }

    // Step 4: expired.
    let expiring = begin_device_login(&srv, &cli).await;
    sqlx::query(
        "UPDATE wyrd.auth_device_authorizations
            SET created_at = statement_timestamp() - interval '11 minutes',
                expires_at = statement_timestamp() - interval '1 minute'",
    )
    .execute(
        &srv.pg_fixture()
            .superuser_pool()
            .await
            .expect("superuser pool opens"),
    )
    .await
    .expect("the device code expires");
    let refused = decide_device(&srv, expiring.user_code().secret(), "approve").await;
    assert_eq!(
        refused.status(),
        StatusCode::BAD_REQUEST,
        "an expired code is not approved"
    );
    for expected in ["expired_token", "invalid_grant"] {
        let (status, body) = device_poll(&srv, expiring.device_code().secret()).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
        assert_eq!(body["error"], expected, "{body}");
        assert!(body.get("access_token").is_none(), "{body}");
    }

    // Step 5: revoking an unknown token succeeds and revokes nothing.
    let form = "token=not-a-token&client_id=wyrd-cli";
    let revoked = srv
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/auth/revoke")
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .body(Body::from(form))
                .expect("revoke request builds"),
        )
        .await
        .expect("auth call completes");
    assert_eq!(revoked.status(), StatusCode::OK);
}

/// Present a `wyrd-ui` refresh token to `POST /auth/token` as that
/// confidential client and read the status and RFC 6749 body.
///
/// # Panics
/// Panics when the request cannot be built or the router fails.
async fn post_refresh(srv: &WyrdTestServer, refresh_token: &str) -> (StatusCode, Value) {
    token_call(
        srv,
        &[
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh_token),
        ],
    )
    .await
}

/// Revoking a human retires the session's refresh authority immediately.
///
/// A human session holds a short-lived access token and a refresh token. This
/// journey drives the served path an operator actually uses: log in, use the
/// access token, revoke the User principal through `/v1/principals/{id}/revoke`,
/// and then show the session cannot continue — the refresh token, which renews
/// without rotating before the revocation, no longer renews — while the access token already issued keeps its
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
    assert_v1_delegated_write_ok(&srv, &access_token, "human-revoke").await;

    // The confidential `wyrd-ui` refresh token does not rotate: the same
    // token renews twice and no successor is issued.
    for renewal in 0..2 {
        let (status, renewed) = post_refresh(&srv, &refresh_token).await;
        assert_eq!(status, StatusCode::OK, "renewal {renewal}: {renewed}");
        assert!(renewed["access_token"].is_string(), "{renewed}");
        assert!(
            renewed.get("refresh_token").is_none(),
            "a wyrd-ui renewal issues no successor: {renewed}"
        );
    }

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
    assert_v1_delegated_write_ok(&srv, &access_token, "human-revoke-window").await;

    // The refresh half is retired in the same transaction, so renewal is
    // refused and mints nothing for the session to continue under.
    let (refresh_status, refresh_body) = post_refresh(&srv, &refresh_token).await;
    assert_eq!(
        (refresh_status, &refresh_body["error"]),
        (StatusCode::BAD_REQUEST, &Value::from("invalid_grant")),
        "the revoked human's refresh token cannot renew: {refresh_body}"
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
/// introspected. The granting and withdrawing logins each stage one
/// `auth.user.roles.sync` event for alice; the unchanged login stages none.
///
/// The membership is restored before the journey returns, because the realm is
/// shared with every other Keycloak journey in this target.
///
/// # Panics
/// Panics when the server or actor bootstrap fails, a login yields no access
/// token, the granted session stops reaching `200` after an unchanged
/// re-login, the role-sync event count differs, delegation from the reduced session fails, or the reduced
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
    assert_v1_delegated_write_ok(&srv, &granted_token, "roles-granted").await;
    let alice = principal_id_of(&granted_token);
    assert_eq!(
        roles_sync_events(&srv, &alice).await,
        1,
        "the granting login records one role change"
    );

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
    assert_v1_delegated_write_ok(&srv, &granted_token, "roles-unchanged").await;
    assert_eq!(
        roles_sync_events(&srv, &alice).await,
        1,
        "an unchanged login records no role change"
    );

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
    assert_eq!(
        roles_sync_events(&srv, &alice).await,
        2,
        "the withdrawing login records its role change"
    );

    // The successor the same login issued carries the reduced authority: it can
    // still be a subject, but the intersection no longer holds card_write.
    let (delegated, _actor) =
        delegate_through_writer(&srv, &reduced_token, "roles-withdrawn").await;
    assert_eq!(
        card_write_status(&srv, &delegated, "roles-withdrawn").await,
        StatusCode::FORBIDDEN,
        "the reduced session cannot write through its actor"
    );

    keycloak
        .set_group_membership("alice", "wyrd-admins", true)
        .await;
}

/// Staged allowed `auth.user.roles.sync` events for User `principal_id`.
///
/// In-process test servers run no audit publisher, so every committed event
/// is still in staging.
///
/// # Panics
/// Panics when the query fails.
async fn roles_sync_events(srv: &WyrdTestServer, principal_id: &str) -> i64 {
    sqlx::query_scalar(
        "SELECT count(*) FROM vala.audit_staging \
          WHERE operation = 'auth.user.roles.sync' AND outcome = 'allowed' \
            AND principal_id = $1::uuid AND resource = 'principal:' || $1",
    )
    .bind(principal_id)
    .fetch_one(
        &srv.pg_fixture()
            .superuser_pool()
            .await
            .expect("superuser pool opens"),
    )
    .await
    .expect("role sync audit reads")
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
///   4. a candidate mapping any subject claim but `sub` is refused with
///      `VALIDATION`, and the old Human trusted-issuer HTTP and CLI writers
///      are refused with `HUMAN_CONNECTION_REQUIRED` while a Workload CLI
///      write still succeeds;
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
    assert_v1_delegated_write_ok(&srv, access, "tenant-a-connection").await;

    // 4. A human connection keyed by anything but the OIDC `sub` is refused,
    //    and the old Human writers are refused; Workload writes remain.
    let mut email_subject = connection_input(PUBLIC_HUMAN_CLIENT, "Public", None, None);
    email_subject["claim_mapping"]["subject"] = Value::from("email");
    let (status, body) = call_json(
        &srv,
        &admin_a.token,
        Method::PUT,
        CANDIDATE,
        Some(email_subject),
    )
    .await;
    assert_refused(
        status,
        &body,
        StatusCode::BAD_REQUEST,
        "WYRD_SPEC_400_VALIDATION",
    );
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
    assert_eq!(login_refusal(&srv).await, "access_denied");
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
///   2. a candidate with a wrong secret begins its test, but the provider
///      refuses the test sign-in's code exchange (`401 INVALID_TOKEN`), so
///      it stays untested and cannot be activated;
///   3. the fixed candidate's stamp lasts fifteen minutes, a malformed
///      recovery key and one without `identity_connections:write` are
///      refused;
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
///      staged on B is activated by the recovery principal and served by the
///      K2-only replica at once, and deactivation on B stops login there;
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

    // 2. A wrong secret fails the test: the provider refuses the sign-in's
    //    code exchange, and the candidate stays untested.
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
    let (status, begun) = begin_test(&replica_a, token, 2).await;
    assert_eq!(status, StatusCode::OK, "the test begins: {begun}");
    let reply = finish_test_sign_in(&replica_a, &begun).await;
    assert_refused(
        reply.status,
        &reply.problem(),
        StatusCode::UNAUTHORIZED,
        "WYRD_AUTH_401_INVALID_TOKEN",
    );
    let (_, listed) = call_json(&replica_a, token, Method::GET, CONNECTIONS, None).await;
    assert!(
        listed["candidate"]["tested_until"].is_null(),
        "a failed test leaves the candidate untested: {listed}"
    );
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
    let (status, begun) = begin_test(&replica_a, token, 3).await;
    assert_eq!(status, StatusCode::OK, "the test begins: {begun}");
    assert_test_completion(&finish_test_sign_in(&replica_a, &begun).await);
    let (_, tested) = call_json(&replica_a, token, Method::GET, CONNECTIONS, None).await;
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
    assert_v1_delegated_write_ok(
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
        assert_v1_delegated_write_ok(
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
    assert_v1_delegated_write_ok(
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
    let (status, begun) = begin_test(&replica_b, token, revision).await;
    assert_eq!(status, StatusCode::OK, "the rotation test begins: {begun}");
    assert_test_completion(&finish_test_sign_in(&replica_b, &begun).await);
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
    assert_eq!(login_refusal(&replica_k2).await, "access_denied");

    // 8. Each recovery decision is staged on the canonical audit path,
    //    attributed to the recovery principal and its verified credential;
    //    the malformed key left none. In-process replicas run no publisher,
    //    so every committed decision is still in staging.
    replica_b
        .wait_oracle_audit_staged(StdDuration::from_secs(30))
        .await
        .expect("audit outbox settles");
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
    let recovery_id = recovery_admin.id().as_uuid().to_string();
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
async fn api_key_id(superuser: &PgPool, principal: &Bootstrap) -> String {
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
/// Panics when the refresh is not `400 invalid_grant`, it returns any token,
/// or a refresh row was added.
async fn assert_refresh_cut_off(srv: &WyrdTestServer, session: &Value, label: &str) {
    let principal = principal_id_of(session["access_token"].as_str().expect("access token"));
    let before = refresh_rows(srv, &principal).await;
    let (status, body) = post_refresh(
        srv,
        session["refresh_token"].as_str().expect("refresh token"),
    )
    .await;
    assert_eq!(
        (status, &body["error"]),
        (StatusCode::BAD_REQUEST, &Value::from("invalid_grant")),
        "{label}: a stale session no longer renews: {body}"
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
///   5. a callback paused after provider authentication — held on the User's
///      refresh-family lock while B's deactivation commits — then fences on
///      the connection slot, refuses the login back to the client with
///      `access_denied`, and commits no code, role change, login audit, or
///      refresh row.
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
    assert_refresh_cut_off(&replica_a, &first, "replacement").await;

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

    // 5. A login paused after provider IO is refused once deactivation
    // commits, and commits nothing.
    activate_keycloak_connection(&replica_b, &admin, PUBLIC_HUMAN_CLIENT, "Public", None).await;
    let principal = principal_id_of(
        sign_in().await["access_token"]
            .as_str()
            .expect("access token"),
    );
    let superuser = replica_a
        .pg_fixture()
        .superuser_pool()
        .await
        .expect("superuser pool opens");
    let committed = || async {
        sqlx::query_as::<_, (i64, i64, i64, i64)>(
            "SELECT (SELECT count(*) FROM wyrd.auth_refresh_tokens
                      WHERE principal_id = $1::uuid),
                    (SELECT count(*) FROM wyrd.auth_login_state WHERE code_hash IS NOT NULL),
                    (SELECT count(*) FROM wyrd.auth_user_roles WHERE user_id = $1::uuid),
                    (SELECT count(*) FROM vala.audit_staging
                      WHERE operation IN ('auth.login', 'auth.user.roles.sync'))",
        )
        .bind(&principal)
        .fetch_one(&superuser)
        .await
        .expect("committed effects read")
    };
    let before = committed().await;
    let provider = authorization_code(
        &replica_a,
        &keycloak,
        PUBLIC_HUMAN_CLIENT,
        "alice",
        "alice-password",
    )
    .await;
    // The same key `lock_refresh_family` derives for this User.
    let family: i64 = sqlx::query_scalar(
        "SELECT hashtextextended(
             'wyrd.auth_refresh_tokens:' || $1::uuid::text || ':user:' || $2::uuid::text, 0)",
    )
    .bind(tenant.as_uuid())
    .bind(&principal)
    .fetch_one(&superuser)
    .await
    .expect("family lock key derives");
    let mut hold = superuser.begin().await.expect("hold transaction begins");
    sqlx::query("SELECT pg_advisory_xact_lock($1)")
        .bind(family)
        .execute(&mut *hold)
        .await
        .expect("the User's refresh family locks");
    let release = async {
        let deadline = tokio::time::Instant::now() + StdDuration::from_secs(30);
        loop {
            let waiting: bool = sqlx::query_scalar(
                "SELECT EXISTS (SELECT 1 FROM pg_locks
                  WHERE locktype = 'advisory' AND NOT granted
                    AND ((classid::bigint << 32) | objid::bigint) = $1)",
            )
            .bind(family)
            .fetch_one(&superuser)
            .await
            .expect("lock waiters read");
            if waiting {
                break;
            }
            assert!(
                tokio::time::Instant::now() < deadline,
                "the callback never reached its family lock"
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
    let (reply, ()) = tokio::join!(
        callback_reply(
            &replica_a,
            &provider.code,
            &provider.state,
            provider.iss.as_deref(),
            "test-tenant-1.wyrd.test",
        ),
        release
    );
    assert_eq!(
        callback_refusal(&reply),
        "access_denied",
        "the in-flight login is refused back to the client"
    );
    assert_eq!(
        committed().await,
        before,
        "the in-flight login committed no session, code, role, or login audit"
    );

    replica_b.shutdown().await.expect("replica B shuts down");
    replica_a.shutdown().await.expect("replica A shuts down");
}

// ─── Tenant human login and Wyrd authority ───────────────────────────────────

/// The second Keycloak realm: a distinct issuer whose `alice` shares the
/// first realm's email, used to switch a tenant's provider.
fn keycloak_second_issuer() -> String {
    format!("{}-2", keycloak_issuer().trim_end_matches('/'))
}

/// A connection input for the second realm's public `wyrd-human` client with
/// `group_role_map`.
fn second_provider_input(group_role_map: &HashMap<String, Vec<String>>) -> Value {
    serde_json::json!({
        "issuer": keycloak_second_issuer(),
        "client_id": PUBLIC_HUMAN_CLIENT,
        "client_auth": "Public",
        "client_secret": null,
        "claim_mapping": { "subject": "sub", "email": "email", "groups": "groups" },
        "group_role_map": group_role_map,
        "expected_revision": null,
    })
}

/// The real Card-write status through a fresh `writer` actor delegated by
/// `subject_jwt`: `201` only when the subject holds `card_write`.
///
/// # Panics
/// Panics when bootstrap, exchange, delegation, or the Card call fails.
async fn delegated_write_decision(
    srv: &WyrdTestServer,
    subject_jwt: &str,
    label: &str,
) -> StatusCode {
    let (delegated, _) = delegate_through_writer(srv, subject_jwt, label).await;
    card_write_status(srv, &delegated, label).await
}

/// Number of role grants recorded for the user `principal_id`.
///
/// # Panics
/// Panics when the query fails.
async fn user_role_count(srv: &WyrdTestServer, principal_id: &str) -> i64 {
    let superuser = srv
        .pg_fixture()
        .superuser_pool()
        .await
        .expect("superuser pool opens");
    sqlx::query_scalar("SELECT count(*) FROM wyrd.auth_user_roles WHERE user_id = $1::uuid")
        .bind(principal_id)
        .fetch_one(&superuser)
        .await
        .expect("user roles read")
}

/// Tenant human login end to end through the header-free authorization
/// request, the common callback, and one-use code redemption:
///   1. alice signs in: `/auth/authorize` redirects to the provider, the
///      callback answers `303` to the `wyrd-ui` redirect URI with a Wyrd code
///      and the client's state but no token or provider code, and the code
///      redeems once at the token endpoint;
///   2. the login created one user keyed by the Keycloak (issuer, subject);
///   3. alice's mapped group (`wyrd-admins` → `writer`) allows a delegated
///      write, while a call outside that grant (connection administration) is
///      refused;
///   4. bob, in no mapped group, signs in with zero role grants and his
///      delegated write is denied;
///   5. the successful issuance is retained on the canonical audit path.
///
/// # Panics
/// Panics when any step deviates from the contract above.
#[tokio::test]
#[ignore = "requires the Keycloak and Dex identity lane"]
async fn tenant_human_login_journey() {
    let keycloak = OidcIssuerFixture::connect(&keycloak_issuer()).await;
    let srv = human_server().await;

    // 1. Authorize, provider, 303, redeem once (asserted by `complete_login`).
    let provider = authorization_code(
        &srv,
        &keycloak,
        PUBLIC_HUMAN_CLIENT,
        "alice",
        "alice-password",
    )
    .await;
    let session = complete_login(&srv, &provider).await;
    let access = session["access_token"].as_str().expect("access token");
    assert!(
        session["refresh_token"].is_string(),
        "a human session renews"
    );
    let alice = principal_id_of(access);

    // 2. One user, keyed by the verified (issuer, subject).
    let superuser = srv
        .pg_fixture()
        .superuser_pool()
        .await
        .expect("superuser pool opens");
    let identities: Vec<(String, String)> = sqlx::query_as(
        "SELECT issuer, subject FROM wyrd.auth_user_identities WHERE user_id = $1::uuid",
    )
    .bind(&alice)
    .fetch_all(&superuser)
    .await
    .expect("identities read");
    assert_eq!(identities.len(), 1, "one identity: {identities:?}");
    assert_eq!(
        identities[0].0.trim_end_matches('/'),
        keycloak_issuer().trim_end_matches('/')
    );

    // 3. The mapped group allows; outside the grant is refused.
    assert_v1_delegated_write_ok(&srv, access, "tenant-login-alice").await;
    let (status, body) = call_json(&srv, access, Method::GET, CONNECTIONS, None).await;
    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "writer cannot administer connections: {body}"
    );

    // 4. An unmapped subject has no authority.
    let bob = human_login(&srv, &keycloak, PUBLIC_HUMAN_CLIENT, "bob", "wyrd-test").await;
    let bob_access = bob["access_token"].as_str().expect("bob access token");
    let bob_id = principal_id_of(bob_access);
    assert_ne!(bob_id, alice, "bob is a separate user");
    assert_eq!(user_role_count(&srv, &bob_id).await, 0, "bob has no grants");
    assert_eq!(
        delegated_write_decision(&srv, bob_access, "tenant-login-bob").await,
        StatusCode::FORBIDDEN,
        "an unmapped subject cannot write"
    );

    // 5. The issuance decision is on the canonical audit path.
    let allowed: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM vala.audit_staging \
          WHERE operation = 'auth.token.exchange' AND outcome = 'allowed' \
            AND principal_id = $1::uuid",
    )
    .bind(&alice)
    .fetch_one(&superuser)
    .await
    .expect("audit reads");
    assert!(allowed >= 1, "alice's issuance is audited");

    srv.shutdown().await.expect("server shuts down");
}

/// Ed25519 key the mock provider signs ID tokens with, and its JWKS `x`.
const MOCK_SIGNING_KEY: &str = "-----BEGIN PRIVATE KEY-----\nMC4CAQAwBQYDK2VwBCIEID78cHNjuFihX8aWPytQRoR2iUKHVXgdh92bcTcjQTYV\n-----END PRIVATE KEY-----\n";
/// Public half of [`MOCK_SIGNING_KEY`] as a JWK `x`.
const MOCK_SIGNING_X: &str = "WhCX9H41EwSjJJI1E6X3z5fTKyCZ3v2DsJluJ-DZ8Vw";
/// An Ed25519 key the mock provider does not publish.
const FOREIGN_SIGNING_KEY: &str = "-----BEGIN PRIVATE KEY-----\nMC4CAQAwBQYDK2VwBCIEIPNZb/xl9U5jhHGkOTPVsxugPf3cN1/NZDDcvMG8Vczw\n-----END PRIVATE KEY-----\n";
/// Client id the seeded mock connection records.
const MOCK_CLIENT_ID: &str = "wyrd-fixture";

/// Serve discovery and JWKS for the mock provider, and answer the token
/// endpoint with `token_response`.
async fn mount_mock_provider(
    server: &wiremock::MockServer,
    token_response: wiremock::ResponseTemplate,
) {
    mount_mock_provider_advertising(server, token_response, &["EdDSA"]).await;
}

/// [`mount_mock_provider`] whose discovery advertises `algorithms` as its
/// ID-token signing algorithms.
async fn mount_mock_provider_advertising(
    server: &wiremock::MockServer,
    token_response: wiremock::ResponseTemplate,
    algorithms: &[&str],
) {
    let discovery = mock_discovery(&server.uri(), algorithms);
    mount_mock_provider_discovering(server, token_response, discovery).await;
}

/// The mock provider's discovery document at `issuer`, advertising
/// `algorithms` and no RFC 9207 issuer-parameter support.
fn mock_discovery(issuer: &str, algorithms: &[&str]) -> Value {
    serde_json::json!({
        "issuer": issuer,
        "authorization_endpoint": format!("{issuer}/authorize"),
        "token_endpoint": format!("{issuer}/token"),
        "jwks_uri": format!("{issuer}/jwks"),
        "response_types_supported": ["code"],
        "subject_types_supported": ["public"],
        "id_token_signing_alg_values_supported": algorithms,
    })
}

/// Reset `server`, then serve `discovery`, the mock JWKS, and
/// `token_response` from the token endpoint; clearing recorded requests lets
/// a journey count the token calls of exactly one callback.
async fn mount_mock_provider_discovering(
    server: &wiremock::MockServer,
    token_response: wiremock::ResponseTemplate,
    discovery: Value,
) {
    server.reset().await;
    Mock::given(method("GET"))
        .and(path("/.well-known/openid-configuration"))
        .respond_with(ResponseTemplate::new(200).set_body_json(discovery))
        .mount(server)
        .await;
    Mock::given(method("GET"))
        .and(path("/jwks"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "keys": [{ "kty": "OKP", "crv": "Ed25519", "kid": "mock-1", "x": MOCK_SIGNING_X }]
        })))
        .mount(server)
        .await;
    Mock::given(method("POST"))
        .and(path("/token"))
        .respond_with(token_response)
        .mount(server)
        .await;
}

/// Sign `claims` as an ID token with `header` and the Ed25519 `pem`.
///
/// # Panics
/// Panics when `pem` is not a valid Ed25519 private key in PEM form, or when
/// `jsonwebtoken` cannot encode the token, such as when `header` names an
/// algorithm the Ed25519 key cannot sign with.
fn sign_id_token(header: &jsonwebtoken::Header, claims: &Value, pem: &str) -> String {
    let key = jsonwebtoken::EncodingKey::from_ed_pem(pem.as_bytes()).expect("key parses");
    jsonwebtoken::encode(header, claims, &key).expect("token signs")
}

/// A token endpoint reply carrying `id_token`.
fn id_token_reply(id_token: &str) -> wiremock::ResponseTemplate {
    wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!({
        "access_token": "provider-access",
        "token_type": "Bearer",
        "id_token": id_token,
    }))
}

/// Give `tenant` an Active connection to the mock provider at `issuer`.
///
/// # Panics
/// Panics when seeding fails.
async fn seed_mock_connection(srv: &WyrdTestServer, tenant: DataTenantId, issuer: &str) {
    let mut conn = srv
        .tenant_conn_for(tenant)
        .await
        .expect("tenant conn opens");
    let binding = wyrd_dev_fixtures::pg::seed_active_human_connection(&mut conn)
        .await
        .expect("connection seeds");
    conn.commit().await.expect("seed commits");
    sqlx::query(
        "UPDATE wyrd.auth_human_connections SET issuer_url = $1, jwks_uri = $2 \
          WHERE connection_id = $3",
    )
    .bind(issuer)
    .bind(format!("{issuer}/jwks"))
    .bind(binding.connection_id)
    .execute(
        &srv.pg_fixture()
            .superuser_pool()
            .await
            .expect("superuser pool opens"),
    )
    .await
    .expect("connection points at the mock provider");
}

/// Begin a `wyrd-ui` authorization request for `slug` and return the
/// client's PKCE verifier and the state and nonce of the provider redirect.
///
/// # Panics
/// Panics when the request does not redirect to the provider with a state
/// and nonce.
async fn begin_mock_login(srv: &WyrdTestServer, slug: &str) -> (String, String, String) {
    let (verifier, challenge) = pkce_pair();
    let (status, location) = authorize(srv, slug, &challenge).await;
    assert_eq!(status, StatusCode::SEE_OTHER, "mock login begins");
    let url = location.expect("authorize redirects to the provider");
    let query = |name: &str| {
        url.query_pairs()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.into_owned())
            .unwrap_or_else(|| panic!("the provider redirect carries {name}: {url}"))
    };
    (verifier, query("state"), query("nonce"))
}

/// Sign alice in through `keycloak` for the fixture tenant and return the
/// Wyrd authorization code the callback issued and its PKCE verifier.
///
/// # Panics
/// Panics when the sign-in or the callback fails.
async fn issued_code(srv: &WyrdTestServer, keycloak: &OidcIssuerFixture) -> (String, String) {
    let provider = authorization_code(
        srv,
        keycloak,
        PUBLIC_HUMAN_CLIENT,
        "alice",
        "alice-password",
    )
    .await;
    let reply = callback_reply(
        srv,
        &provider.code,
        &provider.state,
        provider.iss.as_deref(),
        "test-tenant-1.wyrd.test",
    )
    .await;
    (authorized_code(&reply, &provider.code), provider.verifier)
}

/// Assert a token-endpoint reply is the RFC 6749 §5.2 refusal `error` with
/// `status`, carrying no token.
///
/// # Panics
/// Panics when the reply differs.
fn assert_oauth_refusal(reply: &(StatusCode, Value), status: StatusCode, error: &str, label: &str) {
    assert_eq!(reply.0, status, "{label}: {}", reply.1);
    assert_eq!(reply.1["error"], error, "{label}: {}", reply.1);
    assert!(
        reply.1.get("access_token").is_none(),
        "{label}: a refusal serves no token"
    );
}

/// The common callback and the token endpoint refuse every unusable login and
/// issue nothing:
///   1. a wrong (tampered), unknown, replayed, or expired state is
///      `400 INVALID_STATE`;
///   2. a Wyrd authorization code yields no token for a wrong client secret
///      (`401 invalid_client` with `WWW-Authenticate`), a wrong
///      `redirect_uri`, a PKCE mismatch, or after expiry (`invalid_grant`);
///   3. a JSON token request is `invalid_request` and consumes nothing — the
///      same code still redeems, with `Cache-Control: no-store`;
///   4. a login begun for tenant B under the same issuer completes into B
///      only, whatever `Host` the callback carried;
///   5. against a mock provider: a nonce, issuer, audience, signature, or
///      algorithm mismatch, a validly signed token without `iat`, a
///      multi-audience token without `azp`, an untrusted additional audience
///      even when `azp` names the client, an `azp`
///      naming another client, a validly signed token whose algorithm
///      discovery did not advertise, an `HS256` token even when discovery
///      advertises `HS256` beside an asymmetric algorithm, and a provider
///      outage are refused back to the client (`access_denied`, or
///      `temporarily_unavailable` for the outage), leaving no code, User,
///      identity, role grant, or refresh row, while a valid single-audience
///      token whose `azp` names the client completes;
///   6. an injected audit-staging failure at redemption costs the login
///      nothing: the token and refresh row are issued, the failed audit write
///      is counted, and the decision commits once after the store recovers.
///
/// A discovered unsafe (cleartext or internal) provider URL is proven by the
/// `wyrd-auth` unit tests (`login::destination_tests`,
/// `callback::screening_tests`): the journey server is permissive toward
/// local providers so Keycloak is reachable.
///
/// # Panics
/// Panics when any step deviates from the contract above.
#[tokio::test]
#[ignore = "requires the Keycloak and Dex identity lane"]
async fn tenant_callback_refusal_journey() {
    let failures = wyrd_testing::AuditCommitFailures::install().expect("metrics recorder installs");
    let keycloak = OidcIssuerFixture::connect(&keycloak_issuer()).await;
    let srv = human_server().await;
    let superuser = srv
        .pg_fixture()
        .superuser_pool()
        .await
        .expect("superuser pool opens");
    let sign_in = || {
        authorization_code(
            &srv,
            &keycloak,
            PUBLIC_HUMAN_CLIENT,
            "alice",
            "alice-password",
        )
    };

    // 1. Wrong, unknown, replayed, and expired state.
    let provider = sign_in().await;
    let (status, body) = finish_callback(
        &srv,
        &provider.code,
        &format!("{}x", provider.state),
        provider.iss.as_deref(),
    )
    .await;
    assert_refused(
        status,
        &body,
        StatusCode::BAD_REQUEST,
        "WYRD_AUTH_400_INVALID_STATE",
    );
    let (status, body) = finish_callback(
        &srv,
        &provider.code,
        "never-issued",
        provider.iss.as_deref(),
    )
    .await;
    assert_refused(
        status,
        &body,
        StatusCode::BAD_REQUEST,
        "WYRD_AUTH_400_INVALID_STATE",
    );
    let session = complete_login(&srv, &provider).await;
    let alice = principal_id_of(session["access_token"].as_str().expect("access token"));
    let (status, body) = finish_callback(
        &srv,
        &provider.code,
        &provider.state,
        provider.iss.as_deref(),
    )
    .await;
    assert_refused(
        status,
        &body,
        StatusCode::BAD_REQUEST,
        "WYRD_AUTH_400_INVALID_STATE",
    );
    let expiring = sign_in().await;
    sqlx::query(
        "UPDATE wyrd.auth_login_state SET expires_at = now() - interval '1 second' \
          WHERE state_hash = $1",
    )
    .bind(
        Sha256Hex::digest(expiring.state.as_bytes())
            .as_bytes()
            .as_slice(),
    )
    .execute(&superuser)
    .await
    .expect("state expires");
    let (status, body) = finish_callback(
        &srv,
        &expiring.code,
        &expiring.state,
        expiring.iss.as_deref(),
    )
    .await;
    assert_refused(
        status,
        &body,
        StatusCode::BAD_REQUEST,
        "WYRD_AUTH_400_INVALID_STATE",
    );

    // 2. Code-grant negatives: each refused code is spent.
    let (code, verifier) = issued_code(&srv, &keycloak).await;
    let redirect_uri = ui_redirect();
    let grant = [
        ("grant_type", "authorization_code"),
        ("code", code.as_str()),
        ("redirect_uri", redirect_uri.as_str()),
        ("code_verifier", verifier.as_str()),
    ];
    let (status, headers, body) = token_request(
        &srv,
        "not-the-ui-secret",
        "application/x-www-form-urlencoded",
        url::form_urlencoded::Serializer::new(String::new())
            .extend_pairs(&grant)
            .finish(),
    )
    .await;
    assert_oauth_refusal(
        &(status, body),
        StatusCode::UNAUTHORIZED,
        "invalid_client",
        "wrong client secret",
    );
    assert_eq!(
        headers
            .get(header::WWW_AUTHENTICATE)
            .and_then(|value| value.to_str().ok()),
        Some("Basic realm=\"wyrd\""),
        "invalid_client names the authentication scheme"
    );
    let wrong_redirect = format!("{PUBLIC_ORIGIN}/elsewhere");
    for (label, redirect, wrong_verifier) in [
        ("wrong redirect_uri", wrong_redirect.as_str(), None),
        (
            "PKCE mismatch",
            redirect_uri.as_str(),
            Some("wrong-verifier"),
        ),
        ("expired code", redirect_uri.as_str(), None),
    ] {
        let (code, verifier) = issued_code(&srv, &keycloak).await;
        if label == "expired code" {
            sqlx::query(
                "UPDATE wyrd.auth_login_state SET expires_at = now() - interval '1 second' \
                  WHERE code_hash = $1",
            )
            .bind(Sha256Hex::digest(code.as_bytes()).as_bytes().as_slice())
            .execute(&superuser)
            .await
            .expect("code expires");
        }
        let refused = token_call(
            &srv,
            &[
                ("grant_type", "authorization_code"),
                ("code", &code),
                ("redirect_uri", redirect),
                ("code_verifier", wrong_verifier.unwrap_or(&verifier)),
            ],
        )
        .await;
        assert_oauth_refusal(&refused, StatusCode::BAD_REQUEST, "invalid_grant", label);
        let retry = redeem(&srv, &code, &verifier).await;
        assert_oauth_refusal(
            &retry,
            StatusCode::BAD_REQUEST,
            "invalid_grant",
            "a refused code is spent",
        );
    }

    // 3. A JSON body is not a token request and consumes nothing.
    let (code, verifier) = issued_code(&srv, &keycloak).await;
    let (status, _, body) = token_request(
        &srv,
        UI_CLIENT_SECRET,
        "application/json",
        serde_json::json!({
            "grant_type": "authorization_code",
            "code": code,
            "redirect_uri": ui_redirect(),
            "code_verifier": verifier,
        })
        .to_string(),
    )
    .await;
    assert_oauth_refusal(
        &(status, body),
        StatusCode::BAD_REQUEST,
        "invalid_request",
        "JSON body",
    );
    let form = url::form_urlencoded::Serializer::new(String::new())
        .append_pair("grant_type", "authorization_code")
        .append_pair("code", &code)
        .append_pair("redirect_uri", &ui_redirect())
        .append_pair("code_verifier", &verifier)
        .finish();
    let (status, headers, session) = token_request(
        &srv,
        UI_CLIENT_SECRET,
        "application/x-www-form-urlencoded",
        form,
    )
    .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "the unconsumed code redeems: {session}"
    );
    assert_eq!(session["token_type"], "Bearer", "{session}");
    for (name, value) in [
        (header::CACHE_CONTROL, "no-store"),
        (header::PRAGMA, "no-cache"),
    ] {
        assert_eq!(
            headers.get(&name).and_then(|value| value.to_str().ok()),
            Some(value),
            "a token response is never cached"
        );
    }

    // 4. Same issuer, another tenant: the state alone decides the tenant.
    let tenant_b = srv
        .seed_tenant("test-tenant-2")
        .await
        .expect("tenant B seeds");
    let admin_b = tenant_admin(&srv, tenant_b, "refusal-admin-b").await;
    activate_keycloak_connection(&srv, &admin_b, PUBLIC_HUMAN_CLIENT, "Public", None).await;
    let in_b = authorization_code_for(
        &srv,
        "test-tenant-2",
        &keycloak,
        PUBLIC_HUMAN_CLIENT,
        "alice",
        "alice-password",
    )
    .await;
    let b_session = complete_login(&srv, &in_b).await;
    let b_access = b_session["access_token"].as_str().expect("access token");
    assert_eq!(
        jwt_claims(b_access)["principal"]["tenant_id"],
        tenant_b.to_string(),
        "the session belongs to tenant B"
    );
    assert_ne!(principal_id_of(b_access), alice);

    // 5. Token verification refusals against a mock provider.
    let mock = wiremock::MockServer::start().await;
    let issuer = mock.uri();
    let tenant_c = srv
        .seed_tenant("test-tenant-3")
        .await
        .expect("tenant C seeds");
    seed_mock_connection(&srv, tenant_c, &issuer).await;
    let now = chrono::Utc::now();
    let claims = |nonce: &str| {
        serde_json::json!({
            "sub": "mock-user",
            "iss": issuer,
            "aud": MOCK_CLIENT_ID,
            "exp": (now + ChronoDuration::hours(1)).timestamp(),
            "iat": now.timestamp(),
            "nonce": nonce,
        })
    };
    let mut eddsa = jsonwebtoken::Header::new(jsonwebtoken::Algorithm::EdDSA);
    eddsa.kid = Some("mock-1".to_owned());
    let mut hs256 = jsonwebtoken::Header::new(jsonwebtoken::Algorithm::HS256);
    hs256.kid = Some("mock-1".to_owned());
    /// Builds the mock provider's token-endpoint response for one refusal
    /// case from the nonce the login began with, so each case can reply with a
    /// forged or failing token exchange.
    type Mutation<'a> = Box<dyn Fn(&str) -> wiremock::ResponseTemplate + 'a>;
    let cases: Vec<(&str, Mutation, &str)> = vec![
        (
            "nonce",
            Box::new(|_| {
                id_token_reply(&sign_id_token(
                    &eddsa,
                    &claims("other-nonce"),
                    MOCK_SIGNING_KEY,
                ))
            }),
            "access_denied",
        ),
        (
            "issuer",
            Box::new(|nonce| {
                let mut wrong = claims(nonce);
                wrong["iss"] = Value::from("https://evil.example.com");
                id_token_reply(&sign_id_token(&eddsa, &wrong, MOCK_SIGNING_KEY))
            }),
            "access_denied",
        ),
        (
            "audience",
            Box::new(|nonce| {
                let mut wrong = claims(nonce);
                wrong["aud"] = Value::from("another-client");
                id_token_reply(&sign_id_token(&eddsa, &wrong, MOCK_SIGNING_KEY))
            }),
            "access_denied",
        ),
        (
            "signature",
            Box::new(|nonce| {
                id_token_reply(&sign_id_token(&eddsa, &claims(nonce), FOREIGN_SIGNING_KEY))
            }),
            "access_denied",
        ),
        (
            "algorithm",
            Box::new(|nonce| {
                let key = jsonwebtoken::EncodingKey::from_secret(MOCK_SIGNING_X.as_bytes());
                id_token_reply(&jsonwebtoken::encode(&hs256, &claims(nonce), &key).expect("signs"))
            }),
            "access_denied",
        ),
        (
            "multi-audience token without azp",
            Box::new(|nonce| {
                let mut wrong = claims(nonce);
                wrong["aud"] = serde_json::json!([MOCK_CLIENT_ID, "another-client"]);
                id_token_reply(&sign_id_token(&eddsa, &wrong, MOCK_SIGNING_KEY))
            }),
            "access_denied",
        ),
        (
            "untrusted additional audience with azp naming the client",
            Box::new(|nonce| {
                let mut wrong = claims(nonce);
                wrong["aud"] = serde_json::json!([MOCK_CLIENT_ID, "another-client"]);
                wrong["azp"] = Value::from(MOCK_CLIENT_ID);
                id_token_reply(&sign_id_token(&eddsa, &wrong, MOCK_SIGNING_KEY))
            }),
            "access_denied",
        ),
        (
            "azp naming another client",
            Box::new(|nonce| {
                let mut wrong = claims(nonce);
                wrong["azp"] = Value::from("another-client");
                id_token_reply(&sign_id_token(&eddsa, &wrong, MOCK_SIGNING_KEY))
            }),
            "access_denied",
        ),
        (
            "missing iat",
            Box::new(|nonce| {
                let mut wrong = claims(nonce);
                wrong.as_object_mut().expect("claims object").remove("iat");
                id_token_reply(&sign_id_token(&eddsa, &wrong, MOCK_SIGNING_KEY))
            }),
            "access_denied",
        ),
        (
            "outage",
            Box::new(|_| wiremock::ResponseTemplate::new(503)),
            "temporarily_unavailable",
        ),
    ];
    for (label, reply_for, expected) in &cases {
        mount_mock_provider(&mock, id_token_reply("unused")).await;
        let (_, state, nonce) = begin_mock_login(&srv, "test-tenant-3").await;
        mount_mock_provider(&mock, reply_for(&nonce)).await;
        let error = refused_login(&srv, "mock-code", &state, None).await;
        assert_eq!(error, *expected, "{label}");
    }
    // A validly signed token whose algorithm discovery did not advertise is
    // refused by the advertised-set check; `HS256` advertised beside an
    // asymmetric algorithm (discovery requires one) passes that check and is
    // refused by the shared verifier's symmetric-algorithm guard. Discovery is
    // cached per issuer, so each advertised set is its own provider and tenant
    // C's connection is pointed at it, as a real provider change would be.
    let hmac_key = jsonwebtoken::EncodingKey::from_secret(MOCK_SIGNING_X.as_bytes());
    let algorithm_cases = [
        ("unadvertised EdDSA", &eddsa, None, &["RS256"][..]),
        (
            "advertised HS256",
            &hs256,
            Some(&hmac_key),
            &["EdDSA", "HS256"][..],
        ),
    ];
    for (label, header, hmac, advertised) in algorithm_cases {
        let provider = wiremock::MockServer::start().await;
        seed_mock_connection(&srv, tenant_c, &provider.uri()).await;
        mount_mock_provider_advertising(&provider, id_token_reply("unused"), advertised).await;
        let (_, state, nonce) = begin_mock_login(&srv, "test-tenant-3").await;
        let mut provider_claims = claims(&nonce);
        provider_claims["iss"] = Value::from(provider.uri());
        let id_token = match hmac {
            Some(key) => jsonwebtoken::encode(header, &provider_claims, key).expect("signs"),
            None => sign_id_token(header, &provider_claims, MOCK_SIGNING_KEY),
        };
        mount_mock_provider_advertising(&provider, id_token_reply(&id_token), advertised).await;
        let error = refused_login(&srv, "mock-code", &state, None).await;
        assert_eq!(error, "access_denied", "{label}");
    }
    // Every refusal above happened before identity: tenant C holds no User,
    // provider identity, role grant, or refresh row.
    let persisted: (i64, i64, i64, i64) = sqlx::query_as(
        "SELECT (SELECT count(*) FROM wyrd.auth_users WHERE data_tenant_id = $1), \
                (SELECT count(*) FROM wyrd.auth_user_identities WHERE data_tenant_id = $1), \
                (SELECT count(*) FROM wyrd.auth_user_roles WHERE data_tenant_id = $1), \
                (SELECT count(*) FROM wyrd.auth_refresh_tokens WHERE data_tenant_id = $1)",
    )
    .bind(tenant_c.as_uuid())
    .fetch_one(&superuser)
    .await
    .expect("tenant C rows read");
    assert_eq!(
        persisted,
        (0, 0, 0, 0),
        "refused logins persisted no User, identity, role grant, or refresh row"
    );
    seed_mock_connection(&srv, tenant_c, &issuer).await;
    mount_mock_provider(&mock, id_token_reply("unused")).await;
    let (verifier, state, nonce) = begin_mock_login(&srv, "test-tenant-3").await;
    let mut with_azp = claims(&nonce);
    with_azp["azp"] = Value::from(MOCK_CLIENT_ID);
    mount_mock_provider(
        &mock,
        id_token_reply(&sign_id_token(&eddsa, &with_azp, MOCK_SIGNING_KEY)),
    )
    .await;
    let reply = callback_reply(&srv, "mock-code", &state, None, "test-tenant-1.wyrd.test").await;
    let (status, body) = redeem(&srv, &authorized_code(&reply, "mock-code"), &verifier).await;
    assert_eq!(
        status,
        StatusCode::OK,
        "the unmodified mock token completes: {body}"
    );

    // 6. An audit failure at redemption does not refuse the login.
    let before = refresh_rows(&srv, &alice).await;
    let (failing_code, failing_verifier) = issued_code(&srv, &keycloak).await;
    let exchanges = || async {
        srv.wait_oracle_audit_staged(StdDuration::from_secs(30))
            .await
            .expect("audit outbox settles");
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM vala.audit_staging WHERE operation = 'auth.token.exchange'",
        )
        .fetch_one(&superuser)
        .await
        .expect("exchange decisions read")
    };
    let exchanges_before = exchanges().await;
    sqlx::query(
        r#"CREATE OR REPLACE FUNCTION vala.test_fail_login_audit()
           RETURNS trigger LANGUAGE plpgsql AS $$
           BEGIN
             IF NEW.operation = 'auth.token.exchange' THEN
               RAISE EXCEPTION 'injected login audit failure';
             END IF;
             RETURN NEW;
           END;
           $$;"#,
    )
    .execute(&superuser)
    .await
    .expect("failure function installs");
    sqlx::query(
        r#"CREATE TRIGGER test_fail_login_audit
           BEFORE INSERT ON vala.audit_staging
           FOR EACH ROW EXECUTE FUNCTION vala.test_fail_login_audit()"#,
    )
    .execute(&superuser)
    .await
    .expect("failure trigger installs");
    let (status, body) = redeem(&srv, &failing_code, &failing_verifier).await;
    assert_eq!(
        status,
        StatusCode::OK,
        "an unstageable login audit still issues the token: {body}"
    );
    assert!(
        body.get("access_token").is_some(),
        "the redemption serves a token: {body}"
    );
    failures
        .await_failure(StdDuration::from_secs(30))
        .await
        .expect("the failed audit write is counted");
    sqlx::query("DROP TRIGGER test_fail_login_audit ON vala.audit_staging")
        .execute(&superuser)
        .await
        .expect("failure trigger drops");
    assert_eq!(
        refresh_rows(&srv, &alice).await,
        before + 1,
        "the login inserted its refresh row"
    );
    assert_eq!(
        exchanges().await,
        exchanges_before + 1,
        "the login decision commits exactly once after recovery"
    );

    srv.shutdown().await.expect("server shuts down");
}

/// Serve a mock provider that signs any user in at once under `discovery`:
/// the mock JWKS, `token_response` from the token endpoint, and an
/// authorization endpoint redirecting to the common callback with
/// `mock-code` and the request's state.
async fn mount_authorizing_provider(
    server: &wiremock::MockServer,
    discovery: Value,
    token_response: wiremock::ResponseTemplate,
) {
    mount_mock_provider_discovering(server, token_response, discovery).await;
    Mock::given(method("GET"))
        .and(path("/authorize"))
        .respond_with(|request: &wiremock::Request| {
            let state = request
                .url
                .query_pairs()
                .find(|(key, _)| key == "state")
                .map(|(_, value)| value.into_owned())
                .unwrap_or_default();
            let mut location: Url = format!("{PUBLIC_ORIGIN}/auth/callback")
                .parse()
                .expect("callback parses");
            location
                .query_pairs_mut()
                .append_pair("code", "mock-code")
                .append_pair("state", &state);
            ResponseTemplate::new(302).insert_header("location", location.as_str())
        })
        .mount(server)
        .await;
}

/// Token-endpoint requests `server` recorded since its last reset.
///
/// # Panics
/// Panics when the mock does not record requests.
async fn token_calls(server: &wiremock::MockServer) -> usize {
    server
        .received_requests()
        .await
        .expect("the mock records requests")
        .iter()
        .filter(|request| request.method == Method::POST && request.url.path() == "/token")
        .count()
}

/// One issuer-binding callback case: its label, the provider and its
/// discovery document, extra provider response parameters, the `iss` sent (if
/// any), and whether the login must complete.
type IssuerCase<'a> = (
    &'a str,
    &'a wiremock::MockServer,
    &'a Value,
    &'a [(&'a str, &'a str)],
    Option<&'a str>,
    bool,
);

/// The common callback binds every authorization response to the issuer its
/// login state recorded (RFC 9207), provider-agnostically:
///   1. a provider whose discovery does not advertise
///      `authorization_response_iss_parameter_supported` is tested and
///      activated through the served connection API;
///   2. against it, a response without `iss`, with the login's exact `iss`,
///      and with an unrelated `session_state` parameter each complete;
///   3. a response whose `iss` names another issuer — including a
///      trailing-slash variant of the login's issuer — is refused
///      `401 INVALID_TOKEN` and audited as a denied exchange, with no
///      token-endpoint request and no completion;
///   4. once the connection names a provider that advertises support, a
///      response without `iss` is refused the same way, and one with the exact
///      `iss` completes. Discovery is cached per issuer, so the advertising
///      provider is a second issuer the Active connection is pointed at.
///
/// # Panics
/// Panics when any step deviates from the contract above.
#[tokio::test]
#[ignore = "runs in the identity journey lane"]
async fn tenant_callback_issuer_binding_journey() {
    let srv = human_server_builder()
        .start_in_process()
        .await
        .expect("test server starts");
    let tenant = srv.data_tenant_id();
    let superuser = srv
        .pg_fixture()
        .superuser_pool()
        .await
        .expect("superuser pool opens");
    let mock = wiremock::MockServer::start().await;
    let issuer = mock.uri();
    let silent = mock_discovery(&issuer, &["EdDSA"]);
    let advertising_mock = wiremock::MockServer::start().await;
    let advertising_issuer = advertising_mock.uri();
    let mut advertising = mock_discovery(&advertising_issuer, &["EdDSA"]);
    advertising["authorization_response_iss_parameter_supported"] = Value::Bool(true);

    let mut eddsa = jsonwebtoken::Header::new(jsonwebtoken::Algorithm::EdDSA);
    eddsa.kid = Some("mock-1".to_owned());
    let id_token_for = |issuer: &str, nonce: &str| {
        let now = chrono::Utc::now();
        id_token_reply(&sign_id_token(
            &eddsa,
            &serde_json::json!({
                "sub": "issuer-binding-user",
                "iss": issuer,
                "aud": MOCK_CLIENT_ID,
                "exp": (now + ChronoDuration::hours(1)).timestamp(),
                "iat": now.timestamp(),
                "nonce": nonce,
            }),
            MOCK_SIGNING_KEY,
        ))
    };

    // 1. A provider that does not advertise issuer support is tested through
    //    a real sign-in (its return carries no `iss`) and activated.
    let admin = tenant_admin(&srv, tenant, "issuer-binding-admin").await;
    mount_mock_provider_discovering(&mock, id_token_reply("unused"), silent.clone()).await;
    let (status, candidate) = call_json(
        &srv,
        &admin.token,
        Method::PUT,
        CANDIDATE,
        Some(serde_json::json!({
            "issuer": issuer,
            "client_id": MOCK_CLIENT_ID,
            "client_auth": "Public",
            "claim_mapping": { "subject": "sub" },
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "candidate stages: {candidate}");
    let (status, begun) = begin_test(&srv, &admin.token, 1).await;
    assert_eq!(status, StatusCode::OK, "the test begins: {begun}");
    let test_nonce = Url::parse(begun["authorization_url"].as_str().expect("url"))
        .expect("authorization url parses")
        .query_pairs()
        .find(|(name, _)| name == "nonce")
        .map(|(_, value)| value.into_owned())
        .expect("the test sign-in carries a nonce");
    mount_authorizing_provider(&mock, silent.clone(), id_token_for(&issuer, &test_nonce)).await;
    assert_test_completion(&finish_test_sign_in(&srv, &begun).await);
    let (status, active) = call_json(
        &srv,
        &admin.token,
        Method::POST,
        CANDIDATE_ACTIVATE,
        Some(activation(1, &admin.api_key)),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "candidate activates: {active}");

    let denied = || async {
        srv.wait_oracle_audit_staged(StdDuration::from_secs(30))
            .await
            .expect("audit outbox settles");
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM vala.audit_staging \
              WHERE data_tenant_id = $1 AND operation = 'auth.token.exchange' \
                AND outcome = 'denied'",
        )
        .bind(tenant.as_uuid())
        .fetch_one(&superuser)
        .await
        .expect("denied exchanges read")
    };
    let wrong_slash = format!("{issuer}/");
    let cases: Vec<IssuerCase> = vec![
        ("unadvertised, no iss", &mock, &silent, &[], None, true),
        (
            "unadvertised, exact iss",
            &mock,
            &silent,
            &[],
            Some(&issuer),
            true,
        ),
        (
            "unadvertised, unrelated session_state",
            &mock,
            &silent,
            &[("session_state", "provider-session")],
            Some(&issuer),
            true,
        ),
        (
            "unadvertised, foreign iss",
            &mock,
            &silent,
            &[],
            Some("https://evil.example.com"),
            false,
        ),
        (
            "unadvertised, trailing-slash iss",
            &mock,
            &silent,
            &[],
            Some(&wrong_slash),
            false,
        ),
        (
            "advertised, no iss",
            &advertising_mock,
            &advertising,
            &[],
            None,
            false,
        ),
        (
            "advertised, foreign iss",
            &advertising_mock,
            &advertising,
            &[],
            Some("https://evil.example.com"),
            false,
        ),
        (
            "advertised, exact iss",
            &advertising_mock,
            &advertising,
            &[],
            Some(&advertising_issuer),
            true,
        ),
    ];
    for (label, provider, discovery, extra, iss, completes) in cases {
        let provider_issuer = provider.uri();
        seed_mock_connection(&srv, tenant, &provider_issuer).await;
        mount_mock_provider_discovering(provider, id_token_reply("unused"), discovery.clone())
            .await;
        let (verifier, state, nonce) = begin_mock_login(&srv, FIXTURE_TENANT_SLUG).await;
        mount_mock_provider_discovering(
            provider,
            id_token_for(&provider_issuer, &nonce),
            discovery.clone(),
        )
        .await;
        let denied_before = denied().await;
        let mut params = vec![("code", "mock-code"), ("state", state.as_str())];
        params.extend_from_slice(extra);
        let reply = callback_reply_with(&srv, &params, iss, "test-tenant-1.wyrd.test").await;
        if completes {
            let code = authorized_code(&reply, "mock-code");
            assert_eq!(token_calls(provider).await, 1, "{label}: one code exchange");
            let (status, body) = redeem(&srv, &code, &verifier).await;
            assert_eq!(status, StatusCode::OK, "{label}: the code redeems: {body}");
        } else {
            assert_eq!(callback_refusal(&reply), "access_denied", "{label}");
            assert_eq!(
                token_calls(provider).await,
                0,
                "{label}: the code never reaches a token endpoint"
            );
            assert_eq!(
                denied().await,
                denied_before + 1,
                "{label}: refusal audited"
            );
            let (status, body) = finish_callback(&srv, "mock-code", &state, iss).await;
            assert_refused(
                status,
                &body,
                StatusCode::BAD_REQUEST,
                "WYRD_AUTH_400_INVALID_STATE",
            );
        }
    }

    srv.shutdown().await.expect("server shuts down");
}

/// Sign Keycloak's `alice` in at a begun test's authorization URL and return
/// every query parameter of the provider's return to the deployment
/// callback, without presenting it.
///
/// # Panics
/// Panics when the body carries no authorization URL or the sign-in does not
/// return to the deployment callback.
async fn test_sign_in_return(begun: &Value) -> Vec<(String, String)> {
    let authorization_url: Url = begun["authorization_url"]
        .as_str()
        .unwrap_or_else(|| panic!("a begun test returns an authorization URL: {begun}"))
        .parse()
        .expect("authorization URL parses");
    provider_sign_in(
        &authorization_url,
        "alice",
        "alice-password",
        &format!("{PUBLIC_ORIGIN}/auth/callback"),
    )
    .await
    .query_pairs()
    .into_owned()
    .collect()
}

/// Present `params` to the common callback, replacing `state` with
/// `state_override` when one is given and keeping every other parameter.
async fn present_test_return(
    srv: &WyrdTestServer,
    params: &[(String, String)],
    state_override: Option<&str>,
) -> CallbackReply {
    let params: Vec<(&str, &str)> = params
        .iter()
        .map(|(name, value)| match (name.as_str(), state_override) {
            ("state", Some(state)) => ("state", state),
            (name, _) => (name, value.as_str()),
        })
        .collect();
    callback_reply_with(srv, &params, None, "test-tenant-1.wyrd.test").await
}

/// The `state` of a provider return.
///
/// # Panics
/// Panics when the return carries no state.
fn returned_state(params: &[(String, String)]) -> &str {
    params
        .iter()
        .find(|(name, _)| name == "state")
        .map(|(_, value)| value.as_str())
        .expect("the provider return carries state")
}

/// Stage a public Keycloak candidate for `admin`'s tenant and begin its test,
/// returning the candidate revision and the begun test body.
///
/// # Panics
/// Panics when staging or beginning the test does not return `200`.
async fn begin_keycloak_test(srv: &WyrdTestServer, admin: &TenantAdmin) -> (u64, Value) {
    let (_, listed) = call_json(srv, &admin.token, Method::GET, CONNECTIONS, None).await;
    let (status, staged) = call_json(
        srv,
        &admin.token,
        Method::PUT,
        CANDIDATE,
        Some(connection_input(
            PUBLIC_HUMAN_CLIENT,
            "Public",
            None,
            listed["candidate"]["revision"].as_u64(),
        )),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "candidate stages: {staged}");
    let revision = staged["revision"].as_u64().expect("candidate revision");
    let (status, begun) = begin_test(srv, &admin.token, revision).await;
    assert_eq!(status, StatusCode::OK, "the test begins: {begun}");
    (revision, begun)
}

/// Whether `admin`'s tenant candidate carries an unexpired test stamp.
async fn candidate_tested(srv: &WyrdTestServer, admin: &TenantAdmin) -> bool {
    let (_, listed) = call_json(srv, &admin.token, Method::GET, CONNECTIONS, None).await;
    !listed["candidate"]["tested_until"].is_null()
}

/// A candidate is tested by one real provider sign-in through the common
/// callback, with login state's protections and nothing issued:
///   1. tenant A's genuine provider return presented under tenant B's test
///      state is refused, and neither candidate is tested;
///   2. an expired test state is refused `400 INVALID_STATE` and leaves the
///      candidate untested;
///   3. a completed test sign-in returns the static test page, marks the
///      candidate tested, records one allowed `identity.oidc.candidate.tested`
///      decision, and creates no User, refresh token, API key, browser
///      session, or completion;
///   4. replaying that return is refused `400 INVALID_STATE`;
///   5. on a deployment whose callback the provider does not register, the
///      provider refuses the sign-in and never returns to Wyrd, so the
///      candidate stays untested and cannot be activated.
///
/// A wrong client secret failing the test is proven by
/// `tenant_connection_rotation_journey`.
///
/// # Panics
/// Panics when any step deviates from the contract above.
#[tokio::test]
#[ignore = "requires the Keycloak and Dex identity lane"]
async fn tenant_connection_test_sign_in_journey() {
    let srv = human_server_builder()
        .start_in_process()
        .await
        .expect("test server starts");
    let tenant_a = srv.data_tenant_id();
    let tenant_b = srv
        .seed_tenant("test-sign-in-b")
        .await
        .expect("tenant B seeds");
    let admin_a = tenant_admin(&srv, tenant_a, "test-sign-in-admin-a").await;
    let admin_b = tenant_admin(&srv, tenant_b, "test-sign-in-admin-b").await;
    let superuser = srv
        .pg_fixture()
        .superuser_pool()
        .await
        .expect("superuser pool opens");
    let issued = || async {
        sqlx::query_as::<_, (i64, i64, i64, i64)>(
            "SELECT (SELECT count(*) FROM wyrd.auth_users WHERE auth_type = 'oidc'),
                    (SELECT count(*) FROM wyrd.auth_refresh_tokens),
                    (SELECT count(*) FROM wyrd.auth_api_keys),
                    (SELECT count(*) FROM wyrd.auth_login_state
                      WHERE code_hash IS NOT NULL)",
        )
        .fetch_one(&superuser)
        .await
        .expect("issued state reads")
    };
    let before = issued().await;

    // 1. Cross-tenant: A's genuine return under B's state is refused.
    let (_, begun_a) = begin_keycloak_test(&srv, &admin_a).await;
    let (_, begun_b) = begin_keycloak_test(&srv, &admin_b).await;
    let return_a = test_sign_in_return(&begun_a).await;
    let return_b = test_sign_in_return(&begun_b).await;
    let mixed = present_test_return(&srv, &return_a, Some(returned_state(&return_b))).await;
    assert_refused(
        mixed.status,
        &mixed.problem(),
        StatusCode::UNAUTHORIZED,
        "WYRD_AUTH_401_INVALID_TOKEN",
    );
    assert!(!candidate_tested(&srv, &admin_a).await, "A stays untested");
    assert!(!candidate_tested(&srv, &admin_b).await, "B stays untested");

    // 2. Expired: a test state past its expiry is refused.
    let (_, begun) = begin_keycloak_test(&srv, &admin_a).await;
    let expired = test_sign_in_return(&begun).await;
    sqlx::query(
        "UPDATE wyrd.auth_login_state SET expires_at = now() - interval '1 second' \
          WHERE state_hash = $1",
    )
    .bind(
        Sha256Hex::digest(returned_state(&expired).as_bytes())
            .as_bytes()
            .as_slice(),
    )
    .execute(&superuser)
    .await
    .expect("the test state expires");
    let reply = present_test_return(&srv, &expired, None).await;
    assert_refused(
        reply.status,
        &reply.problem(),
        StatusCode::BAD_REQUEST,
        "WYRD_AUTH_400_INVALID_STATE",
    );
    assert!(!candidate_tested(&srv, &admin_a).await, "A stays untested");

    // 3. A completed test sign-in marks the candidate tested and issues nothing.
    let (revision, begun) = begin_keycloak_test(&srv, &admin_a).await;
    let genuine = test_sign_in_return(&begun).await;
    assert_test_completion(&present_test_return(&srv, &genuine, None).await);
    let (_, listed) = call_json(&srv, &admin_a.token, Method::GET, CONNECTIONS, None).await;
    assert_eq!(listed["candidate"]["tested_revision"], revision);
    let decisions: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM vala.audit_staging \
          WHERE data_tenant_id = $1 AND operation = 'identity.oidc.candidate.tested' \
            AND outcome = 'allowed'",
    )
    .bind(tenant_a.as_uuid())
    .fetch_one(&superuser)
    .await
    .expect("tested decisions read");
    assert_eq!(decisions, 1, "the tester's authority is decided once");
    assert_eq!(
        issued().await,
        before,
        "a test issues no User, credential, session, or authorization code"
    );

    // 4. Replay: the consumed test state is refused.
    let replayed = present_test_return(&srv, &genuine, None).await;
    assert_refused(
        replayed.status,
        &replayed.problem(),
        StatusCode::BAD_REQUEST,
        "WYRD_AUTH_400_INVALID_STATE",
    );
    srv.shutdown().await.expect("server shuts down");

    // 5. Wrong callback: the provider refuses an unregistered redirect.
    let unregistered = WyrdTestServerBuilder::default()
        .with_public_origin(
            "http://unregistered.wyrd.test"
                .parse()
                .expect("public origin parses"),
        )
        .start_in_process()
        .await
        .expect("unregistered server starts");
    let admin = tenant_admin(&unregistered, unregistered.data_tenant_id(), "unregistered").await;
    let (revision, begun) = begin_keycloak_test(&unregistered, &admin).await;
    let refused = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .expect("client builds")
        .get(begun["authorization_url"].as_str().expect("url"))
        .send()
        .await
        .expect("provider answers");
    assert_eq!(
        refused.status(),
        StatusCode::BAD_REQUEST,
        "the provider refuses an unregistered callback"
    );
    assert!(refused.headers().get(header::LOCATION).is_none());
    assert!(!candidate_tested(&unregistered, &admin).await);
    let (status, body) = call_json(
        &unregistered,
        &admin.token,
        Method::POST,
        CANDIDATE_ACTIVATE,
        Some(activation(revision, &admin.api_key)),
    )
    .await;
    assert_refused(
        status,
        &body,
        StatusCode::CONFLICT,
        "WYRD_AUTH_409_CONNECTION_NOT_TESTED",
    );
    unregistered.shutdown().await.expect("server shuts down");
}

/// Switching a tenant's provider ends the old provider's sessions and grants
/// nothing across providers:
///   1. alice signs in through the first realm; her access token lives at
///      most five minutes (`exp - iat <= 300`), which bounds how long a
///      pre-switch access token outlives the switch;
///   2. the administrator tests and activates the second realm with no group
///      mapping, and the pre-switch refresh token is refused;
///   3. the second realm's alice — same email — becomes a separate user with
///      no grants, and her delegated write is denied;
///   4. a mapping change applies at her next issuance;
///   5. the owner's recovery API key still authenticates and administers.
///
/// # Panics
/// Panics when any step deviates from the contract above.
#[tokio::test]
#[ignore = "requires the Keycloak and Dex identity lane"]
async fn tenant_provider_switch_journey() {
    let first = OidcIssuerFixture::connect(&keycloak_issuer()).await;
    let second = OidcIssuerFixture::connect(&keycloak_second_issuer()).await;
    let srv = human_server_builder()
        .start_in_process()
        .await
        .expect("test server starts");
    let tenant = srv.data_tenant_id();
    let admin = tenant_admin(&srv, tenant, "switch-admin").await;
    activate_keycloak_connection(&srv, &admin, PUBLIC_HUMAN_CLIENT, "Public", None).await;

    // 1. A first-provider session with a bounded access token.
    let before = human_login(&srv, &first, PUBLIC_HUMAN_CLIENT, "alice", "alice-password").await;
    let before_access = before["access_token"].as_str().expect("access token");
    let claims = jwt_claims(before_access);
    let lifetime = claims["exp"].as_i64().expect("exp") - claims["iat"].as_i64().expect("iat");
    assert!(
        lifetime <= 300,
        "access tokens live at most five minutes: {lifetime}"
    );
    let first_alice = principal_id_of(before_access);

    // 2. Switch providers; the old refresh token is refused.
    activate_connection(&srv, &admin, second_provider_input(&HashMap::new())).await;
    assert_refresh_cut_off(&srv, &before, "provider switch").await;

    // 3. Same email, separate user, no inherited grants.
    let switched = human_login(
        &srv,
        &second,
        PUBLIC_HUMAN_CLIENT,
        "alice",
        "alice-password",
    )
    .await;
    let switched_access = switched["access_token"].as_str().expect("access token");
    let second_alice = principal_id_of(switched_access);
    assert_ne!(second_alice, first_alice, "a new issuer is a new user");
    assert_eq!(user_role_count(&srv, &second_alice).await, 0);
    assert_eq!(
        delegated_write_decision(&srv, switched_access, "switch-unmapped").await,
        StatusCode::FORBIDDEN,
        "the second provider's alice inherits nothing"
    );
    let superuser = srv
        .pg_fixture()
        .superuser_pool()
        .await
        .expect("superuser pool opens");
    let same_email: i64 =
        sqlx::query_scalar("SELECT count(*) FROM wyrd.auth_users WHERE email = 'alice@wyrd.test'")
            .fetch_one(&superuser)
            .await
            .expect("users read");
    assert_eq!(same_email, 2, "both providers' alice exist separately");

    // 4. A mapping change applies at the next issuance.
    activate_connection(&srv, &admin, second_provider_input(&admins_write())).await;
    let remapped = human_login(
        &srv,
        &second,
        PUBLIC_HUMAN_CLIENT,
        "alice",
        "alice-password",
    )
    .await;
    let remapped_access = remapped["access_token"].as_str().expect("access token");
    assert_eq!(principal_id_of(remapped_access), second_alice);
    assert_v1_delegated_write_ok(&srv, remapped_access, "switch-remapped").await;

    // 5. The owner's API key still recovers administration.
    let recovered = srv
        .exchange_api_key(&admin.api_key)
        .await
        .expect("the owner's key still exchanges");
    let (status, listed) = call_json(&srv, &recovered, Method::GET, CONNECTIONS, None).await;
    assert_eq!(status, StatusCode::OK, "the owner administers: {listed}");
    assert_eq!(
        listed["active"]["issuer"]
            .as_str()
            .map(|issuer| issuer.trim_end_matches('/')),
        Some(keycloak_second_issuer().as_str())
    );

    srv.shutdown().await.expect("server shuts down");
}

/// A workload binding entry for `subject` bound to the Service card `name`.
fn machine_binding(subject: &str, audience: Option<&str>, name: &str) -> WorkloadBindingEntry {
    WorkloadBindingEntry {
        issuer: keycloak_issuer(),
        subject: subject.to_owned(),
        audience: audience.map(ToOwned::to_owned),
        kind: "service".to_owned(),
        name: name.to_owned(),
        space: "prod".to_owned(),
        version: "1.0.0".to_owned(),
    }
}

/// Assert a jwt-bearer refusal: the RFC 6749 §5.2 `400 invalid_grant` and
/// no token.
///
/// # Panics
/// Panics when the exchange is accepted or refused differently.
async fn assert_jwt_bearer_refused(response: axum::http::Response<Body>, label: &str) {
    let status = response.status();
    let bytes = to_bytes(response.into_body(), 65_536)
        .await
        .expect("body reads");
    let body: Value = serde_json::from_slice(&bytes).unwrap_or_default();
    assert_eq!(
        (status, &body["error"]),
        (StatusCode::BAD_REQUEST, &Value::from("invalid_grant")),
        "{label}: refused: {body}"
    );
    assert!(body.get("access_token").is_none(), "{label}: no token");
}

/// Machine credentials are independent of tenant SSO: while the tenant's
/// human connection is Active,
///   1. a human still signs in (SSO is live);
///   2. a tenant API key exchanges and authorizes;
///   3. an exactly bound workload assertion exchanges and authorizes;
///   4. an assertion from another issuer, an unbound subject of the trusted
///      issuer, a bound subject whose token carries another audience, and the
///      exact assertion presented to another tenant are all refused.
///
/// # Panics
/// Panics when any step deviates from the contract above.
#[tokio::test]
#[ignore = "requires the Keycloak and Dex identity lane"]
async fn tenant_machine_independence_journey() {
    let keycloak = OidcIssuerFixture::connect(&keycloak_issuer()).await;
    let other_issuer = OidcIssuerFixture::connect(&keycloak_second_issuer()).await;
    let bound = keycloak
        .workload_token("wyrd-workload", "wyrd-workload-secret", "wyrd-workload")
        .await;
    let peer = keycloak
        .workload_token(
            "wyrd-workload-peer",
            "wyrd-workload-peer-secret",
            "wyrd-workload",
        )
        .await;
    let foreign_audience = keycloak
        .workload_token(
            "wyrd-workload-foreign",
            "wyrd-workload-foreign-secret",
            "wyrd-foreign",
        )
        .await;
    let wrong_issuer = other_issuer
        .workload_token("wyrd-workload", "wyrd-workload-secret", "wyrd-workload")
        .await;
    let subject_of = |jwt: &str| {
        jwt_claims(jwt)["sub"]
            .as_str()
            .expect("workload token carries a subject")
            .to_owned()
    };
    let srv = human_server_builder()
        .with_trusted_issuer_configs(vec![keycloak_workload_issuer()])
        .with_workload_binding_configs(vec![
            machine_binding(&subject_of(&bound), Some("wyrd-workload"), "machine-sa"),
            machine_binding(&subject_of(&foreign_audience), None, "foreign-sa"),
        ])
        .start_in_process()
        .await
        .expect("server boots");
    let tenant = srv.data_tenant_id();
    // The foreign-audience token is refused at the audience check before any
    // principal resolution, so only the accepted workload needs a principal.
    srv.seed_card_principal(
        &binding_card_ref(CardKind::Service, "machine-sa", "prod"),
        &["writer"],
    )
    .await
    .expect("workload principal seeds");
    let admin = tenant_admin(&srv, tenant, "machine-admin").await;
    activate_keycloak_connection(&srv, &admin, PUBLIC_HUMAN_CLIENT, "Public", None).await;

    // 1. SSO is live.
    let human = human_login(
        &srv,
        &keycloak,
        PUBLIC_HUMAN_CLIENT,
        "alice",
        "alice-password",
    )
    .await;
    assert!(human["access_token"].is_string());

    // 2. An API key is unaffected.
    let keyed = srv
        .bootstrap_service("machine-key", &["writer"])
        .await
        .expect("keyed service bootstraps");
    let keyed_token = srv
        .exchange_api_key(keyed.api_key().expect("service has a key"))
        .await
        .expect("api key exchanges");
    assert_v1_delegated_write_ok(&srv, &keyed_token, "machine-api-key").await;

    // 3. The exact workload binding is unaffected.
    let response = post_jwt_bearer(&srv, &bound).await;
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "bound workload exchanges"
    );
    let bytes = to_bytes(response.into_body(), 65_536)
        .await
        .expect("body reads");
    let body: Value = serde_json::from_slice(&bytes).expect("token JSON");
    assert_v1_delegated_write_ok(
        &srv,
        body["access_token"].as_str().expect("access token"),
        "machine-workload",
    )
    .await;

    // 4. Every inexact assertion is refused.
    assert_jwt_bearer_refused(post_jwt_bearer(&srv, &wrong_issuer).await, "wrong issuer").await;
    assert_jwt_bearer_refused(post_jwt_bearer(&srv, &peer).await, "wrong subject").await;
    assert_jwt_bearer_refused(
        post_jwt_bearer(&srv, &foreign_audience).await,
        "wrong audience",
    )
    .await;
    srv.seed_tenant("test-tenant-2")
        .await
        .expect("tenant B seeds");
    assert_jwt_bearer_refused(
        post_jwt_bearer_for_tenant(&srv, &bound, "test-tenant-2").await,
        "wrong tenant",
    )
    .await;

    srv.shutdown().await.expect("server shuts down");
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
///   5. the exchanged Service token reaches a real `/v1/cards` 200,
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

    // Terminal: the exchanged Service token reaches a real /v1/cards 200.
    assert_v1_delegated_write_ok(&srv, wyrd_token, "cloud-journey").await;

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
    assert_v1_delegated_write_ok(srv, refreshed.expose(), "cloud-journey-client").await;
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
    assert_jwt_bearer_refused(
        post_jwt_bearer_for_tenant(&srv, &assertion, TENANT_B_SLUG).await,
        "tenant B fails closed for the unbound subject",
    )
    .await;

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

    assert_jwt_bearer_refused(post_jwt_bearer(&srv, &token).await, "untrusted issuer").await;
}

/// An authorization request for a route key naming no tenant is refused
/// back to the client with `access_denied`; no request header plays a part.
///
/// # Panics
/// Panics when the server fails to start or the refusal differs.
#[tokio::test]
#[ignore = "requires the Keycloak and Dex identity lane"]
async fn conformance_login_rejects_bare_localhost_host() {
    let srv = human_server_builder()
        .start_in_process()
        .await
        .expect("test server starts");

    let (_, challenge) = pkce_pair();
    let (status, location) = authorize(&srv, "no-such-tenant", &challenge).await;
    assert_eq!(status, StatusCode::SEE_OTHER, "authorize redirects");
    assert_eq!(
        client_refusal(location.expect("a refusal redirects").as_str()),
        "access_denied"
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
