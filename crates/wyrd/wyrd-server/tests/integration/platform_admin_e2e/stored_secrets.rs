//! Wrapping-key lifecycle journeys: activation, explicit maintenance passes,
//! recovery, interruption, denial, races, and containment, against a real
//! server, Vault Transit fixture, and repository Postgres.

use axum::http::{Method, StatusCode};
use serde_json::{Value, json};
use wyrd_testing::WyrdTestServer;
use wyrd_testing::kms::TransitFixture;

use super::{
    PLATFORM_CLIENT, PLATFORM_CLIENT_SECRET, body_json, confidential_platform_sign_in,
    initialize_platform_root, mount_platform_provider, platform_key_set, platform_post,
    platform_request, platform_session, tenant_request, tenant_token,
};

/// Every stored envelope with its store and owner, in a stable order.
const STORED_ENVELOPES: &str = "SELECT store, owner, envelope FROM (
      SELECT 'human_connection' AS store, connection_id::text AS owner,
             client_secret_enc AS envelope
        FROM wyrd.auth_human_connections WHERE client_secret_enc IS NOT NULL
      UNION ALL SELECT 'trusted_issuer', data_tenant_id::text || ' ' || issuer_url,
             client_secret_enc
        FROM wyrd.auth_trusted_issuers WHERE client_secret_enc IS NOT NULL
      UNION ALL SELECT 'platform_oidc', issuer_url, client_secret_enc
        FROM platform.oidc_connection WHERE client_secret_enc IS NOT NULL
      UNION ALL SELECT 'gateway_credential', data_tenant_id::text || ' ' || name,
             secret_ciphertext
        FROM wyrd.gateway_provider_credentials WHERE secret_ciphertext IS NOT NULL
      UNION ALL SELECT 'operator_connection', connection_id::text, secret_ciphertext
        FROM wyrd.operator_connections
    ) stored ORDER BY store, owner";

/// One stored envelope: its store, owner, and bytes.
type Stored = (String, String, Vec<u8>);

/// Read every stored envelope.
///
/// # Panics
/// Panics when the read fails.
async fn stored_envelopes(srv: &WyrdTestServer) -> Vec<Stored> {
    sqlx::query_as(STORED_ENVELOPES)
        .fetch_all(srv.operator_pool().pool())
        .await
        .expect("stored envelopes read")
}

/// Split an envelope into the wrapping-key reference it names and its payload
/// tail (nonce and ciphertext), per the documented envelope layout.
///
/// # Panics
/// Panics when `envelope` is shorter than its declared fields.
fn envelope_parts(envelope: &[u8]) -> (&str, &[u8]) {
    let ref_len = usize::from(u16::from_be_bytes([envelope[6], envelope[7]]));
    let key_ref = std::str::from_utf8(&envelope[8..8 + ref_len]).expect("key reference is UTF-8");
    let at = 8 + ref_len;
    let wrapped: [u8; 4] = envelope[at..at + 4].try_into().expect("wrapped length");
    let wrapped_len = usize::try_from(u32::from_be_bytes(wrapped)).expect("fits");
    (key_ref, &envelope[at + 4 + wrapped_len..])
}

/// The wrapping-key reference each stored envelope names.
fn key_refs(stored: &[Stored]) -> Vec<&str> {
    stored
        .iter()
        .map(|(_, _, envelope)| envelope_parts(envelope).0)
        .collect()
}

/// Send `method uri` with an optional JSON body as `token` and decode the
/// answer.
///
/// # Panics
/// Panics when the route does not respond or the body is not JSON.
async fn call(
    srv: &WyrdTestServer,
    token: &str,
    method: Method,
    uri: &str,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let resp = srv
        .oneshot_authenticated(token, tenant_request(method, uri, body))
        .await
        .expect("route responds");
    let status = resp.status();
    let body = to_bytes_json(resp).await;
    (status, body)
}

/// Decode a JSON body, or `null` for an empty one.
async fn to_bytes_json(resp: axum::http::Response<axum::body::Body>) -> Value {
    let bytes = axum::body::to_bytes(resp.into_body(), 1 << 20)
        .await
        .expect("read body");
    if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).expect("decode body")
    }
}

/// A deployment with a secret in every store it seeds.
struct Seeded {
    /// The deployment root credential; exchange it again after a restart.
    root: secrecy::SecretString,
    /// Administrator credential of each seeded tenant.
    admins: Vec<String>,
}

impl Seeded {
    /// A fresh platform session on `srv`.
    async fn session(&self, srv: &WyrdTestServer) -> String {
        platform_session(srv, secrecy::ExposeSecret::expose_secret(&self.root)).await
    }

    /// A fresh access token for the `index`th tenant administrator on `srv`.
    ///
    /// # Panics
    /// Panics when the credential no longer exchanges.
    async fn admin(&self, srv: &WyrdTestServer, index: usize) -> String {
        tenant_token(srv, &self.admins[index])
            .await
            .expect("the tenant administrator authenticates")
    }
}

/// Initialize the deployment, configure confidential platform sign-in
/// against `provider`, and give each of `slugs` a Gateway managed credential
/// and an Operator connection.
///
/// # Panics
/// Panics when any seeding request is refused.
async fn seed(srv: &WyrdTestServer, provider: &wiremock::MockServer, slugs: &[&str]) -> Seeded {
    let root = initialize_platform_root(srv)
        .await
        .expect("deployment initializes");
    let session = platform_session(srv, secrecy::ExposeSecret::expose_secret(&root)).await;
    mount_platform_provider(
        provider,
        platform_key_set("mock-1"),
        wiremock::ResponseTemplate::new(500),
    )
    .await;
    let resp = srv
        .oneshot(platform_request(
            Method::PUT,
            "/platform/oidc/connection",
            &session,
            Some(json!({
                "issuer_url": provider.uri(),
                "client_id": PLATFORM_CLIENT,
                "client_auth": { "method": "secret_post", "secret": PLATFORM_CLIENT_SECRET },
            })),
        ))
        .await
        .expect("configure route responds");
    assert_eq!(
        resp.status(),
        StatusCode::OK,
        "platform connection configures"
    );
    let resp = srv
        .oneshot(platform_request(
            Method::POST,
            "/platform/admins",
            &session,
            Some(json!({ "name": "ops-lead", "match_claim": "ops@example.com" })),
        ))
        .await
        .expect("register route responds");
    assert_eq!(resp.status(), StatusCode::OK, "administrator registers");

    let mut admins = Vec::new();
    for slug in slugs {
        let created = body_json(
            srv.oneshot(platform_post(
                "/platform/tenants",
                &session,
                json!({ "slug": slug, "display_name": slug }),
            ))
            .await
            .expect("tenant route responds"),
        )
        .await;
        let credential = created["admin"]["credential"]
            .as_str()
            .expect("admin credential")
            .to_owned();
        let admin = tenant_token(srv, &credential)
            .await
            .expect("the tenant administrator authenticates");
        let (status, body) = call(
            srv,
            &admin,
            Method::PUT,
            "/v1/admin/gateway/provider-credentials/managed",
            Some(json!({
                "name": "managed",
                "provider": "acme",
                "source": { "managed_secret": { "secret": format!("sk-{slug}") } },
            })),
        )
        .await;
        assert!(status.is_success(), "gateway credential stores: {body}");
        let (status, body) = call(
            srv,
            &admin,
            Method::POST,
            "/v1/operator-connections",
            Some(json!({
                "provider": "slack",
                "name": "ops-slack",
                "workspace_id": "T0001",
                "bot_token": format!("xoxb-{slug}"),
            })),
        )
        .await;
        assert_eq!(
            status,
            StatusCode::CREATED,
            "operator connection stores: {body}"
        );
        admins.push(credential);
    }
    Seeded { root, admins }
}

/// Activating a replacement wrapping key moves no stored secret: a restart
/// with `k2` active and `k1` retained leaves every stored envelope
/// byte-identical, the platform sign-in still opens its secret through the
/// retained key, and only a new write seals under `k2`.
///
/// # Panics
/// Panics when any envelope changes on activation or a new write does not
/// name the active key.
#[tokio::test]
async fn activating_a_replacement_wrapping_key_rewrites_no_stored_secret() {
    let transit = TransitFixture::start().await;
    let srv = Box::pin(
        WyrdTestServer::builder()
            .with_kms_for_test(transit.config("k1", &[]))
            .start_bound(),
    )
    .await
    .expect("server starts");
    let provider = wiremock::MockServer::start().await;
    let seeded = seed(&srv, &provider, &["acme-a"]).await;
    let before = stored_envelopes(&srv).await;
    assert_eq!(before.len(), 3, "one secret per seeded store: {before:?}");
    assert!(key_refs(&before).iter().all(|key| *key == "k1"));

    let srv =
        Box::pin(srv.restart_bound(
            WyrdTestServer::builder().with_kms_for_test(transit.config("k2", &["k1"])),
        ))
        .await
        .expect("server restarts with the replacement key active");
    assert_eq!(
        stored_envelopes(&srv).await,
        before,
        "activation rewrote no stored envelope"
    );
    confidential_platform_sign_in(&srv, &provider).await;

    let (status, body) = call(
        &srv,
        &seeded.admin(&srv, 0).await,
        Method::PUT,
        "/v1/admin/gateway/provider-credentials/written-after",
        Some(json!({
            "name": "written-after",
            "provider": "acme",
            "source": { "managed_secret": { "secret": "sk-after" } },
        })),
    )
    .await;
    assert!(status.is_success(), "a new credential stores: {body}");
    let after = stored_envelopes(&srv).await;
    let written: Vec<&Stored> = after.iter().filter(|row| !before.contains(row)).collect();
    assert_eq!(written.len(), 1, "only the new write added an envelope");
    assert_eq!(
        envelope_parts(&written[0].2).0,
        "k2",
        "new writes seal under the active key"
    );
    srv.shutdown().await.expect("server shuts down");
}
