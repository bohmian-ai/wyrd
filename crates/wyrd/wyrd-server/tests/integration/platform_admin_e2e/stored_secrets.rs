//! Wrapping-key lifecycle journeys: activation, explicit maintenance passes,
//! recovery, interruption, denial, races, and containment, against a real
//! server, a Vault Transit fixture, and repository Postgres.

use axum::body::{Body, to_bytes};
use axum::http::{Method, Response, StatusCode};
use serde_json::{Value, json};
use wyrd_auth_oidc::IssuerConfigResolver as _;
use wyrd_testing::WyrdTestServer;
use wyrd_testing::kms::TransitFixture;

use super::{
    PLATFORM_CLIENT, PLATFORM_CLIENT_SECRET, anonymous_post, confidential_platform_sign_in,
    initialize_platform_root, mount_platform_provider, platform_key_set, platform_post,
    platform_request, platform_session, system_audit_decisions, tenant_request, tenant_token,
};

/// The maintenance route.
const MAINTENANCE: &str = "/platform/stored-secrets/maintenance";

/// Every stored envelope with its store and owner, in a stable order.
const STORED_ENVELOPES: &str = "SELECT store, owner, envelope FROM (
      SELECT 'human_connection' AS store,
             data_tenant_id::text || ' ' || connection_id::text AS owner,
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
      UNION ALL SELECT 'operator_connection',
             data_tenant_id::text || ' ' || connection_id::text, secret_ciphertext
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

/// The store and owner of each stored envelope.
fn owners(stored: &[Stored]) -> Vec<(&str, &str)> {
    stored
        .iter()
        .map(|(store, owner, _)| (store.as_str(), owner.as_str()))
        .collect()
}

/// The wrapping-key reference each stored envelope names.
fn key_refs(stored: &[Stored]) -> Vec<&str> {
    stored
        .iter()
        .map(|(_, _, envelope)| envelope_parts(envelope).0)
        .collect()
}

/// The payload tail (nonce and ciphertext) of each stored envelope.
fn payloads(stored: &[Stored]) -> Vec<&[u8]> {
    stored
        .iter()
        .map(|(_, _, envelope)| envelope_parts(envelope).1)
        .collect()
}

/// Decode a JSON body, or `null` for an empty one.
async fn json_body(resp: Response<Body>) -> Value {
    let bytes = to_bytes(resp.into_body(), 1 << 20)
        .await
        .expect("read body");
    if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).expect("decode body")
    }
}

/// Run one maintenance pass with `body` as the platform session `session`.
///
/// # Panics
/// Panics when the route does not respond.
async fn maintain(srv: &WyrdTestServer, session: &str, body: Value) -> (StatusCode, Value) {
    let resp = srv
        .oneshot(platform_post(MAINTENANCE, session, body))
        .await
        .expect("maintenance route responds");
    let status = resp.status();
    (status, json_body(resp).await)
}

/// One seeded tenant: its public host and administrator credential.
struct Tenant {
    /// The tenant slug; its public host is `<slug>.wyrd.test`.
    slug: String,
    /// The data tenant id, as text.
    id: String,
    /// The administrator credential; exchanged afresh for every call so it
    /// survives a restart.
    credential: String,
}

impl Tenant {
    /// A fresh administrator access token on `srv`.
    ///
    /// # Panics
    /// Panics when the credential no longer exchanges.
    async fn token(&self, srv: &WyrdTestServer) -> String {
        tenant_token(srv, &self.credential)
            .await
            .expect("the tenant administrator authenticates")
    }

    /// Send `method uri` with an optional JSON body as this tenant's
    /// administrator, reached at the tenant's public host.
    ///
    /// # Panics
    /// Panics when the route does not respond or the body is not JSON.
    async fn call(
        &self,
        srv: &WyrdTestServer,
        method: Method,
        uri: &str,
        body: Option<Value>,
    ) -> (StatusCode, Value) {
        let mut request = tenant_request(method, uri, body);
        request.headers_mut().insert(
            "x-forwarded-host",
            format!("{}.wyrd.test", self.slug)
                .parse()
                .expect("host header"),
        );
        let resp = srv
            .oneshot_authenticated(&self.token(srv).await, request)
            .await
            .expect("route responds");
        let status = resp.status();
        (status, json_body(resp).await)
    }

    /// Send `method uri` as [`Self::call`] does and require success.
    ///
    /// # Panics
    /// Panics when the route does not answer with a success status.
    async fn expect_ok(
        &self,
        srv: &WyrdTestServer,
        method: Method,
        uri: &str,
        body: Option<Value>,
    ) {
        let (status, body) = self.call(srv, method, uri, body).await;
        assert!(status.is_success(), "{uri} succeeds: {body}");
    }

    /// Replace this tenant's `managed` Gateway credential with `secret`.
    ///
    /// # Panics
    /// Panics when the replacement is refused.
    async fn replace_gateway_credential(&self, srv: &WyrdTestServer, secret: &str) {
        self.expect_ok(
            srv,
            Method::PUT,
            "/v1/admin/gateway/provider-credentials/managed",
            Some(json!({
                "name": "managed",
                "provider": "acme",
                "source": { "managed_secret": { "secret": secret } },
            })),
        )
        .await;
    }

    /// The id of this tenant's only Operator connection.
    ///
    /// # Panics
    /// Panics when the list is refused or holds no connection.
    async fn operator_connection(&self, srv: &WyrdTestServer) -> String {
        let (status, list) = self
            .call(srv, Method::GET, "/v1/operator-connections", None)
            .await;
        assert_eq!(status, StatusCode::OK, "{list}");
        list.as_array()
            .or_else(|| list["connections"].as_array())
            .and_then(|views| views.first())
            .and_then(|view| view["connection_id"].as_str())
            .expect("an Operator connection")
            .to_owned()
    }
}

/// A deployment with a secret in every store.
struct Seeded {
    /// The deployment root credential; exchange it again after a restart.
    root: secrecy::SecretString,
    /// Each seeded tenant.
    tenants: Vec<Tenant>,
    /// Each tenant's staged human connection id, in tenant order.
    human_connections: Vec<String>,
}

impl Seeded {
    /// A fresh platform session on `srv`.
    async fn session(&self, srv: &WyrdTestServer) -> String {
        platform_session(srv, secrecy::ExposeSecret::expose_secret(&self.root)).await
    }

    /// Prove every seeded owner opens its stored secret on `srv` through its
    /// own consuming route: the platform sign-in sends its client secret, and
    /// each tenant's human connection, trusted issuer, Gateway credential, and
    /// Operator connection reseal their unchanged value.
    ///
    /// # Panics
    /// Panics when any owner cannot open its secret.
    async fn every_owner_opens(&self, srv: &WyrdTestServer, provider: &wiremock::MockServer) {
        confidential_platform_sign_in(srv, provider).await;
        let issuer: String =
            url::form_urlencoded::byte_serialize(provider.uri().as_bytes()).collect();
        for (tenant, human) in self.tenants.iter().zip(&self.human_connections) {
            let operator = tenant.operator_connection(srv).await;
            for uri in [
                format!("/v1/identity/oidc/connections/{human}/renew-encryption"),
                format!("/v1/admin/trusted-issuers/renew-encryption?issuer={issuer}"),
                "/v1/admin/gateway/provider-credentials/managed/renew-encryption".to_owned(),
                format!("/v1/operator-connections/{operator}/renew-encryption"),
            ] {
                let (status, body) = tenant.call(srv, Method::POST, &uri, None).await;
                assert_eq!(status, StatusCode::OK, "{uri} opens its secret: {body}");
            }
        }
    }
}

/// Initialize the deployment, configure confidential platform sign-in
/// against `provider`, and give each of `slugs` a workload trusted issuer and
/// a staged human connection on `provider`, a Gateway managed credential, and
/// an Operator connection: a secret in every store.
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
    for (uri, method, body) in [
        (
            "/platform/oidc/connection",
            Method::PUT,
            json!({
                "issuer_url": provider.uri(),
                "client_id": PLATFORM_CLIENT,
                "client_auth": { "method": "secret_post", "secret": PLATFORM_CLIENT_SECRET },
            }),
        ),
        (
            "/platform/admins",
            Method::POST,
            json!({ "name": "ops-lead", "match_claim": "ops@example.com" }),
        ),
    ] {
        let resp = srv
            .oneshot(platform_request(method, uri, &session, Some(body)))
            .await
            .expect("platform route responds");
        assert_eq!(resp.status(), StatusCode::OK, "{uri} succeeds");
    }

    let mut tenants = Vec::new();
    let mut human_connections = Vec::new();
    for slug in slugs {
        let resp = srv
            .oneshot(platform_post(
                "/platform/tenants",
                &session,
                json!({ "slug": slug, "display_name": slug }),
            ))
            .await
            .expect("tenant route responds");
        let created = json_body(resp).await;
        let tenant = Tenant {
            slug: (*slug).to_owned(),
            id: created["tenant"]["id"]
                .as_str()
                .expect("tenant id")
                .to_owned(),
            credential: created["admin"]["credential"]
                .as_str()
                .expect("admin credential")
                .to_owned(),
        };
        for (method, uri, body) in [
            (
                Method::PUT,
                "/v1/admin/gateway/provider-credentials/managed",
                json!({
                    "name": "managed",
                    "provider": "acme",
                    "source": { "managed_secret": { "secret": format!("sk-{slug}") } },
                }),
            ),
            (
                Method::POST,
                "/v1/operator-connections",
                json!({
                    "provider": "slack",
                    "name": "ops-slack",
                    "workspace_id": "T0001",
                    "bot_token": format!("xoxb-{slug}"),
                }),
            ),
            (
                Method::POST,
                "/v1/admin/trusted-issuers",
                json!({
                    "issuer": provider.uri(),
                    "expected_audience": "wyrd",
                    "client_id": "workload-client",
                    "client_auth": "SecretPost",
                    "client_secret": format!("issuer-{slug}"),
                    "claim_mapping": { "subject": "sub" },
                    "principal_kind": "workload",
                }),
            ),
        ] {
            let (status, body) = tenant.call(srv, method, uri, Some(body)).await;
            assert!(status.is_success(), "{uri} stores: {body}");
        }
        let (status, staged) = tenant
            .call(
                srv,
                Method::PUT,
                "/v1/identity/oidc/candidate",
                Some(json!({
                    "issuer": provider.uri(),
                    "client_id": "human-client",
                    "client_auth": "SecretPost",
                    "client_secret": format!("human-{slug}"),
                    "claim_mapping": { "subject": "sub", "email": "email", "groups": "groups" },
                    "group_role_map": {},
                })),
            )
            .await;
        assert!(status.is_success(), "human connection stages: {staged}");
        human_connections.push(
            staged["connection_id"]
                .as_str()
                .or_else(|| staged["id"].as_str())
                .expect("staged connection id")
                .to_owned(),
        );
        tenants.push(tenant);
    }
    Seeded {
        root,
        tenants,
        human_connections,
    }
}

/// Start a server sealing under Transit key `active`, keeping `retained`
/// readable.
///
/// # Panics
/// Panics when the server cannot start.
async fn start(transit: &TransitFixture, active: &str, retained: &[&str]) -> WyrdTestServer {
    Box::pin(
        WyrdTestServer::builder()
            .with_kms_for_test(transit.config(active, retained))
            .start_bound(),
    )
    .await
    .expect("server starts")
}

/// Restart `srv` sealing under Transit key `active`, keeping `retained`
/// readable, as a rolled-out writer configuration.
///
/// # Panics
/// Panics when the server cannot restart.
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

/// Activating a replacement wrapping key moves no stored secret: a restart
/// with `k2` active and `k1` retained leaves every stored envelope in all five
/// stores byte-identical, the platform sign-in still opens its secret through
/// the retained key, and only a new write seals under `k2`.
///
/// # Panics
/// Panics when any envelope changes on activation or a new write does not
/// name the active key.
#[tokio::test]
async fn activating_a_replacement_wrapping_key_rewrites_no_stored_secret() {
    let transit = TransitFixture::start().await;
    let srv = start(&transit, "k1", &[]).await;
    let provider = wiremock::MockServer::start().await;
    let seeded = seed(&srv, &provider, &["acme-a"]).await;
    let before = stored_envelopes(&srv).await;
    assert_eq!(
        before.len(),
        5,
        "one secret per store: {:?}",
        owners(&before)
    );
    assert!(key_refs(&before).iter().all(|key| *key == "k1"));

    let srv = roll_out(srv, &transit, "k2", &["k1"]).await;
    let after = stored_envelopes(&srv).await;
    assert_eq!(
        key_refs(&after),
        key_refs(&before),
        "activation moved no envelope"
    );
    assert_eq!(after, before, "activation rewrote no stored envelope");
    confidential_platform_sign_in(&srv, &provider).await;

    let (status, body) = seeded.tenants[0]
        .call(
            &srv,
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

/// A platform administrator's routine pass moves every stored secret onto the
/// active key and keeps each value's data key and payload: the reference and
/// wrapped data key change, the nonce and payload ciphertext do not, and every
/// owner opens its secret with the old key gone. Tenant-only and anonymous
/// callers are refused without touching a row, the allowed platform decision
/// is audited, and the report carries counts only.
///
/// # Panics
/// Panics when a refusal mutates a row, the pass changes a payload, the
/// report differs, or an owner cannot open its secret afterwards.
#[tokio::test]
async fn a_routine_pass_rewraps_stored_secrets_and_keeps_their_payloads() {
    let transit = TransitFixture::start().await;
    let srv = start(&transit, "k1", &[]).await;
    let provider = wiremock::MockServer::start().await;
    let seeded = seed(&srv, &provider, &["acme-a"]).await;
    let srv = roll_out(srv, &transit, "k2", &["k1"]).await;
    let before = stored_envelopes(&srv).await;

    let routine = json!({ "mode": "routine" });
    let tenant = seeded.tenants[0].token(&srv).await;
    let resp = srv
        .oneshot(platform_post(MAINTENANCE, &tenant, routine.clone()))
        .await
        .expect("maintenance route responds");
    assert_eq!(
        resp.status(),
        StatusCode::UNAUTHORIZED,
        "a tenant token is refused"
    );
    let resp = srv
        .oneshot(anonymous_post(MAINTENANCE, routine.clone()))
        .await
        .expect("maintenance route responds");
    assert_eq!(
        resp.status(),
        StatusCode::UNAUTHORIZED,
        "an anonymous call is refused"
    );
    assert_eq!(
        stored_envelopes(&srv).await,
        before,
        "refusals moved nothing"
    );

    let session = seeded.session(&srv).await;
    let (status, report) = maintain(&srv, &session, routine).await;
    assert_eq!(status, StatusCode::OK, "{report}");
    assert_eq!(
        report,
        json!({
            "mode": "routine",
            "current": 0,
            "changed": 5,
            "remaining": 0,
            "bounded": false,
            "failures": [],
        })
    );
    let after = stored_envelopes(&srv).await;
    assert_eq!(
        owners(&after),
        owners(&before),
        "resource identities are kept"
    );
    assert!(
        key_refs(&after).iter().all(|key| *key == "k2"),
        "{:?}",
        key_refs(&after)
    );
    assert_eq!(
        payloads(&after),
        payloads(&before),
        "data key and payload are kept"
    );
    assert!(
        after
            .iter()
            .zip(&before)
            .all(|((_, _, moved), (_, _, old))| moved != old),
        "every wrapped data key changed"
    );

    let allowed: Vec<_> = system_audit_decisions(&srv)
        .await
        .into_iter()
        .filter(|row| row["resource"].as_deref() == Some("platform:stored_secrets"))
        .map(|row| (row["permission"].clone(), row["outcome"].clone()))
        .collect();
    assert_eq!(
        allowed,
        vec![(
            Some("platform_credentials:write".to_owned()),
            Some("allowed".to_owned())
        )],
        "the pass audits its one platform decision"
    );

    let srv = roll_out(srv, &transit, "k2", &[]).await;
    seeded.every_owner_opens(&srv, &provider).await;
    srv.shutdown().await.expect("server shuts down");
}

/// Bounded routine passes cover every owner in all five stores across two
/// tenants and the platform: each pass changes at most its limit, reports the
/// rest as remaining and bounded, and repeating converges; an envelope copied
/// onto another tenant's owner is never moved and is reported by store,
/// tenant, owner, and reason with no secret material; and a final pass finds
/// everything else current.
///
/// # Panics
/// Panics when a count differs from the rows observed, an owner is skipped,
/// the copied envelope moves, or a report carries key or secret material.
#[tokio::test]
async fn bounded_routine_passes_cover_every_store_and_report_what_remains() {
    let transit = TransitFixture::start().await;
    let srv = start(&transit, "k1", &[]).await;
    let provider = wiremock::MockServer::start().await;
    let seeded = seed(&srv, &provider, &["acme-a", "acme-b"]).await;
    let srv = roll_out(srv, &transit, "k2", &["k1"]).await;
    let session = seeded.session(&srv).await;

    // Copy tenant A's Gateway envelope onto tenant B's credential: its context
    // names A, so it must never open, or move, as B's.
    let pool = srv.operator_pool().pool().clone();
    let tenants: Vec<String> = sqlx::query_scalar(
        "SELECT data_tenant_id::text FROM wyrd.gateway_provider_credentials
          WHERE name = 'managed' ORDER BY data_tenant_id",
    )
    .fetch_all(&pool)
    .await
    .expect("tenants read");
    let (source, tampered_tenant) = (&tenants[0], &tenants[1]);
    let copied: Vec<u8> = sqlx::query_scalar(
        "UPDATE wyrd.gateway_provider_credentials
            SET secret_ciphertext = (SELECT secret_ciphertext
                                       FROM wyrd.gateway_provider_credentials
                                      WHERE data_tenant_id = $1::uuid AND name = 'managed')
          WHERE data_tenant_id = $2::uuid AND name = 'managed'
         RETURNING secret_ciphertext",
    )
    .bind(source)
    .bind(tampered_tenant)
    .fetch_one(&pool)
    .await
    .expect("envelope copies");
    let total = stored_envelopes(&srv).await.len();
    assert_eq!(total, 9, "four tenant stores per tenant and the platform");

    let limited = json!({ "mode": "routine", "limit": 3 });
    let mut changed = 0;
    let mut passes = 0;
    let last = loop {
        let (status, report) = maintain(&srv, &session, limited.clone()).await;
        assert_eq!(status, StatusCode::OK, "{report}");
        let stored = stored_envelopes(&srv).await;
        let on_k2 = key_refs(&stored).iter().filter(|key| **key == "k2").count();
        let pass_changed = report["changed"].as_u64().expect("changed");
        assert!(
            pass_changed <= 3,
            "a pass changes at most its limit: {report}"
        );
        changed += pass_changed;
        passes += 1;
        assert_eq!(
            usize::try_from(changed).expect("fits"),
            on_k2,
            "changed counts the rows that moved"
        );
        assert_eq!(
            report["current"].as_u64().expect("current")
                + pass_changed
                + report["remaining"].as_u64().expect("remaining"),
            u64::try_from(total).expect("fits"),
            "every observed envelope is counted once: {report}"
        );
        if report["bounded"] == false {
            break report;
        }
        assert!(passes < 10, "passes never converged");
    };
    assert!(passes >= 3, "the limit bounded earlier passes");
    assert_eq!(changed, 8, "every openable envelope moved");
    // A remote backend refuses the mismatched context like any refusal.
    assert_eq!(
        last["failures"],
        json!([{
            "store": "gateway_credential",
            "data_tenant_id": tampered_tenant,
            "owner": "managed",
            "reason": "key_unavailable",
        }])
    );
    assert_eq!(last["remaining"], 1, "only the copied envelope remains");
    let text = last.to_string();
    for secret in ["k1", "k2", "sk-acme", "xoxb-", "issuer-acme", "human-acme"] {
        assert!(!text.contains(secret), "the report names {secret}: {text}");
    }
    let stored = stored_envelopes(&srv).await;
    assert!(
        stored.iter().any(|(_, _, envelope)| *envelope == copied),
        "the copied envelope was not moved"
    );

    let (_, again) = maintain(&srv, &session, json!({ "mode": "routine" })).await;
    assert_eq!(
        (&again["current"], &again["changed"], &again["remaining"]),
        (&json!(8), &json!(0), &json!(1)),
        "{again}"
    );
    srv.shutdown().await.expect("server shuts down");
}

/// Recovery reseals every stored secret under a fresh data key, including
/// secrets a routine pass already moved before the incident was found: after
/// routine rotation onto `k2` keeps every payload, activating a fresh `k3`
/// and running recovery changes every payload, names `k3`, and every owner
/// opens its value with only `k3` configured.
///
/// # Panics
/// Panics when routine changes a payload, recovery keeps one, an envelope is
/// skipped, or an owner cannot open its secret afterwards.
#[tokio::test]
async fn recovery_renews_every_data_key_even_after_a_routine_rotation() {
    let transit = TransitFixture::start().await;
    let srv = start(&transit, "k1", &[]).await;
    let provider = wiremock::MockServer::start().await;
    let seeded = seed(&srv, &provider, &["acme-a"]).await;
    let sealed = stored_envelopes(&srv).await;

    let srv = roll_out(srv, &transit, "k2", &["k1"]).await;
    let (status, routine) = maintain(
        &srv,
        &seeded.session(&srv).await,
        json!({ "mode": "routine" }),
    )
    .await;
    assert_eq!(
        (status, &routine["changed"]),
        (StatusCode::OK, &json!(5)),
        "{routine}"
    );
    let rotated = stored_envelopes(&srv).await;
    assert_eq!(
        payloads(&rotated),
        payloads(&sealed),
        "routine keeps every payload"
    );

    // The incident is found after the routine rotation: activate a fresh key.
    let srv = roll_out(srv, &transit, "k3", &["k2"]).await;
    let (status, recovery) = maintain(
        &srv,
        &seeded.session(&srv).await,
        json!({ "mode": "recovery" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{recovery}");
    assert_eq!(
        (
            &recovery["mode"],
            &recovery["changed"],
            &recovery["remaining"]
        ),
        (&json!("recovery"), &json!(5), &json!(0)),
        "{recovery}"
    );
    let renewed = stored_envelopes(&srv).await;
    assert_eq!(
        owners(&renewed),
        owners(&rotated),
        "resource identities are kept"
    );
    assert!(key_refs(&renewed).iter().all(|key| *key == "k3"));
    for (fresh, old) in payloads(&renewed).iter().zip(payloads(&rotated)) {
        assert_ne!(*fresh, old, "recovery reseals every payload");
    }

    let srv = roll_out(srv, &transit, "k3", &[]).await;
    seeded.every_owner_opens(&srv, &provider).await;
    srv.shutdown().await.expect("server shuts down");
}

/// An interrupted recovery resumes safely and a denied key leaves state
/// intact: a bounded recovery commits some renewals; after a restart the same
/// mode leaves those byte-identical, counts them current, and renews the rest;
/// with the wrapping backend refusing, a routine pass reports each owner as
/// `key_unavailable`, moves nothing, and a later pass converges.
///
/// # Panics
/// Panics when a resumed pass re-renews a finished envelope, a denied pass
/// mutates a row or reports completion, or the final pass does not converge.
#[tokio::test]
async fn interrupted_or_denied_passes_resume_without_losing_committed_work() {
    let transit = TransitFixture::start().await;
    let srv = start(&transit, "k1", &[]).await;
    let provider = wiremock::MockServer::start().await;
    let seeded = seed(&srv, &provider, &["acme-a"]).await;

    let srv = roll_out(srv, &transit, "k2", &["k1"]).await;
    let (_, partial) = maintain(
        &srv,
        &seeded.session(&srv).await,
        json!({ "mode": "recovery", "limit": 2 }),
    )
    .await;
    assert_eq!(
        (
            &partial["changed"],
            &partial["remaining"],
            &partial["bounded"]
        ),
        (&json!(2), &json!(3), &json!(true)),
        "{partial}"
    );
    let interrupted = stored_envelopes(&srv).await;
    let finished: Vec<&Stored> = interrupted
        .iter()
        .filter(|(_, _, envelope)| envelope_parts(envelope).0 == "k2")
        .collect();
    assert_eq!(finished.len(), 2);

    let srv = roll_out(srv, &transit, "k2", &["k1"]).await;
    let (_, resumed) = maintain(
        &srv,
        &seeded.session(&srv).await,
        json!({ "mode": "recovery" }),
    )
    .await;
    assert_eq!(
        (
            &resumed["current"],
            &resumed["changed"],
            &resumed["remaining"]
        ),
        (&json!(2), &json!(3), &json!(0)),
        "{resumed}"
    );
    let after = stored_envelopes(&srv).await;
    for row in finished {
        assert!(
            after.contains(row),
            "a finished renewal is never redone: {}",
            row.1
        );
    }

    let srv = roll_out(srv, &transit, "k3", &["k2"]).await;
    let session = seeded.session(&srv).await;
    let before = stored_envelopes(&srv).await;
    transit.set_available(false);
    let (status, denied) = maintain(&srv, &session, json!({ "mode": "routine" })).await;
    assert_eq!(status, StatusCode::OK, "{denied}");
    assert_eq!(
        (&denied["changed"], &denied["remaining"]),
        (&json!(0), &json!(5)),
        "{denied}"
    );
    let reasons: Vec<&Value> = denied["failures"]
        .as_array()
        .expect("failures")
        .iter()
        .map(|failure| &failure["reason"])
        .collect();
    assert_eq!(reasons, vec![&json!("key_unavailable"); 5], "{denied}");
    assert_eq!(
        stored_envelopes(&srv).await,
        before,
        "a denied pass moved nothing"
    );

    transit.set_available(true);
    let (_, recovered) = maintain(&srv, &session, json!({ "mode": "routine" })).await;
    assert_eq!(
        (&recovered["changed"], &recovered["remaining"]),
        (&json!(5), &json!(0)),
        "{recovered}"
    );
    srv.shutdown().await.expect("server shuts down");
}

/// Stored envelopes the pass wraps before the Gateway store: two human
/// connections, two trusted issuers, and the platform connection.
const GATEWAY_STORE: usize = 5;

/// Stored envelopes the pass wraps before the Operator store: the above and
/// two Gateway credentials.
const OPERATOR_STORE: usize = 7;

/// Waits until a maintenance pass is wrapping a given row.
///
/// A pass reads one store, then wraps its rows in order, sending one
/// `decrypt` per row; on a delayed Transit fixture the row stays in flight
/// for the delay, so a write made then lands after the read and before the
/// swap.
struct WrapBarrier<'a> {
    /// The delayed fixture whose decrypt requests mark wrapping progress.
    transit: &'a TransitFixture,
    /// Decrypt requests served before the pass started.
    before: usize,
}

impl<'a> WrapBarrier<'a> {
    /// Count from `transit`'s current decrypt requests.
    async fn new(transit: &'a TransitFixture) -> Self {
        let before = transit.requests("decrypt").await;
        Self { transit, before }
    }

    /// Return once the pass has begun wrapping the first row after `earlier`
    /// envelopes.
    async fn first_row_of(&self, earlier: usize) {
        while self.transit.requests("decrypt").await <= self.before + earlier {
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    }
}

/// `tenants` in the order a pass wraps their rows within each store: by
/// tenant id.
///
/// # Panics
/// Panics unless there are exactly two tenants.
fn in_wrap_order(tenants: &[Tenant]) -> (&Tenant, &Tenant) {
    let [left, right] = tenants else {
        panic!("two tenants")
    };
    if left.id < right.id {
        (left, right)
    } else {
        (right, left)
    }
}

/// Concurrent owner writes win over a slow pass: while every wrapping call is
/// delayed, tenant A replaces its Gateway credential and rotates its Operator
/// token, and tenant B revokes its credential and disables its connection.
/// Tenant A is whichever tenant's rows the pass wraps second in each store.
/// The pass loses its fence on the two replaced envelopes and reports them as
/// `concurrent_change`, leaving the winning bytes; it moves the revoked and
/// disabled owners' envelopes without re-enabling either; and the next pass
/// finds everything current.
///
/// # Panics
/// Panics when a stale write overwrites a winning envelope, a revoked or
/// disabled owner becomes usable, or the counts differ from the rows.
#[tokio::test]
async fn a_pass_never_overwrites_or_reenables_a_concurrently_changed_owner() {
    let transit = TransitFixture::start_delayed(std::time::Duration::from_millis(400)).await;
    let srv = start(&transit, "k1", &[]).await;
    let provider = wiremock::MockServer::start().await;
    let seeded = seed(&srv, &provider, &["acme-a", "acme-b"]).await;
    let srv = roll_out(srv, &transit, "k2", &["k1"]).await;
    let session = seeded.session(&srv).await;
    let (first, second) = in_wrap_order(&seeded.tenants);
    let (first_operator, second_operator) = (
        first.operator_connection(&srv).await,
        second.operator_connection(&srv).await,
    );
    let wrapping = WrapBarrier::new(&transit).await;

    // While the pass wraps the first tenant's row in a store, the second
    // tenant replaces its secret there (committing before the pass reaches
    // it) and the first tenant changes its own row's lifecycle.
    let racers = async {
        wrapping.first_row_of(GATEWAY_STORE).await;
        second.replace_gateway_credential(&srv, "sk-replaced").await;
        first
            .expect_ok(
                &srv,
                Method::POST,
                "/v1/admin/gateway/provider-credentials/managed/revoke",
                None,
            )
            .await;

        wrapping.first_row_of(OPERATOR_STORE).await;
        second
            .expect_ok(
                &srv,
                Method::PATCH,
                &format!("/v1/operator-connections/{second_operator}"),
                Some(json!({ "provider": "slack", "bot_token": "xoxb-rotated" })),
            )
            .await;
        first
            .expect_ok(
                &srv,
                Method::DELETE,
                &format!("/v1/operator-connections/{first_operator}"),
                None,
            )
            .await;
        stored_envelopes(&srv).await
    };
    let ((status, report), winners) = tokio::join!(
        maintain(&srv, &session, json!({ "mode": "routine" })),
        racers
    );
    assert_eq!(status, StatusCode::OK, "{report}");
    assert_eq!(
        (&report["changed"], &report["remaining"]),
        (&json!(7), &json!(2)),
        "{report}"
    );
    let mut lost: Vec<(&str, &str)> = report["failures"]
        .as_array()
        .expect("failures")
        .iter()
        .map(|failure| {
            assert_eq!(failure["reason"], "concurrent_change", "{failure}");
            (
                failure["store"].as_str().expect("store"),
                failure["owner"].as_str().expect("owner"),
            )
        })
        .collect();
    lost.sort_unstable();
    assert_eq!(
        lost,
        vec![
            ("gateway_credential", "managed"),
            ("operator_connection", second_operator.as_str())
        ]
    );

    // Nothing the pass did after the races overwrote an envelope already on
    // the active key: the replacements and any row it moved first stand.
    let after = stored_envelopes(&srv).await;
    for row in winners
        .iter()
        .filter(|row| envelope_parts(&row.2).0 == "k2")
    {
        assert!(
            after.contains(row),
            "a winning envelope stands: {} {}",
            row.0,
            row.1
        );
    }
    let (credential_state, connection_status): (String, String) = sqlx::query_as(
        "SELECT (SELECT state FROM wyrd.gateway_provider_credentials
                  WHERE data_tenant_id = $1::uuid AND name = 'managed'),
                (SELECT status FROM wyrd.operator_connections WHERE connection_id = $2::uuid)",
    )
    .bind(&first.id)
    .bind(&first_operator)
    .fetch_one(srv.operator_pool().pool())
    .await
    .expect("lifecycle reads");
    assert_eq!(
        (credential_state.as_str(), connection_status.as_str()),
        ("revoked", "disabled"),
        "the pass re-enabled nothing"
    );
    assert!(
        key_refs(&after).iter().all(|key| *key == "k2"),
        "{:?}",
        key_refs(&after)
    );

    let (_, again) = maintain(&srv, &session, json!({ "mode": "routine" })).await;
    assert_eq!(
        (&again["current"], &again["changed"], &again["remaining"]),
        (&json!(9), &json!(0), &json!(0)),
        "{again}"
    );
    second
        .expect_ok(
            &srv,
            Method::POST,
            "/v1/admin/gateway/provider-credentials/managed/renew-encryption",
            None,
        )
        .await;
    let (status, _) = first
        .call(
            &srv,
            Method::POST,
            "/v1/admin/gateway/provider-credentials/managed/renew-encryption",
            None,
        )
        .await;
    assert!(!status.is_success(), "the revoked credential stays refused");
    srv.shutdown().await.expect("server shuts down");
}

/// Removing a wrapping key from trusted configuration contains it and adding
/// it back restores access: with `k1` no longer configured, every owner
/// refuses to open its `k1` envelope with no backend call and no fallback, a
/// pass reports each as `key_not_allowed` and moves nothing; configuring `k1`
/// as retained again lets every owner open its value and a pass converge.
///
/// # Panics
/// Panics when a removed key still opens, a backend decrypt is attempted for
/// it, a refusal mutates a row, or restoring the key does not restore use.
#[tokio::test]
async fn removing_a_key_refuses_its_envelopes_until_it_is_restored() {
    let transit = TransitFixture::start().await;
    let srv = start(&transit, "k1", &[]).await;
    let provider = wiremock::MockServer::start().await;
    let seeded = seed(&srv, &provider, &["acme-a"]).await;
    let sealed = stored_envelopes(&srv).await;

    let srv = roll_out(srv, &transit, "k2", &[]).await;
    let decrypts = transit.requests("decrypt").await;
    let tenant = &seeded.tenants[0];
    let human = &seeded.human_connections[0];
    for uri in [
        format!("/v1/identity/oidc/connections/{human}/renew-encryption"),
        "/v1/admin/gateway/provider-credentials/managed/renew-encryption".to_owned(),
    ] {
        let (status, body) = tenant.call(&srv, Method::POST, &uri, None).await;
        assert!(!status.is_success(), "{uri} refuses a removed key: {body}");
    }
    let (status, contained) = maintain(
        &srv,
        &seeded.session(&srv).await,
        json!({ "mode": "recovery" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{contained}");
    assert_eq!(
        (&contained["changed"], &contained["remaining"]),
        (&json!(0), &json!(5)),
        "{contained}"
    );
    assert!(
        contained["failures"]
            .as_array()
            .expect("failures")
            .iter()
            .all(|failure| failure["reason"] == "key_not_allowed"),
        "{contained}"
    );
    assert_eq!(
        transit.requests("decrypt").await,
        decrypts,
        "no fallback decrypt"
    );
    assert_eq!(
        stored_envelopes(&srv).await,
        sealed,
        "containment moved nothing"
    );

    let srv = roll_out(srv, &transit, "k2", &["k1"]).await;
    seeded.every_owner_opens(&srv, &provider).await;
    let (_, restored) = maintain(
        &srv,
        &seeded.session(&srv).await,
        json!({ "mode": "routine" }),
    )
    .await;
    // Each tenant owner's renewal already resealed it; the pass moves the rest.
    assert_eq!(
        (
            &restored["current"],
            &restored["changed"],
            &restored["remaining"]
        ),
        (&json!(4), &json!(1), &json!(0)),
        "{restored}"
    );
    srv.shutdown().await.expect("server shuts down");
}

/// The edge request timeout the contention journeys run under; a pass's own
/// budget is half of it.
const EDGE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(4);

/// Restart `srv` sealing under `k2` with `k1` retained and an
/// [`EDGE_TIMEOUT`] request timeout.
///
/// # Panics
/// Panics when the server cannot restart.
async fn roll_out_with_edge_timeout(
    srv: WyrdTestServer,
    transit: &TransitFixture,
) -> WyrdTestServer {
    Box::pin(
        srv.restart_bound(
            WyrdTestServer::builder()
                .with_kms_for_test(transit.config("k2", &["k1"]))
                .with_limits_for_test(wyrd_server::state::LimitsConfig {
                    timeout: EDGE_TIMEOUT,
                    ..wyrd_server::state::LimitsConfig::default()
                }),
        ),
    )
    .await
    .expect("server restarts")
}

/// The envelopes of one store.
fn of_store<'a>(stored: &'a [Stored], store: &str) -> Vec<&'a Stored> {
    stored.iter().filter(|(name, _, _)| name == store).collect()
}

/// A pass whose discovery read waits on a locked store answers within its own
/// budget: it moves the stores it read before the locked one, reports itself
/// bounded without counting the stores it never read, and leaves those rows
/// untouched. Once the lock is released, the same mode finishes the rest.
///
/// # Panics
/// Panics when the pass outlives its budget, mutates an unread row, or the
/// repeat does not finish.
#[tokio::test]
async fn a_pass_stalled_on_a_locked_store_answers_within_its_budget() {
    let transit = TransitFixture::start().await;
    let srv = start(&transit, "k1", &[]).await;
    let provider = wiremock::MockServer::start().await;
    let seeded = seed(&srv, &provider, &["stall-read"]).await;
    let srv = roll_out_with_edge_timeout(srv, &transit).await;
    let session = seeded.session(&srv).await;
    let before = stored_envelopes(&srv).await;

    // An administrator's open transaction holds the Gateway store.
    let mut lock = srv
        .operator_pool()
        .pool()
        .begin()
        .await
        .expect("lock begins");
    sqlx::query("LOCK TABLE wyrd.gateway_provider_credentials IN ACCESS EXCLUSIVE MODE")
        .execute(&mut *lock)
        .await
        .expect("store locks");
    let started = std::time::Instant::now();
    let (status, stalled) = maintain(&srv, &session, json!({ "mode": "routine" })).await;
    let elapsed = started.elapsed();
    lock.rollback().await.expect("lock releases");

    assert_eq!(status, StatusCode::OK, "{stalled}");
    assert!(elapsed < EDGE_TIMEOUT, "the pass took {elapsed:?}");
    // Human connection, trusted issuer, and platform connection moved; the
    // Gateway and Operator stores were never read, so they are not counted.
    assert_eq!(
        (
            &stalled["changed"],
            &stalled["remaining"],
            &stalled["bounded"]
        ),
        (&json!(3), &json!(0), &json!(true)),
        "{stalled}"
    );
    let after = stored_envelopes(&srv).await;
    for store in ["gateway_credential", "operator_connection"] {
        assert_eq!(
            of_store(&after, store),
            of_store(&before, store),
            "{store} was not read, so it is unchanged"
        );
    }

    let (_, resumed) = maintain(&srv, &session, json!({ "mode": "routine" })).await;
    assert_eq!(
        (
            &resumed["current"],
            &resumed["changed"],
            &resumed["remaining"],
            &resumed["bounded"]
        ),
        (&json!(3), &json!(2), &json!(0), &json!(false)),
        "{resumed}"
    );
    srv.shutdown().await.expect("server shuts down");
}

/// A swap that waits on a locked row is abandoned at the pass budget without
/// losing earlier work: the three envelopes moved before it stay moved, the
/// locked Gateway credential keeps its bytes and counts as `remaining`, and
/// the abandoned update never lands later. The same mode then moves exactly
/// the two rows left and leaves the finished three byte-identical.
///
/// # Panics
/// Panics when the pass outlives its budget, the abandoned swap applies, or a
/// finished envelope is moved again.
#[tokio::test]
async fn a_swap_stalled_on_a_locked_row_is_abandoned_and_resumed() {
    let transit = TransitFixture::start().await;
    let srv = start(&transit, "k1", &[]).await;
    let provider = wiremock::MockServer::start().await;
    let seeded = seed(&srv, &provider, &["stall-swap"]).await;
    let srv = roll_out_with_edge_timeout(srv, &transit).await;
    let session = seeded.session(&srv).await;
    let before = stored_envelopes(&srv).await;

    // An open transaction holds the Gateway credential row, which a plain
    // read still sees, so the pass wraps it and then waits to swap it.
    let mut lock = srv
        .operator_pool()
        .pool()
        .begin()
        .await
        .expect("lock begins");
    sqlx::query("SELECT 1 FROM wyrd.gateway_provider_credentials FOR UPDATE")
        .execute(&mut *lock)
        .await
        .expect("row locks");
    let started = std::time::Instant::now();
    let (status, stalled) = maintain(&srv, &session, json!({ "mode": "routine" })).await;
    let elapsed = started.elapsed();
    lock.rollback().await.expect("lock releases");

    assert_eq!(status, StatusCode::OK, "{stalled}");
    assert!(elapsed < EDGE_TIMEOUT, "the pass took {elapsed:?}");
    assert_eq!(
        (
            &stalled["changed"],
            &stalled["remaining"],
            &stalled["bounded"]
        ),
        (&json!(3), &json!(1), &json!(true)),
        "{stalled}"
    );
    let interrupted = stored_envelopes(&srv).await;
    assert_eq!(
        of_store(&interrupted, "gateway_credential"),
        of_store(&before, "gateway_credential"),
        "the abandoned swap left the credential's bytes"
    );
    let finished: Vec<&Stored> = interrupted
        .iter()
        .filter(|(_, _, envelope)| envelope_parts(envelope).0 == "k2")
        .collect();
    assert_eq!(finished.len(), 3);

    // Had the abandoned update landed after the lock released, the Gateway
    // credential would already be current here.
    let (_, resumed) = maintain(&srv, &session, json!({ "mode": "routine" })).await;
    assert_eq!(
        (
            &resumed["current"],
            &resumed["changed"],
            &resumed["remaining"],
            &resumed["bounded"]
        ),
        (&json!(3), &json!(2), &json!(0), &json!(false)),
        "{resumed}"
    );
    let after = stored_envelopes(&srv).await;
    for row in finished {
        assert!(
            after.contains(row),
            "a finished envelope is never redone: {}",
            row.1
        );
    }
    srv.shutdown().await.expect("server shuts down");
}

/// Check out every connection of the serving process's operator pool, as
/// concurrent platform work would, so a pass can only wait for one.
///
/// # Panics
/// Panics when the pool's connections cannot all be acquired.
async fn hold_every_connection(
    srv: &WyrdTestServer,
) -> Vec<sqlx::pool::PoolConnection<sqlx::Postgres>> {
    let operator = srv
        .state()
        .postgres
        .operator_pool()
        .expect("serving operator pool");
    let pool = operator.pool();
    let mut held = Vec::new();
    // Background workers share the pool, so an acquire can time out while
    // one of them holds a connection; it frees it within a few attempts.
    for _ in 0..5 {
        while held.len() < pool.options().get_max_connections() as usize {
            match pool.acquire().await {
                Ok(connection) => held.push(connection),
                Err(sqlx::Error::PoolTimedOut) => break,
                Err(error) => panic!("connection acquires: {error}"),
            }
        }
    }
    assert_eq!(
        held.len(),
        pool.options().get_max_connections() as usize,
        "every connection is held"
    );
    held
}

/// The budget of a pass driven directly against the serving pool.
const PASS_BUDGET: std::time::Duration = std::time::Duration::from_secs(2);

/// Run one routine pass over the serving process's operator pool with a
/// [`PASS_BUDGET`] budget, as an already-authorized request would.
///
/// # Panics
/// Panics when the pass fails, for example on a pool timeout.
async fn serving_pass(
    srv: &WyrdTestServer,
    limit: Option<u32>,
) -> wyrd_spec::security::stored_secret::StoredSecretMaintenanceReport {
    wyrd_server::components::platform::stored_secrets::StoredSecretMaintenance::new(
        srv.state()
            .postgres
            .operator_pool()
            .expect("serving operator pool"),
        srv.state().auth.kms.clone(),
    )
    .run(
        wyrd_spec::security::stored_secret::StoredSecretMaintenanceMode::Routine,
        limit.and_then(std::num::NonZeroU32::new),
        PASS_BUDGET,
    )
    .await
    .expect("the pass answers")
}

/// The counts of `report`: current, changed, remaining, and bounded.
fn counts(
    report: &wyrd_spec::security::stored_secret::StoredSecretMaintenanceReport,
) -> (u64, u64, u64, bool) {
    (
        report.current,
        report.changed,
        report.remaining,
        report.bounded,
    )
}

/// An authorized pass that cannot get an operator connection answers within
/// its own budget, not the pool's longer acquisition timeout: it observes and
/// counts nothing, and the envelopes an earlier pass committed stand. Once
/// capacity returns, the same mode moves exactly the rest and leaves the
/// earlier ones byte-identical.
///
/// # Panics
/// Panics when the pass outlives its budget, counts or moves anything while
/// starved, or the repeat redoes finished work.
#[tokio::test]
async fn a_pass_starved_of_connections_answers_within_its_budget() {
    let transit = TransitFixture::start().await;
    let srv = start(&transit, "k1", &[]).await;
    let provider = wiremock::MockServer::start().await;
    seed(&srv, &provider, &["starved"]).await;
    let srv = roll_out(srv, &transit, "k2", &["k1"]).await;
    let first = serving_pass(&srv, Some(2)).await;
    assert_eq!(first.changed, 2, "{first:?}");
    let before = stored_envelopes(&srv).await;

    let held = hold_every_connection(&srv).await;
    let started = std::time::Instant::now();
    let starved = serving_pass(&srv, None).await;
    let elapsed = started.elapsed();
    drop(held);

    assert!(
        elapsed < PASS_BUDGET + PASS_BUDGET / 4,
        "the pass took {elapsed:?}"
    );
    assert_eq!(counts(&starved), (0, 0, 0, true), "{starved:?}");
    assert_eq!(
        stored_envelopes(&srv).await,
        before,
        "a starved pass moves nothing"
    );

    let resumed = serving_pass(&srv, None).await;
    assert_eq!(counts(&resumed), (2, 3, 0, false), "{resumed:?}");
    let after = stored_envelopes(&srv).await;
    for row in before
        .iter()
        .filter(|(_, _, envelope)| envelope_parts(envelope).0 == "k2")
    {
        assert!(
            after.contains(row),
            "a finished envelope is never redone: {}",
            row.1
        );
    }
    srv.shutdown().await.expect("server shuts down");
}

/// Time an authorized pass spends waiting for a connection is spent from its
/// budget, not granted again to the statement that follows: capacity returns
/// late in the budget, the first store's read then waits on a lock, and the
/// pass still answers at its own deadline with nothing observed. Once the
/// lock is released, the same mode moves every envelope.
///
/// # Panics
/// Panics when the pass outlives its budget, counts or moves anything, or
/// the repeat does not finish.
#[tokio::test]
async fn a_pass_that_waited_for_a_connection_keeps_its_deadline_on_a_lock() {
    let transit = TransitFixture::start().await;
    let srv = start(&transit, "k1", &[]).await;
    let provider = wiremock::MockServer::start().await;
    seed(&srv, &provider, &["late-capacity"]).await;
    let srv = roll_out(srv, &transit, "k2", &["k1"]).await;
    let before = stored_envelopes(&srv).await;

    // One held connection locks the first store the pass reads; the others
    // stay busy until three quarters of the budget have passed.
    let mut held = hold_every_connection(&srv).await;
    let mut lock = held.pop().expect("a connection to lock with");
    sqlx::query("BEGIN")
        .execute(&mut *lock)
        .await
        .expect("lock begins");
    sqlx::query("LOCK TABLE wyrd.auth_human_connections IN ACCESS EXCLUSIVE MODE")
        .execute(&mut *lock)
        .await
        .expect("store locks");
    let started = tokio::time::Instant::now();
    // The scenario's release point, not a synchronization wait.
    let release = async {
        tokio::time::sleep_until(started + PASS_BUDGET * 3 / 4).await;
        drop(held);
    };
    let (stalled, ()) = tokio::join!(serving_pass(&srv, None), release);
    let elapsed = started.elapsed();
    sqlx::query("ROLLBACK")
        .execute(&mut *lock)
        .await
        .expect("lock releases");
    drop(lock);

    assert!(
        elapsed < PASS_BUDGET + PASS_BUDGET / 4,
        "the pass took {elapsed:?} of a {PASS_BUDGET:?} budget"
    );
    assert_eq!(counts(&stalled), (0, 0, 0, true), "{stalled:?}");
    assert_eq!(
        stored_envelopes(&srv).await,
        before,
        "a cut read moves nothing"
    );

    let resumed = serving_pass(&srv, None).await;
    assert_eq!(counts(&resumed), (0, 5, 0, false), "{resumed:?}");
    srv.shutdown().await.expect("server shuts down");
}

/// A confidential `[[trusted_issuers]]` entry for `issuer` holding `auth`.
fn configured_issuer(
    issuer: &str,
    auth: wyrd_server::config::ClientAuthEntry,
) -> wyrd_server::config::IssuerEntry {
    wyrd_server::config::IssuerEntry {
        issuer: issuer.to_owned(),
        client_id: "configured-client".to_owned(),
        expected_audience: "wyrd".to_owned(),
        client_auth: auth,
        claim_mapping: wyrd_server::config::ClaimMappingEntry::default(),
        group_role_map: std::collections::HashMap::new(),
        default_roles: Vec::new(),
        principal_kind: wyrd_spec::auth::IssuerTokenPolicy::Workload,
        jwks_ttl_secs: Some(300),
    }
}

/// `SecretPost` holding `secret`.
fn secret_post(secret: &str) -> wyrd_server::config::ClientAuthEntry {
    wyrd_server::config::ClientAuthEntry::SecretPost(secrecy::SecretString::from(secret.to_owned()))
}

/// Restart `srv` sealing under Transit key `active`, keeping `retained`
/// readable, with `issuer` as its only configured trusted issuer.
///
/// # Panics
/// Panics when the server cannot restart.
async fn reboot_with_issuer(
    srv: WyrdTestServer,
    transit: &TransitFixture,
    (active, retained): (&str, &[&str]),
    issuer: wyrd_server::config::IssuerEntry,
) -> WyrdTestServer {
    Box::pin(
        srv.restart_bound(
            WyrdTestServer::builder()
                .with_kms_for_test(transit.config(active, retained))
                .with_trusted_issuer_configs(vec![issuer]),
        ),
    )
    .await
    .expect("server restarts")
}

/// The configured issuer's stored envelope.
async fn configured_envelope(srv: &WyrdTestServer) -> Vec<u8> {
    let stored = stored_envelopes(srv).await;
    let [(_, _, envelope)] = of_store(&stored, "trusted_issuer")[..] else {
        panic!("one configured issuer: {stored:?}")
    };
    envelope.clone()
}

/// The client authentication the server resolves for the configured issuer,
/// opening its secret.
async fn resolved_auth(srv: &WyrdTestServer) -> String {
    let issuers = srv
        .state()
        .auth
        .trusted_issuer_resolver
        .as_ref()
        .expect("issuer resolver")
        .trusted_issuers(&srv.data_tenant_id())
        .await
        .expect("configured issuer opens");
    let [issuer] = &issuers[..] else {
        panic!("one configured issuer")
    };
    match &issuer.client_auth {
        wyrd_auth_oidc::ClientAuth::SecretPost(secret) => {
            format!("post {}", secrecy::ExposeSecret::expose_secret(secret))
        }
        wyrd_auth_oidc::ClientAuth::SecretBasic(secret) => {
            format!("basic {}", secrecy::ExposeSecret::expose_secret(secret))
        }
        other => panic!("a secret method, got {other:?}"),
    }
}

/// Run one unbounded pass through the maintenance owner.
async fn pass(
    srv: &WyrdTestServer,
) -> wyrd_spec::security::stored_secret::StoredSecretMaintenanceReport {
    wyrd_server::components::platform::stored_secrets::StoredSecretMaintenance::new(
        srv.operator_pool().clone(),
        srv.state().auth.kms.clone(),
    )
    .run(
        wyrd_spec::security::stored_secret::StoredSecretMaintenanceMode::Routine,
        None,
        std::time::Duration::from_secs(30),
    )
    .await
    .expect("the pass reads the catalog")
}

/// A configured confidential issuer survives key activation untouched: a
/// restart with `k2` active and the same `[[trusted_issuers]]` entry leaves
/// its envelope byte-identical and still openable, and only an explicit pass
/// moves it to `k2`. Changing the configured secret or its client
/// authentication method still takes effect at the next boot, sealed under
/// the active key.
///
/// # Panics
/// Panics when activation rewrites the unchanged envelope, the server cannot
/// open it, or a configuration change is not applied.
#[tokio::test]
async fn activation_keeps_an_unchanged_configured_issuer_secret() {
    let transit = TransitFixture::start().await;
    let provider = wiremock::MockServer::start().await;
    mount_platform_provider(
        &provider,
        platform_key_set("mock-1"),
        wiremock::ResponseTemplate::new(500),
    )
    .await;
    let configured = |auth| configured_issuer(&provider.uri(), auth);
    let srv = Box::pin(
        WyrdTestServer::builder()
            .with_kms_for_test(transit.config("k1", &[]))
            .with_trusted_issuer_configs(vec![configured(secret_post("configured-1"))])
            .start_bound(),
    )
    .await
    .expect("server starts");
    let sealed = configured_envelope(&srv).await;
    assert_eq!(envelope_parts(&sealed).0, "k1");

    let srv = reboot_with_issuer(
        srv,
        &transit,
        ("k2", &["k1"]),
        configured(secret_post("configured-1")),
    )
    .await;
    assert_eq!(
        configured_envelope(&srv).await,
        sealed,
        "activation rewrote the unchanged configured issuer"
    );
    assert_eq!(resolved_auth(&srv).await, "post configured-1");

    let report = pass(&srv).await;
    assert_eq!((report.changed, report.remaining), (1, 0), "{report:?}");
    let rotated = configured_envelope(&srv).await;
    assert_eq!(envelope_parts(&rotated).0, "k2");
    assert_eq!(envelope_parts(&rotated).1, envelope_parts(&sealed).1);

    // A changed secret, then a changed method, is a new configured credential.
    let srv = reboot_with_issuer(
        srv,
        &transit,
        ("k3", &["k2"]),
        configured(secret_post("configured-2")),
    )
    .await;
    let replaced = configured_envelope(&srv).await;
    assert_eq!(envelope_parts(&replaced).0, "k3");
    assert_eq!(resolved_auth(&srv).await, "post configured-2");
    let srv = reboot_with_issuer(
        srv,
        &transit,
        ("k3", &[]),
        configured(wyrd_server::config::ClientAuthEntry::SecretBasic(
            secrecy::SecretString::from("configured-2".to_owned()),
        )),
    )
    .await;
    assert_ne!(configured_envelope(&srv).await, replaced);
    assert_eq!(resolved_auth(&srv).await, "basic configured-2");
    srv.shutdown().await.expect("server shuts down");
}
